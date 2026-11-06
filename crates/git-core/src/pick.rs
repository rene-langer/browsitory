use git2::{Commit, Oid, Repository, RepositoryState};
use thiserror::Error;

use crate::hooks::HookOptions;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickOutcome {
    Committed {
        commit_id: String,
    },
    Conflicted {
        files: Vec<String>,
    },
    /// `prepare-commit-msg` failed (or emptied the message, #183): nothing was committed, and the
    /// pick stays staged and in progress (git's `CHERRY_PICK_HEAD`/`REVERT_HEAD` and `MERGE_MSG`),
    /// for the commit box to finish or Abort to drop, as after `git cherry-pick`.
    HookFailed {
        hook: String,
        output: String,
    },
}

#[derive(Debug, Error)]
pub enum PickError {
    #[error("{0}")]
    Git(#[from] git2::Error),
    #[error("Finish or abort the current merge/cherry-pick/revert/rebase first.")]
    OperationInProgress,
    #[error("Commit or unstage your staged changes first.")]
    DirtyIndex,
    #[error("This is a merge commit. Choose which parent to apply it against.")]
    MainlineRequired,
    #[error("Parent {mainline} doesn't exist on this commit.")]
    InvalidMainline { mainline: u32 },
    #[error("There is no commit on this branch yet.")]
    UnbornHead,
    #[error("Nothing to apply — those changes are already on this branch.")]
    EmptyResult,
    #[error("{0}")]
    Sign(#[from] crate::signing::SignError),
    #[error("Select at least one commit.")]
    NothingSelected,
    #[error("Resolve all conflicts before continuing.")]
    UnresolvedConflicts,
    #[error("HEAD moved since the cherry-pick paused. Abort it and start again.")]
    HeadMoved,
    #[error("{0}")]
    Merge(#[from] crate::merge::MergeError),
    /// Windows: a cherry-pick or revert stopped at a file another program holds open; what its
    /// checkout had written was put back (#321).
    #[error(transparent)]
    HeldOpen(#[from] crate::HeldOpen),
    #[error("{0}")]
    LinkedFolder(#[from] crate::LinkedFolder),
    #[error(transparent)]
    CaseCollision(#[from] crate::CaseCollision),
    /// `core.ignorecase` can't be read, so the case-collision check can't run (#225).
    #[error(transparent)]
    CaseConfig(#[from] crate::CaseConfigUnreadable),
    /// A hook (or the signing program) was cancelled or couldn't be run.
    #[error("{0}")]
    Hook(#[from] crate::hooks::HookError),
    /// Committing the step Continue finishes failed otherwise (the message file, an empty
    /// message after the hooks).
    #[error("{0}")]
    Commit(crate::commit::CommitError),
    /// A `git cherry-pick A B C` / `git revert A B` started in a terminal is paused between
    /// steps: picks, reverts and Continue would mix with it (#258, D5-A).
    #[error("{}", terminal_sequence_refusal(*.0))]
    TerminalSequence(SequenceKind),
    /// A `git rebase` or `git am` started in a terminal isn't finished (#263).
    #[error(transparent)]
    TerminalRebase(#[from] crate::repo::TerminalRebaseInProgress),
    /// Abort found no terminal sequence (`.git/sequencer/todo`) to abort.
    #[error("No cherry-pick or revert is in progress.")]
    NoTerminalSequence,
    /// Abort of a terminal sequence: it changed since the user confirmed it (the terminal went
    /// on, or the view was out of date). Nothing was changed.
    #[error("{}", terminal_sequence_changed(*.0))]
    TerminalSequenceChanged(SequenceKind),
    /// Abort of a terminal sequence: git's sequencer files can't be read. Nothing was changed.
    #[error("{0}")]
    SequencerDamaged(String),
    /// Abort of a terminal sequence: putting the branch back would overwrite this path's local
    /// change or an untracked file. Nothing was changed.
    #[error(
        "Abort can't put the branch back: '{0}' has changes it would overwrite. Save them \
         elsewhere or discard them, then try again."
    )]
    RewindBlocked(String),
}

fn terminal_sequence_changed(kind: SequenceKind) -> String {
    let noun = match kind {
        SequenceKind::CherryPick => "cherry-pick",
        SequenceKind::Revert => "revert",
    };
    format!("The {noun} in the terminal changed since you looked. Check it again, then abort.")
}

/// Why an operation is refused while a terminal sequence of `kind` is paused (#258).
pub(crate) fn terminal_sequence_refusal(kind: SequenceKind) -> String {
    let noun = match kind {
        SequenceKind::CherryPick => "cherry-pick",
        SequenceKind::Revert => "revert",
    };
    format!(
        "A {noun} started in a terminal isn't finished. Continue it there with git {noun} \
         --continue, or abort it first."
    )
}

/// `Err(kind)` while a terminal sequence is paused between steps. One paused on a step is
/// already refused as an operation in progress.
pub(crate) fn check_no_terminal_sequence(repo: &Repository) -> Result<(), SequenceKind> {
    match terminal_sequence(repo) {
        Some(sequence) if !sequence.paused_on_conflict => Err(sequence.kind),
        _ => Ok(()),
    }
}

impl From<crate::commit::CommitError> for PickError {
    fn from(error: crate::commit::CommitError) -> Self {
        use crate::commit::CommitError;
        match error {
            CommitError::Hook(error) => PickError::Hook(error),
            CommitError::Sign(error) => PickError::Sign(error),
            CommitError::Write(error) => PickError::Git(error),
            other => PickError::Commit(other),
        }
    }
}

#[derive(Clone, Copy)]
enum Kind {
    CherryPick,
    Revert,
}

impl From<SequenceKind> for Kind {
    fn from(kind: SequenceKind) -> Self {
        match kind {
            SequenceKind::CherryPick => Kind::CherryPick,
            SequenceKind::Revert => Kind::Revert,
        }
    }
}

/// Which operation a multi-commit sequence runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum SequenceKind {
    CherryPick,
    Revert,
}

/// Where a stopped sequence is: `current_step` is 1-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceProgress {
    pub kind: SequenceKind,
    pub current_step: usize,
    pub total_steps: usize,
    pub current_commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SequenceOutcome {
    /// Every step ran. `skipped` holds the commits whose changes were already on the branch
    /// (their step came out empty), in application order.
    Completed {
        applied: usize,
        skipped: Vec<String>,
    },
    /// Paused on a conflict in `progress.current_commit` (git state `CherryPick`/`Revert`).
    Conflicted {
        files: Vec<String>,
        progress: SequenceProgress,
    },
    /// A commit hook failed: Continue's (#133), or `prepare-commit-msg` on a step that applied
    /// cleanly (#183). Nothing was committed: the step is paused (git's
    /// `CHERRY_PICK_HEAD`/`REVERT_HEAD` and the staged result are kept) and the returned state
    /// points at it, so Continue (with or without hooks), the commit box and Abort all work.
    HookFailed {
        hook: String,
        output: String,
        progress: SequenceProgress,
    },
    /// Cancel was pressed while a step's hooks ran after its commit (D9-A): that commit (summary
    /// `kept`) stands and the sequence stopped before `progress.current_commit`; Continue goes
    /// on from there.
    Stopped {
        kept: String,
        progress: SequenceProgress,
    },
}

/// How finishing git's paused step went.
enum Paused {
    /// Committed (or skipped as empty); `state.next` moved past it.
    Done,
    /// A commit hook failed; nothing changed.
    HookFailed { hook: String, output: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SequenceAbort {
    /// HEAD went back to where the sequence started. `false` when HEAD had been moved elsewhere
    /// since the sequence stopped, so it was left alone.
    pub rewound: bool,
}

/// A `git cherry-pick A B C` / `git revert A B` started in a terminal that hasn't finished: git's
/// `.git/sequencer/todo` exists (#258). libgit2 reports it only while a step is paused
/// (`CherryPickSequence`/`RevertSequence`); between steps (a step committed by hand, or a stop
/// on an error) its state is `Clean`, so this reads git's files itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalSequence {
    pub kind: SequenceKind,
    /// A step is paused (`CHERRY_PICK_HEAD`/`REVERT_HEAD` exists); otherwise the sequence is
    /// between steps.
    pub paused_on_conflict: bool,
    /// Where the sequence started (`sequencer/head`); empty if missing or unreadable.
    pub started_at: String,
    /// `started_at`'s summary, empty if it can't be read.
    pub started_at_summary: String,
    /// Abort goes back to `started_at`, as `git cherry-pick --abort` does: HEAD is still where
    /// the sequence last left it (`sequencer/abort-safety`) and differs from `started_at`.
    /// Never with `damage`.
    pub rewinds: bool,
    /// The commits that rewind removes (HEAD's commits not in `started_at`); 0 unless
    /// `rewinds`. Counting stops past `COMMITS_TO_REMOVE_CAP`: a larger value means "more".
    pub commits_to_remove: usize,
    /// A sequencer file git's `--abort` can't read either; Abort refuses then.
    pub damage: Option<SequencerDamage>,
}

/// `TerminalSequence::commits_to_remove` counts up to this and one more.
pub const COMMITS_TO_REMOVE_CAP: usize = 1000;

/// What makes git's `cherry-pick --abort` refuse a terminal sequence (git 2.53's
/// `sequencer_rollback` and `rollback_is_safe`), checked in git's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequencerDamage {
    /// `sequencer/head` is missing (git: "no cherry-pick or revert in progress").
    MissingHead,
    /// `sequencer/head` isn't a full commit id (git: "… HEAD file … is corrupt").
    CorruptHead,
    /// `sequencer/abort-safety` exists but isn't a commit id (git: "could not parse").
    CorruptAbortSafety,
    /// The rewind's target (`sequencer/head`) is not in the repository; only when Abort would
    /// rewind (git's `reset --merge` to it fails).
    MissingStart,
}

/// What the user confirmed before Abort of a terminal sequence (what the UI showed). Abort
/// refuses when the sequence no longer matches it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfirmedAbort {
    /// HEAD's full id (empty when unborn).
    pub head_id: String,
    pub started_at: String,
    pub rewinds: bool,
    pub commits_to_remove: usize,
    /// A step was paused on a conflict: the dialog then warned that its changes and
    /// resolutions go.
    pub paused_on_conflict: bool,
}

impl ConfirmedAbort {
    /// The confirmation for the terminal sequence as it is now, if any.
    pub fn now(repo: &Repository) -> Option<Self> {
        terminal_sequence(repo).map(|sequence| Self::of(repo, &sequence))
    }

    fn of(repo: &Repository, sequence: &TerminalSequence) -> Self {
        Self {
            head_id: repo
                .head()
                .ok()
                .and_then(|head| head.target())
                .map(|oid| oid.to_string())
                .unwrap_or_default(),
            started_at: sequence.started_at.clone(),
            rewinds: sequence.rewinds,
            commits_to_remove: sequence.commits_to_remove,
            paused_on_conflict: sequence.paused_on_conflict,
        }
    }
}

/// A full hexadecimal commit id (git's `parse_oid_hex`; libgit2's `Oid::from_str` also takes
/// an abbreviated one).
fn full_oid(text: &str) -> Option<Oid> {
    let full = matches!(text.len(), 40 | 64) && text.bytes().all(|byte| byte.is_ascii_hexdigit());
    full.then(|| Oid::from_str(text).ok()).flatten()
}

/// The terminal sequence in this repository's git folder (per worktree), if any.
pub fn terminal_sequence(repo: &Repository) -> Option<TerminalSequence> {
    let sequencer = repo.path().join("sequencer");
    let todo = std::fs::read(sequencer.join("todo")).ok()?;
    let has = |name: &str| repo.path().join(name).exists();
    // git's `sequencer_get_last_command`: the todo's first command names the operation.
    let first = String::from_utf8_lossy(&todo)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .and_then(|line| line.split_whitespace().next().map(str::to_string));
    let kind = match first.as_deref() {
        Some("pick" | "p") => SequenceKind::CherryPick,
        Some("revert" | "r") => SequenceKind::Revert,
        _ if has("REVERT_HEAD") => SequenceKind::Revert,
        _ => SequenceKind::CherryPick,
    };
    let mut damage = None;
    // git reads `head`'s first line and wants exactly a commit id there.
    let started = match std::fs::read_to_string(sequencer.join("head")) {
        Err(_) => {
            damage = Some(SequencerDamage::MissingHead);
            None
        }
        Ok(text) => {
            let parsed = full_oid(text.split('\n').next().unwrap_or(""));
            if parsed.is_none() {
                damage = Some(SequencerDamage::CorruptHead);
            }
            parsed
        }
    };
    let head = repo.head().ok().and_then(|head| head.target());
    // git's `rollback_is_safe`: a missing abort-safety means "not safe", an unreadable one is
    // an error.
    let abort_safety = match std::fs::read_to_string(sequencer.join("abort-safety")) {
        Err(_) => None,
        Ok(text) => {
            let parsed = text.trim().get(..40).and_then(full_oid);
            if parsed.is_none() && damage.is_none() {
                damage = Some(SequencerDamage::CorruptAbortSafety);
            }
            parsed
        }
    };
    let started_commit = started.and_then(|oid| repo.find_commit(oid).ok());
    let safe = head.is_some() && abort_safety == head;
    if damage.is_none() && safe && started.is_some() && started_commit.is_none() {
        damage = Some(SequencerDamage::MissingStart);
    }
    let rewinds = damage.is_none() && safe && started.is_some_and(|start| Some(start) != head);
    let commits_to_remove = match (started, head) {
        (Some(started), Some(head)) if rewinds => count_commits(repo, head, started),
        _ => 0,
    };
    Some(TerminalSequence {
        kind,
        paused_on_conflict: has("CHERRY_PICK_HEAD") || has("REVERT_HEAD"),
        started_at: started.map(|oid| oid.to_string()).unwrap_or_default(),
        started_at_summary: started_commit
            .as_ref()
            .and_then(|commit| commit.summary().ok().flatten().map(str::to_string))
            .unwrap_or_default(),
        rewinds,
        commits_to_remove,
        damage,
    })
}

/// The commits reachable from `head` but not from `base`, counting up to one past
/// `COMMITS_TO_REMOVE_CAP`.
fn count_commits(repo: &Repository, head: Oid, base: Oid) -> usize {
    let Ok(mut walk) = repo.revwalk() else {
        return 0;
    };
    if walk.push(head).is_err() || walk.hide(base).is_err() {
        return 0;
    }
    walk.take(COMMITS_TO_REMOVE_CAP + 1)
        .filter(Result::is_ok)
        .count()
}

/// A stopped sequence, enough to continue or abort it later. `git-core` never stores it;
/// `repo-service` persists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceState {
    pub kind: SequenceKind,
    /// Application order (oldest-first for cherry-pick, newest-first for revert).
    pub commits: Vec<String>,
    /// Index of the stopped step.
    pub next: usize,
    pub skipped: Vec<String>,
    /// HEAD before the first step; Abort returns here.
    pub original_head: String,
    /// HEAD just before the stopped step ran.
    pub expected_head: String,
    /// The stop was a conflict (git paused the step), not an error. Only then may the user
    /// finish the step outside Browsitory: after an error stop, Continue needs HEAD unmoved.
    pub stopped_on_conflict: bool,
    /// What HEAD was when the sequence started; Abort rewinds only while HEAD is still that.
    pub original_ref: OriginalRef,
}

/// The ref HEAD named when a sequence started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OriginalRef {
    /// A branch, by full ref name (`refs/heads/main`).
    Branch(String),
    Detached,
    /// Not recorded (a state saved by an older version): Abort never rewinds.
    Unknown,
}

impl SequenceState {
    pub fn progress(&self) -> SequenceProgress {
        SequenceProgress {
            kind: self.kind,
            current_step: self.next + 1,
            total_steps: self.commits.len(),
            current_commit: self.commits.get(self.next).cloned().unwrap_or_default(),
        }
    }
}

/// The result of running (part of) a sequence. `state` is `Some` when the sequence stopped and
/// must be kept (a conflict, or an error after something landed), `None` when there is no
/// sequence any more (it completed, or failed before changing anything).
#[derive(Debug)]
pub struct SequenceRun {
    pub outcome: Result<SequenceOutcome, PickError>,
    pub state: Option<SequenceState>,
}

impl SequenceRun {
    fn failed(error: impl Into<PickError>) -> Self {
        Self {
            outcome: Err(error.into()),
            state: None,
        }
    }

    fn stopped(error: impl Into<PickError>, state: SequenceState) -> Self {
        Self {
            outcome: Err(error.into()),
            state: Some(state),
        }
    }
}

/// What one pick/revert step did.
enum StepResult {
    Committed(Oid),
    /// The result equals HEAD's tree; the operation state was cleaned up, nothing committed.
    Empty,
    Conflicted(Vec<String>),
    /// `prepare-commit-msg` failed or emptied the message; the step stays staged and paused.
    HookFailed {
        hook: String,
        output: String,
    },
}

/// Applies `commit_id` on top of HEAD. A clean result is committed immediately with the original
/// message and author; a conflict leaves the repo in `RepositoryState::CherryPick` with
/// `MERGE_MSG` written, for the existing conflict pane + `commit::commit` / `merge::abort_merge`
/// to finish. A merge commit needs `mainline` (1-based parent number, like `git cherry-pick -m`);
/// any other commit must not get one.
///
/// Hooks (#183), as `git cherry-pick` runs them for a commit it creates: `prepare-commit-msg
/// <COMMIT_EDITMSG> message` (it may rewrite the message), the commit (signing reported and
/// cancellable through `hooks`), then `post-commit`. No `pre-commit`/`commit-msg`. A failing
/// `prepare-commit-msg` is `PickOutcome::HookFailed` with the pick left staged; a cancelled one
/// is `PickError::Hook`, likewise.
pub fn cherry_pick(
    repo: &Repository,
    commit_id: &str,
    mainline: Option<u32>,
    hooks: HookOptions<'_>,
) -> Result<PickOutcome, PickError> {
    apply(repo, commit_id, Kind::CherryPick, mainline, hooks)
}

/// Commits the inverse of `commit_id` on top of HEAD. Same conflict, hook and `mainline`
/// contract as `cherry_pick` (state `RepositoryState::Revert`).
pub fn revert(
    repo: &Repository,
    commit_id: &str,
    mainline: Option<u32>,
    hooks: HookOptions<'_>,
) -> Result<PickOutcome, PickError> {
    apply(repo, commit_id, Kind::Revert, mainline, hooks)
}

fn apply(
    repo: &Repository,
    commit_id: &str,
    kind: Kind,
    mainline: Option<u32>,
    mut hooks: HookOptions<'_>,
) -> Result<PickOutcome, PickError> {
    let head = check_ready(repo)?;
    let target = repo.find_commit(Oid::from_str(commit_id)?)?;
    check_mainline(&target, mainline)?;
    check_signing(repo)?;
    match step(repo, &head, &target, kind, mainline, &mut hooks)? {
        StepResult::Committed(oid) => Ok(PickOutcome::Committed {
            commit_id: oid.to_string(),
        }),
        StepResult::Empty => Err(PickError::EmptyResult),
        StepResult::Conflicted(files) => Ok(PickOutcome::Conflicted { files }),
        StepResult::HookFailed { hook, output } => Ok(PickOutcome::HookFailed { hook, output }),
    }
}

/// Clears a pick's own state as `git commit` does (`commit::clear_commit_state`): a terminal
/// `git cherry-pick A B C`'s remaining todo in `.git/sequencer` is kept (#241).
fn clear_state(repo: &Repository) -> Result<(), PickError> {
    crate::commit::clear_commit_state(repo)
        .map_err(|error| PickError::Commit(crate::commit::CommitError::Io(error)))
}

/// The preconditions every pick shares, in order: nothing paused, a born HEAD, nothing staged.
/// Returns HEAD's commit.
fn check_ready(repo: &Repository) -> Result<Commit<'_>, PickError> {
    crate::repo::check_no_terminal_rebase(repo)?;
    if !crate::repo::no_operation(repo) {
        return Err(PickError::OperationInProgress);
    }
    check_no_terminal_sequence(repo).map_err(PickError::TerminalSequence)?;
    let head = match repo.head() {
        Ok(head) => head.peel_to_commit()?,
        Err(error) if error.code() == git2::ErrorCode::UnbornBranch => {
            return Err(PickError::UnbornHead)
        }
        Err(error) => return Err(error.into()),
    };
    let index = fresh_index(repo)?;
    if index.has_conflicts() {
        return Err(PickError::UnresolvedConflicts);
    }
    let staged = repo.diff_tree_to_index(Some(&head.tree()?), Some(&index), None)?;
    if staged.deltas().len() > 0 {
        return Err(PickError::DirtyIndex);
    }
    Ok(head)
}

/// Validate the signing configuration before libgit2 stages anything, so a config error
/// (unsupported gpg.format, missing SSH key) leaves the repository completely untouched.
/// A signer *program* failure can only surface at commit time, after the pick is staged;
/// that leaves the pick in progress (CHERRY_PICK_HEAD/REVERT_HEAD, like git) so the user can
/// commit again or abort. HEAD and refs never move on failure.
fn check_signing(repo: &Repository) -> Result<(), PickError> {
    crate::signing::signing_config(repo)?;
    Ok(())
}

fn check_mainline(target: &Commit<'_>, mainline: Option<u32>) -> Result<(), PickError> {
    let parents = target.parent_count();
    match mainline {
        None if parents > 1 => Err(PickError::MainlineRequired),
        Some(m) if parents <= 1 || m == 0 || m as usize > parents => {
            Err(PickError::InvalidMainline { mainline: m })
        }
        _ => Ok(()),
    }
}

/// Applies `target` on top of `head` (HEAD's commit) and commits a clean, non-empty result
/// through `prepare-commit-msg`, the commit and `post-commit` (see `cherry_pick`). Callers have
/// run `check_ready`, `check_mainline` and `check_signing`.
fn step(
    repo: &Repository,
    head: &Commit<'_>,
    target: &Commit<'_>,
    kind: Kind,
    mainline: Option<u32>,
    hooks: &mut HookOptions<'_>,
) -> Result<StepResult, PickError> {
    // libgit2's default checkout strategy here is SAFE: it errors rather than overwrite a
    // modified working-tree file the pick needs to change. libgit2 may already have written
    // CHERRY_PICK_HEAD/REVERT_HEAD + MERGE_MSG by then; state was Clean on entry (checked by
    // the caller), so clearing those files restores exactly the prior state without touching
    // user files.
    // Safe checkouts create files through a symlinked folder (#114).
    // The step writes its merged result; scan that (an untracked link counts too).
    let merged = match kind {
        Kind::CherryPick => repo.cherrypick_commit(target, head, mainline.unwrap_or(0), None)?,
        Kind::Revert => repo.revert_commit(target, head, mainline.unwrap_or(0), None)?,
    };
    crate::worktree_guard::refuse_linked_folders::<PickError>(
        repo,
        &[crate::worktree_guard::Incoming::Merge(
            crate::worktree_guard::GuardSource::Index(&merged),
        )],
    )?;
    // A checkout that stops part-way (a file another program holds open, a read-only file) is
    // rolled back, so the step can be tried again (#321). libgit2's own default strategy for a
    // pick is safe with conflicts allowed; a builder of our own must say so.
    let written = crate::partial_checkout::Written::default();
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.safe().allow_conflicts(true);
    written.record(&mut checkout);
    let applied = match kind {
        Kind::CherryPick => {
            let mut options = git2::CherrypickOptions::new();
            if let Some(mainline) = mainline {
                options.mainline(mainline);
            }
            options.checkout_builder(checkout);
            repo.cherrypick(target, Some(&mut options))
        }
        Kind::Revert => {
            let mut options = git2::RevertOptions::new();
            if let Some(mainline) = mainline {
                options.mainline(mainline);
            }
            options.checkout_builder(checkout);
            repo.revert(target, Some(&mut options))
        }
    };
    if let Err(error) = applied {
        let restored = crate::partial_checkout::roll_back(repo, &written);
        let _ = crate::commit::clear_commit_state(repo);
        return Err(crate::partial_checkout::report(
            repo,
            restored,
            crate::HeldDuring::RolledBack,
            error,
        ));
    }

    let mut index = repo.index()?;
    if index.has_conflicts() {
        let mut files: Vec<String> = index
            .conflicts()?
            .filter_map(|conflict| conflict.ok())
            .filter_map(|conflict| conflict.our.or(conflict.their).or(conflict.ancestor))
            .map(|entry| String::from_utf8_lossy(&entry.path).into_owned())
            .collect();
        files.sort();
        files.dedup();
        return Ok(StepResult::Conflicted(files));
    }

    let tree_id = index.write_tree()?;
    if tree_id == head.tree_id() {
        clear_state(repo)?;
        return Ok(StepResult::Empty);
    }
    let tree = repo.find_tree(tree_id)?;
    let committer = repo.signature()?;
    let (author, message) = match kind {
        Kind::CherryPick => (
            target.author().to_owned(),
            String::from_utf8_lossy(target.message_bytes()).into_owned(),
        ),
        Kind::Revert => (committer.clone(), revert_message(target, mainline)),
    };
    // git's sequencer runs only `prepare-commit-msg` before a pick's commit. On failure or
    // cancel, git's pick state (and the staged result) stays, as after `git cherry-pick`.
    let message = match crate::commit::run_message_hook(repo, &message, "message", hooks) {
        Ok(message) => message,
        Err(crate::commit::CommitError::HookFailed { hook, output }) => {
            return Ok(StepResult::HookFailed { hook, output })
        }
        Err(error @ crate::commit::CommitError::EmptyMessage) => {
            return Ok(StepResult::HookFailed {
                hook: "prepare-commit-msg".to_string(),
                output: error.to_string(),
            })
        }
        Err(error) => return Err(error.into()),
    };
    let oid = crate::commit::create_commit_reporting(
        repo,
        hooks,
        crate::signing::HeadUpdate {
            expected: Some(head.id()),
            reflog_operation: "commit",
        },
        &author,
        &committer,
        &message,
        &tree,
        &[head],
    )?;
    // As git's sequencer (`try_to_commit`): `post-commit` runs while CHERRY_PICK_HEAD and
    // MERGE_MSG are still there. The commit landed, so a state that can't be cleared is logged,
    // not an error (D7-A): the step counts and the sequence moves on.
    crate::commit::run_post_commit(repo, hooks);
    crate::commit::cleanup_after_commit(repo);
    Ok(StepResult::Committed(oid))
}

/// git's revert message; a mainline revert names the parent it keeps, like `git revert -m`.
fn revert_message(target: &Commit<'_>, mainline: Option<u32>) -> String {
    let reversing = mainline
        .and_then(|m| target.parent_id(m as usize - 1).ok())
        .map(|parent| format!(", reversing\nchanges made to {parent}"))
        .unwrap_or_default();
    let summary = target.summary().ok().flatten().unwrap_or("");
    // git (2.43+): reverting `Revert "x"` says `Reapply "x"`, but never for `Revert "Revert ...`.
    let subject = match summary.strip_prefix("Revert \"") {
        Some(rest) if !rest.starts_with("Revert \"") => format!("Reapply \"{rest}"),
        _ => format!("Revert \"{summary}\""),
    };
    format!(
        "{subject}\n\nThis reverts commit {}{reversing}.\n",
        target.id()
    )
}

/// Cherry-picks `ids_oldest_first` in that order as one sequence. Each clean step is committed
/// with the original author and message, through `prepare-commit-msg … message` and
/// `post-commit` as `git cherry-pick A B C` runs them (#183, see `cherry_pick`); a step whose
/// result is empty is skipped. A conflict, or a failing `prepare-commit-msg`
/// (`SequenceOutcome::HookFailed`), pauses the sequence on that step (git state `CherryPick`,
/// like a single pick) and returns its state for `continue_sequence`/`abort_sequence`. A cancel
/// during a step's `post-commit` keeps that commit and stops the sequence after it
/// (`SequenceOutcome::Stopped`, state kept; Continue goes on with the next step). Merge commits are
/// refused up front.
pub fn cherry_pick_many(
    repo: &Repository,
    ids_oldest_first: &[String],
    hooks: HookOptions<'_>,
) -> SequenceRun {
    start(repo, ids_oldest_first, SequenceKind::CherryPick, hooks)
}

/// Reverts `ids_oldest_first` newest-first (like `git revert A..B`), otherwise as
/// `cherry_pick_many`.
pub fn revert_many(
    repo: &Repository,
    ids_oldest_first: &[String],
    hooks: HookOptions<'_>,
) -> SequenceRun {
    start(repo, ids_oldest_first, SequenceKind::Revert, hooks)
}

fn start(
    repo: &Repository,
    ids: &[String],
    kind: SequenceKind,
    mut hooks: HookOptions<'_>,
) -> SequenceRun {
    if ids.is_empty() {
        return SequenceRun::failed(PickError::NothingSelected);
    }
    let head = match check_ready(repo) {
        Ok(head) => head,
        Err(error) => return SequenceRun::failed(error),
    };
    for id in ids {
        let target = match Oid::from_str(id).and_then(|oid| repo.find_commit(oid)) {
            Ok(target) => target,
            Err(error) => return SequenceRun::failed(error),
        };
        if target.parent_count() > 1 {
            return SequenceRun::failed(PickError::MainlineRequired);
        }
    }
    if let Err(error) = check_signing(repo) {
        return SequenceRun::failed(error);
    }
    // A commit selected twice is applied once (its first occurrence), so no two steps share an
    // id and git's pick state always names exactly one step.
    let mut seen = std::collections::HashSet::new();
    let mut commits: Vec<String> = ids
        .iter()
        .filter(|id| seen.insert(id.as_str()))
        .cloned()
        .collect();
    if kind == SequenceKind::Revert {
        commits.reverse();
    }
    let original_ref = match head_ref(repo) {
        Ok(original_ref) => original_ref,
        Err(error) => return SequenceRun::failed(error),
    };
    let head_id = head.id().to_string();
    run(
        repo,
        SequenceState {
            kind,
            commits,
            next: 0,
            skipped: Vec::new(),
            original_head: head_id.clone(),
            expected_head: head_id,
            stopped_on_conflict: false,
            original_ref,
        },
        &mut hooks,
    )
}

fn head_ref(repo: &Repository) -> Result<OriginalRef, git2::Error> {
    if repo.head_detached()? {
        return Ok(OriginalRef::Detached);
    }
    let head = repo.head()?;
    Ok(head
        .name()
        .map(|name| OriginalRef::Branch(name.to_string()))
        .unwrap_or(OriginalRef::Unknown))
}

/// The repository's index, re-read from disk if another process (a terminal `git add`) changed
/// it since libgit2 loaded it.
fn fresh_index(repo: &Repository) -> Result<git2::Index, git2::Error> {
    let mut index = repo.index()?;
    index.read(false)?;
    Ok(index)
}

/// True once the user pressed Cancel on this run's operation.
fn cancelled(hooks: &HookOptions<'_>) -> bool {
    hooks.cancel.is_some_and(|cancel| cancel())
}

/// Runs the steps from `state.next` on. HEAD is the base of each step (the caller made sure it
/// is `state.expected_head` or one commit further for a step the user finished).
fn run(repo: &Repository, mut state: SequenceState, hooks: &mut HookOptions<'_>) -> SequenceRun {
    while state.next < state.commits.len() {
        // A Cancel during the previous step's `post-commit` (or the step Continue committed)
        // keeps that commit and stops before the next step (D9-A); Continue resumes here.
        // Not an error: the last step's commit stands (`SequenceOutcome::Stopped`).
        if cancelled(hooks) {
            let kept = match repo.head().and_then(|head| head.peel_to_commit()) {
                Ok(head) => {
                    state.expected_head = head.id().to_string();
                    head.summary().ok().flatten().unwrap_or("").to_string()
                }
                Err(error) => return SequenceRun::stopped(error, state),
            };
            state.stopped_on_conflict = false;
            return SequenceRun {
                outcome: Ok(SequenceOutcome::Stopped {
                    kept,
                    progress: state.progress(),
                }),
                state: Some(state),
            };
        }
        let result = (|| {
            let head = repo.head()?.peel_to_commit()?;
            state.expected_head = head.id().to_string();
            let target = repo.find_commit(Oid::from_str(&state.commits[state.next])?)?;
            step(repo, &head, &target, state.kind.into(), None, hooks)
        })();
        match result {
            Ok(StepResult::Committed(_)) => state.next += 1,
            Ok(StepResult::Empty) => {
                state.skipped.push(state.commits[state.next].clone());
                state.next += 1;
            }
            Ok(StepResult::Conflicted(files)) => {
                state.stopped_on_conflict = true;
                return SequenceRun {
                    outcome: Ok(SequenceOutcome::Conflicted {
                        files,
                        progress: state.progress(),
                    }),
                    state: Some(state),
                };
            }
            Ok(StepResult::HookFailed { hook, output }) => {
                // Paused like a conflict: git's pick state and the staged step stay.
                state.stopped_on_conflict = true;
                return SequenceRun {
                    outcome: Ok(SequenceOutcome::HookFailed {
                        hook,
                        output,
                        progress: state.progress(),
                    }),
                    state: Some(state),
                };
            }
            Err(error) => {
                // A cancelled `prepare-commit-msg` leaves the step paused, as a failing one does.
                state.stopped_on_conflict =
                    matches!(error, PickError::Hook(_)) && !crate::repo::no_operation(repo);
                let untouched = state.next == 0
                    && state.skipped.is_empty()
                    && state.expected_head == state.original_head
                    && crate::repo::no_operation(repo);
                return if untouched {
                    SequenceRun::failed(error)
                } else {
                    SequenceRun::stopped(error, state)
                };
            }
        }
    }
    SequenceRun {
        outcome: Ok(SequenceOutcome::Completed {
            applied: state.commits.len() - state.skipped.len(),
            skipped: state.skipped,
        }),
        state: None,
    }
}

/// Goes on with a stopped sequence. When it stopped on a conflict, the resolved step is
/// committed first (author and `MERGE_MSG` like `git cherry-pick --continue`, or skipped when
/// the resolution left nothing to commit); when it stopped on an error, that step is retried.
/// A step the user committed some other way (HEAD is one commit past the stop) counts as done.
/// Anything else that moved HEAD is `HeadMoved`, and an unrelated paused operation is
/// `OperationInProgress`; both keep the state so the sequence can still be aborted.
///
/// Hooks (#133, #183): the step Continue commits (the resolved, paused one) runs
/// `git cherry-pick --continue`'s commit hooks through `hooks`: `pre-commit`,
/// `prepare-commit-msg <file> merge`, `commit-msg`, then the commit (signing reported and
/// cancellable as for a commit), then `post-commit`. A failing hook is
/// `SequenceOutcome::HookFailed` and a cancelled one `PickError::Hook`, both with the state
/// unchanged. An empty resolution is skipped without hooks. The later steps Continue applies by
/// itself run `prepare-commit-msg … message` and `post-commit` like `cherry_pick_many`'s.
pub fn continue_sequence(
    repo: &Repository,
    mut state: SequenceState,
    mut hooks: crate::hooks::HookOptions<'_>,
) -> SequenceRun {
    if state.next >= state.commits.len() {
        return SequenceRun::stopped(
            git2::Error::from_str("the stored sequence has no step left"),
            state,
        );
    }
    // A terminal rebase or `git am` is git's own operation in progress (#263).
    if let Err(error) = crate::repo::check_no_terminal_rebase(repo) {
        return SequenceRun::stopped(error, state);
    }
    // A terminal sequence paused between steps would mix with this one (#258, D5-A).
    if let Err(kind) = check_no_terminal_sequence(repo) {
        return SequenceRun::stopped(PickError::TerminalSequence(kind), state);
    }
    if let Err(error) = check_signing(repo) {
        return SequenceRun::stopped(error, state);
    }
    // The previous step's commit landed but its pick state could not be cleared (D7-A): clear
    // it now, so Continue goes on with the next step instead of refusing.
    if is_leftover_of_committed_step(repo, &state) {
        if let Err(error) = clear_state(repo) {
            return SequenceRun::stopped(error, state);
        }
    }
    let resumed = match repo.state() {
        RepositoryState::CherryPick
        | RepositoryState::CherryPickSequence
        | RepositoryState::Revert
        | RepositoryState::RevertSequence => finish_paused(repo, &mut state, &mut hooks),
        // A terminal bisect is no operation in progress (#250).
        RepositoryState::Clean | RepositoryState::Bisect => {
            resume_clean(repo, &mut state).map(|()| Paused::Done)
        }
        _ => Err(PickError::OperationInProgress),
    };
    match resumed {
        Ok(Paused::Done) => run(repo, state, &mut hooks),
        Ok(Paused::HookFailed { hook, output }) => SequenceRun {
            outcome: Ok(SequenceOutcome::HookFailed {
                hook,
                output,
                progress: state.progress(),
            }),
            state: Some(state),
        },
        Err(error) => SequenceRun::stopped(error, state),
    }
}

/// git's pick state names the step before the current one, and HEAD is where that step's commit
/// left it: the commit landed and only its state was left behind (D7-A).
fn is_leftover_of_committed_step(repo: &Repository, state: &SequenceState) -> bool {
    // A conflict stop's pick state is the paused step, never a leftover (a commit id listed
    // twice is deduplicated by `start`, so the current and the previous step never share it).
    if state.stopped_on_conflict {
        return false;
    }
    let Some(previous) = state.next.checked_sub(1).and_then(|i| state.commits.get(i)) else {
        return false;
    };
    let head_file = match repo.state() {
        RepositoryState::CherryPick | RepositoryState::CherryPickSequence => "CHERRY_PICK_HEAD",
        RepositoryState::Revert | RepositoryState::RevertSequence => "REVERT_HEAD",
        _ => return false,
    };
    let names_previous = repo
        .find_reference(head_file)
        .ok()
        .and_then(|reference| reference.target())
        .is_some_and(|oid| oid.to_string() == *previous);
    let head_unmoved = repo
        .head()
        .ok()
        .and_then(|head| head.target())
        .is_some_and(|oid| oid.to_string() == state.expected_head);
    names_previous && head_unmoved
}

/// True when git's paused pick/revert is this sequence's current step (not one started some
/// other way, e.g. from a terminal).
fn is_own_paused_step(repo: &Repository, state: &SequenceState) -> bool {
    let (expected_state, head_file) = match state.kind {
        SequenceKind::CherryPick => (
            [
                RepositoryState::CherryPick,
                RepositoryState::CherryPickSequence,
            ],
            "CHERRY_PICK_HEAD",
        ),
        SequenceKind::Revert => (
            [RepositoryState::Revert, RepositoryState::RevertSequence],
            "REVERT_HEAD",
        ),
    };
    expected_state.contains(&repo.state())
        && repo
            .find_reference(head_file)
            .ok()
            .and_then(|reference| reference.target())
            .map(|oid| oid.to_string())
            == state.commits.get(state.next).cloned()
}

fn finish_paused(
    repo: &Repository,
    state: &mut SequenceState,
    hooks: &mut crate::hooks::HookOptions<'_>,
) -> Result<Paused, PickError> {
    if !is_own_paused_step(repo, state) {
        return Err(PickError::OperationInProgress);
    }
    let mut index = fresh_index(repo)?;
    if index.has_conflicts() {
        return Err(PickError::UnresolvedConflicts);
    }
    let head = repo.head()?.peel_to_commit()?;
    if head.id().to_string() != state.expected_head {
        return Err(PickError::HeadMoved);
    }
    let current = state.commits[state.next].clone();
    let tree_id = index.write_tree()?;
    // Before any hook: an empty resolution is skipped, as git's `--continue` would refuse it.
    if tree_id == head.tree_id() {
        clear_state(repo)?;
        state.skipped.push(current);
        state.next += 1;
        return Ok(Paused::Done);
    }
    let target = repo.find_commit(Oid::from_str(&current)?)?;
    let committer = repo.signature()?;
    // libgit2's MERGE_MSG lists the conflicts as `#` comment lines; strip them like git's
    // default commit cleanup does.
    let message = repo
        .message()
        .ok()
        .and_then(|message| git2::message_prettify(message, Some(b'#')).ok())
        .filter(|message| !message.trim().is_empty());
    let (author, message) = match state.kind {
        SequenceKind::CherryPick => (
            target.author().to_owned(),
            message.unwrap_or_else(|| String::from_utf8_lossy(target.message_bytes()).into_owned()),
        ),
        SequenceKind::Revert => (
            committer.clone(),
            message.unwrap_or_else(|| revert_message(&target, None)),
        ),
    };
    // `git cherry-pick --continue`'s hooks; a failure leaves everything as it was.
    let hooked = match crate::commit::run_commit_hooks(
        repo,
        &message,
        "pre-commit",
        "merge",
        crate::commit::MessageFile::CommitEditmsg,
        hooks,
    ) {
        Ok(hooked) => hooked,
        Err(crate::commit::CommitError::HookFailed { hook, output }) => {
            return Ok(Paused::HookFailed { hook, output })
        }
        Err(error) => return Err(error.into()),
    };
    if hooked.reload_index {
        index.read(true)?;
        // A `pre-commit` that put the step back to HEAD's tree emptied it: skip it like an empty
        // resolution rather than commit nothing.
        if index.write_tree()? == head.tree_id() {
            clear_state(repo)?;
            state.skipped.push(current);
            state.next += 1;
            return Ok(Paused::Done);
        }
    }
    let tree = repo.find_tree(index.write_tree()?)?;
    crate::commit::create_commit_reporting(
        repo,
        hooks,
        crate::signing::HeadUpdate {
            expected: Some(head.id()),
            reflog_operation: "commit",
        },
        &author,
        &committer,
        &hooked.message,
        &tree,
        &[&head],
    )?;
    // The step landed: a leftover pick state is logged, not an error (#183).
    crate::commit::cleanup_after_commit(repo);
    crate::commit::run_post_commit(repo, hooks);
    state.next += 1;
    Ok(Paused::Done)
}

fn resume_clean(repo: &Repository, state: &mut SequenceState) -> Result<(), PickError> {
    let head = check_ready(repo)?;
    if head.id().to_string() == state.expected_head {
        // Stopped on an error (or the paused step was aborted outside): retry the step.
        return Ok(());
    }
    if state.stopped_on_conflict && is_step_committed_by_hand(repo, &head, state)? {
        state.next += 1;
        return Ok(());
    }
    Err(PickError::HeadMoved)
}

/// After a conflict stop, the user may finish the step outside Browsitory (`git commit` in a
/// terminal). HEAD counts as that step only when it is exactly one commit past the stop, git's
/// paused state is gone, and it is recognisably the step: a cherry-pick keeps the picked
/// commit's author (name, email and time); a revert's message names the reverted commit.
fn is_step_committed_by_hand(
    repo: &Repository,
    head: &Commit<'_>,
    state: &SequenceState,
) -> Result<bool, PickError> {
    if head.parent_count() != 1 || head.parent_id(0)?.to_string() != state.expected_head {
        return Ok(false);
    }
    let pick_head_gone = ["CHERRY_PICK_HEAD", "REVERT_HEAD"]
        .iter()
        .all(|name| repo.find_reference(name).is_err());
    if !pick_head_gone {
        return Ok(false);
    }
    let target = repo.find_commit(Oid::from_str(&state.commits[state.next])?)?;
    Ok(match state.kind {
        SequenceKind::CherryPick => commit_matches_pick(
            repo,
            head,
            Oid::from_str(&state.expected_head)?,
            &target,
            &[&String::from_utf8_lossy(target.message_bytes())],
        )?,
        SequenceKind::Revert => String::from_utf8_lossy(head.message_bytes())
            .contains(&format!("This reverts commit {}", target.id())),
    })
}

/// True when `head` is recognisably `picked` committed by hand on top of `expected_parent`
/// (`git commit` or `git cherry-pick --continue` during the paused pick). Shared by sequences
/// and rebases. All of:
///
/// - exactly one parent, `expected_parent`;
/// - the picked commit's author (name, email and time), which git keeps during a cherry-pick;
/// - one of `messages` (comment lines and trailing whitespace ignored, as git's cleanup does);
/// - every path that applying `picked` onto the parent changes cleanly is changed by `head`
///   too. Conflicted paths may be resolved any way (including to HEAD's side), and more may
///   change (an edit stop's amendment), but a commit that leaves out part of the step is not it.
pub(crate) fn commit_matches_pick(
    repo: &Repository,
    head: &Commit<'_>,
    expected_parent: Oid,
    picked: &Commit<'_>,
    messages: &[&str],
) -> Result<bool, git2::Error> {
    let message = clean_message(head.message_bytes());
    let message_matches = messages
        .iter()
        .any(|expected| clean_message(expected.as_bytes()) == message);
    Ok(message_matches
        && commit_carries_pick(repo, head, expected_parent, picked, Coverage::Changed)?.is_some())
}

/// How `commit_carries_pick` compares a path the pick changes cleanly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Coverage {
    /// `head` changes the path too, to anything: a step committed by hand may have been amended
    /// (an edit stop).
    Changed,
    /// `head` has exactly the pick's result there (blob and mode): a commit made by the pick
    /// itself, whose state was left behind (#226).
    Exact,
}

/// `commit_matches_pick` without the message: `head` has the single parent `expected_parent`,
/// the picked commit's author (name, email and time) and every change that applying `picked`
/// onto that parent makes cleanly, compared as `coverage` says. `Some(n)` when it does, `n`
/// being the number of those clean changes (0 when the pick changes nothing cleanly, so only
/// the author matched). Conflicted paths are not compared.
pub(crate) fn commit_carries_pick(
    repo: &Repository,
    head: &Commit<'_>,
    expected_parent: Oid,
    picked: &Commit<'_>,
    coverage: Coverage,
) -> Result<Option<usize>, git2::Error> {
    if head.parent_count() != 1 || head.parent_id(0)? != expected_parent {
        return Ok(None);
    }
    let (ours, theirs) = (head.author(), picked.author());
    let same_author = ours.name_bytes() == theirs.name_bytes()
        && ours.email_bytes() == theirs.email_bytes()
        && ours.when() == theirs.when();
    if !same_author {
        return Ok(None);
    }
    let parent = repo.find_commit(expected_parent)?;
    let applied = repo.cherrypick_commit(picked, &parent, 0, None)?;
    carries_applied(repo, head, &parent, &applied, coverage)
}

/// The revert counterpart of `commit_carries_pick` with `Coverage::Exact`: `head` has a single
/// parent and exactly the result of reverting `reverted` onto that parent on every path the
/// revert changes cleanly. The author isn't compared (a revert is authored by whoever made it).
pub(crate) fn commit_carries_revert(
    repo: &Repository,
    head: &Commit<'_>,
    reverted: &Commit<'_>,
) -> Result<Option<usize>, git2::Error> {
    if head.parent_count() != 1 {
        return Ok(None);
    }
    let parent = head.parent(0)?;
    let applied = repo.revert_commit(reverted, &parent, 0, None)?;
    carries_applied(repo, head, &parent, &applied, Coverage::Exact)
}

/// Whether `head` carries `applied` (an in-memory pick or revert onto `parent`) on every path it
/// changes cleanly, compared as `coverage` says; `Some(n)` with the number of those paths.
/// Conflicted paths are not compared.
fn carries_applied(
    repo: &Repository,
    head: &Commit<'_>,
    parent: &Commit<'_>,
    applied: &git2::Index,
    coverage: Coverage,
) -> Result<Option<usize>, git2::Error> {
    let mut conflicted = std::collections::HashSet::new();
    for conflict in applied.conflicts()? {
        let conflict = conflict?;
        for entry in [conflict.ancestor, conflict.our, conflict.their]
            .into_iter()
            .flatten()
        {
            conflicted.insert(entry.path);
        }
    }
    let (parent_tree, head_tree) = (parent.tree()?, head.tree()?);
    let diff = repo.diff_tree_to_index(Some(&parent_tree), Some(applied), None)?;
    let mut carried = 0;
    for delta in diff.deltas() {
        if delta.status() == git2::Delta::Conflicted {
            continue;
        }
        for file in [delta.old_file(), delta.new_file()] {
            let Some(path) = file.path() else { continue };
            let bytes = file.path_bytes().unwrap_or_default();
            if conflicted.contains(bytes) {
                continue;
            }
            let entry = |tree: &git2::Tree<'_>| {
                tree.get_path(path)
                    .ok()
                    .map(|entry| (entry.id(), entry.filemode() as u32))
            };
            let carries = match coverage {
                Coverage::Changed => entry(&parent_tree) != entry(&head_tree),
                Coverage::Exact => {
                    let result = applied
                        .get_path(path, 0)
                        .map(|entry| (entry.id, entry.mode));
                    entry(&head_tree) == result
                }
            };
            if !carries {
                return Ok(None);
            }
            carried += 1;
        }
    }
    Ok(Some(carried))
}

