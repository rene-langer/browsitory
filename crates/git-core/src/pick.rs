use git2::{Commit, Oid, Repository, RepositoryState};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PickOutcome {
    Committed { commit_id: String },
    Conflicted { files: Vec<String> },
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
    #[error("{0}")]
    LinkedFolder(#[from] crate::LinkedFolder),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SequenceAbort {
    /// HEAD went back to where the sequence started. `false` when HEAD had been moved elsewhere
    /// since the sequence stopped, so it was left alone.
    pub rewound: bool,
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
}

/// Applies `commit_id` on top of HEAD. A clean result is committed immediately with the original
/// message and author; a conflict leaves the repo in `RepositoryState::CherryPick` with
/// `MERGE_MSG` written, for the existing conflict pane + `commit::commit` / `merge::abort_merge`
/// to finish. A merge commit needs `mainline` (1-based parent number, like `git cherry-pick -m`);
/// any other commit must not get one.
pub fn cherry_pick(
    repo: &Repository,
    commit_id: &str,
    mainline: Option<u32>,
) -> Result<PickOutcome, PickError> {
    apply(repo, commit_id, Kind::CherryPick, mainline)
}

/// Commits the inverse of `commit_id` on top of HEAD. Same conflict and `mainline` contract as
/// `cherry_pick` (state `RepositoryState::Revert`).
pub fn revert(
    repo: &Repository,
    commit_id: &str,
    mainline: Option<u32>,
) -> Result<PickOutcome, PickError> {
    apply(repo, commit_id, Kind::Revert, mainline)
}

fn apply(
    repo: &Repository,
    commit_id: &str,
    kind: Kind,
    mainline: Option<u32>,
) -> Result<PickOutcome, PickError> {
    let head = check_ready(repo)?;
    let target = repo.find_commit(Oid::from_str(commit_id)?)?;
    check_mainline(&target, mainline)?;
    check_signing(repo)?;
    match step(repo, &head, &target, kind, mainline)? {
        StepResult::Committed(oid) => Ok(PickOutcome::Committed {
            commit_id: oid.to_string(),
        }),
        StepResult::Empty => Err(PickError::EmptyResult),
        StepResult::Conflicted(files) => Ok(PickOutcome::Conflicted { files }),
    }
}

/// The preconditions every pick shares, in order: nothing paused, a born HEAD, nothing staged.
/// Returns HEAD's commit.
fn check_ready(repo: &Repository) -> Result<Commit<'_>, PickError> {
    if repo.state() != RepositoryState::Clean {
        return Err(PickError::OperationInProgress);
    }
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

/// Applies `target` on top of `head` (HEAD's commit) and commits a clean, non-empty result.
/// Callers have run `check_ready`, `check_mainline` and `check_signing`.
fn step(
    repo: &Repository,
    head: &Commit<'_>,
    target: &Commit<'_>,
    kind: Kind,
    mainline: Option<u32>,
) -> Result<StepResult, PickError> {
    // libgit2's default checkout strategy here is SAFE: it errors rather than overwrite a
    // modified working-tree file the pick needs to change. libgit2 may already have written
    // CHERRY_PICK_HEAD/REVERT_HEAD + MERGE_MSG by then; state was Clean on entry (checked by
    // the caller), so clearing those files restores exactly the prior state without touching
    // user files.
    // Safe checkouts create files through a symlinked tracked folder (#114).
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
    let applied = match kind {
        Kind::CherryPick => {
            let mut options = git2::CherrypickOptions::new();
            if let Some(mainline) = mainline {
                options.mainline(mainline);
            }
            repo.cherrypick(target, Some(&mut options))
        }
        Kind::Revert => {
            let mut options = git2::RevertOptions::new();
            if let Some(mainline) = mainline {
                options.mainline(mainline);
            }
            repo.revert(target, Some(&mut options))
        }
    };
    if let Err(error) = applied {
        let _ = repo.cleanup_state();
        return Err(error.into());
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
        repo.cleanup_state()?;
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
    let oid = crate::signing::create_commit(
        repo,
        Some(crate::signing::HeadUpdate {
            expected: Some(head.id()),
            reflog_operation: "commit",
        }),
        &author,
        &committer,
        &message,
        &tree,
        &[head],
    )?;
    repo.cleanup_state()?;
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
/// with the original author and message; a step whose result is empty is skipped. A conflict
/// stops the sequence (git state `CherryPick`, like a single pick) and returns its state for
/// `continue_sequence`/`abort_sequence`. Merge commits are refused up front. Like
/// `cherry_pick`, steps run no commit hooks.
pub fn cherry_pick_many(repo: &Repository, ids_oldest_first: &[String]) -> SequenceRun {
    start(repo, ids_oldest_first, SequenceKind::CherryPick)
}

/// Reverts `ids_oldest_first` newest-first (like `git revert A..B`), otherwise as
/// `cherry_pick_many`.
pub fn revert_many(repo: &Repository, ids_oldest_first: &[String]) -> SequenceRun {
    start(repo, ids_oldest_first, SequenceKind::Revert)
}

fn start(repo: &Repository, ids: &[String], kind: SequenceKind) -> SequenceRun {
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
    let mut commits = ids.to_vec();
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

/// Runs the steps from `state.next` on. HEAD is the base of each step (the caller made sure it
/// is `state.expected_head` or one commit further for a step the user finished).
fn run(repo: &Repository, mut state: SequenceState) -> SequenceRun {
    while state.next < state.commits.len() {
        let result = (|| {
            let head = repo.head()?.peel_to_commit()?;
            state.expected_head = head.id().to_string();
            let target = repo.find_commit(Oid::from_str(&state.commits[state.next])?)?;
            step(repo, &head, &target, state.kind.into(), None)
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
            Err(error) => {
                state.stopped_on_conflict = false;
                let untouched = state.next == 0
                    && state.skipped.is_empty()
                    && state.expected_head == state.original_head
                    && repo.state() == RepositoryState::Clean;
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
/// `OperationInProgress`; both keep the state so the sequence can still be aborted. Commits
/// made here run no hooks, like every sequence step.
pub fn continue_sequence(repo: &Repository, mut state: SequenceState) -> SequenceRun {
    if state.next >= state.commits.len() {
        return SequenceRun::stopped(
            git2::Error::from_str("the stored sequence has no step left"),
            state,
        );
    }
    if let Err(error) = check_signing(repo) {
        return SequenceRun::stopped(error, state);
    }
    let resumed = match repo.state() {
        RepositoryState::CherryPick
        | RepositoryState::CherryPickSequence
        | RepositoryState::Revert
        | RepositoryState::RevertSequence => finish_paused(repo, &mut state),
        RepositoryState::Clean => resume_clean(repo, &mut state),
        _ => Err(PickError::OperationInProgress),
    };
    match resumed {
        Ok(()) => run(repo, state),
        Err(error) => SequenceRun::stopped(error, state),
    }
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

fn finish_paused(repo: &Repository, state: &mut SequenceState) -> Result<(), PickError> {
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
    if tree_id == head.tree_id() {
        repo.cleanup_state()?;
        state.skipped.push(current);
        state.next += 1;
        return Ok(());
    }
    let target = repo.find_commit(Oid::from_str(&current)?)?;
    let tree = repo.find_tree(tree_id)?;
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
    crate::signing::create_commit(
        repo,
        Some(crate::signing::HeadUpdate {
            expected: Some(head.id()),
            reflog_operation: "commit",
        }),
        &author,
        &committer,
        &message,
        &tree,
        &[&head],
    )?;
    repo.cleanup_state()?;
    state.next += 1;
    Ok(())
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
        SequenceKind::CherryPick => {
            let (ours, theirs) = (head.author(), target.author());
            ours.name_bytes() == theirs.name_bytes()
                && ours.email_bytes() == theirs.email_bytes()
                && ours.when() == theirs.when()
        }
        SequenceKind::Revert => String::from_utf8_lossy(head.message_bytes())
            .contains(&format!("This reverts commit {}", target.id())),
    })
}

/// Ends a stopped sequence. A paused step of this sequence is aborted like `git cherry-pick
/// --abort` (unrelated edits survive). If HEAD is still where the sequence stopped, the steps
/// that landed are undone by checking out the original HEAD (libgit2's SAFE strategy: a local
/// edit in the way is an error, and Abort can be retried) and moving the branch (or detached
/// HEAD) back. When HEAD was moved elsewhere, or another operation is paused, nothing is moved
/// (`rewound: false`). The caller forgets the state only on `Ok`.
pub fn abort_sequence(
    repo: &Repository,
    state: &SequenceState,
) -> Result<SequenceAbort, PickError> {
    let head_id = repo.head()?.peel_to_commit()?.id().to_string();
    if is_own_paused_step(repo, state) {
        crate::merge::abort_merge(repo)?;
    } else if repo.state() == RepositoryState::Clean
        && head_id == state.expected_head
        && fresh_index(repo)?.has_conflicts()
    {
        // The step's conflict without its CHERRY_PICK_HEAD/REVERT_HEAD (deleted outside):
        // still ours to clean up, like `git cherry-pick --abort` would have.
        crate::merge::abort_merge(repo)?;
    }
    if repo.state() != RepositoryState::Clean {
        // Someone else's operation is paused here; don't pull HEAD from under it.
        return Ok(SequenceAbort {
            rewound: head_id == state.original_head,
        });
    }
    let same_ref =
        head_ref(repo)? == state.original_ref && state.original_ref != OriginalRef::Unknown;
    if !same_ref {
        // HEAD now names another branch (or detached/attached changed): rewinding would move
        // a ref the sequence never touched.
        return Ok(SequenceAbort {
            rewound: head_id == state.original_head,
        });
    }
    if head_id == state.expected_head && state.expected_head != state.original_head {
        let original = repo.find_commit(Oid::from_str(&state.original_head)?)?;
        // Safe checkouts create files through a symlinked tracked folder (#114).
        let tree = original.tree()?;
        crate::worktree_guard::refuse_linked_folders::<PickError>(
            repo,
            &[crate::worktree_guard::Incoming::Checkout(&tree)],
        )?;
        repo.checkout_tree(original.as_object(), None)?;
        let reflog = match state.kind {
            SequenceKind::CherryPick => "cherry-pick: abort",
            SequenceKind::Revert => "revert: abort",
        };
        if repo.head_detached()? {
            repo.set_head_detached(original.id())?;
        } else {
            repo.head()?.set_target(original.id(), reflog)?;
        }
        return Ok(SequenceAbort { rewound: true });
    }
    Ok(SequenceAbort {
        rewound: head_id == state.original_head,
    })
}
