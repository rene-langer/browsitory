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
    let result = handle.start_rebase(onto, plan, None).unwrap();

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
    let result = handle.start_rebase(onto, plan, None).unwrap();
    assert_eq!(result, git_core::rebase::RebaseStepResult::PausedForEdit);
    // 1-indexed: the first (and here only) paused step reports "Step 1 of 1", not "0 of 1".
    assert_eq!(rebase_progress(&handle), Some((1, 1)));

    handle.abort_rebase().unwrap();

    assert_eq!(rebase_progress(&handle), None);
    assert!(handle.get_status().unwrap().is_empty());
}

/// #473: a one-click history edit's undo entry names the edit, also once a paused one finishes.
#[test]
fn a_history_edit_records_its_own_undo_label() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    let head = |repo: &Repository| {
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string()
    };
    commit_all(&repo, "base commit");
    let onto = head(&repo);
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    let a = head(&repo);
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");
    let b = head(&repo);

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let entry = |commit_id: &str, action| git_core::rebase::RebasePlanEntry {
        commit_id: commit_id.to_string(),
        action,
        combined_message: None,
    };
    let edit = |kind, commit_id: &str| {
        Some(crate::worker::HistoryEdit {
            kind,
            commit_id: commit_id.to_string(),
        })
    };

    let plan = vec![
        entry(&a, git_core::rebase::RebaseAction::Drop),
        entry(&b, git_core::rebase::RebaseAction::Pick),
    ];
    let result = handle
        .start_rebase(
            onto.clone(),
            plan,
            edit(crate::worker::HistoryEditKind::Drop, &a),
        )
        .unwrap();
    assert_eq!(result, git_core::rebase::RebaseStepResult::Done);
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some(format!("Drop {}", &a[..7]))
    );
    handle.undo().unwrap();
    assert!(dir.path().join("a.txt").exists());

    // Paused on the way (an Edit stop stands in for a conflict), then continued: the entry
    // recorded at the start keeps the edit's label.
    let plan = vec![
        entry(&b, git_core::rebase::RebaseAction::Edit),
        entry(&a, git_core::rebase::RebaseAction::Pick),
    ];
    let result = handle
        .start_rebase(onto, plan, edit(crate::worker::HistoryEditKind::MoveUp, &a))
        .unwrap();
    assert_eq!(result, git_core::rebase::RebaseStepResult::PausedForEdit);
    assert_eq!(
        handle.rebase_continue().unwrap(),
        git_core::rebase::RebaseStepResult::Done
    );
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some(format!("Move {} up", &a[..7]))
    );
}
