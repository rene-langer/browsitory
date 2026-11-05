use super::*;

#[cfg(unix)]
#[test]
fn a_failing_pre_commit_replies_a_hook_failed_outcome_and_records_no_undo() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    install_hook(&repo, "pre-commit", "echo nope; exit 1");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.stage_file("a.txt".into()).unwrap();
    let undo_before = handle.get_undo_state().unwrap().undo;
    let (events_tx, events_rx) = mpsc::channel();

    let outcome = handle
        .commit("msg".into(), false, Vec::new(), events_tx)
        .unwrap();

    assert_eq!(
        outcome,
        git_core::commit::CommitOutcome::HookFailed {
            hook: "pre-commit".into(),
            output: "nope\n".into()
        }
    );
    let events: Vec<HookEvent> = events_rx.iter().collect();
    assert!(
        matches!(&events[..], [
            HookEvent::Started { operation_id: a, hook: h1, .. },
            HookEvent::Finished { operation_id: b, hook: h2, .. },
        ] if a == b && a.starts_with("commit-") && h1 == "pre-commit" && h2 == "pre-commit"),
        "{events:?}"
    );
    assert_eq!(handle.get_undo_state().unwrap().undo, undo_before);
    assert!(repo.head().is_err(), "nothing may be committed");
}

#[cfg(unix)]
#[test]
fn cancel_transfer_stops_a_running_pre_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    install_hook(&repo, "pre-commit", "sleep 30");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.stage_file("a.txt".into()).unwrap();
    let (events_tx, events_rx) = mpsc::channel();
    let canceller = handle.clone();
    let cancel_thread = std::thread::spawn(move || {
        if let Ok(HookEvent::Started { operation_id, .. }) = events_rx.recv() {
            canceller.cancel_transfer(&operation_id);
        }
    });
    let started = std::time::Instant::now();

    let result = handle.commit("msg".into(), false, Vec::new(), events_tx);

    cancel_thread.join().unwrap();
    assert_eq!(result.unwrap_err(), "The pre-commit hook was cancelled.");
    // Generous bound: only proves the hook's `sleep 30` was not waited out.
    assert!(started.elapsed() < Duration::from_secs(25));
}

#[cfg(unix)]
#[test]
fn a_failing_pre_push_completes_with_hook_failed_and_its_output() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
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
            false,
            false,
            event_tx,
        )
        .unwrap();
    let events: Vec<_> = event_rx.iter().collect();
    assert!(
        events.iter().any(|event| matches!(
            event,
            TransferEvent::Progress(progress)
                if progress.phase == git_core::remote::TransferPhase::RunningHook
        )),
        "{events:?}"
    );
    assert!(
        matches!(
            events.last(),
            Some(TransferEvent::Completed {
                error: Some(TransferErrorKind::HookFailed),
                hook_output: Some(output),
                ..
            }) if output == "tests failed\n"
        ),
        "{events:?}"
    );

    let (event_tx, event_rx) = mpsc::channel();
    handle
        .push_branch(
            "origin".into(),
            branch,
            "topic".into(),
            false,
            true,
            event_tx,
        )
        .unwrap();
    let events: Vec<_> = event_rx.iter().collect();
    assert!(
        matches!(
            events.last(),
            Some(TransferEvent::Completed {
                error: None,
                hook_output: None,
                ..
            })
        ),
        "{events:?}"
    );
}

#[cfg(unix)]
#[test]
fn post_checkout_gets_old_and_new_head_after_switch_and_checkout_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "first");
    let first = repo.head().unwrap().target().unwrap();
    repo.branch("other", &repo.find_commit(first).unwrap(), false)
        .unwrap();
    write_file(dir.path(), "a.txt", "b");
    commit_all(&repo, "second");
    let second = repo.head().unwrap().target().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    let record = dir.path().join("post-checkout.txt");
    // Fails on purpose: a post-checkout failure must not fail the switch.
    install_hook(
        &repo,
        "post-checkout",
        &format!(r#"echo "$@" >> '{}'; exit 1"#, record.display()),
    );
    std::fs::write(
        dir.path().join(".gitignore"),
        "post-checkout.txt\n.gitignore\n",
    )
    .unwrap();
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    handle.switch_branch("other".into(), no_events()).unwrap();
    handle
        .checkout_commit(second.to_string(), no_events())
        .unwrap();
    handle.switch_branch(main, no_events()).unwrap();

    assert_eq!(
        std::fs::read_to_string(&record).unwrap(),
        format!("{second} {first} 1\n{first} {second} 1\n{second} {second} 1\n")
    );
}

