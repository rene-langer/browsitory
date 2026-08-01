use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

use egui::Context;
use git_core::{
    BlameLine, BranchInfo, CommitInfo, FileDiff, FileState, FileStatus, GraphCommit, MergeOutcome,
    Oid, RebaseAction, RebaseStatus, RebaseStep, StashEntry,
};

use crate::worker::{self, Command, Event, ResolvedWith};

pub struct SelectedDiff {
    pub path: String,
    pub staged: bool,
    pub diff: Option<FileDiff>,
}

/// A conflicted path's two sides, loaded for the side-by-side conflict view.
pub struct ConflictDiffView {
    pub path: String,
    pub ours: FileDiff,
    pub theirs: FileDiff,
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
    pub blame: Option<(String, Vec<BlameLine>)>,
    pub graph: Option<Vec<GraphCommit>>,
    /// Currently-selected commit id, shared by the history list and the
    /// graph view so clicking a commit in either highlights it in both.
    pub selected_commit: Option<Oid>,

    /// Paths with unresolved conflicts, populated after a `MergeResult`
    /// (or a rebase step) comes back with `Conflict(paths)`.
    pub merge_conflicts: Vec<String>,
    pub active_conflict: Option<ConflictDiffView>,
    pub merge_target: String,
    pub last_merge_outcome: Option<MergeOutcome>,

    /// The user-editable plan (commit + chosen action per row) shown by
    /// `rebase_planner.rs`, populated from a `RebasePlan` preview event and
    /// mutated locally as the user picks actions, before `StartRebase` is
    /// ever sent.
    pub rebase_plan: Option<Vec<RebaseStep>>,
    /// Index into `rebase_plan` of the step currently being driven — needed
    /// because `Command::RebaseStep`/`ContinueRebaseEdit` report progress by
    /// step index, not by echoing the plan back.
    pub rebase_cursor: usize,
    pub rebase_progress: Option<RebaseStatus>,
    pub rebase_upstream: String,
    pub rebase_onto: String,
    pub rebase_active: bool,
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
            blame: None,
            graph: None,
            selected_commit: None,
            merge_conflicts: Vec::new(),
            active_conflict: None,
            merge_target: String::new(),
            last_merge_outcome: None,
            rebase_plan: None,
            rebase_cursor: 0,
            rebase_progress: None,
            rebase_upstream: String::new(),
            rebase_onto: String::new(),
            rebase_active: false,
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

    pub fn load_blame(&mut self, path: String) {
        self.send(Command::LoadBlame(path));
    }

    pub fn load_graph(&mut self, max_count: usize) {
        self.send(Command::LoadGraph { max_count });
    }

    // --- Merge / conflict resolution -----------------------------------

    pub fn start_merge(&mut self, their_branch: String) {
        self.merge_target = their_branch.clone();
        self.send(Command::MergeBranch(their_branch));
    }

    pub fn abort_merge(&mut self) {
        self.merge_conflicts.clear();
        self.active_conflict = None;
        self.send(Command::AbortMerge);
    }

    pub fn select_conflict(&mut self, path: String) {
        self.send(Command::LoadConflict(path));
    }

    pub fn resolve_conflict(&mut self, path: String, resolution: ResolvedWith) {
        self.send(Command::ResolveConflict { path, resolution });
    }

    pub fn is_merging(&self) -> bool {
        !self.merge_conflicts.is_empty()
    }

    // --- Interactive rebase ----------------------------------------------

    pub fn load_rebase_plan(&mut self, upstream: String) {
        self.rebase_upstream = upstream.clone();
        self.send(Command::LoadRebasePlan(upstream));
    }

    pub fn set_rebase_action(&mut self, index: usize, action: RebaseAction) {
        if let Some(plan) = &mut self.rebase_plan
            && let Some(step) = plan.get_mut(index)
        {
            step.action = action;
        }
    }

    pub fn start_rebase(&mut self) {
        let Some(plan) = &self.rebase_plan else {
            return;
        };
        if plan.is_empty() {
            return;
        }
        self.rebase_cursor = 0;
        self.rebase_active = true;
        let onto = if self.rebase_onto.trim().is_empty() {
            None
        } else {
            Some(self.rebase_onto.trim().to_string())
        };
        self.send(Command::StartRebase {
            upstream: self.rebase_upstream.clone(),
            onto,
        });
    }

    pub fn continue_rebase(&mut self) {
        self.send(Command::ContinueRebaseEdit);
    }

