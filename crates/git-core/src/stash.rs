use git2::{ErrorClass, ErrorCode, Repository, StashFlags};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StashError {
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("There are no changes to stash.")]
    NothingToStash,
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// What to stash, like `git stash push [-m <message>] [--keep-index] [-u] [-- <paths>]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashOptions {
    /// `None`, empty or whitespace-only → libgit2's default "WIP on <branch>: …".
    pub message: Option<String>,
    /// Stash staged changes but also leave them staged in the index and worktree.
    pub keep_index: bool,
    pub include_untracked: bool,
    /// Literal file paths limiting the stash; empty stashes everything.
    pub paths: Vec<String>,
}

impl Default for StashOptions {
    fn default() -> Self {
        Self {
            message: None,
            keep_index: false,
            include_untracked: true,
            paths: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashEntry {
    pub index: usize,
    pub message: String,
    pub commit_id: String,
}

/// Saves a stash with `opts`.
///
/// Without paths this is libgit2's `stash_save2`. With paths Browsitory builds the stash itself
/// (`save_path_limited_stash`): libgit2 1.9.6's path-limited stash (`stash_save_ext` with a
/// pathspec) resets the *whole* index and worktree to HEAD afterwards, silently discarding
/// changes to files outside the paths, and git2 0.21's `StashSaveOptions` cannot pass a message.
pub fn save_stash(repo: &mut Repository, opts: &StashOptions) -> Result<(), StashError> {
    let message = opts
        .message
        .as_deref()
        .map(str::trim)
        .filter(|message| !message.is_empty());
    if !opts.paths.is_empty() {
        return save_path_limited_stash(repo, opts, message);
    }
    let signature = repo.signature()?;
    let mut flags = StashFlags::empty();
    if opts.keep_index {
        flags |= StashFlags::KEEP_INDEX;
    }
    if opts.include_untracked {
        flags |= StashFlags::INCLUDE_UNTRACKED;
    }
    // `stash_save2` (not `stash_save`) because it accepts a `None` message, letting libgit2
    // generate its own default ("WIP on <branch>: <short> <summary>").
    repo.stash_save2(&signature, message, Some(flags))
        .map_err(|error| {
            if error.code() == ErrorCode::NotFound && error.class() == ErrorClass::Stash {
                StashError::NothingToStash
            } else {
                StashError::Git(error)
            }
        })?;
    Ok(())
}

/// `git stash push [-m <message>] [--keep-index] [-u] -- <paths>` with `paths` as literal file
/// paths. Writes the same commit shape libgit2 and git do (worktree commit with parents HEAD,
/// index commit and, when untracked files were stashed, an untracked-files commit) and the same
/// messages, pushes it on `refs/stash`, and only then reverts the stashed paths, so a failure
/// part-way never loses anything: the changes are already in the stash.
///
/// - Worktree commit: HEAD's tree with the matched tracked paths' worktree contents.
/// - Index commit: the whole index, like git (so "Restore staged changes" can restage).
/// - Untracked commit (parentless): only the matched untracked files.
///
/// Reverting touches only the matched paths: tracked ones go back to HEAD in index and worktree
/// (to the index version in the worktree with `keep_index`), untracked ones are deleted.
fn save_path_limited_stash(
    repo: &Repository,
    opts: &StashOptions,
    message: Option<&str>,
) -> Result<(), StashError> {
    let workdir = repo
        .workdir()
        .ok_or_else(|| git2::Error::from_str("cannot stash in a bare repository"))?
        .to_path_buf();
    let signature = repo.signature()?;
    let head = repo.head()?;
    let base = head.peel_to_commit()?;
    let branch = if repo.head_detached()? {
        "(no branch)".to_string()
    } else {
        head.shorthand().unwrap_or("(no branch)").to_string()
    };
    let short_id = base
        .as_object()
        .short_id()?
        .as_str()
        .unwrap_or_default()
        .to_string();
    let base_message = format!(
        "{branch}: {short_id} {}",
        base.summary().ok().flatten().unwrap_or_default()
    );

    let mut status_options = git2::StatusOptions::new();
    status_options
        .include_untracked(opts.include_untracked)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .exclude_submodules(true)
        .disable_pathspec_match(true);
    for path in &opts.paths {
        status_options.pathspec(path.as_str());
    }
    let mut tracked = Vec::new();
    let mut untracked = Vec::new();
    for entry in repo.statuses(Some(&mut status_options))?.iter() {
        let status = entry.status();
        let Ok(path) = entry.path() else { continue };
        if status.contains(git2::Status::IGNORED) || path.ends_with('/') {
            continue;
        }
        if status == git2::Status::WT_NEW {
            untracked.push(path.to_string());
        } else {
            tracked.push(path.to_string());
        }
    }
    if tracked.is_empty() && untracked.is_empty() {
        return Err(StashError::NothingToStash);
    }

    let mut index = repo.index()?;
    let base_tree = base.tree()?;
    let index_tree = repo.find_tree(index.write_tree()?)?;
    let index_commit = repo.find_commit(repo.commit(
        None,
        &signature,
        &signature,
        &format!("index on {base_message}\n"),
        &index_tree,
        &[&base],
    )?)?;

    let config = crate::undo::worktree_config(repo).map_err(undo_error)?;
    let untracked_commit = if untracked.is_empty() {
        None
    } else {
        let mut overlay = git2::Index::new()?;
        for path in &untracked {
            if let Some((id, mode)) = worktree_blob(repo, &workdir, path, &config, None)? {
                overlay.add(&crate::undo::index_entry(path, id, mode))?;
            }
        }
        let tree = repo.find_tree(overlay.write_tree_to(repo)?)?;
        Some(repo.find_commit(repo.commit(
            None,
            &signature,
            &signature,
            &format!("untracked files on {base_message}\n"),
            &tree,
            &[],
        )?)?)
    };

    let mut overlay = git2::Index::new()?;
    overlay.read_tree(&base_tree)?;
    for path in &tracked {
        let previous_mode = overlay
            .get_path(std::path::Path::new(path), 0)
            .map(|entry| entry.mode);
        match worktree_blob(repo, &workdir, path, &config, previous_mode)? {
            Some((id, mode)) => overlay.add(&crate::undo::index_entry(path, id, mode))?,
            None => {
                let _ = overlay.remove_path(std::path::Path::new(path));
            }
        }
    }
    let worktree_tree = repo.find_tree(overlay.write_tree_to(repo)?)?;
    let reflog_message = match message {
        Some(message) => format!("On {branch}: {message}"),
        None => format!("WIP on {base_message}"),
    };
    let mut parents = vec![&base, &index_commit];
    if let Some(commit) = &untracked_commit {
        parents.push(commit);
    }
    let stash_id = repo.commit(
        None,
        &signature,
        &signature,
        &format!("{reflog_message}\n"),
        &worktree_tree,
        &parents,
    )?;
    // Without an existing log libgit2 would not write a reflog for `refs/stash` (it is not
    // under refs/heads), and the reflog *is* the stash list.
    repo.reference_ensure_log("refs/stash")?;
    repo.reference("refs/stash", stash_id, true, &reflog_message)?;

    // The stash is saved; now revert exactly the stashed paths.
    if !tracked.is_empty() {
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force().disable_pathspec_match(true);
        for path in &tracked {
            checkout.path(path.as_str());
        }
        if opts.keep_index {
            index.read(true)?;
            repo.checkout_index(Some(&mut index), Some(&mut checkout))?;
        } else {
            repo.checkout_tree(base_tree.as_object(), Some(&mut checkout))?;
        }
    }
    for path in &untracked {
        let full = workdir.join(path);
        std::fs::remove_file(&full)?;
        // Remove directories the stash emptied, like git's clean -d.
        let mut dir = full.parent();
        while let Some(parent) = dir {
            if parent == workdir || std::fs::remove_dir(parent).is_err() {
                break;
            }
            dir = parent.parent();
        }
    }
    Ok(())
}

/// Blob id and mode of a worktree file, with clean filters applied (as `git add` would);
/// `None` when the file is gone.
fn worktree_blob(
    repo: &Repository,
    workdir: &std::path::Path,
    path: &str,
    config: &crate::undo::WorktreeConfig,
    previous_mode: Option<u32>,
) -> Result<Option<(git2::Oid, u32)>, StashError> {
    let Some((bytes, mode)) =
        crate::undo::read_worktree_file(&workdir.join(path), config, previous_mode)?
    else {
        return Ok(None);
    };
    let id = if mode == crate::undo::MODE_LINK {
        repo.blob(&bytes)?
    } else {
        let mut writer = repo.blob_writer(Some(std::path::Path::new(path)))?;
        std::io::Write::write_all(&mut writer, &bytes)?;
        writer.commit()?
    };
    Ok(Some((id, mode)))
}

fn undo_error(error: crate::undo::UndoError) -> StashError {
    match error {
        crate::undo::UndoError::Git(error) => StashError::Git(error),
        crate::undo::UndoError::Io(error) => StashError::Io(error),
        other => StashError::Git(git2::Error::from_str(&other.to_string())),
    }
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