#[cfg(unix)]
#[test]
fn post_merge_runs_after_a_fast_forward_and_a_squash() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "first");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    let first = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &first, false).unwrap();
    repo.branch("other", &first, false).unwrap();
    repo.set_head("refs/heads/feature").unwrap();
    write_file(dir.path(), "b.txt", "b");
    commit_all(&repo, "feature work");
    repo.set_head("refs/heads/other").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    write_file(dir.path(), "c.txt", "c");
    commit_all(&repo, "other work");
    repo.set_head(&format!("refs/heads/{main}")).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    let record = repo.path().join("post-merge.txt");
    install_hook(
        &repo,
        "post-merge",
        &format!(r#"echo "$@" >> '{}'"#, record.display()),
    );
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let merged = handle
        .start_merge(
            "feature".into(),
            git_core::merge::MergeOptions::default(),
            no_events(),
        )
        .unwrap();
    assert_eq!(merged, MergeOutcome::FastForwarded);
    let squashed = handle
        .start_merge(
            "other".into(),
            git_core::merge::MergeOptions {
                squash: true,
                ..git_core::merge::MergeOptions::default()
            },
            no_events(),
        )
        .unwrap();
    assert_eq!(squashed, MergeOutcome::Squashed);

    assert_eq!(std::fs::read_to_string(&record).unwrap(), "0\n1\n");
}

/// Collects a command's hook events on a thread; with `cancel_hook`, presses Cancel as soon
/// as that hook starts.
#[cfg(unix)]
fn watch_hooks(
    handle: &super::super::WorkerHandle,
    cancel_hook: Option<&'static str>,
) -> (
    mpsc::Sender<HookEvent>,
    std::thread::JoinHandle<Vec<HookEvent>>,
) {
    let (events_tx, events_rx) = mpsc::channel();
    let canceller = handle.clone();
    let thread = std::thread::spawn(move || {
        let mut seen = Vec::new();
        for event in events_rx {
            if let HookEvent::Started {
                operation_id, hook, ..
            } = &event
            {
                if Some(hook.as_str()) == cancel_hook {
                    canceller.cancel_transfer(operation_id);
                }
            }
            seen.push(event);
        }
        seen
    });
    (events_tx, thread)
}

/// `[Started, Finished]` of `hook` from `source` on one operation id starting `prefix`.
#[cfg(unix)]
fn assert_one_reported_hook(
    events: &[HookEvent],
    hook: &str,
    source: super::super::HookSource,
    prefix: &str,
) {
    assert!(
        matches!(events, [
            HookEvent::Started { operation_id: a, hook: h1, source: s1 },
            HookEvent::Finished { operation_id: b, hook: h2, source: s2 },
        ] if a == b && a.starts_with(prefix) && h1 == hook && h2 == hook
            && *s1 == source && *s2 == source),
        "{events:?}"
    );
}

/// HEAD on the initial branch with a second commit; `other` points at the first. Returns
/// the initial branch's name.
#[cfg(unix)]
fn two_branches() -> (TempDir, Repository, String) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "first");
    let first = repo.head().unwrap().target().unwrap();
    repo.branch("other", &repo.find_commit(first).unwrap(), false)
        .unwrap();
    write_file(dir.path(), "a.txt", "b");
    commit_all(&repo, "second");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    (dir, repo, main)
}

/// #185: the "Hook status" strip shows post-checkout with Cancel.
#[cfg(unix)]
#[test]
fn switch_and_checkout_report_post_checkout_on_their_operation() {
    let (dir, repo, _main) = two_branches();
    let second = repo.head().unwrap().target().unwrap();
    install_hook(&repo, "post-checkout", "exit 0");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let (events, watcher) = watch_hooks(&handle, None);
    handle.switch_branch("other".into(), events).unwrap();
    let events = watcher.join().unwrap();
    assert_one_reported_hook(
        &events,
        "post-checkout",
        super::super::HookSource::Checkout,
        "switch-",
    );

    let (events, watcher) = watch_hooks(&handle, None);
    handle.checkout_commit(second.to_string(), events).unwrap();
    let events = watcher.join().unwrap();
    assert_one_reported_hook(
        &events,
        "post-checkout",
        super::super::HookSource::Checkout,
        "checkout-",
    );
}

