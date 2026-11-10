use super::*;

#[test]
fn a_conflicted_sequence_finished_by_continue_is_one_undo_entry() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let original = repo.head().unwrap().target().unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let outcome = handle.cherry_pick_many(ids.clone(), no_events()).unwrap();
    assert!(matches!(
        outcome,
        git_core::pick::SequenceOutcome::Conflicted { .. }
    ));
    assert!(sequence_file(&repo).exists());
    let progress = handle.get_head_info().unwrap().sequence.unwrap();
    assert_eq!((progress.current_step, progress.total_steps), (2, 3));
    assert_eq!(handle.undo().unwrap(), UndoOutcome::OperationInProgress);
    let busy = "Finish or abort the current merge/cherry-pick/revert/rebase first.";
    assert_eq!(
        handle
            .cherry_pick(ids[0].clone(), None, no_events())
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle.revert_many(ids.clone(), no_events()).unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle
            .start_merge(
                "feature".into(),
                git_core::merge::MergeOptions::default(),
                no_events()
            )
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );

    handle
        .resolve_conflict("a.txt".into(), "resolved\n".into())
        .unwrap();
    let outcome = handle.continue_sequence(false, mpsc::channel().0).unwrap();
    assert_eq!(
        outcome,
        git_core::pick::SequenceOutcome::Completed {
            applied: 3,
            skipped: vec![]
        }
    );
    assert_eq!(handle.get_head_info().unwrap().sequence, None);
    assert!(!sequence_file(&repo).exists());
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Cherry-pick 3 commits".into())
    );
    assert!(matches!(handle.undo().unwrap(), UndoOutcome::Done { .. }));
    assert_eq!(repo.head().unwrap().target().unwrap(), original);
}

#[cfg(unix)]
#[test]
fn continue_sequence_reports_hook_progress_and_a_failure_keeps_the_stored_sequence() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.cherry_pick_many(ids, no_events()).unwrap();
    handle
        .resolve_conflict("a.txt".into(), "resolved\n".into())
        .unwrap();
    install_hook(&repo, "commit-msg", "echo no change id; exit 1");
    let undo_before = handle.get_undo_state().unwrap().undo;
    let (events_tx, events_rx) = mpsc::channel();

    let outcome = handle.continue_sequence(false, events_tx).unwrap();

    let git_core::pick::SequenceOutcome::HookFailed {
        hook,
        output,
        progress,
    } = outcome
    else {
        panic!("expected HookFailed, got {outcome:?}")
    };
    assert_eq!(
        (hook.as_str(), output.as_str()),
        ("commit-msg", "no change id\n")
    );
    assert_eq!(progress.current_step, 2);
    let events: Vec<HookEvent> = events_rx.iter().collect();
    assert!(
        matches!(&events[..], [
            HookEvent::Started { operation_id: a, hook: h1, .. },
            HookEvent::Finished { operation_id: b, hook: h2, .. },
        ] if a == b && a.starts_with("continue-") && h1 == "commit-msg" && h2 == "commit-msg"),
        "{events:?}"
    );
    let stored = handle.get_head_info().unwrap().sequence.unwrap();
    assert_eq!(stored.current_step, 2);
    assert!(sequence_file(&repo).exists());
    assert_eq!(handle.get_undo_state().unwrap().undo, undo_before);
}

#[cfg(unix)]
#[test]
fn cancel_transfer_stops_a_continue_hook() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.cherry_pick_many(ids, no_events()).unwrap();
    handle
        .resolve_conflict("a.txt".into(), "resolved\n".into())
        .unwrap();
    install_hook(&repo, "pre-commit", "sleep 30");
    let (events_tx, events_rx) = mpsc::channel();
    let canceller = handle.clone();
    let cancel_thread = std::thread::spawn(move || {
        if let Ok(HookEvent::Started { operation_id, .. }) = events_rx.recv() {
            canceller.cancel_transfer(&operation_id);
        }
    });
    let started = std::time::Instant::now();

    let result = handle.continue_sequence(false, events_tx);

    cancel_thread.join().unwrap();
    assert_eq!(
        result.unwrap_err(),
        crate::wire::ErrorDto::from("The pre-commit hook was cancelled.")
    );
    assert!(started.elapsed() < Duration::from_secs(25));
    assert_eq!(
        handle
            .get_head_info()
            .unwrap()
            .sequence
            .unwrap()
            .current_step,
        2
    );
}

