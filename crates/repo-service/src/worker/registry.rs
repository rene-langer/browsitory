//! One worker per repository, whatever path a caller spells it with (#223).
//!
//! Callers (the Tauri `AppState`, the VSCode sidecar's `Repos`) name a repository by the path
//! string the frontend sent. One repository can be reached through several spellings: a
//! trailing slash, a subfolder, `..` segments, a symbolic link, or a different letter case on a
//! case-insensitive file system. Keyed by that raw string, each spelling got its own worker, so
//! two commands against the same repository could run at the same time on two threads.
//!
//! `WorkerRegistry` keys workers by the repository's canonical root instead (`repo_key`) and
//! remembers which spellings were opened (`aliases`). Lookups (`get`) only consult the aliases,
//! never the file system, so a command is as cheap as before. Closing a spelling drops the
//! worker only when no other spelling still refers to it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use super::{Worker, WorkerHandle};

/// The canonical identity of the repository at (or above) `path`: its work tree root, or its
/// git folder for a bare repository, canonicalized (symbolic links resolved, `..` removed, and
/// the on-disk letter case on Windows and macOS). Falls back to the root with trailing
/// separators trimmed when it can't be canonicalized.
///
/// A linked worktree has its own work tree root, so it gets its own key (and worker).
///
/// Errors with the same text `Worker::spawn` gives for a path that isn't in a repository.
pub fn repo_key(path: &Path) -> Result<String, String> {
    discover(path).map(|(_, key)| key)
}

/// The repository's root as discovered from `path` (not canonicalized, so no `\\?\` prefix
/// on Windows) and its key.
fn discover(path: &Path) -> Result<(PathBuf, String), String> {
    let repo = git_core::repo::open(path).map_err(|e| e.to_string())?;
    let root = repo.workdir().unwrap_or_else(|| repo.path()).to_path_buf();
    let key = match std::fs::canonicalize(&root) {
        Ok(canonical) => canonical,
        Err(_) => trim_trailing_separators(&root),
    };
    Ok((root, key.to_string_lossy().into_owned()))
}

fn trim_trailing_separators(path: &Path) -> PathBuf {
    // `components()` drops trailing separators (and `.` segments) but keeps a root as is.
    path.components().collect()
}

#[derive(Default)]
struct Inner {
    /// Workers by repository key.
    workers: HashMap<String, Worker>,
    /// Each opened spelling and the key of the repository it names.
    aliases: HashMap<String, String>,
}

impl Inner {
    /// Drops `key`'s worker (if any) when no spelling refers to it any more. Returns whether
    /// `key` is now unreferenced.
    fn drop_if_unreferenced(&mut self, key: &str) -> bool {
        if self.aliases.values().any(|k| k == key) {
            return false;
        }
        self.workers.remove(key);
        true
    }
}

/// The open repositories' workers, one per repository (see the module comment).
///
/// The lock is held only for map lookups and updates, never across `Worker::spawn` (which opens
/// a `git2::Repository` and can be slow) or a worker round trip, so a repository that is slow to
/// open can't block requests against another, already open one.
#[derive(Default)]
pub struct WorkerRegistry {
    inner: Mutex<Inner>,
}

