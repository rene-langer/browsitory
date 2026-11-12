use super::*;

fn rebase_file(repo: &Repository) -> std::path::PathBuf {
    repo.path().join("browsitory").join("rebase.json")
}

/// `base` (onto; f.txt = "base\n") → "a" (a.txt) → "b" (f.txt = "from b\n") on the
/// checked-out branch. Returns `(onto, plan [Edit a, Pick b], branch ref, branch tip)`.
fn edit_then_pick(
    dir: &Path,
    repo: &Repository,
) -> (
    String,
    Vec<git_core::rebase::RebasePlanEntry>,
    String,
    git2::Oid,
) {
    write_file(dir, "f.txt", "base\n");
    commit_all(repo, "base");
    let onto = repo.head().unwrap().target().unwrap().to_string();
    write_file(dir, "a.txt", "a\n");
    commit_all(repo, "a");
    let a = repo.head().unwrap().target().unwrap().to_string();
    write_file(dir, "f.txt", "from b\n");
    commit_all(repo, "b");
    let head = repo.head().unwrap();
    let tip = head.target().unwrap();
    let entry = |id: String, action| git_core::rebase::RebasePlanEntry {
        commit_id: id,
        action,
        combined_message: None,
    };
    (
        onto,
        vec![
            entry(a, git_core::rebase::RebaseAction::Edit),
            entry(tip.to_string(), git_core::rebase::RebaseAction::Pick),
        ],
        head.name().unwrap().to_string(),
        tip,
    )
}

fn paused_for_edit(
    dir: &Path,
    repo: &Repository,
) -> (Worker, super::super::WorkerHandle, String, git2::Oid) {
    let (onto, plan, branch, tip) = edit_then_pick(dir, repo);
    let worker = Worker::spawn(dir.to_path_buf()).unwrap();
    let handle = worker.handle();
    assert_eq!(
        handle.start_rebase(onto, plan, None).unwrap(),
        git_core::rebase::RebaseStepResult::PausedForEdit
    );
    (worker, handle, branch, tip)
}

fn branch_target(path: &Path, branch: &str) -> git2::Oid {
    Repository::open(path)
        .unwrap()
        .find_reference(branch)
        .unwrap()
        .target()
        .unwrap()
}

/// `edit_then_pick`'s repository with `f.txt` modified on disk (#170).
fn dirty_before_rebase(
    dir: &Path,
    repo: &Repository,
) -> (
    Worker,
    super::super::WorkerHandle,
    String,
    Vec<git_core::rebase::RebasePlanEntry>,
) {
    let (onto, plan, _, _) = edit_then_pick(dir, repo);
    write_file(dir, "f.txt", "DIRTY\n");
    let worker = Worker::spawn(dir.to_path_buf()).unwrap();
    let handle = worker.handle();
    (worker, handle, onto, plan)
}

#[test]
fn a_refused_rebase_start_records_no_undo_entry_and_keeps_the_edit() {
    let (dir, repo) = init_repo();
    let (_worker, handle, onto, plan) = dirty_before_rebase(dir.path(), &repo);
    let before = handle.get_undo_state().unwrap();

    let error = handle.start_rebase(onto, plan, None).unwrap_err();
    assert_eq!(
        error,
        crate::wire::ErrorDto::from(
            "You have uncommitted changes to f.txt. Commit or stash them before starting a rebase."
        )
    );
    assert_eq!(handle.get_undo_state().unwrap(), before);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
        "DIRTY\n"
    );
}

#[test]
fn a_refused_rebase_start_leaves_no_rebase_stored() {
    let (dir, repo) = init_repo();
    let (_worker, handle, onto, plan) = dirty_before_rebase(dir.path(), &repo);
    handle.start_rebase(onto, plan, None).unwrap_err();
    assert_eq!(rebase_progress(&handle), None);
    assert!(!rebase_file(&repo).exists());
}

#[test]
fn a_paused_rebase_survives_a_worker_restart() {
    let (dir, repo) = init_repo();
    let (worker, handle, _, _) = paused_for_edit(dir.path(), &repo);
    assert!(rebase_file(&repo).exists());

    let (_worker, handle) = restart(worker, handle, dir.path());

    assert_eq!(rebase_progress(&handle), Some((1, 2)));
}

