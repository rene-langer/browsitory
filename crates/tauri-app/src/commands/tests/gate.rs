use super::*;

/// More runtime threads than the async runtime has.
fn saturating_count() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get()) + 2
}

/// Spawns a trivial task and waits (from a plain thread) up to 5 s for it to run.
fn runtime_responds() -> bool {
    let probe = tauri::async_runtime::spawn(async { 7 });
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = done_tx.send(tauri::async_runtime::block_on(probe).ok());
    });
    done_rx.recv_timeout(std::time::Duration::from_secs(5)) == Ok(Some(7))
}

#[test]
fn off_runtime_keeps_async_runtime_threads_free() {
    let mut releases = Vec::new();
    let mut blockers = Vec::new();
    for _ in 0..saturating_count() {
        let (release_tx, release_rx) = mpsc::channel::<()>();
        releases.push(release_tx);
        blockers.push(tauri::async_runtime::spawn(super::super::off_runtime(
            move || release_rx.recv().is_ok(),
        )));
    }

    let responded = runtime_responds();

    for release in releases {
        let _ = release.send(());
    }
    for blocker in blockers {
        assert_eq!(tauri::async_runtime::block_on(blocker).unwrap(), Ok(true));
    }
    assert!(
        responded,
        "a blocked off_runtime job held an async runtime thread"
    );
}

/// A repository with one staged file and a `pre-commit` hook that waits until the returned
/// release file exists, so a commit keeps its worker busy for as long as the test wants.
#[cfg(unix)]
fn repo_with_waiting_pre_commit() -> (tempfile::TempDir, std::path::PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        config.set_bool("commit.gpgSign", false).unwrap();
        config
            .set_str(
                "core.hooksPath",
                repo.path().join("hooks").to_str().unwrap(),
            )
            .unwrap();
    }
    // A first commit ("base"), made by git2 so no hook runs, gives a search something to find.
    std::fs::write(dir.path().join("base.txt"), "base").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("base.txt")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let signature = repo.signature().unwrap();
    repo.commit(Some("HEAD"), &signature, &signature, "base", &tree, &[])
        .unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    index.add_path(Path::new("a.txt")).unwrap();
    index.write().unwrap();
    let release = repo.path().join("release");
    let hooks = repo.path().join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    let hook = hooks.join("pre-commit");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\nwhile [ ! -e '{}' ]; do sleep 0.05; done\n",
            release.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    (dir, release)
}

/// An `AppState` with a worker open for each of `dirs`, as `open_repo` opens them. Returns
/// the paths they were opened with.
fn state_with_repos(dirs: &[&std::path::Path]) -> (super::super::AppState, Vec<String>) {
    let state = super::super::AppState::default();
    let keys = dirs
        .iter()
        .map(|dir| {
            let key = dir.to_string_lossy().into_owned();
            super::super::remember_repo(&state, &key).unwrap();
            key
        })
        .collect();
    (state, keys)
}

/// The gate of the repository opened as `path`.
#[cfg(unix)]
fn gate_of(state: &super::super::AppState, path: &str) -> std::sync::Arc<super::super::RepoGate> {
    let (key, _) = state.registry.get(path).unwrap();
    state.gates.lock().unwrap()[&key].clone()
}

