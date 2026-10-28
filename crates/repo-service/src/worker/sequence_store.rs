//! Persists a stopped multi-commit cherry-pick/revert (`git_core::pick::SequenceState`) to
//! `<gitdir>/browsitory/sequence.json`, so Continue/Abort survive an app restart. Written
//! atomically (temp file + `sync_all` + `rename`, like `undo.json`). A file that is unreadable,
//! malformed, of another version, or that doesn't describe commits of this repository is
//! ignored (logged once), never fatal: a corrupt file must not wedge the repository.
use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use git2::{Oid, Repository};
use git_core::pick::{OriginalRef, SequenceKind, SequenceState};
use serde::{Deserialize, Serialize};

/// 2 added `stoppedOnConflict` and `originalRef`. A version 1 file still loads: it counts as an
/// error stop (Continue then needs HEAD unmoved) with an unknown original ref (Abort never
/// rewinds), the cautious reading of what it doesn't say.
const FORMAT_VERSION: u32 = 2;
const OLDEST_READABLE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
enum StoredKind {
    CherryPick,
    Revert,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSequence {
    version: u32,
    kind: StoredKind,
    commits: Vec<String>,
    next: usize,
    skipped: Vec<String>,
    original_head: String,
    expected_head: String,
    #[serde(default)]
    stopped_on_conflict: bool,
    /// Full ref name, `null` for a detached HEAD; absent in version 1 (unknown).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    original_ref: Option<StoredRef>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "name")]
enum StoredRef {
    Branch(String),
    Detached,
}

impl From<&SequenceState> for StoredSequence {
    fn from(state: &SequenceState) -> Self {
        Self {
            version: FORMAT_VERSION,
            kind: match state.kind {
                SequenceKind::CherryPick => StoredKind::CherryPick,
                SequenceKind::Revert => StoredKind::Revert,
            },
            commits: state.commits.clone(),
            next: state.next,
            skipped: state.skipped.clone(),
            original_head: state.original_head.clone(),
            expected_head: state.expected_head.clone(),
            stopped_on_conflict: state.stopped_on_conflict,
            original_ref: match &state.original_ref {
                OriginalRef::Branch(name) => Some(StoredRef::Branch(name.clone())),
                OriginalRef::Detached => Some(StoredRef::Detached),
                OriginalRef::Unknown => None,
            },
        }
    }
}

impl From<StoredSequence> for SequenceState {
    fn from(stored: StoredSequence) -> Self {
        Self {
            kind: match stored.kind {
                StoredKind::CherryPick => SequenceKind::CherryPick,
                StoredKind::Revert => SequenceKind::Revert,
            },
            commits: stored.commits,
            next: stored.next,
            skipped: stored.skipped,
            original_head: stored.original_head,
            expected_head: stored.expected_head,
            stopped_on_conflict: stored.stopped_on_conflict,
            original_ref: match stored.original_ref {
                Some(StoredRef::Branch(name)) => OriginalRef::Branch(name),
                Some(StoredRef::Detached) => OriginalRef::Detached,
                None => OriginalRef::Unknown,
            },
        }
    }
}

pub(super) struct SequenceStore {
    file: PathBuf,
    /// The last "ignoring" message logged, so a bad file is reported once, not on every command.
    last_warning: RefCell<Option<String>>,
}

impl SequenceStore {
    /// `repo.path()` is the worktree's own git dir, so every worktree has its own sequence.
    pub fn open(repo: &Repository) -> Self {
        Self {
            file: repo.path().join("browsitory").join("sequence.json"),
            last_warning: RefCell::new(None),
        }
    }

    /// The stored sequence, or `None` when there is none or the file can't be used. The state
    /// is checked against `repo`: every id must be a commit there. Whether HEAD or git's own
    /// paused state still match is `continue_sequence`/`abort_sequence`'s job: a stale but
    /// well-formed sequence is still reported, so the user can see it and abort it.
    pub fn load(&self, repo: &Repository) -> Option<SequenceState> {
        let bytes = match fs::read(&self.file) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.last_warning.replace(None);
                return None;
            }
            Err(error) => return self.ignore(format!("can't read it: {error}")),
        };
        let stored = match serde_json::from_slice::<StoredSequence>(&bytes) {
            Ok(stored) => stored,
            Err(error) => return self.ignore(format!("malformed: {error}")),
        };
        if !(OLDEST_READABLE_VERSION..=FORMAT_VERSION).contains(&stored.version) {
            return self.ignore(format!("unknown version {}", stored.version));
        }
        let state = SequenceState::from(stored);
        if let Err(reason) = validate(&state, repo) {
            return self.ignore(reason);
        }
        self.last_warning.replace(None);
        Some(state)
    }

