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
/// command is queued) until its `OperationId` is dropped: after the operation ran, when the
/// command was refused, or when it never reached a worker. A cancel for a known ID is kept; a
/// cancel for any other ID, such as one clicked after the operation finished, is dropped, so the
/// registry cannot grow with late clicks (#219). (A cancel while the command is still queued is
/// kept too, but the UI only learns an ID once the operation started.)
#[derive(Clone, Default)]
pub struct CancelRegistry(Arc<Mutex<HashMap<String, bool>>>);

impl CancelRegistry {
    fn operations(&self) -> MutexGuard<'_, HashMap<String, bool>> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// A new `"<prefix>-<n>"` ID, unique per process, known to this registry until the returned
    /// `OperationId` is dropped.
    pub(super) fn allocate(&self, prefix: &str) -> OperationId {
        let id = format!(
            "{prefix}-{}",
            NEXT_OPERATION_ID.fetch_add(1, Ordering::Relaxed)
        );
        self.operations().insert(id.clone(), false);
        OperationId {
            id,
            registry: self.clone(),
        }
    }

    /// `allocate`, but only while no other operation is known (queued or running); `None`
    /// otherwise. Checked and inserted under one lock, so two callers can't both get an ID.
    pub(super) fn allocate_if_idle(&self, prefix: &str) -> Option<OperationId> {
        let mut operations = self.operations();
        if !operations.is_empty() {
            return None;
        }
        let id = format!(
            "{prefix}-{}",
            NEXT_OPERATION_ID.fetch_add(1, Ordering::Relaxed)
        );
        operations.insert(id.clone(), false);
        drop(operations);
        Some(OperationId {
            id,
            registry: self.clone(),
        })
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

    /// How many operations are known: pending or running.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.operations().len()
    }
}

/// An ID handed out by `CancelRegistry::allocate`, carried by the command it names. The registry
/// forgets the ID when this is dropped, on every path: the worker is done with the command, the
/// gate refused it, the send failed, or the command was still queued when the worker died.
pub(crate) struct OperationId {
    id: String,
    registry: CancelRegistry,
}

impl OperationId {
    pub(super) fn as_str(&self) -> &str {
        &self.id
    }

    pub(super) fn registry(&self) -> &CancelRegistry {
        &self.registry
    }
}

/// A stand-in ID for tests that build a `Command` without a worker, known to no registry.
#[cfg(test)]
impl From<String> for OperationId {
    fn from(id: String) -> Self {
        Self {
            id,
            registry: CancelRegistry::default(),
        }
    }
}

#[cfg(test)]
impl From<&str> for OperationId {
    fn from(id: &str) -> Self {
        id.to_string().into()
    }
}

impl Drop for OperationId {
    fn drop(&mut self) {
        self.registry.operations().remove(&self.id);
    }
}
