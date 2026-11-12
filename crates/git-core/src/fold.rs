//! Fold staged changes into an older commit (#883), like `git commit --fixup <target>` and
//! `git rebase --autosquash` in one step: the staged changes become part of `target`, which keeps
//! its message and author; the commits after it are replayed.
use crate::hooks::HookOptions;
use crate::rebase::{RebaseError, RebaseState, RebaseStepResult};
use git2::Repository;

/// Folds the staged changes into `target`, a commit of the current branch's first-parent
/// history. Returns the paused rebase (`None` once done) and the step result.
pub fn fold_staged_into(
    repo: &Repository,
    target: &str,
    hooks: HookOptions<'_>,
) -> Result<(Option<RebaseState>, RebaseStepResult), RebaseError> {
    let _ = (repo, target, hooks);
    Err(RebaseError::InvalidPlan("not implemented".to_string()))
}
