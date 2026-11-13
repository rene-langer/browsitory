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

/// The index entry of `path`'s content, as text.
fn staged_content(path: &Path, file: &str) -> String {
    let repo = Repository::open(path).unwrap();
    let index = repo.index().unwrap();
    let entry = index.get_path(Path::new(file), 0).unwrap();
    let blob = repo.find_blob(entry.id).unwrap();
    let content = String::from_utf8_lossy(blob.content()).into_owned();
    content
}

/// #883: folding staged changes into an older commit is one Undo step, which brings the
/// changes back staged on the old tip.
#[test]
fn folding_staged_changes_into_a_commit_is_one_undo_step() {
    let (dir, repo) = init_repo();
    let head = |repo: &Repository| repo.head().unwrap().target().unwrap();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "add a");
    let a = head(&repo).to_string();
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "add b");
    let b = head(&repo);
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    write_file(dir.path(), "a.txt", "a, folded\n");
    handle.stage_file("a.txt".to_string()).unwrap();

    assert_eq!(
        handle.fold_staged(a.clone()).unwrap(),
        git_core::rebase::RebaseStepResult::Done
    );

    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some(format!("Fold staged changes into {}", &a[..7]))
    );
    let reopened = Repository::open(dir.path()).unwrap();
    let tip = reopened.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(tip.summary().unwrap(), Some("add b"));
    let folded = tip.parent(0).unwrap();
    assert_eq!(folded.summary().unwrap(), Some("add a"));
    let blob = folded
        .tree()
        .unwrap()
        .get_path(Path::new("a.txt"))
        .unwrap()
        .to_object(&reopened)
        .unwrap()
        .peel_to_blob()
        .unwrap();
    assert_eq!(blob.content(), b"a, folded\n");
    assert!(handle.get_status().unwrap().is_empty());

    handle.undo().unwrap();

    assert_eq!(head(&Repository::open(dir.path()).unwrap()), b);
    assert_eq!(staged_content(dir.path(), "a.txt"), "a, folded\n");
}
