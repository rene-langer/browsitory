use git2::build::CheckoutBuilder;
use git2::{AnnotatedCommit, Oid, Rebase, Repository, Sort};

use crate::conflict::conflicted_paths;
use crate::log::CommitInfo;
use crate::repo::Result;

/// What to do with one commit in an interactive rebase plan.
///
/// git2/libgit2's `Rebase` has **no native support** for any of these beyond
/// mechanical `Pick`: in libgit2's C source, both the fresh-plan builder and
/// the interrupted-rebase reopener hardcode every operation as
/// `GIT_REBASE_OPERATION_PICK`. `RebaseOperationType` has Reword/Edit/Squash/
/// Fixup/Exec variants for API completeness, but nothing in git2 ever
/// produces them, and there's no way to hand it a custom todo list. Every
/// action below except `Pick` is therefore driven entirely by
/// `drive_rebase_step`'s own logic on top of the mechanical
/// cherry-pick-only primitive `Rebase::next()`/`Rebase::commit()` provide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebaseAction {
    Pick,
    Reword(String),
    Edit,
    Squash,
    Fixup,
    Drop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebaseStep {
    pub commit: Oid,
    pub action: RebaseAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebaseStatus {
    /// This step finished successfully and more steps remain; the caller
    /// should drive the next `RebaseStep` in its plan.
    ///
    /// This variant isn't in the original three-variant sketch of this enum
    /// (`Done`/`Conflict`/`PausedForEdit`) — it turned out to be required
    /// once actually implementing a multi-step driving loop: `Done` needs to
    /// mean "the whole rebase is finished" (i.e. `Rebase::finish()` has been
    /// called and the original branch ref now points at the final commit),
    /// which is a different thing from "this one step succeeded, keep
    /// going". See the module-level note in the final report for context.
    StepComplete {
        step_index: usize,
    },
    /// The whole rebase is finished: `Rebase::finish()` has been called.
    Done,
    Conflict {
        step_index: usize,
        paths: Vec<String>,
    },
    PausedForEdit {
        step_index: usize,
    },
}

/// Lists the commits that would be replayed by rebasing `branch` (or HEAD, if
/// `branch` is `None`) onto `upstream` — i.e. commits reachable from `branch`
/// but not from `upstream` — oldest first, matching the order a rebase
/// applies them in. For populating the rebase planner UI before the rebase
/// actually starts.
pub fn plan_rebase(
    repo: &Repository,
    upstream: &str,
    branch: Option<&str>,
) -> Result<Vec<CommitInfo>> {
    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    match branch {
        Some(name) => revwalk.push(resolve_oid(repo, name)?)?,
        None => revwalk.push_head()?,
    }
    revwalk.hide(resolve_oid(repo, upstream)?)?;

    let mut out = Vec::new();
    for oid_result in revwalk {
        let commit = repo.find_commit(oid_result?)?;
        out.push(to_commit_info(&commit));
    }
    out.reverse(); // revwalk yields newest first; a rebase applies oldest first
    Ok(out)
}

/// Starts an interactive rebase of `branch` (or HEAD, if `None`) onto
/// `upstream`, optionally re-parenting onto `onto` instead of `upstream`
/// (same three-way distinction `git rebase --onto` makes). The returned
/// `Rebase` borrows `repo` and must be driven to completion (or aborted) by
/// repeated `drive_rebase_step`/`continue_rebase_step` calls before being
/// dropped — see the worker-thread note on why it can't cross a channel.
pub fn start_rebase<'repo>(
    repo: &'repo Repository,
    upstream: &str,
    branch: Option<&str>,
    onto: Option<&str>,
) -> Result<Rebase<'repo>> {
    let upstream_annotated = resolve_annotated(repo, upstream)?;
    let branch_annotated = branch.map(|b| resolve_annotated(repo, b)).transpose()?;
    let onto_annotated = onto.map(|o| resolve_annotated(repo, o)).transpose()?;

    Ok(repo.rebase(
        branch_annotated.as_ref(),
        Some(&upstream_annotated),
        onto_annotated.as_ref(),
        None,
    )?)
}

