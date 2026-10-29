use git2::{Repository, Sort};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RebaseError {
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("invalid rebase plan: {0}")]
    InvalidPlan(String),
    #[error("no rebase is currently in progress")]
    NotRebasing,
    #[error(
        "HEAD moved out from under the in-progress rebase (expected {expected}, found {actual}) \
         — something else (a branch switch, an external git command) moved it; abort the rebase \
         and start over"
    )]
    HeadMoved { expected: String, actual: String },
    #[error("{0}")]
    Sign(#[from] crate::signing::SignError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    LinkedFolder(#[from] crate::LinkedFolder),
    #[error(transparent)]
    CaseCollision(#[from] crate::CaseCollision),
    #[error(
        "Another merge, cherry-pick or revert is in progress. Finish or abort it before \
         continuing the rebase."
    )]
    OperationInProgress,
    /// `branch` is the short name.
    #[error("{branch} moved since the rebase started. Abort the rebase and start again.")]
    BranchMoved { branch: String },
    #[error("{branch} no longer exists. Abort the rebase.")]
    BranchMissing { branch: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebasePlanCommit {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub timestamp: i64,
}

pub fn commits_since(repo: &Repository, onto: &str) -> Result<Vec<RebasePlanCommit>, RebaseError> {
    let onto_oid = repo.revparse_single(onto)?.peel_to_commit()?.id();

    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    revwalk.push_head()?;
    revwalk.hide(onto_oid)?;
    // First-parent only: a rebase replays a linear sequence of commits, so a merge commit's
    // second-parent side must not be pulled into the plan as a flat run of unrelated commits.
    // The merge commit itself still shows up here (`--first-parent` includes it) — that's
    // deliberate: `validate_plan` rejects a plan containing one with an explicit error rather
    // than silently dropping it (which would lose the merged side's content from the replay).
    revwalk.simplify_first_parent()?;

    let mut commits = Vec::new();
    for oid_result in revwalk {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;
        let id = oid.to_string();
        commits.push(RebasePlanCommit {
            short_id: id[..7].to_string(),
            id,
            summary: commit
                .summary()
                .ok()
                .flatten()
                .unwrap_or_default()
                .to_string(),
            author_name: commit.author().name().ok().unwrap_or_default().to_string(),
            timestamp: commit.time().seconds(),
        });
    }
    // `revwalk` yields newest-first; the plan wants oldest-first, matching actual replay order.
    commits.reverse();
    Ok(commits)
}

use git2::Oid;

use crate::merge::conflict_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebaseAction {
    Pick,
    Reword { message: String },
    Edit,
    Squash,
    Fixup,
    Drop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebasePlanEntry {
    pub commit_id: String,
    pub action: RebaseAction,
    pub combined_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebaseStepResult {
    Conflicted { files: Vec<String> },
    PausedForEdit,
    Advanced,
    Done,
}

/// Why a rebase is stopped. `Conflict` and `Edit` mean the current step was applied to the
/// index (`CHERRY_PICK_HEAD` written); `Error` means a step (or `finish`) failed, so the step at
/// `cursor` is not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebaseStop {
    Conflict,
    Edit,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RebaseProgress {
    pub current_step: usize,
    pub total_steps: usize,
}

/// A paused rebase. Plain data, so `repo-service` can persist it and rebuild it after a
/// restart; whether it still matches the repository is `rebase_health`'s job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebaseState {
    pub plan: Vec<RebasePlanEntry>,
    /// Index of the stopped entry; `plan.len()` after a failed `finish`.
    pub cursor: usize,
    /// `refs/heads/<name>`: the branch being rebased, moved only by `finish`.
    pub original_branch_ref: String,
    /// The branch tip when the rebase started; `finish` refuses when the branch moved.
    pub original_tip: Oid,
    pub onto: Oid,
    pub group_start_parent: Option<Oid>,
    /// The OID this crate last moved `HEAD` to (the initial detach, then each landed step or
    /// group collapse). `rebase_continue` refuses to run when the real `HEAD` has drifted from
    /// it — e.g. because something else switched branches on the same repo mid-pause — since
    /// continuing would commit the rebased step onto whatever ref happened to be checked out.
    pub expected_head: Oid,
    pub stop: RebaseStop,
}

impl RebaseState {
    /// 1-indexed number of the plan entry currently in flight, for a "Step N of M" display —
    /// the first pause reports step 1, not 0. Clamped to `total_steps` so a state whose cursor
    /// has run past the end (a failed `finish`) still reports "N of N" rather than N+1.
    pub fn progress(&self) -> RebaseProgress {
        RebaseProgress {
            current_step: (self.cursor + 1).min(self.plan.len()),
            total_steps: self.plan.len(),
        }
    }

    pub fn current_step(&self) -> usize {
        self.progress().current_step
    }

    pub fn total_steps(&self) -> usize {
        self.progress().total_steps
    }
}

fn validate_plan(repo: &Repository, plan: &[RebasePlanEntry]) -> Result<(), RebaseError> {
    // A merge commit can't be replayed by `Repository::cherrypick` (libgit2 errors with
    // "mainline branch is not specified but ... is a merge commit"), and the rebase model here is
    // first-parent-only anyway. Reject the plan up front — before `start_rebase` detaches HEAD —
    // rather than letting `advance` blow up mid-flight.
    for entry in plan {
        if matches!(entry.action, RebaseAction::Drop) {
            continue;
        }
        let commit = repo.find_commit(Oid::from_str(&entry.commit_id)?)?;
        if commit.parent_count() > 1 {
            return Err(RebaseError::InvalidPlan(format!(
                "{} is a merge commit — merge commits cannot be rebased (drop it from the plan \
                 or pick a different starting point)",
                &entry.commit_id[..7.min(entry.commit_id.len())]
            )));
        }
    }

    // Look past any leading `Drop` entries — a plan like `[Drop, Squash]` has no earlier entry
    // for the `Squash` to combine into either, since `Drop` never lands a commit.
    if let Some(first_non_drop) = plan
        .iter()
        .find(|e| !matches!(e.action, RebaseAction::Drop))
    {
        if matches!(
            first_non_drop.action,
            RebaseAction::Squash | RebaseAction::Fixup
        ) {
            return Err(RebaseError::InvalidPlan(
                "the first non-Drop entry cannot be Squash or Fixup — there is no preceding \
                 commit in the plan for it to combine into"
                    .to_string(),
            ));
        }
    }
    Ok(())
}

pub fn start_rebase(
    repo: &Repository,
    onto: &str,
    plan: Vec<RebasePlanEntry>,
) -> Result<(RebaseState, RebaseStepResult), RebaseError> {
    validate_plan(repo, &plan)?;

    // `Reference::name()` returns `Ok("HEAD")` for a detached HEAD — it only errors on a
    // non-UTF-8 name — so it can't be used to detect "not on a branch". Check explicitly instead;
    // without this, `finish()` would move the literal `HEAD` ref rather than a real branch ref,
    // silently stranding every rebased commit as unreachable with no error surfaced.
    if repo.head_detached()? {
        return Err(RebaseError::InvalidPlan(
            "cannot start a rebase while HEAD is already detached — check out a branch first"
                .to_string(),
        ));
    }

    let onto_commit = repo.revparse_single(onto)?.peel_to_commit()?;
    let head_ref = repo.head()?;
    let original_branch_ref = head_ref.name()?.to_string();
    let original_tip = head_ref.peel_to_commit()?.id();

    // Checking out before detaching HEAD means a refused checkout (modified/untracked files in
    // the way) leaves the repo exactly as it was — the same safety ordering
    // `branch::switch_branch` and `merge::start_merge`'s fast-forward path already use.
    // Safe checkouts create files through a symlinked folder (#114).
    let onto_tree = onto_commit.tree()?;
    crate::worktree_guard::refuse_linked_folders::<RebaseError>(
        repo,
        &[crate::worktree_guard::Incoming::Checkout(&onto_tree)],
    )?;
    repo.checkout_tree(onto_commit.as_object(), None)?;
    repo.set_head_detached(onto_commit.id())?;

    let mut state = RebaseState {
        plan,
        cursor: 0,
        original_branch_ref,
        original_tip,
        onto: onto_commit.id(),
        group_start_parent: None,
        expected_head: onto_commit.id(),
        stop: RebaseStop::Error,
    };

    // Everything past the detach above must roll back on failure: an `Err` escaping here would
    // otherwise leave the repo on a detached HEAD with no `RebaseState` anywhere for the caller
    // to abort with (`worker.rs` only stores the state on `Ok`), stranding the user with no
    // in-app way back to their branch.
    match advance(repo, &mut state) {
        Ok(result) => Ok((state, result)),
        Err(err) => {
            // Best-effort: report the *original* failure even if the rollback itself fails,
            // since that's the actionable one.
            let _ = restore_original_branch(repo, &state.original_branch_ref);
            Err(err)
        }
    }
}

/// Goes on with a paused rebase, deciding from the repository what the pause left behind (the
/// table mirrors `pick::continue_sequence`):
///
/// - git's paused pick is this rebase's step and HEAD is where the rebase left it: commit the
///   index as the step (a resolved conflict or an edit), then go on;
/// - nothing is paused and HEAD is where the rebase left it: the step never landed (an error
///   stop, or `git cherry-pick --abort` outside), so apply it again instead of committing the
///   unchanged index as an empty step;
/// - another operation is paused: `OperationInProgress`; HEAD moved: `HeadMoved`.
///
/// Refusals leave `state` untouched. Any later error marks the stop as `Error` unless the
/// step is still applied and paused; the caller keeps the state as returned (the cursor may
/// have moved past steps that landed).
pub fn rebase_continue(
    repo: &Repository,
    state: &mut RebaseState,
) -> Result<RebaseStepResult, RebaseError> {
    let resume = resume_kind(repo, state)?;
    let result = match resume {
        Resume::Land => land_current_step(repo, state).and_then(|()| {
            state.cursor += 1;
            advance(repo, state)
        }),
        Resume::Reapply => advance(repo, state),
        Resume::Finish => finish(repo, state),
        Resume::Adopt => adopt_step(repo, state).and_then(|()| {
            state.cursor += 1;
            advance(repo, state)
        }),
    };
    if result.is_err() {
        state.stop = if is_own_paused_step(repo, state) {
            match state.stop {
                RebaseStop::Error => RebaseStop::Conflict,
                applied => applied,
            }
        } else {
            RebaseStop::Error
        };
    }
    result
}

enum Resume {
    /// Commit the index as the paused step.
    Land,
    /// Apply the step at the cursor again.
    Reapply,
    /// HEAD is the paused step, committed outside the app: take it as the step.
    Adopt,
    /// Every step landed but `finish` failed: retry it.
    Finish,
}

fn resume_kind(repo: &Repository, state: &RebaseState) -> Result<Resume, RebaseError> {
    // Past the end of the plan there's no step left to land — without this guard,
    // `land_current_step`'s `state.plan[state.cursor]` panics and takes the whole worker thread
    // (and every later command for this repo) down with it. Reachable from a double-clicked
    // Continue, or from a `finish()` that failed after the cursor had already run off the end.
    if state.cursor > state.plan.len() {
        return Err(RebaseError::NotRebasing);
    }
    use git2::RepositoryState::{CherryPick, CherryPickSequence, Clean};
    if state.cursor == state.plan.len() {
        // Every step landed and `finish` failed (the branch moved or was deleted): retry it.
        if repo.state() != Clean {
            return Err(RebaseError::OperationInProgress);
        }
        if !head_at_expected(repo, state)? {
            let head = repo.head()?;
            let finished = head.name().ok() == Some(state.original_branch_ref.as_str())
                && head.target() == Some(state.expected_head);
            if finished {
                // A stale caller (a double-clicked Continue) on a rebase that already finished.
                return Err(RebaseError::NotRebasing);
            }
            return Err(head_moved(repo, state)?);
        }
        return Ok(Resume::Finish);
    }
    match repo.state() {
        CherryPick | CherryPickSequence => {
            if !is_own_paused_step(repo, state) {
                return Err(RebaseError::OperationInProgress);
            }
            // Nothing but this module is supposed to move HEAD while a rebase is paused. If
            // something did (a branch switch, an external `git checkout`), landing this step
            // would commit it onto the wrong ref and silently rewrite unrelated history —
            // refuse instead, leaving `abort` as the way out.
            check_head_at_expected(repo, state)?;
            Ok(Resume::Land)
        }
        Clean => {
            if head_at_expected(repo, state)? {
                return Ok(Resume::Reapply);
            }
            if is_step_committed_by_hand(repo, state)? {
                return Ok(Resume::Adopt);
            }
            Err(head_moved(repo, state)?)
        }
        _ => Err(RebaseError::OperationInProgress),
    }
}

/// True when git's paused pick is this rebase's current step, not one started some other way.
fn is_own_paused_step(repo: &Repository, state: &RebaseState) -> bool {
    use git2::RepositoryState::{CherryPick, CherryPickSequence};
    let Some(entry) = state.plan.get(state.cursor) else {
        return false;
    };
    matches!(repo.state(), CherryPick | CherryPickSequence)
        && repo
            .find_reference("CHERRY_PICK_HEAD")
            .ok()
            .and_then(|reference| reference.target())
            .map(|oid| oid.to_string())
            .as_deref()
            == Some(entry.commit_id.as_str())
}

/// Whether a stored rebase still matches the repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebaseHealth {
    Paused,
    Diverged(RebaseDivergence),
}

/// What changed outside the app while the rebase was paused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RebaseDivergence {
    HeadMoved,
    BranchMoved,
    BranchMissing,
    OtherOperation,
}

