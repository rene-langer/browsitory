//! File system watchers for changes made outside the app (#756).
//!
//! The frontend checks a repository's change stamp (`git_core::change_stamp`) to see whether it
//! changed. Without a watcher it polls every few seconds; with one, it checks when the watcher
//! reports a change and polls only as a slow fallback (watchers miss changes on some network
//! file systems).
//!
//! A `RepoWatcher` follows what the stamp reads: the work tree, except the paths git ignores,
//! and the git folder's HEAD, index, references, stash list, configuration and the files of an
//! operation in progress (`paths`). Objects, most reflogs and lock files are left out, so a
//! fetch or a `git gc` reports nothing. Events are debounced: one `WatchEvent::Changed` once
//! things have been quiet for `DEBOUNCE`, or after `MAX_DELAY` of steady changes.
//!
//! On Linux, inotify watches one folder at a time, and the watches a user may hold are limited
//! and shared with every other program (an editor's watcher included). The watcher adds one
//! watch per folder git doesn't ignore (no `node_modules`, `target` or `.git/objects`), adds
//! watches for new folders as they appear, and gives up on a work tree with more than
//! `MAX_FOLDERS` such folders. On macOS and Windows one recursive watch covers the whole tree.
//!
//! When the watcher can't start, or stops later (the watch limit was reached), the frontend
//! goes back to polling every few seconds: `RepoWatchers::watch` returns `false`, or the sink
//! gets `WatchEvent::Stopped`.

mod paths;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use notify::event::{AccessKind, AccessMode, EventKind, ModifyKind, RenameMode};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use paths::{Layout, Place};

/// How long the watched paths must be quiet before a change is reported.
pub const DEBOUNCE: Duration = Duration::from_millis(300);
/// The longest a change waits to be reported while changes keep coming (a build writing files
/// the work tree doesn't ignore).
pub const MAX_DELAY: Duration = Duration::from_secs(2);
/// The most work tree folders a watcher follows on Linux (one inotify watch each). A larger
/// work tree is polled instead.
pub const MAX_FOLDERS: usize = 20_000;

/// What a watcher reports; on the wire as `RepoWatch.kind`, its variant name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]
#[cfg_attr(any(test, feature = "ts"), ts(rename = "RepoWatchKind"))]
pub enum WatchEvent {
    /// Something the change stamp reads may have changed.
    Changed,
    /// The watcher stopped (the watch limit was reached, or the work tree grew past
    /// `MAX_FOLDERS`): nothing more is reported for the repository.
    Stopped,
}

/// Why a watcher couldn't start.
#[derive(Debug)]
pub enum WatchError {
    /// The path isn't in a repository.
    NotARepository(String),
    /// The work tree has more than `MAX_FOLDERS` folders git doesn't ignore.
    TooManyFolders,
    /// The operating system refused (no inotify instance or watch left, a network file
    /// system without notifications, ...).
    Watcher(String),
}

impl std::fmt::Display for WatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotARepository(message) => write!(f, "{message}"),
            Self::TooManyFolders => write!(f, "more than {MAX_FOLDERS} folders to watch"),
            Self::Watcher(message) => write!(f, "{message}"),
        }
    }
}

type Shared = Arc<Mutex<Option<RecommendedWatcher>>>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Watches one repository until dropped.
pub struct RepoWatcher {
    watcher: Shared,
    alive: Arc<AtomicBool>,
}

impl RepoWatcher {
    /// Starts watching the repository at (or above) `path`. `on_event` runs on the watcher's
    /// own thread.
    pub fn start(
        path: &Path,
        on_event: impl Fn(WatchEvent) + Send + 'static,
    ) -> Result<Self, WatchError> {
        Self::start_with(path, MAX_FOLDERS, on_event)
    }

