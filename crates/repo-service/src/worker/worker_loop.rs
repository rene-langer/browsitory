//! The worker thread's state and the steps every command goes through (#486): the state lock,
//! undo bookkeeping and the operation gate around `dispatch` (`worker_loop/dispatch.rs`), and the
//! read lane a transfer serves while the worker waits for it (`worker_loop/read.rs`, #734).

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use crate::credentials::{CredentialService, CredentialStore};
use crate::pull_requests::{ForgeApi, PullRequestService};

use super::policy::{self, BusyReply, CommandPolicy};
use super::rebase_store::RebaseStore;
use super::sequence_store::SequenceStore;
use super::state_lock::{self, StateGuard};
use super::{
    background_fetch, gate, merge, rebase, undo, Command, PauseHook, PausePoint, SearchGeneration,
};

mod dispatch;
mod read;

/// Everything the worker thread keeps between commands.
pub(super) struct WorkerLoop<S: CredentialStore, A: ForgeApi> {
    repo_path: PathBuf,
    repo: git2::Repository,
    // The commands sent to this worker.
    queue: Receiver<Command>,
    // Commands a transfer's wait took from `queue` but couldn't run (everything not on the read
    // lane); they run, in order, before anything still in `queue` (#734).
    deferred: VecDeque<Command>,
    search_generation: SearchGeneration,
    // Shared with background fetch helpers (see `background_fetch`).
    credential_service: Arc<CredentialService<S>>,
    background_memory: background_fetch::Memory,
    pull_request_service: PullRequestService<A>,
    undo_recorder: undo::UndoRecorder,
    operations: Operations,
    // The clean merge this worker staged, so its commit runs `git merge`'s hooks
    // (#124). Memory only: see `merge::CleanMerge` for the fallback (D2-A).
    clean_merge: Option<merge::CleanMerge>,
    // Undo bookkeeping is shared with other processes on this repository (#172).
    state_lock: state_lock::StateLock,
    logged_busy: bool,
    logged_unavailable: bool,
    // After a command waited `LOCK_WAIT` in vain, later ones wait only `CONTENDED_WAIT`
    // until the lock is taken again, and while it keeps being found busy a run of them
    // waits that once, not once each; those that would refuse still wait (`Contention`).
    // A long holder in the other window (a pull, a hook, a pinentry dialog) costs one
    // long wait, not one per click (the queue, reads included, is behind it), while a
    // brief hold is still waited out (#175, #197).
    contention: state_lock::Contention,
    pause: Option<PauseHook>,
}

/// The paused operations a command may run under.
struct Operations {
    // A stopped multi-commit cherry-pick/revert, persisted so it survives a restart.
    sequence_store: SequenceStore,
    // A paused rebase, persisted likewise (`rebase.json`), plus this session's copy when
    // saving it failed. To git a paused rebase is a cherry-pick on a detached HEAD, or
    // nothing at all after an error stop, so only the store says a rebase is going on;
    // a sequence stopped by an error is clean too.
    rebase_store: RebaseStore,
    rebase_memory: gate::RebaseMemory,
}

impl Operations {
    fn probe(&self) -> gate::OperationProbe<'_> {
        gate::OperationProbe {
            rebase: &self.rebase_store,
            memory: &self.rebase_memory,
            sequence: &self.sequence_store,
        }
    }

    /// Names the operation in progress (`None`: nothing is), for the undo recorder.
    fn marker(&self, repo: &git2::Repository) -> Option<String> {
        self.probe().marker(repo)
    }

    fn rebase_slot(&mut self) -> rebase::RebaseSlot<'_> {
        rebase::RebaseSlot {
            store: &self.rebase_store,
            memory: &mut self.rebase_memory,
        }
    }
}

/// What `acquire` got for a command.
enum LockOutcome {
    Held(StateGuard),
    /// The command doesn't lock, or the lock is unavailable: it runs as usual.
    Unlocked,
    /// Another holder keeps the lock: the command runs without recording an entry.
    Unrecorded,
    /// Another holder keeps the lock and the command refuses to run without it.
    Refused(BusyReply),
}

/// The undo bookkeeping `before` started for a command, which `after` ends.
struct UndoTurn {
    track: bool,
    aborts: bool,
    held: undo::HeldReplies,
}