/// Classifies a stored rebase against the repository. The first failing check decides: the
/// branch is gone, the branch moved, git has another operation paused, HEAD is not where the
/// rebase left it (a step committed by hand that Continue would adopt still counts as paused).
/// A diverged rebase can only be aborted.
pub fn rebase_health(repo: &Repository, state: &RebaseState) -> Result<RebaseHealth, RebaseError> {
    use RebaseDivergence::{BranchMissing, BranchMoved, HeadMoved, OtherOperation};
    let tip = match repo.find_reference(&state.original_branch_ref) {
        Ok(reference) => reference.target(),
        Err(error) if error.code() == git2::ErrorCode::NotFound => {
            return Ok(RebaseHealth::Diverged(BranchMissing))
        }
        Err(error) => return Err(error.into()),
    };
    if tip != Some(state.original_tip) {
        return Ok(RebaseHealth::Diverged(BranchMoved));
    }
    let own_pick = state.cursor < state.plan.len() && is_own_paused_step(repo, state);
    if repo.state() != git2::RepositoryState::Clean && !own_pick {
        return Ok(RebaseHealth::Diverged(OtherOperation));
    }
    if head_at_expected(repo, state)? || is_step_committed_by_hand(repo, state)? {
        return Ok(RebaseHealth::Paused);
    }
    Ok(RebaseHealth::Diverged(HeadMoved))
}

