//! Worker side of git hooks: forwards hook progress to the frontend, logs hook problems the UI
//! does not show, and runs the post-checkout/post-merge hooks after successful commands, each
//! on its command's own operation: reported and cancellable (#178, #185).

use std::sync::mpsc::Sender;

use git_core::hooks::{HookError, HookInvocation, HookOutcome, HookReporter};

/// Which kind of command a hook runs for, so the UI can show it in the right place (#185):
/// `Commit` in the commit box and the sequence panel (commit, amend, Continue), the others in
/// the "Hook status" strip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookSource {
    /// A commit, an amend or a sequence's Continue.
    Commit,
    /// `post-checkout` after switching branches or checking out a commit.
    Checkout,
    /// `post-merge` after a fast-forward or squash merge.
    Merge,
    /// A cherry-pick or revert (single or sequence).
    Pick,
}

/// Progress of a hook, tagged with the command's operation id (which
/// `WorkerHandle::cancel_transfer` accepts to stop the hook) and the kind of command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookEvent {
    Started {
        operation_id: String,
        hook: String,
        source: HookSource,
    },
    Finished {
        operation_id: String,
        hook: String,
        source: HookSource,
    },
}

pub(super) struct ChannelHookReporter {
    pub(super) events: Sender<HookEvent>,
    pub(super) operation_id: String,
    pub(super) source: HookSource,
}

impl HookReporter for ChannelHookReporter {
    fn started(&mut self, hook: &str) {
        let _ = self.events.send(HookEvent::Started {
            operation_id: self.operation_id.clone(),
            hook: hook.to_string(),
            source: self.source,
        });
    }

    fn finished(&mut self, hook: &str, result: &Result<HookOutcome, HookError>) {
        let _ = self.events.send(HookEvent::Finished {
            operation_id: self.operation_id.clone(),
            hook: hook.to_string(),
            source: self.source,
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

/// A post hook run on a command's own operation: reported (the status line) and cancellable like
/// `post-commit`. Its result never changes the command's outcome; the reporter's `finished` logs
/// a failure (and a hook without a reporter is logged here).
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
    let result = git_core::hooks::run_reported(repo, &invocation, options);
    if options.reporter.is_none() {
        log_hook_result(name, &result);
    }
}

/// Runs `name` as a post hook of `op`'s command, reported on `events` as `source` and stopped
/// by `cancel_transfer(op.id)`, then drops `op` from the cancel registry. The command's outcome
/// stands whatever the hook does (#185).
pub(super) fn run_post_hook_on(
    repo: &git2::Repository,
    name: &str,
    args: &[&str],
    op: &super::remote::TransferOp<'_>,
    events: Sender<HookEvent>,
    source: HookSource,
) {
    let cancel = op.cancel_check();
    let mut reporter = ChannelHookReporter {
        events,
        operation_id: op.id.clone(),
        source,
    };
    run_post_hook_reported(
        repo,
        name,
        args,
        &mut git_core::hooks::HookOptions {
            cancel: Some(&cancel),
            reporter: Some(&mut reporter),
            ..Default::default()
        },
    );
}

/// HEAD's commit, or the zero id while unborn (what git passes to post-checkout).
pub(super) fn head_oid(repo: &git2::Repository) -> git2::Oid {
    repo.head()
        .ok()
        .and_then(|head| head.peel_to_commit().ok())
        .map(|commit| commit.id())
        .unwrap_or(git2::Oid::ZERO_SHA1)
}

/// `post-checkout <old HEAD> <new HEAD> 1` (1 = a branch/HEAD checkout, not a file checkout),
/// reported and cancellable on `op` (#185).
pub(super) fn run_post_checkout(
    repo: &git2::Repository,
    before: git2::Oid,
    op: &super::remote::TransferOp<'_>,
    events: Sender<HookEvent>,
) {
    let old = before.to_string();
    let new = head_oid(repo).to_string();
    run_post_hook_on(
        repo,
        "post-checkout",
        &[&old, &new, "1"],
        op,
        events,
        HookSource::Checkout,
    );
}
