use super::*;

#[test]
fn remote_and_current_upstream_round_trip_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    repo.remote("origin", "https://example.com/owner/repo.git")
        .unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("topic", &head, false).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("branch.topic.remote", "origin").unwrap();
    config
        .set_str("branch.topic.merge", "refs/heads/topic")
        .unwrap();
    // Repo-local, so a developer's global `fetch.prune` cannot leak into `prune` below.
    config.set_bool("fetch.prune", false).unwrap();
    // Makes `push_branch` independent of whether the init default branch is main or master.
    config.set_str("push.default", "upstream").unwrap();
    drop(config);

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle
        .set_current_upstream("origin".into(), "main".into())
        .unwrap();

    assert_eq!(
        handle.list_remotes().unwrap(),
        vec![git_core::remote::RemoteInfo {
            name: "origin".into(),
            fetch_url: "https://example.com/owner/repo.git".into(),
            push_url: None,
            prune: false,
        }]
    );
    assert_eq!(
        handle.get_current_upstream().unwrap(),
        Some(git_core::remote::UpstreamInfo {
            local_branch: repo.head().unwrap().shorthand().unwrap().into(),
            remote_name: "origin".into(),
            remote_branch: "main".into(),
            ahead: None,
            behind: None,
            push_branch: Some("main".into()),
        })
    );

    let mut affected_branches: Vec<_> = handle
        .get_remote_upstreams("origin".into())
        .unwrap()
        .into_iter()
        .map(|upstream| upstream.local_branch)
        .collect();
    affected_branches.sort();
    let mut expected_branches = vec![
        repo.head().unwrap().shorthand().unwrap().to_string(),
        "topic".to_string(),
    ];
    expected_branches.sort();
    assert_eq!(affected_branches, expected_branches);

    handle.remove_remote("origin".into(), true).unwrap();
    assert!(handle.list_remotes().unwrap().is_empty());
    assert_eq!(handle.get_current_upstream().unwrap(), None);
    assert!(handle
        .get_remote_upstreams("origin".into())
        .unwrap()
        .is_empty());
}

#[test]
fn transfer_fetch_streams_ordered_owned_events() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let (event_tx, event_rx) = mpsc::channel();

    let operation_id = worker
        .handle()
        .fetch_remote("origin".into(), None, event_tx)
        .expect("start fetch");
    let events: Vec<_> = event_rx.iter().collect();

    assert!(matches!(
        events.first(),
        Some(TransferEvent::Started {
            operation_id: id,
            operation: TransferOperation::Fetch,
        }) if id == &operation_id
    ));
    assert!(events.iter().any(|event| matches!(
        event,
        TransferEvent::Progress(progress) if progress.operation_id == operation_id
    )));
    assert!(matches!(
        events.last(),
        Some(TransferEvent::Completed {
            operation_id: id,
            operation: TransferOperation::Fetch,
            error: None,
            hook_output: None,
        }) if id == &operation_id
    ));
}

#[test]
fn transfer_fetch_failure_streams_only_a_sanitized_terminal_error() {
    let (local_dir, _repo) = init_repo();
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let (event_tx, event_rx) = mpsc::channel();

    let operation_id = worker
        .handle()
        .fetch_remote("missing-remote".into(), None, event_tx)
        .expect("start fetch");
    let events: Vec<_> = event_rx.iter().collect();

    assert!(matches!(
        events.last(),
        Some(TransferEvent::Completed {
            operation_id: id,
            operation: TransferOperation::Fetch,
            error: Some(TransferErrorKind::TransferFailed),
            hook_output: None,
        }) if id == &operation_id
    ));
}