/// #169: a sequence whose Continue hook failed survives a restart, and Continue without hooks
/// then finishes it.
#[cfg(unix)]
#[test]
fn continue_without_hooks_after_a_restart_finishes_the_sequence() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    install_hook(&repo, "pre-commit", "exit 1");
    {
        let first = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = first.handle();
        handle.cherry_pick_many(ids, no_events()).unwrap();
        handle
            .resolve_conflict("a.txt".into(), "resolved\n".into())
            .unwrap();
        assert!(matches!(
            handle.continue_sequence(false, mpsc::channel().0).unwrap(),
            git_core::pick::SequenceOutcome::HookFailed { .. }
        ));
    }
    let second = Worker::spawn(dir.path().to_path_buf()).unwrap();

    let outcome = second
        .handle()
        .continue_sequence(true, mpsc::channel().0)
        .unwrap();

    assert!(matches!(
        outcome,
        git_core::pick::SequenceOutcome::Completed { applied: 3, .. }
    ));
    assert!(!sequence_file(&repo).exists());
}

#[test]
fn a_stored_sequence_refuses_everything_that_moves_head_or_the_index() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.cherry_pick_many(ids.clone(), no_events()).unwrap();
    let base = repo.head().unwrap().target().unwrap().to_string();
    let busy = "Finish or abort the current merge/cherry-pick/revert/rebase first.";

    assert_eq!(
        handle
            .commit("x".into(), false, Vec::new(), mpsc::channel().0)
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle
            .reset_to_commit(base.clone(), git_core::reset::ResetMode::Hard)
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle
            .checkout_commit(base.clone(), no_events())
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle
            .switch_branch("feature".into(), no_events())
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle.pull_current_upstream(mpsc::channel().0).unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle.apply_stash(0, "s".into(), false).unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle.pop_stash(0, "s".into(), false).unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    assert_eq!(
        handle
            .restore_reflog_entry("HEAD".into(), base.clone())
            .unwrap_err(),
        crate::wire::ErrorDto::from(busy)
    );
    // Read-only and resolving commands still work.
    assert!(handle.get_status().is_ok());
    handle
        .resolve_whole_file("a.txt".into(), git_core::merge::ConflictSide::Theirs)
        .unwrap();
    assert!(handle.continue_sequence(false, mpsc::channel().0).is_ok());
}

#[test]
fn the_undo_label_counts_the_commits_that_were_applied() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    // "f one" is already on the branch: it is skipped, so two commits are applied.
    git_core::pick::cherry_pick(&repo, &ids[0], None, Default::default()).unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.cherry_pick_many(ids.clone(), no_events()).unwrap();
    handle
        .resolve_whole_file("a.txt".into(), git_core::merge::ConflictSide::Theirs)
        .unwrap();
    assert_eq!(
        handle.continue_sequence(false, mpsc::channel().0).unwrap(),
        git_core::pick::SequenceOutcome::Completed {
            applied: 2,
            skipped: vec![ids[0].clone()],
        }
    );
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Cherry-pick 2 commits".into())
    );
}

#[test]
fn an_aborted_sequence_records_nothing() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let original = repo.head().unwrap().target().unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert_eq!(
        handle.revert_many(vec![], no_events()).unwrap_err(),
        crate::wire::ErrorDto::from("Select at least one commit.")
    );
    assert!(!sequence_file(&repo).exists());
    handle.cherry_pick_many(ids, no_events()).unwrap();
    assert_eq!(
        handle.abort_sequence(None).unwrap(),
        git_core::pick::SequenceAbort { rewound: true }
    );
    assert_eq!(repo.head().unwrap().target().unwrap(), original);
    assert_eq!(handle.get_head_info().unwrap().sequence, None);
    assert!(!sequence_file(&repo).exists());
    assert_eq!(
        handle.get_undo_state().unwrap(),
        UndoState {
            undo: None,
            redo: None
        }
    );
    let none = "No cherry-pick or revert is in progress.";
    assert_eq!(
        handle
            .continue_sequence(false, mpsc::channel().0)
            .unwrap_err(),
        crate::wire::ErrorDto::from(none)
    );
    assert_eq!(
        handle.abort_sequence(None).unwrap_err(),
        crate::wire::ErrorDto::from(none)
    );
}