/// A commit message as git's default cleanup leaves it: `#` comment lines dropped, trailing
/// whitespace trimmed.
fn clean_message(message: &[u8]) -> String {
    let text = String::from_utf8_lossy(message);
    git2::message_prettify(text.as_ref(), Some(b'#'))
        .unwrap_or_else(|_| text.into_owned())
        .trim_end()
        .to_string()
}

/// Ends a stopped sequence. A paused step of this sequence is aborted like `git cherry-pick
/// --abort` (unrelated edits survive). If HEAD is still where the sequence stopped, the steps
/// that landed are undone by checking out the original HEAD (libgit2's SAFE strategy: a local
/// edit in the way is an error, and Abort can be retried) and moving the branch (or detached
/// HEAD) back. A rename of the sequence's branch is followed, as git's `--abort` does
/// ([`renamed_to`], #293). When HEAD was moved elsewhere, or another operation is paused,
/// nothing is moved (`rewound: false`). The caller forgets the state only on `Ok`.
pub fn abort_sequence(
    repo: &Repository,
    state: &SequenceState,
) -> Result<SequenceAbort, PickError> {
    let head_id = repo.head()?.peel_to_commit()?.id().to_string();
    if is_own_paused_step(repo, state) {
        crate::merge::abort_merge(repo)?;
    } else if is_leftover_of_committed_step(repo, state) {
        // The previous step's commit landed but its pick state stayed (D7-A): ours, and nothing
        // is staged in it (the worktree and index are that commit), so clear it and rewind.
        // Only the pick's own files: a terminal bisect or cherry-pick list goes on (#249).
        crate::commit::clear_operation_files(repo)
            .map_err(|error| PickError::Commit(crate::commit::CommitError::Io(error)))?;
    } else if crate::repo::no_operation(repo)
        && head_id == state.expected_head
        && fresh_index(repo)?.has_conflicts()
    {
        // The step's conflict without its CHERRY_PICK_HEAD/REVERT_HEAD (deleted outside):
        // still ours to clean up, like `git cherry-pick --abort` would have.
        crate::merge::abort_merge(repo)?;
    }
    if !crate::repo::no_operation(repo) {
        // Someone else's operation is paused here; don't pull HEAD from under it. A terminal
        // bisect isn't one: git's `cherry-pick --abort` rewinds during a bisect too (#250).
        return Ok(SequenceAbort {
            rewound: head_id == state.original_head,
        });
    }
    let current_ref = head_ref(repo)?;
    let same_ref = (current_ref == state.original_ref
        && state.original_ref != OriginalRef::Unknown)
        || (head_id == state.expected_head && renamed_to(repo, state, &current_ref));
    if !same_ref {
        // HEAD now names another branch (or detached/attached changed): rewinding would move
        // a ref the sequence never touched.
        return Ok(SequenceAbort {
            rewound: head_id == state.original_head,
        });
    }
    if head_id == state.expected_head && state.expected_head != state.original_head {
        let original = repo.find_commit(Oid::from_str(&state.original_head)?)?;
        let reflog = match state.kind {
            SequenceKind::CherryPick => "cherry-pick: abort",
            SequenceKind::Revert => "revert: abort",
        };
        rewind_to(repo, &original, reflog)?;
        return Ok(SequenceAbort { rewound: true });
    }
    Ok(SequenceAbort {
        rewound: head_id == state.original_head,
    })
}

