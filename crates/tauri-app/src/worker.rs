use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::thread;

use git_core::branch::BranchInfo;
use git_core::diff::DiffHunk;
use git_core::log::CommitInfo;
use git_core::status::StatusEntry;

pub(crate) enum Command {
    GetStatus {
        reply: Sender<Result<Vec<StatusEntry>, String>>,
    },
    GetLog {
        limit: usize,
        reply: Sender<Result<Vec<CommitInfo>, String>>,
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
            let repo = repo;
            for command in rx {
                match command {
                    Command::GetStatus { reply } => {
                        let result = git_core::status::status(&repo).map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::GetLog { limit, reply } => {
                        let result = git_core::log::log(&repo, limit).map_err(|e| e.to_string());
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
                        let result =
                            git_core::commit::commit(&repo, &message).map_err(|e| e.to_string());
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
                        let result =
                            git_core::branch::rename_branch(&repo, &old_name, &new_name)
                                .map_err(|e| e.to_string());
                        let _ = reply.send(result);
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
    pub fn get_status(&self) -> Result<Vec<StatusEntry>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetStatus { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn get_log(&self, limit: usize) -> Result<Vec<CommitInfo>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetLog {
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
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use git2::Repository;
    use tempfile::TempDir;

    use super::Worker;

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
    fn get_log_reflects_a_commit() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "file.txt", "hello");
        commit_all(&repo, "initial commit");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let commits = worker.handle().get_log(10).unwrap();

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

        assert!(
            !handle
                .list_branches()
                .unwrap()
                .iter()
                .any(|b| b.name == "feature")
        );
    }
}
