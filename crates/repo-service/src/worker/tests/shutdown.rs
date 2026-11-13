//! #965: when the desktop app exits, its workers finish the command they are running (a write
//! cut off by the process exit leaves `.git/index.lock` behind) and start no other.
use super::*;
use std::time::Instant;

#[test]
fn stopping_waits_for_the_running_write_and_starts_no_queued_command() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    write_file(dir.path(), "b.txt", "b");
    let (worker, pause) = Worker::spawn_with_pause_before_finish(dir.path().to_path_buf());
    let handle = worker.handle();

    pause.arm();
    let staging = {
        let handle = handle.clone();
        thread::spawn(move || handle.stage_file("a.txt".into()))
    };
    pause.wait_reached();
    let queued = {
        let handle = handle.clone();
        thread::spawn(move || handle.stage_file("b.txt".into()))
    };
    let stopping = thread::spawn(move || worker.stop(Duration::from_secs(5)));
    thread::sleep(Duration::from_millis(300));
    assert!(
        !stopping.is_finished(),
        "stop returned while the worker was still running a write"
    );

    pause.go();
    assert!(
        stopping.join().unwrap(),
        "the running write did not finish within the grace period"
    );
    assert_eq!(staging.join().unwrap(), Ok(()));
    assert!(
        queued.join().unwrap().is_err(),
        "a command queued behind the running one still ran after stop"
    );
    let index = repo.index().unwrap();
    assert!(index.get_path(Path::new("a.txt"), 0).is_some());
    assert!(index.get_path(Path::new("b.txt"), 0).is_none());
    assert!(!repo.path().join("index.lock").exists());
}

#[test]
fn stopping_gives_up_once_the_grace_period_is_over() {
    let (dir, _repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    let (worker, pause) = Worker::spawn_with_pause_before_finish(dir.path().to_path_buf());
    let handle = worker.handle();

    pause.arm();
    let staging = thread::spawn(move || handle.stage_file("a.txt".into()));
    pause.wait_reached();
    let started = Instant::now();
    let finished = worker.stop(Duration::from_millis(200));
    let waited = started.elapsed();

    assert!(!finished, "stop said the paused write had finished");
    assert!(
        waited >= Duration::from_millis(200),
        "stop gave up after {waited:?}, before the grace period was over"
    );
    assert!(waited < Duration::from_secs(3), "stop waited {waited:?}");
    pause.go();
    assert_eq!(staging.join().unwrap(), Ok(()));
}

#[test]
fn stopping_an_idle_worker_returns_at_once_and_it_runs_nothing_more() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    handle.get_status().unwrap();

    let started = Instant::now();
    assert!(worker.stop(Duration::from_secs(5)));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "stopping an idle worker took {:?}",
        started.elapsed()
    );

    assert!(handle.stage_file("a.txt".into()).is_err());
    assert!(repo
        .index()
        .unwrap()
        .get_path(Path::new("a.txt"), 0)
        .is_none());
}

#[test]
fn a_stopped_registry_opens_no_repository_and_its_workers_run_nothing() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    let path = dir.path().to_string_lossy().into_owned();
    let registry = WorkerRegistry::new();
    let (_, handle) = registry.open(&path).unwrap();
    handle.get_status().unwrap();

    assert!(registry.stop_all(Duration::from_secs(5)));

    assert!(
        registry.open(&path).is_err(),
        "opened a repository after stop_all"
    );
    assert!(handle.stage_file("a.txt".into()).is_err());
    assert!(repo
        .index()
        .unwrap()
        .get_path(Path::new("a.txt"), 0)
        .is_none());
}