/// The sequence's branch (`originalRef`) was renamed to `current` (HEAD's branch) during the
/// sequence, maybe in steps (#293): `originalRef` no longer exists, and `current`'s reflog
/// records the renames from it (`git branch -m` and libgit2 write "[Bb]ranch: renamed <old> to
/// <new>" and carry the log along). git's `cherry-pick --abort` rewinds a renamed branch, since
/// it records commits, not the name.
///
/// Only renames written during the sequence count: a rename entry's ids are the branch tip at
/// that moment, which then was the sequence's start (`original_head`) or a commit after it (a
/// rename by a step's `post-commit` hook too). An older rename that once had the same name, and
/// a branch switched to after the original was deleted (no rename entries), aren't taken for
/// a rename. This is a heuristic, not proof: a reflog entry can be written with any message
/// (`git update-ref -m`), so a forged "renamed" entry at such a commit would count.
fn renamed_to(repo: &Repository, state: &SequenceState, current: &OriginalRef) -> bool {
    let (OriginalRef::Branch(original), OriginalRef::Branch(current)) =
        (&state.original_ref, current)
    else {
        return false;
    };
    if original == current || repo.find_reference(original).is_ok() {
        return false;
    }
    let Ok(start) = Oid::from_str(&state.original_head) else {
        return false;
    };
    let during_the_sequence =
        |id: Oid| id == start || repo.graph_descendant_of(id, start).unwrap_or(false);
    let Ok(reflog) = repo.reflog(current) else {
        return false;
    };
    // Entries come newest first; follow the renames oldest first.
    let mut name = original.clone();
    for entry in (0..reflog.len()).rev().filter_map(|i| reflog.get(i)) {
        if !during_the_sequence(entry.id_new()) {
            continue;
        }
        let Ok(Some(message)) = entry.message() else {
            continue;
        };
        let Some(rest) = message
            .strip_prefix("Branch: renamed ")
            .or_else(|| message.strip_prefix("branch: renamed "))
        else {
            continue;
        };
        if let Some(new) = rest.strip_prefix(&format!("{name} to ")) {
            name = new.trim().to_string();
        }
    }
    name == *current
}