/// Drives one step of an in-progress rebase according to `step.action`.
///
/// Every action starts the same way — `Rebase::next()` mechanically
/// cherry-picks `step.commit`'s patch onto whatever the previous step left
/// as HEAD — and then diverges:
/// - **Pick**/**Reword**: commit immediately if the cherry-pick applied
///   cleanly (keeping the original message, or overriding it for Reword);
///   otherwise pause with `Conflict`.
/// - **Edit**: pause with `PausedForEdit` on a clean apply, leaving the
///   applied patch staged for the caller to amend via `stage::stage_path`
///   before calling `continue_rebase_step`.
/// - **Squash**/**Fixup**: see `squash_or_fixup`'s doc comment — no native
///   support exists, so this collapses two commits into one after the fact.
/// - **Drop**: discards whatever `next()` just applied, regardless of
///   whether it conflicted (the changes are being thrown away either way, so
///   a conflict from the unwanted cherry-pick isn't something the user needs
///   to resolve) — see `drop_step`'s doc comment for why this commits and
///   rewinds via `checkout_tree` rather than resetting.
pub fn drive_rebase_step(
    repo: &Repository,
    rebase: &mut Rebase<'_>,
    step: &RebaseStep,
) -> Result<RebaseStatus> {
    match &step.action {
        RebaseAction::Pick => pick_like(repo, rebase, None),
        RebaseAction::Reword(message) => pick_like(repo, rebase, Some(message.as_str())),
        RebaseAction::Edit => {
            advance(rebase)?;
            if repo.index()?.has_conflicts() {
                return conflict_status(repo, rebase);
            }
            Ok(RebaseStatus::PausedForEdit {
                step_index: rebase.operation_current().unwrap_or(0),
            })
        }
        RebaseAction::Squash | RebaseAction::Fixup => squash_or_fixup(repo, rebase, step),
        RebaseAction::Drop => drop_step(repo, rebase),
    }
}

/// Resumes a step that previously returned `Conflict` (once the caller has
/// resolved every conflicted path via `stage::stage_path`) or
/// `PausedForEdit` (once the caller is done amending).
///
/// Takes the original `RebaseStep` again — a deliberate deviation from the
/// zero-argument `continue_after_edit(repo, rebase)` sketched before this was
/// implemented. Without the step, a resumed Reword can't recover the message
/// it was supposed to commit with, and a resumed Squash/Fixup can't recover
/// which collapsing behavior to run — both only became apparent once actually
/// wiring up conflict resolution for those two actions, which is exactly the
/// kind of thing the up-front git2 spike (see rebase test file) was meant to
/// surface before UI got built on top of it. Callers (`worker.rs`) hold the
/// pending `RebaseStep` alongside `active_rebase` for this purpose.
pub fn continue_rebase_step(
    repo: &Repository,
    rebase: &mut Rebase<'_>,
    step: &RebaseStep,
) -> Result<RebaseStatus> {
    if repo.index()?.has_conflicts() {
        return conflict_status(repo, rebase);
    }
    match &step.action {
        RebaseAction::Pick | RebaseAction::Edit => finish_commit(repo, rebase, None),
        RebaseAction::Reword(message) => finish_commit(repo, rebase, Some(message.as_str())),
        RebaseAction::Squash | RebaseAction::Fixup => finish_squash_or_fixup(repo, rebase, step),
        // A conflicted/paused Drop is handled entirely inside drive_rebase_step
        // (it resets instead of pausing), so there's nothing to resume here.
        RebaseAction::Drop => {
            let step_index = rebase.operation_current().unwrap_or(0);
            finish_or_continue(repo, rebase, step_index)
        }
    }
}

/// Aborts a rebase that's currently in progress, resetting the repository
/// and working directory back to their pre-rebase state (`Rebase::abort`
/// consumes the git2-managed rebase state; taking `Rebase` by value here
/// mirrors that — there's nothing left to drive afterward).
pub fn abort_rebase(_repo: &Repository, mut rebase: Rebase<'_>) -> Result<()> {
    rebase.abort()?;
    Ok(())
}

fn pick_like(
    repo: &Repository,
    rebase: &mut Rebase<'_>,
    message: Option<&str>,
) -> Result<RebaseStatus> {
    advance(rebase)?;
    if repo.index()?.has_conflicts() {
        return conflict_status(repo, rebase);
    }
    finish_commit(repo, rebase, message)
}

