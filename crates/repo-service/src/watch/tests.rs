//! The watcher against real temp-dir repositories. Each test waits for what it expects with a
//! generous timeout and never sleeps for an exact time, so a slow CI runner only makes it slower.
//! Where a test checks that something is *not* reported, it makes that change first and then a
//! change that is reported, and checks that the first event came after the second change.

use std::path::Path;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use git2::Repository;
use tempfile::TempDir;

use super::{RepoWatcher, RepoWatchers, WatchError, WatchEvent, DEBOUNCE};

/// Far longer than an event ever takes, so a busy runner doesn't fail the test.
const TIMEOUT: Duration = Duration::from_secs(20);

fn repo_with_commit() -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join(".gitignore"), "build/\n*.log\n").unwrap();
    std::fs::write(dir.path().join("a.txt"), "a\n").unwrap();
    {
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.add_path(Path::new(".gitignore")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let signature = git2::Signature::now("Test User", "test@example.com").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "base", &tree, &[])
            .unwrap();
    }
    (dir, repo)
}

/// The events of a watcher, with the time each arrived.
type Events = mpsc::Receiver<(WatchEvent, Instant)>;

fn start(path: &Path) -> (RepoWatcher, Events) {
    let (tx, rx) = mpsc::channel();
    let watcher = RepoWatcher::start(path, move |event| {
        let _ = tx.send((event, Instant::now()));
    })
    .unwrap();
    (watcher, rx)
}

fn next(events: &Events) -> (WatchEvent, Instant) {
    events
        .recv_timeout(TIMEOUT)
        .expect("the watcher reports a change")
}

/// Waits until nothing has been reported for a while, so the next event is caused by what the
/// test does next (a watch may report its own start on some platforms).
fn settle(events: &Events) {
    while events.recv_timeout(DEBOUNCE * 4).is_ok() {}
}

#[test]
fn a_new_file_in_the_work_tree_is_reported() {
    let (dir, _repo) = repo_with_commit();
    let (_watcher, events) = start(dir.path());
    settle(&events);

    std::fs::write(dir.path().join("new.txt"), "new\n").unwrap();

    assert_eq!(next(&events).0, WatchEvent::Changed);
}

#[test]
fn a_folder_created_after_the_start_is_watched_too() {
    let (dir, _repo) = repo_with_commit();
    let (_watcher, events) = start(dir.path());
    settle(&events);

    std::fs::create_dir_all(dir.path().join("src/deep")).unwrap();
    assert_eq!(next(&events).0, WatchEvent::Changed);
    settle(&events);
    std::fs::write(dir.path().join("src/deep/lib.rs"), "fn f() {}\n").unwrap();

    assert_eq!(next(&events).0, WatchEvent::Changed);
}

#[test]
fn changes_git_ignores_are_not_reported() {
    let (dir, _repo) = repo_with_commit();
    let (_watcher, events) = start(dir.path());
    settle(&events);

    std::fs::create_dir(dir.path().join("build")).unwrap();
    std::fs::write(dir.path().join("build/out.o"), "o").unwrap();
    std::fs::write(dir.path().join("debug.log"), "log").unwrap();
    // Well past the debounce, so an event for the above would have come by now on any machine
    // that isn't badly overloaded.
    std::thread::sleep(DEBOUNCE * 4);
    let reported_change = Instant::now();
    std::fs::write(dir.path().join("a.txt"), "changed\n").unwrap();

    let (event, at) = next(&events);
    assert_eq!(event, WatchEvent::Changed);
    assert!(at >= reported_change, "an ignored change was reported");
}

#[test]
fn a_new_commit_is_reported() {
    let (dir, repo) = repo_with_commit();
    let (_watcher, events) = start(dir.path());
    settle(&events);

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let signature = git2::Signature::now("Test User", "test@example.com").unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "second",
        &head.tree().unwrap(),
        &[&head],
    )
    .unwrap();

    assert_eq!(next(&events).0, WatchEvent::Changed);
}

#[test]
fn new_objects_alone_are_not_reported() {
    let (dir, repo) = repo_with_commit();
    let (_watcher, events) = start(dir.path());
    settle(&events);

    repo.blob(b"only an object").unwrap();
    std::fs::write(dir.path().join(".git/FETCH_HEAD"), "x\n").unwrap();
    std::thread::sleep(DEBOUNCE * 4);
    let reported_change = Instant::now();
    repo.reference(
        "refs/heads/other",
        repo.head().unwrap().target().unwrap(),
        false,
        "test",
    )
    .unwrap();

    let (event, at) = next(&events);
    assert_eq!(event, WatchEvent::Changed);
    assert!(at >= reported_change, "a new object was reported");
}

