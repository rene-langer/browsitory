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
//!
//! The guard unlocks explicitly when dropped (#175), instead of relying on closing the file. A
//! child process that any thread of this process forks holds a copy of every descriptor until it
//! execs (std opens with `O_CLOEXEC`, which only takes effect at `exec`), and a `flock` lock
//! belongs to the open file description, so closing our descriptor would leave the lock held
//! through that copy. `flock(2)`: "the lock is released either by an explicit LOCK_UN operation on
//! any of these duplicate file descriptors, or when all such file descriptors have been closed."
//! Open-file-description locks (`F_OFD_SETLK`) would not help: they are inherited across `fork`
//! the same way. On Windows the handle isn't inheritable; the unlock is there for symmetry.
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long a command that must hold the lock waits for it before running without it (D6-A).
#[cfg(not(test))]
pub(super) const LOCK_WAIT: Duration = Duration::from_secs(10);
#[cfg(test)]
pub(super) const LOCK_WAIT: Duration = Duration::from_secs(1);

/// How long a command that must hold the lock waits for it after an earlier one waited the full
/// `LOCK_WAIT` in vain, until one takes it again (#175, D2-A). A long holder in the other window
/// (a pull, a hook, a pinentry dialog) then costs one `LOCK_WAIT`, then about this per click,
/// while a brief hold (the other window staging a file) no longer refuses or goes unrecorded.
#[cfg(not(test))]
pub(super) const CONTENDED_WAIT: Duration = Duration::from_secs(1);
#[cfg(test)]
pub(super) const CONTENDED_WAIT: Duration = Duration::from_millis(300);

/// How long the next locked command waits for the lock, from how recently it was found busy
/// (#175, #197):
///
/// - not contended: `LOCK_WAIT`;
/// - the first `Busy` (a full `LOCK_WAIT` in vain) marks it contended: the next command waits
///   `CONTENDED_WAIT`, so a brief hold right after a long one is still waited out;
/// - every later `Busy` opens a burst window that lasts until `CONTENDED_WAIT` after the last
///   command that ran without the lock finished (`finished`). Inside it, a command that would run
///   unrecorded tries once (`ZERO`): a burst of clicks or queued commands behind the other
///   window's long hold costs about one `CONTENDED_WAIT`, not one each, however long each
///   command takes. A command that refuses when the lock is busy (Undo, Redo, discards, Reset
///   hard) still waits `CONTENDED_WAIT`, so it doesn't refuse at once. After a quiet
///   `CONTENDED_WAIT`, the next command waits `CONTENDED_WAIT` again;
/// - taking the lock (`held`) resets it.
#[derive(Debug, Default)]
pub(super) struct Contention {
    contended: bool,
    quick_until: Option<Instant>,
}

impl Contention {
    /// How long a locked command waits at `now`; `refuses`: it refuses when the lock stays busy.
    pub fn wait(&self, now: Instant, refuses: bool) -> Duration {
        if !self.contended {
            LOCK_WAIT
        } else if !refuses && self.quick_until.is_some_and(|until| now < until) {
            Duration::ZERO
        } else {
            CONTENDED_WAIT
        }
    }

    pub fn held(&mut self) {
        *self = Self::default();
    }

    pub fn busy(&mut self, now: Instant) {
        if self.contended {
            self.extend(now);
        } else {
            self.contended = true;
        }
    }

    /// A command that found the lock busy finished at `now`: an open window lasts until
    /// `CONTENDED_WAIT` after it, so a slow command doesn't close it.
    pub fn finished(&mut self, now: Instant) {
        if self.quick_until.is_some() {
            self.extend(now);
        }
    }

    fn extend(&mut self, now: Instant) {
        let until = now + CONTENDED_WAIT;
        self.quick_until = Some(self.quick_until.map_or(until, |old| old.max(until)));
    }
}

/// `<gitdir>/browsitory/state.lock`. `repo.path()` is per worktree, so each linked worktree has
/// its own lock, as it has its own `undo.json`.
pub(super) struct StateLock {
    file: PathBuf,
}

/// Exclusion until dropped.
#[derive(Debug)]
pub(super) struct StateGuard {
    file: File,
}

