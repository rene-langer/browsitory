//! #448 through the worker: a commit's hunk reverted (undoable), a patch saved and applied;
//! #658: previewed, and applied as commits (one Undo step).
use super::*;
use crate::worker::{CommitChange, PatchSource};
use git_core::patch::PatchApply;

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
    let error = handle
        .apply_patch(patch.clone(), PatchApply::WorkTree)
        .unwrap_err();
    assert!(
        error.message.starts_with("The patch doesn't apply"),
        "{error}"
    );
    assert_eq!(handle.get_undo_state().unwrap().undo, None);

    // Reverted first, it applies, and Undo takes it back.
    write_file(dir.path(), "a.txt", "one\n");
    handle.apply_patch(patch, PatchApply::WorkTree).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "two\n"
    );
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Apply patch".to_string())
    );
}

#[test]
fn a_patch_applied_as_commits_is_one_undo_step() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "base");
    let base = head(&repo);
    write_file(dir.path(), "a.txt", "two\n");
    commit_all(&repo, "Second");
    write_file(dir.path(), "a.txt", "three\n");
    commit_all(&repo, "Third");
    let tip = head(&repo);
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let walk: Vec<String> = {
        let mut revwalk = repo.revwalk().unwrap();
        revwalk.push(git2::Oid::from_str(&tip).unwrap()).unwrap();
        revwalk.set_sorting(git2::Sort::REVERSE).unwrap();
        revwalk.map(|id| id.unwrap().to_string()).skip(1).collect()
    };
    let patch = handle.get_patch(PatchSource::Commits(walk)).unwrap();
    let base_commit = repo
        .find_commit(git2::Oid::from_str(&base).unwrap())
        .unwrap();
    repo.reset(base_commit.as_object(), git2::ResetType::Hard, None)
        .unwrap();

    let preview = handle.preview_patch(patch.clone()).unwrap();
    assert_eq!(preview.parts.len(), 2);
    assert_eq!(preview.problem, None);
    assert_eq!(head(&repo), base, "the preview changed HEAD");

    handle.apply_patch(patch, PatchApply::Commits).unwrap();
    let applied = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(applied.summary().unwrap(), Some("Third"));
    assert_eq!(
        applied.parent(0).unwrap().parent_id(0).unwrap().to_string(),
        base
    );
    let label = "Apply patch as commits".to_string();
    assert_eq!(handle.get_undo_state().unwrap().undo, Some(label.clone()));
    assert_eq!(handle.undo().unwrap(), UndoOutcome::Done { label });
    assert_eq!(head(&repo), base);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\n"
    );
}

/// Two commits on `base` as a patch series, with HEAD reset to `base`.
fn series_on_base(
    dir: &Path,
    repo: &git2::Repository,
    handle: &crate::worker::WorkerHandle,
) -> (String, Vec<u8>) {
    write_file(dir, "a.txt", "one\n");
    commit_all(repo, "base");
    let base = head(repo);
    write_file(dir, "a.txt", "two\n");
    commit_all(repo, "Second");
    let second = head(repo);
    write_file(dir, "a.txt", "three\n");
    commit_all(repo, "Third");
    let third = head(repo);
    let patch = handle
        .get_patch(PatchSource::Commits(vec![second, third]))
        .unwrap();
    let base_commit = repo
        .find_commit(git2::Oid::from_str(&base).unwrap())
        .unwrap();
    repo.reset(base_commit.as_object(), git2::ResetType::Hard, None)
        .unwrap();
    (base, patch)
}

#[cfg(unix)]
#[test]
fn a_rejecting_applypatch_msg_replies_hook_failed_reports_the_hook_and_records_nothing() {
    let (dir, repo) = init_repo();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let (base, patch) = series_on_base(dir.path(), &repo, &handle);
    install_hook(
        &repo,
        "applypatch-msg",
        "if grep -q Third \"$1\"; then echo 'no Third'; exit 1; fi",
    );
    let undo_before = handle.get_undo_state().unwrap().undo;
    let (events_tx, events_rx) = mpsc::channel();

    let outcome = handle
        .apply_patch_reported(patch, PatchApply::Commits, events_tx)
        .unwrap();

    assert_eq!(
        outcome,
        git_core::patch::PatchApplyOutcome::HookFailed {
            hook: "applypatch-msg".into(),
            output: "no Third\n".into(),
            subject: "Third".into(),
        }
    );
    let events: Vec<HookEvent> = events_rx.iter().collect();
    assert!(
        matches!(&events[..], [
            HookEvent::Started { operation_id: a, hook: h1, source: s1 },
            HookEvent::Finished { operation_id: b, hook: h2, .. },
            HookEvent::Started { operation_id: c, .. },
            HookEvent::Finished { .. },
        ] if a == b && a == c && a.starts_with("apply-") && h1 == "applypatch-msg"
            && h2 == "applypatch-msg" && *s1 == crate::worker::HookSource::Patch),
        "{events:?}"
    );
    assert_eq!(head(&repo), base);
    assert_eq!(handle.get_undo_state().unwrap().undo, undo_before);
}

#[cfg(unix)]
#[test]
fn hooks_that_accept_leave_one_applied_outcome_and_one_undo_step() {
    let (dir, repo) = init_repo();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let (_base, patch) = series_on_base(dir.path(), &repo, &handle);
    install_hook(&repo, "pre-applypatch", "exit 0");
    install_hook(&repo, "post-applypatch", "exit 1");
    let (events_tx, _events_rx) = mpsc::channel();

    let outcome = handle
        .apply_patch_reported(patch, PatchApply::Commits, events_tx)
        .unwrap();

    assert_eq!(outcome, git_core::patch::PatchApplyOutcome::Applied);
    let applied = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(applied.summary().unwrap(), Some("Third"));
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Apply patch as commits".to_string())
    );
}
