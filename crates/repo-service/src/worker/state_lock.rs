//! An advisory lock around Browsitory's undo bookkeeping in one git dir (#172).
//!
//! The Tauri app and the VSCode sidecar (or two workers in one process) can share a repository.
//! Every write to `undo.json` happens under this lock, after a forced reload, and a command that
//! can end a paused operation holds it from before it runs until its undo entry is recorded.
//!
//! It is an OS lock on an open handle (`flock` on Linux/macOS, `LockFileEx` on Windows, through
//! std's `File::try_lock`), not the file's existence: a process that exits or is killed, or a
//! worker thread that panics, releases it, so there is no staleness to handle. The file is never
//! deleted: deleting a locked file would let the next opener lock a new inode while the old
//! holder still believes it has exclusion.
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long a command that must hold the lock waits for it before running without it (D6-A).
#[cfg(not(test))]
pub(super) const LOCK_WAIT: Duration = Duration::from_secs(10);
#[cfg(test)]
pub(super) const LOCK_WAIT: Duration = Duration::from_millis(300);

/// `<gitdir>/browsitory/state.lock`. `repo.path()` is per worktree, so each linked worktree has
/// its own lock, as it has its own `undo.json`.
pub(super) struct StateLock {
    file: PathBuf,
}

/// Exclusion until dropped.
#[derive(Debug)]
pub(super) struct StateGuard {
    _file: File,
}

#[derive(Debug)]
pub(super) enum Acquire {
    Held(StateGuard),
    /// Another holder kept it for the whole wait.
    Busy,
    /// The lock can't be taken here at all (the folder can't be created, or the file system
    /// doesn't support locks, as some network mounts): carry on unlocked.
    Unavailable(std::io::Error),
}

impl StateLock {
    pub fn open(repo: &git2::Repository) -> Self {
        Self::at(repo.path())
    }

    /// The lock of the git dir `gitdir`.
    pub fn at(gitdir: &Path) -> Self {
        Self {
            file: gitdir.join("browsitory").join("state.lock"),
        }
    }

    /// Tries once, then polls (10 ms, doubling to 100 ms) until `wait` has elapsed.
    pub fn acquire(&self, wait: Duration) -> Acquire {
        let file = match self.open_file() {
            Ok(file) => file,
            Err(error) => return Acquire::Unavailable(error),
        };
        let deadline = Instant::now() + wait;
        let mut pause = Duration::from_millis(10);
        loop {
            match file.try_lock() {
                Ok(()) => return Acquire::Held(StateGuard { _file: file }),
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Error(error)) => return Acquire::Unavailable(error),
            }
            let now = Instant::now();
            if now >= deadline {
                return Acquire::Busy;
            }
            std::thread::sleep(pause.min(deadline - now));
            pause = (pause * 2).min(Duration::from_millis(100));
        }
    }

    pub fn try_acquire(&self) -> Acquire {
        self.acquire(Duration::ZERO)
    }

    fn open_file(&self) -> std::io::Result<File> {
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&self.file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::time::Instant;

    fn repo() -> (tempfile::TempDir, git2::Repository) {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        (dir, repo)
    }

    fn held(acquire: Acquire) -> StateGuard {
        match acquire {
            Acquire::Held(guard) => guard,
            Acquire::Busy => panic!("busy"),
            Acquire::Unavailable(error) => panic!("unavailable: {error}"),
        }
    }

    fn is_busy(acquire: &Acquire) -> bool {
        matches!(acquire, Acquire::Busy)
    }

    #[test]
    fn a_second_handle_is_busy_until_the_first_releases() {
        let (_dir, repo) = repo();
        let (a, b) = (StateLock::open(&repo), StateLock::open(&repo));
        let guard = held(a.acquire(Duration::ZERO));
        assert!(is_busy(&b.try_acquire()));
        drop(guard);
        held(b.try_acquire());
    }

    #[test]
    fn acquire_waits_then_reports_busy() {
        let (_dir, repo) = repo();
        let (a, b) = (StateLock::open(&repo), StateLock::open(&repo));
        let _guard = held(a.try_acquire());
        let start = Instant::now();
        assert!(is_busy(&b.acquire(Duration::from_millis(150))));
        assert!(start.elapsed() >= Duration::from_millis(150));
    }

    #[test]
    fn acquire_succeeds_when_the_holder_releases_during_the_wait() {
        let (_dir, repo) = repo();
        let (a, b) = (StateLock::open(&repo), StateLock::open(&repo));
        let guard = held(a.try_acquire());
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            drop(guard);
        });
        let start = Instant::now();
        held(b.acquire(Duration::from_secs(2)));
        assert!(start.elapsed() < Duration::from_secs(2));
        releaser.join().unwrap();
    }

    #[test]
    fn a_panicking_holder_thread_releases_the_lock() {
        let (_dir, repo) = repo();
        let path = repo.path().to_path_buf();
        let holder = std::thread::spawn(move || {
            let _guard = held(StateLock::at(&path).try_acquire());
            panic!("holder panics while holding the lock");
        });
        assert!(holder.join().is_err());
        held(StateLock::open(&repo).try_acquire());
    }

    /// Not a test on its own: the child process of `a_killed_holder_process_releases_the_lock`.
    #[test]
    #[ignore = "child process of a_killed_holder_process_releases_the_lock"]
    fn lock_holder_child() {
        let Some(gitdir) = std::env::var_os("BROWSITORY_LOCK_HOLDER") else {
            return;
        };
        let _guard = held(StateLock::at(std::path::Path::new(&gitdir)).try_acquire());
        println!("locked");
        std::thread::sleep(Duration::from_secs(60));
    }

    #[test]
    fn a_killed_holder_process_releases_the_lock() {
        let (_dir, repo) = repo();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "worker::state_lock::tests::lock_holder_child",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("BROWSITORY_LOCK_HOLDER", repo.path())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let stdout = child.stdout.take().unwrap();
        let mut lines = BufReader::new(stdout).lines();
        loop {
            let line = lines
                .next()
                .expect("the child exited before locking")
                .unwrap();
            if line.contains("locked") {
                break;
            }
        }
        let lock = StateLock::open(&repo);
        assert!(is_busy(&lock.try_acquire()));
        child.kill().unwrap();
        child.wait().unwrap();
        held(lock.acquire(Duration::from_secs(5)));
    }

    #[test]
    fn the_lock_file_stays_after_release() {
        let (_dir, repo) = repo();
        let lock = StateLock::open(&repo);
        drop(held(lock.try_acquire()));
        assert!(repo.path().join("browsitory").join("state.lock").is_file());
        held(lock.try_acquire());
    }

    #[test]
    fn each_linked_worktree_has_its_own_lock() {
        let (dir, repo) = repo();
        {
            let mut index = repo.index().unwrap();
            let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
            let sig = git2::Signature::now("T", "t@example.com").unwrap();
            repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
                .unwrap();
        }
        let path = dir.path().with_extension("linked");
        let worktree = repo.worktree("linked", &path, None).unwrap();
        let linked = git2::Repository::open_from_worktree(&worktree).unwrap();
        let _main = held(StateLock::open(&repo).try_acquire());
        let _linked = held(StateLock::open(&linked).try_acquire());
        drop(_linked);
        std::fs::remove_dir_all(&path).unwrap();
    }
}
