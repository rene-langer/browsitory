//! Undo stack: which worker commands are recorded, under which label, and the recorder that
//! snapshots around them. See `docs/superpowers/specs/2026-10-18-sm-parity-batch3b-design.md`.
use std::sync::mpsc::Sender;

use git2::Repository;
use git_core::reset::ResetMode;
use git_core::undo::{capture, restore, RepoSnapshot, SnapshotScope, UndoError};

use super::undo_store::{UndoEntry, UndoStacks, UndoStore, UNDO_LIMIT};
use super::{Command, WorkerHandle};

pub(super) const MAX_LABEL_CHARS: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UndoPlan {
    pub label: String,
    pub scope: SnapshotScope,
}

pub(super) fn plan_for(command: &Command) -> Option<UndoPlan> {
    use SnapshotScope::{Full, RefsAndIndex};
    let (label, scope) = match command {
        Command::StageFile { path, .. } => (with_path("Stage ", path, ""), RefsAndIndex),
        Command::UnstageFile { path, .. } => (with_path("Unstage ", path, ""), RefsAndIndex),
        Command::StageHunk { path, .. } => (with_path("Stage hunk in ", path, ""), RefsAndIndex),
        Command::UnstageHunk { path, .. } => {
            (with_path("Unstage hunk in ", path, ""), RefsAndIndex)
        }
        Command::StageLines { path, .. } => (with_path("Stage lines in ", path, ""), RefsAndIndex),
        Command::UnstageLines { path, .. } => {
            (with_path("Unstage lines in ", path, ""), RefsAndIndex)
        }
        Command::Commit { message, .. } => (quoted("Commit ", message), RefsAndIndex),
        Command::AmendCommit { message, .. } => {
            if message.trim().is_empty() {
                ("Amend last commit".to_string(), RefsAndIndex)
            } else {
                (quoted("Amend ", message), RefsAndIndex)
            }
        }
        Command::CreateBranch { name, .. } => (fit(format!("Create branch {name}")), RefsAndIndex),
        Command::DeleteBranch { name, .. } => (fit(format!("Delete branch {name}")), RefsAndIndex),
        Command::RenameBranch {
            old_name, new_name, ..
        } => (
            fit(format!("Rename branch {old_name} to {new_name}")),
            RefsAndIndex,
        ),
        Command::CreateTag { name, .. } => (fit(format!("Create tag {name}")), RefsAndIndex),
        Command::DeleteTag { name, .. } => (fit(format!("Delete tag {name}")), RefsAndIndex),
        Command::DiscardHunk { path, .. } => (with_path("Discard hunk in ", path, ""), Full),
        Command::DiscardLines { path, .. } => (with_path("Discard lines in ", path, ""), Full),
        Command::DiscardFile { path, .. } => (with_path("Discard changes to ", path, ""), Full),
        Command::DiscardAllUnstaged { .. } => ("Discard all unstaged changes".to_string(), Full),
        Command::IgnorePath { pattern, .. } => (with_path("Ignore ", pattern, ""), Full),
        Command::SwitchBranch { name, .. } => (fit(format!("Switch to {name}")), Full),
        Command::CheckoutCommit { commit_id, .. } => {
            (format!("Check out {}", short(commit_id)), Full)
        }
        Command::ResetToCommit {
            commit_id, mode, ..
        } => {
            let mode = match mode {
                ResetMode::Soft => "soft",
                ResetMode::Mixed => "mixed",
                ResetMode::Hard => "hard",
            };
            (format!("Reset to {} ({mode})", short(commit_id)), Full)
        }
        Command::CherryPick { commit_id, .. } => {
            (format!("Cherry-pick {}", short(commit_id)), Full)
        }
        Command::RevertCommit { commit_id, .. } => (format!("Revert {}", short(commit_id)), Full),
        Command::StartMerge { branch_name, .. } => (fit(format!("Merge {branch_name}")), Full),
        Command::StartRebase { onto, .. } => (fit(format!("Rebase onto {onto}")), Full),
        Command::PullCurrentUpstream { .. } => ("Pull".to_string(), Full),
        Command::RestoreReflogEntry {
            reference, new_id, ..
        } => (
            fit(format!("Restore {reference} to {}", short(new_id))),
            Full,
        ),
        _ => return None,
    };
    Some(UndoPlan { label, scope })
}