#[test]
fn a_restarted_worker_continues_the_rebase_to_completion() {
    let (dir, repo) = init_repo();
    let (worker, handle, branch, tip) = paused_for_edit(dir.path(), &repo);
    let (_worker, handle) = restart(worker, handle, dir.path());

    assert_eq!(
        handle.rebase_continue().unwrap(),
        git_core::rebase::RebaseStepResult::Done
    );

    // Replayed onto its own base, b may get its old id back; check the result instead.
    let _ = tip;
    let reopened = Repository::open(dir.path()).unwrap();
    assert_eq!(reopened.head().unwrap().name().unwrap(), branch);
    let head = reopened.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.summary().unwrap(), Some("b"));
    assert_eq!(head.parent(0).unwrap().summary().unwrap(), Some("a"));
    assert_eq!(reopened.state(), git2::RepositoryState::Clean);
    assert!(!rebase_file(&repo).exists());
    assert_eq!(rebase_progress(&handle), None);
}

/// Edit commit… (#882): the stop survives a restart with the edited commit named, and the
/// restarted worker recommits it under the message Continue passes.
#[test]
fn a_restarted_worker_continues_an_edit_with_a_new_message() {
    let (dir, repo) = init_repo();
    let (onto, plan, branch, _) = edit_then_pick(dir.path(), &repo);
    let a = plan[0].commit_id.clone();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let edit = crate::worker::HistoryEdit {
        kind: crate::worker::HistoryEditKind::Edit,
        commit_id: a.clone(),
    };
    assert_eq!(
        handle.start_rebase(onto, plan, Some(edit)).unwrap(),
        git_core::rebase::RebaseStepResult::PausedForEdit
    );
    write_file(dir.path(), "a.txt", "a, edited\n");
    handle.stage_file("a.txt".to_string()).unwrap();
    let (_worker, handle) = restart(worker, handle, dir.path());
    let editing = handle.get_head_info().unwrap().rebase.unwrap().editing;
    assert_eq!(editing.as_deref(), Some(a.as_str()));

    assert_eq!(
        handle
            .rebase_continue_with_message(Some("a, edited\n".to_string()))
            .unwrap(),
        git_core::rebase::RebaseStepResult::Done
    );

    let reopened = Repository::open(dir.path()).unwrap();
    assert_eq!(reopened.head().unwrap().name().unwrap(), branch);
    let head = reopened.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.summary().unwrap(), Some("b"));
    let edited = head.parent(0).unwrap();
    assert_eq!(edited.message().unwrap(), "a, edited\n");
    let blob = edited
        .tree()
        .unwrap()
        .get_path(Path::new("a.txt"))
        .unwrap()
        .to_object(&reopened)
        .unwrap()
        .peel_to_blob()
        .unwrap();
    assert_eq!(blob.content(), b"a, edited\n");
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some(format!("Edit {}", &a[..7]))
    );
    assert!(!rebase_file(&repo).exists());
}

#[test]
fn a_restarted_worker_aborts_the_rebase() {
    let (dir, repo) = init_repo();
    let (worker, handle, branch, tip) = paused_for_edit(dir.path(), &repo);
    let (_worker, handle) = restart(worker, handle, dir.path());

    handle.abort_rebase().unwrap();

    assert_eq!(branch_target(dir.path(), &branch), tip);
    let reopened = Repository::open(dir.path()).unwrap();
    assert_eq!(reopened.head().unwrap().name().unwrap(), branch);
    assert!(!rebase_file(&repo).exists());
    assert_eq!(rebase_progress(&handle), None);
}

#[test]
fn a_crashed_worker_keeps_the_paused_rebase() {
    let (dir, repo) = init_repo();
    let (onto, plan, _, _) = edit_then_pick(dir.path(), &repo);
    let path = dir.path().to_string_lossy().into_owned();
    let registry = WorkerRegistry::new();
    let (_, handle) = registry.open(&path).unwrap();
    handle.start_rebase(onto, plan, None).unwrap();

    handle.crash_for_test();
    while handle.get_status().is_ok() {
        thread::sleep(Duration::from_millis(10));
    }
    let (_, fresh) = registry.open(&path).unwrap();

    assert_eq!(rebase_progress(&fresh), Some((1, 2)));
}

#[test]
fn a_rebase_error_stop_is_persisted_with_its_cursor() {
    let (dir, repo) = init_repo();
    let (worker, handle, _, _) = paused_for_edit(dir.path(), &repo);
    // A local edit in the way of b: a lands, b is refused.
    write_file(dir.path(), "f.txt", "local\n");
    assert!(handle.rebase_continue().is_err());
    let (_worker, handle) = restart(worker, handle, dir.path());
    assert_eq!(rebase_progress(&handle), Some((2, 2)));

    write_file(dir.path(), "f.txt", "base\n");
    assert_eq!(
        handle.rebase_continue().unwrap(),
        git_core::rebase::RebaseStepResult::Done
    );

    let reopened = Repository::open(dir.path()).unwrap();
    let head = reopened.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.summary().unwrap(), Some("b"));
    assert_ne!(head.tree_id(), head.parent(0).unwrap().tree_id());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("f.txt")).unwrap(),
        "from b\n"
    );
}

