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

pub struct RebaseState {
    plan: Vec<RebasePlanEntry>,
    cursor: usize,
    original_branch_ref: String,
    // Not read by this task's code — recorded here for `abort_rebase` (Task 3), which reattaches
    // to `original_branch_ref` at this tip to fully undo an in-progress rebase.
    #[allow(dead_code)]
    original_tip: Oid,
    group_start_parent: Option<Oid>,
}

impl RebaseState {
    /// 1-indexed count of plan entries processed so far, for a "Step N of M" display. Reaches
    /// `total_steps()` once the rebase is `Done`.
    pub fn current_step(&self) -> usize {
        self.cursor
    }

    pub fn total_steps(&self) -> usize {
        self.plan.len()
    }
}

fn validate_plan(plan: &[RebasePlanEntry]) -> Result<(), RebaseError> {
    if let Some(first) = plan.first() {
        if matches!(first.action, RebaseAction::Squash | RebaseAction::Fixup) {
            return Err(RebaseError::InvalidPlan(
                "the first entry cannot be Squash or Fixup — there is no preceding commit in \
                 the plan for it to combine into"
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
    validate_plan(&plan)?;

    let onto_commit = repo.revparse_single(onto)?.peel_to_commit()?;
    let head_ref = repo.head()?;
    let original_branch_ref = head_ref
        .name()
        .map_err(|_| RebaseError::InvalidPlan("HEAD is not on a branch".to_string()))?
        .to_string();
    let original_tip = head_ref.peel_to_commit()?.id();

    // Checking out before detaching HEAD means a refused checkout (modified/untracked files in
    // the way) leaves the repo exactly as it was — the same safety ordering
    // `branch::switch_branch` and `merge::start_merge`'s fast-forward path already use.
    repo.checkout_tree(onto_commit.as_object(), None)?;
    repo.set_head_detached(onto_commit.id())?;

    let mut state = RebaseState {
        plan,
        cursor: 0,
        original_branch_ref,
        original_tip,
        group_start_parent: None,
    };

    let result = advance(repo, &mut state)?;
    Ok((state, result))
}

pub fn rebase_continue(
    repo: &Repository,
    state: &mut RebaseState,
) -> Result<RebaseStepResult, RebaseError> {
    // The working index is presumed ready — either a conflict was just resolved, or an `Edit`
    // pause's amendment is staged. Land it as this step's commit, then keep advancing.
    land_current_step(repo, state)?;
    state.cursor += 1;
    advance(repo, state)
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

        repo.cherrypick(&commit, None)?;

        if repo.index()?.has_conflicts() {
            return Ok(RebaseStepResult::Conflicted {
                files: conflicted_paths(repo)?,
            });
        }

        if matches!(entry.action, RebaseAction::Edit) {
            return Ok(RebaseStepResult::PausedForEdit);
        }

        land_current_step(repo, state)?;
        state.cursor += 1;
    }
}

fn starts_a_group(plan: &[RebasePlanEntry], index: usize) -> bool {
    matches!(
        plan.get(index + 1).map(|e| &e.action),
        Some(RebaseAction::Squash) | Some(RebaseAction::Fixup)
    )
}

fn ends_a_group(plan: &[RebasePlanEntry], index: usize) -> bool {
    matches!(
        plan[index].action,
        RebaseAction::Squash | RebaseAction::Fixup
    ) && !matches!(
        plan.get(index + 1).map(|e| &e.action),
        Some(RebaseAction::Squash) | Some(RebaseAction::Fixup)
    )
}

/// Commits the currently-staged result for `plan[state.cursor]` as a real intermediate commit
/// (parent = current `HEAD`, author preserved from the original commit, committer = the current
/// user) — then, if this entry ends a squash/fixup group, collapses the whole group's chain of
/// intermediate commits into one final commit reparented onto `group_start_parent`, carrying the
/// group leader's `combined_message`. The leader is found by walking backward from this entry to
/// the nearest preceding entry that isn't itself `Squash`/`Fixup` — guaranteed to exist, since
/// `validate_plan` rejects a plan whose very first entry is `Squash`/`Fixup`.
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

    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let parent = repo.head()?.peel_to_commit()?;
    let committer = repo.signature()?;
    repo.commit(
        Some("HEAD"),
        &original_commit.author(),
        &committer,
        &message,
        &tree,
        &[&parent],
    )?;

    if ends_a_group(&state.plan, state.cursor) {
        let leader_index = (0..=state.cursor)
            .rev()
            .find(|&i| {
                !matches!(
                    state.plan[i].action,
                    RebaseAction::Squash | RebaseAction::Fixup
                )
            })
            .expect("validate_plan guarantees a non-squash/fixup leader precedes any group");
        let combined_message = state.plan[leader_index]
            .combined_message
            .clone()
            .unwrap_or_else(|| message.clone());

        let final_tree = repo.head()?.peel_to_commit()?.tree()?;
        let group_parent = repo.find_commit(
            state
                .group_start_parent
                .expect("ends_a_group implies a group was started"),
        )?;
        // Not `update_ref: Some("HEAD")` here: libgit2 rejects a ref-updating commit() whose
        // first parent isn't the ref's *current* target, and the current target is the last
        // intermediate commit in the group's chain, not `group_parent` — that's the whole point
        // of the collapse (reparenting past that chain). Create the commit untethered, then move
        // HEAD to it directly.
        let final_oid = repo.commit(
            None,
            &original_commit.author(),
            &committer,
            &combined_message,
            &final_tree,
            &[&group_parent],
        )?;
        repo.set_head_detached(final_oid)?;
        state.group_start_parent = None;
    }

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

fn finish(repo: &Repository, state: &RebaseState) -> Result<RebaseStepResult, RebaseError> {
    let final_oid = repo.head()?.peel_to_commit()?.id();
    let mut branch_ref = repo.find_reference(&state.original_branch_ref)?;
    branch_ref.set_target(final_oid, "rebase (finish): returning to branch")?;
    repo.set_head(&state.original_branch_ref)?;
    // Clears CHERRY_PICK_HEAD and any other in-progress-operation state left behind by the
    // `cherrypick()` calls in `advance` — without this, `repo.state()` stays `CherryPick` even
    // though the rebase (from this module's own state machine's point of view) is done.
    repo.cleanup_state()?;
    Ok(RebaseStepResult::Done)
}
