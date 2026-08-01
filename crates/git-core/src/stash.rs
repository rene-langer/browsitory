//! Stash operations.
//!
//! Every other module in this crate takes `&Repository` (see CLAUDE.md's
//! "git-core is dependency-injected on purpose" section) so the crate stays
//! unit-testable against real temp-dir repos without a module-level
//! singleton. This module is the sole, deliberate exception: `git2`'s
//! `stash_*` methods (`Repository::stash_save2`, `stash_apply`, `stash_pop`,
//! `stash_drop`, `stash_foreach`) all require `&mut Repository` because
//! libgit2 needs exclusive access to the repository while it manipulates the
//! stash's underlying commit graph and reflog. Callers (the worker thread in
//! `crates/app/src/worker.rs`) already own their `Repository` handle
//! exclusively, so this is a non-issue in practice — just worth calling out
//! so it reads as deliberate, not an inconsistency with the rest of the
//! crate.

use git2::{Oid, Repository, StashFlags};

use crate::repo::Result;

#[derive(Debug, Clone)]
pub struct StashEntry {
    pub index: usize,
    pub message: String,
}

/// Lists all stash entries, newest (index 0) first — matches `git stash
/// list`'s ordering, which is also the order `git2::Repository::stash_foreach`
/// iterates in.
pub fn list_stashes(repo: &mut Repository) -> Result<Vec<StashEntry>> {
    let mut out = Vec::new();
    // `stash_foreach` already hands us index/message/oid as separate,
    // pre-parsed fields — no string parsing needed to pull the message out
    // of some combined "WIP on branch: message" form.
    repo.stash_foreach(|index, message, _oid| {
        out.push(StashEntry {
            index,
            message: message.to_string(),
        });
        true
    })?;
    Ok(out)
}

/// Stashes the current working-tree and index changes.
///
/// Defaults to `StashFlags::INCLUDE_UNTRACKED`: most users expect "stash my
/// changes" to sweep up untracked files too, not just tracked
/// modifications — matching `git stash -u` rather than plain `git stash`.
pub fn create_stash(repo: &mut Repository, message: Option<&str>) -> Result<Oid> {
    let signature = repo.signature()?;
    let oid = repo.stash_save2(&signature, message, Some(StashFlags::INCLUDE_UNTRACKED))?;
    Ok(oid)
}

/// Applies (without removing) the stash entry at `index` to the working
/// tree and index.
pub fn apply_stash(repo: &mut Repository, index: usize) -> Result<()> {
    repo.stash_apply(index, None)?;
    Ok(())
}

/// Applies the stash entry at `index` and removes it from the stash list if
/// the apply succeeds.
pub fn pop_stash(repo: &mut Repository, index: usize) -> Result<()> {
    repo.stash_pop(index, None)?;
    Ok(())
}

/// Removes the stash entry at `index` from the stash list without applying
/// it.
pub fn drop_stash(repo: &mut Repository, index: usize) -> Result<()> {
    repo.stash_drop(index)?;
    Ok(())
}
