use thiserror::Error;

#[derive(Debug, Error)]
pub enum CommitError {
    #[error("failed to create commit: {0}")]
    Write(#[from] git2::Error),
    #[error("There is no commit to amend yet.")]
    NothingToAmend,
    #[error("Finish or abort the current merge/cherry-pick/revert/rebase first.")]
    OperationInProgress,
}

/// Commits the current index. Author/committer come from `repo.signature()` (reads
/// `user.name`/`user.email` from git config — returns a clear `git2::Error` if unset).
/// Parent is the current `HEAD` commit if one exists, or no parents for a repo's first commit —
/// plus, if a merge is in progress (`RepositoryState::Merge`), `MERGE_HEAD`'s commit(s) as
/// additional parents, matching real `git commit`'s own behavior of producing a merge commit
/// once conflicts (if any) are resolved and staged. `repo` needs to be `&mut` only because
/// reading `MERGE_HEAD` via `mergehead_foreach` requires it — no other part of this function
/// does.
pub fn commit(repo: &mut git2::Repository, message: &str) -> Result<String, CommitError> {
    // `mergehead_foreach` needs `&mut repo`, so it must run — and finish — before anything
    // below takes an immutable borrow that could outlive it. `git2::Tree` (bound below) has a
    // `Drop` impl, which under NLL's drop-check extends its immutable borrow of `repo` to the
    // end of this function's scope, not just its last use — so it (and everything borrowed
    // alongside it) is confined to the block below, ending before `cleanup_state` needs `&mut`
    // again.
    let is_merging = repo.state() == git2::RepositoryState::Merge;
    let mut merge_parent_ids = Vec::new();
    if is_merging {
        repo.mergehead_foreach(|oid| {
            merge_parent_ids.push(*oid);
            true
        })?;
    }

    let oid = {
        let mut index = repo.index()?;
        let tree_id = index.write_tree()?;
        let tree = repo.find_tree(tree_id)?;
        let signature = repo.signature()?;

        let head_parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let merge_parents: Vec<git2::Commit> = merge_parent_ids
            .iter()
            .filter_map(|oid| repo.find_commit(*oid).ok())
            .collect();

        let mut parents: Vec<&git2::Commit> = head_parent.iter().collect();
        parents.extend(merge_parents.iter());

        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )?
    };

    // Cherry-pick/revert leave CHERRY_PICK_HEAD/REVERT_HEAD + MERGE_MSG behind on conflict; the
    // commit that lands the resolved result is single-parent (HEAD only — they are not merges),
    // but the state files must still be cleared or `repo.state()` stays non-Clean.
    let finishes_pick = matches!(
        repo.state(),
        git2::RepositoryState::CherryPick | git2::RepositoryState::Revert
    );
    if is_merging || finishes_pick {
        repo.cleanup_state()?;
    }

    Ok(oid.to_string())
}

/// Rewrites HEAD's commit with the current index tree and `message`, keeping its parents and
/// author; committer becomes `repo.signature()`. Works on a branch (the branch ref moves) or a
/// detached HEAD. Refused while any operation (merge, cherry-pick, revert, rebase) is paused,
/// since amending then would silently drop that operation's extra state.
pub fn amend_head(repo: &git2::Repository, message: &str) -> Result<String, CommitError> {
    if repo.state() != git2::RepositoryState::Clean {
        return Err(CommitError::OperationInProgress);
    }
    let head = match repo.head() {
        Ok(head) => head.peel_to_commit()?,
        Err(error) if error.code() == git2::ErrorCode::UnbornBranch => {
            return Err(CommitError::NothingToAmend)
        }
        Err(error) => return Err(error.into()),
    };
    let tree_id = repo.index()?.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let committer = repo.signature()?;
    let oid = head.amend(
        Some("HEAD"),
        None,
        Some(&committer),
        None,
        Some(message),
        Some(&tree),
    )?;
    Ok(oid.to_string())
}