impl Drop for StateGuard {
    /// Releases the lock for every copy of the descriptor, including one a forked child still
    /// holds before its `exec` (see the module docs).
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
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
        self.acquire_with(wait, || {})
    }

    /// `acquire`, calling `on_wait` once when the first try finds the lock busy; `wait` starts
    /// after it returns. Only tests pass a non-empty `on_wait` (the worker's lock-wait pause,
    /// #274), to change the repository while the worker is known to be waiting.
    pub fn acquire_with(&self, wait: Duration, on_wait: impl FnOnce()) -> Acquire {
        let file = match self.open_file() {
            Ok(file) => file,
            Err(error) => return Acquire::Unavailable(error),
        };
        let mut on_wait = Some(on_wait);
        let mut deadline = Instant::now() + wait;
        let mut pause = Duration::from_millis(10);
        loop {
            match file.try_lock() {
                Ok(()) => return Acquire::Held(StateGuard { file }),
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Error(error)) => return Acquire::Unavailable(error),
            }
            if let Some(on_wait) = on_wait.take() {
                on_wait();
                deadline = Instant::now() + wait;
            }
            let now = Instant::now();
            if now >= deadline {
                return Acquire::Busy;
            }
            std::thread::sleep(pause.min(deadline - now));
            pause = (pause * 2).min(Duration::from_millis(100));
        }
    }

    #[cfg(test)]
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
    fn contention_waits_long_then_briefly_then_once_per_burst() {
        let t0 = Instant::now();
        let mut contention = Contention::default();
        assert_eq!(contention.wait(t0, false), LOCK_WAIT);
        assert_eq!(contention.wait(t0, true), LOCK_WAIT);
        // The first timeout: the next command still waits, briefly.
        contention.busy(t0);
        assert_eq!(contention.wait(t0, false), CONTENDED_WAIT);
        assert_eq!(contention.wait(t0 + LOCK_WAIT, false), CONTENDED_WAIT);
        // `finished` alone opens no window.
        contention.finished(t0);
        assert_eq!(contention.wait(t0, false), CONTENDED_WAIT);
        // Busy again: commands within `CONTENDED_WAIT` of it try once.
        let t1 = t0 + CONTENDED_WAIT;
        contention.busy(t1);
        let soon = t1 + Duration::from_millis(10);
        assert_eq!(contention.wait(soon, false), Duration::ZERO);
        assert_eq!(contention.wait(t1 + CONTENDED_WAIT, false), CONTENDED_WAIT);
        // Each busy try extends the window.
        let t2 = t1 + CONTENDED_WAIT / 2;
        contention.busy(t2);
        assert_eq!(contention.wait(t1 + CONTENDED_WAIT, false), Duration::ZERO);
        assert_eq!(contention.wait(t2 + CONTENDED_WAIT, false), CONTENDED_WAIT);
        // Taking the lock resets it.
        contention.held();
        assert_eq!(contention.wait(t2, false), LOCK_WAIT);
        contention.busy(t2);
        assert_eq!(contention.wait(t2, false), CONTENDED_WAIT);
    }

    /// #197 review: inside a window, a command that would refuse still waits briefly.
    #[test]
    fn contention_lets_refusing_commands_wait_inside_a_burst() {
        let t0 = Instant::now();
        let mut contention = Contention::default();
        contention.busy(t0);
        contention.busy(t0);
        assert_eq!(contention.wait(t0, false), Duration::ZERO);
        assert_eq!(contention.wait(t0, true), CONTENDED_WAIT);
    }

    /// #197 review: a slow command doesn't close the window; it lasts until `CONTENDED_WAIT`
    /// after the command finished.
    #[test]
    fn contention_window_runs_from_the_end_of_the_last_command() {
        let t0 = Instant::now();
        let mut contention = Contention::default();
        contention.busy(t0);
        contention.busy(t0);
        let done = t0 + 3 * CONTENDED_WAIT;
        contention.finished(done);
        assert_eq!(
            contention.wait(done + CONTENDED_WAIT / 2, false),
            Duration::ZERO
        );
        assert_eq!(
            contention.wait(done + CONTENDED_WAIT, false),
            CONTENDED_WAIT
        );
        // `finished` never shortens the window.
        let mut contention = Contention::default();
        contention.busy(t0);
        contention.busy(t0 + CONTENDED_WAIT);
        contention.finished(t0);
        assert_eq!(
            contention.wait(t0 + CONTENDED_WAIT + CONTENDED_WAIT / 2, false),
            Duration::ZERO
        );
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

    /// #274: `on_wait` runs once, when the first try finds the lock busy, and the wait starts
    /// after it returns, so a test can change the repository "meanwhile" for as long as it needs.
    #[test]
    fn acquire_with_calls_on_wait_once_before_the_wait_starts() {
        let (_dir, repo) = repo();
        let (a, b) = (StateLock::open(&repo), StateLock::open(&repo));
        let mut calls = 0;
        held(b.acquire_with(Duration::ZERO, || calls += 1));
        assert_eq!(calls, 0, "the lock was free");

        let guard = held(a.try_acquire());
        let mut guard = Some(guard);
        let acquired = b.acquire_with(Duration::from_millis(100), || {
            calls += 1;
            // Longer than the wait: still held, since the wait starts after this.
            std::thread::sleep(Duration::from_millis(300));
            drop(guard.take());
        });
        held(acquired);
        assert_eq!(calls, 1);

        // Held through several polls (10, 20, 40, 80, 100 ms…): still called only once.
        let guard = held(a.try_acquire());
        let mut releaser = None;
        let mut calls = 0;
        let acquired = b.acquire_with(Duration::from_secs(5), || {
            calls += 1;
            releaser = Some(std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(250));
                drop(guard);
            }));
        });
        held(acquired);
        releaser.unwrap().join().unwrap();
        assert_eq!(calls, 1);
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
    fn a_copied_descriptor_does_not_keep_a_released_lock() {
        let (_dir, repo) = repo();
        let (a, b) = (StateLock::open(&repo), StateLock::open(&repo));
        let guard = held(a.try_acquire());
        // Stands in for the copy a child process holds between its `fork` and its `exec`.
        let copy = guard.file.try_clone().unwrap();
        drop(guard);
        held(b.try_acquire());
        drop(copy);
    }

    #[cfg(unix)]
    #[test]
    fn a_child_forked_while_the_lock_was_held_does_not_keep_it() {
        use std::io::{Read, Write};
        use std::os::unix::process::CommandExt;
        let (_dir, repo) = repo();
        let (a, b) = (StateLock::open(&repo), StateLock::open(&repo));
        let guard = held(a.try_acquire());
        // The child writes a byte here once it is forked, so the test drops the guard only
        // while the child surely holds a copy of the lock's descriptor (#182 review: a fixed
        // sleep passed vacuously when the fork came late).
        let (mut forked_rx, forked_tx) = std::io::pipe().unwrap();
        let forker = std::thread::spawn(move || {
            let mut command = std::process::Command::new("sh");
            command.arg("-c").arg(":");
            // SAFETY: `write` and `nanosleep` are async-signal-safe; writing one byte to a pipe
            // allocates nothing.
            unsafe {
                command.pre_exec(move || {
                    (&forked_tx).write_all(&[1])?;
                    std::thread::sleep(Duration::from_secs(2));
                    Ok(())
                });
            }
            // `spawn` returns once the child has exec'd (or failed to), so the child holds a
            // copy of the lock's descriptor from the byte until about two seconds later.
            command.spawn().unwrap().wait().unwrap();
        });
        let (forked, wait_forked) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut byte = [0];
            let _ = forked.send(forked_rx.read_exact(&mut byte));
        });
        wait_forked
            .recv_timeout(Duration::from_secs(30))
            .expect("the child was not forked within 30 s")
            .unwrap();
        drop(guard);
        let acquired = b.try_acquire();
        // Still before the child's `exec`, or the check proved nothing: fail, don't pass.
        assert!(
            !forker.is_finished(),
            "the child exec'd before the lock was checked"
        );
        forker.join().unwrap();
        held(acquired);
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