    fn ignore(&self, reason: String) -> Option<SequenceState> {
        let message = format!(
            "browsitory: ignoring sequence state {}: {reason}",
            self.file.display()
        );
        if self.last_warning.borrow().as_deref() != Some(message.as_str()) {
            eprintln!("{message}");
            self.last_warning.replace(Some(message));
        }
        None
    }

    pub fn save(&self, state: &SequenceState) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(&StoredSequence::from(state))
            .map_err(|error| error.to_string())?;
        let dir = self.file.parent().expect("sequence file has a parent");
        fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        let temp = dir.join(format!("sequence.json.{}.tmp", std::process::id()));
        let write = || -> std::io::Result<()> {
            let mut handle = fs::File::create(&temp)?;
            handle.write_all(&json)?;
            handle.sync_all()?;
            fs::rename(&temp, &self.file)
        };
        if let Err(error) = write() {
            let _ = fs::remove_file(&temp);
            return Err(error.to_string());
        }
        self.last_warning.replace(None);
        Ok(())
    }

    /// Deletes the file; a missing file is fine.
    pub fn clear(&self) -> Result<(), String> {
        match fs::remove_file(&self.file) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.to_string()),
            _ => {
                self.last_warning.replace(None);
                Ok(())
            }
        }
    }
}

fn validate(state: &SequenceState, repo: &Repository) -> Result<(), String> {
    if state.commits.is_empty() || state.next >= state.commits.len() {
        return Err(format!(
            "step {} of {} is out of range",
            state.next + 1,
            state.commits.len()
        ));
    }
    if let Some(stray) = state.skipped.iter().find(|id| !state.commits.contains(id)) {
        return Err(format!(
            "skipped commit {stray} is not part of the sequence"
        ));
    }
    let ids = state
        .commits
        .iter()
        .chain([&state.original_head, &state.expected_head]);
    for id in ids {
        let found = Oid::from_str(id)
            .ok()
            .is_some_and(|oid| repo.find_commit(oid).is_ok());
        if !found {
            return Err(format!("{id:?} is not a commit in this repository"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A repo with three commits; returns their ids oldest-first.
    fn repo_with_commits() -> (tempfile::TempDir, Repository, Vec<String>) {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        let tree = {
            let id = repo.treebuilder(None).unwrap().write().unwrap();
            repo.find_tree(id).unwrap()
        };
        let mut ids: Vec<String> = Vec::new();
        for message in ["a", "b", "c"] {
            let parent = ids
                .last()
                .map(|id| repo.find_commit(Oid::from_str(id).unwrap()).unwrap());
            let parents: Vec<&git2::Commit> = parent.iter().collect();
            let id = repo
                .commit(None, &sig, &sig, message, &tree, &parents)
                .unwrap();
            ids.push(id.to_string());
        }
        drop(tree);
        (dir, repo, ids)
    }

    fn state(ids: &[String]) -> SequenceState {
        SequenceState {
            kind: SequenceKind::Revert,
            commits: vec![ids[1].clone(), ids[0].clone()],
            next: 1,
            skipped: vec![ids[1].clone()],
            original_head: ids[2].clone(),
            expected_head: ids[2].clone(),
            stopped_on_conflict: true,
            original_ref: OriginalRef::Branch("refs/heads/main".into()),
        }
    }

    fn file(repo: &Repository) -> PathBuf {
        repo.path().join("browsitory/sequence.json")
    }

    #[test]
    fn saves_and_loads_a_state_round_trip() {
        let (_dir, repo, ids) = repo_with_commits();
        let store = SequenceStore::open(&repo);
        assert_eq!(store.load(&repo), None);
        store.save(&state(&ids)).unwrap();
        assert!(file(&repo).exists());
        assert_eq!(store.load(&repo), Some(state(&ids)));
        // No temp file left behind.
        let leftovers: Vec<_> = fs::read_dir(repo.path().join("browsitory"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, ["sequence.json"]);
        store.clear().unwrap();
        assert_eq!(store.load(&repo), None);
        store.clear().unwrap(); // clearing twice is fine
    }

    #[test]
    fn a_new_store_on_the_same_repo_sees_the_saved_state() {
        // What an app restart does: a fresh worker opens the store again.
        let (dir, repo, ids) = repo_with_commits();
        SequenceStore::open(&repo).save(&state(&ids)).unwrap();
        let reopened = Repository::open(dir.path()).unwrap();
        assert_eq!(
            SequenceStore::open(&reopened).load(&reopened),
            Some(state(&ids))
        );
    }

    #[test]
    fn a_corrupt_or_unknown_state_file_is_ignored() {
        let (_dir, repo, ids) = repo_with_commits();
        let store = SequenceStore::open(&repo);
        fs::create_dir_all(repo.path().join("browsitory")).unwrap();
        for content in [
            "{not json".to_string(),
            String::new(),
            "\u{0}\u{1}binary".to_string(),
            r#"{"version":1}"#.to_string(),
            r#"{"version":1,"kind":"Rebase","commits":[],"next":0,"skipped":[],"originalHead":"","expectedHead":""}"#.to_string(),
            format!(
                r#"{{"version":3,"kind":"CherryPick","commits":["{0}"],"next":0,"skipped":[],"originalHead":"{0}","expectedHead":"{0}"}}"#,
                ids[0]
            ),
        ] {
            fs::write(file(&repo), &content).unwrap();
            assert_eq!(store.load(&repo), None, "{content}");
        }
        store.save(&state(&ids)).unwrap(); // overwrites the unreadable file
        assert_eq!(store.load(&repo), Some(state(&ids)));
    }

    #[test]
    fn a_state_that_does_not_fit_this_repository_is_ignored() {
        let (_dir, repo, ids) = repo_with_commits();
        let store = SequenceStore::open(&repo);
        let foreign = "f".repeat(40);
        let cases = [
            SequenceState {
                commits: vec![foreign.clone()],
                next: 0,
                skipped: vec![],
                ..state(&ids)
            },
            SequenceState {
                original_head: "not an id".into(),
                ..state(&ids)
            },
            SequenceState {
                next: 2,
                ..state(&ids)
            },
            SequenceState {
                commits: vec![],
                next: 0,
                skipped: vec![],
                ..state(&ids)
            },
            SequenceState {
                skipped: vec![ids[2].clone()],
                ..state(&ids)
            },
        ];
        for case in cases {
            store.save(&case).unwrap();
            assert_eq!(store.load(&repo), None, "{case:?}");
        }
    }

    #[test]
    fn a_directory_in_place_of_the_file_is_ignored_and_reported_on_save() {
        let (_dir, repo, ids) = repo_with_commits();
        let store = SequenceStore::open(&repo);
        fs::create_dir_all(file(&repo)).unwrap();
        assert_eq!(store.load(&repo), None);
        assert!(store.save(&state(&ids)).is_err());
        assert!(store.clear().is_err());
    }

    #[test]
    fn a_version_1_file_loads_as_an_error_stop_with_an_unknown_ref() {
        let (_dir, repo, ids) = repo_with_commits();
        let store = SequenceStore::open(&repo);
        fs::create_dir_all(repo.path().join("browsitory")).unwrap();
        fs::write(
            file(&repo),
            format!(
                r#"{{"version":1,"kind":"CherryPick","commits":["{0}"],"next":0,"skipped":[],"originalHead":"{0}","expectedHead":"{0}"}}"#,
                ids[0]
            ),
        )
        .unwrap();
        let state = store.load(&repo).unwrap();
        assert!(!state.stopped_on_conflict);
        assert_eq!(state.original_ref, OriginalRef::Unknown);
        assert_eq!(state.kind, SequenceKind::CherryPick);
    }

    #[test]
    fn a_detached_original_head_round_trips() {
        let (_dir, repo, ids) = repo_with_commits();
        let store = SequenceStore::open(&repo);
        let detached = SequenceState {
            original_ref: OriginalRef::Detached,
            ..state(&ids)
        };
        store.save(&detached).unwrap();
        assert_eq!(store.load(&repo), Some(detached));
    }

    #[test]
    fn the_wire_format_is_camel_case_with_a_version() {
        let (_dir, repo, ids) = repo_with_commits();
        let store = SequenceStore::open(&repo);
        store.save(&state(&ids)).unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(file(&repo)).unwrap()).unwrap();
        assert_eq!(json["version"], 2);
        assert_eq!(json["stoppedOnConflict"], true);
        assert_eq!(
            json["originalRef"],
            serde_json::json!({ "kind": "Branch", "name": "refs/heads/main" })
        );
        assert_eq!(json["kind"], "Revert");
        assert_eq!(json["next"], 1);
        assert_eq!(json["commits"], serde_json::json!([ids[1], ids[0]]));
        assert_eq!(json["skipped"], serde_json::json!([ids[1]]));
        assert_eq!(json["originalHead"], ids[2]);
        assert_eq!(json["expectedHead"], ids[2]);
    }
}
