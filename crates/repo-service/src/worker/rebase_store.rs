//! Persists a paused interactive rebase (`git_core::rebase::RebaseState`) to
//! `<gitdir>/browsitory/rebase.json` (format: `git_core::rebase_file`), so Continue/Abort
//! survive an app restart, a closed tab and a crashed worker. Loading, the "ignore a bad file,
//! log once" rule and the atomic write are [`StateFile`]'s. Whether HEAD and the branch still
//! match a loaded rebase is `rebase_health`'s job. Other worktrees load the file too, to protect
//! its branch (#291).
use super::state_file::{StateCodec, StateFile};
use std::path::{Path, PathBuf};

use git2::Repository;
use git_core::rebase::RebaseState;

pub(super) type RebaseStore = StateFile<RebaseCodec>;

pub(super) struct RebaseCodec;

impl StateCodec for RebaseCodec {
    type State = RebaseState;
    const WHAT: &'static str = "rebase";
    fn file(git_dir: &Path) -> PathBuf {
        git_core::rebase_file::file(git_dir)
    }
    fn encode(state: &RebaseState) -> Result<Vec<u8>, String> {
        git_core::rebase_file::encode(state)
    }
    fn decode(bytes: &[u8], repo: &Repository) -> Result<RebaseState, String> {
        git_core::rebase_file::decode(bytes, repo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Oid;
    use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseStop};
    use std::fs;

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
            fold_base: None,
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
            fold_base: None,
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
        let mut version_3 = valid.clone();
        version_3["version"] = 3.into();
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
            version_3.to_string(),
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
            fold_base: None,
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
                "version": 2,
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