impl WorkerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Opens `path` and returns its repository key and a live worker handle.
    ///
    /// Always recomputes the key, so a spelling that now names another repository (the folder
    /// was replaced) follows it. Reuses the repository's worker when it is alive; a dead one
    /// (its thread exited, e.g. after a panic inside a `Command` match arm,
    /// AUD-2026-09-05-CONC-001) is evicted and respawned, so a repository can't stay wedged
    /// behind a dead handle.
    ///
    /// The worker is spawned from the repository's discovered root, not from `path`: it reopens
    /// the repository from there after worktree commands, and `path` (say a subfolder) may be
    /// gone by then while another spelling still uses the worker.
    pub fn open(&self, path: &str) -> Result<(String, WorkerHandle), String> {
        self.open_with(path, |_| {})
    }

    /// Like `open`, but runs `on_drop` (with the registry still locked, see `get_with`) for each
    /// key this call leaves without a worker: the repository `path` named before, when it now
    /// names another one and nothing else names the old one; or `path`'s own repository, when
    /// its dead worker was evicted and the respawn failed.
    pub fn open_with(
        &self,
        path: &str,
        mut on_drop: impl FnMut(&str),
    ) -> Result<(String, WorkerHandle), String> {
        let (root, key) = discover(Path::new(path))?;
        {
            let mut inner = self.lock();
            if let Some(worker) = inner.workers.get(&key) {
                let handle = worker.handle();
                if handle.is_alive() {
                    Self::record_alias(&mut inner, path, &key, &mut on_drop);
                    return Ok((key, handle));
                }
                inner.workers.remove(&key);
            }
        }

        let worker = match Worker::spawn(root) {
            Ok(worker) => worker,
            Err(error) => {
                let inner = self.lock();
                if !inner.workers.contains_key(&key) {
                    on_drop(&key);
                }
                return Err(error);
            }
        };
        let mut inner = self.lock();
        // Another caller may have opened the same repository (through any spelling) while this
        // one was spawning, unlocked. Prefer the worker already in the map: this one is dropped
        // and its idle thread exits.
        let handle = inner.workers.entry(key.clone()).or_insert(worker).handle();
        Self::record_alias(&mut inner, path, &key, &mut on_drop);
        Ok((key, handle))
    }

    fn record_alias(inner: &mut Inner, path: &str, key: &str, on_drop: &mut dyn FnMut(&str)) {
        if let Some(previous) = inner.aliases.insert(path.to_string(), key.to_string()) {
            if previous != key && inner.drop_if_unreferenced(&previous) {
                on_drop(&previous);
            }
        }
    }

    /// The repository key and worker handle for an opened spelling. Doesn't touch the file
    /// system: a spelling that was never opened (or was closed) is `None`, even when it names
    /// an open repository.
    pub fn get(&self, path: &str) -> Option<(String, WorkerHandle)> {
        self.get_with(path, |key, handle| (key.to_string(), handle))
    }

    /// Like `get`, but runs `f` with the key and handle while the registry is still locked, so
    /// a caller keeping per-repository state of its own (the Tauri app's gates) can't race a
    /// `close_with` of the same repository. `f` must be quick and must not call back into the
    /// registry.
    pub fn get_with<R>(&self, path: &str, f: impl FnOnce(&str, WorkerHandle) -> R) -> Option<R> {
        let inner = self.lock();
        let key = inner.aliases.get(path)?;
        let handle = inner.workers.get(key)?.handle();
        Some(f(key, handle))
    }

    /// Forgets the spelling `path`. When no other spelling refers to its repository, drops the
    /// worker (if it still has one) and returns the key. Calls already holding a handle finish
    /// as usual.
    pub fn close(&self, path: &str) -> Option<String> {
        self.close_with(path, |_| {})
    }

    /// Like `close`, but runs `on_drop` with the key while the registry is still locked, when
    /// no spelling refers to the repository any more (see `get_with`).
    pub fn close_with(&self, path: &str, on_drop: impl FnOnce(&str)) -> Option<String> {
        let mut inner = self.lock();
        let key = inner.aliases.remove(path)?;
        if inner.drop_if_unreferenced(&key) {
            on_drop(&key);
            Some(key)
        } else {
            None
        }
    }

    /// The number of workers (open repositories).
    pub fn len(&self) -> usize {
        self.lock().workers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use git2::Repository;
    use tempfile::TempDir;

    use super::{repo_key, WorkerRegistry};

    /// A repository with one commit (so a linked worktree can be added) and a subfolder `sub`.
    fn repo_with_commit() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("f.txt"), "f\n").unwrap();
        {
            let mut index = repo.index().unwrap();
            index.add_path(Path::new("sub/f.txt")).unwrap();
            index.write().unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let signature = git2::Signature::now("Test User", "test@example.com").unwrap();
            repo.commit(Some("HEAD"), &signature, &signature, "base", &tree, &[])
                .unwrap();
        }
        (dir, repo)
    }

    fn spelling(path: &Path) -> String {
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn spellings_of_one_repository_share_one_worker() {
        let (dir, _repo) = repo_with_commit();
        let registry = WorkerRegistry::new();
        let plain = spelling(dir.path());
        let spellings = [
            plain.clone(),
            format!("{plain}{}", std::path::MAIN_SEPARATOR),
            spelling(&dir.path().join("sub").join("..")),
            spelling(&dir.path().join("sub")),
        ];

        let keys: Vec<String> = spellings
            .iter()
            .map(|path| registry.open(path).unwrap().0)
            .collect();

        assert!(keys.iter().all(|key| *key == keys[0]), "{keys:?}");
        assert_eq!(registry.len(), 1);
        for path in &spellings {
            let (key, handle) = registry.get(path).unwrap();
            assert_eq!(key, keys[0]);
            assert!(handle.get_status().is_ok());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_path_shares_the_worker() {
        let (dir, _repo) = repo_with_commit();
        let links = TempDir::new().unwrap();
        let link = links.path().join("link");
        std::os::unix::fs::symlink(dir.path(), &link).unwrap();
        let registry = WorkerRegistry::new();

        let (direct, _) = registry.open(&spelling(dir.path())).unwrap();
        let (linked, _) = registry.open(&spelling(&link)).unwrap();

        assert_eq!(direct, linked);
        assert_eq!(registry.len(), 1);
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn a_case_variant_shares_the_worker() {
        let parent = TempDir::new().unwrap();
        let root = parent.path().join("CaseRepo");
        Repository::init(&root).unwrap();
        let registry = WorkerRegistry::new();

        let (original, _) = registry.open(&spelling(&root)).unwrap();
        let (variant, _) = registry
            .open(&spelling(&parent.path().join("caserepo")))
            .unwrap();

        assert_eq!(original, variant);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn a_linked_worktree_gets_its_own_worker() {
        let (dir, repo) = repo_with_commit();
        let parent = TempDir::new().unwrap();
        let linked = parent.path().join("wt");
        repo.worktree("wt", &linked, None).unwrap();
        let registry = WorkerRegistry::new();

        let (main_key, _) = registry.open(&spelling(dir.path())).unwrap();
        let (linked_key, _) = registry.open(&spelling(&linked)).unwrap();

        assert_ne!(main_key, linked_key);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn closing_one_spelling_keeps_the_worker_for_the_other() {
        let (dir, _repo) = repo_with_commit();
        let registry = WorkerRegistry::new();
        let plain = spelling(dir.path());
        let slashed = format!("{plain}{}", std::path::MAIN_SEPARATOR);
        registry.open(&plain).unwrap();
        registry.open(&slashed).unwrap();

        assert_eq!(registry.close(&slashed), None);

        assert_eq!(registry.len(), 1);
        assert!(registry.get(&slashed).is_none());
        let (_, handle) = registry.get(&plain).unwrap();
        assert!(handle.get_status().is_ok());
    }

    #[test]
    fn closing_the_last_spelling_drops_the_worker() {
        let (dir, _repo) = repo_with_commit();
        let registry = WorkerRegistry::new();
        let plain = spelling(dir.path());
        let sub = spelling(&dir.path().join("sub"));
        let (key, _) = registry.open(&plain).unwrap();
        registry.open(&sub).unwrap();

        assert_eq!(registry.close(&plain), None);
        assert_eq!(registry.close(&sub), Some(key));

        assert!(registry.is_empty());
        assert!(registry.get(&sub).is_none());
        // Closing again (or a spelling never opened) is a no-op.
        assert_eq!(registry.close(&sub), None);
    }

    #[test]
    fn close_with_runs_only_when_the_worker_is_dropped() {
        let (dir, _repo) = repo_with_commit();
        let registry = WorkerRegistry::new();
        let plain = spelling(dir.path());
        let sub = spelling(&dir.path().join("sub"));
        registry.open(&plain).unwrap();
        registry.open(&sub).unwrap();
        let mut dropped = Vec::new();

        registry.close_with(&plain, |key| dropped.push(key.to_string()));
        assert!(dropped.is_empty());
        registry.close_with(&sub, |key| dropped.push(key.to_string()));

        assert_eq!(dropped, [repo_key(dir.path()).unwrap()]);
    }

    /// The worker reopens its repository after worktree commands; it must do so from the
    /// repository's root, not from the first opener's spelling, which may stop existing.
    #[test]
    fn the_worker_outlives_the_folder_its_first_spelling_named() {
        let (dir, _repo) = repo_with_commit();
        let registry = WorkerRegistry::new();
        let plain = spelling(dir.path());
        let sub = spelling(&dir.path().join("sub"));
        registry.open(&sub).unwrap();
        registry.open(&plain).unwrap();
        registry.close(&sub);
        std::fs::remove_dir_all(dir.path().join("sub")).unwrap();

        let (_, handle) = registry.get(&plain).unwrap();

        assert_eq!(handle.prune_worktrees(), Ok(()));
        assert!(handle.get_status().is_ok());
    }

    /// A spelling that now names another repository moves to it; the old repository's worker
    /// is dropped when nothing else names it, and the drop hook hears about it.
    #[cfg(unix)]
    #[test]
    fn reopening_a_spelling_that_names_another_repository_reports_the_dropped_key() {
        let (first, _a) = repo_with_commit();
        let (second, _b) = repo_with_commit();
        let links = TempDir::new().unwrap();
        let link = links.path().join("link");
        std::os::unix::fs::symlink(first.path(), &link).unwrap();
        let registry = WorkerRegistry::new();
        let (first_key, _) = registry.open(&spelling(&link)).unwrap();
        std::fs::remove_file(&link).unwrap();
        std::os::unix::fs::symlink(second.path(), &link).unwrap();
        let mut dropped = Vec::new();

        let (second_key, _) = registry
            .open_with(&spelling(&link), |key| dropped.push(key.to_string()))
            .unwrap();

        assert_ne!(first_key, second_key);
        assert_eq!(dropped, [first_key]);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn get_of_a_spelling_never_opened_is_none() {
        let (dir, _repo) = repo_with_commit();
        let registry = WorkerRegistry::new();
        registry.open(&spelling(dir.path())).unwrap();

        assert!(registry.get(&spelling(&dir.path().join("sub"))).is_none());
    }

    #[test]
    fn a_dead_worker_is_respawned_under_its_key() {
        let (dir, _repo) = repo_with_commit();
        let path = spelling(dir.path());
        let registry = WorkerRegistry::new();
        let (key, handle) = registry.open(&path).expect("spawn the first worker");
        assert!(handle.get_status().is_ok());

        // Crash the worker thread from inside its dispatch loop, the same failure mode a
        // panicking git-core call inside a `Command` match arm would trigger.
        handle.crash_for_test();

        // The channel-disconnect signal std::sync::mpsc gives is immediate, so this call
        // shouldn't hang; bound it with a timeout anyway so a regression fails instead of
        // wedging the test.
        let (done_tx, done_rx) = mpsc::channel();
        let crashed = handle.clone();
        thread::spawn(move || {
            let _ = done_tx.send(crashed.get_status());
        });
        let error = done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("worker did not report an error after crashing; call hung")
            .expect_err("a crashed worker must return an error, not a status list");
        assert!(
            error.contains("worker thread stopped"),
            "unexpected error message: {error}"
        );

        // The registry still holds the dead worker: `open` (what `open_repo` calls in both
        // tauri-app and vscode-sidecar) must notice and respawn rather than hand back the same
        // broken handle (AUD-2026-09-05-CONC-001).
        assert!(!handle.is_alive());
        let (fresh_key, fresh) = registry
            .open(&path)
            .expect("respawn a fresh worker for the same repository");
        assert_eq!(fresh_key, key);
        assert!(fresh.get_status().is_ok());
        assert!(registry.get(&path).unwrap().1.get_status().is_ok());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn opening_a_folder_that_is_no_repository_fails_as_before() {
        let dir = TempDir::new().unwrap();
        let path = spelling(dir.path());
        let registry = WorkerRegistry::new();

        let error = registry.open(&path).err().expect("not a repository");

        let spawn_error = super::Worker::spawn(dir.path().to_path_buf())
            .err()
            .expect("not a repository");
        assert_eq!(error, spawn_error);
        assert!(error.starts_with("failed to open repository"), "{error}");
        assert!(registry.is_empty());
        assert!(registry.get(&path).is_none());
    }

    #[test]
    fn a_bare_repository_is_keyed_by_its_git_folder() {
        let dir = TempDir::new().unwrap();
        Repository::init_bare(dir.path()).unwrap();
        let registry = WorkerRegistry::new();

        let (key, handle) = registry.open(&spelling(dir.path())).unwrap();

        assert_eq!(key, spelling(&std::fs::canonicalize(dir.path()).unwrap()));
        assert!(handle.workdir().is_err());
    }
}
