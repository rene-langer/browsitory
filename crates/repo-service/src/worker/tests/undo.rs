use super::*;

#[test]
fn undo_and_redo_a_discard_round_trip_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "precious\n");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    handle.discard_file("a.txt".into()).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\n"
    );
    assert_eq!(
        handle.get_undo_state().unwrap(),
        UndoState {
            undo: Some("Discard changes to a.txt".into()),
            redo: None
        }
    );

    assert_eq!(
        handle.undo().unwrap(),
        UndoOutcome::Done {
            label: "Discard changes to a.txt".into()
        }
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "precious\n"
    );
    assert_eq!(
        handle.get_undo_state().unwrap(),
        UndoState {
            undo: None,
            redo: Some("Discard changes to a.txt".into())
        }
    );

    assert_eq!(
        handle.redo().unwrap(),
        UndoOutcome::Done {
            label: "Discard changes to a.txt".into()
        }
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\n"
    );
}

#[test]
fn failed_and_no_op_commands_record_nothing() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "initial");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert!(handle.delete_branch("missing".into(), false).is_err());
    handle.stage_file("a.txt".into()).unwrap(); // already clean: no change
    assert_eq!(
        handle.get_undo_state().unwrap(),
        UndoState {
            undo: None,
            redo: None
        }
    );
    assert_eq!(handle.undo().unwrap(), UndoOutcome::Nothing);
}

#[test]
fn a_new_command_clears_redo() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "two\n");
    write_file(dir.path(), "b.txt", "b\n");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.stage_file("a.txt".into()).unwrap();
    handle.undo().unwrap();
    handle.stage_file("b.txt".into()).unwrap();
    assert_eq!(
        handle.get_undo_state().unwrap(),
        UndoState {
            undo: Some("Stage b.txt".into()),
            redo: None
        }
    );
}

#[test]
fn external_change_refuses_undo_and_keeps_the_entry() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "initial");
    write_file(dir.path(), "a.txt", "precious\n");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.discard_file("a.txt".into()).unwrap();
    write_file(dir.path(), "a.txt", "typed later\n");
    assert_eq!(
        handle.undo().unwrap(),
        UndoOutcome::RepositoryChanged {
            label: "Discard changes to a.txt".into()
        }
    );
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Discard changes to a.txt".into())
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "typed later\n"
    );
}

/// Undo that would put HEAD back on a branch another worktree has checked out since is a
/// refusal outcome naming that worktree, not a failure, and keeps the entry (#291).
#[test]
fn undo_onto_a_branch_held_by_another_worktree_is_a_refusal() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "initial");
    let base = repo.head().unwrap().shorthand().unwrap().to_string();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .create_branch("other".into(), "HEAD".into(), true)
        .unwrap();
    let label = handle.get_undo_state().unwrap().undo.unwrap();
    let parent = TempDir::new().unwrap();
    let linked = parent.path().join("linked");
    git_core::worktree::create_worktree(&repo, "linked", &linked, &base, None).unwrap();

    let UndoOutcome::BranchHeld {
        label: refused,
        message,
    } = handle.undo().unwrap()
    else {
        panic!("expected a BranchHeld refusal");
    };

    assert_eq!(refused, label);
    assert_eq!(
        held_folder(&message, &base, "is checked out"),
        std::fs::canonicalize(&linked).unwrap()
    );
    assert_eq!(handle.get_undo_state().unwrap().undo, Some(label));
}

#[test]
fn stage_during_a_conflicted_merge_is_not_recorded_and_abort_drops_pending() {
    let (dir, _repo) = conflicting_branches();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let outcome = handle
        .start_merge(
            "feature".into(),
            git_core::merge::MergeOptions::default(),
            no_events(),
        )
        .unwrap();
    assert!(matches!(outcome, MergeOutcome::Conflicted { .. }));
    // A recordable command while the merge is paused must not become its own entry.
    write_file(dir.path(), "b.txt", "unrelated\n");
    handle.stage_file("b.txt".into()).unwrap();
    assert_eq!(
        handle.get_undo_state().unwrap(),
        UndoState {
            undo: None,
            redo: None
        }
    );
    assert_eq!(handle.undo().unwrap(), UndoOutcome::OperationInProgress);
    assert_eq!(handle.redo().unwrap(), UndoOutcome::OperationInProgress);
    // An untracked file written mid-merge survives the abort, so the repo afterwards differs
    // from before the merge: only an explicit drop — not a "nothing changed" check — keeps
    // the aborted merge off the stack.
    write_file(dir.path(), "notes.txt", "kept\n");
    handle.abort_merge().unwrap();
    assert!(dir.path().join("notes.txt").exists());
    assert_eq!(
        handle.get_undo_state().unwrap(),
        UndoState {
            undo: None,
            redo: None
        }
    );
    assert_eq!(handle.undo().unwrap(), UndoOutcome::Nothing);
    // And the recorder is idle again: the next command records normally.
    handle
        .create_branch("after".into(), "HEAD".into(), true)
        .unwrap();
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Create branch after".into())
    );
}

#[test]
fn a_conflicted_merge_finished_by_commit_is_one_entry() {
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
    handle
        .resolve_conflict("a.txt".into(), "resolved\n".into())
        .unwrap();
    handle
        .commit("Merge feature".into(), false, Vec::new(), mpsc::channel().0)
        .unwrap();
    assert_eq!(
        handle.get_undo_state().unwrap(),
        UndoState {
            undo: Some("Merge feature".into()),
            redo: None
        }
    );
    assert_eq!(
        handle.undo().unwrap(),
        UndoOutcome::Done {
            label: "Merge feature".into()
        }
    );
    let reopened = git2::Repository::open(dir.path()).unwrap();
    assert_eq!(reopened.head().unwrap().target().unwrap(), main_tip);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "main\n"
    );
}

#[test]
fn history_survives_a_worker_restart_and_is_capped() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "one\n");
    commit_all(&repo, "initial");
    {
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        for n in 0..55 {
            handle
                .create_branch(format!("b{n}"), "HEAD".into(), true)
                .unwrap();
        }
    }
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Create branch b54".into())
    );
    let mut undone = 0;
    while let UndoOutcome::Done { .. } = handle.undo().unwrap() {
        undone += 1;
    }
    assert_eq!(undone, 50);
}