/// The forced checkout of Abort can't create a file in a read-only folder, so Abort fails.
/// The rebase must stay stored and HEAD where the rebase left it, so Abort can be retried.
#[test]
#[cfg(unix)]
fn a_failed_abort_keeps_the_stored_rebase() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = init_repo();
    std::fs::create_dir(dir.path().join("d")).unwrap();
    write_file(dir.path(), "d/g.txt", "base\n");
    commit_all(&repo, "base");
    let onto = repo.head().unwrap().target().unwrap().to_string();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "a");
    let a = repo.head().unwrap().target().unwrap().to_string();
    // The branch tip has d/new.txt, which going back to the branch must create.
    write_file(dir.path(), "d/new.txt", "new\n");
    commit_all(&repo, "b");
    let branch = repo.head().unwrap().name().unwrap().to_string();
    let tip = repo.head().unwrap().target().unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let plan = vec![git_core::rebase::RebasePlanEntry {
        commit_id: a,
        action: git_core::rebase::RebaseAction::Edit,
        combined_message: None,
    }];
    assert_eq!(
        handle.start_rebase(onto, plan, None).unwrap(),
        git_core::rebase::RebaseStepResult::PausedForEdit
    );
    assert!(!dir.path().join("d/new.txt").exists());
    let folder = dir.path().join("d");
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(folder.join("probe"), "").is_ok() {
        // Root ignores folder permissions: no reliable failure to provoke here.
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }

    let failed = handle.abort_rebase();
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert!(failed.is_err());
    assert!(rebase_file(&repo).exists());
    assert_eq!(rebase_progress(&handle), Some((1, 1)));
    assert!(Repository::open(dir.path())
        .unwrap()
        .head_detached()
        .unwrap());
    handle.abort_rebase().unwrap();
    let reopened = Repository::open(dir.path()).unwrap();
    assert_eq!(reopened.head().unwrap().name().unwrap(), branch);
    assert_eq!(reopened.head().unwrap().target().unwrap(), tip);
    assert_eq!(
        std::fs::read_to_string(folder.join("new.txt")).unwrap(),
        "new\n"
    );
    assert!(!rebase_file(&repo).exists());
}

#[test]
fn a_failed_save_keeps_the_rebase_for_the_session() {
    let (dir, repo) = init_repo();
    let (onto, plan, branch, _) = edit_then_pick(dir.path(), &repo);
    std::fs::create_dir_all(rebase_file(&repo)).unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let error = handle.start_rebase(onto, plan, None).unwrap_err();

    assert!(error.message.contains("could not be saved"), "{error}");
    let short = branch.trim_start_matches("refs/heads/");
    assert!(
        error.message.contains(&format!("git switch {short}")),
        "{error}"
    );
    assert_eq!(rebase_progress(&handle), Some((1, 2)));
    assert_eq!(
        handle.rebase_continue().unwrap(),
        git_core::rebase::RebaseStepResult::Done
    );
    assert_eq!(rebase_progress(&handle), None);
}

