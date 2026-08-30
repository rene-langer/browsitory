use super::{Command, WorkerHandle};
use git_core::forge::{ForgeProvider, ForgeRepository};
use std::sync::mpsc::Sender;

use crate::credentials::{CredentialService, CredentialStore};
use crate::pull_requests::{
    CreatePullRequest, ForgeApi, PullRequest, PullRequestList, PullRequestService,
};

fn resolve_repository(
    repo: &git2::Repository,
    remote_name: &str,
) -> Result<ForgeRepository, String> {
    git_core::forge::detect_forge_repositories(repo)
        .ok()
        .into_iter()
        .flatten()
        .find(|repository| repository.remote_name == remote_name)
        .ok_or_else(|| "this remote is not a supported forge repository".to_string())
}

pub(super) fn detect(repo: &git2::Repository, reply: Sender<Result<Vec<ForgeRepository>, String>>) {
    let repositories = git_core::forge::detect_forge_repositories(repo).unwrap_or_default();
    let _ = reply.send(Ok(repositories));
}

pub(super) fn save_token<S: CredentialStore>(
    credential_service: &CredentialService<S>,
    provider: ForgeProvider,
    account: String,
    token: String,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        credential_service
            .save_forge_token(provider, &account, &token)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn forget_token<S: CredentialStore>(
    credential_service: &CredentialService<S>,
    provider: ForgeProvider,
    account: String,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        credential_service
            .forget_forge_token(provider, &account)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn list_pull_requests<S: CredentialStore, A: ForgeApi>(
    repo: &git2::Repository,
    credential_service: &CredentialService<S>,
    pull_request_service: &PullRequestService<A>,
    remote_name: String,
    account: String,
    reply: Sender<Result<PullRequestList, String>>,
) {
    let result = (|| {
        let repository = resolve_repository(repo, &remote_name)?;
        let token = credential_service
            .lookup_forge_token(repository.provider, &account)
            .map_err(|error| error.to_string())?;
        pull_request_service
            .list_pull_requests(&repository, token.as_deref())
            .map_err(|error| error.to_string())
    })();
    let _ = reply.send(result);
}

pub(super) fn create_pull_request<S: CredentialStore, A: ForgeApi>(
    repo: &git2::Repository,
    credential_service: &CredentialService<S>,
    pull_request_service: &PullRequestService<A>,
    remote_name: String,
    account: String,
    create: CreatePullRequest,
    reply: Sender<Result<PullRequest, String>>,
) {
    let result = (|| {
        let repository = resolve_repository(repo, &remote_name)?;
        let token = credential_service
            .lookup_forge_token(repository.provider, &account)
            .map_err(|error| error.to_string())?;
        pull_request_service
            .create_pull_request(&repository, token.as_deref(), &create)
            .map_err(|error| error.to_string())
    })();
    let _ = reply.send(result);
}

impl WorkerHandle {
    pub fn detect_forge_repository(&self) -> Result<Vec<ForgeRepository>, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::DetectForgeRepository { reply: tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn save_forge_token(
        &self,
        provider: ForgeProvider,
        account: String,
        token: String,
    ) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::SaveForgeToken {
                provider,
                account,
                token,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn forget_forge_token(
        &self,
        provider: ForgeProvider,
        account: String,
    ) -> Result<(), String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ForgetForgeToken {
                provider,
                account,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn list_pull_requests(
        &self,
        remote_name: String,
        account: String,
    ) -> Result<PullRequestList, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ListPullRequests {
                remote_name,
                account,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn create_pull_request(
        &self,
        remote_name: String,
        account: String,
        create: CreatePullRequest,
    ) -> Result<PullRequest, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::CreatePullRequest {
                remote_name,
                account,
                create,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}