impl<S, A> WorkerLoop<S, A>
where
    S: CredentialStore + Send + Sync + 'static,
    A: ForgeApi + Send + 'static,
{
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        repo_path: PathBuf,
        repo: git2::Repository,
        queue: Receiver<Command>,
        search_generation: SearchGeneration,
        credential_store: S,
        forge_api: A,
        pause: Option<PauseHook>,
    ) -> Self {
        let credential_service = Arc::new(CredentialService::new(credential_store));
        let background_memory = background_fetch::Memory::default();
        let pull_request_service = PullRequestService::new(forge_api);
        let undo_recorder = undo::UndoRecorder::open(&repo);
        let sequence_store = SequenceStore::open(&repo);
        let rebase_store = RebaseStore::open(&repo);
        let state_lock = state_lock::StateLock::open(&repo);
        Self {
            repo_path,
            repo,
            queue,
            deferred: VecDeque::new(),
            search_generation,
            credential_service,
            background_memory,
            pull_request_service,
            undo_recorder,
            operations: Operations {
                sequence_store,
                rebase_store,
                rebase_memory: gate::RebaseMemory::default(),
            },
            clean_merge: None,
            state_lock,
            logged_busy: false,
            logged_unavailable: false,
            contention: state_lock::Contention::default(),
            pause,
        }
    }

    /// Runs the commands sent to this worker, one at a time, until every handle is gone. The
    /// commands a transfer deferred come first.
    pub(super) fn run(mut self) {
        loop {
            let command = match self.deferred.pop_front() {
                Some(command) => command,
                None => match self.queue.recv() {
                    Ok(command) => command,
                    Err(_) => return,
                },
            };
            self.step(command);
        }
    }

    /// What a transfer borrows (see `read::Host`), and the credentials and background fetch
    /// memory it uses.
    fn transfer_parts(
        &mut self,
    ) -> (
        read::Host<'_>,
        &Arc<CredentialService<S>>,
        &background_fetch::Memory,
    ) {
        let host = read::Host {
            repo: &mut self.repo,
            queue: &self.queue,
            deferred: &mut self.deferred,
            state: read::ReadState {
                search_generation: &self.search_generation,
                undo_recorder: &mut self.undo_recorder,
                operations: &self.operations,
                clean_merge: &self.clean_merge,
            },
        };
        (host, &self.credential_service, &self.background_memory)
    }

    /// Runs one command: lock, undo bookkeeping and gate, the command itself, then the undo
    /// entry and the lock's release.
    pub(super) fn step(&mut self, mut command: Command) {
        let policy = policy::of(&command);
        // A command that can record an entry or end a paused operation holds the lock
        // from before `settle` until its `finish`, so no other holder ever sees "the
        // operation ended, but its entry isn't recorded yet". Reads never wait.
        let (guard, unrecorded) = match self.acquire(&policy) {
            LockOutcome::Held(guard) => (Some(guard), false),
            LockOutcome::Unlocked => (None, false),
            LockOutcome::Unrecorded => (None, true),
            LockOutcome::Refused(busy) => {
                undo::reply_busy(&mut command, busy);
                return;
            }
        };
        let (command, turn) = self.before(command, policy, guard.as_ref(), unrecorded);
        self.dispatch(command, guard.as_ref());
        self.after(turn, guard, unrecorded);
    }

    /// Takes the state lock for a command that locks state, logging the first busy and
    /// unavailable lock and tracking contention.
    fn acquire(&mut self, policy: &CommandPolicy) -> LockOutcome {
        if !policy.locks_state {
            return LockOutcome::Unlocked;
        }
        let wait = self
            .contention
            .wait(std::time::Instant::now(), policy.busy.is_some());
        if let Some(pause) = &self.pause {
            pause.record_lock_wait(wait);
        }
        match self.state_lock.acquire_with(wait, || {
            if let Some(pause) = &self.pause {
                pause.pause_here(PausePoint::LockWait);
            }
        }) {
            state_lock::Acquire::Held(guard) => {
                self.contention.held();
                LockOutcome::Held(guard)
            }
            state_lock::Acquire::Busy => {
                self.contention.busy(std::time::Instant::now());
                if let Some(busy) = policy.busy {
                    return LockOutcome::Refused(busy);
                }
                if !self.logged_busy {
                    self.logged_busy = true;
                    eprintln!(
                        "browsitory: undo history is locked by another window; \
                         running the command without recording it for undo"
                    );
                }
                LockOutcome::Unrecorded
            }
            state_lock::Acquire::Unavailable(error) => {
                if !self.logged_unavailable {
                    self.logged_unavailable = true;
                    eprintln!("browsitory: undo history lock unavailable: {error}");
                }
                LockOutcome::Unlocked
            }
        }
    }

    /// Settles and begins the command's undo entry, holds its reply when it must wait for the
    /// entry, and gates it while an operation is paused.
    fn before(
        &mut self,
        mut command: Command,
        policy: CommandPolicy,
        guard: Option<&StateGuard>,
        unrecorded: bool,
    ) -> (Command, UndoTurn) {
        self.undo_recorder
            .settle(&self.repo, guard, || self.operations.marker(&self.repo));
        let plan = policy.undo.filter(|_| !unrecorded);
        let track = plan.is_some() || self.undo_recorder.has_pending();
        let aborts = policy.aborts;
        let mut held = undo::HeldReplies::default();
        if track && policy.holds_reply {
            undo::hold_reply(&mut command, &mut held);
        }
        if let Some(plan) = plan {
            self.undo_recorder.begin(
                &self.repo,
                plan,
                self.operations.marker(&self.repo).as_deref(),
            );
        }
        // While a cherry-pick/revert sequence or a rebase is stored, commands that would
        // move HEAD or rewrite the index under it are refused before git-core runs.
        if let Some(scope) = self.operations.probe().gate_scope(&self.repo) {
            command = gate::gate_during_operation(command, &policy.gate, &scope);
        }
        let turn = UndoTurn {
            track,
            aborts,
            held,
        };
        (command, turn)
    }

    /// Finishes the command's undo entry, then releases the lock and the held reply.
    fn after(&mut self, turn: UndoTurn, guard: Option<StateGuard>, unrecorded: bool) {
        if let Some(pause) = &self.pause {
            pause.pause_here(PausePoint::BeforeFinish);
        }
        if turn.track {
            self.undo_recorder.finish(
                &self.repo,
                guard.as_ref(),
                || self.operations.marker(&self.repo),
                turn.aborts,
            );
        }
        drop(guard);
        turn.held.release();
        if unrecorded {
            self.contention.finished(std::time::Instant::now());
        }
    }
}
