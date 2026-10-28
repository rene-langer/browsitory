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
    #[error("This is a merge commit. Choose which parent to apply it against.")]
    MainlineRequired,
    #[error("Parent {mainline} doesn't exist on this commit.")]
    InvalidMainline { mainline: u32 },
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

/// What one pick/revert step did.
enum StepResult {
    Committed(Oid),
    /// The result equals HEAD's tree; the operation state was cleaned up, nothing committed.
    Empty,
    Conflicted(Vec<String>),
}

/// Applies `commit_id` on top of HEAD. A clean result is committed immediately with the original
/// message and author; a conflict leaves the repo in `RepositoryState::CherryPick` with
/// `MERGE_MSG` written, for the existing conflict pane + `commit::commit` / `merge::abort_merge`
/// to finish. A merge commit needs `mainline` (1-based parent number, like `git cherry-pick -m`);
/// any other commit must not get one.
pub fn cherry_pick(
    repo: &Repository,
    commit_id: &str,
    mainline: Option<u32>,
) -> Result<PickOutcome, PickError> {
    apply(repo, commit_id, Kind::CherryPick, mainline)
}

/// Commits the inverse of `commit_id` on top of HEAD. Same conflict and `mainline` contract as
/// `cherry_pick` (state `RepositoryState::Revert`).
pub fn revert(
    repo: &Repository,
    commit_id: &str,
    mainline: Option<u32>,
) -> Result<PickOutcome, PickError> {
    apply(repo, commit_id, Kind::Revert, mainline)
}

fn apply(
    repo: &Repository,
    commit_id: &str,
    kind: Kind,
    mainline: Option<u32>,
) -> Result<PickOutcome, PickError> {
    let head = check_ready(repo)?;
    let target = repo.find_commit(Oid::from_str(commit_id)?)?;
    check_mainline(&target, mainline)?;
    check_signing(repo)?;
    match step(repo, &head, &target, kind, mainline)? {
        StepResult::Committed(oid) => Ok(PickOutcome::Committed {
            commit_id: oid.to_string(),
        }),
        StepResult::Empty => Err(PickError::EmptyResult),
        StepResult::Conflicted(files) => Ok(PickOutcome::Conflicted { files }),
    }
}

/// The preconditions every pick shares, in order: nothing paused, a born HEAD, nothing staged.
/// Returns HEAD's commit.
fn check_ready(repo: &Repository) -> Result<Commit<'_>, PickError> {
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
    let staged = repo.diff_tree_to_index(Some(&head.tree()?), None, None)?;
    if staged.deltas().len() > 0 {
        return Err(PickError::DirtyIndex);
    }
    Ok(head)
}

/// Validate the signing configuration before libgit2 stages anything, so a config error
/// (unsupported gpg.format, missing SSH key) leaves the repository completely untouched.
/// A signer *program* failure can only surface at commit time, after the pick is staged;
/// that leaves the pick in progress (CHERRY_PICK_HEAD/REVERT_HEAD, like git) so the user can
/// commit again or abort. HEAD and refs never move on failure.
fn check_signing(repo: &Repository) -> Result<(), PickError> {
    crate::signing::signing_config(repo)?;
    Ok(())
}

fn check_mainline(target: &Commit<'_>, mainline: Option<u32>) -> Result<(), PickError> {
    let parents = target.parent_count();
    match mainline {
        None if parents > 1 => Err(PickError::MainlineRequired),
        Some(m) if parents <= 1 || m == 0 || m as usize > parents => {
            Err(PickError::InvalidMainline { mainline: m })
        }
        _ => Ok(()),
    }
}

/// Applies `target` on top of `head` (HEAD's commit) and commits a clean, non-empty result.
/// Callers have run `check_ready`, `check_mainline` and `check_signing`.
fn step(
    repo: &Repository,
    head: &Commit<'_>,
    target: &Commit<'_>,
    kind: Kind,
    mainline: Option<u32>,
) -> Result<StepResult, PickError> {
    // libgit2's default checkout strategy here is SAFE: it errors rather than overwrite a
    // modified working-tree file the pick needs to change. libgit2 may already have written
    // CHERRY_PICK_HEAD/REVERT_HEAD + MERGE_MSG by then; state was Clean on entry (checked by
    // the caller), so clearing those files restores exactly the prior state without touching
    // user files.
    let applied = match kind {
        Kind::CherryPick => {
            let mut options = git2::CherrypickOptions::new();
            if let Some(mainline) = mainline {
                options.mainline(mainline);
            }
            repo.cherrypick(target, Some(&mut options))
        }
        Kind::Revert => {
            let mut options = git2::RevertOptions::new();
            if let Some(mainline) = mainline {
                options.mainline(mainline);
            }
            repo.revert(target, Some(&mut options))
        }
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
        return Ok(StepResult::Conflicted(files));
    }

    let tree_id = index.write_tree()?;
    if tree_id == head.tree_id() {
        repo.cleanup_state()?;
        return Ok(StepResult::Empty);
    }
    let tree = repo.find_tree(tree_id)?;
    let committer = repo.signature()?;
    let (author, message) = match kind {
        Kind::CherryPick => (
            target.author().to_owned(),
            String::from_utf8_lossy(target.message_bytes()).into_owned(),
        ),
        Kind::Revert => (committer.clone(), revert_message(target, mainline)),
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
        &[head],
    )?;
    repo.cleanup_state()?;
    Ok(StepResult::Committed(oid))
}

/// git's revert message; a mainline revert names the parent it keeps, like `git revert -m`.
fn revert_message(target: &Commit<'_>, mainline: Option<u32>) -> String {
    let reversing = mainline
        .and_then(|m| target.parent_id(m as usize - 1).ok())
        .map(|parent| format!(", reversing\nchanges made to {parent}"))
        .unwrap_or_default();
    format!(
        "Revert \"{}\"\n\nThis reverts commit {}{reversing}.\n",
        target.summary().ok().flatten().unwrap_or(""),
        target.id()
    )
}
