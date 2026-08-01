use git2::{BranchType, Oid, Repository};

use crate::repo::Result;

#[derive(Debug, Clone)]
pub struct BranchInfo {
    pub name: String,
    pub is_head: bool,
    pub upstream: Option<String>,
}

/// Lists local branches, newest-`git2`-iteration-order (not sorted further),
/// each annotated with whether it's the currently checked-out branch and its
/// upstream tracking branch's shorthand name, if any.
pub fn list_branches(repo: &Repository) -> Result<Vec<BranchInfo>> {
    let mut out = Vec::new();
    for branch in repo.branches(Some(BranchType::Local))? {
        let (branch, _branch_type) = branch?;
        // `Reference::shorthand()` returns `Result<&str, Error>`, not
        // `Option<&str>` (same footgun class as `Commit::summary()` /
        // `Signature::name()`/`email()` — see CLAUDE.md's git2 gotchas). It
        // only errs when the name isn't valid UTF-8; skip such branches
        // rather than failing the whole listing.
        let Ok(name) = branch.get().shorthand() else {
            continue;
        };
        let name = name.to_string();
        let is_head = branch.is_head();
        let upstream = branch
            .upstream()
            .ok()
            .and_then(|upstream| upstream.get().shorthand().ok().map(str::to_string));

        out.push(BranchInfo {
            name,
            is_head,
            upstream,
        });
    }
    Ok(out)
}

/// Creates a new branch pointing at `start_point`, or at HEAD's commit when
/// `start_point` is `None`.
pub fn create_branch(repo: &Repository, name: &str, start_point: Option<Oid>) -> Result<()> {
    let commit = match start_point {
        Some(oid) => repo.find_commit(oid)?,
        None => repo.head()?.peel_to_commit()?,
    };
    repo.branch(name, &commit, false)?;
    Ok(())
}

/// Deletes a local branch. Refuses to delete the currently checked-out
/// branch: `git2::Branch::delete()` doesn't guard against this itself and
/// would leave HEAD dangling (pointing at a now-deleted ref), unlike plain
/// `git branch -d`, which refuses with "cannot delete the branch you are on".
pub fn delete_branch(repo: &Repository, name: &str) -> Result<()> {
    if let Ok(head) = repo.head()
        && head.shorthand().ok() == Some(name)
    {
        return Err(git2::Error::from_str("cannot delete the currently checked-out branch").into());
    }
    let mut branch = repo.find_branch(name, BranchType::Local)?;
    branch.delete()?;
    Ok(())
}

/// Renames a local branch, leaving its upstream/tracking configuration
/// (which `git2` carries over automatically) and its tip commit untouched.
pub fn rename_branch(repo: &Repository, old_name: &str, new_name: &str) -> Result<()> {
    let mut branch = repo.find_branch(old_name, BranchType::Local)?;
    branch.rename(new_name, false)?;
    Ok(())
}

/// Switches HEAD to point at branch `name` and checks out its tree into the
/// working directory.
///
/// Checks out the target branch's tree *before* moving `HEAD`, with the
/// default, non-forced `CheckoutBuilder` (libgit2's `GIT_CHECKOUT_SAFE`) so
/// a dirty working tree that would be clobbered by the switch surfaces as a
/// `git2::Error` naturally, rather than needing a redundant pre-check here —
/// and, crucially, so a refused checkout leaves `HEAD` untouched. Doing this
/// in the opposite order (`set_head` then `checkout_head`) would move `HEAD`
/// to the new branch first and only then discover the working tree is dirty,
/// leaving the repository in an inconsistent state (`HEAD` pointing at the
/// new branch, working tree still holding the old branch's uncommitted
/// changes).
///
/// Builds the full `refs/heads/{name}` ref name for `set_head` — a bare
/// shorthand resolves incorrectly (it would be looked up as-is rather than
/// under `refs/heads/`).
pub fn switch_branch(repo: &Repository, name: &str) -> Result<()> {
    let branch = repo.find_branch(name, BranchType::Local)?;
    let commit = branch.get().peel_to_commit()?;
    repo.checkout_tree(commit.as_object(), None)?;

    let ref_name = format!("refs/heads/{name}");
    repo.set_head(&ref_name)?;
    Ok(())
}