#[cfg(unix)]
#[test]
fn cancel_stops_post_checkout_and_keeps_the_switch_or_checkout() {
    let (dir, repo, _main) = two_branches();
    let first = repo
        .find_branch("other", git2::BranchType::Local)
        .unwrap()
        .get()
        .target()
        .unwrap();
    install_hook(&repo, "post-checkout", "sleep 30");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let started = std::time::Instant::now();

    let (events, watcher) = watch_hooks(&handle, Some("post-checkout"));
    handle.switch_branch("other".into(), events).unwrap();
    watcher.join().unwrap();
    assert_eq!(
        Repository::open(dir.path())
            .unwrap()
            .head()
            .unwrap()
            .shorthand()
            .unwrap(),
        "other"
    );

    let (events, watcher) = watch_hooks(&handle, Some("post-checkout"));
    handle.checkout_commit(first.to_string(), events).unwrap();
    watcher.join().unwrap();
    let reopened = Repository::open(dir.path()).unwrap();
    assert!(reopened.head_detached().unwrap());
    assert_eq!(reopened.head().unwrap().target().unwrap(), first);

    // Generous bound: only proves neither hook's `sleep 30` was waited out.
    assert!(started.elapsed() < Duration::from_secs(25));
}

#[cfg(unix)]
#[test]
fn fast_forward_and_squash_report_post_merge_and_cancel_keeps_the_merge() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "first");
    let first = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &first, false).unwrap();
    repo.branch("other", &first, false).unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    repo.set_head("refs/heads/feature").unwrap();
    write_file(dir.path(), "b.txt", "b");
    commit_all(&repo, "feature work");
    let feature = repo.head().unwrap().target().unwrap();
    repo.set_head("refs/heads/other").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    write_file(dir.path(), "c.txt", "c");
    commit_all(&repo, "other work");
    repo.set_head(&format!("refs/heads/{main}")).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    install_hook(&repo, "post-merge", r#"[ "$1" = 1 ] && exit 0; sleep 30"#);
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let started = std::time::Instant::now();

    let (events, watcher) = watch_hooks(&handle, Some("post-merge"));
    let merged = handle
        .start_merge(
            "feature".into(),
            git_core::merge::MergeOptions::default(),
            events,
        )
        .unwrap();
    assert_eq!(merged, MergeOutcome::FastForwarded);
    assert_one_reported_hook(
        &watcher.join().unwrap(),
        "post-merge",
        super::super::HookSource::Merge,
        "merge-",
    );
    assert!(started.elapsed() < Duration::from_secs(25));
    assert_eq!(
        Repository::open(dir.path())
            .unwrap()
            .head()
            .unwrap()
            .target()
            .unwrap(),
        feature
    );

    let (events, watcher) = watch_hooks(&handle, None);
    let squashed = handle
        .start_merge(
            "other".into(),
            git_core::merge::MergeOptions {
                squash: true,
                ..git_core::merge::MergeOptions::default()
            },
            events,
        )
        .unwrap();
    assert_eq!(squashed, MergeOutcome::Squashed);
    assert_one_reported_hook(
        &watcher.join().unwrap(),
        "post-merge",
        super::super::HookSource::Merge,
        "merge-",
    );
}

/// `upstream` with one commit, `local` cloned from it, then a second upstream commit, so a
/// pull fast-forwards.
#[cfg(unix)]
fn pull_fixture() -> (TempDir, TempDir, Repository) {
    let (upstream_dir, upstream) = init_repo();
    write_file(upstream_dir.path(), "a.txt", "a\n");
    commit_all(&upstream, "first");
    let local_dir = TempDir::new().unwrap();
    let local =
        Repository::clone(upstream_dir.path().to_str().unwrap(), local_dir.path()).expect("clone");
    {
        let mut config = local.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        config.set_bool("commit.gpgSign", false).unwrap();
        let hooks = local.path().join("hooks");
        config
            .set_str("core.hooksPath", hooks.to_str().unwrap())
            .unwrap();
    }
    write_file(upstream_dir.path(), "b.txt", "b\n");
    commit_all(&upstream, "second");
    (upstream_dir, local_dir, local)
}