    fn start_with(
        path: &Path,
        max_folders: usize,
        on_event: impl Fn(WatchEvent) + Send + 'static,
    ) -> Result<Self, WatchError> {
        let repo =
            git_core::repo::open(path).map_err(|e| WatchError::NotARepository(e.to_string()))?;
        let layout = Layout::of(&repo);
        let (events_tx, events_rx) = mpsc::channel();
        let watcher = notify::recommended_watcher(events_tx)
            .map_err(|e| WatchError::Watcher(e.to_string()))?;
        let watcher: Shared = Arc::new(Mutex::new(Some(watcher)));
        let mut state = State {
            repo,
            layout,
            watcher: Arc::clone(&watcher),
            folders: HashSet::new(),
            work_folders: 0,
            max_folders,
        };
        state.watch_initial()?;
        let alive = Arc::new(AtomicBool::new(true));
        let thread_alive = Arc::clone(&alive);
        thread::Builder::new()
            .name("repo-watcher".into())
            .spawn(move || {
                let stopped = state.run(events_rx, &on_event);
                // Stopped before saying so (#797): a `watch` the sink (or a frontend that got the
                // event) calls right away must find this watcher dead and start a new one.
                // Dropping the notify watcher frees its watches at once.
                thread_alive.store(false, Ordering::SeqCst);
                lock(&state.watcher).take();
                if stopped {
                    on_event(WatchEvent::Stopped);
                }
            })
            .map_err(|e| WatchError::Watcher(e.to_string()))?;
        Ok(Self { watcher, alive })
    }

    /// Whether the watcher still reports changes (it hasn't stopped).
    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }
}

impl Drop for RepoWatcher {
    fn drop(&mut self) {
        // Dropping the notify watcher closes its event channel, which ends the thread.
        lock(&self.watcher).take();
    }
}

/// Why the event loop ends early.
struct Stop;

struct State {
    repo: git2::Repository,
    layout: Layout,
    watcher: Shared,
    /// The folders watched one by one (Linux).
    folders: HashSet<PathBuf>,
    /// How many of `folders` are in the work tree (the git folder's don't count).
    work_folders: usize,
    max_folders: usize,
}

/// Whether watches cover only the folder itself, not its subfolders (inotify).
const PER_FOLDER: bool = cfg!(any(target_os = "linux", target_os = "android"));

impl State {
    fn watch_initial(&mut self) -> Result<(), WatchError> {
        if PER_FOLDER {
            return self.sync().map_err(|error| match error {
                SyncError::TooMany => WatchError::TooManyFolders,
                SyncError::Watch(message) => WatchError::Watcher(message),
            });
        }
        let mut roots: Vec<PathBuf> = Vec::new();
        let layout = &self.layout;
        for root in layout
            .workdir
            .iter()
            .chain([&layout.git_dir, &layout.common_dir])
        {
            if !roots.iter().any(|watched| root.starts_with(watched)) {
                roots.push(root.clone());
            }
        }
        let mut guard = lock(&self.watcher);
        let watcher = guard
            .as_mut()
            .expect("the watcher lives until the thread ends");
        for root in roots {
            watcher
                .watch(&root, RecursiveMode::Recursive)
                .map_err(|e| WatchError::Watcher(e.to_string()))?;
        }
        Ok(())
    }

    /// Adds a watch for every folder that should have one and doesn't (Linux): the whole work
    /// tree and git folders at the start, and again when the ignore rules change.
    fn sync(&mut self) -> Result<(), SyncError> {
        let roots: Vec<PathBuf> = self
            .layout
            .workdir
            .iter()
            .chain([&self.layout.git_dir, &self.layout.common_dir])
            .cloned()
            .collect();
        for root in roots {
            self.add_folder_tree(&root)?;
        }
        Ok(())
    }

