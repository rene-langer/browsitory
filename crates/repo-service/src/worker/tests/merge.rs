use super::*;

fn diverged_feature_repo() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "feature.txt", "new\n");
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "main.txt", "new\n");
    commit_all(&repo, "main commit");
    (dir, repo)
}

#[test]
fn squash_merge_round_trips_and_is_undoable_and_labelled() {
    let (dir, _repo) = diverged_feature_repo();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let outcome = handle
        .start_merge(
            "feature".into(),
            git_core::merge::MergeOptions {
                fast_forward: git_core::merge::FastForward::Allow,
                squash: true,
            },
            no_events(),
        )
        .unwrap();

    assert_eq!(outcome, git_core::merge::MergeOutcome::Squashed);
    assert!(handle.get_head_info().unwrap().squash_message.is_some());
    assert_eq!(
        handle.get_undo_state().unwrap().undo.as_deref(),
        Some("Squash merge feature")
    );
    handle.undo().unwrap();
    assert!(handle.get_status().unwrap().is_empty());
    assert!(!dir.path().join("feature.txt").exists());
    assert!(handle.get_head_info().unwrap().squash_message.is_none());
}

#[test]
fn no_ff_merge_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "feature.txt", "new\n");
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let outcome = handle
        .start_merge(
            "feature".into(),
            git_core::merge::MergeOptions {
                fast_forward: git_core::merge::FastForward::Never,
                squash: false,
            },
            no_events(),
        )
        .unwrap();

    assert_eq!(outcome, git_core::merge::MergeOutcome::Merged);
    assert!(handle.get_merge_message().unwrap().is_some());
}

#[test]
fn merge_defaults_round_trip_through_the_worker() {
    let (dir, repo) = init_repo();
    // Set in the repo's own config so a global `merge.ff` cannot leak in (issue #111).
    repo.config().unwrap().set_str("merge.ff", "only").unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();

    let defaults = worker.handle().merge_defaults().unwrap();

    assert_eq!(defaults.fast_forward, git_core::merge::FastForward::Only);
    assert!(!defaults.squash);
}

#[test]
fn start_merge_and_get_merge_message_round_trip_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "shared.txt", "line one\nfeature two\n");
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "shared.txt", "line one\nmain two\n");
    commit_all(&repo, "main commit");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let outcome = handle
        .start_merge(
            "feature".into(),
            git_core::merge::MergeOptions::default(),
            no_events(),
        )
        .unwrap();

    assert!(matches!(
        outcome,
        git_core::merge::MergeOutcome::Conflicted { .. }
    ));
    assert!(handle
        .get_merge_message()
        .unwrap()
        .unwrap()
        .contains("feature"));
}

#[test]
fn get_conflict_hunks_then_resolve_conflict_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "shared.txt", "line one\nfeature two\n");
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "shared.txt", "line one\nmain two\n");
    commit_all(&repo, "main commit");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .start_merge(
            "feature".into(),
            git_core::merge::MergeOptions::default(),
            no_events(),
        )
        .unwrap();

    let segments = handle.get_conflict_hunks("shared.txt".into()).unwrap();
    assert!(!segments.is_empty());

    handle
        .resolve_conflict("shared.txt".into(), "line one\nresolved\n".into())
        .unwrap();

    // Resolving and staging a conflict does not empty the status list — the file now
    // differs from HEAD, so it shows as one staged entry, exactly like any other staged
    // change. It only leaves `status()` once actually committed.
    let status = handle.get_status().unwrap();
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].path, "shared.txt");
    assert!(status[0].staged);
    assert_eq!(status[0].kind, git_core::status::StatusKind::Modified);
}

#[test]
fn abort_merge_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "shared.txt", "line one\nfeature two\n");
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "shared.txt", "line one\nmain two\n");
    commit_all(&repo, "main commit");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .start_merge(
            "feature".into(),
            git_core::merge::MergeOptions::default(),
            no_events(),
        )
        .unwrap();

    handle.abort_merge().unwrap();

    assert!(handle.get_merge_message().unwrap().is_none());
    assert!(handle.get_status().unwrap().is_empty());
}

#[test]
fn resolve_add_delete_conflict_round_trips_through_the_worker() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "shared.txt", "v1\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "shared.txt", "v2\n");
    commit_all(&repo, "feature commit modifies");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    std::fs::remove_file(dir.path().join("shared.txt")).unwrap();
    git_core::stage::stage_file(&repo, "shared.txt").unwrap();
    git_core::commit::commit(
        &mut repo,
        "main commit deletes",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .start_merge(
            "feature".into(),
            git_core::merge::MergeOptions::default(),
            no_events(),
        )
        .unwrap();

    handle
        .resolve_add_delete_conflict(
            "shared.txt".into(),
            git_core::merge::FileConflictChoice::Theirs,
        )
        .unwrap();

    let status = handle.get_status().unwrap();
    assert_eq!(status.len(), 1);
    assert!(status[0].staged);
}