#[test]
fn abort_merge_during_a_sequence_clears_it() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let original = repo.head().unwrap().target().unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.cherry_pick_many(ids, no_events()).unwrap();
    handle.abort_merge().unwrap();
    assert_eq!(handle.get_head_info().unwrap().sequence, None);
    assert!(!sequence_file(&repo).exists());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    assert_eq!(repo.head().unwrap().target().unwrap(), original);
}

/// A terminal `git cherry-pick` list as git leaves it after a stop between steps: one step
/// committed (`b.txt`) on top of where it started, `abort-safety` at that commit, the next
/// step in the todo (#258). Returns where it started and the step's commit.
fn terminal_sequence_between_steps(
    dir: &std::path::Path,
    repo: &Repository,
    next: &str,
) -> (git2::Oid, git2::Oid) {
    let started = repo.head().unwrap().target().unwrap();
    write_file(dir, "b.txt", "b\n");
    commit_all(repo, "f one");
    let step = repo.head().unwrap().target().unwrap();
    let sequencer = repo.path().join("sequencer");
    std::fs::create_dir(&sequencer).unwrap();
    std::fs::write(sequencer.join("todo"), format!("pick {next} f three\n")).unwrap();
    std::fs::write(sequencer.join("head"), format!("{started}\n")).unwrap();
    std::fs::write(sequencer.join("abort-safety"), format!("{step}\n")).unwrap();
    (started, step)
}

#[test]
fn abort_sequence_aborts_a_terminal_sequence_when_none_is_stored() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let (started, _) = terminal_sequence_between_steps(dir.path(), &repo, &ids[2]);
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let info = handle.get_head_info().unwrap().terminal_sequence.unwrap();
    assert!(info.rewinds);
    assert_eq!(info.commits_to_remove, 1);
    let confirmed = git_core::pick::ConfirmedAbort::now(&repo).unwrap();

    assert_eq!(
        handle.abort_sequence(Some(confirmed)).unwrap(),
        git_core::pick::SequenceAbort { rewound: true }
    );

    assert_eq!(repo.head().unwrap().target().unwrap(), started);
    assert!(!dir.path().join("b.txt").exists());
    assert!(!repo.path().join("sequencer").exists());
    assert_eq!(handle.get_head_info().unwrap().terminal_sequence, None);
    assert_eq!(
        handle.abort_sequence(None).unwrap_err(),
        crate::wire::ErrorDto::from("No cherry-pick or revert is in progress.")
    );
}

/// A terminal sequence is aborted only as confirmed: without a confirmation, or with one
/// that no longer matches (the terminal went on), nothing changes (#281 review).
#[test]
fn abort_sequence_refuses_an_unconfirmed_or_outdated_terminal_abort() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let (_, step) = terminal_sequence_between_steps(dir.path(), &repo, &ids[2]);
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let changed =
        "The cherry-pick in the terminal changed since you looked. Check it again, then abort.";

    assert_eq!(
        handle.abort_sequence(None).unwrap_err(),
        crate::wire::ErrorDto::from(changed)
    );
    let outdated = git_core::pick::ConfirmedAbort {
        commits_to_remove: 0,
        rewinds: false,
        ..git_core::pick::ConfirmedAbort::now(&repo).unwrap()
    };
    assert_eq!(
        handle.abort_sequence(Some(outdated)).unwrap_err(),
        crate::wire::ErrorDto::from(changed)
    );

    assert_eq!(repo.head().unwrap().target().unwrap(), step);
    assert!(repo.path().join("sequencer").join("todo").exists());
}

/// The UI routes a terminal sequence's Abort through `abortSequence`; an `AbortMerge` that
/// reaches one comes from a view that predates it, and changes nothing (#253).
#[test]
fn abort_merge_refuses_a_terminal_sequence() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let (_, step) = terminal_sequence_between_steps(dir.path(), &repo, &ids[2]);
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    assert_eq!(
        handle.abort_merge().unwrap_err(),
        crate::wire::ErrorDto::from(
            "This cherry-pick was started in a terminal. Try Abort again to see what it will undo."
        )
    );

    assert_eq!(repo.head().unwrap().target().unwrap(), step);
    assert!(repo.path().join("sequencer").join("todo").exists());
}