    /// Watches `folder` and the folders below it that should be watched (Linux).
    fn add_folder_tree(&mut self, folder: &Path) -> Result<(), SyncError> {
        let mut pending = vec![folder.to_path_buf()];
        while let Some(folder) = pending.pop() {
            if !self.wants_folder(&folder) {
                continue;
            }
            if self.folders.insert(folder.clone()) {
                if self.is_work_folder(&folder) {
                    self.work_folders += 1;
                    if self.work_folders > self.max_folders {
                        return Err(SyncError::TooMany);
                    }
                }
                let mut guard = lock(&self.watcher);
                let Some(watcher) = guard.as_mut() else {
                    return Ok(());
                };
                match watcher.watch(&folder, RecursiveMode::NonRecursive) {
                    Ok(()) => {}
                    Err(error) if is_limit(&error) => {
                        return Err(SyncError::Watch(error.to_string()))
                    }
                    // Gone again, or unreadable: nothing to follow there.
                    Err(_) => {
                        drop(guard);
                        self.forget_folder_tree(&folder);
                        continue;
                    }
                }
            }
            let Ok(entries) = std::fs::read_dir(&folder) else {
                continue;
            };
            for entry in entries.flatten() {
                // `file_type` doesn't follow a symbolic link: a linked folder isn't walked.
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    pending.push(entry.path());
                }
            }
        }
        Ok(())
    }

    fn is_work_folder(&self, folder: &Path) -> bool {
        matches!(self.layout.place(folder), Place::Work(_))
    }

    /// Whether `folder` should have a watch of its own (Linux).
    fn wants_folder(&self, folder: &Path) -> bool {
        match self.layout.place(folder) {
            Place::Git(relative) => paths::is_watched_git_folder(relative),
            Place::Work(relative) => relative.as_os_str().is_empty() || !self.is_ignored(relative),
            Place::Outside => false,
        }
    }

    fn is_ignored(&self, relative: &Path) -> bool {
        self.repo.status_should_ignore(relative).unwrap_or(false)
    }

    /// Forgets the watches of `folder` and the folders below it: it was deleted or moved away.
    fn forget_folder_tree(&mut self, folder: &Path) {
        if !self.folders.contains(folder) {
            return;
        }
        let gone: Vec<PathBuf> = self
            .folders
            .iter()
            .filter(|watched| watched.starts_with(folder))
            .cloned()
            .collect();
        let mut guard = lock(&self.watcher);
        for watched in gone {
            self.folders.remove(&watched);
            if matches!(self.layout.place(&watched), Place::Work(_)) {
                self.work_folders -= 1;
            }
            if let Some(watcher) = guard.as_mut() {
                let _ = watcher.unwatch(&watched);
            }
        }
    }

    /// Reports changes until the watcher is dropped (false) or has to stop (true); the caller
    /// reports `Stopped`.
    fn run(
        &mut self,
        events: mpsc::Receiver<notify::Result<notify::Event>>,
        on_event: &dyn Fn(WatchEvent),
    ) -> bool {
        loop {
            let Ok(first) = events.recv() else {
                return false;
            };
            let relevant = match self.handle(first) {
                Ok(relevant) => relevant,
                Err(Stop) => return true,
            };
            if !relevant {
                continue;
            }
            let started = Instant::now();
            loop {
                let left = MAX_DELAY.saturating_sub(started.elapsed());
                if left.is_zero() {
                    break;
                }
                match events.recv_timeout(DEBOUNCE.min(left)) {
                    Ok(event) => {
                        if self.handle(event).is_err() {
                            return true;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return false,
                }
            }
            on_event(WatchEvent::Changed);
        }
    }

    /// Follows one event: keeps the watches up to date (Linux) and says whether it may change
    /// the stamp.
    fn handle(&mut self, event: notify::Result<notify::Event>) -> Result<bool, Stop> {
        let event = match event {
            Ok(event) => event,
            Err(error) if is_limit(&error) => {
                eprintln!("browsitory: stopped watching for changes: {error}");
                return Err(Stop);
            }
            // Something went wrong on the operating system's side: check, to be safe.
            Err(_) => return Ok(true),
        };
        if event.need_rescan() {
            // Events were lost (the queue overflowed): new folders may lack their watches.
            self.resync()?;
            return Ok(true);
        }
        if is_read(&event.kind) {
            return Ok(false);
        }
        let mut relevant = false;
        let mut rules_changed = false;
        for path in &event.paths {
            if PER_FOLDER {
                self.follow_folders(&event.kind, path)?;
            }
            match self.layout.place(path) {
                Place::Git(relative) => {
                    relevant |= paths::is_relevant_git_path(relative);
                    rules_changed |= relative == Path::new("info/exclude");
                }
                Place::Work(relative) => {
                    if relative.as_os_str().is_empty() {
                        continue;
                    }
                    rules_changed |= paths::changes_ignore_rules(relative);
                    relevant |= !self.is_ignored(relative);
                }
                Place::Outside => {}
            }
        }
        if rules_changed && PER_FOLDER {
            self.resync()?;
        }
        Ok(relevant)
    }

    fn resync(&mut self) -> Result<(), Stop> {
        if !PER_FOLDER {
            return Ok(());
        }
        self.sync().map_err(|error| {
            eprintln!("browsitory: stopped watching for changes: {error}");
            Stop
        })
    }

    /// Adds watches for a folder that appeared, forgets those of one that went away (Linux).
    fn follow_folders(&mut self, kind: &EventKind, path: &Path) -> Result<(), Stop> {
        match kind {
            EventKind::Remove(_) | EventKind::Modify(ModifyKind::Name(RenameMode::From)) => {
                self.forget_folder_tree(path);
            }
            EventKind::Create(_) | EventKind::Modify(ModifyKind::Name(_)) => {
                if path.is_dir() {
                    self.add_folder_tree(path).map_err(|error| {
                        eprintln!("browsitory: stopped watching for changes: {error}");
                        Stop
                    })?;
                } else if !path.exists() {
                    self.forget_folder_tree(path);
                }
            }
            _ => {}
        }
        Ok(())
    }
}

enum SyncError {
    TooMany,
    Watch(String),
}

impl std::fmt::Display for SyncError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooMany => write!(f, "more than {MAX_FOLDERS} folders to watch"),
            Self::Watch(message) => write!(f, "{message}"),
        }
    }
}