#[cfg(unix)]
#[test]
fn queued_calls_for_a_busy_repo_do_not_delay_another_repo() {
    let (dir_a, release) = repo_with_waiting_pre_commit();
    let dir_b = tempfile::TempDir::new().unwrap();
    git2::Repository::init(dir_b.path()).unwrap();
    let (state, keys) = state_with_repos(&[dir_a.path(), dir_b.path()]);
    let (started_tx, started_rx) = mpsc::channel();
    let commit = tauri::async_runtime::spawn(
        super::super::repo_worker(&state, &keys[0])
            .unwrap()
            .call(move |w| w.commit("msg".into(), false, Vec::new(), started_tx)),
    );
    assert!(matches!(
        started_rx.recv_timeout(std::time::Duration::from_secs(10)),
        Ok(HookEvent::Started { .. })
    ));
    // More calls than Tokio's blocking pool has threads (512), as repeated refreshes queue
    // up behind a long hook.
    let reads: Vec<_> = (0..600)
        .map(|_| {
            tauri::async_runtime::spawn(
                super::super::repo_worker(&state, &keys[0])
                    .unwrap()
                    .call(|w| w.get_status()),
            )
        })
        .collect();
    // Let the runtime start them before repository B asks.
    std::thread::sleep(std::time::Duration::from_millis(500));
    let other = super::super::repo_worker(&state, &keys[1]).unwrap();
    let (answered_tx, answered_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = tauri::async_runtime::block_on(other.call(|w| w.get_status()));
        let _ = answered_tx.send(result.is_ok());
    });

    let answered = answered_rx.recv_timeout(std::time::Duration::from_secs(5));

    std::fs::write(&release, "").unwrap();
    assert!(matches!(
        tauri::async_runtime::block_on(commit).unwrap(),
        Ok(git_core::commit::CommitOutcome::Committed { .. })
    ));
    for read in reads {
        assert!(tauri::async_runtime::block_on(read).unwrap().is_ok());
    }
    assert_eq!(
        answered,
        Ok(true),
        "calls queued for a busy repository delayed another repository"
    );
}

#[cfg(unix)]
#[test]
fn repo_worker_call_does_not_hold_a_runtime_thread_while_a_hook_runs() {
    let (dir, release) = repo_with_waiting_pre_commit();
    let (state, keys) = state_with_repos(&[dir.path()]);
    let (started_tx, started_rx) = mpsc::channel();
    let commit = tauri::async_runtime::spawn(
        super::super::repo_worker(&state, &keys[0])
            .unwrap()
            .call(move |w| w.commit("msg".into(), false, Vec::new(), started_tx)),
    );
    // The hook is running: the worker is busy until the release file appears.
    assert!(matches!(
        started_rx.recv_timeout(std::time::Duration::from_secs(10)),
        Ok(HookEvent::Started { .. })
    ));
    // Each read gets a gate of its own, so every one of them reaches `off_runtime` and waits
    // on the worker there: this checks the #125 property, not the #181 gate.
    let (_, handle) = state.registry.get(&keys[0]).unwrap();
    let reads: Vec<_> = (0..saturating_count())
        .map(|_| {
            tauri::async_runtime::spawn(
                super::super::RepoWorker::with_own_gate(handle.clone()).call(|w| w.get_status()),
            )
        })
        .collect();

    let responded = runtime_responds();

    std::fs::write(&release, "").unwrap();
    assert!(matches!(
        tauri::async_runtime::block_on(commit).unwrap(),
        Ok(git_core::commit::CommitOutcome::Committed { .. })
    ));
    for read in reads {
        assert!(tauri::async_runtime::block_on(read).unwrap().is_ok());
    }
    assert!(
        responded,
        "a queued worker call held an async runtime thread"
    );
}

/// Starts a commit whose `pre-commit` hook waits for `release`, and returns once the hook
/// runs, i.e. once the commit's call holds the repository's gate.
#[cfg(unix)]
fn hold_gate_with_hook(
    state: &super::super::AppState,
    key: &str,
) -> tauri::async_runtime::JoinHandle<Result<git_core::commit::CommitOutcome, String>> {
    let (started_tx, started_rx) = mpsc::channel();
    let commit = tauri::async_runtime::spawn(
        super::super::repo_worker(state, key)
            .unwrap()
            .call(move |w| w.commit("msg".into(), false, Vec::new(), started_tx)),
    );
    assert!(matches!(
        started_rx.recv_timeout(std::time::Duration::from_secs(10)),
        Ok(HookEvent::Started { .. })
    ));
    assert!(gate_of(state, key).try_lock().is_err());
    commit
}

