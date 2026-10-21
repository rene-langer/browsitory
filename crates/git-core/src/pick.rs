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
    #[error("Cherry-picking or reverting merge commits isn't supported yet.")]
    MergeCommitUnsupported,
    #[error("There is no commit on this branch yet.")]
    UnbornHead,
    #[error("Nothing to apply — those changes are already on this branch.")]
    EmptyResult,
    #[error("{0}")]
    Sign(#[from] crate::signing::SignError),
}

#[derive(Clone, Copy)]
enum Kind {
    CherryPick,
    Revert,
}

/// Applies `commit_id` on top of HEAD. A clean result is committed immediately with the original
/// message and author; a conflict leaves the repo in `RepositoryState::CherryPick` with
/// `MERGE_MSG` written, for the existing conflict pane + `commit::commit` / `merge::abort_merge`
/// to finish.
pub fn cherry_pick(repo: &Repository, commit_id: &str) -> Result<PickOutcome, PickError> {
    apply(repo, commit_id, Kind::CherryPick)
}

/// Commits the inverse of `commit_id` on top of HEAD. Same conflict contract as `cherry_pick`
/// (state `RepositoryState::Revert`).
pub fn revert(repo: &Repository, commit_id: &str) -> Result<PickOutcome, PickError> {
    apply(repo, commit_id, Kind::Revert)
}

fn apply(repo: &Repository, commit_id: &str, kind: Kind) -> Result<PickOutcome, PickError> {
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
    let target = repo.find_commit(Oid::from_str(commit_id)?)?;
    if target.parent_count() > 1 {
        return Err(PickError::MergeCommitUnsupported);
    }
    let staged = repo.diff_tree_to_index(Some(&head.tree()?), None, None)?;
    if staged.deltas().len() > 0 {
        return Err(PickError::DirtyIndex);
    }

    // Validate the signing configuration before libgit2 stages anything, so a config error
    // (unsupported gpg.format, missing SSH key) leaves the repository completely untouched.
    // A signer *program* failure can only surface at commit time, after the pick is staged;
    // that leaves the pick in progress (CHERRY_PICK_HEAD/REVERT_HEAD, like git) so the user can
    // commit again or abort. HEAD and refs never move on failure.
    crate::signing::signing_config(repo)?;

    // libgit2's default checkout strategy here is SAFE: it errors rather than overwrite a
    // modified working-tree file the pick needs to change. libgit2 may already have written
    // CHERRY_PICK_HEAD/REVERT_HEAD + MERGE_MSG by then; state was Clean on entry (checked above),
    // so clearing those files restores exactly the prior state without touching user files.
    let applied = match kind {
        Kind::CherryPick => repo.cherrypick(&target, None),
        Kind::Revert => repo.revert(&target, None),
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
        return Ok(PickOutcome::Conflicted { files });
    }

    let tree_id = index.write_tree()?;
    if tree_id == head.tree_id() {
        repo.cleanup_state()?;
        return Err(PickError::EmptyResult);
    }
    let tree = repo.find_tree(tree_id)?;
    let committer = repo.signature()?;
    let (author, message) = match kind {
        Kind::CherryPick => (
            target.author().to_owned(),
            String::from_utf8_lossy(target.message_bytes()).into_owned(),
        ),
        Kind::Revert => (committer.clone(), revert_message(&target)),
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
        &[&head],
    )?;
    repo.cleanup_state()?;
    Ok(PickOutcome::Committed {
        commit_id: oid.to_string(),
    })
}

fn revert_message(target: &Commit<'_>) -> String {
    format!(
        "Revert \"{}\"\n\nThis reverts commit {}.\n",
        target.summary().ok().flatten().unwrap_or(""),
        target.id()
    )
}
