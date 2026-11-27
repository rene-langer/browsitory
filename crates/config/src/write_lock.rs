//! Exclusion around each read-modify-write of `config.toml` (#1127). Its writers run at the same
//! time: the desktop app's commands on a thread pool, each VS Code sidecar request on its own
//! thread, and the app and every VS Code window as separate processes on one file.
//!
//! In this process a `Mutex`; across processes an OS lock on `config.toml.lock` next to the file
//! (`flock` on Linux/macOS, `LockFileEx` on Windows, through std's `File::try_lock`), like the
//! undo bookkeeping's `state.lock` (`repo-service`'s `worker/state_lock.rs`, which explains the
//! explicit unlock): a process that exits or is killed releases it, and the file is never deleted.
//!
//! A writer that gets the lock moves the lock file's length on by one (1 to `TURNS` bytes, then
//! back to 1), so a waiting process can see writes finish while the lock stays taken (#1212).

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

static IN_PROCESS: Mutex<()> = Mutex::new(());

/// How long a writer waits while no other process finishes a write. One takes milliseconds (on
/// a busy Windows runner about 200 ms, and one process's run of writes can keep the lock for
/// seconds); a holder that finishes none for this long is stuck, and the write then goes ahead
/// unlocked rather than not at all.
const WAIT: Duration = Duration::from_secs(5);

/// The lock file's lengths that count the turns: more than a waiter can miss between two looks.
const TURNS: u64 = 64;

/// Exclusion until dropped.
pub(crate) struct WriteGuard {
    file: Option<File>,
    _in_process: MutexGuard<'static, ()>,
}

impl Drop for WriteGuard {
    fn drop(&mut self) {
        if let Some(file) = &self.file {
            let _ = file.unlock();
        }
    }
}

fn lock_file_path(config_file: &Path) -> PathBuf {
    let mut name = config_file
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "config.toml".into());
    name.push(".lock");
    config_file.with_file_name(name)
}

/// Waits for the other writers of `config_file`. Where the lock file can't be created or locked
/// (a read-only folder, a file system without locks), only this process's writers are excluded.
pub(crate) fn lock(config_file: &Path) -> WriteGuard {
    let in_process = IN_PROCESS.lock().unwrap_or_else(PoisonError::into_inner);
    WriteGuard {
        file: lock_across_processes(&lock_file_path(config_file), WAIT),
        _in_process: in_process,
    }
}

fn lock_across_processes(path: &Path, wait: Duration) -> Option<File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok()?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .ok()?;
    let mut seen_turn = turn(&file);
    let mut deadline = Instant::now() + wait;
    let mut pause = Duration::from_millis(1);
    loop {
        match file.try_lock() {
            Ok(()) => {
                take_turn(&file);
                return Some(file);
            }
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(_)) => return None,
        }
        let now = Instant::now();
        let current_turn = turn(&file);
        if current_turn != seen_turn {
            seen_turn = current_turn;
            deadline = now + wait;
        } else if now >= deadline {
            return None;
        }
        std::thread::sleep(pause.min(deadline - now));
        pause = (pause * 2).min(Duration::from_millis(50));
    }
}

/// Reads the length on the waiter's own handle: on Windows the locked bytes can't be read.
fn turn(file: &File) -> Option<u64> {
    file.metadata().ok().map(|metadata| metadata.len())
}

fn take_turn(file: &File) {
    if let Some(len) = turn(file) {
        let _ = file.set_len(len % TURNS + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1212: on a slow disk (Windows CI: about 200 ms per write) one process's run of writes
    /// keeps the lock longer than the wait, though each write in it finishes. The waiter waits
    /// for its turn instead of writing unlocked and undoing one of them.
    #[test]
    fn a_waiter_outlasts_the_wait_while_other_writers_keep_finishing() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.toml.lock");
        let wait = Duration::from_secs(2);
        let first = lock_across_processes(&path, wait).expect("the first writer locks");
        let others = {
            let path = path.clone();
            std::thread::spawn(move || {
                // Writes of 100 ms each, back to back, for three times the wait.
                let mut held = Some(first);
                let until = Instant::now() + wait * 3;
                while Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(100));
                    drop(held.take());
                    held = lock_across_processes(&path, Duration::from_secs(60));
                }
            })
        };

        // It may also get in between two of the others' writes, which is just as safe.
        let waiter = lock_across_processes(&path, wait);
        let locked = waiter.is_some();
        drop(waiter);
        others.join().unwrap();

        assert!(
            locked,
            "the waiter gave up while the other writers still finished writes"
        );
    }

    /// A holder that finishes no write for the whole wait is stuck: the write goes ahead
    /// unlocked rather than not at all.
    #[test]
    fn a_holder_that_finishes_nothing_is_waited_for_only_so_long() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("config.toml.lock");
        let wait = Duration::from_millis(500);
        let stuck = lock_across_processes(&path, wait).expect("the stuck writer locks");

        let started = Instant::now();
        let waiter = lock_across_processes(&path, wait);
        let waited = started.elapsed();
        drop(stuck);

        assert!(waiter.is_none(), "the waiter got a lock that is still held");
        assert!(
            waited >= wait,
            "gave up after {waited:?}, before the wait of {wait:?}"
        );
    }
}
