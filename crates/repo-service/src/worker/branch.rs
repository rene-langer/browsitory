use super::{Command, WorkerHandle};
use git_core::branch::BranchInfo;
use std::sync::mpsc::Sender;
pub(super) fn list(repo: &git2::Repository, reply: Sender<Result<Vec<BranchInfo>, String>>) {
    let _ = reply.send(git_core::branch::list_branches(repo).map_err(|error| error.to_string()));
}
pub(super) fn create(
    repo: &git2::Repository,
    name: String,
    start_point: String,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::branch::create_branch(repo, &name, &start_point)
            .map_err(|error| error.to_string()),
    );
}
/// Switches branches; `post-checkout` then runs reported and cancellable on `op`, before the
/// reply. A cancel stops only the hook: the switch stands (#185).
pub(super) fn switch(
    repo: &git2::Repository,
    name: String,
    op: super::remote::TransferOp<'_>,
    events: Sender<super::HookEvent>,
    reply: Sender<Result<(), String>>,
) {
    let before = super::hooks::head_oid(repo);
    let result = git_core::branch::switch_branch(repo, &name).map_err(|error| error.to_string());
    if result.is_ok() {
        super::hooks::run_post_checkout(repo, before, &op, events);
    }
    op.clear();
    let _ = reply.send(result);
}
pub(super) fn delete(
    repo: &git2::Repository,
    name: String,
    force: bool,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::branch::delete_branch(repo, &name, force).map_err(|error| error.to_string()),
    );
}
pub(super) fn rename(
    repo: &git2::Repository,
    old_name: String,
    new_name: String,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::branch::rename_branch(repo, &old_name, &new_name)
            .map_err(|error| error.to_string()),
    );
}
impl WorkerHandle {
    pub fn list_branches(&self) -> Result<Vec<BranchInfo>, String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ListBranches { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn create_branch(&self, name: String, start_point: String) -> Result<(), String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
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
    /// Switches to `name`; `post-checkout` is reported on `events` (`HookSource::Checkout`)
    /// under a `switch-<n>` id that `cancel_transfer` stops (#185).
    pub fn switch_branch(
        &self,
        name: String,
        events: Sender<super::HookEvent>,
    ) -> Result<(), String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::SwitchBranch {
                name,
                operation_id: super::remote::next_operation_id("switch"),
                events,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn delete_branch(&self, name: String, force: bool) -> Result<(), String> {
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
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
        let (reply_tx, reply_rx) = std::sync::mpsc::channel();
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