#[test]
fn cancelling_a_transfer_by_operation_id_surfaces_a_cancelled_completion_event() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    let (event_tx, event_rx) = mpsc::channel();
    let (reply_tx, reply_rx) = mpsc::channel();

    // The operation is marked cancelled *before* the worker dequeues it. A fetch from a
    // local bare remote finishes in microseconds, so cancelling after `fetch_remote` returned
    // would race the transfer and make this test flaky; pre-marking is well-defined precisely
    // because `cancel_transfer` writes to the shared registry rather than sending a command,
    // and the registry is what the `git2` progress callbacks poll. The ID comes from the
    // registry, as `fetch_remote` takes it, so the registry keeps this early cancel (#219).
    let operation = handle.cancelled.allocate("fetch");
    let operation_id = operation.as_str().to_string();
    handle.cancel_transfer(&operation_id);
    handle
        .tx
        .send(Command::FetchRemote {
            remote_name: "origin".into(),
            prune: None,
            operation_id: operation,
            events: event_tx,
            reply: reply_tx,
        })
        .expect("queue fetch");
    let _ = reply_rx.recv();
    let events: Vec<_> = event_rx.iter().collect();

    assert!(matches!(
        events.last(),
        Some(TransferEvent::Completed {
            operation_id: id,
            operation: TransferOperation::Fetch,
            error: Some(TransferErrorKind::Cancelled),
            hook_output: None,
        }) if id == &operation_id
    ));
    // The cancelled transfer never created the tracking ref it would have on success.
    assert!(Repository::open(local_dir.path())
        .expect("open local repo")
        .find_reference("refs/remotes/origin/main")
        .is_err());
    // Completion clears the registry entry, so a long-lived worker's cancel set cannot grow
    // without bound.
    assert_eq!(handle.cancelled.len(), 0);
}

#[test]
fn cancel_transfer_reaches_a_worker_that_is_not_reading_its_command_channel() {
    // The whole point of the cancel registry: while a transfer is blocked inside `git2`'s
    // network I/O the worker thread is not reading `rx`, so a `Command`-based cancel would
    // queue behind the very operation it is meant to interrupt. A dead worker is the
    // strongest form of "not reading commands" a test can create — every `tx.send` fails,
    // yet the cancel must still land.
    let (dir, _repo) = init_repo();
    let worker = Worker::spawn(dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    // Handed out, as for a transfer queued before the worker stopped reading commands.
    let operation = handle.cancelled.allocate("fetch");
    handle.crash_for_test();
    while handle.is_alive() {
        thread::sleep(Duration::from_millis(5));
    }

    handle.cancel_transfer(operation.as_str());

    assert!(handle.cancelled.is_cancelled(operation.as_str()));
}

#[test]
fn lists_remote_tracking_branches_after_a_fetch() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    let (event_tx, event_rx) = mpsc::channel();
    handle
        .fetch_remote("origin".into(), None, event_tx)
        .expect("start fetch");
    let _events: Vec<_> = event_rx.iter().collect();

    let branches = handle
        .list_remote_branches("origin".into())
        .expect("list remote branches");

    assert_eq!(branches.len(), 1);
}

#[test]
fn non_fast_forward_branch_push_streams_a_safe_terminal_error_kind() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let local = Repository::open(local_dir.path()).expect("open local repo");
    write_file(local_dir.path(), "README.md", "unrelated local history\n");
    commit_all(&local, "unrelated local commit");
    drop(local);
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let (event_tx, event_rx) = mpsc::channel();

    let operation_id = worker
        .handle()
        .push_current_branch("origin".into(), false, event_tx)
        .expect("start branch push");
    let events: Vec<_> = event_rx.iter().collect();

    assert!(matches!(
        events.last(),
        Some(TransferEvent::Completed {
            operation_id: id,
            operation: TransferOperation::PushBranch,
            error: Some(TransferErrorKind::NonFastForward),
            hook_output: None,
        }) if id == &operation_id
    ));
}

#[test]
fn fetch_all_remotes_reports_each_unreachable_remote_and_completes() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let missing = TempDir::new().expect("missing dir");
    Repository::open(local_dir.path())
        .expect("open local")
        .remote(
            "broken",
            missing.path().join("nope").to_str().expect("path"),
        )
        .expect("add broken remote");
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let (event_tx, event_rx) = mpsc::channel();

    let operation_id = worker
        .handle()
        .fetch_all_remotes(None, event_tx)
        .expect("start fetch all");
    let events: Vec<_> = event_rx.iter().collect();

    assert!(operation_id.starts_with("fetch-all-"));
    assert!(matches!(
        events.first(),
        Some(TransferEvent::Started {
            operation: TransferOperation::FetchAll,
            ..
        })
    ));
    let failed: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            TransferEvent::RemoteFailed {
                operation_id: id,
                operation,
                remote_name,
                error,
            } => {
                assert_eq!(id, &operation_id);
                assert_eq!(*operation, TransferOperation::FetchAll);
                Some((remote_name.clone(), *error))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        failed,
        vec![("broken".to_string(), TransferErrorKind::TransferFailed)]
    );
    assert_eq!(completed(&events), (TransferOperation::FetchAll, None));
    let local = Repository::open(local_dir.path()).expect("reopen local");
    assert!(
        local
            .branches(Some(git2::BranchType::Remote))
            .expect("branches")
            .count()
            > 0
    );
}

