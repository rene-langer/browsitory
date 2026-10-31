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
    /// `"{key}: {value}"` as given.
    #[error("Invalid trailer \"{0}\".")]
    InvalidTrailer(String),
    #[error("Invalid author. Use a name and an email address.")]
    InvalidAuthor,
}

/// A person as typed or listed in the UI: an amend author override, or a recent author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureInput {
    pub name: String,
    pub email: String,
}

/// One `Key: value` line for a commit message's trailer block (e.g. `Co-authored-by`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trailer {
    pub key: String,
    pub value: String,
}

fn is_trailer_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// `Key: value` (or `Key:value`) with a valid key: the key (lowercase) and the trimmed value.
fn parse_trailer_line(line: &str) -> Option<(String, String)> {
    let (key, value) = line.split_once(':')?;
    let key = key.trim_end();
    is_trailer_key(key).then(|| (key.to_ascii_lowercase(), value.trim().to_string()))
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// Adds `trailers` to `message`'s trailer block the way `git interpret-trailers` places them:
/// the last paragraph is the trailer block when the message has more than one paragraph and
/// every line of it is a `Key: value` trailer (or a whitespace-led continuation of one), and the
/// new lines are appended to it; otherwise a blank line and a new block follow the message.
/// Trailing blank lines are dropped. Lines starting with `#` are message text (nothing strips
/// them in Browsitory), so trailers always go at the very end. A trailer already in the block (same key
/// and value, both ignoring ASCII case, as git compares them) is not added again. No trailers leaves `message` byte-identical, and so
/// does a call whose every trailer is already there. A blank `message` with trailers is
/// [`CommitError::EmptyMessage`]: trailers alone must not turn an empty message into a commit.
pub fn append_trailers(message: &str, trailers: &[Trailer]) -> Result<String, CommitError> {
    if trailers.is_empty() {
        return Ok(message.to_string());
    }
    if message.trim().is_empty() {
        return Err(CommitError::EmptyMessage);
    }
    let mut lines_to_add: Vec<(String, String, String)> = Vec::new();
    for trailer in trailers {
        let value = trailer.value.trim();
        if !is_trailer_key(&trailer.key) || value.is_empty() || value.contains(['\n', '\r', '\0']) {
            return Err(CommitError::InvalidTrailer(format!(
                "{}: {}",
                trailer.key, trailer.value
            )));
        }
        lines_to_add.push((
            trailer.key.to_ascii_lowercase(),
            value.to_string(),
            format!("{}: {}", trailer.key, value),
        ));
    }

    // `#` lines are ordinary text here: Browsitory commits the message as typed (like
    // `git commit -m`, no comment cleanup), so moving them after the trailers, as
    // `git interpret-trailers` does for an editor buffer, would reorder the user's message.
    let body = message.trim_end();

    let body_lines: Vec<&str> = body.split('\n').collect();
    let last_blank = body_lines.iter().rposition(|line| is_blank(line));
    let block: Option<&[&str]> = match last_blank {
        Some(blank) if body_lines[..blank].iter().any(|line| !is_blank(line)) => {
            let paragraph = &body_lines[blank + 1..];
            let first_is_trailer = paragraph
                .first()
                .is_some_and(|line| parse_trailer_line(line).is_some());
            let all_trailers = paragraph
                .iter()
                .all(|line| line.starts_with([' ', '\t']) || parse_trailer_line(line).is_some());
            (first_is_trailer && all_trailers).then_some(paragraph)
        }
        _ => None,
    };

    let mut present: Vec<(String, String)> = block
        .unwrap_or_default()
        .iter()
        .filter_map(|line| parse_trailer_line(line))
        .collect();
    let mut new_lines = Vec::new();
    for (key, value, line) in lines_to_add {
        // Like git's `addIfDifferent`: key and value both compared ignoring ASCII case.
        if present
            .iter()
            .any(|(k, v)| *k == key && v.eq_ignore_ascii_case(&value))
        {
            continue;
        }
        present.push((key, value));
        new_lines.push(line);
    }
    if new_lines.is_empty() {
        return Ok(message.to_string());
    }

    let mut out = String::with_capacity(message.len() + 64);
    if !body.is_empty() {
        out.push_str(body);
        out.push_str(if block.is_some() { "\n" } else { "\n\n" });
    }
    out.push_str(&new_lines.join("\n"));
    out.push('\n');
    Ok(out)
}

fn valid_author(author: &SignatureInput) -> Option<(&str, &str)> {
    let name = author.name.trim();
    let email = author.email.trim();
    let name_ok = !name.is_empty()
        && !name
            .chars()
            .any(|c| matches!(c, '<' | '>') || c.is_control());
    let email_ok = !email.is_empty()
        && !email
            .chars()
            .any(|c| matches!(c, '<' | '>') || c.is_whitespace() || c.is_control());
    (name_ok && email_ok).then_some((name, email))
}

/// How many commits from HEAD `recent_authors` looks at, at most.
pub const RECENT_AUTHORS_WALK_LIMIT: usize = 1000;

/// Up to `limit` distinct authors (by case-insensitive email) of the newest
/// `RECENT_AUTHORS_WALK_LIMIT` commits reachable from HEAD, newest first, without the current
/// user (`repo.signature()`'s email). An unborn HEAD has none.
pub fn recent_authors(
    repo: &git2::Repository,
    limit: usize,
) -> Result<Vec<SignatureInput>, CommitError> {
    let mut authors = Vec::new();
    if limit == 0 {
        return Ok(authors);
    }
    match repo.head() {
        Ok(_) => {}
        Err(error)
            if matches!(
                error.code(),
                git2::ErrorCode::UnbornBranch | git2::ErrorCode::NotFound
            ) =>
        {
            return Ok(authors)
        }
        Err(error) => return Err(error.into()),
    }
    let me = repo
        .signature()
        .ok()
        .and_then(|s| s.email().ok().map(str::to_lowercase));
    let mut seen = std::collections::HashSet::new();
    let mut walk = repo.revwalk()?;
    // Unsorted on purpose: any sort mode makes libgit2 walk (and sort) the whole history before
    // yielding the first commit, so `take` would bound only the output, not the work. The
    // unsorted walk is lazy and already roughly newest-first (a commit-time priority queue).
    walk.set_sorting(git2::Sort::NONE)?;
    walk.push_head()?;
    for oid in walk.take(RECENT_AUTHORS_WALK_LIMIT) {
        let commit = repo.find_commit(oid?)?;
        let author = commit.author();
        let (Ok(name), Ok(email)) = (author.name(), author.email()) else {
            continue;
        };
        let key = email.to_lowercase();
        if email.trim().is_empty() || Some(&key) == me.as_ref() || !seen.insert(key) {
            continue;
        }
        authors.push(SignatureInput {
            name: name.to_string(),
            email: email.to_string(),
        });
        if authors.len() == limit {
            break;
        }
    }
    Ok(authors)
}

pub(crate) struct HookedMessage {
    pub(crate) message: String,
    /// A pre-commit ran, so the on-disk index may differ from the cached one.
    pub(crate) reload_index: bool,
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

/// `pre_hook` (`pre-commit`, or `pre-merge-commit` for a clean merge's commit) ->
/// `prepare-commit-msg` -> `commit-msg`, as `git commit -m` (or `git merge`) runs them. The
/// message goes through `<git dir>/COMMIT_EDITMSG` only when a message hook will run, and is
/// re-read afterwards so hook edits apply.
pub(crate) fn run_commit_hooks(
    repo: &git2::Repository,
    message: &str,
    pre_hook: &str,
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
            name: pre_hook,
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
pub(crate) fn run_post_commit(repo: &git2::Repository, options: &mut HookOptions<'_>) {
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

/// `prepare-commit-msg`'s second argument, as `git commit` picks it when the message came from a
/// file: `merge` while finishing a merge, cherry-pick or revert (MERGE_MSG, which pre-fills
/// Browsitory's message box too), `squash` for a pending squash merge, else `message`.
fn message_source(repo: &git2::Repository, is_merging: bool) -> &'static str {
    if is_merging || repo.path().join("MERGE_MSG").exists() {
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
    hooks: HookOptions<'_>,
) -> Result<String, CommitError> {
    commit_with(repo, message, CommitHookSet::Commit, hooks).map(|committed| committed.commit_id)
}

/// Which hooks a commit runs: `git commit`'s, or `git merge`'s when it concludes a clean merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommitHookSet {
    /// `pre-commit`, `prepare-commit-msg`, `commit-msg`, `post-commit`.
    #[default]
    Commit,
    /// For the commit concluding a merge that Browsitory started and that stopped without
    /// conflicts (#124): what `git merge` runs when it creates the commit, `pre-merge-commit`
    /// (in place of `pre-commit`, also skipped by `no_verify`), `prepare-commit-msg … merge` and
    /// `commit-msg`, and **no** `post-commit`. The caller (the worker) runs `post-merge 0` after
    /// it when `Committed::merge_hooks` says these hooks ran. Outside a merge (`repo.state() != Merge`) this is `Commit`.
    CleanMerge,
}

/// What `commit_with` committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    pub commit_id: String,
    /// `git merge`'s hooks ran (`CleanMerge` while a merge was in progress), so the caller owes
    /// `post-merge 0`. False when `CleanMerge` fell back to `git commit`'s hooks, for example
    /// because the merge was aborted after the caller chose the set.
    pub merge_hooks: bool,
}

/// `commit` with a choice of hook set (see `CommitHookSet`). Whether the merge hooks really
/// ran is decided here, from the repository's state at commit time, and reported in
/// `Committed::merge_hooks`.
pub fn commit_with(
    repo: &mut git2::Repository,
    message: &str,
    set: CommitHookSet,
    mut hooks: HookOptions<'_>,
) -> Result<Committed, CommitError> {
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

    // `git merge`'s hooks only for a clean merge's commit; otherwise `git commit`'s.
    let merge_hooks = set == CommitHookSet::CleanMerge && is_merging;
    let pre_hook = if merge_hooks {
        "pre-merge-commit"
    } else {
        "pre-commit"
    };
    let source = message_source(repo, is_merging);
    let hooked = run_commit_hooks(repo, message, pre_hook, source, &mut hooks)?;

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

        create_commit_reporting(
            repo,
            &mut hooks,
            crate::signing::HeadUpdate {
                expected: head_parent.as_ref().map(|commit| commit.id()),
                reflog_operation: "commit",
            },
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

    if !merge_hooks {
        run_post_commit(repo, &mut hooks);
    }
    Ok(Committed {
        commit_id: oid.to_string(),
        merge_hooks,
    })
}

/// `signing::create_commit_with`, bounded by the commit's cancel check. While a signing program
/// runs, the hook reporter sees a `"signing"` pseudo-hook (started/finished, reported as
/// `NotInstalled` so nothing logs it): the commit box shows "Waiting for the signing program…"
/// with the hooks' Cancel button (#110).
#[allow(clippy::too_many_arguments)]
pub(crate) fn create_commit_reporting(
    repo: &git2::Repository,
    hooks: &mut HookOptions<'_>,
    head_update: crate::signing::HeadUpdate<'_>,
    author: &git2::Signature<'_>,
    committer: &git2::Signature<'_>,
    message: &str,
    tree: &git2::Tree<'_>,
    parents: &[&git2::Commit<'_>],
) -> Result<git2::Oid, CommitError> {
    const SIGNING: &str = "signing";
    let signing = matches!(crate::signing::signing_config(repo), Ok(Some(_)));
    if signing {
        if let Some(reporter) = hooks.reporter.as_mut() {
            reporter.started(SIGNING);
        }
    }
    let never = || false;
    let result = crate::signing::create_commit_with(
        repo,
        Some(head_update),
        author,
        committer,
        message,
        tree,
        parents,
        &crate::signing::SignOptions {
            cancel: hooks.cancel.unwrap_or(&never),
            timeout: crate::signing::SIGN_TIMEOUT,
        },
    );
    if signing {
        if let Some(reporter) = hooks.reporter.as_mut() {
            reporter.finished(SIGNING, &Ok(hooks::HookOutcome::NotInstalled));
        }
    }
    Ok(result?)
}

/// Rewrites HEAD's commit with the current index tree and `message`, keeping its parents and
/// author (or, with `author`, replacing the author's name and email but keeping its time);
/// committer becomes `repo.signature()`. An invalid `author` (blank, or `<`, `>`, control
/// characters, or whitespace in the email) is `InvalidAuthor`, checked before anything runs. Works on a branch (the branch ref moves) or a
/// detached HEAD. Refused while any operation (merge, cherry-pick, revert, rebase) is paused,
/// since amending then would silently drop that operation's extra state. Goes through `signing::create_commit`, so it is signed when `commit.gpgSign`
/// asks for it. Runs the commit hooks exactly as `commit` does.
pub fn amend_head(
    repo: &git2::Repository,
    message: &str,
    author: Option<&SignatureInput>,
    mut hooks: HookOptions<'_>,
) -> Result<String, CommitError> {
    let author = match author {
        Some(author) => Some(valid_author(author).ok_or(CommitError::InvalidAuthor)?),
        None => None,
    };
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
    let hooked = run_commit_hooks(repo, message, "pre-commit", "message", &mut hooks)?;
    let mut index = repo.index()?;
    if hooked.reload_index {
        index.read(true)?;
    }
    let tree_id = index.write_tree()?;
    let tree = repo.find_tree(tree_id)?;
    let committer = repo.signature()?;
    // Like `git commit --amend --author`: the new name and email, the original author time.
    let author = match author {
        Some((name, email)) => git2::Signature::new(name, email, &head.author().when())?,
        None => head.author().to_owned(),
    };
    let parents: Vec<git2::Commit> = head.parents().collect();
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    let oid = create_commit_reporting(
        repo,
        &mut hooks,
        crate::signing::HeadUpdate {
            expected: Some(head.id()),
            reflog_operation: "commit (amend)",
        },
        &author,
        &committer,
        &hooked.message,
        &tree,
        &parent_refs,
    )?;
    run_post_commit(repo, &mut hooks);
    Ok(oid.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn co(value: &str) -> Trailer {
        Trailer {
            key: "Co-authored-by".into(),
            value: value.into(),
        }
    }

    const ADA: &str = "Ada <ada@x.org>";

    #[test]
    fn no_trailers_leaves_the_message_unchanged() {
        assert_eq!(
            append_trailers("Fix bug  \n\n", &[]).unwrap(),
            "Fix bug  \n\n"
        );
        assert_eq!(append_trailers("", &[]).unwrap(), "");
    }

    #[test]
    fn trailers_go_after_a_blank_line() {
        assert_eq!(
            append_trailers("Fix bug\n", &[co(ADA)]).unwrap(),
            "Fix bug\n\nCo-authored-by: Ada <ada@x.org>\n"
        );
        assert_eq!(
            append_trailers("Fix bug", &[co(ADA)]).unwrap(),
            "Fix bug\n\nCo-authored-by: Ada <ada@x.org>\n"
        );
        assert_eq!(
            append_trailers(
                "Fix bug\n\nBody text.\n\n\n",
                &[co(ADA), co("Bob <bob@x.org>")]
            )
            .unwrap(),
            "Fix bug\n\nBody text.\n\nCo-authored-by: Ada <ada@x.org>\nCo-authored-by: Bob <bob@x.org>\n"
        );
        assert!(matches!(
            append_trailers("  \n", &[co(ADA)]),
            Err(CommitError::EmptyMessage)
        ));
    }

    #[test]
    fn an_existing_trailer_block_is_extended() {
        assert_eq!(
            append_trailers("Fix\n\nBody\n\nSigned-off-by: Me <me@x.org>\n", &[co(ADA)]).unwrap(),
            "Fix\n\nBody\n\nSigned-off-by: Me <me@x.org>\nCo-authored-by: Ada <ada@x.org>\n"
        );
        // A continuation line belongs to the trailer above it.
        assert_eq!(
            append_trailers("Fix\n\nNote: long\n  wrapped\n", &[co(ADA)]).unwrap(),
            "Fix\n\nNote: long\n  wrapped\nCo-authored-by: Ada <ada@x.org>\n"
        );
    }

    #[test]
    fn duplicates_are_skipped() {
        let message = "Fix\n\nCo-authored-by: Ada <ada@x.org>\n";
        assert_eq!(append_trailers(message, &[co(ADA)]).unwrap(), message);
        // Key case and spacing don't make a different trailer, as in `git interpret-trailers`.
        let message = "Fix\n\nco-authored-by:Ada <ada@x.org>\n";
        assert_eq!(append_trailers(message, &[co(ADA)]).unwrap(), message);
        // Value case doesn't either: git compares trailer values with `strcasecmp`.
        let message = "Fix\n\nCo-authored-by: ADA <ADA@x.org>\n";
        assert_eq!(append_trailers(message, &[co(ADA)]).unwrap(), message);
        // Twice in one call: added once.
        assert_eq!(
            append_trailers("Fix\n", &[co(ADA), co(" Ada <ada@x.org> ")]).unwrap(),
            "Fix\n\nCo-authored-by: Ada <ada@x.org>\n"
        );
        // The same text in the body is not a trailer, so it doesn't count.
        assert_eq!(
            append_trailers("Co-authored-by: Ada <ada@x.org>\n", &[co(ADA)]).unwrap(),
            "Co-authored-by: Ada <ada@x.org>\n\nCo-authored-by: Ada <ada@x.org>\n"
        );
    }

    #[test]
    fn a_paragraph_mixing_text_and_trailers_is_not_a_trailer_block() {
        assert_eq!(
            append_trailers("Fix\n\nBody\nKey: v\n", &[co(ADA)]).unwrap(),
            "Fix\n\nBody\nKey: v\n\nCo-authored-by: Ada <ada@x.org>\n"
        );
    }

    #[test]
    fn a_one_paragraph_message_that_looks_like_a_trailer_is_still_the_subject() {
        assert_eq!(
            append_trailers("fix: crash on start\n", &[co(ADA)]).unwrap(),
            "fix: crash on start\n\nCo-authored-by: Ada <ada@x.org>\n"
        );
    }

    #[test]
    fn hash_lines_are_message_text_and_trailers_go_last() {
        // Browsitory never strips `#` lines, so they stay where the user put them.
        assert_eq!(
            append_trailers("Fix crash\n\n#123\n", &[co(ADA)]).unwrap(),
            "Fix crash\n\n#123\n\nCo-authored-by: Ada <ada@x.org>\n"
        );
        assert_eq!(
            append_trailers("Subject\n# not a comment\n", &[co(ADA)]).unwrap(),
            "Subject\n# not a comment\n\nCo-authored-by: Ada <ada@x.org>\n"
        );
        assert_eq!(
            append_trailers("Subject\n\nSigned-off-by: A <a@x>\n# note\n", &[co(ADA)]).unwrap(),
            "Subject\n\nSigned-off-by: A <a@x>\n# note\n\nCo-authored-by: Ada <ada@x.org>\n"
        );
    }

    #[test]
    fn invalid_trailers_are_refused() {
        for (key, value) in [
            ("Bad key", "v"),
            ("", "v"),
            ("-lead", "v"),
            ("Key:", "v"),
            ("Kéy", "v"),
            ("Key", "  "),
            ("Key", "a\nb"),
            ("Key", "a\rb"),
            ("Key", "a\0b"),
        ] {
            let t = Trailer {
                key: key.into(),
                value: value.into(),
            };
            assert!(
                matches!(
                    append_trailers("x", &[t]),
                    Err(CommitError::InvalidTrailer(_))
                ),
                "{key:?} {value:?}"
            );
        }
        assert_eq!(
            CommitError::InvalidTrailer("Bad key: v".into()).to_string(),
            "Invalid trailer \"Bad key: v\"."
        );
    }
}