/// A `git rebase` started in a terminal and stopped on a conflict (#263): head info reports
/// it (not as a merge), and Abort merge, a new rebase and reset refuse, naming it, with the
/// repository unchanged.
#[test]
fn a_terminal_rebase_is_reported_and_refused() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let base_branch = repo.head().unwrap().shorthand().unwrap().to_string();
    let main_tip = repo.head().unwrap().target().unwrap();
    let git = |args: &[&str]| test_support::git(dir.path(), args);
    assert!(git(&["checkout", "-q", "feature"]).status.success());
    assert!(!git(&["rebase", &base_branch]).status.success());
    let stopped_at = repo.head().unwrap().target().unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let info = handle.get_head_info().unwrap();
    assert_eq!(
        info.terminal_rebase,
        Some(git_core::repo::TerminalRebase {
            kind: git_core::repo::TerminalRebaseKind::Rebase,
            branch: "feature".into(),
            onto: main_tip.to_string(),
            current_step: 2,
            total_steps: 3,
        })
    );
    assert_eq!(info.operation, None);

    let refusal = "A rebase started in a terminal isn't finished. Continue it there with git \
                   rebase --continue, or abort it with git rebase --abort.";
    assert_eq!(
        handle.abort_merge().unwrap_err(),
        crate::wire::ErrorDto::from(refusal)
    );
    let plan = vec![git_core::rebase::RebasePlanEntry {
        commit_id: ids[0].clone(),
        action: git_core::rebase::RebaseAction::Pick,
        combined_message: None,
    }];
    assert_eq!(
        handle
            .start_rebase(main_tip.to_string(), plan, None)
            .unwrap_err(),
        crate::wire::ErrorDto::from(refusal)
    );
    assert_eq!(
        handle
            .reset_to_commit(main_tip.to_string(), git_core::reset::ResetMode::Hard)
            .unwrap_err(),
        crate::wire::ErrorDto::from(refusal)
    );

    assert_eq!(repo.head().unwrap().target().unwrap(), stopped_at);
    assert!(repo.path().join("rebase-merge").is_dir());
    assert!(repo.path().join("MERGE_MSG").exists());
    assert!(std::fs::read_to_string(dir.path().join("a.txt"))
        .unwrap()
        .contains("<<<<<<<"));
}

#[test]
fn abort_sequence_prefers_the_stored_sequence() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    let original = repo.head().unwrap().target().unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert!(matches!(
        handle.cherry_pick_many(ids.clone(), no_events()).unwrap(),
        git_core::pick::SequenceOutcome::Conflicted { .. }
    ));
    // A terminal list appears meanwhile (two lines, so aborting a step keeps it).
    let sequencer = repo.path().join("sequencer");
    std::fs::create_dir(&sequencer).unwrap();
    let todo = format!("pick {} x\npick {} y\n", ids[0], ids[2]);
    std::fs::write(sequencer.join("todo"), &todo).unwrap();

    assert_eq!(
        handle.abort_sequence(None).unwrap(),
        git_core::pick::SequenceAbort { rewound: true }
    );

    assert_eq!(repo.head().unwrap().target().unwrap(), original);
    assert!(!sequence_file(&repo).exists());
    assert_eq!(
        std::fs::read_to_string(sequencer.join("todo")).unwrap(),
        todo
    );
}

fn in_progress() -> String {
    git_core::pick::PickError::OperationInProgress.to_string()
}

#[test]
fn renaming_the_sequence_branch_is_refused_while_a_sequence_is_paused() {
    let (_dir, repo, _worker, handle, branch) = paused_sequence();
    assert_eq!(
        handle.rename_branch(branch.clone(), "other".into()),
        Err(crate::wire::ErrorDto::from(in_progress()))
    );
    assert!(repo.find_branch(&branch, git2::BranchType::Local).is_ok());
    assert_eq!(
        handle.abort_sequence(None).unwrap(),
        git_core::pick::SequenceAbort { rewound: true }
    );
}

