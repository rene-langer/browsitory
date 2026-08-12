use git2::{Repository, StashFlags};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StashError {
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashEntry {
    pub index: usize,
    pub message: String,
    pub commit_id: String,
}

pub fn save_stash(repo: &mut Repository) -> Result<(), StashError> {
    let signature = repo.signature()?;
    // `stash_save2` (not `stash_save`) because it's the only variant that accepts a `None`
    // message, letting libgit2 generate its own default (e.g. "WIP on main: <short-id>
    // <summary>"). Untracked files are always included — no flag to opt out, matching this
    // project's fixed default (see the design spec's decision).
    repo.stash_save2(&signature, None, Some(StashFlags::INCLUDE_UNTRACKED))?;
    Ok(())
}

pub fn list_stashes(repo: &mut Repository) -> Result<Vec<StashEntry>, StashError> {
    let mut entries = Vec::new();
    // `stash_foreach` requires `&mut self` despite being read-only — a libgit2 API constraint,
    // not a design choice (see the design spec's "&mut Repository wrinkle" note).
    repo.stash_foreach(|index, message, oid| {
        entries.push(StashEntry {
            index,
            message: message.to_string(),
            commit_id: oid.to_string(),
        });
        true
    })?;
    Ok(entries)
}

pub fn apply_stash(repo: &mut Repository, index: usize) -> Result<(), StashError> {
    // `None` options means libgit2's default (safe) checkout strategy applies, refusing to
    // overwrite a conflicting file exactly like `branch::switch_branch`'s
    // `checkout_tree(..., None)` does. Same philosophy, same error-propagation shape.
    repo.stash_apply(index, None)?;
    Ok(())
}

pub fn drop_stash(repo: &mut Repository, index: usize) -> Result<(), StashError> {
    // Indices shift after a drop (git2's own behavior) — callers always re-fetch the full list
    // afterward, so a stale index is never held across calls.
    repo.stash_drop(index)?;
    Ok(())
}
