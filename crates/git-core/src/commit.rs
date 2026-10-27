use std::ffi::OsStr;

use thiserror::Error;

use crate::hooks::{self, HookInvocation, HookLookup, HookOptions};

/// What a commit or amend produced, as it crosses IPC: a hook failure is an expected outcome,
/// not an error, like `pick::PickOutcome::Conflicted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitOutcome {
    Committed { commit_id: String },
    HookFailed { hook: String, output: String },
}

#[derive(Debug, Error)]
pub enum CommitError {
    #[error("failed to create commit: {0}")]
    Write(#[from] git2::Error),
    #[error("There is no commit to amend yet.")]
    NothingToAmend,
    #[error("Finish or abort the current merge/cherry-pick/revert/rebase first.")]
    OperationInProgress,
    #[error("{0}")]
    Sign(#[from] crate::signing::SignError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// A gate hook (`pre-commit`, `prepare-commit-msg`, `commit-msg`) failed; nothing was
    /// committed. `output` is what the user sees.
    #[error("The {hook} hook failed.")]
    HookFailed { hook: String, output: String },
    #[error("{0}")]
    Hook(#[from] hooks::HookError),
    #[error("Aborting commit due to empty commit message.")]
    EmptyMessage,
}

struct HookedMessage {
    message: String,
    /// A pre-commit ran, so the on-disk index may differ from the cached one.
    reload_index: bool,
}

/// Runs one gate hook; `Ok(true)` when it actually ran (and succeeded).
fn gate(
    repo: &git2::Repository,
    invocation: &HookInvocation<'_>,
    options: &mut HookOptions<'_>,
) -> Result<bool, CommitError> {
    let outcome = match hooks::run_reported(repo, invocation, options) {
        Ok(outcome) => outcome,
        // A hook that cannot start blocks like a failing one, and is retryable without hooks.
        Err(error @ hooks::HookError::Spawn { .. }) => {
            return Err(CommitError::HookFailed {
                hook: invocation.name.to_string(),
                output: hooks::spawn_failure_output(&error),
            })
        }
        Err(error) => return Err(error.into()),
    };
    if let Some(output) = hooks::failure_output(&outcome, options.timeout) {
        return Err(CommitError::HookFailed {
            hook: invocation.name.to_string(),
            output,
        });
    }
    Ok(matches!(outcome, hooks::HookOutcome::Ran(_)))
}

fn installed(repo: &git2::Repository, name: &str) -> Result<bool, CommitError> {
    Ok(matches!(
        hooks::find_hook(repo, name)?,
        HookLookup::Found(_)
    ))
}

/// `pre-commit` -> `prepare-commit-msg` -> `commit-msg`, as `git commit -m` runs them. The
/// message goes through `<git dir>/COMMIT_EDITMSG` only when a message hook will run, and is
/// re-read afterwards so hook edits apply.
fn run_commit_hooks(
    repo: &git2::Repository,
    message: &str,
    source: &str,
    options: &mut HookOptions<'_>,
) -> Result<HookedMessage, CommitError> {
    let index_path = repo.path().join("index");
    let env: [(&str, &OsStr); 2] = [
        ("GIT_INDEX_FILE", index_path.as_os_str()),
        ("GIT_EDITOR", OsStr::new(":")),
    ];
    let mut reload_index = false;
    if !options.no_verify {
        let pre_commit = HookInvocation {
            name: "pre-commit",
            args: &[],
            stdin: None,
            env: &env,
        };
        reload_index = gate(repo, &pre_commit, options)?;
    }
    let message_hooks = installed(repo, "prepare-commit-msg")?
        || (!options.no_verify && installed(repo, "commit-msg")?);
    if !message_hooks {
        return Ok(HookedMessage {
            message: message.to_string(),
            reload_index,
        });
    }
    let path = repo.path().join("COMMIT_EDITMSG");
    std::fs::write(&path, message)?;
    let path_arg = path.to_string_lossy().into_owned();
    let prepare_args = [path_arg.as_str(), source];
    gate(
        repo,
        &HookInvocation {
            name: "prepare-commit-msg",
            args: &prepare_args,
            stdin: None,
            env: &env,
        },
        options,
    )?;
    if !options.no_verify {
        let msg_args = [path_arg.as_str()];
        gate(
            repo,
            &HookInvocation {
                name: "commit-msg",
                args: &msg_args,
                stdin: None,
                env: &env,
            },
            options,
        )?;
    }
    let edited = String::from_utf8_lossy(&std::fs::read(&path)?).into_owned();
    if edited.trim().is_empty() {
        return Err(CommitError::EmptyMessage);
    }
    Ok(HookedMessage {
        message: edited,
        reload_index,
    })
}

/// Failure is ignored: the commit already exists. The reporter (the worker) logs it.
fn run_post_commit(repo: &git2::Repository, options: &mut HookOptions<'_>) {
    let index_path = repo.path().join("index");
    let env: [(&str, &OsStr); 2] = [
        ("GIT_INDEX_FILE", index_path.as_os_str()),
        ("GIT_EDITOR", OsStr::new(":")),
    ];
    let _ = hooks::run_reported(
        repo,
        &HookInvocation {
            name: "post-commit",
            args: &[],
            stdin: None,
            env: &env,
        },
        options,
    );
}

/// `prepare-commit-msg`'s second argument.
fn message_source(repo: &git2::Repository, is_merging: bool) -> &'static str {
    if is_merging {
        "merge"
    } else if repo.path().join("SQUASH_MSG").exists() {
        "squash"
    } else {
        "message"
    }
}

/// Commits the current index. Author/committer come from `repo.signature()` (reads
/// `user.name`/`user.email` from git config — returns a clear `git2::Error` if unset).
/// Parent is the current `HEAD` commit if one exists, or no parents for a repo's first commit —
/// plus, if a merge is in progress (`RepositoryState::Merge`), `MERGE_HEAD`'s commit(s) as
/// additional parents, matching real `git commit`'s own behavior of producing a merge commit
/// once conflicts (if any) are resolved and staged. `repo` needs to be `&mut` only because
/// reading `MERGE_HEAD` via `mergehead_foreach` requires it — no other part of this function
/// does.
///
/// Finishing a paused cherry-pick (`CherryPick`/`CherryPickSequence`) authors the commit as the
/// picked commit's author (read from `CHERRY_PICK_HEAD`, like `git cherry-pick --continue`) with
/// the current user as committer. Finishing a paused revert authors it as the current user. Either
/// way the result is single-parent and the operation state is cleared.
///
/// Runs the commit hooks like `git commit -m`: `pre-commit` (files it stages are committed),
/// `prepare-commit-msg`, `commit-msg` (both may rewrite the message), then `post-commit` after
/// the commit landed (its failure is ignored). `hooks.no_verify` skips `pre-commit` and
/// `commit-msg`. A failing gate hook is `CommitError::HookFailed`, nothing committed.
pub fn commit(
    repo: &mut git2::Repository,
    message: &str,
    mut hooks: HookOptions<'_>,
) -> Result<String, CommitError> {
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

    let pick_author = if matches!(
        repo.state(),
        git2::RepositoryState::CherryPick | git2::RepositoryState::CherryPickSequence
    ) {
        repo.find_reference("CHERRY_PICK_HEAD")
            .ok()
            .and_then(|r| r.peel_to_commit().ok())
            .map(|c| c.author().to_owned())
    } else {
        None
    };

    let source = message_source(repo, is_merging);
    let hooked = run_commit_hooks(repo, message, source, &mut hooks)?;

    let oid = {
        let mut index = repo.index()?;
        if hooked.reload_index {
            index.read(true)?;
        }
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

        crate::signing::create_commit(
            repo,
            Some(crate::signing::HeadUpdate {
                expected: head_parent.as_ref().map(|commit| commit.id()),
                reflog_operation: "commit",
            }),
            pick_author.as_ref().unwrap_or(&signature),
            &signature,
            &hooked.message,
            &tree,
            &parents,
        )?
    };

    // Cherry-pick/revert leave CHERRY_PICK_HEAD/REVERT_HEAD + MERGE_MSG behind on conflict; the
    // commit that lands the resolved result is single-parent (HEAD only — they are not merges),
    // but the state files must still be cleared or `repo.state()` stays non-Clean.
    let finishes_pick = matches!(
        repo.state(),
        git2::RepositoryState::CherryPick
            | git2::RepositoryState::CherryPickSequence
            | git2::RepositoryState::Revert
            | git2::RepositoryState::RevertSequence
    );
    if is_merging || finishes_pick {
        repo.cleanup_state()?;
    }
    // A squash merge's message has been used (or replaced) by this commit.
    crate::merge::clear_squash_message(repo)?;

    run_post_commit(repo, &mut hooks);
    Ok(oid.to_string())
}

/// Rewrites HEAD's commit with the current index tree and `message`, keeping its parents and
/// author; committer becomes `repo.signature()`. Works on a branch (the branch ref moves) or a
/// detached HEAD. Refused while any operation (merge, cherry-pick, revert, rebase) is paused,
/// since amending then would silently drop that operation's extra state. Goes through `signing::create_commit`, so it is signed when `commit.gpgSign`
/// asks for it. Runs the commit hooks exactly as `commit` does.
pub fn amend_head(
    repo: &git2::Repository,
    message: &str,
    mut hooks: HookOptions<'_>,
) -> Result<String, CommitError> {
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
    let hooked = run_commit_hooks(repo, message, "message", &mut hooks)?;
    let mut index = repo.index()?;
    if hooked.reload_index {
        index.read(true)?;
    }
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let committer = repo.signature()?;
    let parents: Vec<git2::Commit> = head.parents().collect();
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    let oid = crate::signing::create_commit(
        repo,
        Some(crate::signing::HeadUpdate {
            expected: Some(head.id()),
            reflog_operation: "commit (amend)",
        }),
        &head.author(),
        &committer,
        &hooked.message,
        &tree,
        &parent_refs,
    )?;
    run_post_commit(repo, &mut hooks);
    Ok(oid.to_string())
}
