use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use egui::Context;
use git_core::{BlameLine, CommitInfo, FileDiff, FileStatus, GraphCommit, Oid, Repository};

pub enum Command {
    RefreshStatus,
    LoadLog { skip: usize, limit: usize },
    LoadDiff { path: String, staged: bool },
    Stage(String),
    Unstage(String),
    Commit(String),
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
        let repo = match git_core::open(&path) {
            Ok(repo) => repo,
            Err(e) => {
                let _ = evt_tx.send(Event::Error(e.to_string()));
                return;
            }
        };

        for cmd in cmd_rx {
            let event = handle(&repo, cmd);
            if evt_tx.send(event).is_err() {
                break; // UI side hung up (repo closed/app exiting)
            }
            ctx.request_repaint();
        }
    });

    (cmd_tx, evt_rx)
}

fn handle(repo: &Repository, cmd: Command) -> Event {
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
