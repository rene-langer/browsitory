//! Shared progress-reporting types for push/fetch/pull.
//!
//! `fetch`/`pull` and `push`/`push_tag` are added on top of these by
//! separate workstreams (Phase 3); this module intentionally starts out
//! holding only the types both sides need, so neither workstream's branch
//! has to fight the other over ownership of this file's function bodies.

/// A snapshot of transfer progress, cheap to copy so it can be sent through
/// an `mpsc` channel on every libgit2 progress callback invocation (which
/// can fire many times per second) without allocating.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProgressUpdate {
    pub received_objects: usize,
    pub total_objects: usize,
    pub indexed_objects: usize,
    pub received_bytes: usize,
    pub stage: TransferStage,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TransferStage {
    #[default]
    Negotiating,
    Receiving,
    Indexing,
    Pushing,
    Done,
}