/// Every command that would move HEAD or rewrite the index under a stored rebase replies
/// with the operation-in-progress text, and HEAD stays put.
fn assert_moves_are_refused(handle: &super::super::WorkerHandle, path: &Path) {
    let busy = "Finish or abort the current merge/cherry-pick/revert/rebase first.";
    let repo = Repository::open(path).unwrap();
    let head = repo.head().unwrap().target().unwrap();
    let id = head.to_string();
    let branch = repo
        .branches(Some(git2::BranchType::Local))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .0
        .name()
        .unwrap()
        .unwrap()
        .to_string();
    assert_eq!(
        handle
            .switch_branch(branch.clone(), no_events())
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle
            .cherry_pick(id.clone(), None, no_events())
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle
            .cherry_pick_many(vec![id.clone()], no_events())
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle
            .commit("x".into(), false, Vec::new(), mpsc::channel().0)
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle
            .amend_commit("x".into(), false, None, Vec::new(), mpsc::channel().0)
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle
            .start_merge(
                branch,
                git_core::merge::MergeOptions::default(),
                no_events()
            )
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle.start_rebase(id.clone(), vec![], None).unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle
            .reset_to_commit(id.clone(), git_core::reset::ResetMode::Hard)
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle.checkout_commit(id.clone(), no_events()).unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle.pop_stash(0, id, false).unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(repo.head().unwrap().target().unwrap(), head);
}

#[test]
fn commands_are_refused_while_a_rebase_is_stored() {
    let (dir, repo) = init_repo();
    let (_worker, handle, _, _) = paused_for_edit(dir.path(), &repo);
    assert_moves_are_refused(&handle, dir.path());
}

#[test]
fn commands_are_refused_after_a_rebase_error_stop() {
    let (dir, repo) = init_repo();
    let (_worker, handle, _, _) = paused_for_edit(dir.path(), &repo);
    write_file(dir.path(), "f.txt", "local\n");
    assert!(handle.rebase_continue().is_err());
    // To git nothing is going on now; only the stored rebase says otherwise.
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    assert_moves_are_refused(&handle, dir.path());
}

#[test]
fn staging_and_conflict_resolution_stay_available_during_a_rebase() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "base\n");
    commit_all(&repo, "base");
    let onto = repo.head().unwrap().target().unwrap().to_string();
    write_file(dir.path(), "a.txt", "on top\n");
    commit_all(&repo, "on top");
    write_file(dir.path(), "a.txt", "again\n");
    commit_all(&repo, "again");
    let picked = repo.head().unwrap().target().unwrap().to_string();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let plan = vec![git_core::rebase::RebasePlanEntry {
        commit_id: picked,
        action: git_core::rebase::RebaseAction::Pick,
        combined_message: None,
    }];
    assert!(matches!(
        handle.start_rebase(onto, plan, None).unwrap(),
        git_core::rebase::RebaseStepResult::Conflicted { .. }
    ));

    handle
        .resolve_conflict("a.txt".into(), "resolved\n".into())
        .unwrap();
    write_file(dir.path(), "notes.txt", "notes\n");
    handle.stage_file("notes.txt".into()).unwrap();
    handle.unstage_file("notes.txt".into()).unwrap();

    assert_eq!(
        handle.rebase_continue().unwrap(),
        git_core::rebase::RebaseStepResult::Done
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "resolved\n"
    );
}

fn health(info: &git_core::repo::HeadInfo) -> git_core::rebase::RebaseHealth {
    info.rebase.as_ref().expect("a stored rebase").health
}

#[test]
fn get_head_info_reports_a_paused_rebase_after_restart() {
    let (dir, repo) = init_repo();
    let (worker, handle, branch, _) = paused_for_edit(dir.path(), &repo);
    // An edit pause leaves HEAD on `onto`: the step is applied to the index only.
    let onto = Repository::open(dir.path())
        .unwrap()
        .head()
        .unwrap()
        .target()
        .unwrap();
    let (_worker, handle) = restart(worker, handle, dir.path());

    let info = handle.get_head_info().unwrap();

    let rebase = info.rebase.unwrap();
    assert_eq!(rebase.branch, branch.trim_start_matches("refs/heads/"));
    assert_eq!(rebase.onto, onto.to_string());
    assert_eq!((rebase.current_step, rebase.total_steps), (1, 2));
    assert_eq!(rebase.health, git_core::rebase::RebaseHealth::Paused);
}

#[test]
fn get_head_info_reports_a_diverged_rebase_when_head_moved_outside() {
    let (dir, repo) = init_repo();
    let (worker, handle, branch, _) = paused_for_edit(dir.path(), &repo);
    drop((worker, handle));
    // `git switch <branch>` in a terminal while the app is closed.
    repo.set_head(&branch).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let info = handle.get_head_info().unwrap();

    assert_eq!(
        health(&info),
        git_core::rebase::RebaseHealth::Diverged(git_core::rebase::RebaseDivergence::HeadMoved)
    );
    assert_eq!(
        handle
            .checkout_commit(info.head_id.unwrap(), no_events())
            .unwrap_err(),
        crate::wire::ErrorDto::from(
            "Finish or abort the current merge/cherry-pick/revert/rebase first."
        )
    );
    assert_eq!(
        handle.abort_rebase().unwrap(),
        git_core::rebase::RebaseAbort { restored: false }
    );
    assert_eq!(handle.get_head_info().unwrap().rebase, None);
    assert!(!rebase_file(&repo).exists());
}