/// After a conflict or edit stop the user may finish the step with git (`git cherry-pick
/// --continue` or `git commit`, which `git status` suggests). HEAD counts as that step only when
/// git has nothing paused, HEAD is detached exactly one commit past the stop, and it keeps the
/// planned commit's author (`pick::commit_matches_pick`). After an error stop the step was
/// never applied, so nothing is adopted.
fn is_step_committed_by_hand(repo: &Repository, state: &RebaseState) -> Result<bool, RebaseError> {
    let Some(entry) = state.plan.get(state.cursor) else {
        return Ok(false);
    };
    if state.stop == RebaseStop::Error
        || matches!(entry.action, RebaseAction::Drop)
        || repo.state() != git2::RepositoryState::Clean
        || !repo.head_detached()?
    {
        return Ok(false);
    }
    let head = repo.head()?.peel_to_commit()?;
    let picked = repo.find_commit(Oid::from_str(&entry.commit_id)?)?;
    Ok(crate::pick::commit_matches_pick(
        &head,
        state.expected_head,
        &picked,
    ))
}

/// Takes HEAD (checked by `is_step_committed_by_hand`) as the current step: a `Reword` entry
/// gets its planned message, then the bookkeeping `land_current_step` does after its commit.
fn adopt_step(repo: &Repository, state: &mut RebaseState) -> Result<(), RebaseError> {
    let entry = state.plan[state.cursor].clone();
    if let RebaseAction::Reword { message } = &entry.action {
        let head = repo.head()?.peel_to_commit()?;
        if head.message_bytes() != message.as_bytes() {
            let parent = head.parent(0)?;
            let committer = repo.signature()?;
            let reworded = crate::signing::create_commit(
                repo,
                None,
                &head.author(),
                &committer,
                message,
                &head.tree()?,
                &[&parent],
            )?;
            repo.set_head_detached(reworded)?;
        }
    }
    state.expected_head = repo.head()?.peel_to_commit()?.id();
    after_landed(repo, state)
}

