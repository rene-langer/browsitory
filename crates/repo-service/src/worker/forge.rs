use super::{Command, ReadHandle, WorkerHandle};
use crate::wire::ErrorDto;
use git_core::forge::{ForgeProvider, ForgeRepository};
use std::sync::mpsc::Sender;

use crate::credentials::{CredentialService, CredentialStore};
use crate::pull_requests::{
    CreatePullRequest, ForgeApi, PullRequest, PullRequestList, PullRequestService,
};

fn resolve_repository(
    repo: &git2::Repository,
    remote_name: &str,
) -> Result<ForgeRepository, ErrorDto> {
    git_core::forge::detect_forge_repositories(repo)
        .ok()
        .into_iter()
        .flatten()
        .find(|repository| repository.remote_name == remote_name)
        .ok_or_else(|| "this remote is not a supported forge repository".into())
}

pub(super) fn detect(
    repo: &git2::Repository,
    reply: Sender<Result<Vec<ForgeRepository>, ErrorDto>>,
) {
    let repositories = git_core::forge::detect_forge_repositories(repo).unwrap_or_default();
    let _ = reply.send(Ok(repositories));
}

pub(super) fn save_token<S: CredentialStore>(
    credential_service: &CredentialService<S>,
    provider: ForgeProvider,
    account: String,
    token: String,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        credential_service
            .save_forge_token(provider, &account, &token)
            .map_err(ErrorDto::from),
    );
}

pub(super) fn forget_token<S: CredentialStore>(
    credential_service: &CredentialService<S>,
    provider: ForgeProvider,
    account: String,
    reply: Sender<Result<(), ErrorDto>>,
) {
    let _ = reply.send(
        credential_service
            .forget_forge_token(provider, &account)
            .map_err(ErrorDto::from),
    );
}

pub(super) fn list_pull_requests<S: CredentialStore, A: ForgeApi>(
    repo: &git2::Repository,
    credential_service: &CredentialService<S>,
    pull_request_service: &PullRequestService<A>,
    remote_name: String,
    account: String,
    reply: Sender<Result<PullRequestList, ErrorDto>>,
) {
    let result = (|| {
        let repository = resolve_repository(repo, &remote_name)?;
        let token = credential_service
            .lookup_forge_token(repository.provider, &account)
            .map_err(ErrorDto::from)?;
        pull_request_service
            .list_pull_requests(&repository, token.as_deref())
            .map_err(ErrorDto::from_error)
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
    reply: Sender<Result<PullRequest, ErrorDto>>,
) {
    let result = (|| {
        let repository = resolve_repository(repo, &remote_name)?;
        let token = credential_service
            .lookup_forge_token(repository.provider, &account)
            .map_err(ErrorDto::from)?;
        pull_request_service
            .create_pull_request(&repository, token.as_deref(), &create)
            .map_err(ErrorDto::from_error)
    })();
    let _ = reply.send(result);
}

impl WorkerHandle {
    pub fn save_forge_token(
        &self,
        provider: ForgeProvider,
        account: String,
        token: String,
    ) -> Result<(), ErrorDto> {
        self.request(|tx| Command::SaveForgeToken {
            provider,
            account,
            token,
            reply: tx,
        })
    }

    pub fn forget_forge_token(
        &self,
        provider: ForgeProvider,
        account: String,
    ) -> Result<(), ErrorDto> {
        self.request(|tx| Command::ForgetForgeToken {
            provider,
            account,
            reply: tx,
        })
    }

    pub fn list_pull_requests(
        &self,
        remote_name: String,
        account: String,
    ) -> Result<PullRequestList, ErrorDto> {
        self.request(|tx| Command::ListPullRequests {
            remote_name,
            account,
            reply: tx,
        })
    }

    pub fn create_pull_request(
        &self,
        remote_name: String,
        account: String,
        create: CreatePullRequest,
    ) -> Result<PullRequest, ErrorDto> {
        self.request(|tx| Command::CreatePullRequest {
            remote_name,
            account,
            create,
            reply: tx,
        })
    }
}

impl ReadHandle {
    pub fn detect_forge_repository(&self) -> Result<Vec<ForgeRepository>, ErrorDto> {
        self.read(|tx| Command::DetectForgeRepository { reply: tx })
    }
}
