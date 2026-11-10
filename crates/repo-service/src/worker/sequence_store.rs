//! Persists a stopped multi-commit cherry-pick/revert (`git_core::pick::SequenceState`) to
//! `<gitdir>/browsitory/sequence.json` (format: `git_core::sequence_file`), so Continue/Abort
//! survive an app restart. Loading, the "ignore a bad file, log once" rule and the atomic write
//! are [`StateFile`]'s. Every id must be a commit in the repository; whether HEAD or git's own
//! paused state still match is `continue_sequence`/`abort_sequence`'s job. Other worktrees load
//! the file too, to protect its branch (#293).
use super::state_file::{StateCodec, StateFile};
use std::path::{Path, PathBuf};

use git2::Repository;
use git_core::pick::SequenceState;

pub(super) type SequenceStore = StateFile<SequenceCodec>;

pub(super) struct SequenceCodec;

impl StateCodec for SequenceCodec {
    type State = SequenceState;
    const WHAT: &'static str = "sequence";
    fn file(git_dir: &Path) -> PathBuf {
        git_core::sequence_file::file(git_dir)
    }
    fn encode(state: &SequenceState) -> Result<Vec<u8>, String> {
        git_core::sequence_file::encode(state)
    }
    fn decode(bytes: &[u8], repo: &Repository) -> Result<SequenceState, String> {
        git_core::sequence_file::decode(bytes, repo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Oid;
    use git_core::pick::{OriginalRef, SequenceKind};
    use std::fs;

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
