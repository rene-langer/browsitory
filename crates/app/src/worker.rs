use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use egui::Context;
use git_core::{
    BlameLine, BranchInfo, CommitInfo, FileDiff, FileStatus, GraphCommit, MergeOutcome, Oid,
    ProgressUpdate, RebaseAction, RebaseStatus, RebaseStep, RemoteInfo, Repository, StashEntry,
};

pub enum Command {
    RefreshStatus,
    LoadLog {
        skip: usize,
        limit: usize,
    },
    LoadDiff {
        path: String,
        staged: bool,
    },
    Stage(String),
    Unstage(String),
    Commit(String),
    LoadBranches,
    CreateBranch {
        name: String,
        start_point: Option<Oid>,
    },
    DeleteBranch(String),
    RenameBranch {
        old_name: String,
        new_name: String,
    },
    SwitchBranch(String),
    LoadStashes,
    CreateStash {
        message: Option<String>,
    },
    ApplyStash(usize),
    PopStash(usize),
    DropStash(usize),
    LoadBlame(String),
    LoadGraph {
        max_count: usize,
    },

    MergeBranch(String),
    AbortMerge,
    LoadConflict(String),
    ResolveConflict {
        path: String,
        resolution: ResolvedWith,
    },

    /// Read-only preview of the commits an interactive rebase of `upstream`
    /// would replay, for the planner UI to assign per-row actions to before
    /// anything is actually started. Not in the original command sketch —
    /// added because `rebase_planner.rs` needs a commit list to populate its
    /// dropdowns *before* `StartRebase` (which really begins mutating
    /// on-disk rebase state) can be sent.
    LoadRebasePlan(String),
    StartRebase {
        upstream: String,
        onto: Option<String>,
    },
    /// Drives whatever step is "current" in the on-disk rebase state (see
    /// the module doc comment for why this reopens the rebase from disk
    /// each time rather than holding a live `git_core::Rebase` in worker
    /// state).
    RebaseStep(RebaseAction),
    /// Resumes the step that most recently returned `Conflict` or
    /// `PausedForEdit` (after the caller resolved conflicts via `Stage`/
    /// `ResolveConflict`, or finished amending for an `Edit` pause).
    ContinueRebaseEdit,
    AbortRebase,

    LoadRemotes,
    AddRemote {
        name: String,
        url: String,
    },
    RemoveRemote(String),
    RenameRemote {
        old_name: String,
        new_name: String,
    },
    SetRemoteUrl {
        name: String,
        url: String,
    },
    Fetch(String),
    Pull {
        remote: String,
        branch: String,
    },
}

/// How a conflicted path should be resolved. `Manual` is a no-op signal —
/// the UI has already written the user's hand-edited content to the working
/// tree file before sending this command, so the worker just needs to stage
/// it (same as the other two variants' final step).
pub enum ResolvedWith {
    Ours,
    Theirs,
    Manual,
}

pub enum Event {
    Status(Vec<FileStatus>),
    Log {
        skip: usize,
        entries: Vec<CommitInfo>,
    },
    Diff {
        path: String,
        staged: bool,
        diff: FileDiff,
    },
    Committed(Oid),
    Branches(Vec<BranchInfo>),
    BranchSwitched(String),
    Stashes(Vec<StashEntry>),
    StashCreated,
    Blame {
        path: String,
        lines: Vec<BlameLine>,
    },
    Graph(Vec<GraphCommit>),
    MergeResult(MergeOutcome),
    ConflictDiff {
        path: String,
        ours: FileDiff,
        theirs: FileDiff,
    },
    RebasePlan(Vec<CommitInfo>),
    RebaseStarted {
        steps: Vec<CommitInfo>,
    },
    RebaseProgress(RebaseStatus),
    RebaseFinished,
    Error(String),

    Remotes(Vec<RemoteInfo>),
    /// Fired repeatedly mid-`Fetch`/`Pull` (once per libgit2 `transfer_progress`
    /// callback invocation), unlike every other `Event` variant, which is sent
    /// exactly once per command. Shared with push progress (Workstream E) —
    /// one progress event type for all transfer kinds.
    TransferProgress(ProgressUpdate),
    FetchFinished,
    PullFinished(MergeOutcome),
}

