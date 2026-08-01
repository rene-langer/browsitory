use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

use egui::Context;
use git_core::{BranchInfo, CommitInfo, FileDiff, FileStatus, Oid, StashEntry};

use crate::worker::{self, Command, Event};

pub struct SelectedDiff {
    pub path: String,
    pub staged: bool,
    pub diff: Option<FileDiff>,
}

pub struct RepoSession {
    pub path: PathBuf,
    pub name: String,
    tx: Sender<Command>,
    rx: Receiver<Event>,
    pub status: Vec<FileStatus>,
    pub commits: Vec<CommitInfo>,
    pub selected_diff: Option<SelectedDiff>,
    pub commit_message: String,
    pub last_commit: Option<Oid>,
    pub branches: Vec<BranchInfo>,
    pub current_branch: Option<String>,
    pub stashes: Vec<StashEntry>,
    pub error: Option<String>,
    /// UI-only scratch state for the branch/stash panels (`crates/app/src/ui/branch_panel.rs`,
    /// `crates/app/src/ui/stash_panel.rs`) — mirrors how `commit_message` above is UI-bound
    /// scratch state for the staging panel, not data mirrored from the worker thread.
    pub new_branch_name: String,
    pub rename_target: Option<String>,
    pub rename_input: String,
    pub new_stash_message: String,
}

const LOG_PAGE_SIZE: usize = 200;

impl RepoSession {
    pub fn open(path: PathBuf, ctx: Context) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let (tx, rx) = worker::spawn(path.clone(), ctx);

        let session = Self {
            path,
            name,
            tx,
            rx,
            status: Vec::new(),
            commits: Vec::new(),
            selected_diff: None,
            commit_message: String::new(),
            last_commit: None,
            branches: Vec::new(),
            current_branch: None,
            stashes: Vec::new(),
            error: None,
            new_branch_name: String::new(),
            rename_target: None,
            rename_input: String::new(),
            new_stash_message: String::new(),
        };
        session.send(Command::RefreshStatus);
        session.send(Command::LoadLog {
            skip: 0,
            limit: LOG_PAGE_SIZE,
        });
        session.send(Command::LoadBranches);
        session.send(Command::LoadStashes);
        session
    }

    fn send(&self, cmd: Command) {
        let _ = self.tx.send(cmd);
    }

    pub fn stage(&self, path: String) {
        self.send(Command::Stage(path));
    }

    pub fn unstage(&self, path: String) {
        self.send(Command::Unstage(path));
    }

    pub fn select_diff(&mut self, path: String, staged: bool) {
        self.send(Command::LoadDiff {
            path: path.clone(),
            staged,
        });
        self.selected_diff = Some(SelectedDiff {
            path,
            staged,
            diff: None,
        });
    }

    pub fn commit(&mut self) {
        let message = self.commit_message.trim();
        if !message.is_empty() {
            self.send(Command::Commit(message.to_string()));
        }
    }

    pub fn create_branch(&self, name: String, start_point: Option<Oid>) {
        self.send(Command::CreateBranch { name, start_point });
    }

    pub fn delete_branch(&self, name: String) {
        self.send(Command::DeleteBranch(name));
    }

    pub fn rename_branch(&self, old_name: String, new_name: String) {
        self.send(Command::RenameBranch { old_name, new_name });
    }

    pub fn switch_branch(&self, name: String) {
        self.send(Command::SwitchBranch(name));
    }

    pub fn create_stash(&self, message: Option<String>) {
        self.send(Command::CreateStash { message });
    }

    pub fn apply_stash(&self, index: usize) {
        self.send(Command::ApplyStash(index));
    }

    pub fn pop_stash(&self, index: usize) {
        self.send(Command::PopStash(index));
    }

    pub fn drop_stash(&self, index: usize) {
        self.send(Command::DropStash(index));
    }

    pub fn poll_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Status(entries) => self.status = entries,
                Event::Log { skip, mut entries } => {
                    if skip == 0 {
                        self.commits = entries;
                    } else if skip == self.commits.len() {
                        self.commits.append(&mut entries);
                    }
                }
                Event::Diff { path, staged, diff } => {
                    if let Some(selected) = &mut self.selected_diff
                        && selected.path == path
                        && selected.staged == staged
                    {
                        selected.diff = Some(diff);
                    }
                }
                Event::Committed(oid) => {
                    self.last_commit = Some(oid);
                    self.commit_message.clear();
                    self.selected_diff = None;
                    self.send(Command::RefreshStatus);
                    self.send(Command::LoadLog {
                        skip: 0,
                        limit: LOG_PAGE_SIZE,
                    });
                }
                Event::Branches(branches) => self.branches = branches,
                Event::BranchSwitched(name) => {
                    self.current_branch = Some(name);
                    self.selected_diff = None;
                    // HEAD moved: status, history, and the branch list's
                    // `is_head` flags are all stale — reload the same way
                    // `Event::Committed` reloads status/log above.
                    self.send(Command::RefreshStatus);
                    self.send(Command::LoadLog {
                        skip: 0,
                        limit: LOG_PAGE_SIZE,
                    });
                    self.send(Command::LoadBranches);
                }
                Event::Stashes(stashes) => self.stashes = stashes,
                Event::StashCreated => {
                    self.send(Command::RefreshStatus);
                    self.send(Command::LoadStashes);
                }
                Event::Error(message) => self.error = Some(message),
            }
        }
    }
}

pub struct AppState {
    pub config: config::ConfigStore,
    pub sessions: Vec<RepoSession>,
    pub active: Option<usize>,
}

impl AppState {
    pub fn new() -> Self {
        let config = config::ConfigStore::load().unwrap_or_else(|_| {
            config::ConfigStore::load_from(std::env::temp_dir().join("browsitory-config.toml"))
                .expect("temp dir is always writable")
        });
        Self {
            config,
            sessions: Vec::new(),
            active: None,
        }
    }

    pub fn open_repo(&mut self, path: PathBuf, ctx: Context) {
        if let Some(index) = self.sessions.iter().position(|s| s.path == path) {
            self.active = Some(index);
            return;
        }
        let _ = self.config.add_repo(path.clone());
        self.sessions.push(RepoSession::open(path, ctx));
        self.active = Some(self.sessions.len() - 1);
    }

    pub fn active_session(&mut self) -> Option<&mut RepoSession> {
        self.active.and_then(move |i| self.sessions.get_mut(i))
    }
}
