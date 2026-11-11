//! Undo stack: which worker commands are recorded, under which label, and the recorder that
//! snapshots around them. See `docs/superpowers/specs/2026-10-18-parity-batch3b-design.md`.
use crate::wire::ErrorDto;
use std::sync::mpsc::Sender;
use std::time::Duration;

use git2::Repository;
use git_core::reset::ResetMode;
use git_core::undo::{capture, restore, RepoSnapshot, SnapshotScope, UndoError};

use super::policy::{reply_of, BusyReply};
use super::rebase::{HistoryEdit, HistoryEditKind};
use super::state_lock::{Acquire, StateGuard, StateLock, LOCK_WAIT};
use super::undo_store::{PendingEntry, UndoEntry, UndoStacks, UndoStore, UNDO_LIMIT};
use super::{Command, WorkerHandle};

pub(super) const MAX_LABEL_CHARS: usize = 80;

/// Undo/Redo's reply while another holder keeps the state lock past `LOCK_WAIT` (D7-A).
pub(super) const UNDO_BUSY: &str =
    "Another Browsitory window is changing this repository's undo history. Try again in a moment.";

/// The reply of a command that destroys work-tree content with no other way back (discards, a
/// hard reset) while another holder keeps the state lock past `LOCK_WAIT`: it would run
/// unrecorded, so it is refused instead (M4, option C).
pub(super) const DISCARD_BUSY: &str = "Another Browsitory window is changing this repository's \
undo history. Try again in a moment, so this change can be undone.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UndoPlan {
    pub label: String,
    pub scope: SnapshotScope,
}

/// While another holder keeps the state lock past `LOCK_WAIT`, a command whose policy refuses it
/// then (`CommandPolicy::busy`) gets `busy`'s text and must not run: Undo/Redo (D7-A), and the
/// commands that destroy work-tree content with no other way back (discarding a hunk, lines, a
/// file or all unstaged changes, and a hard reset), which would run unrecorded, so nothing could
/// bring the content back. Every other command runs unrecorded instead (D6-A): branch, tag and
/// reflog changes stay recoverable from the reflog; switches, merges, picks and pulls refuse to
/// overwrite local changes.
pub(super) fn reply_busy(command: &mut Command, busy: BusyReply) {
    if let Some(reply) = reply_of(command) {
        reply.refuse(busy.text().into());
    }
}

/// "Amend “<summary>”", or a fallback for an empty message.
pub(super) fn amend_label(message: &str) -> String {
    if message.trim().is_empty() {
        "Amend last commit".to_string()
    } else {
        quoted("Amend ", message)
    }
}

pub(super) fn reset_label(commit_id: &str, mode: ResetMode) -> String {
    let mode = match mode {
        ResetMode::Soft => "soft",
        ResetMode::Mixed => "mixed",
        ResetMode::Hard => "hard",
    };
    format!("Reset to {} ({mode})", short(commit_id))
}

/// "Merge <branch>" or "Squash merge <branch>", with the branch's `refs/heads/` or
/// `refs/remotes/` prefix dropped.
pub(super) fn merge_label(branch_name: &str, squash: bool) -> String {
    let verb = if squash { "Squash merge" } else { "Merge" };
    let name = branch_name
        .strip_prefix("refs/heads/")
        .or_else(|| branch_name.strip_prefix("refs/remotes/"))
        .unwrap_or(branch_name);
    fit(format!("{verb} {name}"))
}

/// A finished sequence's undo entry names the commits it actually applied (skipped ones
/// changed nothing), not the number selected.
pub(super) fn relabel_sequence(
    recorder: &mut UndoRecorder,
    kind: git_core::pick::SequenceKind,
    result: &Result<git_core::pick::SequenceOutcome, ErrorDto>,
) {
    if let Ok(git_core::pick::SequenceOutcome::Completed { applied, .. }) = result {
        let verb = match kind {
            git_core::pick::SequenceKind::CherryPick => "Cherry-pick",
            git_core::pick::SequenceKind::Revert => "Revert",
        };
        recorder.relabel(count_label(verb, *applied));
    }
}

/// "Rebase onto <onto>", with a commit id shortened like every other label's (the frontend's
/// "Rebase onto here" passes the full id) and a ref name kept as it is.
fn rebase_label(onto: &str) -> String {
    let is_full_id = matches!(onto.len(), 40 | 64) && onto.bytes().all(|b| b.is_ascii_hexdigit());
    if is_full_id {
        format!("Rebase onto {}", short(onto))
    } else {
        fit(format!("Rebase onto {onto}"))
    }
}

/// A `StartRebase`'s label: what the user did for a one-click history edit (#473), else
/// [`rebase_label`].
pub(super) fn start_rebase_label(onto: &str, edit: Option<&HistoryEdit>) -> String {
    let Some(edit) = edit else {
        return rebase_label(onto);
    };
    let id = short(&edit.commit_id);
    match edit.kind {
        HistoryEditKind::Reword => format!("Reword {id}"),
        HistoryEditKind::Squash => format!("Squash {id} into its parent"),
        HistoryEditKind::Fixup => format!("Fix up {id} into its parent"),
        HistoryEditKind::MoveUp => format!("Move {id} up"),
        HistoryEditKind::MoveDown => format!("Move {id} down"),
        HistoryEditKind::Drop => format!("Drop {id}"),
    }
}