/// #185: a pull's post-merge shows in the transfer panel ("Running post-merge hook…") and
/// its Cancel stops only the hook.
#[cfg(unix)]
#[test]
fn a_pull_reports_post_merge_as_running_hook_and_cancel_keeps_the_pull() {
    let (_upstream_dir, local_dir, local) = pull_fixture();
    // The hook says it runs by creating `marker`; the test cancels it only then (#285).
    let marker_dir = TempDir::new().unwrap();
    let marker = marker_dir.path().join("post-merge-started");
    install_hook(
        &local,
        "post-merge",
        &format!(": > '{}'; sleep 30", marker.display()),
    );
    let worker = Worker::spawn(local_dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let (events_tx, events_rx) = mpsc::channel();
    let canceller = handle.clone();
    let watched_marker = marker.clone();
    let watcher = std::thread::spawn(move || {
        let mut seen = Vec::new();
        let mut hook_was_running = false;
        for event in events_rx {
            if let TransferEvent::Progress(progress) = &event {
                if progress.phase == git_core::remote::TransferPhase::RunningHook {
                    // Generous bound: the hook only has to start, however busy the machine.
                    let deadline = std::time::Instant::now() + Duration::from_secs(20);
                    while !watched_marker.exists() && std::time::Instant::now() < deadline {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    hook_was_running = watched_marker.exists();
                    canceller.cancel_transfer(&progress.operation_id);
                }
            }
            seen.push(event);
        }
        (seen, hook_was_running)
    });
    let started = std::time::Instant::now();

    let outcome = handle.pull_current_upstream(events_tx);

    let (events, hook_was_running) = watcher.join().unwrap();
    assert!(hook_was_running, "post-merge never started: {events:?}");
    assert!(
        matches!(
            outcome,
            Ok(git_core::remote::PullOutcome::FastForwarded { .. })
        ),
        "{outcome:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(25));
    assert!(local_dir.path().join("b.txt").exists());
    let running = events.iter().position(|event| {
        matches!(event, TransferEvent::Progress(progress)
            if progress.phase == git_core::remote::TransferPhase::RunningHook
                && progress.operation == TransferOperation::Pull)
    });
    let completed = events
        .iter()
        .position(|event| matches!(event, TransferEvent::Completed { error: None, .. }));
    assert!(
        matches!((running, completed), (Some(r), Some(c)) if r < c),
        "{events:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_pull_without_post_merge_reports_no_running_hook() {
    let (_upstream_dir, local_dir, _local) = pull_fixture();
    let worker = Worker::spawn(local_dir.path().to_path_buf()).unwrap();
    let (events_tx, events_rx) = mpsc::channel();

    worker.handle().pull_current_upstream(events_tx).unwrap();

    assert!(!events_rx.iter().any(|event| matches!(event,
        TransferEvent::Progress(progress)
            if progress.phase == git_core::remote::TransferPhase::RunningHook)));
}

/// #183: a clean pick's hooks show in the "Hook status" strip (`HookSource::Pick`).
#[cfg(unix)]
#[test]
fn cherry_pick_many_reports_its_hooks_with_source_pick() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    install_hook(&repo, "prepare-commit-msg", "exit 0");
    install_hook(&repo, "post-commit", "exit 0");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let (events, watcher) = watch_hooks(&handle, None);
    let outcome = handle
        .cherry_pick_many(vec![ids[0].clone(), ids[2].clone()], events)
        .unwrap();

    assert!(matches!(
        outcome,
        git_core::pick::SequenceOutcome::Completed { applied: 2, .. }
    ));
    let events = watcher.join().unwrap();
    let hooks: Vec<(String, super::super::HookSource)> = events
        .iter()
        .filter_map(|event| match event {
            HookEvent::Started {
                operation_id,
                hook,
                source,
            } if operation_id.starts_with("pick-") => Some((hook.clone(), *source)),
            _ => None,
        })
        .collect();
    let pick = super::super::HookSource::Pick;
    assert_eq!(
        hooks,
        [
            ("prepare-commit-msg".to_string(), pick),
            ("post-commit".to_string(), pick),
            ("prepare-commit-msg".to_string(), pick),
            ("post-commit".to_string(), pick),
        ]
    );
}

/// D9-A: Cancel during a clean step's post-commit keeps that commit and stops the sequence
/// after it; Continue goes on with the next step.
#[cfg(unix)]
#[test]
fn cancel_during_a_picks_post_commit_stops_the_sequence_after_that_step() {
    let (dir, repo, ids) = side_commits_with_a_conflict();
    install_hook(&repo, "post-commit", "sleep 30");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let before = repo.head().unwrap().target().unwrap();

    let (events, watcher) = watch_hooks(&handle, Some("post-commit"));
    let result = handle.cherry_pick_many(vec![ids[0].clone(), ids[2].clone()], events);
    watcher.join().unwrap();

    assert!(
        matches!(
            &result,
            Ok(git_core::pick::SequenceOutcome::Stopped { kept, .. }) if kept == "f one"
        ),
        "{result:?}"
    );
    let reopened = Repository::open(dir.path()).unwrap();
    let head = reopened.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.summary().unwrap(), Some("f one"));
    assert_eq!(head.parent_id(0).unwrap(), before);
    assert!(sequence_file(&repo).exists(), "the sequence is kept");

    std::fs::remove_file(repo.path().join("hooks").join("post-commit")).unwrap();
    let outcome = handle.continue_sequence(false, no_events()).unwrap();
    assert!(matches!(
        outcome,
        git_core::pick::SequenceOutcome::Completed { .. }
    ));
    let reopened = Repository::open(dir.path()).unwrap();
    let head = reopened.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.summary().unwrap(), Some("f three"));
    assert!(!sequence_file(&repo).exists());
}

/// #183 (D7-A): the merge commit landed, so it counts and post-merge runs even though git's
/// merge state could not be cleared.
#[cfg(unix)]
#[test]
fn a_merge_commit_whose_state_cannot_be_cleared_still_counts_and_runs_post_merge() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = diverged_repo(false);
    let marker = dir.path().join("post-merge-ran");
    install_hook(
        &repo,
        "post-merge",
        &format!("touch '{}'", marker.display()),
    );
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert_eq!(merge_no_ff(&handle), MergeOutcome::Merged);
    let git_dir = repo.path().to_path_buf();
    std::fs::set_permissions(&git_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(git_dir.join("probe"), "").is_ok() {
        eprintln!("the git dir stays writable (root?); skipping");
        let _ = std::fs::remove_file(git_dir.join("probe"));
        std::fs::set_permissions(&git_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        return;
    }

    let outcome = handle.commit("Merge feature".into(), false, Vec::new(), no_events());

    std::fs::set_permissions(&git_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(
            outcome,
            Ok(git_core::commit::CommitOutcome::Committed { .. })
        ),
        "{outcome:?}"
    );
    assert!(marker.exists(), "post-merge must run");
    let reopened = Repository::open(dir.path()).unwrap();
    let head = reopened.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_count(), 2);
}

/// HEAD on the initial branch, with `feature` diverged from it: both sides added a different
/// file, or (with `conflict`) both changed `a.txt`.
#[cfg(unix)]
fn diverged_repo(conflict: bool) -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "first");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    {
        let first = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &first, false).unwrap();
    }
    repo.set_head("refs/heads/feature").unwrap();
    if conflict {
        write_file(dir.path(), "a.txt", "feature\n");
    } else {
        write_file(dir.path(), "feature.txt", "feature\n");
    }
    commit_all(&repo, "feature work");
    repo.set_head(&format!("refs/heads/{main}")).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    if conflict {
        write_file(dir.path(), "a.txt", "main\n");
    } else {
        let _ = std::fs::remove_file(dir.path().join("feature.txt"));
        write_file(dir.path(), "main.txt", "main\n");
    }
    commit_all(&repo, "main work");
    (dir, repo)
}

