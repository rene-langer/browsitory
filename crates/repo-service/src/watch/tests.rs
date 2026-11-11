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

use super::{RepoWatcher, RepoWatchers, SyncError, WatchError, WatchEvent, DEBOUNCE, MAX_FOLDERS};

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

/// A watcher is no longer alive by the time it reports `Stopped` (#797), so a `watch` called
/// right after (from the sink, or by a frontend that got the event) starts a new one instead of
/// keeping the stopped one. The sink holds the watcher's thread for a while after sending the
/// event, which makes the order "report, then mark stopped" fail every time.
#[cfg(target_os = "linux")]
#[test]
fn a_watcher_is_no_longer_alive_when_it_reports_stopped() {
    let (dir, _repo) = repo_with_commit();
    let (tx, events) = mpsc::channel();
    let watcher = RepoWatcher::start_with(dir.path(), 2, move |event| {
        let _ = tx.send(event);
        if event == WatchEvent::Stopped {
            std::thread::sleep(Duration::from_secs(1));
        }
    })
    .unwrap();

    std::fs::create_dir_all(dir.path().join("x/y/z")).unwrap();

    loop {
        match events.recv_timeout(TIMEOUT) {
            Ok(WatchEvent::Stopped) => break,
            Ok(WatchEvent::Changed) => continue,
            Err(error) => panic!("no Stopped event: {error}"),
        }
    }
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

#[test]
fn a_folder_deleted_and_made_again_is_watched_again() {
    let (dir, _repo) = repo_with_commit();
    let (_watcher, events) = start(dir.path());
    settle(&events);
    let sub = dir.path().join("sub");

    std::fs::create_dir(&sub).unwrap();
    assert_eq!(next(&events).0, WatchEvent::Changed);
    settle(&events);
    std::fs::remove_dir(&sub).unwrap();
    assert_eq!(next(&events).0, WatchEvent::Changed);
    settle(&events);
    std::fs::create_dir(&sub).unwrap();
    assert_eq!(next(&events).0, WatchEvent::Changed);
    settle(&events);
    std::fs::write(sub.join("f.txt"), "f\n").unwrap();

    assert_eq!(next(&events).0, WatchEvent::Changed);
}

/// Deleted folders stop counting towards the limit.
#[cfg(target_os = "linux")]
#[test]
fn deleted_folders_free_their_place_under_the_limit() {
    let (dir, _repo) = repo_with_commit();
    let (tx, events) = mpsc::channel();
    let _watcher = RepoWatcher::start_with(dir.path(), 2, move |event| {
        let _ = tx.send((event, Instant::now()));
    })
    .unwrap();
    settle(&events);

    for round in 0..3 {
        let folder = dir.path().join(format!("f{round}"));
        std::fs::create_dir(&folder).unwrap();
        assert_eq!(next(&events).0, WatchEvent::Changed);
        settle(&events);
        std::fs::remove_dir(&folder).unwrap();
        assert_eq!(next(&events).0, WatchEvent::Changed);
        settle(&events);
    }
    std::fs::create_dir(dir.path().join("last")).unwrap();

    assert_eq!(next(&events).0, WatchEvent::Changed);
}

#[test]
fn a_folder_no_longer_ignored_is_watched() {
    let (dir, _repo) = repo_with_commit();
    std::fs::create_dir_all(dir.path().join("build/x")).unwrap();
    let (_watcher, events) = start(dir.path());
    settle(&events);

    std::fs::write(dir.path().join(".gitignore"), "*.log\n").unwrap();
    assert_eq!(next(&events).0, WatchEvent::Changed);
    settle(&events);
    std::fs::write(dir.path().join("build/x/out.txt"), "o").unwrap();

    assert_eq!(next(&events).0, WatchEvent::Changed);
}

#[test]
fn a_folder_no_longer_excluded_is_watched() {
    let (dir, _repo) = repo_with_commit();
    let exclude = dir.path().join(".git/info/exclude");
    std::fs::create_dir_all(exclude.parent().unwrap()).unwrap();
    std::fs::write(&exclude, "local/\n").unwrap();
    std::fs::create_dir_all(dir.path().join("local/x")).unwrap();
    let (_watcher, events) = start(dir.path());
    settle(&events);

    std::fs::write(&exclude, "").unwrap();
    assert_eq!(next(&events).0, WatchEvent::Changed);
    settle(&events);
    std::fs::write(dir.path().join("local/x/out.txt"), "o").unwrap();

    assert_eq!(next(&events).0, WatchEvent::Changed);
}

#[test]
fn each_unwatch_moves_the_epoch_on() {
    let (watchers, _events) = collecting_watchers();

    watchers.unwatch("/r");
    let first = watchers.epoch("/r");
    watchers.unwatch("/r");

    assert_ne!(first, 0);
    assert_ne!(watchers.epoch("/r"), first);
}

#[test]
fn the_reasons_name_the_folder_limit() {
    let limit = MAX_FOLDERS.to_string();
    assert!(WatchError::TooManyFolders.to_string().contains(&limit));
    assert!(SyncError::TooMany.to_string().contains(&limit));
    assert_eq!(WatchError::Watcher("no".into()).to_string(), "no");
    assert_eq!(
        WatchError::NotARepository("none".into()).to_string(),
        "none"
    );
    assert_eq!(SyncError::Watch("full".into()).to_string(), "full");
}