/// HEAD detached at `expected_head`, where the rebase left it.
fn head_at_expected(repo: &Repository, state: &RebaseState) -> Result<bool, RebaseError> {
    Ok(repo.head_detached()? && repo.head()?.peel_to_commit()?.id() == state.expected_head)
}

fn check_head_at_expected(repo: &Repository, state: &RebaseState) -> Result<(), RebaseError> {
    if head_at_expected(repo, state)? {
        return Ok(());
    }
    Err(head_moved(repo, state)?)
}

/// The `HeadMoved` refusal, naming where HEAD is now.
fn head_moved(repo: &Repository, state: &RebaseState) -> Result<RebaseError, RebaseError> {
    let head = repo.head()?;
    let id = head.peel_to_commit()?.id().to_string();
    let actual = if repo.head_detached()? {
        id
    } else {
        format!("{} at {id}", head.shorthand().unwrap_or("a branch"))
    };
    Ok(RebaseError::HeadMoved {
        expected: state.expected_head.to_string(),
        actual,
    })
}

/// Auto-advances through plan entries starting at `state.cursor`: applies each via cherry-pick,
/// pausing at the first conflict or `Edit` step, or reaching `Done` once every entry lands.
fn advance(repo: &Repository, state: &mut RebaseState) -> Result<RebaseStepResult, RebaseError> {
    loop {
        if state.cursor >= state.plan.len() {
            return finish(repo, state);
        }

        let entry = state.plan[state.cursor].clone();

        if matches!(entry.action, RebaseAction::Drop) {
            state.cursor += 1;
            continue;
        }

        let commit = repo.find_commit(Oid::from_str(&entry.commit_id)?)?;

        if state.group_start_parent.is_none() && starts_a_group(&state.plan, state.cursor) {
            state.group_start_parent = Some(repo.head()?.peel_to_commit()?.id());
        }

        // Safe checkouts create files through a symlinked folder (#114).
        // The step writes the picked result; scan that (an untracked link counts too).
        let head_commit = repo.head()?.peel_to_commit()?;
        let merged = repo.cherrypick_commit(&commit, &head_commit, 0, None)?;
        crate::worktree_guard::refuse_linked_folders::<RebaseError>(
            repo,
            &[crate::worktree_guard::Incoming::Merge(
                crate::worktree_guard::GuardSource::Index(&merged),
            )],
        )?;
        repo.cherrypick(&commit, None)?;

        if repo.index()?.has_conflicts() {
            state.stop = RebaseStop::Conflict;
            return Ok(RebaseStepResult::Conflicted {
                files: conflicted_paths(repo)?,
            });
        }

        if matches!(entry.action, RebaseAction::Edit) {
            state.stop = RebaseStop::Edit;
            return Ok(RebaseStepResult::PausedForEdit);
        }

        land_current_step(repo, state)?;
        state.cursor += 1;
    }
}

