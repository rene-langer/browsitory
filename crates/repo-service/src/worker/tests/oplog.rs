//! The operation log the worker keeps (#887).

use super::*;
use crate::oplog::{OperationEntry, OperationOutcome, Rerun};
use crate::wire::ErrorKind;

/// The log, newest first. A worker command first, so a command sent before has ended.
fn log(handle: &super::super::WorkerHandle) -> Vec<OperationEntry> {
    handle.get_status().unwrap();
    handle.operation_log(None, 100).entries
}

fn named<'a>(entries: &'a [OperationEntry], name: &str) -> &'a OperationEntry {
    entries
        .iter()
        .find(|entry| entry.name == name)
        .unwrap_or_else(|| panic!("no {name} entry in {entries:#?}"))
}

#[test]
fn changes_are_logged_newest_first_and_reads_are_not() {
    let (dir, _repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    handle.stage_file("a.txt".into()).unwrap();
    handle.get_status().unwrap();
    handle.list_branches().unwrap();
    handle
        .commit("first commit".into(), false, Vec::new(), no_events())
        .unwrap();

    let entries = log(&handle);
    let summary: Vec<_> = entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.target.as_deref(), entry.outcome))
        .collect();
    assert_eq!(
        summary,
        [
            ("Commit", Some("first commit"), OperationOutcome::Succeeded),
            ("Stage", Some("a.txt"), OperationOutcome::Succeeded),
        ]
    );
    assert!(entries[0].id > entries[1].id);
    assert!(entries.iter().all(|entry| entry.duration_ms.is_some()));
    assert!(entries.iter().all(|entry| entry.rerun.is_none()));
}

#[test]
fn a_failed_command_is_logged_with_its_error() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "base");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let error = handle.delete_branch("missing".into(), false).unwrap_err();

    let entries = log(&handle);
    let entry = named(&entries, "Delete branch");
    assert_eq!(entry.target.as_deref(), Some("missing"));
    assert_eq!(entry.outcome, OperationOutcome::Failed);
    assert_eq!(entry.message.as_deref(), Some(error.message.as_str()));
    assert_eq!(entry.error_kind, Some(error.kind));
}

#[test]
fn a_command_the_paused_sequence_refuses_is_logged_as_failed() {
    let (_dir, _repo, _worker, handle, _branch) = paused_sequence();

    let error = handle
        .switch_branch("other".into(), no_events())
        .unwrap_err();

    let entries = log(&handle);
    let entry = named(&entries, "Switch branch");
    assert_eq!(entry.outcome, OperationOutcome::Failed);
    assert_eq!(entry.message.as_deref(), Some(error.message.as_str()));
    // The sequence that stopped on its conflict is logged as stopped.
    let pick = named(&entries, "Cherry-pick");
    assert_eq!(pick.outcome, OperationOutcome::Stopped);
}

#[test]
fn a_merge_that_stops_on_conflicts_is_logged_as_stopped() {
    let (dir, _repo) = conflicting_branches();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let outcome = handle
        .start_merge(
            "feature".into(),
            git_core::merge::MergeOptions {
                fast_forward: git_core::merge::FastForward::Allow,
                squash: false,
            },
            no_events(),
        )
        .unwrap();
    assert!(matches!(outcome, MergeOutcome::Conflicted { .. }));

    let entries = log(&handle);
    let entry = named(&entries, "Merge");
    assert_eq!(entry.target.as_deref(), Some("feature"));
    assert_eq!(entry.outcome, OperationOutcome::Stopped);
    assert_eq!(
        entry.message.as_deref(),
        Some("Stopped on conflicts in 1 file.")
    );
}

#[test]
fn a_remote_url_is_logged_without_its_credentials() {
    let (dir, _repo) = init_repo();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    // The worker refuses such a URL; the refused command is logged with its parameters.
    let refused = handle
        .add_remote(
            "upstream".into(),
            "https://alice:s3cret@example.test/r.git".into(),
            Some("https://token@example.test/push.git".into()),
        )
        .unwrap_err();
    handle
        .save_https_credential("upstream".into(), "alice".into(), "tok3n".into())
        .ok();

    let entries = log(&handle);
    let text = format!("{entries:?}");
    assert!(!text.contains("s3cret"), "{text}");
    assert!(!text.contains("token@"), "{text}");
    assert!(!text.contains("tok3n"), "{text}");
    let add = named(&entries, "Add remote");
    assert_eq!(add.target.as_deref(), Some("upstream"));
    assert_eq!(add.outcome, OperationOutcome::Failed);
    assert_eq!(add.message.as_deref(), Some(refused.message.as_str()));
    let values: Vec<_> = add.params.iter().map(|p| p.value.as_str()).collect();
    assert!(
        values.contains(&"https://***@example.test/r.git"),
        "{values:?}"
    );
    assert!(
        values.contains(&"https://***@example.test/push.git"),
        "{values:?}"
    );
}