/// Char-based: an id that isn't a real hex oid (any user/IPC string) must not panic.
fn short(oid: &str) -> String {
    oid.chars().take(7).collect()
}

fn fit(label: String) -> String {
    truncate_end(&label, MAX_LABEL_CHARS)
}

fn with_path(prefix: &str, path: &str, suffix: &str) -> String {
    let budget = MAX_LABEL_CHARS.saturating_sub(prefix.chars().count() + suffix.chars().count());
    format!("{prefix}{}{suffix}", ellipsize_middle(path, budget))
}

fn quoted(prefix: &str, message: &str) -> String {
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
    Done { label: String },
    Nothing,
    RepositoryChanged { label: String },
    OperationInProgress,
}

pub(super) struct UndoRecorder {
    store: UndoStore,
    stacks: UndoStacks,
    active: Option<(UndoPlan, RepoSnapshot)>,
    /// An operation that stopped mid-way (conflicted merge/cherry-pick/revert, paused rebase):
    /// its label and pre-operation snapshot, recorded once nothing is in progress any more.
    pending: Option<(String, RepoSnapshot)>,
}

impl UndoRecorder {
    pub fn open(repo: &Repository) -> Self {
        let (store, stacks) = UndoStore::open(repo);
        Self {
            store,
            stacks,
            active: None,
            pending: None,
        }
    }

    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Runs before every command. A pending operation that is no longer in progress ended
    /// outside the worker (e.g. `git merge --abort` in a terminal): whether it was finished or
    /// abandoned is unknowable, so it's dropped rather than recorded with whatever comes next.
    pub fn settle(&mut self, in_progress: bool) {
        if !in_progress {
            self.pending = None;
        }
    }

