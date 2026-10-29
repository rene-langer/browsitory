//! Persists a paused interactive rebase (`git_core::rebase::RebaseState`) to
//! `<gitdir>/browsitory/rebase.json`, so Continue/Abort survive an app restart, a closed tab and
//! a crashed worker. Written atomically (temp file + `sync_all` + `rename`, like
//! `sequence.json`). A file that is unreadable, malformed, of another version, or that doesn't
//! describe commits of this repository is ignored (logged once), never fatal: a corrupt file
//! must not wedge the repository.
//!
//! Versioning follows `sequence_store`: a later version that only adds fields with a safe
//! default bumps `FORMAT_VERSION` and keeps reading the older one (`OLDEST_READABLE_VERSION`).
//! An unknown version is ignored, and the next `StartRebase` overwrites it.
use std::cell::RefCell;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use git2::{Oid, Repository};
use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseState, RebaseStop};
use serde::{Deserialize, Serialize};

const FORMAT_VERSION: u32 = 1;
const OLDEST_READABLE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredRebase {
    version: u32,
    original_branch_ref: String,
    original_tip: String,
    onto: String,
    plan: Vec<StoredEntry>,
    cursor: usize,
    group_start_parent: Option<String>,
    expected_head: String,
    stop: StoredStop,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredEntry {
    commit_id: String,
    action: StoredAction,
    combined_message: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind")]
enum StoredAction {
    Pick,
    Reword { message: String },
    Edit,
    Squash,
    Fixup,
    Drop,
}

#[derive(Serialize, Deserialize)]
enum StoredStop {
    Conflict,
    Edit,
    Error,
}

impl From<&RebaseState> for StoredRebase {
    fn from(state: &RebaseState) -> Self {
        Self {
            version: FORMAT_VERSION,
            original_branch_ref: state.original_branch_ref.clone(),
            original_tip: state.original_tip.to_string(),
            onto: state.onto.to_string(),
            plan: state
                .plan
                .iter()
                .map(|entry| StoredEntry {
                    commit_id: entry.commit_id.clone(),
                    action: match &entry.action {
                        RebaseAction::Pick => StoredAction::Pick,
                        RebaseAction::Reword { message } => StoredAction::Reword {
                            message: message.clone(),
                        },
                        RebaseAction::Edit => StoredAction::Edit,
                        RebaseAction::Squash => StoredAction::Squash,
                        RebaseAction::Fixup => StoredAction::Fixup,
                        RebaseAction::Drop => StoredAction::Drop,
                    },
                    combined_message: entry.combined_message.clone(),
                })
                .collect(),
            cursor: state.cursor,
            group_start_parent: state.group_start_parent.map(|oid| oid.to_string()),
            expected_head: state.expected_head.to_string(),
            stop: match state.stop {
                RebaseStop::Conflict => StoredStop::Conflict,
                RebaseStop::Edit => StoredStop::Edit,
                RebaseStop::Error => StoredStop::Error,
            },
        }
    }
}

/// Rebuilds the state, checking it against `repo`: the plan is non-empty with the cursor in
/// range and not on a `Drop`, the branch is a valid `refs/heads/` name, every id is a commit
/// here, and a `Reword` has a message.
fn to_state(stored: StoredRebase, repo: &Repository) -> Result<RebaseState, String> {
    if stored.plan.is_empty() || stored.cursor > stored.plan.len() {
        return Err(format!(
            "step {} of {} is out of range",
            stored.cursor + 1,
            stored.plan.len()
        ));
    }
    let branch = &stored.original_branch_ref;
    if !branch.starts_with("refs/heads/") || !git2::Reference::is_valid_name(branch) {
        return Err(format!("{branch:?} is not a branch"));
    }
    let commit = |id: &str| -> Result<Oid, String> {
        Oid::from_str(id)
            .ok()
            .filter(|oid| repo.find_commit(*oid).is_ok())
            .ok_or_else(|| format!("{id:?} is not a commit in this repository"))
    };
    let mut plan = Vec::with_capacity(stored.plan.len());
    for entry in stored.plan {
        commit(&entry.commit_id)?;
        let action = match entry.action {
            StoredAction::Pick => RebaseAction::Pick,
            StoredAction::Reword { message } if message.trim().is_empty() => {
                return Err("a reworded step has no message".to_string())
            }
            StoredAction::Reword { message } => RebaseAction::Reword { message },
            StoredAction::Edit => RebaseAction::Edit,
            StoredAction::Squash => RebaseAction::Squash,
            StoredAction::Fixup => RebaseAction::Fixup,
            StoredAction::Drop => RebaseAction::Drop,
        };
        plan.push(RebasePlanEntry {
            commit_id: entry.commit_id,
            action,
            combined_message: entry.combined_message,
        });
    }
    if plan
        .get(stored.cursor)
        .is_some_and(|entry| entry.action == RebaseAction::Drop)
    {
        return Err("the current step is dropped".to_string());
    }
    Ok(RebaseState {
        plan,
        cursor: stored.cursor,
        original_tip: commit(&stored.original_tip)?,
        onto: commit(&stored.onto)?,
        group_start_parent: stored
            .group_start_parent
            .as_deref()
            .map(commit)
            .transpose()?,
        expected_head: commit(&stored.expected_head)?,
        original_branch_ref: stored.original_branch_ref,
        stop: match stored.stop {
            StoredStop::Conflict => RebaseStop::Conflict,
            StoredStop::Edit => RebaseStop::Edit,
            StoredStop::Error => RebaseStop::Error,
        },
    })
}

pub(super) struct RebaseStore {
    file: PathBuf,
    /// The last "ignoring" message logged, so a bad file is reported once, not on every command.
    last_warning: RefCell<Option<String>>,
}

impl RebaseStore {
    /// `repo.path()` is the worktree's own git dir, so every worktree has its own rebase.
    pub fn open(repo: &Repository) -> Self {
        Self {
            file: repo.path().join("browsitory").join("rebase.json"),
            last_warning: RefCell::new(None),
        }
    }

    /// The stored rebase, or `None` when there is none or the file can't be used. Whether HEAD
    /// and the branch still match is `rebase_health`'s job: a stale but well-formed rebase is
    /// still reported, so the user can see it and abort it.
    pub fn load(&self, repo: &Repository) -> Option<RebaseState> {
        let bytes = match fs::read(&self.file) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.last_warning.replace(None);
                return None;
            }
            Err(error) => return self.ignore(format!("can't read it: {error}")),
        };
        let stored = match serde_json::from_slice::<StoredRebase>(&bytes) {
            Ok(stored) => stored,
            Err(error) => return self.ignore(format!("malformed: {error}")),
        };
        if !(OLDEST_READABLE_VERSION..=FORMAT_VERSION).contains(&stored.version) {
            return self.ignore(format!("unknown version {}", stored.version));
        }
        match to_state(stored, repo) {
            Ok(state) => {
                self.last_warning.replace(None);
                Some(state)
            }
            Err(reason) => self.ignore(reason),
        }
    }

    fn ignore(&self, reason: String) -> Option<RebaseState> {
        let message = format!(
            "browsitory: ignoring rebase state {}: {reason}",
            self.file.display()
        );
        if self.last_warning.borrow().as_deref() != Some(message.as_str()) {
            eprintln!("{message}");
            self.last_warning.replace(Some(message));
        }
        None
    }

    pub fn save(&self, state: &RebaseState) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(&StoredRebase::from(state))
            .map_err(|error| error.to_string())?;
        let dir = self.file.parent().expect("rebase file has a parent");
        fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        let temp = dir.join(format!("rebase.json.{}.tmp", std::process::id()));
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A repo with three commits on `refs/heads/main`; returns their ids oldest-first.
    fn repo_with_commits() -> (tempfile::TempDir, Repository, Vec<Oid>) {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        let tree = {
            let id = repo.treebuilder(None).unwrap().write().unwrap();
            repo.find_tree(id).unwrap()
        };
        let mut ids: Vec<Oid> = Vec::new();
        for message in ["a", "b", "c"] {
            let parent = ids.last().map(|id| repo.find_commit(*id).unwrap());
            let parents: Vec<&git2::Commit> = parent.iter().collect();
            ids.push(
                repo.commit(None, &sig, &sig, message, &tree, &parents)
                    .unwrap(),
            );
        }
        repo.reference("refs/heads/main", ids[2], true, "test")
            .unwrap();
        drop(tree);
        (dir, repo, ids)
    }

    fn entry(id: Oid, action: RebaseAction) -> RebasePlanEntry {
        RebasePlanEntry {
            commit_id: id.to_string(),
            action,
            combined_message: None,
        }
    }

    fn state(ids: &[Oid]) -> RebaseState {
        RebaseState {
            plan: vec![
                RebasePlanEntry {
                    combined_message: Some("both".into()),
                    ..entry(ids[1], RebaseAction::Pick)
                },
                entry(
                    ids[2],
                    RebaseAction::Reword {
                        message: "new message".into(),
                    },
                ),
            ],
            cursor: 1,
            original_branch_ref: "refs/heads/main".into(),
            original_tip: ids[2],
            onto: ids[0],
            group_start_parent: Some(ids[0]),
            expected_head: ids[1],
            stop: RebaseStop::Conflict,
        }
    }

    fn file(repo: &Repository) -> PathBuf {
        repo.path().join("browsitory/rebase.json")
    }

    #[test]
    fn saves_and_loads_a_state_round_trip() {
        let (_dir, repo, ids) = repo_with_commits();
        let store = RebaseStore::open(&repo);
        assert_eq!(store.load(&repo), None);
        store.save(&state(&ids)).unwrap();
        assert!(file(&repo).exists());
        assert_eq!(store.load(&repo), Some(state(&ids)));
        let every_action = RebaseState {
            plan: vec![
                entry(ids[0], RebaseAction::Pick),
                entry(ids[1], RebaseAction::Edit),
                entry(ids[1], RebaseAction::Squash),
                entry(ids[2], RebaseAction::Fixup),
                entry(ids[2], RebaseAction::Drop),
            ],
            cursor: 1,
            group_start_parent: None,
            stop: RebaseStop::Edit,
            ..state(&ids)
        };
        store.save(&every_action).unwrap();
        assert_eq!(store.load(&repo), Some(every_action));
        // No temp file left behind.
        let leftovers: Vec<_> = fs::read_dir(repo.path().join("browsitory"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, ["rebase.json"]);
        store.clear().unwrap();
        assert_eq!(store.load(&repo), None);
        store.clear().unwrap(); // clearing twice is fine
    }

    #[test]
    fn a_new_store_on_the_same_repo_sees_the_saved_state() {
        // What an app restart does: a fresh worker opens the store again.
        let (dir, repo, ids) = repo_with_commits();
        RebaseStore::open(&repo).save(&state(&ids)).unwrap();
        let reopened = Repository::open(dir.path()).unwrap();
        assert_eq!(
            RebaseStore::open(&reopened).load(&reopened),
            Some(state(&ids))
        );
    }

    #[test]
    fn a_corrupt_or_unknown_state_file_is_ignored() {
        let (_dir, repo, ids) = repo_with_commits();
        let store = RebaseStore::open(&repo);
        store.save(&state(&ids)).unwrap();
        let valid: serde_json::Value =
            serde_json::from_slice(&fs::read(file(&repo)).unwrap()).unwrap();
        let mut version_2 = valid.clone();
        version_2["version"] = 2.into();
        let mut bad_action = valid.clone();
        bad_action["plan"][0]["action"] = serde_json::json!({ "kind": "Rebase" });
        let mut no_kind = valid.clone();
        no_kind["plan"][0]["action"] = serde_json::json!({});
        let mut bad_stop = valid;
        bad_stop["stop"] = "Paused".into();
        for content in [
            "{not json".to_string(),
            String::new(),
            "\u{0}\u{1}binary".to_string(),
            r#"{"version":1}"#.to_string(),
            version_2.to_string(),
            bad_action.to_string(),
            no_kind.to_string(),
            bad_stop.to_string(),
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
        let store = RebaseStore::open(&repo);
        let foreign = Oid::from_str(&"f".repeat(40)).unwrap();
        let cases = [
            RebaseState {
                plan: vec![entry(foreign, RebaseAction::Pick)],
                cursor: 0,
                ..state(&ids)
            },
            RebaseState {
                expected_head: foreign,
                ..state(&ids)
            },
            RebaseState {
                onto: foreign,
                ..state(&ids)
            },
            RebaseState {
                original_tip: foreign,
                ..state(&ids)
            },
            RebaseState {
                group_start_parent: Some(foreign),
                ..state(&ids)
            },
            RebaseState {
                cursor: 3,
                ..state(&ids)
            },
            RebaseState {
                plan: vec![],
                cursor: 0,
                ..state(&ids)
            },
            RebaseState {
                plan: vec![
                    entry(ids[1], RebaseAction::Pick),
                    entry(ids[2], RebaseAction::Drop),
                ],
                ..state(&ids)
            },
            RebaseState {
                plan: vec![
                    entry(ids[1], RebaseAction::Pick),
                    entry(
                        ids[2],
                        RebaseAction::Reword {
                            message: " \n".into(),
                        },
                    ),
                ],
                ..state(&ids)
            },
            RebaseState {
                original_branch_ref: "HEAD".into(),
                ..state(&ids)
            },
            RebaseState {
                original_branch_ref: "refs/tags/x".into(),
                ..state(&ids)
            },
            RebaseState {
                original_branch_ref: "refs/heads/bad..name".into(),
                ..state(&ids)
            },
        ];
        for case in cases {
            store.save(&case).unwrap();
            assert!(file(&repo).exists());
            assert_eq!(store.load(&repo), None, "{case:?}");
        }
    }

    #[test]
    fn a_cursor_past_the_end_after_a_failed_finish_loads() {
        let (_dir, repo, ids) = repo_with_commits();
        let store = RebaseStore::open(&repo);
        let finishing = RebaseState {
            cursor: 2,
            stop: RebaseStop::Error,
            ..state(&ids)
        };
        store.save(&finishing).unwrap();
        assert_eq!(store.load(&repo), Some(finishing));
    }

    #[test]
    fn a_directory_in_place_of_the_file_is_ignored_and_reported_on_save() {
        let (_dir, repo, ids) = repo_with_commits();
        let store = RebaseStore::open(&repo);
        fs::create_dir_all(file(&repo)).unwrap();
        assert_eq!(store.load(&repo), None);
        assert!(store.save(&state(&ids)).is_err());
        assert!(store.clear().is_err());
    }

    #[test]
    fn the_wire_format_is_camel_case_with_a_version() {
        let (_dir, repo, ids) = repo_with_commits();
        let store = RebaseStore::open(&repo);
        store.save(&state(&ids)).unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(file(&repo)).unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "version": 1,
                "originalBranchRef": "refs/heads/main",
                "originalTip": ids[2].to_string(),
                "onto": ids[0].to_string(),
                "plan": [
                    {
                        "commitId": ids[1].to_string(),
                        "action": { "kind": "Pick" },
                        "combinedMessage": "both",
                    },
                    {
                        "commitId": ids[2].to_string(),
                        "action": { "kind": "Reword", "message": "new message" },
                        "combinedMessage": null,
                    },
                ],
                "cursor": 1,
                "groupStartParent": ids[0].to_string(),
                "expectedHead": ids[1].to_string(),
                "stop": "Conflict",
            })
        );
    }

    #[test]
    fn a_linked_worktree_has_its_own_rebase_file() {
        let (dir, repo, ids) = repo_with_commits();
        let parent = tempfile::TempDir::new().unwrap();
        let wt_path = parent.path().join("wt");
        let mut options = git2::WorktreeAddOptions::new();
        let branch = repo
            .branch("wt", &repo.find_commit(ids[2]).unwrap(), false)
            .unwrap();
        options.reference(Some(branch.get()));
        repo.worktree("wt", &wt_path, Some(&options)).unwrap();
        let linked = Repository::open(&wt_path).unwrap();

        RebaseStore::open(&linked).save(&state(&ids)).unwrap();

        assert!(linked.path().join("browsitory/rebase.json").exists());
        assert_eq!(RebaseStore::open(&repo).load(&repo), None);
        let main = Repository::open(dir.path()).unwrap();
        assert_eq!(RebaseStore::open(&main).load(&main), None);
        assert_eq!(RebaseStore::open(&linked).load(&linked), Some(state(&ids)));
    }
}
