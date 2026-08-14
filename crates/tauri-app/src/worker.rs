use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::thread;

use git_core::blame::BlameLine;
use git_core::branch::BranchInfo;
use git_core::diff::DiffHunk;
use git_core::graph::GraphCommit;
use git_core::merge::{ConflictSegment, FileConflictChoice, MergeOutcome};
use git_core::rebase::{RebasePlanCommit, RebasePlanEntry, RebaseState, RebaseStepResult};
use git_core::remote::{RemoteInfo, UpstreamInfo};
use git_core::stash::StashEntry;
use git_core::status::StatusEntry;

static NEXT_TRANSFER_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TransferEvent {
    Started {
        operation_id: String,
    },
    Progress(git_core::remote::TransferProgress),
    Completed {
        operation_id: String,
        error: Option<String>,
    },
}

struct ChannelReporter(Sender<TransferEvent>);

impl git_core::remote::TransferReporter for ChannelReporter {
    fn report(&mut self, progress: git_core::remote::TransferProgress) {
        let _ = self.0.send(TransferEvent::Progress(progress));
    }
}

struct NoCredentials;

impl git_core::remote::CredentialProvider for NoCredentials {
    fn credential(
        &mut self,
        _url: &str,
        _username: Option<&str>,
        _allowed: git2::CredentialType,
    ) -> Result<git2::Cred, git2::Error> {
        Err(git2::Error::from_str(
            "authentication is not configured for this remote",
        ))
    }
}

pub(crate) enum Command {
    GetStatus {
        reply: Sender<Result<Vec<StatusEntry>, String>>,
    },
    GetCommitGraph {
        limit: usize,
        reply: Sender<Result<Vec<GraphCommit>, String>>,
    },
    GetWorkingDiff {
        path: String,
        staged: bool,
        reply: Sender<Result<Vec<DiffHunk>, String>>,
    },
    GetCommitDiff {
        commit_id: String,
        path: String,
        reply: Sender<Result<Vec<DiffHunk>, String>>,
    },
    GetCommitFiles {
        commit_id: String,
        reply: Sender<Result<Vec<String>, String>>,
    },
    GetBlame {
        commit_id: String,
        path: String,
        reply: Sender<Result<Vec<BlameLine>, String>>,
    },
    StageFile {
        path: String,
        reply: Sender<Result<(), String>>,
    },
    UnstageFile {
        path: String,
        reply: Sender<Result<(), String>>,
    },
    Commit {
        message: String,
        reply: Sender<Result<String, String>>,
    },
    ListBranches {
        reply: Sender<Result<Vec<BranchInfo>, String>>,
    },
    CreateBranch {
        name: String,
        start_point: String,
        reply: Sender<Result<(), String>>,
    },
    SwitchBranch {
        name: String,
        reply: Sender<Result<(), String>>,
    },
    DeleteBranch {
        name: String,
        force: bool,
        reply: Sender<Result<(), String>>,
    },
    RenameBranch {
        old_name: String,
        new_name: String,
        reply: Sender<Result<(), String>>,
    },
    ListRemotes {
        reply: Sender<Result<Vec<RemoteInfo>, String>>,
    },
    GetCurrentUpstream {
        reply: Sender<Result<Option<UpstreamInfo>, String>>,
    },
    GetRemoteUpstreams {
        name: String,
        reply: Sender<Result<Vec<UpstreamInfo>, String>>,
    },
    AddRemote {
        name: String,
        fetch_url: String,
        push_url: Option<String>,
        reply: Sender<Result<(), String>>,
    },
    RenameRemote {
        old_name: String,
        new_name: String,
        reply: Sender<Result<(), String>>,
    },
    UpdateRemoteUrls {
        name: String,
        fetch_url: String,
        push_url: Option<String>,
        reply: Sender<Result<(), String>>,
    },
    RemoveRemote {
        name: String,
        clear_upstreams: bool,
        reply: Sender<Result<(), String>>,
    },
    SetCurrentUpstream {
        remote_name: String,
        remote_branch: String,
        reply: Sender<Result<(), String>>,
    },
    ClearCurrentUpstream {
        reply: Sender<Result<(), String>>,
    },
    ListStashes {
        reply: Sender<Result<Vec<StashEntry>, String>>,
    },
    SaveStash {
        reply: Sender<Result<(), String>>,
    },
    ApplyStash {
        index: usize,
        reply: Sender<Result<(), String>>,
    },
    DropStash {
        index: usize,
        reply: Sender<Result<(), String>>,
    },
    #[allow(dead_code)]
    StartMerge {
        branch_name: String,
        reply: Sender<Result<MergeOutcome, String>>,
    },
    #[allow(dead_code)]
    GetConflictHunks {
        path: String,
        reply: Sender<Result<Vec<ConflictSegment>, String>>,
    },
    #[allow(dead_code)]
    ResolveConflict {
        path: String,
        resolved_content: String,
        reply: Sender<Result<(), String>>,
    },
    #[allow(dead_code)]
    AbortMerge {
        reply: Sender<Result<(), String>>,
    },
    #[allow(dead_code)]
    GetMergeMessage {
        reply: Sender<Result<Option<String>, String>>,
    },
    ResolveAddDeleteConflict {
        path: String,
        choice: FileConflictChoice,
        reply: Sender<Result<(), String>>,
    },
    #[allow(dead_code)]
    CommitsSince {
        onto: String,
        reply: Sender<Result<Vec<RebasePlanCommit>, String>>,
    },
    #[allow(dead_code)]
    StartRebase {
        onto: String,
        plan: Vec<RebasePlanEntry>,
        reply: Sender<Result<RebaseStepResult, String>>,
    },
    #[allow(dead_code)]
    RebaseContinue {
        reply: Sender<Result<RebaseStepResult, String>>,
    },
    #[allow(dead_code)]
    AbortRebase {
        reply: Sender<Result<(), String>>,
    },
    #[allow(dead_code)]
    GetRebaseProgress {
        reply: Sender<Result<Option<(usize, usize)>, String>>,
    },
    FetchRemote {
        remote_name: String,
        operation_id: String,
        events: Sender<TransferEvent>,
        reply: Sender<Result<String, String>>,
    },
}