/// The label of a failed rebase start that still changed the repository. `git_core::rebase`
/// rolls a failed start back to the branch; an entry is only left when the rollback didn't
/// restore everything (or failed: `rolled_back` is false while HEAD is still detached). Its
/// Undo restores the state before the start, so the entry is kept, but must not read like a
/// rebase that happened. `base` is the start's own label ([`start_rebase_label`]).
fn failed_rebase_label(base: &str, rolled_back: bool) -> String {
    if rolled_back {
        fit(format!("{base} failed (rolled back)"))
    } else {
        fit(format!("{base} failed"))
    }
}

/// Renames the entry of a `StartRebase` that failed (see [`failed_rebase_label`]); `label` is
/// the one it was recorded under.
pub(super) fn relabel_rebase_start(
    recorder: &mut UndoRecorder,
    repo: &Repository,
    label: &str,
    result: &Result<git_core::rebase::RebaseStepResult, ErrorDto>,
) {
    if result.is_err() {
        let rolled_back = !repo.head_detached().unwrap_or(true);
        recorder.relabel(failed_rebase_label(label, rolled_back));
    }
}

pub(super) fn count_label(verb: &str, count: usize) -> String {
    format!(
        "{verb} {count} {}",
        if count == 1 { "commit" } else { "commits" }
    )
}

/// Char-based: an id that isn't a real hex oid (any user/IPC string) must not panic.
pub(super) fn short(oid: &str) -> String {
    oid.chars().take(7).collect()
}

pub(super) fn fit(label: String) -> String {
    truncate_end(&label, MAX_LABEL_CHARS)
}

pub(super) fn with_path(prefix: &str, path: &str, suffix: &str) -> String {
    let budget = MAX_LABEL_CHARS.saturating_sub(prefix.chars().count() + suffix.chars().count());
    format!("{prefix}{}{suffix}", ellipsize_middle(path, budget))
}

pub(super) fn quoted(prefix: &str, message: &str) -> String {
    let summary = message.lines().next().unwrap_or("").trim();
    // prefix + “ + summary + ”
    let budget = MAX_LABEL_CHARS.saturating_sub(prefix.chars().count() + 2);
    format!("{prefix}“{}”", truncate_end(summary, budget))
}

fn truncate_end(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let kept: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{kept}…")
}

