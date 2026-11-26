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
    /// `core.ignorecase` is on and the checkout includes two paths that differ only in case
    /// (#403); nothing was changed.
    #[error(transparent)]
    CasePair(#[from] crate::CasePair),
    /// `core.ignorecase` can't be read, so the case-collision check can't run (#225).
    #[error(transparent)]
    CaseConfig(#[from] crate::CaseConfigUnreadable),
    /// Windows: a checkout stopped at a file another program holds open (#303).
    #[error(transparent)]
    HeldOpen(#[from] crate::HeldOpen),
    /// A file to write uses an external filter driver (#1063); nothing changed.
    #[error(transparent)]
    ExternalFilter(#[from] crate::ExternalFilter),
    /// A hard reset would overwrite the local changes of a skip-worktree file, which the status
    /// hides and Undo can't bring back; git refuses too ("not uptodate"). Nothing changed (#1145).
    #[error("'{0}' has local changes git doesn't show (it is marked skip-worktree), and the reset would overwrite them. Nothing was changed.")]
    SkipWorktreeEdited(String),
    /// A setting written without a value that libgit2 would crash on (#1182); nothing
    /// changed.
    #[error(transparent)]
    ValuelessSetting(#[from] crate::ValuelessSetting),
}

/// Moves the current branch (or detached HEAD) to `commit_id`. Soft keeps index + working tree,
/// Mixed resets the index only, Hard resets both — untracked files are never removed (same as
/// `git reset --hard`).
///
/// Runs during a terminal `git bisect` and keeps it, as git does (#250). libgit2's mixed and
/// hard `git_reset` end with `cleanup_state`, which would delete `BISECT_LOG` (and a terminal
/// rebase's or cherry-pick list's state), so those two are done here step by step, as libgit2
/// does them, with `commit::clear_operation_files` instead (#249). Unlike libgit2, which moves
/// HEAD before it writes the index, the index is written first, as `git reset` does (#796).
pub fn reset_to(repo: &Repository, commit_id: &str, mode: ResetMode) -> Result<(), ResetError> {
    if mode == ResetMode::Hard {
        crate::valueless_setting::refuse_for_checkout::<ResetError>(repo)?;
    }
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
        let mut index = repo.index()?;
        index.read(false)?;
        // Read before the checkout, which rewrites the entries it writes without the mark.
        let marks = crate::index_marks::marks_after(repo, &index, &tree)?;
        if mode == ResetMode::Hard {
            let left = crate::index_marks::left_alone::<ResetError>(repo, &index, &tree, &marks)?;
            if let Some(name) = left.edited {
                return Err(ResetError::SkipWorktreeEdited(name));
            }
            let paths = if left.paths.is_empty() {
                None
            } else {
                Some(crate::index_marks::paths_except(repo, &tree, |path| {
                    left.paths.contains(path)
                })?)
            };
            // No paths would mean the whole tree.
            if paths.as_ref().is_none_or(|paths| !paths.is_empty()) {
                check_out(repo, &tree, paths.as_deref())?;
            }
        }
        // git's order: the index first, the ref last (#796). Moving HEAD first left a window in
        // which HEAD named the target while the index still held the old tree, and a reset whose
        // index write failed (another command's `index.lock`) moved HEAD anyway.
        crate::index_marks::reset_index_to_tree(&mut index, &tree, &marks.skip_worktree)?;
        move_head(repo, target.id())?;
        crate::commit::clear_operation_files(repo)?;
    }
    // Like git's `remove_branch_state`: a reset abandons a pending squash merge's message.
    crate::merge::clear_squash_message(repo)?;
    Ok(())
}

/// The hard reset's forced checkout of `tree`, limited to the literal `paths` when given.
fn check_out(
    repo: &Repository,
    tree: &git2::Tree<'_>,
    paths: Option<&[Vec<u8>]>,
) -> Result<(), ResetError> {
    // A forced checkout writes (and deletes) through a symlink that replaced a tracked file or
    // folder; remove such links first, like git (#114).
    crate::worktree_guard::prepare_forced_checkout::<ResetError, Vec<u8>>(
        repo,
        Some(&crate::worktree_guard::GuardSource::Tree(tree)),
        paths,
    )?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    if let Some(paths) = paths {
        checkout.disable_pathspec_match(true);
        for path in paths {
            checkout.path(crate::worktree_guard::path_from_bytes(path));
        }
    }
    let filters = crate::hardlink::on_checkout(repo, &mut checkout);
    // A file another program holds open: say so, not libgit2's raw error (#303).
    repo.checkout_tree(tree.as_object(), Some(&mut checkout))
        .map_err(crate::worktree_guard::held_open::<ResetError>(
            repo.workdir(),
            crate::HeldDuring::Reset,
        ))?;
    filters.check()?;
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
