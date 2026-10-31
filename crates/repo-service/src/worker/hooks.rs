//! Worker side of git hooks: forwards hook progress to the frontend, logs hook problems the UI
//! does not show, and runs the post-checkout/post-merge hooks after successful commands (a
//! merge commit's post-merge on the commit's own operation, reported and cancellable).

use std::sync::mpsc::Sender;

use git_core::hooks::{HookError, HookInvocation, HookOutcome, HookReporter, HOOK_TIMEOUT};

/// Progress of a commit/amend hook, tagged with the command's operation id (which
/// `WorkerHandle::cancel_transfer` accepts to stop the hook).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookEvent {
    Started { operation_id: String, hook: String },
    Finished { operation_id: String, hook: String },
}

pub(super) struct ChannelHookReporter {
    pub(super) events: Sender<HookEvent>,
    pub(super) operation_id: String,
}

impl HookReporter for ChannelHookReporter {
    fn started(&mut self, hook: &str) {
        let _ = self.events.send(HookEvent::Started {
            operation_id: self.operation_id.clone(),
            hook: hook.to_string(),
        });
    }

    fn finished(&mut self, hook: &str, result: &Result<HookOutcome, HookError>) {
        let _ = self.events.send(HookEvent::Finished {
            operation_id: self.operation_id.clone(),
            hook: hook.to_string(),
        });
        log_hook_result(hook, result);
    }
}

/// Logs what the UI never shows: a failed `post-*` hook, a hook skipped for lacking an execute
/// bit, and (post hooks only) a missing shell or a hook that could not start. Gate hook failures
/// reach the UI instead.
pub(super) fn log_hook_result(hook: &str, result: &Result<HookOutcome, HookError>) {
    let post = hook.starts_with("post-");
    match result {
        Ok(HookOutcome::Ran(run)) if post && !run.success => {
            let note = if run.timed_out { " (timed out)" } else { "" };
            eprintln!(
                "browsitory: the {hook} hook failed{note} (ignored):\n{}",
                run.output
            )
        }
        Ok(HookOutcome::NotExecutable) => {
            eprintln!("browsitory: ignored the {hook} hook because it is not executable")
        }
        Ok(HookOutcome::NoShell) if post => {
            eprintln!("browsitory: skipped the {hook} hook: no sh.exe next to git.exe or on PATH")
        }
        Err(error) if post => eprintln!("browsitory: {error}"),
        _ => {}
    }
}

/// `git-core` runs `pre-push` inside the push and never prints; a `pre-push` it skipped for
/// lacking an execute bit is logged here, like every other hook.
pub(super) fn log_skipped_pre_push(repo: &git2::Repository, no_verify: bool) {
    if no_verify {
        return;
    }
    if let Ok(git_core::hooks::HookLookup::NotExecutable(_)) =
        git_core::hooks::find_hook(repo, "pre-push")
    {
        log_hook_result("pre-push", &Ok(HookOutcome::NotExecutable));
    }
}

/// Runs a hook whose failure must not affect the command that triggered it. Not cancellable;
/// bounded by `HOOK_TIMEOUT`.
pub(super) fn run_post_hook(repo: &git2::Repository, name: &str, args: &[&str]) {
    let invocation = HookInvocation {
        name,
        args,
        stdin: None,
        env: &[],
    };
    let result = git_core::hooks::run_hook(repo, &invocation, &|| false, HOOK_TIMEOUT);
    log_hook_result(name, &result);
}

/// A post hook run on a command's own operation: reported (the status line) and cancellable like
/// `post-commit`. Its result never changes the command's outcome; the reporter's `finished` logs
/// a failure.
pub(super) fn run_post_hook_reported(
    repo: &git2::Repository,
    name: &str,
    args: &[&str],
    options: &mut git_core::hooks::HookOptions<'_>,
) {
    let invocation = HookInvocation {
        name,
        args,
        stdin: None,
        env: &[],
    };
    let _ = git_core::hooks::run_reported(repo, &invocation, options);
}

/// HEAD's commit, or the zero id while unborn (what git passes to post-checkout).
pub(super) fn head_oid(repo: &git2::Repository) -> git2::Oid {
    repo.head()
        .ok()
        .and_then(|head| head.peel_to_commit().ok())
        .map(|commit| commit.id())
        .unwrap_or(git2::Oid::ZERO_SHA1)
}

/// `post-checkout <old HEAD> <new HEAD> 1` (1 = a branch/HEAD checkout, not a file checkout).
pub(super) fn run_post_checkout(repo: &git2::Repository, before: git2::Oid) {
    let old = before.to_string();
    let new = head_oid(repo).to_string();
    run_post_hook(repo, "post-checkout", &[&old, &new, "1"]);
}