#[test]
fn get_head_info_reports_no_rebase_after_it_finishes() {
    let (dir, repo) = init_repo();
    let (_worker, handle, _, _) = paused_for_edit(dir.path(), &repo);
    assert!(handle.get_head_info().unwrap().rebase.is_some());
    handle.rebase_continue().unwrap();
    assert_eq!(handle.get_head_info().unwrap().rebase, None);
}

fn no_undo() -> UndoState {
    UndoState {
        undo: None,
        redo: None,
    }
}

fn undo_file(repo: &Repository) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(repo.path().join("browsitory/undo.json")).unwrap())
        .unwrap()
}

#[test]
fn a_sequence_finished_after_a_restart_is_one_undo_entry() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let original = repo.head().unwrap().target().unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert!(matches!(
        handle.cherry_pick_many(ids, no_events()).unwrap(),
        git_core::pick::SequenceOutcome::Conflicted { .. }
    ));
    let (_worker, handle) = restart(worker, handle, dir.path());

    handle
        .resolve_conflict("a.txt".into(), "resolved\n".into())
        .unwrap();
    assert!(matches!(
        handle.continue_sequence(false, mpsc::channel().0).unwrap(),
        git_core::pick::SequenceOutcome::Completed { .. }
    ));

    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Cherry-pick 3 commits".into())
    );
    assert!(matches!(handle.undo().unwrap(), UndoOutcome::Done { .. }));
    assert_eq!(repo.head().unwrap().target().unwrap(), original);
}

#[test]
fn a_rebase_finished_after_a_restart_is_one_undo_entry() {
    let (dir, repo) = init_repo();
    let (worker, handle, branch, tip) = paused_for_edit(dir.path(), &repo);
    let (_worker, handle) = restart(worker, handle, dir.path());
    write_file(dir.path(), "a.txt", "amended\n");
    handle.stage_file("a.txt".into()).unwrap();

    assert_eq!(
        handle.rebase_continue().unwrap(),
        git_core::rebase::RebaseStepResult::Done
    );

    let label = handle.get_undo_state().unwrap().undo.unwrap();
    assert!(label.starts_with("Rebase onto "), "{label}");
    assert!(matches!(handle.undo().unwrap(), UndoOutcome::Done { .. }));
    assert_eq!(branch_target(dir.path(), &branch), tip);
}

#[test]
fn a_merge_finished_after_a_restart_is_one_undo_entry() {
    let (dir, repo) = conflicting_branches();
    let main_tip = repo.head().unwrap().target().unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .start_merge(
            "feature".into(),
            git_core::merge::MergeOptions::default(),
            no_events(),
        )
        .unwrap();
    let (_worker, handle) = restart(worker, handle, dir.path());

    handle
        .resolve_conflict("a.txt".into(), "resolved\n".into())
        .unwrap();
    handle
        .commit("Merge feature".into(), false, Vec::new(), mpsc::channel().0)
        .unwrap();

    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Merge feature".into())
    );
    assert!(matches!(handle.undo().unwrap(), UndoOutcome::Done { .. }));
    let reopened = Repository::open(dir.path()).unwrap();
    assert_eq!(reopened.head().unwrap().target().unwrap(), main_tip);
}

#[test]
fn a_rebase_aborted_after_a_restart_records_nothing() {
    let (dir, repo) = init_repo();
    let (worker, handle, _, _) = paused_for_edit(dir.path(), &repo);
    let (_worker, handle) = restart(worker, handle, dir.path());
    handle.abort_rebase().unwrap();
    assert_eq!(handle.get_undo_state().unwrap(), no_undo());
    assert!(undo_file(&repo).get("pending").is_none());
}

/// `git merge --abort` in a terminal: forget the merge and reset to HEAD.
fn abort_outside(repo: &Repository) {
    repo.cleanup_state().unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.reset(head.as_object(), git2::ResetType::Hard, None)
        .unwrap();
}

#[test]
fn an_operation_aborted_outside_while_closed_drops_the_pending_entry() {
    let (dir, repo) = conflicting_branches();
    {
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        worker
            .handle()
            .start_merge(
                "feature".into(),
                git_core::merge::MergeOptions::default(),
                no_events(),
            )
            .unwrap();
    }
    assert_eq!(undo_file(&repo)["pending"]["label"], "Merge feature");
    abort_outside(&repo);

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    assert_eq!(handle.get_undo_state().unwrap(), no_undo());
    assert!(undo_file(&repo).get("pending").is_none());
    handle
        .create_branch("after".into(), "HEAD".into(), true)
        .unwrap();
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Create branch after".into())
    );
}