/// Spawns the dedicated worker thread for one open repository.
///
/// `git2::Repository` isn't `Send`, so it's opened and used entirely within
/// this thread rather than being passed across the channel — only plain
/// owned data (`Command`/`Event`) ever crosses the boundary. One thread per
/// open repo also means switching between repos never contends on a shared
/// handle.
///
/// Beyond `repo` itself, the loop now also carries `pending_rebase_step`:
/// the `RebaseStep` (action + commit) that most recently paused on a
/// conflict or an `Edit`, needed by `ContinueRebaseEdit` (which carries no
/// payload of its own) to know how to finish committing once the user has
/// resolved things. This is plain owned data with no lifetime tie to `repo`,
/// unlike an in-progress `git_core::Rebase<'repo>` itself — see the doc
/// comment on `rebase_command` for why the actual `Rebase` handle is never
/// stored across commands at all.
pub fn spawn(path: PathBuf, ctx: Context) -> (Sender<Command>, Receiver<Event>) {
    let (cmd_tx, cmd_rx) = mpsc::channel::<Command>();
    let (evt_tx, evt_rx) = mpsc::channel::<Event>();

    thread::spawn(move || {
        let mut repo = match git_core::open(&path) {
            Ok(repo) => repo,
            Err(e) => {
                let _ = evt_tx.send(Event::Error(e.to_string()));
                return;
            }
        };
        let mut pending_rebase_step: Option<RebaseStep> = None;

        for cmd in cmd_rx {
            let event = handle(&mut repo, &mut pending_rebase_step, cmd, &evt_tx, &ctx);
            if evt_tx.send(event).is_err() {
                break; // UI side hung up (repo closed/app exiting)
            }
            ctx.request_repaint();
        }
    });

    (cmd_tx, evt_rx)
}