    pub fn abort_rebase(&mut self) {
        self.rebase_active = false;
        self.rebase_plan = None;
        self.rebase_progress = None;
        self.merge_conflicts.clear();
        self.active_conflict = None;
        self.send(Command::AbortRebase);
    }

    /// Sends the `RebaseStep` command for whatever row `rebase_cursor`
    /// currently points at. Called once after `RebaseStarted`, and again
    /// after each `StepComplete` to auto-advance through the plan without
    /// requiring the user to click through every non-interactive step.
    fn drive_current_rebase_step(&mut self) {
        let Some(plan) = &self.rebase_plan else {
            return;
        };
        let Some(step) = plan.get(self.rebase_cursor) else {
            return;
        };
        self.send(Command::RebaseStep(step.action.clone()));
    }

    pub fn poll_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Status(entries) => {
                    // Recomputed from ground truth on every status refresh
                    // (rather than only mutated by MergeResult/RebaseProgress)
                    // so it naturally clears as the user resolves conflicts
                    // one-by-one via `stage_path` — a resolved path drops its
                    // `FileState::Conflicted` staged state the moment it's
                    // re-added to the index.
                    self.merge_conflicts = entries
                        .iter()
                        .filter(|f| f.staged == Some(FileState::Conflicted))
                        .map(|f| f.path.clone())
                        .collect();
                    if let Some(conflict) = &self.active_conflict
                        && !self.merge_conflicts.contains(&conflict.path)
                    {
                        self.active_conflict = None;
                    }
                    self.status = entries;
                }
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
                    self.merge_conflicts.clear();
                    self.active_conflict = None;
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
                Event::Blame { path, lines } => self.blame = Some((path, lines)),
                Event::Graph(commits) => self.graph = Some(commits),

                Event::MergeResult(outcome) => {
                    self.last_merge_outcome = Some(outcome.clone());
                    match outcome {
                        MergeOutcome::Conflict(paths) => self.merge_conflicts = paths,
                        MergeOutcome::UpToDate => {}
                        MergeOutcome::FastForward | MergeOutcome::Merged => {
                            self.merge_conflicts.clear();
                        }
                    }
                    self.send(Command::RefreshStatus);
                    self.send(Command::LoadLog {
                        skip: 0,
                        limit: LOG_PAGE_SIZE,
                    });
                }
                Event::ConflictDiff { path, ours, theirs } => {
                    self.active_conflict = Some(ConflictDiffView { path, ours, theirs });
                }

                Event::RebasePlan(commits) => {
                    self.rebase_plan = Some(
                        commits
                            .into_iter()
                            .map(|c| RebaseStep {
                                commit: c.id,
                                action: RebaseAction::Pick,
                            })
                            .collect(),
                    );
                }
                Event::RebaseStarted { steps } => {
                    // Reconcile with whatever actions the user already chose
                    // in the planner, keyed by commit id, rather than
                    // clobbering them with fresh all-Pick defaults.
                    let chosen: std::collections::HashMap<Oid, RebaseAction> = self
                        .rebase_plan
                        .take()
                        .into_iter()
                        .flatten()
                        .map(|s| (s.commit, s.action))
                        .collect();
                    self.rebase_plan = Some(
                        steps
                            .into_iter()
                            .map(|c| RebaseStep {
                                commit: c.id,
                                action: chosen.get(&c.id).cloned().unwrap_or(RebaseAction::Pick),
                            })
                            .collect(),
                    );
                    self.rebase_cursor = 0;
                    self.merge_conflicts.clear();
                    self.active_conflict = None;
                    self.drive_current_rebase_step();
                }
                Event::RebaseProgress(status) => {
                    self.rebase_progress = Some(status.clone());
                    match status {
                        RebaseStatus::Conflict { paths, .. } => {
                            self.merge_conflicts = paths;
                            self.active_conflict = None;
                        }
                        RebaseStatus::PausedForEdit { .. } => {
                            self.merge_conflicts.clear();
                        }
                        RebaseStatus::StepComplete { .. } => {
                            self.merge_conflicts.clear();
                            self.active_conflict = None;
                            self.rebase_cursor += 1;
                            self.send(Command::RefreshStatus);
                            self.drive_current_rebase_step();
                        }
                        RebaseStatus::Done => {}
                    }
                }
                Event::RebaseFinished => {
                    self.rebase_active = false;
                    self.rebase_plan = None;
                    self.rebase_progress = None;
                    self.merge_conflicts.clear();
                    self.active_conflict = None;
                    self.send(Command::RefreshStatus);
                    self.send(Command::LoadLog {
                        skip: 0,
                        limit: LOG_PAGE_SIZE,
                    });
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