#[test]
fn fetch_all_remotes_fails_when_every_remote_fails() {
    let (local_dir, repo) = init_repo();
    let missing = TempDir::new().expect("missing dir");
    repo.remote(
        "broken",
        missing.path().join("nope").to_str().expect("path"),
    )
    .expect("add broken remote");
    drop(repo);
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let (event_tx, event_rx) = mpsc::channel();

    worker
        .handle()
        .fetch_all_remotes(Some(true), event_tx)
        .expect("start fetch all");
    let events: Vec<_> = event_rx.iter().collect();

    assert_eq!(
        completed(&events),
        (
            TransferOperation::FetchAll,
            Some(TransferErrorKind::TransferFailed)
        )
    );
}

#[test]
fn fetch_all_remotes_without_remotes_completes() {
    let (local_dir, _repo) = init_repo();
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let (event_tx, event_rx) = mpsc::channel();

    worker
        .handle()
        .fetch_all_remotes(None, event_tx)
        .expect("start fetch all");
    let events: Vec<_> = event_rx.iter().collect();

    assert_eq!(completed(&events), (TransferOperation::FetchAll, None));
}

#[test]
fn push_branch_publishes_with_upstream_and_delete_remote_branch_removes_it() {
    let (_source_dir, remote_dir, local_dir) = local_and_bare_remote();
    let local = Repository::open(local_dir.path()).expect("open local");
    write_file(local_dir.path(), "local.txt", "local\n");
    commit_all(&local, "local root");
    let head = local
        .head()
        .expect("head")
        .peel_to_commit()
        .expect("commit");
    local.branch("topic", &head, false).expect("create topic");
    drop(head);
    drop(local);
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    let bare_has = |name: &str| {
        Repository::open_bare(remote_dir.path())
            .expect("open bare")
            .find_reference(&format!("refs/heads/{name}"))
            .is_ok()
    };
    assert!(bare_has("main") || bare_has("master"));

    let (event_tx, event_rx) = mpsc::channel();
    let push_id = handle
        .push_branch(
            "origin".into(),
            "topic".into(),
            "topic".into(),
            true,
            false,
            event_tx,
        )
        .expect("start push");
    let events: Vec<_> = event_rx.iter().collect();
    assert!(push_id.starts_with("push-"));
    assert_eq!(completed(&events), (TransferOperation::PushBranch, None));
    assert!(bare_has("topic"));
    let topic = handle
        .list_branches()
        .expect("list branches")
        .into_iter()
        .find(|branch| branch.name == "topic")
        .expect("topic listed");
    assert_eq!(topic.upstream.as_deref(), Some("origin/topic"));

    let (event_tx, event_rx) = mpsc::channel();
    let delete_id = handle
        .delete_remote_branch("origin".into(), "topic".into(), false, event_tx)
        .expect("start delete");
    let events: Vec<_> = event_rx.iter().collect();
    assert!(delete_id.starts_with("delete-remote-"));
    assert_eq!(
        completed(&events),
        (TransferOperation::DeleteRemoteBranch, None)
    );
    assert!(!bare_has("topic"));
    // Only that branch: the remote's default branch is untouched.
    assert!(bare_has("main") || bare_has("master"));
    assert!(handle.get_undo_state().expect("undo state").undo.is_none());
}