fn handle(
    repo: &mut Repository,
    pending_rebase_step: &mut Option<RebaseStep>,
    cmd: Command,
    evt_tx: &Sender<Event>,
    ctx: &Context,
) -> Event {
    match cmd {
        Command::RefreshStatus => refresh_status(repo),
        Command::LoadLog { skip, limit } => match git_core::commit_log(repo, None, skip, limit) {
            Ok(entries) => Event::Log { skip, entries },
            Err(e) => Event::Error(e.to_string()),
        },
        Command::LoadDiff { path, staged } => {
            let result = if staged {
                git_core::staged_file_diff(repo, &path)
            } else {
                git_core::unstaged_file_diff(repo, &path)
            };
            match result {
                Ok(diff) => Event::Diff { path, staged, diff },
                Err(e) => Event::Error(e.to_string()),
            }
        }
        Command::Stage(path) => match git_core::stage_path(repo, &path) {
            Ok(()) => refresh_status(repo),
            Err(e) => Event::Error(e.to_string()),
        },
        Command::Unstage(path) => match git_core::unstage_path(repo, &path) {
            Ok(()) => refresh_status(repo),
            Err(e) => Event::Error(e.to_string()),
        },
        Command::Commit(message) => match git_core::create_commit(repo, &message) {
            Ok(oid) => Event::Committed(oid),
            Err(e) => Event::Error(e.to_string()),
        },
        Command::LoadBranches => load_branches(repo),
        Command::CreateBranch { name, start_point } => {
            match git_core::create_branch(repo, &name, start_point) {
                Ok(()) => load_branches(repo),
                Err(e) => Event::Error(e.to_string()),
            }
        }
        Command::DeleteBranch(name) => match git_core::delete_branch(repo, &name) {
            Ok(()) => load_branches(repo),
            Err(e) => Event::Error(e.to_string()),
        },
        Command::RenameBranch { old_name, new_name } => {
            match git_core::rename_branch(repo, &old_name, &new_name) {
                Ok(()) => load_branches(repo),
                Err(e) => Event::Error(e.to_string()),
            }
        }
        Command::SwitchBranch(name) => match git_core::switch_branch(repo, &name) {
            // HEAD moved, so both the working tree (status) and history (log)
            // are stale — refresh both, same as `Command::Commit` does via
            // `Event::Committed` in `RepoSession::poll_events`.
            Ok(()) => Event::BranchSwitched(name),
            Err(e) => Event::Error(e.to_string()),
        },
        Command::LoadStashes => load_stashes(repo),
        Command::CreateStash { message } => {
            match git_core::create_stash(repo, message.as_deref()) {
                Ok(_oid) => Event::StashCreated,
                Err(e) => Event::Error(e.to_string()),
            }
        }
        Command::ApplyStash(index) => match git_core::apply_stash(repo, index) {
            Ok(()) => refresh_status(repo),
            Err(e) => Event::Error(e.to_string()),
        },
        Command::PopStash(index) => match git_core::pop_stash(repo, index) {
            Ok(()) => refresh_status(repo),
            Err(e) => Event::Error(e.to_string()),
        },
        Command::DropStash(index) => match git_core::drop_stash(repo, index) {
            Ok(()) => load_stashes(repo),
            Err(e) => Event::Error(e.to_string()),
        },
        Command::LoadBlame(path) => match git_core::blame_file(repo, &path) {
            Ok(lines) => Event::Blame { path, lines },
            Err(e) => Event::Error(e.to_string()),
        },
        Command::LoadGraph { max_count } => match git_core::graph_log(repo, max_count) {
            Ok(commits) => Event::Graph(commits),
            Err(e) => Event::Error(e.to_string()),
        },
        Command::MergeBranch(branch) => match git_core::merge_branch(repo, &branch) {
            Ok(outcome) => Event::MergeResult(outcome),
            Err(e) => Event::Error(e.to_string()),
        },
        Command::AbortMerge => match git_core::abort_merge(repo) {
            Ok(()) => refresh_status(repo),
            Err(e) => Event::Error(e.to_string()),
        },
        Command::LoadConflict(path) => load_conflict(repo, path),
        Command::ResolveConflict { path, resolution } => resolve_conflict(repo, path, resolution),

        Command::LoadRebasePlan(upstream) => match git_core::plan_rebase(repo, &upstream, None) {
            Ok(steps) => Event::RebasePlan(steps),
            Err(e) => Event::Error(e.to_string()),
        },
        Command::StartRebase { upstream, onto } => {
            start_rebase(repo, pending_rebase_step, upstream, onto)
        }
        Command::RebaseStep(action) => rebase_step(repo, pending_rebase_step, action),
        Command::ContinueRebaseEdit => continue_rebase(repo, pending_rebase_step),
        Command::AbortRebase => abort_rebase(repo, pending_rebase_step),

        Command::LoadRemotes => load_remotes(repo),
        Command::AddRemote { name, url } => match git_core::add_remote(repo, &name, &url) {
            Ok(()) => load_remotes(repo),
            Err(e) => Event::Error(e.to_string()),
        },
        Command::RemoveRemote(name) => match git_core::remove_remote(repo, &name) {
            Ok(()) => load_remotes(repo),
            Err(e) => Event::Error(e.to_string()),
        },
        Command::RenameRemote { old_name, new_name } => {
            match git_core::rename_remote(repo, &old_name, &new_name) {
                Ok(()) => load_remotes(repo),
                Err(e) => Event::Error(e.to_string()),
            }
        }
        Command::SetRemoteUrl { name, url } => match git_core::set_remote_url(repo, &name, &url) {
            Ok(()) => load_remotes(repo),
            Err(e) => Event::Error(e.to_string()),
        },
        Command::Fetch(remote) => fetch(repo, &remote, evt_tx, ctx),
        Command::Pull { remote, branch } => pull(repo, &remote, &branch, evt_tx, ctx),
    }
}

fn refresh_status(repo: &Repository) -> Event {
    match git_core::status(repo) {
        Ok(entries) => Event::Status(entries),
        Err(e) => Event::Error(e.to_string()),
    }
}

fn load_branches(repo: &Repository) -> Event {
    match git_core::list_branches(repo) {
        Ok(branches) => Event::Branches(branches),
        Err(e) => Event::Error(e.to_string()),
    }
}