#[test]
fn a_fetch_is_logged_with_how_to_run_it_again() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let worker = Worker::spawn(local_dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let (event_tx, event_rx) = mpsc::channel();
    handle
        .fetch_remote("origin".into(), Some(true), event_tx)
        .unwrap();
    let _: Vec<_> = event_rx.iter().collect();

    let entries = log(&handle);
    let entry = named(&entries, "Fetch");
    assert_eq!(entry.target.as_deref(), Some("origin"));
    assert_eq!(entry.outcome, OperationOutcome::Succeeded);
    assert_eq!(
        entry.rerun,
        Some(Rerun::Fetch {
            remote_name: "origin".into(),
            prune: Some(true),
        })
    );
}

#[test]
fn a_failed_fetch_is_logged_with_its_transfer_error_and_what_went_wrong() {
    let (local_dir, _repo) = init_repo();
    let worker = Worker::spawn(local_dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let (event_tx, event_rx) = mpsc::channel();
    handle
        .fetch_remote("missing-remote".into(), None, event_tx)
        .unwrap();
    let _: Vec<_> = event_rx.iter().collect();

    let entries = log(&handle);
    let entry = named(&entries, "Fetch");
    assert_eq!(entry.outcome, OperationOutcome::Failed);
    assert_eq!(
        entry.transfer_error,
        Some(TransferErrorKind::TransferFailed)
    );
    let message = entry.message.as_deref().unwrap_or_default();
    assert!(message.contains("missing-remote"), "{message}");
}

#[test]
fn a_push_refused_before_it_starts_is_logged_as_failed() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "base");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let (event_tx, event_rx) = mpsc::channel();

    // No remote at all: refused before any transfer starts.
    let result = handle.push_current_branch("nowhere".into(), false, event_tx);
    // A transfer that did start ends before its events do (reads still run while it waits).
    let _: Vec<_> = event_rx.iter().collect();

    let entries = log(&handle);
    let entry = named(&entries, "Push");
    match result {
        Err(error) => {
            assert_eq!(entry.outcome, OperationOutcome::Failed);
            assert_eq!(entry.message.as_deref(), Some(error.message.as_str()));
        }
        Ok(_) => assert_eq!(entry.outcome, OperationOutcome::Failed, "{entry:#?}"),
    }
}