/// The action of the nearest entry strictly after `index` that isn't itself `Drop` — `Drop`
/// entries never land a commit, so a group boundary must look past them rather than at the
/// literal next slot (a `Drop` sitting between a group's leader and its members, or between two
/// members, must not hide the group from `starts_a_group`/`ends_a_group`).
fn next_non_drop_action(plan: &[RebasePlanEntry], index: usize) -> Option<&RebaseAction> {
    plan[index + 1..]
        .iter()
        .find(|e| !matches!(e.action, RebaseAction::Drop))
        .map(|e| &e.action)
}

fn starts_a_group(plan: &[RebasePlanEntry], index: usize) -> bool {
    matches!(
        next_non_drop_action(plan, index),
        Some(RebaseAction::Squash) | Some(RebaseAction::Fixup)
    )
}

fn ends_a_group(plan: &[RebasePlanEntry], index: usize) -> bool {
    matches!(
        plan[index].action,
        RebaseAction::Squash | RebaseAction::Fixup
    ) && !matches!(
        next_non_drop_action(plan, index),
        Some(RebaseAction::Squash) | Some(RebaseAction::Fixup)
    )
}

/// Commits the currently-staged result for `plan[state.cursor]` as a real intermediate commit
/// (parent = current `HEAD`, author preserved from the original commit, committer = the current
/// user) — then, if this entry ends a squash/fixup group, collapses the whole group's chain of
/// intermediate commits into one final commit reparented onto `group_start_parent`, carrying the
/// group leader's `combined_message` and the group leader's author (not this last member's). The
/// leader is found by walking backward from this entry to the nearest preceding entry that's
/// neither `Squash`/`Fixup` nor `Drop` (a `Drop` never lands a commit, so it can't be the leader
/// either, even if one sits between the leader and the group) — guaranteed to exist, since
/// `validate_plan` rejects a plan whose first non-`Drop` entry is `Squash`/`Fixup`.
fn land_current_step(repo: &Repository, state: &mut RebaseState) -> Result<(), RebaseError> {
    let entry = state.plan[state.cursor].clone();
    let original_commit = repo.find_commit(Oid::from_str(&entry.commit_id)?)?;
    let message = match &entry.action {
        RebaseAction::Reword { message } => message.clone(),
        _ => original_commit
            .message()
            .ok()
            .unwrap_or_default()
            .to_string(),
    };

    // Re-read the index from disk: a terminal `git add` during the pause must be seen.
    let mut index = repo.index()?;
    index.read(false)?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let parent = repo.head()?.peel_to_commit()?;
    let committer = repo.signature()?;
    crate::signing::create_commit(
        repo,
        Some(crate::signing::HeadUpdate {
            expected: Some(parent.id()),
            reflog_operation: "commit",
        }),
        &original_commit.author(),
        &committer,
        &message,
        &tree,
        &[&parent],
    )?;
    // Re-baseline the drift check right away: the collapse below can fail (e.g. the signer is
    // cancelled), and a retried Continue must see the HEAD this step just landed, not a stale
    // one that would read as `HeadMoved`.
    state.expected_head = repo.head()?.peel_to_commit()?.id();
    after_landed(repo, state)
}