pub struct Worker {
    tx: Sender<Command>,
}

/// Cheap, cloneable handle to a `Worker`'s command channel.
///
/// Callers clone this out of shared state and drop the lock *before* blocking on a
/// reply, so a slow (or wedged) repository operation can't serialize unrelated commands.
#[derive(Clone)]
pub struct WorkerHandle {
    tx: Sender<Command>,
}

impl Worker {
    pub fn spawn(path: PathBuf) -> Result<Self, String> {
        let repo = git_core::repo::open(&path).map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel::<Command>();

        thread::spawn(move || {
            let mut repo = repo;
            let mut rebase_state: Option<RebaseState> = None;
            for command in rx {
                match command {
                    Command::GetStatus { reply } => {
                        let result = git_core::status::status(&repo).map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::GetCommitGraph { limit, reply } => {
                        let result =
                            git_core::graph::graph_log(&repo, limit).map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::GetWorkingDiff {
                        path,
                        staged,
                        reply,
                    } => {
                        let result = git_core::diff::working_diff(&repo, &path, staged)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::GetCommitDiff {
                        commit_id,
                        path,
                        reply,
                    } => {
                        let result = git_core::diff::commit_diff(&repo, &commit_id, &path)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::GetCommitFiles { commit_id, reply } => {
                        let result = git_core::diff::commit_files(&repo, &commit_id)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::GetBlame {
                        commit_id,
                        path,
                        reply,
                    } => {
                        let result = git_core::blame::blame_file(&repo, &commit_id, &path)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::StageFile { path, reply } => {
                        let result =
                            git_core::stage::stage_file(&repo, &path).map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::UnstageFile { path, reply } => {
                        let result =
                            git_core::stage::unstage_file(&repo, &path).map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::Commit { message, reply } => {
                        let result = git_core::commit::commit(&mut repo, &message)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::ListBranches { reply } => {
                        let result =
                            git_core::branch::list_branches(&repo).map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::CreateBranch {
                        name,
                        start_point,
                        reply,
                    } => {
                        let result = git_core::branch::create_branch(&repo, &name, &start_point)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::SwitchBranch { name, reply } => {
                        let result = git_core::branch::switch_branch(&repo, &name)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::DeleteBranch { name, force, reply } => {
                        let result = git_core::branch::delete_branch(&repo, &name, force)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::RenameBranch {
                        old_name,
                        new_name,
                        reply,
                    } => {
                        let result = git_core::branch::rename_branch(&repo, &old_name, &new_name)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::ListRemotes { reply } => {
                        let result =
                            git_core::remote::list_remotes(&repo).map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::GetCurrentUpstream { reply } => {
                        let result =
                            git_core::remote::current_upstream(&repo).map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::AddRemote {
                        name,
                        fetch_url,
                        push_url,
                        reply,
                    } => {
                        let result = git_core::remote::add_remote(
                            &repo,
                            &name,
                            &fetch_url,
                            push_url.as_deref(),
                        )
                        .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::RenameRemote {
                        old_name,
                        new_name,
                        reply,
                    } => {
                        let result = git_core::remote::rename_remote(&repo, &old_name, &new_name)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::UpdateRemoteUrls {
                        name,
                        fetch_url,
                        push_url,
                        reply,
                    } => {
                        let result = git_core::remote::update_remote_urls(
                            &repo,
                            &name,
                            &fetch_url,
                            push_url.as_deref(),
                        )
                        .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::GetRemoteUpstreams { name, reply } => {
                        let result = git_core::remote::remote_upstreams(&repo, &name)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::RemoveRemote {
                        name,
                        clear_upstreams,
                        reply,
                    } => {
                        let result = (if clear_upstreams {
                            git_core::remote::remove_remote_and_clear_upstreams(&repo, &name)
                        } else {
                            git_core::remote::remove_remote(&repo, &name)
                        })
                        .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::SetCurrentUpstream {
                        remote_name,
                        remote_branch,
                        reply,
                    } => {
                        let result = git_core::remote::set_current_upstream(
                            &repo,
                            &remote_name,
                            &remote_branch,
                        )
                        .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::ClearCurrentUpstream { reply } => {
                        let result = git_core::remote::clear_current_upstream(&repo)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::ListStashes { reply } => {
                        let result =
                            git_core::stash::list_stashes(&mut repo).map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::SaveStash { reply } => {
                        let result =
                            git_core::stash::save_stash(&mut repo).map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::ApplyStash { index, reply } => {
                        let result = git_core::stash::apply_stash(&mut repo, index)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::DropStash { index, reply } => {
                        let result = git_core::stash::drop_stash(&mut repo, index)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::StartMerge { branch_name, reply } => {
                        let result = git_core::merge::start_merge(&repo, &branch_name)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::GetConflictHunks { path, reply } => {
                        let result = git_core::merge::conflict_hunks(&repo, &path)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::ResolveConflict {
                        path,
                        resolved_content,
                        reply,
                    } => {
                        let result =
                            git_core::merge::resolve_conflict(&repo, &path, &resolved_content)
                                .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::AbortMerge { reply } => {
                        let result = git_core::merge::abort_merge(&repo).map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::GetMergeMessage { reply } => {
                        let result: Result<Option<String>, String> =
                            Ok(git_core::merge::merge_message(&repo));
                        let _ = reply.send(result);
                    }
                    Command::ResolveAddDeleteConflict {
                        path,
                        choice,
                        reply,
                    } => {
                        let result =
                            git_core::merge::resolve_add_delete_conflict(&repo, &path, choice)
                                .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::CommitsSince { onto, reply } => {
                        let result = git_core::rebase::commits_since(&repo, &onto)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::StartRebase { onto, plan, reply } => {
                        let result = git_core::rebase::start_rebase(&repo, &onto, plan)
                            .map_err(|e| e.to_string())
                            .map(|(state, step)| {
                                if !matches!(step, RebaseStepResult::Done) {
                                    rebase_state = Some(state);
                                }
                                step
                            });
                        let _ = reply.send(result);
                    }
                    Command::RebaseContinue { reply } => {
                        let result = match rebase_state.as_mut() {
                            Some(state) => git_core::rebase::rebase_continue(&repo, state)
                                .map_err(|e| e.to_string()),
                            None => Err("no rebase is currently in progress".to_string()),
                        };
                        if matches!(result, Ok(RebaseStepResult::Done)) {
                            rebase_state = None;
                        }
                        let _ = reply.send(result);
                    }
                    Command::AbortRebase { reply } => {
                        let result = match rebase_state.take() {
                            Some(state) => git_core::rebase::abort_rebase(&repo, state)
                                .map_err(|e| e.to_string()),
                            None => Err("no rebase is currently in progress".to_string()),
                        };
                        let _ = reply.send(result);
                    }
                    Command::GetRebaseProgress { reply } => {
                        let progress = rebase_state
                            .as_ref()
                            .map(|s| (s.current_step(), s.total_steps()));
                        let _ = reply.send(Ok(progress));
                    }
                    Command::FetchRemote {
                        remote_name,
                        operation_id,
                        events,
                        reply,
                    } => {
                        let _ = events.send(TransferEvent::Started {
                            operation_id: operation_id.clone(),
                        });
                        let _ = reply.send(Ok(operation_id.clone()));

                        let mut credentials = NoCredentials;
                        let mut reporter = ChannelReporter(events.clone());
                        let result = git_core::remote::fetch_remote(
                            &repo,
                            &remote_name,
                            operation_id.clone(),
                            &mut credentials,
                            &mut reporter,
                        );
                        let _ = events.send(TransferEvent::Completed {
                            operation_id,
                            // libgit2 may include remote-provided text in an error. Keep events
                            // secret-free and let a later credential workflow surface remediation.
                            error: result.err().map(|_| "fetch failed".to_string()),
                        });
                    }
                }
            }
        });

        Ok(Worker { tx })
    }

    /// A cloneable handle to this worker, cheap enough to take out of a mutex guard.
    pub fn handle(&self) -> WorkerHandle {
        WorkerHandle {
            tx: self.tx.clone(),
        }
    }
}

impl WorkerHandle {
    pub(crate) fn fetch_remote(
        &self,
        remote_name: String,
        events: Sender<TransferEvent>,
    ) -> Result<String, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        let operation_id = format!("fetch-{}", NEXT_TRANSFER_ID.fetch_add(1, Ordering::Relaxed));
        self.tx
            .send(Command::FetchRemote {
                remote_name,
                operation_id,
                events,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_status(&self) -> Result<Vec<StatusEntry>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetStatus { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_commit_graph(&self, limit: usize) -> Result<Vec<GraphCommit>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetCommitGraph {
                limit,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_working_diff(&self, path: String, staged: bool) -> Result<Vec<DiffHunk>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetWorkingDiff {
                path,
                staged,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_commit_diff(
        &self,
        commit_id: String,
        path: String,
    ) -> Result<Vec<DiffHunk>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetCommitDiff {
                commit_id,
                path,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_commit_files(&self, commit_id: String) -> Result<Vec<String>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetCommitFiles {
                commit_id,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_blame(&self, commit_id: String, path: String) -> Result<Vec<BlameLine>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetBlame {
                commit_id,
                path,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn stage_file(&self, path: String) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::StageFile {
                path,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn unstage_file(&self, path: String) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::UnstageFile {
                path,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn commit(&self, message: String) -> Result<String, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::Commit {
                message,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn list_branches(&self) -> Result<Vec<BranchInfo>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::ListBranches { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn create_branch(&self, name: String, start_point: String) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::CreateBranch {
                name,
                start_point,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn switch_branch(&self, name: String) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::SwitchBranch {
                name,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn delete_branch(&self, name: String, force: bool) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::DeleteBranch {
                name,
                force,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn rename_branch(&self, old_name: String, new_name: String) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::RenameBranch {
                old_name,
                new_name,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn list_remotes(&self) -> Result<Vec<RemoteInfo>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::ListRemotes { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_current_upstream(&self) -> Result<Option<UpstreamInfo>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetCurrentUpstream { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn get_remote_upstreams(&self, name: String) -> Result<Vec<UpstreamInfo>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetRemoteUpstreams {
                name,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn add_remote(
        &self,
        name: String,
        fetch_url: String,
        push_url: Option<String>,
    ) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::AddRemote {
                name,
                fetch_url,
                push_url,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn rename_remote(&self, old_name: String, new_name: String) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::RenameRemote {
                old_name,
                new_name,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn update_remote_urls(
        &self,
        name: String,
        fetch_url: String,
        push_url: Option<String>,
    ) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::UpdateRemoteUrls {
                name,
                fetch_url,
                push_url,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn remove_remote(&self, name: String, clear_upstreams: bool) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::RemoveRemote {
                name,
                clear_upstreams,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn set_current_upstream(
        &self,
        remote_name: String,
        remote_branch: String,
    ) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::SetCurrentUpstream {
                remote_name,
                remote_branch,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn clear_current_upstream(&self) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::ClearCurrentUpstream { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn list_stashes(&self) -> Result<Vec<StashEntry>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::ListStashes { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn save_stash(&self) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::SaveStash { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn apply_stash(&self, index: usize) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::ApplyStash {
                index,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn drop_stash(&self, index: usize) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::DropStash {
                index,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    #[allow(dead_code)]
    pub fn start_merge(&self, branch_name: String) -> Result<MergeOutcome, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::StartMerge {
                branch_name,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    #[allow(dead_code)]
    pub fn get_conflict_hunks(&self, path: String) -> Result<Vec<ConflictSegment>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetConflictHunks {
                path,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    #[allow(dead_code)]
    pub fn resolve_conflict(&self, path: String, resolved_content: String) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::ResolveConflict {
                path,
                resolved_content,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    #[allow(dead_code)]
    pub fn abort_merge(&self) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::AbortMerge { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    #[allow(dead_code)]
    pub fn get_merge_message(&self) -> Result<Option<String>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetMergeMessage { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn resolve_add_delete_conflict(
        &self,
        path: String,
        choice: FileConflictChoice,
    ) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::ResolveAddDeleteConflict {
                path,
                choice,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    #[allow(dead_code)]
    pub fn commits_since(&self, onto: String) -> Result<Vec<RebasePlanCommit>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::CommitsSince {
                onto,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    #[allow(dead_code)]
    pub fn start_rebase(
        &self,
        onto: String,
        plan: Vec<RebasePlanEntry>,
    ) -> Result<RebaseStepResult, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::StartRebase {
                onto,
                plan,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    #[allow(dead_code)]
    pub fn rebase_continue(&self) -> Result<RebaseStepResult, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::RebaseContinue { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    #[allow(dead_code)]
    pub fn abort_rebase(&self) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::AbortRebase { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    #[allow(dead_code)]
    pub fn get_rebase_progress(&self) -> Result<Option<(usize, usize)>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetRebaseProgress { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::mpsc;

    use git2::Repository;
    use tempfile::TempDir;

    use super::{TransferEvent, Worker};

    fn init_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().expect("create temp dir");
        let repo = Repository::init(dir.path()).expect("init repo");
        {
            let mut config = repo.config().expect("repo config");
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
        }
        (dir, repo)
    }

    fn write_file(dir: &Path, relative_path: &str, contents: &str) {
        std::fs::write(dir.join(relative_path), contents).expect("write file");
    }

    /// Stages everything in the worktree and commits it on `HEAD`, creating the first commit
    /// when there is none yet. Mirrors `crates/git-core/tests/common/mod.rs::commit_all`.
    fn commit_all(repo: &Repository, message: &str) {
        let mut index = repo.index().expect("open index");
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .expect("stage all");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let signature = repo.signature().expect("signature");

        let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();

        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parents,
        )
        .expect("commit");
    }

    fn local_and_bare_remote() -> (TempDir, TempDir, TempDir) {
        let (source_dir, source) = init_repo();
        write_file(source_dir.path(), "README.md", "initial commit\n");
        commit_all(&source, "initial commit");

        let remote_dir = TempDir::new().expect("create bare remote");
        let remote = Repository::init_bare(remote_dir.path()).expect("init bare remote");
        let branch = source
            .head()
            .expect("source head")
            .shorthand()
            .expect("source branch")
            .to_string();
        let branch_ref = format!("refs/heads/{branch}");
        source
            .remote("origin", remote_dir.path().to_str().expect("remote path"))
            .expect("add source remote");
        source
            .find_remote("origin")
            .expect("source remote")
            .push(&[format!("{branch_ref}:{branch_ref}")], None)
            .expect("push source commit");
        drop(remote);

        let (local_dir, local) = init_repo();
        local
            .remote("origin", remote_dir.path().to_str().expect("remote path"))
            .expect("add local remote");

        (source_dir, remote_dir, local_dir)
    }

    #[test]
    fn get_status_reflects_an_untracked_file() {
        let (dir, _repo) = init_repo();
        write_file(dir.path(), "untracked.txt", "hello");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let entries = worker.handle().get_status().unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "untracked.txt");
    }

    #[test]
    fn spawn_fails_on_a_non_repository_path() {
        let dir = TempDir::new().unwrap();

        let result = Worker::spawn(dir.path().to_path_buf());

        assert!(result.is_err());
    }

    #[test]
    fn get_commit_graph_reflects_a_commit() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "hello");
        commit_all(&repo, "initial commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let commits = worker.handle().get_commit_graph(10).unwrap();

        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].summary, "initial commit");
    }

    #[test]
    fn stage_then_commit_round_trips_through_the_worker() {
        let (dir, _repo) = init_repo();
        write_file(dir.path(), "new.txt", "hello");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.stage_file("new.txt".into()).unwrap();
        let result = handle.commit("message".into());

        assert!(result.is_ok());
        assert!(handle.get_status().unwrap().is_empty());
    }

    #[test]
    fn list_branches_reflects_the_initial_branch_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let branches = worker.handle().list_branches().unwrap();

        assert_eq!(branches.len(), 1);
        assert!(branches[0].is_current);
    }

    #[test]
    fn create_then_switch_branch_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .create_branch("feature".into(), "HEAD".into())
            .unwrap();

        let branches = handle.list_branches().unwrap();
        let feature = branches.iter().find(|b| b.name == "feature").unwrap();
        assert!(feature.is_current);

        let initial_branch_name = branches
            .iter()
            .find(|b| b.name != "feature")
            .unwrap()
            .name
            .clone();
        handle.switch_branch(initial_branch_name.clone()).unwrap();

        let branches_after = handle.list_branches().unwrap();
        assert!(
            branches_after
                .iter()
                .find(|b| b.name == initial_branch_name)
                .unwrap()
                .is_current
        );
    }

    #[test]
    fn rename_branch_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let initial_branch_name = handle.list_branches().unwrap()[0].name.clone();

        handle
            .rename_branch(initial_branch_name, "renamed".into())
            .unwrap();

        let branches = handle.list_branches().unwrap();
        assert_eq!(branches[0].name, "renamed");
    }

    #[test]
    fn delete_branch_with_force_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .create_branch("feature".into(), "HEAD".into())
            .unwrap();
        let initial_branch_name = handle
            .list_branches()
            .unwrap()
            .into_iter()
            .find(|b| b.name != "feature")
            .unwrap()
            .name;
        handle.switch_branch(initial_branch_name).unwrap();

        handle.delete_branch("feature".into(), true).unwrap();

        assert!(!handle
            .list_branches()
            .unwrap()
            .iter()
            .any(|b| b.name == "feature"));
    }

    #[test]
    fn save_then_list_stash_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");
        write_file(dir.path(), "file.txt", "v2");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.save_stash().unwrap();

        let stashes = handle.list_stashes().unwrap();
        assert_eq!(stashes.len(), 1);
    }

    #[test]
    fn remote_and_current_upstream_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");
        repo.remote("origin", "https://example.com/owner/repo.git")
            .unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("topic", &head, false).unwrap();
        let mut config = repo.config().unwrap();
        config.set_str("branch.topic.remote", "origin").unwrap();
        config
            .set_str("branch.topic.merge", "refs/heads/topic")
            .unwrap();
        drop(config);

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle
            .set_current_upstream("origin".into(), "main".into())
            .unwrap();

        assert_eq!(
            handle.list_remotes().unwrap(),
            vec![git_core::remote::RemoteInfo {
                name: "origin".into(),
                fetch_url: "https://example.com/owner/repo.git".into(),
                push_url: None,
            }]
        );
        assert_eq!(
            handle.get_current_upstream().unwrap(),
            Some(git_core::remote::UpstreamInfo {
                local_branch: repo.head().unwrap().shorthand().unwrap().into(),
                remote_name: "origin".into(),
                remote_branch: "main".into(),
            })
        );

        let mut affected_branches: Vec<_> = handle
            .get_remote_upstreams("origin".into())
            .unwrap()
            .into_iter()
            .map(|upstream| upstream.local_branch)
            .collect();
        affected_branches.sort();
        let mut expected_branches = vec![
            repo.head().unwrap().shorthand().unwrap().to_string(),
            "topic".to_string(),
        ];
        expected_branches.sort();
        assert_eq!(affected_branches, expected_branches);

        handle.remove_remote("origin".into(), true).unwrap();
        assert!(handle.list_remotes().unwrap().is_empty());
        assert_eq!(handle.get_current_upstream().unwrap(), None);
        assert!(handle
            .get_remote_upstreams("origin".into())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn transfer_fetch_streams_ordered_owned_events() {
        let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
        let worker = Worker::spawn(local_dir.path().to_path_buf()).expect("spawn worker");
        let (event_tx, event_rx) = mpsc::channel();

        let operation_id = worker
            .handle()
            .fetch_remote("origin".into(), event_tx)
            .expect("start fetch");
        let events: Vec<_> = event_rx.iter().collect();

        assert!(matches!(
            events.first(),
            Some(TransferEvent::Started { operation_id: id }) if id == &operation_id
        ));
        assert!(events.iter().any(|event| matches!(
            event,
            TransferEvent::Progress(progress) if progress.operation_id == operation_id
        )));
        assert!(matches!(
            events.last(),
            Some(TransferEvent::Completed { operation_id: id, error: None }) if id == &operation_id
        ));
    }

    #[test]
    fn apply_then_drop_stash_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "v1");
        commit_all(&repo, "initial commit");
        write_file(dir.path(), "file.txt", "v2");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.save_stash().unwrap();

        handle.apply_stash(0).unwrap();
        let contents = std::fs::read_to_string(dir.path().join("file.txt")).unwrap();
        assert_eq!(contents, "v2");

        handle.drop_stash(0).unwrap();
        assert!(handle.list_stashes().unwrap().is_empty());
    }

    #[test]
    fn get_blame_reflects_a_commit() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "hello\n");
        commit_all(&repo, "initial commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let lines = worker
            .handle()
            .get_blame("HEAD".into(), "file.txt".into())
            .unwrap();

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].content, "hello");
    }

    #[test]
    fn start_merge_and_get_merge_message_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "shared.txt", "line one\nline two\n");
        commit_all(&repo, "base commit");
        let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
            .name
            .clone();

        git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
        write_file(dir.path(), "shared.txt", "line one\nfeature two\n");
        commit_all(&repo, "feature commit");
        git_core::branch::switch_branch(&repo, &main_branch).unwrap();
        write_file(dir.path(), "shared.txt", "line one\nmain two\n");
        commit_all(&repo, "main commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let outcome = handle.start_merge("feature".into()).unwrap();

        assert!(matches!(
            outcome,
            git_core::merge::MergeOutcome::Conflicted { .. }
        ));
        assert!(handle
            .get_merge_message()
            .unwrap()
            .unwrap()
            .contains("feature"));
    }

    #[test]
    fn get_conflict_hunks_then_resolve_conflict_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "shared.txt", "line one\nline two\n");
        commit_all(&repo, "base commit");
        let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
            .name
            .clone();

        git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
        write_file(dir.path(), "shared.txt", "line one\nfeature two\n");
        commit_all(&repo, "feature commit");
        git_core::branch::switch_branch(&repo, &main_branch).unwrap();
        write_file(dir.path(), "shared.txt", "line one\nmain two\n");
        commit_all(&repo, "main commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.start_merge("feature".into()).unwrap();

        let segments = handle.get_conflict_hunks("shared.txt".into()).unwrap();
        assert!(!segments.is_empty());

        handle
            .resolve_conflict("shared.txt".into(), "line one\nresolved\n".into())
            .unwrap();

        // Resolving and staging a conflict does not empty the status list — the file now
        // differs from HEAD, so it shows as one staged entry, exactly like any other staged
        // change. It only leaves `status()` once actually committed.
        let status = handle.get_status().unwrap();
        assert_eq!(status.len(), 1);
        assert_eq!(status[0].path, "shared.txt");
        assert!(status[0].staged);
        assert_eq!(status[0].kind, git_core::status::StatusKind::Modified);
    }

    #[test]
    fn abort_merge_round_trips_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "shared.txt", "line one\nline two\n");
        commit_all(&repo, "base commit");
        let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
            .name
            .clone();

        git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
        write_file(dir.path(), "shared.txt", "line one\nfeature two\n");
        commit_all(&repo, "feature commit");
        git_core::branch::switch_branch(&repo, &main_branch).unwrap();
        write_file(dir.path(), "shared.txt", "line one\nmain two\n");
        commit_all(&repo, "main commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.start_merge("feature".into()).unwrap();

        handle.abort_merge().unwrap();

        assert!(handle.get_merge_message().unwrap().is_none());
        assert!(handle.get_status().unwrap().is_empty());
    }

    #[test]
    fn resolve_add_delete_conflict_round_trips_through_the_worker() {
        let (dir, mut repo) = init_repo();
        write_file(dir.path(), "shared.txt", "v1\n");
        commit_all(&repo, "base commit");
        let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
            .name
            .clone();

        git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
        write_file(dir.path(), "shared.txt", "v2\n");
        commit_all(&repo, "feature commit modifies");
        git_core::branch::switch_branch(&repo, &main_branch).unwrap();
        std::fs::remove_file(dir.path().join("shared.txt")).unwrap();
        git_core::stage::stage_file(&repo, "shared.txt").unwrap();
        git_core::commit::commit(&mut repo, "main commit deletes").unwrap();

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.start_merge("feature".into()).unwrap();

        handle
            .resolve_add_delete_conflict(
                "shared.txt".into(),
                git_core::merge::FileConflictChoice::Theirs,
            )
            .unwrap();

        let status = handle.get_status().unwrap();
        assert_eq!(status.len(), 1);
        assert!(status[0].staged);
    }

    #[test]
    fn commits_since_and_start_rebase_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "base.txt", "v1\n");
        commit_all(&repo, "base commit");
        let onto = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        write_file(dir.path(), "a.txt", "a\n");
        commit_all(&repo, "add a");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let commits = handle.commits_since(onto.clone()).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].summary, "add a");

        let plan = vec![git_core::rebase::RebasePlanEntry {
            commit_id: commits[0].id.clone(),
            action: git_core::rebase::RebaseAction::Pick,
            combined_message: None,
        }];
        let result = handle.start_rebase(onto, plan).unwrap();

        assert_eq!(result, git_core::rebase::RebaseStepResult::Done);
        assert_eq!(handle.get_rebase_progress().unwrap(), None);
    }

    #[test]
    fn rebase_continue_and_abort_rebase_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "base.txt", "v1\n");
        commit_all(&repo, "base commit");
        let onto = repo
            .head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .id()
            .to_string();
        write_file(dir.path(), "a.txt", "a\n");
        commit_all(&repo, "add a");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        let commits = handle.commits_since(onto.clone()).unwrap();
        let plan = vec![git_core::rebase::RebasePlanEntry {
            commit_id: commits[0].id.clone(),
            action: git_core::rebase::RebaseAction::Edit,
            combined_message: None,
        }];
        let result = handle.start_rebase(onto, plan).unwrap();
        assert_eq!(result, git_core::rebase::RebaseStepResult::PausedForEdit);
        // 1-indexed: the first (and here only) paused step reports "Step 1 of 1", not "0 of 1".
        assert_eq!(handle.get_rebase_progress().unwrap(), Some((1, 1)));

        handle.abort_rebase().unwrap();

        assert_eq!(handle.get_rebase_progress().unwrap(), None);
        assert!(handle.get_status().unwrap().is_empty());
    }
}
