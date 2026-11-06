//! The cancel registry keeps only operations that are still to finish (#219).

use super::*;

/// Waits until the worker is done with every command sent before: it answers a command before
/// it is done with it, so the registry is only settled once a later command has run.
fn settle(handle: &super::super::WorkerHandle) {
    handle.list_remotes().expect("list remotes");
}

#[test]
fn a_cancel_for_a_finished_transfer_leaves_the_registry_empty() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    let (event_tx, event_rx) = mpsc::channel();

    let operation_id = handle
        .fetch_remote("origin".into(), None, event_tx)
        .expect("fetch");
    // All events, so the fetch has completed, and the worker is done with its command.
    let events: Vec<_> = event_rx.iter().collect();
    assert!(matches!(
        events.last(),
        Some(TransferEvent::Completed { error: None, .. })
    ));
    settle(&handle);

    // A late click on Cancel, after the fetch finished.
    handle.cancel_transfer(&operation_id);

    assert_eq!(handle.cancelled.len(), 0);
}

#[test]
fn finished_commands_of_every_kind_leave_the_registry_empty() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("topic", &head, false).unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();

    write_file(dir.path(), "file.txt", "v2");
    handle.stage_file("file.txt".into()).expect("stage");
    handle
        .commit("second".into(), false, Vec::new(), no_events())
        .expect("commit");
    handle
        .switch_branch("topic".into(), no_events())
        .expect("switch");
    settle(&handle);

    assert_eq!(handle.cancelled.len(), 0);
}

#[test]
fn a_command_refused_during_a_sequence_leaves_the_registry_empty() {
    let (dir, _repo, ids) = side_commits_with_a_conflict();
    let worker = Worker::spawn(dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    assert!(matches!(
        handle.cherry_pick_many(ids.clone(), no_events()).unwrap(),
        git_core::pick::SequenceOutcome::Conflicted { .. }
    ));

    // Refused by the gate before any operation starts.
    assert!(handle
        .cherry_pick(ids[0].clone(), None, no_events())
        .is_err());
    assert!(handle
        .commit("refused".into(), false, Vec::new(), no_events())
        .is_err());
    settle(&handle);

    assert_eq!(handle.cancelled.len(), 0);
}

#[test]
fn an_early_cancel_still_stops_the_operation_once_it_starts() {
    let registry = super::super::CancelRegistry::default();
    let id = registry.allocate("fetch");

    // The cancel arrives while the command is still queued.
    registry.cancel(&id);
    let done = registry.done_on_drop(&id);
    let op = super::super::remote::TransferOp::new(id.clone(), &registry);

    assert!(op.cancel_check()());
    drop(op);
    drop(done);
    assert_eq!(registry.len(), 0);
}

#[test]
fn a_late_cancel_leaves_other_operations_alone() {
    let registry = super::super::CancelRegistry::default();
    let running = registry.allocate("push");
    let queued = registry.allocate("fetch");
    let finished = registry.allocate("pull");
    drop(registry.done_on_drop(&finished));

    registry.cancel(&running);
    registry.cancel(&finished);

    assert!(registry.is_cancelled(&running));
    assert!(!registry.is_cancelled(&queued));
    assert!(!registry.is_cancelled(&finished));
    assert_eq!(registry.len(), 2);
}

#[test]
fn a_cancel_for_an_id_the_worker_never_handed_out_is_dropped() {
    let (dir, _repo) = init_repo();
    let worker = Worker::spawn(dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();

    handle.cancel_transfer("fetch-999999");

    assert_eq!(handle.cancelled.len(), 0);
}
