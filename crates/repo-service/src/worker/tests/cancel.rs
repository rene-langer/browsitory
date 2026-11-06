//! The cancel registry keeps only operations that are still to finish (#219).

use super::super::WorkerHandle;
use super::*;

/// Waits until the worker is done with every command sent before: it answers a command before
/// it is done with it, so the registry is only settled once a later command has run.
fn settle(handle: &WorkerHandle) {
    handle.list_remotes().expect("list remotes");
}

/// Sends `command` (built around a fresh `prefix` ID) and waits for its reply, as the handle
/// method would, but returns the ID so a test can click Cancel late.
fn run_with_id<T>(
    handle: &WorkerHandle,
    prefix: &str,
    command: impl FnOnce(super::super::cancel::OperationId, mpsc::Sender<T>) -> Command,
) -> (String, T) {
    let operation_id = handle.cancelled.allocate(prefix);
    let id = operation_id.as_str().to_string();
    let (reply, reply_rx) = mpsc::channel();
    handle
        .tx
        .send(command(operation_id, reply))
        .expect("queue command");
    (id, reply_rx.recv().expect("reply"))
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
fn late_cancels_for_finished_commits_and_switches_leave_the_registry_empty() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("topic", &head, false).unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();

    write_file(dir.path(), "file.txt", "v2");
    handle.stage_file("file.txt".into()).expect("stage");
    let (commit_id, committed) =
        run_with_id(&handle, "commit", |operation_id, reply| Command::Commit {
            message: "second".into(),
            no_verify: false,
            trailers: Vec::new(),
            operation_id,
            events: no_events(),
            reply,
        });
    committed.expect("commit");
    let (switch_id, switched) = run_with_id(&handle, "switch", |operation_id, reply| {
        Command::SwitchBranch {
            name: "topic".into(),
            operation_id,
            events: no_events(),
            reply,
        }
    });
    switched.expect("switch");
    settle(&handle);

    handle.cancel_transfer(&commit_id);
    handle.cancel_transfer(&switch_id);

    assert_eq!(handle.cancelled.len(), 0);
}

#[test]
fn late_cancels_for_commands_refused_during_a_sequence_leave_the_registry_empty() {
    let (dir, _repo, ids) = side_commits_with_a_conflict();
    let worker = Worker::spawn(dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    assert!(matches!(
        handle.cherry_pick_many(ids.clone(), no_events()).unwrap(),
        git_core::pick::SequenceOutcome::Conflicted { .. }
    ));

    // Refused by the gate before any operation starts.
    let (pick_id, picked) =
        run_with_id(&handle, "pick", |operation_id, reply| Command::CherryPick {
            commit_id: ids[0].clone(),
            mainline: None,
            operation_id,
            events: no_events(),
            reply,
        });
    assert!(picked.is_err());
    let (commit_id, committed) =
        run_with_id(&handle, "commit", |operation_id, reply| Command::Commit {
            message: "refused".into(),
            no_verify: false,
            trailers: Vec::new(),
            operation_id,
            events: no_events(),
            reply,
        });
    assert!(committed.is_err());
    settle(&handle);

    handle.cancel_transfer(&pick_id);
    handle.cancel_transfer(&commit_id);

    assert_eq!(handle.cancelled.len(), 0);
}

#[test]
fn commands_a_crashed_worker_never_ran_leave_the_registry_empty() {
    let (dir, _repo) = init_repo();
    let worker = Worker::spawn(dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    let queued = handle.cancelled.allocate("fetch");
    let queued_id = queued.as_str().to_string();

    // Queued behind the panic: dropped with the channel when the worker thread unwinds.
    handle.crash_for_test();
    let (reply, _reply_rx) = mpsc::channel();
    let _ = handle.tx.send(Command::FetchRemote {
        remote_name: "origin".into(),
        prune: None,
        operation_id: queued,
        events: mpsc::channel().0,
        reply,
    });
    while handle.is_alive() {
        thread::sleep(Duration::from_millis(5));
    }
    // Sent to the dead worker: the failed send hands the command back, and it is dropped.
    assert!(handle
        .commit("never".into(), false, Vec::new(), no_events())
        .is_err());

    handle.cancel_transfer(&queued_id);
    assert_eq!(handle.cancelled.len(), 0);
}

#[test]
fn an_early_cancel_still_stops_the_operation_once_it_starts() {
    let registry = super::super::CancelRegistry::default();
    let operation_id = registry.allocate("fetch");

    // The cancel arrives while the command is still queued.
    registry.cancel(operation_id.as_str());
    let op = super::super::remote::TransferOp::new(operation_id);

    assert!(op.cancel_check()());
    drop(op);
    assert_eq!(registry.len(), 0);
}

#[test]
fn a_late_cancel_leaves_other_operations_alone() {
    let registry = super::super::CancelRegistry::default();
    let running = registry.allocate("push");
    let queued = registry.allocate("fetch");
    let finished = registry.allocate("pull");
    let finished_id = finished.as_str().to_string();
    drop(finished);

    registry.cancel(running.as_str());
    registry.cancel(&finished_id);

    assert!(registry.is_cancelled(running.as_str()));
    assert!(!registry.is_cancelled(queued.as_str()));
    assert!(!registry.is_cancelled(&finished_id));
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