fn finish_commit(
    repo: &Repository,
    rebase: &mut Rebase<'_>,
    message: Option<&str>,
) -> Result<RebaseStatus> {
    let step_index = rebase.operation_current().unwrap_or(0);
    let sig = repo.signature()?;
    rebase.commit(None, &sig, message)?;
    finish_or_continue(repo, rebase, step_index)
}

/// No "skip without applying" exists either (same limitation as squash/
/// fixup — see the `RebaseAction` doc comment). Two approaches were tried
/// and failed empirically (against a real repo, not guessed) before landing
/// on this one:
///
/// 1. `next()` then an unconditional `repo.reset(HEAD, Hard, ...)`, skipping
///    `commit()` entirely.
/// 2. `next()`, `commit()` (mirroring squash/fixup: always commit, even a
///    throwaway commit), then `repo.reset(parent, Hard, ...)` to rewind.
///
/// Both failed identically: the *following* step's `next()` errored trying
/// to open `.git/rebase-merge/msgnum` for writing, because `.git/rebase-merge`
/// had vanished entirely. Tracked down in libgit2's C source
/// (`reset.c`): `git_reset` calls `git_repository_state_cleanup()`
/// unconditionally for any `Mixed`/`Hard` reset — by design, since `git
/// reset --hard` doubles as a way real users abort merges/rebases/etc. So
/// **`Repository::reset` (Mixed or Hard) can never be called while a rebase
/// is in progress**, full stop, regardless of what precedes it.
///
/// The fix: always commit (same as the two attempts above), but sync the
/// working tree/index back to the discarded commit's parent via
/// `checkout_tree` (which updates both, per its own doc comment) instead of
/// `reset` — `checkout_tree` doesn't touch repository state at all. Validated
/// in `tests/rebase.rs`'s drop test, including a debug spike
/// (see git history of this file) that printed `.git/rebase-merge`'s
/// contents after each step to catch the disappearing directory directly.
fn drop_step(repo: &Repository, rebase: &mut Rebase<'_>) -> Result<RebaseStatus> {
    advance(rebase)?;
    let step_index = rebase.operation_current().unwrap_or(0);

    if repo.index()?.has_conflicts() {
        let head_commit = repo.head()?.peel_to_commit()?;
        checkout_commit_tree(repo, &head_commit)?;
    }

    let sig = repo.signature()?;
    let temp_oid = rebase.commit(None, &sig, None)?;
    let temp_commit = repo.find_commit(temp_oid)?;
    let parent = temp_commit.parent(0)?;

    repo.set_head_detached(parent.id())?;
    checkout_commit_tree(repo, &parent)?;

    finish_or_continue(repo, rebase, step_index)
}

/// Syncs the working tree and index to `commit`'s tree via `checkout_tree`
/// (force strategy). Deliberately NOT `Repository::reset` — see the
/// `drop_step` doc comment for why a Mixed/Hard reset can't be used while a
/// rebase (or merge) is in progress: it silently wipes the in-progress state.
fn checkout_commit_tree(repo: &Repository, commit: &git2::Commit<'_>) -> Result<()> {
    let tree = commit.tree()?;
    let mut checkout = CheckoutBuilder::new();
    checkout.force();
    repo.checkout_tree(tree.as_object(), Some(&mut checkout))?;
    Ok(())
}

/// No native squash/fixup exists in git2 (see the `RebaseAction` doc
/// comment), so this hand-rolls it on top of the mechanical pick primitive,
/// validated against a real repo in `tests/rebase.rs`'s squash/fixup tests
/// before any UI was built on top of it (per the up-front spike this module
/// started from):
///
/// 1. Apply the step exactly like `Pick` — `next()` then `commit()` — giving
///    a normal temporary commit whose parent is the previous step's result
///    (the commit being squashed/fixed up *into*).
/// 2. Immediately replace that temporary commit with a new one sharing its
///    tree (the cherry-picked content is correct either way) but reparented
///    onto the *previous* commit's own parent — collapsing the previous
///    commit and the temporary commit into one. The message is either the
///    two messages joined (Squash) or just the previous commit's message,
///    discarding this step's (Fixup); authorship is kept from the previous
///    commit, matching real `git rebase -i`'s squash/fixup behavior.
/// 3. Re-point the rebase's notion of "current HEAD" at the replacement
///    commit via `set_head_detached` (mid-rebase HEAD is always detached) so
///    the *next* step's `next()` cherry-picks against the corrected history
///    instead of the discarded temporary commit.
fn squash_or_fixup(
    repo: &Repository,
    rebase: &mut Rebase<'_>,
    step: &RebaseStep,
) -> Result<RebaseStatus> {
    advance(rebase)?;
    if repo.index()?.has_conflicts() {
        return conflict_status(repo, rebase);
    }
    finish_squash_or_fixup(repo, rebase, step)
}

