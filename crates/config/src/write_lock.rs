//! Exclusion around each read-modify-write of `config.toml` (#1127). Its writers run at the same
//! time: the desktop app's commands on a thread pool, each VS Code sidecar request on its own
//! thread, and the app and every VS Code window as separate processes on one file.
//!
//! In this process a `Mutex`; across processes an OS lock on `config.toml.lock` next to the file
//! (`flock` on Linux/macOS, `LockFileEx` on Windows, through std's `File::try_lock`), like the
//! undo bookkeeping's `state.lock` (`repo-service`'s `worker/state_lock.rs`, which explains the
//! explicit unlock): a process that exits or is killed releases it, and the file is never deleted.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

static IN_PROCESS: Mutex<()> = Mutex::new(());

/// How long a writer waits for another process's write. One takes milliseconds; a holder that
/// keeps it this long is stuck, and the write then goes ahead unlocked rather than not at all.
const WAIT: Duration = Duration::from_secs(5);

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
        file: lock_across_processes(&lock_file_path(config_file)),
        _in_process: in_process,
    }
}

fn lock_across_processes(path: &Path) -> Option<File> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).ok()?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .ok()?;
    let deadline = Instant::now() + WAIT;
    let mut pause = Duration::from_millis(1);
    loop {
        match file.try_lock() {
            Ok(()) => return Some(file),
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Error(_)) => return None,
        }
        let now = Instant::now();
        if now >= deadline {
            return None;
        }
        std::thread::sleep(pause.min(deadline - now));
        pause = (pause * 2).min(Duration::from_millis(50));
    }
}