/// Puts HEAD (the branch, or a detached HEAD) back at `original`, with the work tree and index
/// checked out like `git reset --merge`: libgit2's SAFE strategy keeps unrelated local edits and
/// untracked files, and refuses (changing nothing) when a local edit or an untracked file is in
/// the way of a file it would change.
fn rewind_to(repo: &Repository, original: &Commit<'_>, reflog: &str) -> Result<(), PickError> {
    // Safe checkouts create files through a symlinked folder (#114). The original tree is one
    // the user already had (#161), so its case collisions are not "brought in".
    let tree = original.tree()?;
    crate::worktree_guard::refuse_linked_folders_known::<PickError>(
        repo,
        &[crate::worktree_guard::Incoming::Checkout(&tree)],
        Some(&tree),
    )?;
    repo.checkout_tree(original.as_object(), None)?;
    if repo.head_detached()? {
        // With the reflog message, unlike `set_head_detached` ("checkout: moving from …").
        repo.reference("HEAD", original.id(), true, reflog)?;
    } else {
        repo.head()?.set_target(original.id(), reflog)?;
    }
    Ok(())
}

/// Ends a terminal `git cherry-pick A B C` / `git revert A B` (`.git/sequencer`) like
/// `git cherry-pick --abort` (#253, D7-A). Every check runs before anything is written, so a
/// refusal changes nothing, as with git:
///
/// - `confirmed` must still describe the sequence (HEAD, start, whether and how far it
///   rewinds), else `TerminalSequenceChanged`: the user confirmed something else;
/// - git's sequencer files must be readable (`SequencerDamaged`, git refuses these too);
/// - a paused step's touched paths must not hold work-tree changes on top of staged ones
///   (`merge::plan_operation_reset`, git's "not uptodate");
/// - a rewind must not overwrite a local change or an untracked file (`RewindBlocked`).
///
/// Then:
///
/// 1. a paused step (or its conflicts) is reset like `merge::abort_merge`, keeping unrelated
///    edits;
/// 2. when HEAD is still where the sequence last left it (`sequencer/abort-safety`), HEAD goes
///    back to where it started (`sequencer/head`), removing the commits the sequence made, as
///    git's `reset --merge` (reflog `reset: moving to <id>`). Otherwise HEAD stays (git:
///    "You seem to have moved HEAD. Not rewinding");
/// 3. the list is removed.
///
/// `rewound` is true when HEAD ends where the sequence started. Another operation paused here
/// (a merge started during the sequence) is `OperationInProgress`, nothing changed.
pub fn abort_terminal_sequence(
    repo: &Repository,
    confirmed: &ConfirmedAbort,
) -> Result<SequenceAbort, PickError> {
    let sequence = terminal_sequence(repo).ok_or(PickError::NoTerminalSequence)?;
    let ours = matches!(
        repo.state(),
        RepositoryState::Clean
            | RepositoryState::Bisect
            | RepositoryState::CherryPick
            | RepositoryState::CherryPickSequence
            | RepositoryState::Revert
            | RepositoryState::RevertSequence
    );
    if !ours {
        crate::repo::check_no_terminal_rebase(repo)?;
        return Err(PickError::OperationInProgress);
    }
    if let Some(damage) = sequence.damage {
        return Err(PickError::SequencerDamaged(damage_message(
            sequence.kind,
            damage,
            &sequence.started_at,
        )));
    }
    if ConfirmedAbort::of(repo, &sequence) != *confirmed {
        return Err(PickError::TerminalSequenceChanged(sequence.kind));
    }
    let step = if sequence.paused_on_conflict || fresh_index(repo)?.has_conflicts() {
        Some(crate::merge::plan_operation_reset(repo)?)
    } else {
        None
    };
    let original = if sequence.rewinds {
        let original = repo.find_commit(Oid::from_str(&sequence.started_at)?)?;
        let reset = step.as_ref().map(|step| step.paths()).unwrap_or_default();
        check_rewind(repo, &original, reset)?;
        Some(original)
    } else {
        None
    };
    // Nothing written before this point.
    if let Some(step) = step {
        crate::merge::abort_step_keeping_list(repo, step)?;
    }
    if let Some(original) = original {
        rewind_to(
            repo,
            &original,
            &format!("reset: moving to {}", sequence.started_at),
        )?;
    }
    match std::fs::remove_dir_all(repo.path().join("sequencer")) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(PickError::Commit(crate::commit::CommitError::Io(error)))
        }
        _ => {}
    }
    let head = repo.head().ok().and_then(|head| head.target());
    Ok(SequenceAbort {
        rewound: head.is_some_and(|head| head.to_string() == sequence.started_at),
    })
}

