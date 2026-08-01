use git2::{Oid, Repository, RepositoryState};

use crate::repo::Result;

/// Commits the current index contents onto HEAD (or as the first commit, on
/// an unborn HEAD). Requires `user.name`/`user.email` to be set in the
/// repository or global git config, same as the real `git commit`.
///
/// If a merge is in progress (`MERGE_HEAD` is set — i.e. `merge::merge_branch`
/// returned a conflict that's now resolved, or a clean merge that hasn't been
/// committed yet), the resulting commit gets a second parent from
/// `MERGE_HEAD`, same as plain `git commit` does after a conflicted merge.
///
/// Takes `&mut Repository` (not `&Repository` like the rest of `git-core`)
/// solely because `Repository::mergehead_foreach` — the only way to read
/// `MERGE_HEAD` — requires `&mut self` in git2, even though it doesn't
/// actually mutate anything; a `&mut Repository` reborrows fine as `&Repository`
/// at call sites that don't otherwise need mutability.
pub fn create_commit(repo: &mut Repository, message: &str) -> Result<Oid> {
    // Read MERGE_HEAD (the only thing that needs `&mut self`) before taking
    // any of the immutable borrows below (`tree`, `head_commit`, ...) — those
    // borrow types (`Tree<'repo>`, `Commit<'repo>`) hold a real lifetime tie
    // to `repo`, unlike `git_core::Rebase`'s phantom one, so they can't
    // coexist with a `&mut self` call once taken.
    //
    // `mergehead_foreach` errors outright (rather than simply not invoking
    // the callback) when `MERGE_HEAD` doesn't exist — found by actually
    // running this against a real repo with no merge in progress, which is
    // the common case for every plain `git commit`. Only call it when
    // `repo.state()` says a merge is actually in progress.
    let mut merge_head_ids = Vec::new();
    if repo.state() == RepositoryState::Merge {
        repo.mergehead_foreach(|oid| {
            merge_head_ids.push(*oid);
            true
        })?;
    }

    let mut index = repo.index()?;
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let signature = repo.signature()?;

    let head_commit = match repo.head() {
        Ok(head) => Some(head.peel_to_commit()?),
        Err(_) => None,
    };

    let merge_commits = merge_head_ids
        .iter()
        .map(|oid| repo.find_commit(*oid))
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut parents: Vec<&git2::Commit> = head_commit.iter().collect();
    parents.extend(merge_commits.iter());

    let oid = repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parents,
    )?;

    if !merge_head_ids.is_empty() {
        repo.cleanup_state()?;
    }

    Ok(oid)
}