fn load_conflict(repo: &Repository, path: String) -> Event {
    let sides = match git_core::read_conflict(repo, &path) {
        Ok(sides) => sides,
        Err(e) => return Event::Error(e.to_string()),
    };
    let ours = git_core::diff_blob_sides(
        repo,
        &path,
        sides.ancestor.as_deref(),
        sides.ours.as_deref(),
    );
    let theirs = git_core::diff_blob_sides(
        repo,
        &path,
        sides.ancestor.as_deref(),
        sides.theirs.as_deref(),
    );
    match (ours, theirs) {
        (Ok(ours), Ok(theirs)) => Event::ConflictDiff { path, ours, theirs },
        (Err(e), _) => Event::Error(e.to_string()),
        (_, Err(e)) => Event::Error(e.to_string()),
    }
}

fn resolve_conflict(repo: &Repository, path: String, resolution: ResolvedWith) -> Event {
    if !matches!(resolution, ResolvedWith::Manual) {
        let sides = match git_core::read_conflict(repo, &path) {
            Ok(sides) => sides,
            Err(e) => return Event::Error(e.to_string()),
        };
        let content = match resolution {
            ResolvedWith::Ours => sides.ours,
            ResolvedWith::Theirs => sides.theirs,
            ResolvedWith::Manual => unreachable!("filtered out above"),
        };
        let Some(workdir) = repo.workdir() else {
            return Event::Error("repository has no working tree".to_string());
        };
        let full_path = workdir.join(&path);
        match content {
            Some(bytes) => {
                if let Some(parent) = full_path.parent()
                    && let Err(e) = std::fs::create_dir_all(parent)
                {
                    return Event::Error(e.to_string());
                }
                if let Err(e) = std::fs::write(&full_path, bytes) {
                    return Event::Error(e.to_string());
                }
            }
            // The chosen side doesn't have this file at all (e.g. taking
            // "theirs" on a "deleted by them" conflict) — the resolution is
            // to not have the file either.
            None => {
                if let Err(e) = std::fs::remove_file(&full_path)
                    && e.kind() != std::io::ErrorKind::NotFound
                {
                    return Event::Error(e.to_string());
                }
            }
        }
    }

    match git_core::stage_path(repo, &path) {
        Ok(()) => refresh_status(repo),
        Err(e) => Event::Error(e.to_string()),
    }
}

fn load_stashes(repo: &mut Repository) -> Event {
    match git_core::list_stashes(repo) {
        Ok(stashes) => Event::Stashes(stashes),
        Err(e) => Event::Error(e.to_string()),
    }
}

/// The `Rebase<'repo>` git2 hands back from `start_rebase`/`open_rebase` is
/// never stored in worker state across commands (unlike what an earlier
/// sketch of this module assumed). It borrows `'repo` from `&Repository`,
/// and `handle`'s `repo: &mut Repository` parameter is freshly reborrowed
/// each call — a borrow tied to one command can't be stashed in a variable
/// that has to outlive many subsequent commands (including ones needing
/// `&mut Repository` for unrelated operations), so the borrow checker
/// rejects keeping a live `Rebase` around between `handle` calls.
///
/// The fix: `git_rebase_next`/`git_rebase_commit` persist their progress to
/// `.git/rebase-merge/` on disk (the same mechanism `git rebase --continue`
/// relies on from a fresh CLI invocation), so each rebase-related command
/// just reopens the in-progress rebase fresh via `repo.open_rebase(None)`,
/// drives exactly one step, and lets the handle drop at the end of the
/// function — no cross-command lifetime to manage, no unsafe lifetime
/// extension needed. The only state that *does* need to persist across
/// commands is `pending_rebase_step` (plain owned data, not a git2 borrow):
/// `ContinueRebaseEdit` carries no payload, so the worker has to remember
/// which action (and, for Reword, which message) the paused step was
/// supposed to finish with.
fn start_rebase(
    repo: &Repository,
    pending_rebase_step: &mut Option<RebaseStep>,
    upstream: String,
    onto: Option<String>,
) -> Event {
    let steps = match git_core::plan_rebase(repo, &upstream, None) {
        Ok(steps) => steps,
        Err(e) => return Event::Error(e.to_string()),
    };
    match git_core::start_rebase(repo, &upstream, None, onto.as_deref()) {
        // The returned `Rebase` is intentionally dropped immediately — see
        // this module's doc comment above `start_rebase` (the free
        // function, shadowed here) for why.
        Ok(_rebase) => {
            *pending_rebase_step = None;
            Event::RebaseStarted { steps }
        }
        Err(e) => Event::Error(e.to_string()),
    }
}