/// Whether the rewind from HEAD to `original` can run without losing anything, checked before
/// anything is written (git's `reset --merge` refuses the same cases up front): no linked
/// folder on the way, and every path that differs between the two commits is unchanged in
/// the index and the work tree (no staged or unstaged change, no untracked or ignored file in
/// the way). `reset` are the paused step's paths, which its abort puts back to HEAD first.
fn check_rewind(
    repo: &Repository,
    original: &Commit<'_>,
    reset: &[Vec<u8>],
) -> Result<(), PickError> {
    let tree = original.tree()?;
    crate::worktree_guard::refuse_linked_folders_known::<PickError>(
        repo,
        &[crate::worktree_guard::Incoming::Checkout(&tree)],
        Some(&tree),
    )?;
    let head_tree = repo.head()?.peel_to_tree()?;
    let diff = repo.diff_tree_to_tree(Some(&head_tree), Some(&tree), None)?;
    for delta in diff.deltas() {
        for file in [delta.old_file(), delta.new_file()] {
            let (Some(path), Some(bytes)) = (file.path(), file.path_bytes()) else {
                continue;
            };
            if reset.iter().any(|reset| reset.as_slice() == bytes) {
                continue;
            }
            let blocked = match repo.status_file(path) {
                Ok(status) => !status.is_empty(),
                // Nowhere: in neither HEAD, the index nor the work tree.
                Err(error) if error.code() == git2::ErrorCode::NotFound => false,
                // A folder (`status_file` errors on one): a file the steps replaced by a
                // folder that HEAD tracks. Only something in it that the rewind would lose
                // blocks it.
                Err(_) => !is_clean_tracked_folder(repo, &head_tree, path)?,
            };
            if blocked {
                return Err(PickError::RewindBlocked(
                    path.to_string_lossy().into_owned(),
                ));
            }
        }
    }
    Ok(())
}