fn finish_squash_or_fixup(
    repo: &Repository,
    rebase: &mut Rebase<'_>,
    step: &RebaseStep,
) -> Result<RebaseStatus> {
    let step_index = rebase.operation_current().unwrap_or(0);
    let sig = repo.signature()?;
    let temp_oid = rebase.commit(None, &sig, None)?;
    let temp_commit = repo.find_commit(temp_oid)?;
    let tree = temp_commit.tree()?;
    let prev_commit = temp_commit.parent(0)?;

    let new_message = match &step.action {
        RebaseAction::Squash => format!(
            "{}\n\n{}",
            prev_commit.message().unwrap_or_default().trim_end(),
            temp_commit.message().unwrap_or_default().trim_end(),
        ),
        RebaseAction::Fixup => prev_commit.message().unwrap_or_default().to_string(),
        other => unreachable!("finish_squash_or_fixup only called for Squash/Fixup, got {other:?}"),
    };

    let author = prev_commit.author();
    let new_parents: Vec<git2::Commit> = prev_commit.parents().collect();
    let parent_refs: Vec<&git2::Commit> = new_parents.iter().collect();

    let new_oid = repo.commit(None, &author, &sig, &new_message, &tree, &parent_refs)?;
    repo.set_head_detached(new_oid)?;

    finish_or_continue(repo, rebase, step_index)
}

fn finish_or_continue(
    repo: &Repository,
    rebase: &mut Rebase<'_>,
    step_index: usize,
) -> Result<RebaseStatus> {
    if step_index + 1 >= rebase.len() {
        let sig = repo.signature()?;
        rebase.finish(Some(&sig))?;
        Ok(RebaseStatus::Done)
    } else {
        Ok(RebaseStatus::StepComplete { step_index })
    }
}

fn advance(rebase: &mut Rebase<'_>) -> Result<()> {
    match rebase.next() {
        Some(Ok(_operation)) => Ok(()),
        Some(Err(e)) => Err(e.into()),
        None => Err(git2::Error::from_str("rebase has no more operations to apply").into()),
    }
}

fn conflict_status(repo: &Repository, rebase: &mut Rebase<'_>) -> Result<RebaseStatus> {
    Ok(RebaseStatus::Conflict {
        step_index: rebase.operation_current().unwrap_or(0),
        paths: conflicted_paths(repo)?,
    })
}

fn resolve_oid(repo: &Repository, spec: &str) -> Result<Oid> {
    if let Ok(reference) = repo.resolve_reference_from_short_name(spec) {
        return Ok(reference.peel_to_commit()?.id());
    }
    Ok(repo.revparse_single(spec)?.peel_to_commit()?.id())
}

fn resolve_annotated<'repo>(repo: &'repo Repository, spec: &str) -> Result<AnnotatedCommit<'repo>> {
    if let Ok(reference) = repo.resolve_reference_from_short_name(spec) {
        return Ok(repo.reference_to_annotated_commit(&reference)?);
    }
    let obj = repo.revparse_single(spec)?;
    Ok(repo.find_annotated_commit(obj.id())?)
}

fn to_commit_info(commit: &git2::Commit) -> CommitInfo {
    let author = commit.author();
    CommitInfo {
        id: commit.id(),
        summary: commit
            .summary()
            .ok()
            .flatten()
            .unwrap_or_default()
            .to_string(),
        author_name: author.name().unwrap_or_default().to_string(),
        author_email: author.email().unwrap_or_default().to_string(),
        time: commit.time().seconds(),
        parent_ids: commit.parent_ids().collect(),
    }
}