#[cfg(unix)]
fn merge_no_ff(handle: &super::super::WorkerHandle) -> MergeOutcome {
    handle
        .start_merge(
            "feature".into(),
            git_core::merge::MergeOptions {
                fast_forward: git_core::merge::FastForward::Never,
                ..git_core::merge::MergeOptions::default()
            },
            no_events(),
        )
        .unwrap()
}

/// A hook that appends `<name> <args>` to `<git dir>/hook-log`; returns the log's path.
#[cfg(unix)]
fn log_hook(repo: &Repository, name: &str) -> std::path::PathBuf {
    let log = repo.path().join("hook-log");
    install_hook(
        repo,
        name,
        &format!(r#"echo "{name} $*" >> '{}'"#, log.display()),
    );
    log
}

#[cfg(unix)]
fn commit_message(
    handle: &super::super::WorkerHandle,
    no_verify: bool,
) -> git_core::commit::CommitOutcome {
    handle
        .commit(
            "Merge feature".into(),
            no_verify,
            Vec::new(),
            mpsc::channel().0,
        )
        .unwrap()
}

#[cfg(unix)]
#[test]
fn a_staged_merge_runs_no_post_merge_until_it_is_committed() {
    let (dir, repo) = diverged_repo(false);
    let log = log_hook(&repo, "post-merge");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    assert_eq!(merge_no_ff(&handle), MergeOutcome::Merged);
    assert!(
        !log.exists(),
        "post-merge ran before the merge commit existed"
    );

    assert!(matches!(
        commit_message(&handle, false),
        git_core::commit::CommitOutcome::Committed { .. }
    ));
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "post-merge 0\n");
}