#[test]
fn the_log_outlives_the_worker_and_names_the_repository_key() {
    let (dir, _repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.stage_file("a.txt".into()).unwrap();
    let (worker, handle) = restart(worker, handle, dir.path());

    handle.get_status().unwrap();
    let page = handle.operation_log(None, 100);
    assert_eq!(page.repo, super::super::repo_key(dir.path()).unwrap());
    assert_eq!(page.entries.len(), 1);
    assert_eq!(page.entries[0].name, "Stage");
    drop(worker);
}

#[test]
fn every_spelling_of_a_repository_shares_one_log() {
    let (dir, _repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    let from_root = Worker::spawn(dir.path().to_path_buf()).unwrap();
    from_root.handle().stage_file("a.txt".into()).unwrap();
    drop(from_root);

    let from_sub = Worker::spawn(dir.path().join("sub")).unwrap();
    let entries = log(&from_sub.handle());
    assert_eq!(entries.len(), 1);
}

#[cfg(unix)]
#[test]
fn a_failing_pre_commit_is_logged_as_failed_with_the_hooks_run() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    install_hook(&repo, "pre-commit", "echo nope; exit 1");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.stage_file("a.txt".into()).unwrap();

    handle
        .commit("msg".into(), false, Vec::new(), no_events())
        .unwrap();

    let entries = log(&handle);
    let entry = named(&entries, "Commit");
    assert_eq!(entry.outcome, OperationOutcome::Failed);
    assert_eq!(
        entry.message.as_deref(),
        Some("The pre-commit hook failed.")
    );
    assert_eq!(entry.runs.len(), 1, "{:#?}", entry.runs);
    assert!(entry.runs[0].program.ends_with("pre-commit"));
    assert_eq!(entry.runs[0].output, "nope\n");
    assert_eq!(
        entry.runs[0].status,
        git_core::process::ProgramRunStatus::Failed
    );
}

#[cfg(unix)]
#[test]
fn a_cancelled_hook_logs_its_command_as_cancelled_and_the_log_shows_it_running_first() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    install_hook(&repo, "pre-commit", "sleep 30");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.stage_file("a.txt".into()).unwrap();
    let (events_tx, events_rx) = mpsc::channel();
    let watcher = handle.clone();
    let cancel_thread = std::thread::spawn(move || {
        if let Ok(HookEvent::Started { operation_id, .. }) = events_rx.recv() {
            // The log is read without waiting for the worker, which is busy with the commit.
            let running = watcher.operation_log(None, 1).entries;
            watcher.cancel_transfer(&operation_id);
            return running;
        }
        Vec::new()
    });

    let result = handle.commit("msg".into(), false, Vec::new(), events_tx);

    let running = cancel_thread.join().unwrap();
    assert!(result.is_err());
    assert_eq!(running.len(), 1);
    assert_eq!(running[0].name, "Commit");
    assert_eq!(running[0].outcome, OperationOutcome::Running);
    let entries = log(&handle);
    let entry = named(&entries, "Commit");
    assert_eq!(entry.id, running[0].id);
    assert_eq!(entry.outcome, OperationOutcome::Cancelled);
    assert_eq!(
        entry.runs[0].status,
        git_core::process::ProgramRunStatus::Cancelled
    );
}

#[cfg(unix)]
#[test]
fn a_failing_pre_push_is_logged_with_its_transfer_error_and_the_hooks_output() {
    let (_source_dir, remote_dir, local_dir) = local_and_bare_remote();
    let local = Repository::open(local_dir.path()).unwrap();
    write_file(local_dir.path(), "new.txt", "new");
    commit_all(&local, "to push");
    let branch = local.head().unwrap().shorthand().unwrap().to_string();
    install_hook(&local, "pre-push", "echo 'tests failed'; exit 1");
    drop(local);
    let worker = Worker::spawn(local_dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let (event_tx, event_rx) = mpsc::channel();
    handle
        .push_branch(
            "origin".into(),
            branch.clone(),
            "topic".into(),
            true,
            false,
            event_tx,
        )
        .unwrap();
    let _: Vec<_> = event_rx.iter().collect();

    let entries = log(&handle);
    let entry = named(&entries, "Push");
    assert_eq!(entry.outcome, OperationOutcome::Failed);
    assert_eq!(entry.transfer_error, Some(TransferErrorKind::HookFailed));
    assert_eq!(entry.runs.len(), 1, "{:#?}", entry.runs);
    let run = &entry.runs[0];
    assert!(run.program.ends_with("pre-push"));
    assert_eq!(
        run.args,
        [
            "origin".to_string(),
            remote_dir.path().to_str().unwrap().to_string()
        ]
    );
    assert_eq!(run.output, "tests failed\n");
    assert_eq!(
        entry.rerun,
        Some(Rerun::PushBranch {
            remote_name: "origin".into(),
            local_branch: branch,
            remote_branch: "topic".into(),
            set_upstream: true,
            no_verify: false,
        })
    );
}

#[test]
fn a_worker_error_kind_is_kept() {
    let (dir, _repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.stage_file("a.txt".into()).unwrap();
    // No identity: the commit is refused with its own kind.
    let repo = Repository::open(dir.path()).unwrap();
    let mut config = repo.config().unwrap();
    config.remove("user.name").unwrap();
    config.remove("user.email").unwrap();

    let error = handle
        .commit("msg".into(), false, Vec::new(), no_events())
        .unwrap_err();

    let entries = log(&handle);
    let entry = named(&entries, "Commit");
    assert_eq!(entry.outcome, OperationOutcome::Failed);
    assert_eq!(entry.error_kind, Some(error.kind));
    assert_eq!(error.kind, ErrorKind::MissingIdentity);
}
