//! #734: reads answer while the worker waits for a fetch, pull or push, and everything else still
//! waits for it.
//!
//! No test waits for "nothing happened within N ms": a read must answer while the transfer can't
//! have ended (a silent remote, a hook waiting for the test), and a command that must wait proves
//! it by its reply arriving only after the transfer's `Completed` was sent.
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use git2::Repository;
use git_core::remote::{TransferErrorKind, TransferOperation};

use super::remote::{background_options, repo_with_a_silent_remote};
use super::{TransferEvent, Worker};
use crate::worker::WorkerHandle;

/// Runs `read` on its own thread and returns its result, or fails the test when it doesn't
/// answer in time: then it waited behind the transfer.
fn answers<T: Send + 'static>(what: &str, read: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(read());
    });
    rx.recv_timeout(Duration::from_secs(20))
        .unwrap_or_else(|_| panic!("{what} waited behind the transfer"))
}

fn is_completed(event: &TransferEvent) -> bool {
    matches!(event, TransferEvent::Completed { .. })
}

/// Waits for the transfer's `Completed` and returns it as (operation, error).
fn completion(
    event_rx: &mpsc::Receiver<TransferEvent>,
) -> (TransferOperation, Option<TransferErrorKind>) {
    loop {
        match event_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("the transfer completes")
        {
            TransferEvent::Completed {
                operation, error, ..
            } => return (operation, error),
            _ => continue,
        }
    }
}

/// Every read a refresh makes, plus a commit's files: each answers.
fn every_refresh_read_answers(handle: &WorkerHandle, commit_id: &str) {
    let h = handle.clone();
    answers("GetChangeStamp", move || h.get_change_stamp()).expect("stamp");
    let h = handle.clone();
    answers("GetStatus", move || h.get_status()).expect("status");
    let h = handle.clone();
    answers("GetStatusWithStamp", move || h.get_status_with_stamp()).expect("status and stamp");
    let h = handle.clone();
    answers("GetCommitGraph", move || {
        h.get_commit_graph(50, git_core::graph::GraphSelection::default(), None)
    })
    .expect("graph");
    let h = handle.clone();
    answers("ListBranches", move || h.list_branches()).expect("branches");
    let h = handle.clone();
    answers("ListStashes", move || h.list_stashes()).expect("stashes");
    let h = handle.clone();
    answers("GetHeadInfo", move || h.get_head_info()).expect("head info");
    let h = handle.clone();
    answers("GetUndoState", move || h.get_undo_state()).expect("undo state");
    let h = handle.clone();
    let id = commit_id.to_string();
    answers("GetCommitFiles", move || h.get_commit_files(id)).expect("commit files");
}

fn head_id(path: &std::path::Path) -> String {
    Repository::open(path)
        .expect("open")
        .head()
        .expect("head")
        .target()
        .expect("head commit")
        .to_string()
}

#[test]
fn reads_answer_while_a_fetch_is_still_connecting() {
    let (_source_dir, _remote_dir, local_dir, server, _branch) = repo_with_a_silent_remote();
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    let (event_tx, event_rx) = mpsc::channel();

    let operation_id = handle
        .fetch_remote("silent".into(), None, event_tx)
        .expect("start fetch");
    server
        .accepted
        .recv_timeout(Duration::from_secs(30))
        .expect("the fetch connects to the silent server");

    every_refresh_read_answers(&handle, &head_id(local_dir.path()));
    // The silent remote never answers: the fetch is still running.
    assert!(!event_rx.try_iter().any(|event| is_completed(&event)));

    handle.cancel_transfer(&operation_id);
    assert_eq!(
        completion(&event_rx),
        (TransferOperation::Fetch, Some(TransferErrorKind::Cancelled))
    );
}

#[test]
fn a_write_sent_during_a_fetch_waits_for_it_while_a_later_read_answers() {
    let (_source_dir, _remote_dir, local_dir, server, _branch) = repo_with_a_silent_remote();
    std::fs::write(local_dir.path().join("new.txt"), "new\n").expect("write file");
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    let (event_tx, event_rx) = mpsc::channel();

    let operation_id = handle
        .fetch_remote("silent".into(), None, event_tx)
        .expect("start fetch");
    server
        .accepted
        .recv_timeout(Duration::from_secs(30))
        .expect("the fetch connects to the silent server");
    let (staged_tx, staged_rx) = mpsc::channel();
    {
        let handle = handle.clone();
        thread::spawn(move || {
            let _ = staged_tx.send(handle.stage_file("new.txt".into()));
        });
    }

    // Sent after the stage (which waits for the fetch), it answers first, and sees the file
    // still unstaged.
    let h = handle.clone();
    let status = answers("GetStatus", move || h.get_status()).expect("status");
    let new = status
        .iter()
        .find(|entry| entry.path == "new.txt")
        .expect("new.txt in status");
    assert!(!new.staged, "{new:?}");
    assert!(
        staged_rx.try_recv().is_err(),
        "the stage ran during the fetch"
    );

    handle.cancel_transfer(&operation_id);
    staged_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("the stage answers once the fetch ended")
        .expect("stage");
    // The fetch's `Completed` was sent before the stage ran.
    assert!(event_rx.try_iter().any(|event| is_completed(&event)));
}

