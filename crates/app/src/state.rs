use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

use egui::Context;
use git_core::{BlameLine, CommitInfo, FileDiff, FileStatus, GraphCommit, Oid};

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
    pub error: Option<String>,
    pub blame: Option<(String, Vec<BlameLine>)>,
    pub graph: Option<Vec<GraphCommit>>,
    /// Currently-selected commit id, shared by the history list and the
    /// graph view so clicking a commit in either highlights it in both.
    pub selected_commit: Option<Oid>,
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
            error: None,
            blame: None,
            graph: None,
            selected_commit: None,
        };
        session.send(Command::RefreshStatus);
        session.send(Command::LoadLog {
            skip: 0,
            limit: LOG_PAGE_SIZE,
        });
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
        // Selecting a file's diff switches the central panel back from the
        // blame overlay to the diff view.
        self.blame = None;
    }

    pub fn select_commit(&mut self, id: Oid) {
        self.selected_commit = Some(id);
    }

    pub fn commit(&mut self) {
        let message = self.commit_message.trim();
        if !message.is_empty() {
            self.send(Command::Commit(message.to_string()));
        }
    }

    pub fn load_blame(&mut self, path: String) {
        self.send(Command::LoadBlame(path));
    }

    pub fn load_graph(&mut self, max_count: usize) {
        self.send(Command::LoadGraph { max_count });
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
                Event::Blame { path, lines } => self.blame = Some((path, lines)),
                Event::Graph(commits) => self.graph = Some(commits),
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
