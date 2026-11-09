//! #448 through the worker: a commit's hunk reverted (undoable), a patch saved and applied.
use super::*;
use crate::worker::{CommitChange, PatchSource};

fn head(repo: &git2::Repository) -> String {
    repo.head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string()
}

#[test]
fn reverting_a_commit_hunk_is_undoable_and_commits_nothing() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\ntwo\nthree\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "a.txt", "one\nTWO\nthree\n");
    commit_all(&repo, "shout");
    let id = head(&repo);
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let hunks = handle
        .get_commit_diff(
            id.clone(),
            "a.txt".into(),
            git_core::diff::DiffOptions::default(),
        )
        .unwrap();

    handle
        .apply_commit_change(CommitChange {
            commit_id: id.clone(),
            path: "a.txt".into(),
            old_path: None,
            hunk: Some(git_core::patch::CommitHunk {
                old_start: hunks[0].old_start,
                new_start: hunks[0].new_start,
                options: git_core::diff::HunkOptions {
                    context_lines: 3,
                    algorithm: git_core::diff::DiffAlgorithm::Default,
                },
            }),
            reverse: true,
            stage: false,
        })
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\ntwo\nthree\n"
    );
    assert_eq!(head(&repo), id);
    let label = format!("Revert hunk in a.txt from {}", &id[..7]);
    assert_eq!(handle.get_undo_state().unwrap().undo, Some(label.clone()));

    assert_eq!(handle.undo().unwrap(), UndoOutcome::Done { label });
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\nTWO\nthree\n"
    );
}

#[test]
fn a_patch_that_does_not_apply_records_nothing() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "a.txt", "two\n");
    commit_all(&repo, "change");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let patch = handle
        .get_patch(PatchSource::Commits(vec![head(&repo)]))
        .unwrap();

    // The working tree already holds the change: the patch doesn't apply.
    let error = handle.apply_patch(patch.clone()).unwrap_err();
    assert!(error.starts_with("The patch doesn't apply"), "{error}");
    assert_eq!(handle.get_undo_state().unwrap().undo, None);

    // Reverted first, it applies, and Undo takes it back.
    write_file(dir.path(), "a.txt", "one\n");
    handle.apply_patch(patch).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "two\n"
    );
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Apply patch".to_string())
    );
}
