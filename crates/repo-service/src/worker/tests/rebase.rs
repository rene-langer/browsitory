use super::*;

#[test]
fn commits_since_and_start_rebase_round_trip_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let commits = handle.commits_since(onto.clone()).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].summary, "add a");

    let plan = vec![git_core::rebase::RebasePlanEntry {
        commit_id: commits[0].id.clone(),
        action: git_core::rebase::RebaseAction::Pick,
        combined_message: None,
    }];
    let result = handle.start_rebase(onto, plan).unwrap();

    assert_eq!(result, git_core::rebase::RebaseStepResult::Done);
    assert_eq!(rebase_progress(&handle), None);
}

#[test]
fn rebase_continue_and_abort_rebase_round_trip_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let onto = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let commits = handle.commits_since(onto.clone()).unwrap();
    let plan = vec![git_core::rebase::RebasePlanEntry {
        commit_id: commits[0].id.clone(),
        action: git_core::rebase::RebaseAction::Edit,
        combined_message: None,
    }];
    let result = handle.start_rebase(onto, plan).unwrap();
    assert_eq!(result, git_core::rebase::RebaseStepResult::PausedForEdit);
    // 1-indexed: the first (and here only) paused step reports "Step 1 of 1", not "0 of 1".
    assert_eq!(rebase_progress(&handle), Some((1, 1)));

    handle.abort_rebase().unwrap();

    assert_eq!(rebase_progress(&handle), None);
    assert!(handle.get_status().unwrap().is_empty());
}