#[test]
fn a_different_operation_started_outside_does_not_inherit_the_pending_entry() {
    let (dir, repo) = conflicting_branches();
    {
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let base = head.parent(0).unwrap();
        repo.branch("other", &base, false).unwrap();
    }
    repo.set_head("refs/heads/other").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    write_file(dir.path(), "a.txt", "other\n");
    commit_all(&repo, "other change");
    let main = repo
        .branches(Some(git2::BranchType::Local))
        .unwrap()
        .map(|branch| branch.unwrap().0.name().unwrap().unwrap().to_string())
        .find(|name| name != "other" && name != "feature")
        .unwrap();
    repo.set_head(&format!("refs/heads/{main}")).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    {
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        worker
            .handle()
            .start_merge(
                "feature".into(),
                git_core::merge::MergeOptions::default(),
                no_events(),
            )
            .unwrap();
    }
    abort_outside(&repo);
    // `git merge other` in a terminal, which conflicts too.
    let other = repo
        .find_annotated_commit(repo.refname_to_id("refs/heads/other").unwrap())
        .unwrap();
    repo.merge(&[&other], None, None).unwrap();
    assert!(repo.index().unwrap().has_conflicts());

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .resolve_conflict("a.txt".into(), "resolved\n".into())
        .unwrap();
    handle
        .commit("Merge other".into(), false, Vec::new(), mpsc::channel().0)
        .unwrap();

    // Not "Merge feature"; and "Merge other" can't be recorded: its "before" is unknown.
    assert_eq!(handle.get_undo_state().unwrap(), no_undo());
}

#[test]
fn undo_refuses_while_a_restored_pending_entry_exists() {
    let (dir, repo) = conflicting_branches();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .create_branch("before".into(), "HEAD".into(), true)
        .unwrap();
    handle
        .start_merge(
            "feature".into(),
            git_core::merge::MergeOptions::default(),
            no_events(),
        )
        .unwrap();
    let (_worker, handle) = restart(worker, handle, dir.path());
    assert_eq!(handle.undo().unwrap(), UndoOutcome::OperationInProgress);
    assert_eq!(handle.redo().unwrap(), UndoOutcome::OperationInProgress);
    assert!(Repository::open(dir.path())
        .unwrap()
        .find_branch("before", git2::BranchType::Local)
        .is_ok());
    let _ = repo;
}