/// What follows a landed step (committed here, or by hand and adopted): if the entry ends a
/// squash/fixup group, collapse the group; then re-baseline `expected_head` and forget git's
/// paused pick.
fn after_landed(repo: &Repository, state: &mut RebaseState) -> Result<(), RebaseError> {
    let committer = repo.signature()?;
    if ends_a_group(&state.plan, state.cursor) {
        // Walk backward to the nearest preceding entry that's neither `Squash`/`Fixup` (a group
        // member) nor `Drop` (never landed, so it can't be the leader either) — that's the group
        // leader. Guaranteed to exist: `validate_plan` rejects a plan whose first non-`Drop`
        // entry is `Squash`/`Fixup`.
        let leader_index = (0..=state.cursor)
            .rev()
            .find(|&i| {
                !matches!(
                    state.plan[i].action,
                    RebaseAction::Squash | RebaseAction::Fixup | RebaseAction::Drop
                )
            })
            .ok_or_else(|| {
                RebaseError::InvalidPlan(
                    "a squash/fixup group has no leader — validate_plan should have rejected \
                     this plan"
                        .to_string(),
                )
            })?;
        // The collapsed commit keeps the LEADER's author and message, not the last group
        // member's — `original_commit`/`message` at this point belong to this step's own (last)
        // commit, which is the wrong one; the leader is the semantically-original commit for the
        // whole group.
        let leader_commit =
            repo.find_commit(Oid::from_str(&state.plan[leader_index].commit_id)?)?;
        let combined_message = state.plan[leader_index]
            .combined_message
            .clone()
            .unwrap_or_else(|| match &state.plan[leader_index].action {
                // A `Reword`ed leader's new message is what the user asked this commit to say,
                // so it wins over the leader's original message as the implicit fallback.
                RebaseAction::Reword { message } => message.clone(),
                _ => leader_commit.message().ok().unwrap_or_default().to_string(),
            });

        let final_tree = repo.head()?.peel_to_commit()?.tree()?;
        let group_parent = repo.find_commit(state.group_start_parent.ok_or_else(|| {
            RebaseError::InvalidPlan(
                "ends_a_group fired without a recorded group_start_parent — internal rebase \
                 state error"
                    .to_string(),
            )
        })?)?;
        // No `HeadUpdate` here: libgit2 rejects a ref-updating commit() whose
        // first parent isn't the ref's *current* target, and the current target is the last
        // intermediate commit in the group's chain, not `group_parent` — that's the whole point
        // of the collapse (reparenting past that chain). Create the commit untethered, then move
        // HEAD to it directly.
        let final_oid = crate::signing::create_commit(
            repo,
            None,
            &leader_commit.author(),
            &committer,
            &combined_message,
            &final_tree,
            &[&group_parent],
        )?;
        repo.set_head_detached(final_oid)?;
        state.group_start_parent = None;
    }

    // This function is the only place (besides `start_rebase`'s initial detach) that moves HEAD
    // during a rebase, so re-baseline the drift check from wherever it just landed.
    state.expected_head = repo.head()?.peel_to_commit()?.id();
    // The step is committed: forget its CHERRY_PICK_HEAD/MERGE_MSG, so a later stop (an error
    // before the next pick, a failed `finish`) is a clean state, never a stale pick.
    repo.cleanup_state()?;

    Ok(())
}

