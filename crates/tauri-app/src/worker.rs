use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::thread;

use git_core::status::StatusEntry;

pub(crate) enum Command {
    GetStatus {
        reply: Sender<Result<Vec<StatusEntry>, String>>,
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
}