#[test]
fn create_tag_at_a_commit_and_delete_remote_tag_round_trip_through_the_worker() {
    let (_source_dir, remote_dir, local_dir) = local_and_bare_remote();
    let local = Repository::open(local_dir.path()).expect("open local");
    write_file(local_dir.path(), "older.txt", "older\n");
    commit_all(&local, "older");
    let older = local.head().expect("head").target().expect("target");
    write_file(local_dir.path(), "local.txt", "local\n");
    commit_all(&local, "newer");
    drop(local);
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();

    handle
        .create_tag("v0".into(), None, Some(older.to_string()))
        .expect("create tag at an older commit");
    let tags = handle.list_tags().expect("list tags");
    assert_eq!(
        tags[0].commit_id.as_deref(),
        Some(older.to_string().as_str())
    );
    assert_eq!(
        handle.get_undo_state().expect("undo state").undo.as_deref(),
        Some("Create tag v0")
    );

    let (event_tx, event_rx) = mpsc::channel();
    handle
        .push_tags("origin".into(), vec!["v0".into()], false, event_tx)
        .expect("start tag push");
    let _: Vec<_> = event_rx.iter().collect();
    let bare_has_tag = || {
        Repository::open_bare(remote_dir.path())
            .expect("open bare")
            .find_reference("refs/tags/v0")
            .is_ok()
    };
    assert!(bare_has_tag());

    let (event_tx, event_rx) = mpsc::channel();
    let delete_id = handle
        .delete_remote_tag("origin".into(), "v0".into(), false, event_tx)
        .expect("start delete");
    let events: Vec<_> = event_rx.iter().collect();
    assert!(delete_id.starts_with("delete-remote-"));
    assert_eq!(
        completed(&events),
        (TransferOperation::DeleteRemoteTag, None)
    );
    assert!(!bare_has_tag());
    // The local tag stays, and the remote delete records no undo step.
    assert_eq!(handle.list_tags().expect("list tags").len(), 1);
    assert_eq!(
        handle.get_undo_state().expect("undo state").undo.as_deref(),
        Some("Create tag v0")
    );
}

#[test]
fn push_and_force_push_refuse_up_front_when_push_default_needs_a_choice() {
    let (_source_dir, remote_dir, local_dir) = local_and_bare_remote();
    let local = Repository::open(local_dir.path()).expect("open local");
    write_file(local_dir.path(), "local.txt", "local\n");
    commit_all(&local, "local root");
    let mut config = local.config().expect("config");
    config
        .set_str("push.default", "simple")
        .expect("push.default");
    config
        .set_str("branch.feat.remote", "origin")
        .expect("remote");
    config
        .set_str("branch.feat.merge", "refs/heads/main")
        .expect("merge");
    drop(config);
    let head = local
        .head()
        .expect("head")
        .peel_to_commit()
        .expect("commit");
    local.branch("feat", &head, false).expect("create feat");
    local.set_head("refs/heads/feat").expect("checkout feat");
    drop(head);
    drop(local);
    let bare_main = || {
        Repository::open_bare(remote_dir.path())
            .expect("open bare")
            .find_reference("refs/heads/main")
            .ok()
            .and_then(|reference| reference.target())
    };
    let before = bare_main();
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();

    let (event_tx, event_rx) = mpsc::channel();
    let pushed = handle.push_current_branch("origin".into(), false, event_tx);
    assert!(event_rx.iter().next().is_none());
    let (event_tx, event_rx) = mpsc::channel();
    let forced = handle.force_push_current_branch("origin".into(), false, event_tx);
    assert!(event_rx.iter().next().is_none());

    for result in [pushed, forced] {
        let message = result.expect_err("must not start a transfer");
        assert!(message.message.contains("Push to…"), "{message}");
    }
    assert_eq!(bare_main(), before);
    let upstream = handle
        .get_current_upstream()
        .expect("upstream")
        .expect("set");
    assert_eq!(upstream.push_branch, None);
}