#[test]
fn deleting_the_sequence_branch_is_refused_while_a_sequence_is_paused() {
    let (_dir, repo, _worker, handle, branch) = paused_sequence();
    // Move HEAD off the branch (as `git switch` in a terminal), so libgit2's own refusal to
    // delete the current branch doesn't hide the gate.
    repo.cleanup_state().unwrap();
    repo.set_head("refs/heads/feature").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    assert_eq!(
        handle.delete_branch(branch.clone(), true),
        Err(crate::wire::ErrorDto::from(in_progress()))
    );
    assert!(repo.find_branch(&branch, git2::BranchType::Local).is_ok());
}

#[test]
fn adding_a_worktree_on_the_sequence_branch_is_refused() {
    let (dir, _repo, _worker, handle, branch) = paused_sequence();
    let path = dir.path().with_extension("wt");
    assert_eq!(
        handle.create_worktree("wt".into(), path.clone(), branch, None),
        Err(crate::wire::ErrorDto::from(in_progress()))
    );
    assert!(!path.exists());
}

#[test]
fn stash_save_is_refused_while_a_sequence_is_paused() {
    let (dir, repo, _worker, handle, _branch) = paused_sequence();
    write_file(dir.path(), "b.txt", "edited\n");
    assert_eq!(
        handle.save_stash(git_core::stash::StashOptions::default()),
        Err(crate::wire::ErrorDto::from(in_progress()))
    );
    assert!(repo.find_reference("refs/stash").is_err());
    assert_eq!(repo.state(), git2::RepositoryState::CherryPick);
}

#[test]
fn other_branches_stay_manageable_during_a_paused_sequence() {
    let (_dir, repo, _worker, handle, _branch) = paused_sequence();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("spare", &head, false).unwrap();
    handle
        .rename_branch("spare".into(), "spare2".into())
        .unwrap();
    handle.delete_branch("spare2".into(), true).unwrap();
    assert!(repo.find_branch("spare2", git2::BranchType::Local).is_err());
}

#[test]
fn a_stopped_sequence_survives_a_worker_restart() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    {
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        worker.handle().cherry_pick_many(ids, no_events()).unwrap();
    }
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let progress = handle.get_head_info().unwrap().sequence.unwrap();
    assert_eq!(progress.current_step, 2);
    handle
        .resolve_whole_file("a.txt".into(), git_core::merge::ConflictSide::Theirs)
        .unwrap();
    assert_eq!(
        handle.continue_sequence(false, mpsc::channel().0).unwrap(),
        git_core::pick::SequenceOutcome::Completed {
            applied: 3,
            skipped: vec![]
        }
    );
    assert!(!sequence_file(&repo).exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "feature\n"
    );
}

#[test]
fn a_sequence_stopped_by_an_error_blocks_undo_and_offers_continue() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    // c.txt untracked in the way of step three.
    write_file(dir.path(), "c.txt", "mine\n");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    // Step two conflicts first; take theirs and continue into step three's error.
    handle.cherry_pick_many(ids, no_events()).unwrap();
    handle
        .resolve_whole_file("a.txt".into(), git_core::merge::ConflictSide::Theirs)
        .unwrap();
    assert!(handle.continue_sequence(false, mpsc::channel().0).is_err());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    let progress = handle.get_head_info().unwrap().sequence.unwrap();
    assert_eq!(progress.current_step, 3);
    assert_eq!(handle.undo().unwrap(), UndoOutcome::OperationInProgress);
    std::fs::remove_file(dir.path().join("c.txt")).unwrap();
    assert!(matches!(
        handle.continue_sequence(false, mpsc::channel().0).unwrap(),
        git_core::pick::SequenceOutcome::Completed { applied: 3, .. }
    ));
    assert_eq!(
        handle.get_undo_state().unwrap().undo,
        Some("Cherry-pick 3 commits".into())
    );
}

#[test]
fn a_corrupt_sequence_file_never_blocks_the_repository() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    std::fs::create_dir_all(repo.path().join("browsitory")).unwrap();
    std::fs::write(sequence_file(&repo), "{not json").unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert_eq!(handle.get_head_info().unwrap().sequence, None);
    assert!(matches!(
        handle
            .cherry_pick(ids[0].clone(), None, no_events())
            .unwrap(),
        git_core::pick::PickOutcome::Committed { .. }
    ));
    assert!(matches!(handle.undo().unwrap(), UndoOutcome::Done { .. }));
}