/// `path` is a folder in the work tree, HEAD has a folder there too, and nothing in it is
/// changed, staged, untracked or ignored: removing it loses nothing.
fn is_clean_tracked_folder(
    repo: &Repository,
    head_tree: &git2::Tree<'_>,
    path: &std::path::Path,
) -> Result<bool, PickError> {
    let Some(workdir) = repo.workdir() else {
        return Ok(false);
    };
    let is_folder = std::fs::symlink_metadata(workdir.join(path)).is_ok_and(|meta| meta.is_dir());
    let tracked_folder = head_tree
        .get_path(path)
        .is_ok_and(|entry| entry.kind() == Some(git2::ObjectType::Tree));
    if !is_folder || !tracked_folder {
        return Ok(false);
    }
    let prefix = format!("{}/", path.to_string_lossy().replace('\\', "/"));
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(true)
        .recurse_ignored_dirs(true)
        .pathspec(&prefix);
    let statuses = repo.statuses(Some(&mut options))?;
    Ok(statuses.iter().all(|entry| {
        !entry.path_bytes().starts_with(prefix.as_bytes()) || entry.status().is_empty()
    }))
}

/// Why Abort refuses a damaged terminal sequence, with git's way out (`--quit` ends it without
/// moving HEAD).
fn damage_message(kind: SequenceKind, damage: SequencerDamage, started_at: &str) -> String {
    let noun = match kind {
        SequenceKind::CherryPick => "cherry-pick",
        SequenceKind::Revert => "revert",
    };
    let what = match damage {
        SequencerDamage::MissingHead => {
            format!("git's record of where this {noun} started (.git/sequencer/head) is missing")
        }
        SequencerDamage::CorruptHead => {
            format!("git's record of where this {noun} started (.git/sequencer/head) is damaged")
        }
        SequencerDamage::CorruptAbortSafety => format!(
            "git's record of where this {noun} left HEAD (.git/sequencer/abort-safety) is damaged"
        ),
        SequencerDamage::MissingStart => format!(
            "the commit this {noun} started from ({}) is missing",
            started_at.get(..7).unwrap_or(started_at)
        ),
    };
    format!(
        "Can't abort: {what}. Run git {noun} --quit in a terminal to end it and keep HEAD where \
         it is."
    )
}
