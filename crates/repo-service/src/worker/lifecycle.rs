//! Stopping a worker when the app exits (#965).
//!
//! The process ends when the desktop app's last window closes, and with it every worker thread,
//! wherever it is. A thread cut off in the middle of a libgit2 write (a status read refreshing
//! the index's stat data, a stage, a checkout) leaves `.git/index.lock` or a ref lock behind,
//! and every later git command in that repository fails on it. So the app first asks its workers
//! to stop: each finishes the command it is running, starts no other, and the app waits for that
//! up to a grace period before it exits.

use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::Instant;

/// Whether a worker is running a command, and whether it was asked to stop. Shared by the
/// worker thread (`WorkerLoop::run`) and the `Worker` that stops it.
#[derive(Default)]
pub(crate) struct Lifecycle {
    state: Mutex<State>,
    idle: Condvar,
}

#[derive(Default)]
struct State {
    stopping: bool,
    running: bool,
}

/// Marks a command as running until dropped, so a command that panics still ends its run.
pub(crate) struct Running<'a>(&'a Lifecycle);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.lock().running = false;
        self.0.idle.notify_all();
    }
}

impl Lifecycle {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Called by the worker thread before a command: `None` once the worker was asked to stop,
    /// and the command is then not run.
    pub(crate) fn begin(&self) -> Option<Running<'_>> {
        let mut state = self.lock();
        if state.stopping {
            return None;
        }
        state.running = true;
        Some(Running(self))
    }

    /// From now on the worker starts no command.
    pub(crate) fn request_stop(&self) {
        self.lock().stopping = true;
    }

    /// Waits until the worker runs no command or `deadline` passes. Returns whether it is idle.
    pub(crate) fn wait_idle(&self, deadline: Instant) -> bool {
        let mut state = self.lock();
        while state.running {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                return false;
            };
            state = self
                .idle
                .wait_timeout(state, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        true
    }
}