#[test]
fn tags_and_branch_push_round_trip_through_the_worker() {
    let (local_dir, repo) = init_repo();
    write_file(local_dir.path(), "README.md", "initial commit\n");
    commit_all(&repo, "initial commit");

    let remote_dir = TempDir::new().expect("create bare remote directory");
    Repository::init_bare(remote_dir.path()).expect("init bare remote");
    repo.remote("origin", remote_dir.path().to_str().expect("remote path"))
        .expect("add origin");
    drop(repo);

    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();

    handle
        .create_tag("v1.0.0".into(), Some("first release".into()), None)
        .expect("create tag");
    let tags = handle.list_tags().expect("list tags");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0].name, "v1.0.0");
    assert_eq!(
        tags[0].target_id,
        handle
            .get_commit_graph(1, git_core::graph::GraphSelection::default())
            .unwrap()[0]
            .id
    );
    assert_eq!(
        tags[0].commit_id.as_deref(),
        Some(
            handle
                .get_commit_graph(1, git_core::graph::GraphSelection::default())
                .unwrap()[0]
                .id
                .as_str()
        )
    );
    assert!(tags[0].annotated);
    assert_eq!(tags[0].message.as_deref(), Some("first release"));
    assert_eq!(tags[0].tagger_name.as_deref(), Some("Test User"));
    assert!(tags[0].timestamp.is_some());

    let (event_tx, event_rx) = mpsc::channel();
    let operation_id = handle
        .push_current_branch("origin".into(), false, event_tx)
        .expect("start branch push");
    let events: Vec<_> = event_rx.iter().collect();

    assert!(matches!(
        events.first(),
        Some(TransferEvent::Started {
            operation_id: id,
            operation: TransferOperation::PushBranch,
        }) if id == &operation_id
    ));
    assert!(events.iter().all(|event| match event {
        TransferEvent::Started {
            operation_id: id, ..
        }
        | TransferEvent::RemoteFailed {
            operation_id: id, ..
        }
        | TransferEvent::Completed {
            operation_id: id, ..
        } => id == &operation_id,
        TransferEvent::Progress(progress) => progress.operation_id == operation_id,
    }));
    assert!(matches!(
        events.last(),
        Some(TransferEvent::Completed { error: None, .. })
    ));
    assert!(Repository::open_bare(remote_dir.path())
        .expect("open bare remote")
        .head()
        .is_ok());
}

#[test]
fn pull_fetch_failure_returns_only_a_sanitized_error() {
    let (local_dir, repo) = init_repo();
    write_file(local_dir.path(), "README.md", "initial commit\n");
    commit_all(&repo, "initial commit");
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    let secret_path = local_dir.path().join("alice-secret").join("missing.git");
    repo.remote("origin", secret_path.to_str().unwrap())
        .unwrap();
    let mut config = repo.config().unwrap();
    config
        .set_str(&format!("branch.{branch}.remote"), "origin")
        .unwrap();
    config
        .set_str(&format!("branch.{branch}.merge"), "refs/heads/main")
        .unwrap();
    drop(config);
    drop(repo);

    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let (event_tx, _event_rx) = mpsc::channel();

    let error = worker
        .handle()
        .pull_current_upstream(event_tx)
        .expect_err("pull should fail while fetching");

    assert_eq!(error, crate::wire::ErrorDto::pull_failed());
    assert!(!error.message.contains("alice-secret"));
}

#[test]
fn pull_with_untracked_file_reports_dirty_worktree() {
    let (local_dir, repo) = init_repo();
    write_file(local_dir.path(), "README.md", "initial commit\n");
    commit_all(&repo, "initial commit");
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    repo.remote("origin", local_dir.path().to_str().unwrap())
        .unwrap();
    let mut config = repo.config().unwrap();
    config
        .set_str(&format!("branch.{branch}.remote"), "origin")
        .unwrap();
    config
        .set_str(
            &format!("branch.{branch}.merge"),
            &format!("refs/heads/{branch}"),
        )
        .unwrap();
    drop(config);
    drop(repo);
    write_file(local_dir.path(), "untracked.txt", "x");

    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let (event_tx, _event_rx) = mpsc::channel();

    let error = worker
        .handle()
        .pull_current_upstream(event_tx)
        .expect_err("pull should refuse a dirty worktree");

    assert_eq!(error.message, "cannot pull with a dirty worktree");
    assert_eq!(error.kind, crate::wire::ErrorKind::DirtyWorktree);
}

