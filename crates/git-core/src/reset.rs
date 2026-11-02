use git2::{Oid, Repository, ResetType};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

#[derive(Debug, Error)]
pub enum ResetError {
    #[error("{0}")]
    Git(#[from] git2::Error),
    #[error("Finish or abort the current merge/cherry-pick/revert/rebase first.")]
    OperationInProgress,
    /// A `git rebase` or `git am` started in a terminal isn't finished (#263).
    #[error(transparent)]
    TerminalRebase(#[from] crate::repo::TerminalRebaseInProgress),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    CaseCollision(#[from] crate::CaseCollision),
}

/// Moves the current branch (or detached HEAD) to `commit_id`. Soft keeps index + working tree,
/// Mixed resets the index only, Hard resets both — untracked files are never removed (same as
/// `git reset --hard`).
///
/// Runs during a terminal `git bisect` and keeps it, as git does (#250). libgit2's mixed and
/// hard `git_reset` end with `cleanup_state`, which would delete `BISECT_LOG` (and a terminal
/// rebase's or cherry-pick list's state), so those two are done here step by step, as libgit2
/// does them, with `commit::clear_operation_files` instead (#249).
pub fn reset_to(repo: &Repository, commit_id: &str, mode: ResetMode) -> Result<(), ResetError> {
    crate::repo::check_no_terminal_rebase(repo)?;
    if !crate::repo::no_operation(repo) {
        return Err(ResetError::OperationInProgress);
    }
    let target = repo.find_commit(Oid::from_str(commit_id)?)?;
    if mode == ResetMode::Soft {
        // A soft reset only moves HEAD; libgit2 cleans no state for it.
        repo.reset(target.as_object(), ResetType::Soft, None)?;
    } else {
        let tree = target.tree()?;
        if mode == ResetMode::Hard {
            // A forced checkout writes (and deletes) through a symlink that replaced a tracked
            // file or folder; remove such links first, like git (#114).
            crate::worktree_guard::prepare_forced_checkout::<ResetError, &str>(
                repo,
                Some(&crate::worktree_guard::GuardSource::Tree(&tree)),
                None,
            )?;
            let mut checkout = git2::build::CheckoutBuilder::new();
            checkout.force();
            repo.checkout_tree(tree.as_object(), Some(&mut checkout))?;
        }
        move_head(repo, target.id())?;
        let mut index = repo.index()?;
        index.read_tree(&tree)?;
        index.write()?;
        crate::commit::clear_operation_files(repo)?;
    }
    // Like git's `remove_branch_state`: a reset abandons a pending squash merge's message.
    crate::merge::clear_squash_message(repo)?;
    Ok(())
}

/// Points the ref HEAD ends at (a branch, or HEAD itself when detached) at `target`, with git's
/// reflog message, as libgit2's `git_reset` (`git_reference__update_terminal`) does. The
/// symbolic chain is followed to its end, so a branch that is a symbolic ref to another
/// (`git symbolic-ref refs/heads/alias refs/heads/main`) stays an alias and the branch it names
/// moves; libgit2 writes HEAD's reflog too when it ends at that branch.
fn move_head(repo: &Repository, target: Oid) -> Result<(), git2::Error> {
    // git's and libgit2's limit on nested symbolic refs.
    const MAX_NESTING: usize = 5;
    let message = format!("reset: moving to {target}");
    let mut name = "HEAD".to_string();
    for _ in 0..=MAX_NESTING {
        let next = match repo.find_reference(&name) {
            Ok(reference) => reference.symbolic_target()?.map(str::to_string),
            // An unborn branch: create it, as libgit2 does.
            Err(error) if error.code() == git2::ErrorCode::NotFound => None,
            Err(error) => return Err(error),
        };
        match next {
            Some(next) => name = next,
            None => {
                repo.reference(&name, target, true, &message)?;
                return Ok(());
            }
        }
    }
    Err(git2::Error::from_str(&format!(
        "too many nested symbolic refs from HEAD to {name}"
    )))
}