fn rebase_step(
    repo: &Repository,
    pending_rebase_step: &mut Option<RebaseStep>,
    action: RebaseAction,
) -> Event {
    let mut rebase = match repo.open_rebase(None) {
        Ok(rebase) => rebase,
        Err(e) => return Event::Error(e.to_string()),
    };
    let next_index = rebase.operation_current().map(|i| i + 1).unwrap_or(0);
    let commit = rebase
        .nth(next_index)
        .map(|op| op.id())
        .unwrap_or(Oid::ZERO_SHA1);
    let step = RebaseStep { commit, action };

    match git_core::drive_rebase_step(repo, &mut rebase, &step) {
        Ok(status) => finish_rebase_command(pending_rebase_step, step, status),
        Err(e) => Event::Error(e.to_string()),
    }
}

fn continue_rebase(repo: &Repository, pending_rebase_step: &mut Option<RebaseStep>) -> Event {
    let Some(step) = pending_rebase_step.clone() else {
        return Event::Error("no paused rebase step to continue".to_string());
    };
    let mut rebase = match repo.open_rebase(None) {
        Ok(rebase) => rebase,
        Err(e) => return Event::Error(e.to_string()),
    };
    match git_core::continue_rebase_step(repo, &mut rebase, &step) {
        Ok(status) => finish_rebase_command(pending_rebase_step, step, status),
        Err(e) => Event::Error(e.to_string()),
    }
}

fn finish_rebase_command(
    pending_rebase_step: &mut Option<RebaseStep>,
    step: RebaseStep,
    status: RebaseStatus,
) -> Event {
    match &status {
        RebaseStatus::Conflict { .. } | RebaseStatus::PausedForEdit { .. } => {
            *pending_rebase_step = Some(step);
        }
        RebaseStatus::StepComplete { .. } | RebaseStatus::Done => {
            *pending_rebase_step = None;
        }
    }
    match status {
        RebaseStatus::Done => Event::RebaseFinished,
        other => Event::RebaseProgress(other),
    }
}

fn abort_rebase(repo: &Repository, pending_rebase_step: &mut Option<RebaseStep>) -> Event {
    let rebase = match repo.open_rebase(None) {
        Ok(rebase) => rebase,
        Err(e) => return Event::Error(e.to_string()),
    };
    match git_core::abort_rebase(repo, rebase) {
        Ok(()) => {
            *pending_rebase_step = None;
            Event::RebaseFinished
        }
        Err(e) => Event::Error(e.to_string()),
    }
}

fn load_remotes(repo: &Repository) -> Event {
    match git_core::list_remotes(repo) {
        Ok(remotes) => Event::Remotes(remotes),
        Err(e) => Event::Error(e.to_string()),
    }
}

/// Unlike every other command handler in this file, `fetch`/`pull` send
/// `Event::TransferProgress` repeatedly *during* the call (once per libgit2
/// `transfer_progress` callback invocation) via `evt_tx`, in addition to the
/// single terminal `Event` returned here that the `spawn` loop sends the same
/// way it sends every other command's result.
fn fetch(repo: &Repository, remote: &str, evt_tx: &Sender<Event>, ctx: &Context) -> Event {
    let result = git_core::fetch(repo, remote, |update| {
        let _ = evt_tx.send(Event::TransferProgress(update));
        ctx.request_repaint();
    });
    match result {
        Ok(()) => Event::FetchFinished,
        Err(e) => Event::Error(e.to_string()),
    }
}

fn pull(
    repo: &Repository,
    remote: &str,
    branch: &str,
    evt_tx: &Sender<Event>,
    ctx: &Context,
) -> Event {
    let result = git_core::pull(repo, remote, branch, |update| {
        let _ = evt_tx.send(Event::TransferProgress(update));
        ctx.request_repaint();
    });
    match result {
        Ok(outcome) => Event::PullFinished(outcome),
        Err(e) => Event::Error(e.to_string()),
    }
}
