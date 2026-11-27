use super::{Command, ReadHandle, WorkerHandle};
use crate::wire::ErrorDto;
use git_core::forge::{ForgeProvider, ForgeRepository};
use std::sync::mpsc::{self, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

use super::transfer_thread::TransferHost;

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

/// The forge repository behind `remote_name` and the token saved for `account` on its forge:
/// the part of a forge request that reads the repository and the keychain, on the worker.
fn resolve_with_token<S: CredentialStore>(
    repo: &git2::Repository,
    credential_service: &CredentialService<S>,
    remote_name: &str,
    account: &str,
) -> Result<(ForgeRepository, Option<String>), ErrorDto> {
    let repository = resolve_repository(repo, remote_name)?;
    let token = credential_service
        .lookup_forge_token(repository.provider, account)
        .map_err(ErrorDto::from)?;
    Ok((repository, token))
}

/// Answers `reply` with `request`'s result on a thread of its own (#1160): a forge that doesn't
/// answer (no network, a proxy that accepts and never answers) would otherwise hold every read
/// and write of the repository for up to the HTTP timeout. The request touches neither the
/// repository nor the worker's state.
fn off_the_worker<T: Send + 'static>(
    reply: Sender<Result<T, ErrorDto>>,
    request: impl FnOnce() -> Result<T, ErrorDto> + Send + 'static,
) {
    // A thread that can't start drops `reply`, which the caller sees as a failed request.
    let _ = std::thread::Builder::new()
        .name("forge-request".to_string())
        .spawn(move || {
            let _ = reply.send(request());
        });
}

/// Nothing waits for the listing but its caller: the worker goes on at once.
pub(super) fn list_pull_requests<S: CredentialStore, A: ForgeApi + Send + Sync + 'static>(
    repo: &git2::Repository,
    credential_service: &CredentialService<S>,
    pull_request_service: &Arc<PullRequestService<A>>,
    remote_name: String,
    account: String,
    reply: Sender<Result<PullRequestList, ErrorDto>>,
) {
    let (repository, token) =
        match resolve_with_token(repo, credential_service, &remote_name, &account) {
            Ok(resolved) => resolved,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
    let service = Arc::clone(pull_request_service);
    off_the_worker(reply, move || {
        service
            .list_pull_requests(&repository, token.as_deref())
            .map_err(ErrorDto::from_error)
    });
}

/// The operation log notes a create when the worker replies, so the worker waits for it, and
/// serves the read lane's commands meanwhile, as during a transfer (#734): reads go on, other
/// commands wait for the create.
pub(super) fn create_pull_request<S: CredentialStore, A: ForgeApi + Send + Sync + 'static>(
    host: &mut dyn TransferHost,
    credential_service: &CredentialService<S>,
    pull_request_service: &Arc<PullRequestService<A>>,
    remote_name: String,
    account: String,
    create: CreatePullRequest,
    reply: Sender<Result<PullRequest, ErrorDto>>,
) {
    let (repository, token) =
        match resolve_with_token(host.repo(), credential_service, &remote_name, &account) {
            Ok(resolved) => resolved,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
    let service = Arc::clone(pull_request_service);
    let (created_tx, created) = mpsc::channel();
    off_the_worker(created_tx, move || {
        service
            .create_pull_request(&repository, token.as_deref(), &create)
            .map_err(ErrorDto::from_error)
    });
    let result = loop {
        // After a read, look again without waiting: more reads may be queued.
        let answer = if host.serve_read() {
            match created.try_recv() {
                Ok(result) => Some(result),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some(Err(request_lost())),
            }
        } else {
            match created.recv_timeout(POLL) {
                Ok(result) => Some(result),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => Some(Err(request_lost())),
            }
        };
        if let Some(result) = answer {
            break result;
        }
    };
    let _ = reply.send(result);
}

/// How often the worker looks at a create it waits for while no read is queued.
const POLL: Duration = Duration::from_millis(25);

fn request_lost() -> ErrorDto {
    "the request to the provider ended unexpectedly".into()
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
