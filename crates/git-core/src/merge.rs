use git2::build::CheckoutBuilder;
use git2::{AnnotatedCommit, Repository};

use crate::conflict::conflicted_paths;
use crate::repo::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeOutcome {
    /// `their_branch` is already reachable from HEAD; nothing to do.
    UpToDate,
    /// HEAD was simply moved forward to `their_branch` (no merge commit).
    FastForward,
    /// A real (non-fast-forward) merge completed without conflicts. The
    /// result is staged in the index but not yet committed — the caller
    /// still needs to call `commit::create_commit`, which picks up the
    /// second parent from `MERGE_HEAD` automatically.
    Merged,
    /// The merge produced conflicts. `MERGE_HEAD` and the conflicted index
    /// are left in place (not aborted) so the UI can walk `conflict::conflicted_paths`,
    /// let the user resolve each one via `stage::stage_path`, and then commit.
    Conflict(Vec<String>),
}

/// Merges `their_branch` (a branch name, local or remote-tracking, e.g.
/// `"feature"` or `"origin/main"`) into HEAD.
///
/// Short-circuits through `merge_analysis` first so a fast-forward is just a
/// ref move + checkout (not a real three-way `repo.merge()` call, which would
/// needlessly create a merge commit-shaped state for a case that doesn't need
/// one). Falls back to `repo.merge()` for genuine three-way merges.
pub fn merge_branch(repo: &Repository, their_branch: &str) -> Result<MergeOutcome> {
    let their = resolve_annotated_commit(repo, their_branch)?;
    let (analysis, _preference) = repo.merge_analysis(&[&their])?;

    if analysis.is_up_to_date() {
        return Ok(MergeOutcome::UpToDate);
    }

    if analysis.is_fast_forward() {
        fast_forward(repo, &their)?;
        return Ok(MergeOutcome::FastForward);
    }

    repo.merge(&[&their], None, None)?;

    let index = repo.index()?;
    if index.has_conflicts() {
        Ok(MergeOutcome::Conflict(conflicted_paths(repo)?))
    } else {
        Ok(MergeOutcome::Merged)
    }
}

/// Cancels an in-progress merge (conflicted or not-yet-committed clean
/// merge), restoring the working tree and index to their pre-merge state.
///
/// `cleanup_state` alone only clears `MERGE_HEAD`/`MERGE_MSG` metadata — it
/// does not touch the working tree or index, so a forced checkout back to
/// HEAD's tree is required too, otherwise partially-merged/conflicted content
/// would be left sitting in the working tree after "aborting".
pub fn abort_merge(repo: &Repository) -> Result<()> {
    repo.cleanup_state()?;
    let mut checkout = CheckoutBuilder::new();
    checkout.force();
    repo.checkout_head(Some(&mut checkout))?;
    Ok(())
}

fn resolve_annotated_commit<'repo>(
    repo: &'repo Repository,
    branch: &str,
) -> Result<AnnotatedCommit<'repo>> {
    let reference = repo.resolve_reference_from_short_name(branch)?;
    Ok(repo.reference_to_annotated_commit(&reference)?)
}

/// A fast-forward is a plain ref move (no merge commit): the current branch
/// ref is pointed straight at `their`'s commit, then HEAD/working tree are
/// checked out to match.
fn fast_forward(repo: &Repository, their: &AnnotatedCommit<'_>) -> Result<()> {
    let mut head_ref = repo.head()?;
    let ref_name = head_ref.name()?.to_string();

    head_ref.set_target(their.id(), "fast-forward merge")?;
    repo.set_head(&ref_name)?;

    let mut checkout = CheckoutBuilder::new();
    checkout.force();
    repo.checkout_head(Some(&mut checkout))?;
    Ok(())
}