    /// Snapshots before a recordable command. Skipped while an operation is paused: whatever
    /// runs then (staging, resolving, the final commit) belongs to that operation's entry.
    pub fn begin(&mut self, repo: &Repository, plan: UndoPlan, in_progress: bool) {
        if self.pending.is_some() || in_progress {
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

    /// Runs after every recordable command and every command issued while one is pending.
    /// `aborted` is true when the command was an abort (merge or rebase): a pending operation
    /// it ends is dropped rather than recorded — the user backed out of it.
    pub fn finish(&mut self, repo: &Repository, in_progress: bool, aborted: bool) {
        if let Some((plan, before)) = self.active.take() {
            if in_progress {
                self.pending = Some((plan.label, before));
                return;
            }
            self.record(repo, plan.label, plan.scope, before);
            return;
        }
        if in_progress {
            return;
        }
        if let Some((label, before)) = self.pending.take() {
            if !aborted {
                self.record(repo, label, SnapshotScope::Full, before);
            }
        }
    }

    fn record(
        &mut self,
        repo: &Repository,
        label: String,
        scope: SnapshotScope,
        before: RepoSnapshot,
    ) {
        let after = match capture(repo, scope) {
            Ok(after) => after,
            Err(error) => {
                eprintln!("browsitory: not recording \"{label}\" for undo: {error}");
                return;
            }
        };
        if after == before {
            return;
        }
        self.store.reload_if_changed(&mut self.stacks);
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

    pub fn undo(&mut self, repo: &Repository, in_progress: bool) -> Result<UndoOutcome, String> {
        self.step(repo, in_progress, Direction::Undo)
    }

    pub fn redo(&mut self, repo: &Repository, in_progress: bool) -> Result<UndoOutcome, String> {
        self.step(repo, in_progress, Direction::Redo)
    }

    fn step(
        &mut self,
        repo: &Repository,
        in_progress: bool,
        direction: Direction,
    ) -> Result<UndoOutcome, String> {
        self.store.reload_if_changed(&mut self.stacks);
        if in_progress || self.pending.is_some() {
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
            Err(UndoError::OperationInProgress) => return Ok(UndoOutcome::OperationInProgress),
            Err(error) => return Err(error.to_string()),
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

    /// A failed save (e.g. the history file was written by a newer version) never fails the
    /// user's command: the in-memory stacks stay authoritative for this session.
    fn persist(&mut self, repo: &Repository) {
        if let Err(error) = self.store.save(repo, &self.stacks) {
            eprintln!("browsitory: could not save undo history: {error}");
        }
    }
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
    fn hold<T: 'static>(&mut self, reply: &mut Sender<T>) {
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

/// Swaps the reply of every command that can change what the recorder snapshots: everything
/// `plan_for` records, plus the commands that can end a pending operation (`RebaseContinue`,
/// `AbortMerge`, `AbortRebase`). Guarded by `every_recordable_command_has_its_reply_held`.
///
/// Arms are grouped by reply type: an or-pattern must bind `reply` at one type, so commands
/// whose reply isn't `Sender<Result<(), String>>` (`Commit` replies with the new oid,
/// `StartMerge` with a `MergeOutcome`, ...) get arms of their own.
pub(super) fn hold_reply(command: &mut Command, held: &mut HeldReplies) {
    match command {
        Command::StageFile { reply, .. }
        | Command::UnstageFile { reply, .. }
        | Command::StageHunk { reply, .. }
        | Command::UnstageHunk { reply, .. }
        | Command::StageLines { reply, .. }
        | Command::UnstageLines { reply, .. }
        | Command::AmendCommit { reply, .. }
        | Command::CreateBranch { reply, .. }
        | Command::DeleteBranch { reply, .. }
        | Command::RenameBranch { reply, .. }
        | Command::CreateTag { reply, .. }
        | Command::DeleteTag { reply, .. }
        | Command::DiscardHunk { reply, .. }
        | Command::DiscardLines { reply, .. }
        | Command::DiscardFile { reply, .. }
        | Command::DiscardAllUnstaged { reply, .. }
        | Command::IgnorePath { reply, .. }
        | Command::SwitchBranch { reply, .. }
        | Command::CheckoutCommit { reply, .. }
        | Command::ResetToCommit { reply, .. }
        | Command::RestoreReflogEntry { reply, .. }
        | Command::AbortMerge { reply, .. }
        | Command::AbortRebase { reply, .. } => held.hold(reply),
        Command::Commit { reply, .. } => held.hold(reply),
        Command::StartMerge { reply, .. } => held.hold(reply),
        Command::StartRebase { reply, .. } | Command::RebaseContinue { reply, .. } => {
            held.hold(reply)
        }
        Command::PullCurrentUpstream { reply, .. } => held.hold(reply),
        Command::CherryPick { reply, .. } | Command::RevertCommit { reply, .. } => held.hold(reply),
        _ => {}
    }
}

pub(super) fn get_state(recorder: &mut UndoRecorder, reply: Sender<Result<UndoState, String>>) {
    let _ = reply.send(Ok(recorder.state()));
}

impl WorkerHandle {
    pub fn get_undo_state(&self) -> Result<UndoState, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetUndoState { reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn undo(&self) -> Result<UndoOutcome, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::Undo { reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn redo(&self) -> Result<UndoOutcome, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::Redo { reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

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
        recorder.begin(&repo, plan("Merge feature"), false);
        recorder.finish(&repo, true, false);
        assert!(recorder.has_pending());
        // ...and is aborted in a terminal. The next command sees nothing in progress.
        recorder.settle(false);
        recorder.begin(&repo, plan("Add b.txt"), false);
        std::fs::write(dir.path().join("b.txt"), "b\n").unwrap();
        recorder.finish(&repo, false, false);
        assert_eq!(recorder.state().undo, Some("Add b.txt".into()));
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
    fn read_and_transfer_commands_are_not_recorded() {
        let (tx, _rx) = channel();
        assert_eq!(label(Command::GetStatus { reply: tx }), None);
        let (tx, _rx) = channel();
        assert_eq!(label(Command::SaveStash { reply: tx }), None);
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
            reply: tx,
        })
        .unwrap();
        assert_eq!(text.chars().count(), 80);
        assert!(text.ends_with("…”"));
    }

    /// Builds the command with a fresh reply channel, then checks it is recordable and that
    /// `hold_reply` moved the original sender into `HeldReplies`: with the command (and its
    /// proxy sender) dropped, the original receiver stays connected but empty until release.
    macro_rules! assert_held {
        ($variant:ident { $($field:ident: $value:expr),* $(,)? }) => {{
            let (reply, rx) = channel();
            let mut command = Command::$variant { $($field: $value,)* reply };
            assert!(plan_for(&command).is_some(), "{} is not recordable", stringify!($variant));
            let mut held = HeldReplies::default();
            hold_reply(&mut command, &mut held);
            drop(command);
            assert_eq!(
                rx.try_recv().map(|_| ()),
                Err(std::sync::mpsc::TryRecvError::Empty),
                "{} reply was not held",
                stringify!($variant)
            );
            held.release();
            assert_eq!(
                rx.try_recv().map(|_| ()),
                Err(std::sync::mpsc::TryRecvError::Disconnected)
            );
        }};
    }

    #[test]
    fn every_recordable_command_has_its_reply_held() {
        let s = || String::from("x");
        assert_held!(StageFile { path: s() });
        assert_held!(UnstageFile { path: s() });
        assert_held!(StageHunk {
            path: s(),
            old_start: 1,
            new_start: 1,
            context_lines: 3
        });
        assert_held!(UnstageHunk {
            path: s(),
            old_start: 1,
            new_start: 1,
            context_lines: 3
        });
        assert_held!(StageLines {
            path: s(),
            old_start: 1,
            new_start: 1,
            context_lines: 3,
            line_indices: vec![],
            expected_lines: vec![],
        });
        assert_held!(UnstageLines {
            path: s(),
            old_start: 1,
            new_start: 1,
            context_lines: 3,
            line_indices: vec![],
            expected_lines: vec![],
        });
        assert_held!(Commit { message: s() });
        assert_held!(AmendCommit { message: s() });
        assert_held!(CreateBranch {
            name: s(),
            start_point: s()
        });
        assert_held!(DeleteBranch {
            name: s(),
            force: false
        });
        assert_held!(RenameBranch {
            old_name: s(),
            new_name: s()
        });
        assert_held!(CreateTag {
            name: s(),
            message: None
        });
        assert_held!(DeleteTag { name: s() });
        assert_held!(DiscardHunk {
            path: s(),
            old_start: 1,
            new_start: 1,
            context_lines: 3
        });
        assert_held!(DiscardLines {
            path: s(),
            old_start: 1,
            new_start: 1,
            context_lines: 3,
            line_indices: vec![],
            expected_lines: vec![],
        });
        assert_held!(DiscardFile { path: s() });
        assert_held!(DiscardAllUnstaged { paths: vec![] });
        assert_held!(IgnorePath { pattern: s() });
        assert_held!(SwitchBranch { name: s() });
        assert_held!(CheckoutCommit { commit_id: s() });
        assert_held!(ResetToCommit {
            commit_id: s(),
            mode: ResetMode::Hard
        });
        assert_held!(CherryPick { commit_id: s() });
        assert_held!(RevertCommit { commit_id: s() });
        assert_held!(StartMerge { branch_name: s() });
        assert_held!(StartRebase {
            onto: s(),
            plan: vec![]
        });
        assert_held!(PullCurrentUpstream {
            operation_id: s(),
            events: channel().0
        });
        assert_held!(RestoreReflogEntry {
            reference: s(),
            new_id: s()
        });
    }

    #[test]
    fn non_ascii_commit_ids_are_shortened_by_chars() {
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::CheckoutCommit {
                commit_id: "äöüäöüäöü".into(),
                reply: tx
            })
            .map(|(l, _)| l),
            Some("Check out äöüäöüä".into())
        );
    }

    #[test]
    fn empty_amend_message_has_a_fallback_label() {
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::AmendCommit {
                message: "  ".into(),
                reply: tx
            })
            .map(|(l, _)| l),
            Some("Amend last commit".into())
        );
    }
}