/// #178: the commit box shows "Running post-merge hook…" with Cancel, as for post-commit.
#[cfg(unix)]
#[test]
fn a_clean_merge_commit_reports_post_merge_on_the_commit_operation() {
    let (dir, repo) = diverged_repo(false);
    let log = log_hook(&repo, "post-merge");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert_eq!(merge_no_ff(&handle), MergeOutcome::Merged);
    let (events_tx, events_rx) = mpsc::channel();

    let outcome = handle
        .commit("Merge feature".into(), false, Vec::new(), events_tx)
        .unwrap();

    assert!(matches!(
        outcome,
        git_core::commit::CommitOutcome::Committed { .. }
    ));
    let events: Vec<HookEvent> = events_rx.iter().collect();
    assert!(
        matches!(&events[..], [
            HookEvent::Started { operation_id: a, hook: h1, .. },
            HookEvent::Finished { operation_id: b, hook: h2, .. },
        ] if a == b && a.starts_with("commit-") && h1 == "post-merge" && h2 == "post-merge"),
        "{events:?}"
    );
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "post-merge 0\n");
}

#[cfg(unix)]
#[test]
fn cancel_transfer_stops_a_running_post_merge_and_keeps_the_commit() {
    let (dir, repo) = diverged_repo(false);
    install_hook(&repo, "post-merge", "sleep 30");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert_eq!(merge_no_ff(&handle), MergeOutcome::Merged);
    let (events_tx, events_rx) = mpsc::channel();
    let canceller = handle.clone();
    let cancel_thread = std::thread::spawn(move || {
        for event in events_rx {
            if let HookEvent::Started {
                operation_id, hook, ..
            } = event
            {
                if hook == "post-merge" {
                    canceller.cancel_transfer(&operation_id);
                }
            }
        }
    });
    let started = std::time::Instant::now();

    let result = handle.commit("Merge feature".into(), false, Vec::new(), events_tx);

    cancel_thread.join().unwrap();
    assert!(
        matches!(
            result,
            Ok(git_core::commit::CommitOutcome::Committed { .. })
        ),
        "{result:?}"
    );
    // Generous bound: only proves the hook's `sleep 30` was not waited out.
    assert!(started.elapsed() < Duration::from_secs(25));
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_count(), 2);
}

/// #154 case A (D9-A): discarding a file that is now a folder deletes the untracked files in
/// it, as `git restore` does; Undo must bring them back.
#[cfg(unix)]
#[test]
fn undo_of_discarding_a_file_restores_untracked_files_in_the_folder_that_replaced_it() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "d", "file\n");
    commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("d")).unwrap();
    std::fs::create_dir(dir.path().join("d")).unwrap();
    write_file(dir.path(), "d/u", "untracked\n");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    handle.discard_file("d".into()).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("d")).unwrap(),
        "file\n"
    );

    assert!(matches!(handle.undo().unwrap(), UndoOutcome::Done { .. }));
    assert!(dir.path().join("d").is_dir());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("d/u")).unwrap(),
        "untracked\n"
    );
}