#[test]
fn branch_and_stash_commands_are_refused_during_a_rebase() {
    let (dir, repo) = init_repo();
    let (_worker, handle, branch, tip) = paused_for_edit(dir.path(), &repo);
    let busy = "Finish or abort the current merge/cherry-pick/revert/rebase first.";
    let short = branch.trim_start_matches("refs/heads/").to_string();
    assert_eq!(
        handle.delete_branch(short.clone(), true).unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle.rename_branch(short, "renamed".into()).unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle
            .save_stash(git_core::stash::StashOptions::default())
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    let parent = TempDir::new().unwrap();
    assert_eq!(
        handle
            .create_worktree(
                "rebased".into(),
                parent.path().join("rebased"),
                branch.trim_start_matches("refs/heads/").into(),
                None,
            )
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(branch_target(dir.path(), &branch), tip);

    // Creating a branch checks it out (`git checkout -b`), which would move HEAD.
    assert_eq!(
        handle
            .create_branch("new".into(), tip.to_string(), true)
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    // Other branches stay manageable.
    repo.branch("other", &repo.find_commit(tip).unwrap(), false)
        .unwrap();
    handle
        .rename_branch("other".into(), "renamed".into())
        .unwrap();
    handle.delete_branch("renamed".into(), true).unwrap();
}

/// A cherry-pick paused in one worktree protects its branch from the other worktrees'
/// windows (#293). HEAD stays on that branch during the pause, so moving, deleting and
/// switching to it, or adding a worktree on it, were refused already ("checked out"); the
/// refusal now names the cherry-pick. Renaming it is refused too, unlike git: libgit2 moves
/// that worktree's HEAD along, and Abort then left the commits the sequence made in place
/// (`rewound: false`), because HEAD named another branch than the one it started on.
#[test]
fn another_worktree_cant_touch_the_branch_a_paused_sequence_runs_on() {
    let (dir, repo, _worker, handle, branch) = paused_sequence();
    let full = format!("refs/heads/{branch}");
    let original = repo.reflog(&full).unwrap().get(1).unwrap().id_new();
    let paused_at = repo.head().unwrap().target().unwrap();
    assert_ne!(original, paused_at, "the first step landed");
    let parent = TempDir::new().unwrap();
    let linked_path = parent.path().join("linked");
    git_core::worktree::create_worktree(&repo, "linked", &linked_path, "feature", None).unwrap();
    let linked_worker = Worker::spawn(linked_path).unwrap();
    let linked = linked_worker.handle();
    let main_folder = std::fs::canonicalize(dir.path()).unwrap();
    let refused = |result: Result<(), crate::wire::ErrorDto>| {
        let message = result.unwrap_err();
        assert_eq!(
            held_folder(&message.message, &branch, "has a paused cherry-pick"),
            main_folder
        );
    };

    refused(linked.rename_branch(branch.clone(), "renamed".into()));
    refused(linked.delete_branch(branch.clone(), true));
    refused(linked.switch_branch(branch.clone(), no_events()));
    refused(linked.create_worktree(
        "second".into(),
        parent.path().join("second"),
        branch.clone(),
        None,
    ));
    refused(linked.restore_reflog_entry(full.clone(), original.to_string()));
    assert_eq!(repo.head().unwrap().name().unwrap(), full);
    assert_eq!(branch_target(dir.path(), &full), paused_at);

    // Abort rewinds the branch the sequence started on.
    assert_eq!(
        handle.abort_sequence(None).unwrap(),
        git_core::pick::SequenceAbort { rewound: true }
    );
    assert_eq!(repo.head().unwrap().target(), Some(original));
    // Once the sequence is over, the branch is only checked out there, as before.
    let message = linked.delete_branch(branch.clone(), true).unwrap_err();
    assert_eq!(
        held_folder(&message.message, &branch, "is checked out"),
        main_folder
    );
}

/// A rebase paused in one worktree protects its branch from the other worktrees' windows
/// too (#291), with the wording a branch held by another worktree gets: Continue and Abort
/// return to that branch.
#[test]
fn another_worktree_cant_touch_the_branch_a_paused_rebase_rebases() {
    let (dir, repo) = init_repo();
    let (_worker, handle, branch, tip) = paused_for_edit(dir.path(), &repo);
    let short = branch.trim_start_matches("refs/heads/").to_string();
    repo.branch("side", &repo.find_commit(tip).unwrap(), false)
        .unwrap();
    let parent = TempDir::new().unwrap();
    let linked_path = parent.path().join("linked");
    git_core::worktree::create_worktree(&repo, "linked", &linked_path, "side", None).unwrap();
    let linked_worker = Worker::spawn(linked_path).unwrap();
    let linked = linked_worker.handle();
    let main_folder = std::fs::canonicalize(dir.path()).unwrap();
    let refused = |result: Result<(), crate::wire::ErrorDto>| {
        let message = result.unwrap_err();
        assert_eq!(
            held_folder(&message.message, &short, "is being rebased"),
            main_folder
        );
    };

    refused(linked.delete_branch(short.clone(), true));
    refused(linked.rename_branch(short.clone(), "renamed".into()));
    refused(linked.switch_branch(short.clone(), no_events()));
    refused(linked.create_worktree(
        "second".into(),
        parent.path().join("second"),
        short.clone(),
        None,
    ));
    assert_eq!(branch_target(dir.path(), &branch), tip);

    // Once the rebase is aborted, the branch is checked out there again, as before.
    handle.abort_rebase().unwrap();
    let message = linked.delete_branch(short.clone(), true).unwrap_err();
    assert_eq!(
        held_folder(&message.message, &short, "is checked out"),
        main_folder
    );
}

/// A `rebase.json` that can't be deleted (read-only folder) must not keep gating the
/// repository after the rebase ended.
#[test]
#[cfg(unix)]
fn a_rebase_file_that_cannot_be_removed_is_still_forgotten() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = init_repo();
    let (_worker, handle, branch, _) = paused_for_edit(dir.path(), &repo);
    let folder = repo.path().join("browsitory");
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(folder.join("probe"), "").is_ok() {
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
        return; // root: no reliable failure to provoke
    }

    let aborted = handle.abort_rebase();
    let rebase = handle.get_head_info().map(|info| info.rebase);
    let switched =
        handle.switch_branch(branch.trim_start_matches("refs/heads/").into(), no_events());
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(
        aborted.unwrap(),
        git_core::rebase::RebaseAbort { restored: true }
    );
    assert!(rebase_file(&repo).exists());
    assert_eq!(rebase.unwrap(), None);
    assert!(switched.is_ok(), "{switched:?}");
}

#[test]
fn a_pending_entry_recorded_by_another_worker_is_not_recorded_twice() {
    let (dir, _repo) = conflicting_branches();
    let first = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let second = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let (a, b) = (first.handle(), second.handle());
    a.start_merge(
        "feature".into(),
        git_core::merge::MergeOptions::default(),
        no_events(),
    )
    .unwrap();
    assert_eq!(b.get_undo_state().unwrap(), no_undo()); // b now knows the pending entry
    a.resolve_conflict("a.txt".into(), "resolved\n".into())
        .unwrap();
    a.commit("Merge feature".into(), false, Vec::new(), mpsc::channel().0)
        .unwrap();

    b.create_branch("after".into(), "HEAD".into(), true)
        .unwrap();

    assert_eq!(
        b.undo().unwrap(),
        UndoOutcome::Done {
            label: "Create branch after".into()
        }
    );
    assert_eq!(
        b.undo().unwrap(),
        UndoOutcome::Done {
            label: "Merge feature".into()
        }
    );
    assert_eq!(b.undo().unwrap(), UndoOutcome::Nothing);
}

/// Another writer (the other app) moves HEAD while this worker is landing a step, so the
/// step's commit loses HEAD's compare-and-swap. This worker's copy is then stale and must not
/// be saved over the stored rebase. The race is provoked with a fake signing program that
/// moves HEAD on its second call (the second step's commit).
#[test]
#[cfg(unix)]
fn a_continue_that_loses_head_to_another_writer_saves_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = init_repo();
    let (_worker, handle, _, tip) = paused_for_edit(dir.path(), &repo);
    let tools = TempDir::new().unwrap();
    let mark = tools.path().join("called-once");
    let script = tools.path().join("fake-gpg");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\ncat > /dev/null\nif [ -e '{mark}' ]; then git -C '{workdir}' update-ref --no-deref HEAD {tip}; fi\ntouch '{mark}'\nprintf -- '-----BEGIN PGP SIGNATURE-----\\nfake\\n-----END PGP SIGNATURE-----\\n'\necho '[GNUPG:] SIG_CREATED D 1 8 00 0 X' >&2\n",
            mark = mark.display(),
            workdir = dir.path().display(),
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_bool("commit.gpgSign", true).unwrap();
        config
            .set_str("gpg.program", script.to_str().unwrap())
            .unwrap();
    }
    let stored = std::fs::read_to_string(rebase_file(&repo)).unwrap();

    assert!(handle.rebase_continue().is_err());

    assert!(mark.exists(), "the fake signer never ran");
    assert_eq!(
        Repository::open(dir.path())
            .unwrap()
            .head()
            .unwrap()
            .target(),
        Some(tip)
    );
    assert_eq!(std::fs::read_to_string(rebase_file(&repo)).unwrap(), stored);
}