fn conflicted_paths(repo: &Repository) -> Result<Vec<String>, RebaseError> {
    let index = repo.index()?;
    let mut files = Vec::new();
    for conflict in index.conflicts()? {
        let conflict = conflict?;
        if let Some(path) = conflict_path(&conflict) {
            if !files.contains(&path) {
                files.push(path);
            }
        }
    }
    Ok(files)
}

/// Moves the original branch to the rebased HEAD and reattaches HEAD. The branch must still be
/// where the rebase found it (compare-and-swap): a branch moved during the pause (another
/// worktree, `git branch -f`) would otherwise lose those commits. On an error the caller keeps
/// the state with `cursor == plan.len()`, and Continue retries.
fn finish(repo: &Repository, state: &RebaseState) -> Result<RebaseStepResult, RebaseError> {
    let branch = short_branch(&state.original_branch_ref);
    let current = match repo.find_reference(&state.original_branch_ref) {
        Ok(reference) => reference.target(),
        Err(error) if error.code() == git2::ErrorCode::NotFound => {
            return Err(RebaseError::BranchMissing { branch })
        }
        Err(error) => return Err(error.into()),
    };
    if current != Some(state.original_tip) {
        return Err(RebaseError::BranchMoved { branch });
    }
    let final_oid = repo.head()?.peel_to_commit()?.id();
    repo.reference_matching(
        &state.original_branch_ref,
        final_oid,
        true,
        state.original_tip,
        "rebase (finish): returning to branch",
    )
    .map_err(|error| match error.code() {
        git2::ErrorCode::Modified => RebaseError::BranchMoved {
            branch: branch.clone(),
        },
        _ => error.into(),
    })?;
    repo.set_head(&state.original_branch_ref)?;
    // Clears CHERRY_PICK_HEAD and any other in-progress-operation state left behind by the
    // `cherrypick()` calls in `advance` — without this, `repo.state()` stays `CherryPick` even
    // though the rebase (from this module's own state machine's point of view) is done.
    repo.cleanup_state()?;
    Ok(RebaseStepResult::Done)
}

/// `refs/heads/topic` → `topic`.
fn short_branch(branch_ref: &str) -> String {
    branch_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(branch_ref)
        .to_string()
}

pub fn abort_rebase(repo: &Repository, state: RebaseState) -> Result<(), RebaseError> {
    restore_original_branch(repo, &state.original_branch_ref)
}

/// Undoes an in-progress rebase: the original branch ref is never touched while a rebase runs —
/// only the detached HEAD moves — so recovery is just reattaching to it and force-checking-out
/// its tree over whatever the in-progress rebase left in the working directory/index. Shared by
/// `abort_rebase` and `start_rebase`'s post-detach rollback, which need identical behavior.
fn restore_original_branch(repo: &Repository, branch_ref: &str) -> Result<(), RebaseError> {
    repo.set_head(branch_ref)?;
    // The forced checkout writes (and deletes) through a symlink that replaced a tracked file or
    // folder; remove such links first, like git (#114).
    let tree = repo.head()?.peel_to_tree()?;
    crate::worktree_guard::prepare_forced_checkout::<RebaseError, &str>(
        repo,
        Some(&crate::worktree_guard::GuardSource::Tree(&tree)),
        None,
    )?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    repo.checkout_head(Some(&mut checkout))?;
    repo.cleanup_state()?;
    Ok(())
}