/// `git merge --no-verify` still runs post-merge; so does "Skip hooks".
#[cfg(unix)]
#[test]
fn post_merge_runs_even_with_skip_hooks() {
    let (dir, repo) = diverged_repo(false);
    let log = log_hook(&repo, "post-merge");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert_eq!(merge_no_ff(&handle), MergeOutcome::Merged);

    assert!(matches!(
        commit_message(&handle, true),
        git_core::commit::CommitOutcome::Committed { .. }
    ));
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "post-merge 0\n");
}

#[cfg(unix)]
#[test]
fn a_clean_merge_commit_runs_pre_merge_commit_not_pre_commit() {
    let (dir, repo) = diverged_repo(false);
    let log = log_hook(&repo, "pre-commit");
    log_hook(&repo, "pre-merge-commit");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert_eq!(merge_no_ff(&handle), MergeOutcome::Merged);
    let (events_tx, events_rx) = mpsc::channel();

    let outcome = handle
        .commit("Merge feature".into(), false, Vec::new(), events_tx)
        .unwrap();

    assert!(matches!(
        outcome,
        git_core::commit::CommitOutcome::Committed { .. }
    ));
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        "pre-merge-commit \n"
    );
    let events: Vec<HookEvent> = events_rx.iter().collect();
    assert!(
        matches!(&events[0], HookEvent::Started { hook, .. } if hook == "pre-merge-commit"),
        "{events:?}"
    );
}

/// "Skip hooks" offers what the commit will run: `pre-merge-commit` for the clean merge this
/// worker staged, `pre-commit` for any other staged merge (here: after a restart).
#[cfg(unix)]
#[test]
fn head_info_lists_the_pre_hook_the_merge_commit_will_run() {
    let (dir, repo) = diverged_repo(false);
    install_hook(&repo, "pre-commit", "exit 0");
    install_hook(&repo, "pre-merge-commit", "exit 0");
    {
        let first = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = first.handle();
        assert_eq!(merge_no_ff(&handle), MergeOutcome::Merged);
        assert_eq!(
            handle.get_head_info().unwrap().commit_hooks,
            vec!["pre-merge-commit".to_string()]
        );
    }
    let second = Worker::spawn(dir.path().to_path_buf()).unwrap();
    assert_eq!(
        second.handle().get_head_info().unwrap().commit_hooks,
        vec!["pre-commit".to_string()]
    );
}

/// The worker picked the clean-merge hook set, but the merge was aborted (another window, a
/// terminal) before the commit: the commit runs the ordinary hooks, and post-merge must not
/// run after it.
#[cfg(unix)]
#[test]
fn a_clean_merge_aborted_before_the_commit_runs_no_post_merge() {
    let (dir, repo) = diverged_repo(false);
    let log = log_hook(&repo, "pre-commit");
    log_hook(&repo, "pre-merge-commit");
    log_hook(&repo, "post-merge");
    log_hook(&repo, "post-commit");
    write_file(dir.path(), "staged.txt", "x\n");
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("staged.txt")).unwrap();
    index.write().unwrap();
    let mut repo = Repository::open(dir.path()).unwrap();
    let cancelled: super::super::CancelRegistry =
        Arc::new(Mutex::new(std::collections::HashSet::new()));
    let (reply_tx, reply_rx) = mpsc::channel();

    let head = repo.head().unwrap().peel_to_commit().unwrap().id();
    let committed = super::super::status::commit(
        &mut repo,
        "after the abort".into(),
        false,
        Vec::new(),
        git_core::commit::CommitHookSet::CleanMerge {
            head,
            merge_heads: Vec::new(),
        },
        super::super::remote::TransferOp {
            id: "commit-1".into(),
            cancelled: &cancelled,
        },
        mpsc::channel().0,
        reply_tx,
    );

    assert!(committed);
    assert!(matches!(
        reply_rx.recv().unwrap(),
        Ok(git_core::commit::CommitOutcome::Committed { .. })
    ));
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        "pre-commit \npost-commit \n"
    );
}