#[test]
fn force_push_streams_a_completed_force_push_event() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let local = Repository::open(local_dir.path()).expect("open local repo");
    // Record the remote tip as the lease's expected value.
    local
        .find_remote("origin")
        .expect("origin")
        .fetch(&["refs/heads/*:refs/remotes/origin/*"], None, None)
        .expect("fetch remote tip");
    write_file(local_dir.path(), "README.md", "rewritten local history\n");
    commit_all(&local, "local commit");
    drop(local);
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let (event_tx, event_rx) = mpsc::channel();

    let operation_id = worker
        .handle()
        .force_push_current_branch("origin".into(), false, event_tx)
        .expect("start force push");
    let events: Vec<_> = event_rx.iter().collect();

    assert!(operation_id.starts_with("force-push-"));
    assert!(matches!(
        events.last(),
        Some(TransferEvent::Completed {
            operation_id: id,
            operation: TransferOperation::ForcePushBranch,
            error: None,
            hook_output: None,
        }) if id == &operation_id
    ));
}

#[test]
fn a_background_fetch_fetches_every_remote_quietly_and_records_no_undo_entry() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    let undo_before = handle.get_undo_state().expect("undo state");
    let (event_tx, event_rx) = mpsc::channel();

    let operation_id = handle
        .background_fetch(None, 5, event_tx)
        .expect("start background fetch")
        .expect("an idle worker starts the background fetch");
    let events: Vec<_> = event_rx.iter().collect();

    assert!(operation_id.starts_with("background-fetch-"));
    assert!(matches!(
        events.first(),
        Some(TransferEvent::Started {
            operation: TransferOperation::BackgroundFetch,
            ..
        })
    ));
    assert_eq!(
        completed(&events),
        (TransferOperation::BackgroundFetch, None)
    );
    let local = Repository::open(local_dir.path()).expect("reopen local");
    assert!(
        local
            .branches(Some(git2::BranchType::Remote))
            .expect("branches")
            .count()
            > 0
    );
    assert_eq!(handle.get_undo_state().expect("undo state"), undo_before);
    // Done: the registry forgets it, so the next background fetch may start.
    assert_eq!(handle.cancelled.len(), 0);
}

#[test]
fn a_background_fetch_reports_a_failing_remote_as_background_fetch() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let missing = TempDir::new().expect("missing dir");
    Repository::open(local_dir.path())
        .expect("open local")
        .remote(
            "broken",
            missing.path().join("nope").to_str().expect("path"),
        )
        .expect("add broken remote");
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let (event_tx, event_rx) = mpsc::channel();

    worker
        .handle()
        .background_fetch(None, 5, event_tx)
        .expect("start background fetch")
        .expect("started");
    let events: Vec<_> = event_rx.iter().collect();

    assert!(events.iter().any(|event| matches!(
        event,
        TransferEvent::RemoteFailed {
            operation: TransferOperation::BackgroundFetch,
            remote_name,
            ..
        } if remote_name == "broken"
    )));
    assert_eq!(
        completed(&events),
        (TransferOperation::BackgroundFetch, None)
    );
}

#[test]
fn a_background_fetch_is_skipped_while_another_operation_is_queued_or_running() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    // A manual fetch (or a commit, a push, another background fetch…) holds an operation ID
    // from the moment it is queued until it finished.
    let manual = handle.cancelled.allocate("fetch");
    let (event_tx, event_rx) = mpsc::channel();

    assert_eq!(handle.background_fetch(None, 5, event_tx.clone()), Ok(None));
    drop(event_tx);
    assert!(
        event_rx.iter().next().is_none(),
        "a skipped fetch sends no event"
    );
    drop(manual);
    let (event_tx, _event_rx) = mpsc::channel();
    assert!(handle
        .background_fetch(None, 5, event_tx)
        .expect("start")
        .is_some());
}

