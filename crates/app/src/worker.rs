use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use egui::Context;
use git_core::{
    BlameLine, BranchInfo, CommitInfo, FileDiff, FileStatus, GraphCommit, Oid, Repository,
    StashEntry,
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
    LoadGraph { max_count: usize },
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
    Error(String),
}

/// Spawns the dedicated worker thread for one open repository.
///
/// `git2::Repository` isn't `Send`, so it's opened and used entirely within
/// this thread rather than being passed across the channel — only plain
/// owned data (`Command`/`Event`) ever crosses the boundary. One thread per
/// open repo also means switching between repos never contends on a shared
/// handle.
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

        for cmd in cmd_rx {
            let event = handle(&mut repo, cmd);
            if evt_tx.send(event).is_err() {
                break; // UI side hung up (repo closed/app exiting)
            }
            ctx.request_repaint();
        }
    });

    (cmd_tx, evt_rx)
}

fn handle(repo: &mut Repository, cmd: Command) -> Event {
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

fn load_stashes(repo: &mut Repository) -> Event {
    match git_core::list_stashes(repo) {
        Ok(stashes) => Event::Stashes(stashes),
        Err(e) => Event::Error(e.to_string()),
    }
}