/// A Merge request refused while the clean merge is staged ("already merging") must not
/// make the worker forget it.
#[cfg(unix)]
#[test]
fn a_refused_second_merge_keeps_the_clean_merge_remembered() {
    let (dir, repo) = diverged_repo(false);
    let log = log_hook(&repo, "pre-commit");
    log_hook(&repo, "pre-merge-commit");
    log_hook(&repo, "post-merge");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert_eq!(merge_no_ff(&handle), MergeOutcome::Merged);
    assert!(handle
        .start_merge(
            "feature".into(),
            git_core::merge::MergeOptions::default(),
            no_events()
        )
        .is_err());

    assert!(matches!(
        commit_message(&handle, false),
        git_core::commit::CommitOutcome::Committed { .. }
    ));
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        "pre-merge-commit \npost-merge 0\n"
    );
}

#[cfg(unix)]
#[test]
fn a_failing_pre_merge_commit_replies_hook_failed_and_commit_without_hooks_lands_the_merge() {
    let (dir, repo) = diverged_repo(false);
    install_hook(&repo, "pre-merge-commit", "echo blocked; exit 1");
    let log = log_hook(&repo, "post-merge");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert_eq!(merge_no_ff(&handle), MergeOutcome::Merged);

    assert_eq!(
        commit_message(&handle, false),
        git_core::commit::CommitOutcome::HookFailed {
            hook: "pre-merge-commit".into(),
            output: "blocked\n".into()
        }
    );
    assert_eq!(
        handle.get_head_info().unwrap().operation,
        Some(git_core::repo::OperationKind::Merge)
    );
    assert!(!log.exists());

    assert!(matches!(
        commit_message(&handle, true),
        git_core::commit::CommitOutcome::Committed { .. }
    ));
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_count(), 2);
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "post-merge 0\n");
}

#[cfg(unix)]
#[test]
fn a_merge_commit_after_conflicts_runs_post_commit_not_post_merge() {
    let (dir, repo) = diverged_repo(true);
    let log = log_hook(&repo, "post-merge");
    log_hook(&repo, "post-commit");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    assert!(matches!(
        merge_no_ff(&handle),
        MergeOutcome::Conflicted { .. }
    ));
    write_file(dir.path(), "a.txt", "resolved\n");
    handle.stage_file("a.txt".into()).unwrap();

    assert!(matches!(
        commit_message(&handle, false),
        git_core::commit::CommitOutcome::Committed { .. }
    ));
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "post-commit \n");
}

/// D2-A: the memory of a clean merge doesn't survive a restart, and the commit then runs
/// `git commit`'s hooks, as `git merge --no-commit` + `git commit` does.
#[cfg(unix)]
#[test]
fn a_clean_merge_committed_after_a_worker_restart_runs_the_commit_hooks() {
    let (dir, repo) = diverged_repo(false);
    let log = log_hook(&repo, "pre-commit");
    log_hook(&repo, "pre-merge-commit");
    log_hook(&repo, "post-merge");
    {
        let first = Worker::spawn(dir.path().to_path_buf()).unwrap();
        assert_eq!(merge_no_ff(&first.handle()), MergeOutcome::Merged);
    }
    let second = Worker::spawn(dir.path().to_path_buf()).unwrap();

    assert!(matches!(
        commit_message(&second.handle(), false),
        git_core::commit::CommitOutcome::Committed { .. }
    ));
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "pre-commit \n");
}

#[cfg(unix)]
#[test]
fn amend_with_no_verify_skips_a_failing_pre_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "first");
    install_hook(&repo, "pre-commit", "exit 1");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();

    let failed = handle
        .amend_commit("amended".into(), false, None, Vec::new(), mpsc::channel().0)
        .unwrap();
    assert!(matches!(
        failed,
        git_core::commit::CommitOutcome::HookFailed { .. }
    ));
    let outcome = handle
        .amend_commit("amended".into(), true, None, Vec::new(), mpsc::channel().0)
        .unwrap();

    assert!(matches!(
        outcome,
        git_core::commit::CommitOutcome::Committed { .. }
    ));
}