#[test]
fn a_dropped_watcher_reports_nothing_more() {
    let (dir, _repo) = repo_with_commit();
    let (watcher, events) = start(dir.path());

    drop(watcher);

    // The watcher's thread ends and drops its callback, which closes the channel.
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(_) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => panic!("the watcher's thread didn't end"),
        }
    }
}

#[test]
fn a_path_outside_a_repository_is_not_watched() {
    let dir = TempDir::new().unwrap();

    let result = RepoWatcher::start(&dir.path().join("missing"), |_| {});

    assert!(matches!(result, Err(WatchError::NotARepository(_))));
}

/// inotify needs a watch per folder; the other platforms watch the tree with one.
#[cfg(target_os = "linux")]
#[test]
fn a_work_tree_with_too_many_folders_is_not_watched() {
    let (dir, _repo) = repo_with_commit();
    for name in ["one", "two", "three", "four"] {
        std::fs::create_dir(dir.path().join(name)).unwrap();
    }
    // Ignored folders don't count.
    std::fs::create_dir_all(dir.path().join("build/a/b/c/d")).unwrap();

    let too_many = RepoWatcher::start_with(dir.path(), 4, |_| {});
    let enough = RepoWatcher::start_with(dir.path(), 5, |_| {});

    assert!(matches!(too_many, Err(WatchError::TooManyFolders)));
    assert!(enough.is_ok());
}

#[cfg(target_os = "linux")]
#[test]
fn growing_past_the_folder_limit_stops_the_watcher() {
    let (dir, _repo) = repo_with_commit();
    let (tx, events) = mpsc::channel();
    let watcher = RepoWatcher::start_with(dir.path(), 2, move |event| {
        let _ = tx.send((event, Instant::now()));
    })
    .unwrap();

    std::fs::create_dir_all(dir.path().join("x/y/z")).unwrap();

    let stopped = loop {
        match events.recv_timeout(TIMEOUT) {
            Ok((WatchEvent::Stopped, _)) => break true,
            Ok(_) => continue,
            Err(_) => break false,
        }
    };
    assert!(stopped);
    assert!(!watcher.is_alive());
}

#[test]
fn a_linked_worktree_reports_its_own_changes() {
    let (dir, repo) = repo_with_commit();
    let parent = TempDir::new().unwrap();
    let linked = parent.path().join("wt");
    repo.worktree("wt", &linked, None).unwrap();
    let (_watcher, events) = start(&linked);
    settle(&events);

    std::fs::write(linked.join("new.txt"), "new\n").unwrap();
    assert_eq!(next(&events).0, WatchEvent::Changed);
    settle(&events);
    // A branch made in the main work tree shows in the linked one's branch list.
    repo.reference(
        "refs/heads/from-main",
        repo.head().unwrap().target().unwrap(),
        false,
        "test",
    )
    .unwrap();
    assert_eq!(next(&events).0, WatchEvent::Changed);
    drop(dir);
}

fn collecting_watchers() -> (RepoWatchers, mpsc::Receiver<(String, WatchEvent)>) {
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let watchers = RepoWatchers::new(Arc::new(move |path: &str, event| {
        let _ = tx.lock().unwrap().send((path.to_string(), event));
    }));
    (watchers, rx)
}

#[test]
fn repo_watchers_report_the_path_the_repository_was_watched_with() {
    let (dir, _repo) = repo_with_commit();
    let (watchers, events) = collecting_watchers();
    let path = format!("{}{}", dir.path().display(), std::path::MAIN_SEPARATOR);

    assert!(watchers.watch(&path));
    assert!(watchers.watch(&path), "watching again keeps the watcher");
    assert!(watchers.is_watching(&path));
    while events.recv_timeout(DEBOUNCE * 4).is_ok() {}
    std::fs::write(dir.path().join("new.txt"), "new\n").unwrap();

    let (reported, event) = events.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(reported, path);
    assert_eq!(event, WatchEvent::Changed);
}

#[test]
fn repo_watchers_stop_on_unwatch() {
    let (dir, _repo) = repo_with_commit();
    let (watchers, _events) = collecting_watchers();
    let path = dir.path().display().to_string();
    watchers.watch(&path);

    watchers.unwatch(&path);
    watchers.unwatch(&path);

    assert!(!watchers.is_watching(&path));
}

#[test]
fn repo_watchers_do_not_watch_a_folder_outside_a_repository() {
    let dir = TempDir::new().unwrap();
    let (watchers, _events) = collecting_watchers();
    let path = dir.path().join("missing").display().to_string();

    assert!(!watchers.watch(&path));
    assert!(!watchers.is_watching(&path));
}