fn ellipsize_middle(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let head = keep / 2;
    let tail = keep - head;
    let start: String = text.chars().take(head).collect();
    let end: String = text.chars().skip(count - tail).collect();
    format!("{start}…{end}")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoState {
    pub undo: Option<String>,
    pub redo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoOutcome {
    Done {
        label: String,
    },
    Nothing,
    RepositoryChanged {
        label: String,
    },
    /// A branch to move, delete or check out is held by a worktree (#291): `message` says which
    /// and how. The entry stays on its stack.
    BranchHeld {
        label: String,
        message: String,
    },
    OperationInProgress,
}

/// Records undo entries around worker commands. A paused operation's entry (`stacks.pending`)
/// lives in `undo.json` with the marker of the operation it belongs to
/// (`OperationProbe::marker`), so an operation finished after an app restart, or in another
/// process sharing the repository, is still one entry, and only that operation can record it.
///
/// Every write to `undo.json` happens under the repository's [`StateLock`], after a forced
/// reload, and re-checks its decision there (#172). Methods take the caller's guard when it holds
/// the lock (the worker holds it across commands that can end an operation); without one they
/// take it themselves only when about to write, and skip the write while another holder keeps it
/// (a later command repeats it).
pub(super) struct UndoRecorder {
    store: UndoStore,
    stacks: UndoStacks,
    active: Option<(UndoPlan, RepoSnapshot)>,
    /// A new label for the paused operation's entry, applied when it is recorded.
    pending_label: Option<String>,
    /// The `operation` of a pending entry that `settle` found stale before this command but
    /// couldn't drop (another holder kept the lock), so `finish` drops it rather than recording
    /// it (#175).
    stale_unsettled: Option<String>,
    lock: StateLock,
    logged_busy: bool,
    logged_unavailable: bool,
}

impl UndoRecorder {
    pub fn open(repo: &Repository) -> Self {
        let (store, stacks) = UndoStore::open(repo);
        Self {
            store,
            stacks,
            active: None,
            pending_label: None,
            stale_unsettled: None,
            lock: StateLock::open(repo),
            logged_busy: false,
            logged_unavailable: false,
        }
    }

    /// Makes the next write exclusive: with the caller's guard, or the lock taken within `wait`
    /// (returned, so the caller keeps it for the write), then a forced reload. `None` (logged
    /// once) while another holder keeps the lock; `Some(None)` too when locks don't work here,
    /// which carries on unlocked as before the lock existed.
    fn exclusive(
        &mut self,
        guard: Option<&StateGuard>,
        wait: Duration,
    ) -> Option<Option<StateGuard>> {
        let own = if guard.is_some() {
            None
        } else {
            match self.lock.acquire(wait) {
                Acquire::Held(guard) => Some(guard),
                Acquire::Busy => {
                    if !self.logged_busy {
                        self.logged_busy = true;
                        eprintln!(
                            "browsitory: undo history is locked by another window; not changing \
                             it now"
                        );
                    }
                    return None;
                }
                Acquire::Unavailable(error) => {
                    if !self.logged_unavailable {
                        self.logged_unavailable = true;
                        eprintln!("browsitory: undo history lock unavailable: {error}");
                    }
                    None
                }
            }
        };
        self.reload();
        Some(own)
    }

    /// A forced reload, except while `undo.json` was written by a newer version: it is never
    /// overwritten, so the in-memory stacks are this session's history. After any other failed
    /// save the file stays authoritative (it may hold the other window's newer entries).
    fn reload(&mut self) {
        if !self.store.is_blocked() {
            self.store.reload(&mut self.stacks);
        }
    }

    /// Renames the entry being recorded (the running command's, or the paused operation's).
    /// A renamed pending entry gets its new label when the `finish` that follows records it.
    pub fn relabel(&mut self, label: String) {
        if let Some((plan, _)) = self.active.as_mut() {
            plan.label = label;
        } else {
            self.pending_label = Some(label);
        }
    }

    pub fn has_pending(&self) -> bool {
        self.stacks.pending.is_some()
    }

    fn stale(&self, marker: Option<&str>) -> bool {
        self.stacks
            .pending
            .as_ref()
            .is_some_and(|pending| Some(pending.operation.as_str()) != marker)
    }

    /// Runs before every command with the marker of the operation in progress (`None`: none).
    /// A pending entry whose operation is no longer the one in progress ended outside the
    /// worker (e.g. `git merge --abort` in a terminal, maybe while the app was closed, maybe
    /// followed by another operation): whether it was finished or abandoned is unknowable, so
    /// it's dropped rather than recorded with whatever comes next. The drop is decided again
    /// under the lock: while another holder keeps it, that holder may be about to record the
    /// entry (its command ended the operation), so it is left alone, and remembered: the
    /// operation had ended before this command ran, so the `finish` after it drops the entry
    /// (if still pending) instead of recording it as this command's (#175).
    pub fn settle(
        &mut self,
        repo: &Repository,
        guard: Option<&StateGuard>,
        marker: impl Fn() -> Option<String>,
    ) {
        // A relabel is meant for the `finish` of the command that made it; one left over from a
        // command that didn't track (nothing was pending) must not rename a later entry.
        self.pending_label = None;
        self.stale_unsettled = None;
        if guard.is_some() {
            // `begin` runs next and decides from what's stored.
            self.reload();
        } else {
            self.store.reload_if_changed(&mut self.stacks);
        }
        if !self.stale(marker().as_deref()) {
            return;
        }
        let Some(_exclusive) = self.exclusive(guard, Duration::ZERO) else {
            self.stale_unsettled = self
                .stacks
                .pending
                .as_ref()
                .map(|pending| pending.operation.clone());
            return;
        };
        if self.stale(marker().as_deref()) {
            self.stacks.pending = None;
            self.persist(repo);
        }
    }

    /// Snapshots before a recordable command. Skipped while an operation is paused: whatever
    /// runs then (staging, resolving, the final commit) belongs to that operation's entry.
    pub fn begin(&mut self, repo: &Repository, plan: UndoPlan, marker: Option<&str>) {
        if self.stacks.pending.is_some() || marker.is_some() {
            return;
        }
        match capture(repo, plan.scope) {
            Ok(before) => self.active = Some((plan, before)),
            Err(error) => {
                eprintln!(
                    "browsitory: not recording \"{}\" for undo: {error}",
                    plan.label
                )
            }
        }
    }

    /// Runs after every recordable command and every command issued while one is pending, with
    /// the marker of the operation now in progress. A command that leaves an operation paused
    /// makes its entry pending (saved). `aborted` is true when the command was an abort (merge,
    /// rebase or sequence): a pending operation it ends is dropped rather than recorded — the
    /// user backed out of it. So is one that `settle` already found ended before this command
    /// (`stale_unsettled`): it ended outside, and its outcome is unknowable.
    pub fn finish(
        &mut self,
        repo: &Repository,
        guard: Option<&StateGuard>,
        marker: impl Fn() -> Option<String>,
        aborted: bool,
    ) {
        let relabel = self.pending_label.take();
        let stale_unsettled = self.stale_unsettled.take();
        if let Some((plan, before)) = self.active.take() {
            let Some(_exclusive) = self.exclusive(guard, Duration::ZERO) else {
                return;
            };
            if let Some(operation) = marker() {
                // Added to the freshly loaded stacks, so another writer's entries survive.
                self.stacks.pending = Some(PendingEntry {
                    label: plan.label,
                    before,
                    operation,
                });
                self.persist(repo);
                return;
            }
            self.record(repo, plan.label, plan.scope, before, false);
            return;
        }
        if marker().is_some() {
            return;
        }
        // Another process sharing `undo.json` may have recorded (or dropped) it already.
        self.store.reload_if_changed(&mut self.stacks);
        if self.stacks.pending.is_none() {
            return;
        }
        let Some(_exclusive) = self.exclusive(guard, Duration::ZERO) else {
            return;
        };
        if marker().is_some() {
            return;
        }
        if let Some(pending) = self.stacks.pending.take() {
            if aborted || stale_unsettled.as_ref() == Some(&pending.operation) {
                self.persist(repo);
            } else {
                let label = relabel.unwrap_or(pending.label);
                self.record(repo, label, SnapshotScope::Full, pending.before, true);
            }
        }
    }

    /// Pushes `label` (before → now) onto the undo stack. `ends_pending` when it records the
    /// paused operation's entry, which is then gone from `undo.json` too, recorded or not.
    /// The caller holds the lock (or locks are unavailable) and has just reloaded.
    fn record(
        &mut self,
        repo: &Repository,
        label: String,
        scope: SnapshotScope,
        before: RepoSnapshot,
        ends_pending: bool,
    ) {
        if ends_pending {
            self.stacks.pending = None;
        }
        let after = match capture(repo, scope) {
            Ok(after) => after,
            Err(error) => {
                eprintln!("browsitory: not recording \"{label}\" for undo: {error}");
                if ends_pending {
                    self.persist(repo);
                }
                return;
            }
        };
        if after == before {
            if ends_pending {
                self.persist(repo);
            }
            return;
        }
        let (before, after) = changed_refs_only(before, after);
        self.stacks.undo.push(UndoEntry {
            label,
            before,
            after,
        });
        self.stacks.redo.clear();
        let overflow = self.stacks.undo.len().saturating_sub(UNDO_LIMIT);
        self.stacks.undo.drain(..overflow);
        self.persist(repo);
    }

    pub fn state(&mut self) -> UndoState {
        self.store.reload_if_changed(&mut self.stacks);
        UndoState {
            undo: self.stacks.undo.last().map(|entry| entry.label.clone()),
            redo: self.stacks.redo.last().map(|entry| entry.label.clone()),
        }
    }

    /// Restores the last entry's "before". Under the caller's guard, or the lock taken within
    /// `LOCK_WAIT`; `UNDO_BUSY` (nothing restored) while another holder keeps it.
    pub fn undo(
        &mut self,
        repo: &Repository,
        guard: Option<&StateGuard>,
        marker: impl Fn() -> Option<String>,
    ) -> Result<UndoOutcome, ErrorDto> {
        self.step(repo, guard, marker, Direction::Undo)
    }

    pub fn redo(
        &mut self,
        repo: &Repository,
        guard: Option<&StateGuard>,
        marker: impl Fn() -> Option<String>,
    ) -> Result<UndoOutcome, ErrorDto> {
        self.step(repo, guard, marker, Direction::Redo)
    }

    fn step(
        &mut self,
        repo: &Repository,
        guard: Option<&StateGuard>,
        marker: impl Fn() -> Option<String>,
        direction: Direction,
    ) -> Result<UndoOutcome, ErrorDto> {
        let Some(_exclusive) = self.exclusive(guard, LOCK_WAIT) else {
            return Err(UNDO_BUSY.into());
        };
        if marker().is_some() || self.stacks.pending.is_some() {
            return Ok(UndoOutcome::OperationInProgress);
        }
        let (source, verb) = match direction {
            Direction::Undo => (&self.stacks.undo, "undo"),
            Direction::Redo => (&self.stacks.redo, "redo"),
        };
        let Some(entry) = source.last() else {
            return Ok(UndoOutcome::Nothing);
        };
        let (expected, target) = match direction {
            Direction::Undo => (&entry.after, &entry.before),
            Direction::Redo => (&entry.before, &entry.after),
        };
        let label = entry.label.clone();
        match restore(
            repo,
            expected,
            target,
            &format!("browsitory: {verb} {label}"),
        ) {
            Ok(()) => {}
            Err(UndoError::RepositoryChanged) => {
                return Ok(UndoOutcome::RepositoryChanged { label })
            }
            Err(UndoError::Held(held)) => {
                return Ok(UndoOutcome::BranchHeld {
                    label,
                    message: held.to_string(),
                })
            }
            Err(UndoError::OperationInProgress) => return Ok(UndoOutcome::OperationInProgress),
            Err(error) => return Err(ErrorDto::from_error(error)),
        }
        let (from, to) = match direction {
            Direction::Undo => (&mut self.stacks.undo, &mut self.stacks.redo),
            Direction::Redo => (&mut self.stacks.redo, &mut self.stacks.undo),
        };
        let entry = from.pop().expect("entry checked above");
        to.push(entry);
        self.persist(repo);
        Ok(UndoOutcome::Done { label })
    }

    /// A failed save never fails the user's command. When the file was written by a newer
    /// version the in-memory stacks are this session's history (see `reload`); after a transient
    /// failure the next locked write reloads the file first, so this entry may be lost but the
    /// other window's never are.
    fn persist(&mut self, repo: &Repository) {
        if let Err(error) = self.store.save(repo, &self.stacks) {
            eprintln!("browsitory: could not save undo history: {error}");
        }
    }
}

/// `restore` only checks and writes refs whose values differ between the two snapshots, so the
/// rest is dropped: otherwise every entry would store (and the anchor pin) every branch and tag.
/// A moved HEAD branch differs, so its commits stay in the entry and pinned.
fn changed_refs_only(
    mut before: RepoSnapshot,
    mut after: RepoSnapshot,
) -> (RepoSnapshot, RepoSnapshot) {
    let unchanged: Vec<String> = before
        .refs
        .iter()
        .filter(|(name, oid)| after.refs.get(*name) == Some(*oid))
        .map(|(name, _)| name.clone())
        .collect();
    for name in &unchanged {
        before.refs.remove(name);
        after.refs.remove(name);
    }
    (before, after)
}

#[derive(Clone, Copy)]
enum Direction {
    Undo,
    Redo,
}

/// Replies held back until the recorder has taken its "after" snapshot. Without this a caller
/// could act on the reply — or the user edit a file — before the snapshot, and that later change
/// would be folded into the entry (and silently reverted by its undo).
#[derive(Default)]
pub(super) struct HeldReplies(Vec<Box<dyn FnOnce()>>);

impl HeldReplies {
    pub(super) fn hold<T: 'static>(&mut self, reply: &mut Sender<T>) {
        let (proxy, rx) = std::sync::mpsc::channel();
        let original = std::mem::replace(reply, proxy);
        self.0.push(Box::new(move || {
            if let Ok(value) = rx.try_recv() {
                let _ = original.send(value);
            }
        }));
    }

    pub fn release(self) {
        for forward in self.0 {
            forward();
        }
    }
}

/// Swaps `command`'s reply for a proxy, keeping the original in `held`. The worker calls it for
/// every command whose policy holds its reply (`CommandPolicy::holds_reply`: everything recorded,
/// and the commands that can end a pending operation) while the recorder tracks the command.
pub(super) fn hold_reply(command: &mut Command, held: &mut HeldReplies) {
    if let Some(reply) = reply_of(command) {
        reply.hold(held);
    }
}

pub(super) fn get_state(recorder: &mut UndoRecorder, reply: Sender<Result<UndoState, ErrorDto>>) {
    let _ = reply.send(Ok(recorder.state()));
}

impl WorkerHandle {
    pub fn get_undo_state(&self) -> Result<UndoState, ErrorDto> {
        self.request(|reply| Command::GetUndoState { reply })
    }

    pub fn undo(&self) -> Result<UndoOutcome, ErrorDto> {
        self.request(|reply| Command::Undo { reply })
    }

    pub fn redo(&self) -> Result<UndoOutcome, ErrorDto> {
        self.request(|reply| Command::Redo { reply })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn plan_for(command: &Command) -> Option<UndoPlan> {
        super::super::policy::of(command).undo
    }

    fn label(command: Command) -> Option<(String, SnapshotScope)> {
        plan_for(&command).map(|plan| (plan.label, plan.scope))
    }

    #[test]
    fn a_pending_operation_ended_outside_the_worker_is_dropped() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let mut recorder = UndoRecorder::open(&repo);
        let plan = |label: &str| UndoPlan {
            label: label.into(),
            scope: SnapshotScope::Full,
        };
        // A merge stops on conflicts...
        recorder.begin(&repo, plan("Merge feature"), None);
        recorder.finish(&repo, None, || Some("merge a b".into()), false);
        assert!(recorder.has_pending());
        // ...and is aborted in a terminal. The next command sees nothing in progress.
        recorder.settle(&repo, None, || None);
        recorder.begin(&repo, plan("Add b.txt"), None);
        std::fs::write(dir.path().join("b.txt"), "b\n").unwrap();
        recorder.finish(&repo, None, || None, false);
        assert_eq!(recorder.state().undo, Some("Add b.txt".into()));
    }

    fn plan(label: &str) -> UndoPlan {
        UndoPlan {
            label: label.into(),
            scope: SnapshotScope::Full,
        }
    }

    fn hold(repo: &Repository) -> super::super::state_lock::StateGuard {
        match super::super::state_lock::StateLock::open(repo).try_acquire() {
            super::super::state_lock::Acquire::Held(guard) => guard,
            other => panic!("{other:?}"),
        }
    }

    /// Two recorders on one repository stand for two processes (#172).
    fn two_recorders() -> (tempfile::TempDir, Repository, UndoRecorder, UndoRecorder) {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let a = UndoRecorder::open(&repo);
        let b = UndoRecorder::open(&repo);
        (dir, repo, a, b)
    }

    /// Runs one recordable "command" in `recorder` that writes `name`.
    fn record_write(recorder: &mut UndoRecorder, repo: &Repository, name: &str) {
        recorder.settle(repo, None, || None);
        recorder.begin(repo, plan(name), None);
        std::fs::write(repo.workdir().unwrap().join(name), name).unwrap();
        recorder.finish(repo, None, || None, false);
    }

    fn stored_pending(repo: &Repository) -> Option<String> {
        UndoStore::open(repo).1.pending.map(|pending| pending.label)
    }

    #[test]
    fn settle_keeps_a_pending_entry_while_another_holder_has_the_lock() {
        let (dir, repo, mut a, mut b) = two_recorders();
        a.begin(&repo, plan("Merge X"), None);
        a.finish(&repo, None, || Some("merge X".into()), false);
        assert_eq!(stored_pending(&repo), Some("Merge X".into()));

        let guard = hold(&repo);
        // The operation looks ended to B (A cleared its state but hasn't recorded yet).
        b.settle(&repo, None, || None);
        drop(guard);
        assert_eq!(stored_pending(&repo), Some("Merge X".into()));

        std::fs::write(dir.path().join("merged.txt"), "m").unwrap();
        a.finish(&repo, None, || None, false);
        assert_eq!(a.state().undo, Some("Merge X".into()));
        assert_eq!(b.state().undo, Some("Merge X".into()));
        assert_eq!(stored_pending(&repo), None);
    }

    #[test]
    fn settle_drops_a_stale_pending_entry_when_the_lock_is_free() {
        let (_dir, repo, mut a, mut b) = two_recorders();
        a.begin(&repo, plan("Merge X"), None);
        a.finish(&repo, None, || Some("merge X".into()), false);
        b.settle(&repo, None, || None);
        assert_eq!(stored_pending(&repo), None);
    }

    #[test]
    fn finish_reloads_before_adding_a_pending_entry() {
        let (_dir, repo, mut a, mut b) = two_recorders();
        record_write(&mut b, &repo, "B1");
        // A loaded before B saved, and starts a merge that pauses.
        a.begin(&repo, plan("Merge X"), None);
        a.finish(&repo, None, || Some("merge X".into()), false);
        let (_, stored) = UndoStore::open(&repo);
        assert_eq!(
            stored
                .undo
                .iter()
                .map(|e| e.label.as_str())
                .collect::<Vec<_>>(),
            ["B1"]
        );
        assert_eq!(stored.pending.map(|p| p.label), Some("Merge X".into()));
    }

    #[test]
    fn recording_in_turn_keeps_both_entries() {
        let (_dir, repo, mut a, mut b) = two_recorders();
        record_write(&mut a, &repo, "A1");
        record_write(&mut b, &repo, "B1");
        record_write(&mut a, &repo, "A2");
        for recorder in [&mut a, &mut b] {
            recorder.store.reload(&mut recorder.stacks);
            assert_eq!(
                recorder
                    .stacks
                    .undo
                    .iter()
                    .map(|e| e.label.as_str())
                    .collect::<Vec<_>>(),
                ["A1", "B1", "A2"]
            );
        }
    }

    #[test]
    fn undo_reports_busy_while_the_lock_stays_held() {
        let (dir, repo, mut a, _b) = two_recorders();
        record_write(&mut a, &repo, "A1");
        let _guard = hold(&repo);
        let start = std::time::Instant::now();
        assert_eq!(
            a.undo(&repo, None, || None),
            Err(crate::wire::ErrorDto::from(UNDO_BUSY))
        );
        assert!(start.elapsed() >= super::super::state_lock::LOCK_WAIT);
        assert_eq!(
            a.redo(&repo, None, || None),
            Err(crate::wire::ErrorDto::from(UNDO_BUSY))
        );
        assert!(dir.path().join("A1").exists());
        assert_eq!(a.state().undo, Some("A1".into()));
    }

    #[test]
    fn a_failed_save_keeps_the_session_history() {
        // Written by a newer version: never overwritten, so every save fails. The in-memory
        // stacks stay authoritative for the session, also under the lock's forced reloads.
        let (dir, repo, _, _) = two_recorders();
        std::fs::create_dir_all(repo.path().join("browsitory")).unwrap();
        std::fs::write(
            repo.path().join("browsitory/undo.json"),
            r#"{"version":99,"undo":[],"redo":[]}"#,
        )
        .unwrap();
        let mut a = UndoRecorder::open(&repo);
        record_write(&mut a, &repo, "A1");
        assert_eq!(a.state().undo, Some("A1".into()));
        assert_eq!(
            a.undo(&repo, None, || None),
            Ok(UndoOutcome::Done { label: "A1".into() })
        );
        assert!(!dir.path().join("A1").exists());
    }

    /// F3: only "written by a newer version" makes the session's history authoritative. After a
    /// transient save failure the next write still reloads, so it can't overwrite the other
    /// window's entries with stale stacks.
    #[test]
    #[cfg(unix)]
    fn a_transient_save_failure_keeps_reloading() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, repo, mut a, mut b) = two_recorders();
        record_write(&mut a, &repo, "A0"); // creates the folder
        let folder = repo.path().join("browsitory");
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o555)).unwrap();
        if std::fs::write(folder.join("probe"), "").is_ok() {
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
            return; // root: no reliable failure to provoke
        }
        record_write(&mut a, &repo, "A1"); // its save fails
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o755)).unwrap();
        record_write(&mut b, &repo, "B1");
        // A's next command holds the lock, as the worker does for recordable commands.
        let guard = hold(&repo);
        a.settle(&repo, Some(&guard), || None);
        a.begin(&repo, plan("A2"), None);
        std::fs::write(repo.workdir().unwrap().join("A2"), "A2").unwrap();
        a.finish(&repo, Some(&guard), || None, false);
        drop(guard);
        let (_, stored) = UndoStore::open(&repo);
        let labels: Vec<_> = stored.undo.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, ["A0", "B1", "A2"]);
    }

    #[test]
    fn a_relabel_with_nothing_pending_never_labels_a_later_entry() {
        let (dir, repo, mut a, mut b) = two_recorders();
        // In A, a sequence completed unrecorded: it relabels, and no `finish` follows.
        a.settle(&repo, None, || None);
        a.relabel("Cherry-pick 3 commits".into());
        // In B, a merge pauses.
        b.settle(&repo, None, || None);
        b.begin(&repo, plan("Merge X"), None);
        b.finish(&repo, None, || Some("merge X".into()), false);
        // A commits it.
        a.settle(&repo, None, || Some("merge X".into()));
        std::fs::write(dir.path().join("m"), "m").unwrap();
        a.finish(&repo, None, || None, false);
        assert_eq!(a.state().undo, Some("Merge X".into()));
    }

    #[test]
    fn undo_busy_copy_is_exact() {
        assert_eq!(
            UNDO_BUSY,
            "Another Browsitory window is changing this repository's undo history. Try again \
             in a moment."
        );
    }

    #[test]
    fn entries_store_only_the_refs_they_change() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("Test User", "test@example.com").unwrap();
        let head = repo
            .commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        let head = repo.find_commit(head).unwrap();
        repo.branch("other", &head, false).unwrap();
        repo.tag_lightweight("v1", head.as_object(), false).unwrap();
        let plan = |label: &str| UndoPlan {
            label: label.into(),
            scope: SnapshotScope::RefsAndIndex,
        };

        let mut recorder = UndoRecorder::open(&repo);
        recorder.begin(&repo, plan("Stage a.txt"), None);
        std::fs::write(dir.path().join("a.txt"), "two\n").unwrap();
        index.add_path(std::path::Path::new("a.txt")).unwrap();
        index.write().unwrap();
        recorder.finish(&repo, None, || None, false);
        let staged = recorder.stacks.undo.last().unwrap();
        assert!(staged.before.refs.is_empty() && staged.after.refs.is_empty());

        recorder.begin(&repo, plan("Create branch x"), None);
        repo.branch("x", &head, false).unwrap();
        recorder.finish(&repo, None, || None, false);
        let created = recorder.stacks.undo.last().unwrap();
        assert!(created.before.refs.is_empty());
        assert_eq!(
            created.after.refs.keys().collect::<Vec<_>>(),
            vec!["refs/heads/x"]
        );

        let (_, reloaded) = UndoStore::open(&repo);
        assert_eq!(reloaded.undo, recorder.stacks.undo);
        let json = std::fs::read_to_string(repo.path().join("browsitory/undo.json")).unwrap();
        assert!(!json.contains("refs/heads/other") && !json.contains("refs/tags/v1"));

        let mut reopened = UndoRecorder::open(&repo);
        assert_eq!(
            reopened.undo(&repo, None, || None),
            Ok(UndoOutcome::Done {
                label: "Create branch x".into()
            })
        );
        assert!(repo.find_branch("x", git2::BranchType::Local).is_err());
        assert!(repo.find_branch("other", git2::BranchType::Local).is_ok());
    }

    #[test]
    fn labels_index_and_ref_commands() {
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::StageFile {
                path: "src/a.ts".into(),
                reply: tx
            }),
            Some(("Stage src/a.ts".into(), SnapshotScope::RefsAndIndex))
        );
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::Commit {
                message: "Fix parser\n\nbody".into(),
                no_verify: false,
                trailers: Vec::new(),
                operation_id: "commit-1".into(),
                events: channel().0,
                reply: tx
            }),
            Some(("Commit “Fix parser”".into(), SnapshotScope::RefsAndIndex))
        );
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::RenameBranch {
                old_name: "a".into(),
                new_name: "b".into(),
                reply: tx
            }),
            Some(("Rename branch a to b".into(), SnapshotScope::RefsAndIndex))
        );
    }

    #[test]
    fn labels_worktree_commands_with_full_scope() {
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::DiscardFile {
                path: "a.txt".into(),
                reply: tx
            }),
            Some(("Discard changes to a.txt".into(), SnapshotScope::Full))
        );
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::ResetToCommit {
                commit_id: "1a2b3c4d5e6f".into(),
                mode: git_core::reset::ResetMode::Hard,
                reply: tx
            }),
            Some(("Reset to 1a2b3c4 (hard)".into(), SnapshotScope::Full))
        );
    }

    #[test]
    fn merge_labels_name_squash_and_strip_ref_prefixes() {
        let merge = |branch_name: &str, squash: bool| {
            let (tx, _rx) = channel();
            label(Command::StartMerge {
                operation_id: String::new().into(),
                events: channel().0,
                branch_name: branch_name.into(),
                options: git_core::merge::MergeOptions {
                    squash,
                    ..Default::default()
                },
                reply: tx,
            })
        };
        assert_eq!(
            merge("feature", false),
            Some(("Merge feature".into(), SnapshotScope::Full))
        );
        assert_eq!(
            merge("feature", true),
            Some(("Squash merge feature".into(), SnapshotScope::Full))
        );
        assert_eq!(
            merge("refs/remotes/origin/main", false),
            Some(("Merge origin/main".into(), SnapshotScope::Full))
        );
        assert_eq!(
            merge("refs/heads/topic", false),
            Some(("Merge topic".into(), SnapshotScope::Full))
        );
    }

    #[test]
    fn read_and_transfer_commands_are_not_recorded() {
        let (tx, _rx) = channel();
        assert_eq!(label(Command::GetStatus { reply: tx }), None);
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::SaveStash {
                options: git_core::stash::StashOptions::default(),
                reply: tx
            }),
            None
        );
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::PopStash {
                index: 0,
                commit_id: "abc".into(),
                restore_index: true,
                reply: tx
            }),
            None
        );
    }

    #[test]
    fn long_paths_are_middle_ellipsized_to_80_chars() {
        let path = format!("{}/file.rs", "deep/".repeat(30));
        let (tx, _rx) = channel();
        let (text, _) = label(Command::StageFile { path, reply: tx }).unwrap();
        assert_eq!(text.chars().count(), 80);
        assert!(text.starts_with("Stage deep/"));
        assert!(text.ends_with("/file.rs"));
        assert!(text.contains('…'));
    }

    #[test]
    fn long_summaries_are_end_truncated() {
        let (tx, _rx) = channel();
        let (text, _) = label(Command::Commit {
            message: "x".repeat(200),
            no_verify: false,
            trailers: Vec::new(),
            operation_id: "commit-1".into(),
            events: channel().0,
            reply: tx,
        })
        .unwrap();
        assert_eq!(text.chars().count(), 80);
        assert!(text.ends_with("…”"));
    }

    #[test]
    fn sequences_are_labelled_by_count() {
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::CherryPickMany {
                operation_id: String::new().into(),
                events: channel().0,
                commit_ids: vec!["a".into(), "b".into(), "c".into()],
                reply: tx
            }),
            Some(("Cherry-pick 3 commits".into(), SnapshotScope::Full))
        );
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::RevertMany {
                operation_id: String::new().into(),
                events: channel().0,
                commit_ids: vec!["a".into()],
                reply: tx
            }),
            Some(("Revert 1 commit".into(), SnapshotScope::Full))
        );
    }

    #[test]
    fn non_ascii_commit_ids_are_shortened_by_chars() {
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::CheckoutCommit {
                operation_id: String::new().into(),
                events: channel().0,
                commit_id: "äöüäöüäöü".into(),
                reply: tx
            })
            .map(|(l, _)| l),
            Some("Check out äöüäöüä".into())
        );
    }

    const ONTO: &str = "707e824e1af1b266aaa5c4faf7517e586b174183";

    fn start_rebase(onto: &str, edit: Option<HistoryEdit>) -> Command {
        Command::StartRebase {
            onto: onto.into(),
            plan: Vec::new(),
            edit,
            operation_id: "rebase-1".into(),
            events: channel().0,
            reply: channel().0,
        }
    }

    fn rebase_label(onto: &str) -> Option<String> {
        label(start_rebase(onto, None)).map(|(l, _)| l)
    }

    #[test]
    fn rebase_labels_shorten_a_commit_id_and_keep_a_ref_name() {
        assert_eq!(rebase_label(ONTO), Some("Rebase onto 707e824".into()));
        assert_eq!(rebase_label("main"), Some("Rebase onto main".into()));
        assert_eq!(
            rebase_label("origin/feature"),
            Some("Rebase onto origin/feature".into())
        );
    }

    /// #619: the branch menus and the pull's Rebase pass a full ref name; the label names the
    /// branch as the branch list shows it.
    #[test]
    fn rebase_labels_name_a_branch_by_its_short_name() {
        assert_eq!(
            rebase_label("refs/heads/feat/x"),
            Some("Rebase onto feat/x".into())
        );
        assert_eq!(
            rebase_label("refs/remotes/origin/main"),
            Some("Rebase onto origin/main".into())
        );
        assert_eq!(
            rebase_label("refs/tags/v1"),
            Some("Rebase onto refs/tags/v1".into())
        );
    }

    const EDITED: &str = "1a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d";

    /// #473: a one-click history edit's entry names the edit and the edited commit, not `onto`.
    #[test]
    fn a_history_edit_is_labelled_with_what_the_user_did() {
        let cases = [
            (HistoryEditKind::Reword, "Reword 1a2b3c4"),
            (HistoryEditKind::Squash, "Squash 1a2b3c4 into its parent"),
            (HistoryEditKind::Fixup, "Fix up 1a2b3c4 into its parent"),
            (HistoryEditKind::MoveUp, "Move 1a2b3c4 up"),
            (HistoryEditKind::MoveDown, "Move 1a2b3c4 down"),
            (HistoryEditKind::Drop, "Drop 1a2b3c4"),
        ];
        for (kind, expected) in cases {
            let edit = HistoryEdit {
                kind,
                commit_id: EDITED.into(),
            };
            assert_eq!(
                label(start_rebase(ONTO, Some(edit))),
                Some((expected.to_string(), SnapshotScope::Full)),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn a_failed_rebase_start_is_labelled_as_failed() {
        assert_eq!(
            failed_rebase_label("Rebase onto 707e824", true),
            "Rebase onto 707e824 failed (rolled back)"
        );
        assert_eq!(
            failed_rebase_label("Rebase onto 707e824", false),
            "Rebase onto 707e824 failed"
        );

        let dir = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let mut recorder = UndoRecorder::open(&repo);
        let command = start_rebase(ONTO, None);
        let plan = plan_for(&command).unwrap();
        recorder.begin(&repo, plan.clone(), None);
        std::fs::write(dir.path().join("b.txt"), "b\n").unwrap();
        relabel_rebase_start(
            &mut recorder,
            &repo,
            &plan.label,
            &Err("Signing failed".into()),
        );
        recorder.finish(&repo, None, || None, false);
        assert_eq!(
            recorder.state().undo,
            Some("Rebase onto 707e824 failed (rolled back)".into())
        );

        recorder.begin(&repo, plan.clone(), None);
        std::fs::write(dir.path().join("c.txt"), "c\n").unwrap();
        relabel_rebase_start(
            &mut recorder,
            &repo,
            &plan.label,
            &Ok(git_core::rebase::RebaseStepResult::Done),
        );
        recorder.finish(&repo, None, || None, false);
        assert_eq!(recorder.state().undo, Some("Rebase onto 707e824".into()));

        // A failed history edit keeps its own label (#473).
        let edit = HistoryEdit {
            kind: HistoryEditKind::Drop,
            commit_id: EDITED.into(),
        };
        let plan = plan_for(&start_rebase(ONTO, Some(edit))).unwrap();
        recorder.begin(&repo, plan.clone(), None);
        std::fs::write(dir.path().join("d.txt"), "d\n").unwrap();
        relabel_rebase_start(
            &mut recorder,
            &repo,
            &plan.label,
            &Err("Signing failed".into()),
        );
        recorder.finish(&repo, None, || None, false);
        assert_eq!(
            recorder.state().undo,
            Some("Drop 1a2b3c4 failed (rolled back)".into())
        );
    }

    #[test]
    fn empty_amend_message_has_a_fallback_label() {
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::AmendCommit {
                message: "  ".into(),
                no_verify: false,
                author: None,
                trailers: Vec::new(),
                operation_id: "amend-1".into(),
                events: channel().0,
                reply: tx
            })
            .map(|(l, _)| l),
            Some("Amend last commit".into())
        );
    }
}