/// #223: a second spelling of an open repository waits at the same gate.
#[cfg(unix)]
#[test]
fn two_spellings_share_one_gate() {
    let (dir, release) = repo_with_waiting_pre_commit();
    let (state, keys) = state_with_repos(&[dir.path()]);
    let slashed = format!("{}/", keys[0]);
    state.registry.open(&slashed).unwrap();
    let commit = hold_gate_with_hook(&state, &keys[0]);

    let via_other = super::super::repo_worker(&state, &slashed).unwrap();
    assert!(std::sync::Arc::ptr_eq(
        &via_other.gate,
        &gate_of(&state, &keys[0])
    ));
    let (answered_tx, answered_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = tauri::async_runtime::block_on(via_other.call(|w| w.get_status()));
        let _ = answered_tx.send(result.is_ok());
    });
    let while_held = answered_rx.recv_timeout(std::time::Duration::from_millis(500));

    std::fs::write(&release, "").unwrap();
    assert!(tauri::async_runtime::block_on(commit).unwrap().is_ok());
    assert!(
        while_held.is_err(),
        "a call through another spelling ran while the gate was held"
    );
    assert_eq!(
        answered_rx.recv_timeout(std::time::Duration::from_secs(10)),
        Ok(true)
    );
}

#[cfg(unix)]
#[test]
fn workdir_answers_while_a_hook_holds_the_gate() {
    let (dir, release) = repo_with_waiting_pre_commit();
    let (state, keys) = state_with_repos(&[dir.path()]);
    let commit = hold_gate_with_hook(&state, &keys[0]);

    let (tx, rx) = mpsc::channel();
    let workdir = std::thread::scope(|scope| {
        scope.spawn(|| {
            let _ = tx.send(super::super::tools::workdir_of(&state, &keys[0]));
        });
        rx.recv_timeout(std::time::Duration::from_secs(2))
    });

    std::fs::write(&release, "").unwrap();
    assert!(tauri::async_runtime::block_on(commit).unwrap().is_ok());
    let workdir = workdir.expect("workdir waited behind the gate").unwrap();
    assert_eq!(
        std::fs::canonicalize(workdir).unwrap(),
        std::fs::canonicalize(dir.path()).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn a_search_waiting_at_the_gate_is_cancelled_before_it_runs() {
    let (dir, release) = repo_with_waiting_pre_commit();
    let (state, keys) = state_with_repos(&[dir.path()]);
    let commit = hold_gate_with_hook(&state, &keys[0]);
    let search = tauri::async_runtime::spawn(
        super::super::search::queue_search(
            &state,
            &keys[0],
            "base".into(),
            git_core::graph::GraphSelection::default(),
            None,
        )
        .unwrap(),
    );

    super::super::repo_worker(&state, &keys[0])
        .unwrap()
        .signal()
        .cancel_search();
    std::fs::write(&release, "").unwrap();

    assert!(tauri::async_runtime::block_on(commit).unwrap().is_ok());
    assert_eq!(
        tauri::async_runtime::block_on(search).unwrap().unwrap_err(),
        "Search cancelled."
    );
    // A search started after the cancel runs normally.
    let page = tauri::async_runtime::block_on(
        super::super::search::queue_search(
            &state,
            &keys[0],
            "base".into(),
            git_core::graph::GraphSelection::default(),
            None,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(page.commits.len(), 1);
}

#[test]
fn calls_for_one_repo_keep_their_order() {
    use std::future::Future;
    use std::pin::Pin;
    use std::task::Poll;
    let dir = tempfile::TempDir::new().unwrap();
    git2::Repository::init(dir.path()).unwrap();
    let (state, keys) = state_with_repos(&[dir.path()]);
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    type Call = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;
    let mut calls: Vec<Option<Call>> = (0..10u64)
        .map(|i| {
            let seen = seen.clone();
            let call: Call = Box::pin(super::super::repo_worker(&state, &keys[0]).unwrap().call(
                move |w| {
                    // Earlier calls take longer: without the gate, later ones would overtake.
                    std::thread::sleep(std::time::Duration::from_millis((10 - i) * 10));
                    w.get_status()?;
                    seen.lock().unwrap().push(i);
                    Ok(())
                },
            ));
            Some(call)
        })
        .collect();

    // Poll the calls in the order they were made, as the commands they stand for arrive.
    tauri::async_runtime::block_on(std::future::poll_fn(|cx| {
        for slot in calls.iter_mut() {
            if let Some(call) = slot {
                if let Poll::Ready(result) = call.as_mut().poll(cx) {
                    assert_eq!(result, Ok(()));
                    *slot = None;
                }
            }
        }
        if calls.iter().all(Option::is_none) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }));

    assert_eq!(*seen.lock().unwrap(), (0..10).collect::<Vec<_>>());
}

#[test]
fn closing_a_repo_drops_its_gate() {
    let dir = tempfile::TempDir::new().unwrap();
    git2::Repository::init(dir.path()).unwrap();
    let (state, keys) = state_with_repos(&[dir.path()]);
    let first = super::super::repo_worker(&state, &keys[0]).unwrap();
    let again = super::super::repo_worker(&state, &keys[0]).unwrap();
    assert!(std::sync::Arc::ptr_eq(&first.gate, &again.gate));

    super::super::forget_repo(&state, &keys[0]);

    assert!(state.gates.lock().unwrap().is_empty());
    assert!(state.registry.is_empty());
    assert!(super::super::repo_worker(&state, &keys[0]).is_err());
}

/// #223: reopening a spelling that now names another repository drops the old one's gate.
#[cfg(unix)]
#[test]
fn reopening_a_spelling_that_names_another_repository_drops_the_old_gate() {
    let first = tempfile::TempDir::new().unwrap();
    git2::Repository::init(first.path()).unwrap();
    let second = tempfile::TempDir::new().unwrap();
    git2::Repository::init(second.path()).unwrap();
    let links = tempfile::TempDir::new().unwrap();
    let link = links.path().join("link");
    std::os::unix::fs::symlink(first.path(), &link).unwrap();
    let (state, keys) = state_with_repos(&[&link]);
    super::super::repo_worker(&state, &keys[0]).unwrap();
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(second.path(), &link).unwrap();

    super::super::remember_repo(&state, &keys[0]).unwrap();

    assert!(state.gates.lock().unwrap().is_empty());
    assert_eq!(state.registry.len(), 1);
    super::super::repo_worker(&state, &keys[0]).unwrap();
    let (key, _) = state.registry.get(&keys[0]).unwrap();
    assert_eq!(
        state.gates.lock().unwrap().keys().collect::<Vec<_>>(),
        [&key]
    );
}

/// #223: closing one spelling's tab keeps the repository's worker and gate for the other.
#[test]
fn closing_one_spelling_keeps_the_gate() {
    let dir = tempfile::TempDir::new().unwrap();
    git2::Repository::init(dir.path()).unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    let (state, keys) = state_with_repos(&[dir.path()]);
    let sub = dir.path().join("sub").to_string_lossy().into_owned();
    state.registry.open(&sub).unwrap();
    let first = super::super::repo_worker(&state, &keys[0]).unwrap();

    super::super::forget_repo(&state, &sub);

    let again = super::super::repo_worker(&state, &keys[0]).unwrap();
    assert!(std::sync::Arc::ptr_eq(&first.gate, &again.gate));
    assert_eq!(state.gates.lock().unwrap().len(), 1);
    assert!(super::super::repo_worker(&state, &sub).is_err());
    assert!(tauri::async_runtime::block_on(again.call(|w| w.get_status())).is_ok());
}

#[test]
fn two_open_repos_have_independent_worker_state() {
    use repo_service::worker::Worker;
    use std::collections::HashMap;

    let dir_a = tempfile::TempDir::new().unwrap();
    let repo_a = git2::Repository::init(dir_a.path()).unwrap();
    {
        let mut config = repo_a.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    std::fs::write(dir_a.path().join("a.txt"), "a").unwrap();

    let dir_b = tempfile::TempDir::new().unwrap();
    let repo_b = git2::Repository::init(dir_b.path()).unwrap();
    {
        let mut config = repo_b.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }

    let mut workers: HashMap<String, Worker> = HashMap::new();
    workers.insert(
        dir_a.path().to_string_lossy().into_owned(),
        Worker::spawn(dir_a.path().to_path_buf()).unwrap(),
    );
    workers.insert(
        dir_b.path().to_string_lossy().into_owned(),
        Worker::spawn(dir_b.path().to_path_buf()).unwrap(),
    );

    let handle_a = workers[&dir_a.path().to_string_lossy().into_owned()].handle();
    let handle_b = workers[&dir_b.path().to_string_lossy().into_owned()].handle();

    handle_a.stage_file("a.txt".to_string()).unwrap();

    assert_eq!(handle_a.get_status().unwrap().len(), 1);
    assert!(handle_a.get_status().unwrap()[0].staged);
    assert!(handle_b.get_status().unwrap().is_empty());
}