fn is_limit(error: &notify::Error) -> bool {
    matches!(error.kind, notify::ErrorKind::MaxFilesWatch)
}

/// A file was only opened or read (the app's own reads included), not changed.
fn is_read(kind: &EventKind) -> bool {
    matches!(kind, EventKind::Access(access) if *access != AccessKind::Close(AccessMode::Write))
}

/// Reports a repository's changes for one opened path spelling.
pub type WatchSink = Arc<dyn Fn(&str, WatchEvent) + Send + Sync>;

/// The watchers of the repositories the frontend shows (#756), by the path it opened them with.
/// Both transports keep one: `watch` when a tab's workspace mounts, `unwatch` when it unmounts
/// or the repository is closed.
pub struct RepoWatchers {
    sink: WatchSink,
    inner: Mutex<Watched>,
}

#[derive(Default)]
struct Watched {
    watchers: HashMap<String, RepoWatcher>,
    /// Bumped by each `unwatch`, so a `watch` still starting when its path is unwatched drops
    /// its watcher instead of keeping it.
    epochs: HashMap<String, u64>,
}

impl RepoWatchers {
    pub fn new(sink: WatchSink) -> Self {
        Self {
            sink,
            inner: Mutex::default(),
        }
    }

    /// Watches the repository at `repo_path` (again, when its watcher stopped). Returns whether
    /// it is watched; when not, the reason is logged and the caller polls instead.
    pub fn watch(&self, repo_path: &str) -> bool {
        let epoch = {
            let inner = lock(&self.inner);
            if inner
                .watchers
                .get(repo_path)
                .is_some_and(RepoWatcher::is_alive)
            {
                return true;
            }
            inner.epochs.get(repo_path).copied().unwrap_or(0)
        };
        let sink = Arc::clone(&self.sink);
        let path = repo_path.to_string();
        // Started unlocked: walking a large work tree takes a while.
        let started = RepoWatcher::start(Path::new(repo_path), move |event| sink(&path, event));
        let watcher = match started {
            Ok(watcher) => watcher,
            Err(error) => {
                eprintln!(
                    "browsitory: not watching {repo_path} for changes ({error}); \
                     checking every few seconds instead"
                );
                return false;
            }
        };
        let mut inner = lock(&self.inner);
        if inner.epochs.get(repo_path).copied().unwrap_or(0) != epoch {
            return false;
        }
        // Another `watch` of the same path may have finished first; either watcher will do.
        let replaced = inner.watchers.insert(repo_path.to_string(), watcher);
        drop(inner);
        drop(replaced);
        true
    }

    /// Stops watching `repo_path`; a no-op when it isn't watched.
    pub fn unwatch(&self, repo_path: &str) {
        let removed = {
            let mut inner = lock(&self.inner);
            *inner.epochs.entry(repo_path.to_string()).or_default() += 1;
            inner.watchers.remove(repo_path)
        };
        drop(removed);
    }

    #[cfg(test)]
    fn epoch(&self, repo_path: &str) -> u64 {
        lock(&self.inner)
            .epochs
            .get(repo_path)
            .copied()
            .unwrap_or(0)
    }

    /// Whether `repo_path` has a live watcher.
    pub fn is_watching(&self, repo_path: &str) -> bool {
        lock(&self.inner)
            .watchers
            .get(repo_path)
            .is_some_and(RepoWatcher::is_alive)
    }
}