#[test]
fn reads_answer_while_a_pull_is_fetching() {
    let (_source_dir, _remote_dir, local_dir, server, branch) = repo_with_a_silent_remote();
    Repository::open(local_dir.path())
        .expect("open local")
        .config()
        .expect("config")
        .set_str(&format!("branch.{branch}.remote"), "silent")
        .expect("upstream on the silent remote");
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    let (event_tx, event_rx) = mpsc::channel();

    let pulling = {
        let handle = handle.clone();
        thread::spawn(move || handle.pull_current_upstream(event_tx))
    };
    let operation_id = match event_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("the pull starts")
    {
        TransferEvent::Started { operation_id, .. } => operation_id,
        other => panic!("expected Started first, got {other:?}"),
    };
    server
        .accepted
        .recv_timeout(Duration::from_secs(30))
        .expect("the pull's fetch connects to the silent server");

    every_refresh_read_answers(&handle, &head_id(local_dir.path()));

    handle.cancel_transfer(&operation_id);
    assert_eq!(
        completion(&event_rx),
        (TransferOperation::Pull, Some(TransferErrorKind::Cancelled))
    );
    pulling
        .join()
        .expect("pull thread")
        .expect_err("a cancelled pull fails");
}

#[test]
fn reads_answer_during_a_background_fetch() {
    let (_source_dir, _remote_dir, local_dir, server, _branch) = repo_with_a_silent_remote();
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    let (event_tx, event_rx) = mpsc::channel();

    let operation_id = handle
        .background_fetch_with(background_options(Duration::from_secs(600)), event_tx)
        .expect("start background fetch")
        .expect("an idle worker starts the background fetch");
    server
        .accepted
        .recv_timeout(Duration::from_secs(30))
        .expect("the background fetch connects to the silent server");

    let h = handle.clone();
    answers("GetStatus", move || h.get_status()).expect("status");
    let h = handle.clone();
    answers("GetChangeStamp", move || h.get_change_stamp()).expect("stamp");

    handle.cancel_transfer(&operation_id);
    assert_eq!(
        completion(&event_rx),
        (
            TransferOperation::BackgroundFetch,
            Some(TransferErrorKind::Cancelled)
        )
    );
}

/// A hook that says it started (`<name>.started` in the git folder) and waits until the test
/// creates `<name>.release`.
#[cfg(unix)]
fn waiting_hook(repo: &Repository, name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let started = repo.path().join(format!("{name}.started"));
    let release = repo.path().join(format!("{name}.release"));
    super::install_hook(
        repo,
        name,
        &format!(
            "touch '{}'\nwhile [ ! -f '{}' ]; do sleep 0.05; done",
            started.display(),
            release.display()
        ),
    );
    (started, release)
}

#[cfg(unix)]
fn wait_for_file(path: &std::path::Path) {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "{} never appeared",
            path.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

/// A push's `pre-push` hook runs on the transfer helper: reads answer while it runs.
#[cfg(unix)]
#[test]
fn reads_answer_while_a_pre_push_hook_runs() {
    let (_source_dir, _remote_dir, local_dir, _server, _branch) = repo_with_a_silent_remote();
    let local = Repository::open(local_dir.path()).expect("open local");
    super::write_file(local_dir.path(), "local.txt", "local\n");
    super::commit_all(&local, "local commit");
    let (started, release) = waiting_hook(&local, "pre-push");
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    let (event_tx, event_rx) = mpsc::channel();

    handle
        .push_current_branch("origin".into(), false, event_tx)
        .expect("start push");
    wait_for_file(&started);

    every_refresh_read_answers(&handle, &head_id(local_dir.path()));
    assert!(!event_rx.try_iter().any(|event| is_completed(&event)));

    std::fs::write(&release, "").expect("release the hook");
    assert_eq!(completion(&event_rx), (TransferOperation::PushBranch, None));
}

/// A pull's merge and its `post-merge` hook run on the worker, which writes the index and work
/// tree then: a read sent meanwhile answers only after the pull ended.
#[cfg(unix)]
#[test]
fn reads_wait_for_a_pulls_merge_and_post_merge_hook() {
    let (source_dir, _remote_dir, local_dir, _server, _branch) = repo_with_a_silent_remote();
    let source = Repository::open(source_dir.path()).expect("open source");
    super::write_file(source_dir.path(), "upstream.txt", "upstream\n");
    super::commit_all(&source, "upstream commit");
    let head = source.head().expect("source head");
    let refspec = format!("{0}:{0}", head.name().expect("branch ref"));
    source
        .find_remote("origin")
        .expect("source origin")
        .push(&[refspec], None)
        .expect("push upstream commit");
    let local = Repository::open(local_dir.path()).expect("open local");
    let (started, release) = waiting_hook(&local, "post-merge");
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    let (event_tx, event_rx) = mpsc::channel();

    let pulling = {
        let handle = handle.clone();
        thread::spawn(move || handle.pull_current_upstream(event_tx))
    };
    wait_for_file(&started);
    let (status_tx, status_rx) = mpsc::channel();
    {
        let handle = handle.clone();
        thread::spawn(move || {
            let _ = status_tx.send(handle.get_status());
        });
    }

    std::fs::write(&release, "").expect("release the hook");
    status_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("the read answers after the pull")
        .expect("status");
    // The pull's `Completed` was sent before the read ran.
    assert!(event_rx.try_iter().any(|event| is_completed(&event)));
    pulling.join().expect("pull thread").expect("pull");
}
