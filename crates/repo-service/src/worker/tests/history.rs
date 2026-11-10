use super::*;

#[test]
fn get_blame_reflects_a_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "hello\n");
    commit_all(&repo, "initial commit");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let lines = worker
        .handle()
        .get_blame("HEAD".into(), "file.txt".into())
        .unwrap();

    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].content, "hello");
}

#[test]
fn get_file_history_and_rename_diff_round_trip_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "l1\nl2\nl3\nl4\nl5\n");
    commit_all(&repo, "add file");
    write_file(dir.path(), "other.txt", "x\n");
    commit_all(&repo, "unrelated");
    write_file(dir.path(), "file.txt", "l1\nl2\nl3\nl4\nCHANGED\n");
    commit_all(&repo, "edit file");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let history = handle.get_file_history("file.txt".into(), 10).unwrap();

    let summaries: Vec<&str> = history.iter().map(|e| e.commit.summary.as_str()).collect();
    assert_eq!(summaries, vec!["edit file", "add file"]);

    // No rename in this repo: diffing the same path as both sides behaves like commit_diff.
    let hunks = handle
        .get_commit_rename_diff(
            history[0].commit.id.clone(),
            "file.txt".into(),
            "file.txt".into(),
            git_core::diff::DiffOptions::default(),
        )
        .unwrap();
    assert_eq!(hunks.len(), 1);
}

#[test]
fn get_line_history_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "l1\nl2\nl3\nl4\nl5\n");
    commit_all(&repo, "add file");
    write_file(dir.path(), "file.txt", "l1\nTWO\nl3\nl4\nl5\n");
    commit_all(&repo, "edit two");
    write_file(dir.path(), "file.txt", "l1\nTWO\nl3\nl4\nFIVE\n");
    commit_all(&repo, "edit five");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let history = handle
        .get_line_history("HEAD".into(), "file.txt".into(), 1, 3, 10)
        .unwrap();

    let summaries: Vec<&str> = history
        .entries
        .iter()
        .map(|e| e.commit.summary.as_str())
        .collect();
    assert_eq!(summaries, vec!["edit two", "add file"]);
    assert!(!history.truncated);

    let error = handle
        .get_line_history("HEAD".into(), "file.txt".into(), 4, 9, 10)
        .unwrap_err();
    assert_eq!(
        error,
        crate::wire::ErrorDto::from("Lines 4-9 aren't in the file, which has 5 lines.")
    );
}

#[test]
fn range_and_working_tree_compares_round_trip_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "first");
    let first = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    write_file(dir.path(), "a.txt", "two\n");
    commit_all(&repo, "second");
    let second = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    write_file(dir.path(), "a.txt", "three\n");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let a = git_core::diff::ChangedFile {
        path: "a.txt".into(),
        old_path: None,
    };
    let opts = git_core::diff::DiffOptions::default();

    assert_eq!(
        handle
            .get_range_files(first.clone(), second.clone())
            .unwrap(),
        vec![a.clone()]
    );
    let hunks = handle
        .get_range_diff(first.clone(), second, "a.txt".into(), None, opts)
        .unwrap();
    assert!(hunks[0].lines.iter().any(|l| l.content == "two\n"));

    assert_eq!(
        handle
            .get_working_tree_compare_files(first.clone())
            .unwrap(),
        vec![a]
    );
    let hunks = handle
        .get_working_tree_compare_diff(first, "a.txt".into(), None, opts)
        .unwrap();
    assert!(hunks[0].lines.iter().any(|l| l.content == "three\n"));
}

#[test]
fn branch_compare_range_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    // A name no `init.defaultBranch` would pick, so HEAD is never this branch.
    repo.branch("compare-worker-target", &base, false).unwrap();
    write_file(dir.path(), "a.txt", "two\n");
    commit_all(&repo, "main only");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let range = handle
        .get_branch_compare_range("refs/heads/compare-worker-target".into())
        .unwrap();
    assert_eq!(range.from_id, base.id().to_string());
    assert_eq!(range.to_id, base.id().to_string());
    assert_eq!(
        handle
            .get_branch_compare_range("refs/heads/nope".into())
            .unwrap_err(),
        crate::wire::ErrorDto::from("No branch named \"nope\".")
    );
}

#[test]
fn history_commands_round_trip_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "base\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    let base = repo.head().unwrap().target().unwrap().to_string();
    git_core::branch::create_branch(&repo, "side", "HEAD").unwrap();
    git_core::branch::switch_branch(&repo, "side").unwrap();
    write_file(dir.path(), "side.txt", "side\n");
    commit_all(&repo, "side commit");
    let side = repo.head().unwrap().target().unwrap().to_string();
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    assert_eq!(handle.get_head_info().unwrap().head_id, Some(base.clone()));

    let outcome = handle.cherry_pick(side.clone(), None, no_events()).unwrap();
    assert!(matches!(
        outcome,
        git_core::pick::PickOutcome::Committed { .. }
    ));

    handle
        .amend_commit(
            "side, amended".into(),
            false,
            None,
            Vec::new(),
            mpsc::channel().0,
        )
        .unwrap();
    let picked = handle.get_head_info().unwrap().head_id.unwrap();

    let reverted = handle.revert_commit(picked, None, no_events()).unwrap();
    assert!(matches!(
        reverted,
        git_core::pick::PickOutcome::Committed { .. }
    ));

    handle
        .reset_to_commit(base.clone(), git_core::reset::ResetMode::Hard)
        .unwrap();
    assert_eq!(handle.get_head_info().unwrap().head_id, Some(base.clone()));

    handle.checkout_commit(side, no_events()).unwrap();
    assert!(handle.get_head_info().unwrap().detached_at.is_some());

    assert!(handle
        .cherry_pick("not-a-sha".into(), None, no_events())
        .is_err());
    assert!(
        handle.get_head_info().is_ok(),
        "worker survives a failed command"
    );
}