/// A server that accepts connections and never answers: a host a fetch can't get anything from,
/// without the network (#462). Dropping it closes what it accepted, so a fetch still waiting on
/// it ends.
struct SilentServer {
    url: String,
    accepted: mpsc::Receiver<()>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl SilentServer {
    fn start() -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        listener.set_nonblocking(true).expect("nonblocking");
        let url = format!(
            "http://127.0.0.1:{}/repo.git",
            listener.local_addr().expect("address").port()
        );
        let (accepted_tx, accepted) = mpsc::channel();
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            let mut held = Vec::new();
            while !stopped.load(std::sync::atomic::Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        held.push(stream);
                        let _ = accepted_tx.send(());
                    }
                    Err(_) => thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        Self {
            url,
            accepted,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for SilentServer {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn background_options(
    connect_limit: Duration,
) -> super::super::background_fetch::BackgroundFetchOptions {
    super::super::background_fetch::BackgroundFetchOptions {
        prune: None,
        interval: Duration::from_secs(5 * 60),
        connect_limit,
    }
}

fn remote_failures(events: &[TransferEvent]) -> Vec<(String, TransferErrorKind)> {
    events
        .iter()
        .filter_map(|event| match event {
            TransferEvent::RemoteFailed {
                remote_name, error, ..
            } => Some((remote_name.clone(), *error)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_cancel_stops_waiting_for_a_background_fetch_that_is_still_connecting() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let server = SilentServer::start();
    Repository::open(local_dir.path())
        .expect("open local")
        .remote("silent", &server.url)
        .expect("add silent remote");
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
    // Well within the connect limit, the worker keeps waiting for it.
    thread::sleep(Duration::from_millis(300));
    let early: Vec<_> = event_rx.try_iter().collect();
    assert!(
        !early
            .iter()
            .any(|event| matches!(event, TransferEvent::Completed { .. })),
        "{early:?}"
    );
    assert!(
        early.iter().any(|event| matches!(
            event,
            TransferEvent::Progress(progress) if progress.operation_id == operation_id
        )),
        "the other remote's progress is passed on: {early:?}"
    );
    // The user starts an action: the app cancels the background fetch, then sends the action.
    let started = std::time::Instant::now();
    handle.cancel_transfer(&operation_id);
    handle.get_status().expect("status");

    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the action waited {:?} for a fetch that was still connecting",
        started.elapsed()
    );
    let events: Vec<_> = event_rx.iter().collect();
    assert_eq!(
        completed(&events),
        (
            TransferOperation::BackgroundFetch,
            Some(TransferErrorKind::Cancelled)
        )
    );
    assert_eq!(handle.cancelled.len(), 0);
    let local = Repository::open(local_dir.path()).expect("reopen local");
    assert!(local
        .references_glob("refs/remotes/silent/*")
        .expect("references")
        .next()
        .is_none());
}

#[test]
fn a_remote_that_never_answers_is_given_up_after_the_connect_limit_and_skipped_next_time() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let server = SilentServer::start();
    Repository::open(local_dir.path())
        .expect("open local")
        .remote("silent", &server.url)
        .expect("add silent remote");
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    let options = background_options(Duration::from_millis(300));

    let (event_tx, event_rx) = mpsc::channel();
    handle
        .background_fetch_with(options, event_tx)
        .expect("start")
        .expect("started");
    let events: Vec<_> = event_rx.iter().collect();

    // The other remote is fetched; the silent one fails quietly, and the tick counts as a fetch.
    assert_eq!(
        remote_failures(&events),
        [("silent".to_string(), TransferErrorKind::TransferFailed)]
    );
    assert_eq!(
        completed(&events),
        (TransferOperation::BackgroundFetch, None)
    );
    server
        .accepted
        .recv_timeout(Duration::from_secs(5))
        .expect("the first tick tried the silent remote");

    // The next tick skips it (backing off, and its first connect is still waiting) instead of
    // waiting for it again.
    let (event_tx, event_rx) = mpsc::channel();
    let started = std::time::Instant::now();
    handle
        .background_fetch_with(options, event_tx)
        .expect("start")
        .expect("started");
    let events: Vec<_> = event_rx.iter().collect();
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(
        remote_failures(&events),
        [("silent".to_string(), TransferErrorKind::TransferFailed)]
    );
    assert!(server.accepted.try_recv().is_err(), "no second connect");
}

/// A `CredentialStore` fake whose tokens the test can still read after the worker took it.
#[derive(Clone, Default)]
struct SharedCredentialStore(Arc<Mutex<HashMap<CredentialKey, String>>>);

impl CredentialStore for SharedCredentialStore {
    fn get(&self, key: &CredentialKey) -> Result<Option<String>, CredentialStoreError> {
        Ok(self.0.lock().unwrap().get(key).cloned())
    }

    fn set(&self, key: &CredentialKey, token: &str) -> Result<(), CredentialStoreError> {
        self.0.lock().unwrap().insert(key.clone(), token.to_owned());
        Ok(())
    }

    fn delete(&self, key: &CredentialKey) -> Result<(), CredentialStoreError> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

/// Each remote-management command replies, and an HTTPS token is saved for the remote it
/// names (#527: the mutation run found these replies untested).
#[test]
fn remote_management_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "base");
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    let store = SharedCredentialStore::default();
    let worker = Worker::spawn_with(
        dir.path().to_path_buf(),
        store.clone(),
        super::forge::FakeForgeApi::queue(vec![]),
    )
    .unwrap();
    let handle = worker.handle();
    let first = "https://first.example.com/repo.git";
    let second = "https://second.example.com/repo.git";

    handle
        .add_remote("origin".into(), first.into(), None)
        .unwrap();
    handle
        .add_remote("other".into(), second.into(), None)
        .unwrap();
    handle
        .rename_remote("other".into(), "second".into())
        .unwrap();
    let remotes = handle.list_remotes().unwrap();
    let names: Vec<_> = remotes.iter().map(|remote| remote.name.as_str()).collect();
    assert_eq!(names, ["origin", "second"]);
    handle
        .update_remote_urls("second".into(), second.into(), Some(first.into()))
        .unwrap();
    let pushes: Vec<_> = handle
        .list_remotes()
        .unwrap()
        .into_iter()
        .map(|remote| remote.push_url)
        .collect();
    assert_eq!(pushes, [None, Some(first.to_string())]);
    assert_eq!(handle.get_remote_auth_mode("second".into()).unwrap(), None);

    handle
        .save_https_credential("second".into(), "rene".into(), "token".into())
        .unwrap();
    let key = CredentialKey::for_https(second, "rene").unwrap();
    assert_eq!(
        store.0.lock().unwrap().keys().cloned().collect::<Vec<_>>(),
        [key]
    );
    let https = git_core::remote::RemoteAuthMode::HttpsToken {
        username: "rene".into(),
    };
    handle
        .set_remote_auth_mode("second".into(), https.clone())
        .unwrap();
    assert_eq!(
        handle.get_remote_auth_mode("second".into()).unwrap(),
        Some(https)
    );
    handle.forget_https_credential("second".into()).unwrap();
    assert!(store.0.lock().unwrap().is_empty());

    {
        let mut config = repo.config().unwrap();
        config
            .set_str(&format!("branch.{branch}.remote"), "origin")
            .unwrap();
        config
            .set_str(
                &format!("branch.{branch}.merge"),
                &format!("refs/heads/{branch}"),
            )
            .unwrap();
    }
    assert!(handle.get_current_upstream().unwrap().is_some());
    handle.clear_current_upstream().unwrap();
    assert_eq!(handle.get_current_upstream().unwrap(), None);
}

/// Receives events until the operation's `Completed`, without waiting for the channel to close.
fn until_completed(event_rx: &mpsc::Receiver<TransferEvent>) {
    loop {
        let event = event_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("the operation completes");
        if matches!(event, TransferEvent::Completed { .. }) {
            return;
        }
    }
}

#[test]
fn a_background_fetch_starts_as_soon_as_the_previous_transfer_reported_completed() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
    let handle = worker.handle();
    let options = background_options(Duration::from_secs(30));

    // The app starts the next tick as soon as it sees `Completed` (a tick overdue while the
    // user's own fetch ran, #567): the finished operation must not still count as running.
    let (event_tx, event_rx) = mpsc::channel();
    handle
        .fetch_all_remotes(None, event_tx)
        .expect("start fetch all");
    until_completed(&event_rx);
    for _ in 0..3 {
        let (event_tx, event_rx) = mpsc::channel();
        handle
            .background_fetch_with(options, event_tx)
            .expect("start")
            .expect("the previous transfer completed, so the tick starts");
        until_completed(&event_rx);
    }
}
