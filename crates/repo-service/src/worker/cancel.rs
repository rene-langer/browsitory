use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

static NEXT_OPERATION_ID: AtomicU64 = AtomicU64::new(1);

/// The cancellable operations of one worker, and which of them the user asked to stop.
///
/// This is deliberately a *side channel* rather than another `Command`: the worker thread
/// processes one command at a time, and while a fetch/pull/push is blocked inside `git2`'s
/// network I/O it is not reading `rx` at all — a cancel sent through the command channel would
/// queue behind the very transfer it is meant to interrupt. `WorkerHandle::cancel_transfer`
/// therefore writes straight into this registry, and `git-core`'s progress callbacks poll it on
/// every tick from inside the blocked call (see `worker/remote.rs`'s `cancel_check`).
///
/// An operation is known from the moment the handle hands out its ID (`allocate`, before the
/// command is queued) until the worker is done with the command that carries it
/// (`OperationDone`), whether the command ran or was refused. A cancel for a known ID is kept,
/// even one that arrives before the worker starts the operation. A cancel for any other ID,
/// such as one clicked after the operation finished, is dropped, so the registry cannot grow
/// with late clicks (#219).
#[derive(Clone, Default)]
pub struct CancelRegistry(Arc<Mutex<HashMap<String, bool>>>);

impl CancelRegistry {
    fn operations(&self) -> MutexGuard<'_, HashMap<String, bool>> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// A new `"<prefix>-<n>"` ID, unique per process, known to this registry until the operation
    /// it names is done.
    pub(super) fn allocate(&self, prefix: &str) -> String {
        let id = format!(
            "{prefix}-{}",
            NEXT_OPERATION_ID.fetch_add(1, Ordering::Relaxed)
        );
        self.operations().insert(id.clone(), false);
        id
    }

    /// Asks the operation `id` to stop; ignored when no such operation is pending or running.
    pub(super) fn cancel(&self, id: &str) {
        if let Some(cancelled) = self.operations().get_mut(id) {
            *cancelled = true;
        }
    }

    pub(super) fn is_cancelled(&self, id: &str) -> bool {
        self.operations().get(id).copied().unwrap_or(false)
    }

    /// Forgets a cancel asked for so far; the operation stays known.
    pub(super) fn reset(&self, id: &str) {
        if let Some(cancelled) = self.operations().get_mut(id) {
            *cancelled = false;
        }
    }

    /// Forgets the operation `id` once the returned value is dropped.
    pub(super) fn done_on_drop(&self, id: &str) -> OperationDone {
        OperationDone {
            registry: self.clone(),
            id: id.to_string(),
        }
    }

    /// How many operations are known: pending or running.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.operations().len()
    }
}

/// Forgets its operation in the registry when dropped: the operation is done, and a cancel for
/// it from now on is dropped.
pub(super) struct OperationDone {
    registry: CancelRegistry,
    id: String,
}

impl Drop for OperationDone {
    fn drop(&mut self) {
        self.registry.operations().remove(&self.id);
    }
}