/// #151: a rebase step's signing program is reported as the `signing` pseudo-hook under a
/// `rebase-<n>` id (`HookSource::Pick`, so the hook strip shows it), and cancelling that id stops
/// the signer. The step stays paused, so Continue can try it again.
#[test]
#[cfg(unix)]
fn cancelling_a_rebase_steps_signer_stops_it() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = init_repo();
    let (_worker, handle, _, _tip) = paused_for_edit(dir.path(), &repo);
    let tools = TempDir::new().unwrap();
    let script = tools.path().join("hung-gpg");
    std::fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_bool("commit.gpgSign", true).unwrap();
        config
            .set_str("gpg.program", script.to_str().unwrap())
            .unwrap();
    }
    let started = std::time::Instant::now();
    let (events_tx, events) = mpsc::channel();

    let result = std::thread::scope(|scope| {
        let continued = scope.spawn(|| handle.rebase_continue_reporting(events_tx));
        let first = events.recv().unwrap();
        let super::super::HookEvent::Started {
            operation_id,
            hook,
            source,
        } = &first
        else {
            panic!("{first:?}");
        };
        assert_eq!(hook, "signing");
        assert_eq!(*source, super::super::HookSource::Pick);
        assert!(operation_id.starts_with("rebase-"), "{operation_id}");
        handle.cancel_transfer(operation_id);
        continued.join().unwrap()
    });

    assert_eq!(result.unwrap_err().message, "Signing was cancelled.");
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    assert!(matches!(
        events.recv().unwrap(),
        super::super::HookEvent::Finished { ref hook, .. } if hook == "signing"
    ));
    // Still paused at the step: with signing off, Continue lands it.
    repo.config()
        .unwrap()
        .set_bool("commit.gpgSign", false)
        .unwrap();
    assert_eq!(
        handle.rebase_continue().unwrap(),
        git_core::rebase::RebaseStepResult::Done
    );
}
