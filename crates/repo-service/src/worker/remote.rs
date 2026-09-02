use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;

use git_core::remote::{PullOutcome, RemoteInfo, TransferOperation, UpstreamInfo};

use super::{Command, TransferEvent, WorkerHandle};
use crate::credentials::{CredentialService, CredentialStore, CredentialStoreError};

static NEXT_TRANSFER_ID: AtomicU64 = AtomicU64::new(1);

struct ChannelReporter {
    events: Sender<TransferEvent>,
    operation_id: String,
}

impl git_core::remote::TransferReporter for ChannelReporter {
    fn report(&mut self, progress: git_core::remote::TransferProgress) {
        let _ = self.events.send(TransferEvent::Progress(
            git_core::remote::TransferProgress {
                operation_id: self.operation_id.clone(),
                ..progress
            },
        ));
    }
}

pub(super) fn list(repo: &git2::Repository, reply: Sender<Result<Vec<RemoteInfo>, String>>) {
    let _ = reply.send(git_core::remote::list_remotes(repo).map_err(|error| error.to_string()));
}

pub(super) fn list_branches(
    repo: &git2::Repository,
    remote_name: String,
    reply: Sender<Result<Vec<String>, String>>,
) {
    let _ = reply.send(
        git_core::remote::list_remote_branches(repo, &remote_name)
            .map_err(|error| error.to_string()),
    );
}

pub(super) fn current_upstream(
    repo: &git2::Repository,
    reply: Sender<Result<Option<UpstreamInfo>, String>>,
) {
    let _ = reply.send(git_core::remote::current_upstream(repo).map_err(|error| error.to_string()));
}

pub(super) fn add(
    repo: &git2::Repository,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::remote::add_remote(repo, &name, &fetch_url, push_url.as_deref())
            .map_err(|error| error.to_string()),
    );
}
pub(super) fn rename(
    repo: &git2::Repository,
    old_name: String,
    new_name: String,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::remote::rename_remote(repo, &old_name, &new_name)
            .map_err(|error| error.to_string()),
    );
}
pub(super) fn update_urls(
    repo: &git2::Repository,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::remote::update_remote_urls(repo, &name, &fetch_url, push_url.as_deref())
            .map_err(|error| error.to_string()),
    );
}
pub(super) fn upstreams(
    repo: &git2::Repository,
    name: String,
    reply: Sender<Result<Vec<UpstreamInfo>, String>>,
) {
    let _ = reply
        .send(git_core::remote::remote_upstreams(repo, &name).map_err(|error| error.to_string()));
}
pub(super) fn auth_mode(
    repo: &git2::Repository,
    name: String,
    reply: Sender<Result<Option<git_core::remote::RemoteAuthMode>, String>>,
) {
    let _ = reply.send(
        git_core::remote::remote_auth_profile(repo, &name)
            .map_err(|_| "could not read remote authentication settings".to_string()),
    );
}
pub(super) fn save_https<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    remote_name: String,
    username: String,
    token: String,
    reply: Sender<Result<(), String>>,
) {
    let result = git_core::remote::list_remotes(repo)
        .map_err(|_| "could not find remote".to_string())
        .and_then(|remotes| {
            remotes
                .into_iter()
                .find(|remote| remote.name == remote_name)
                .ok_or_else(|| "could not find remote".to_string())
        })
        .and_then(|remote| {
            credentials
                .save_https(&remote.fetch_url, &username, &token)
                .map_err(credential_operation_error)
        });
    let _ = reply.send(result);
}
pub(super) fn forget_https<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    remote_name: String,
    reply: Sender<Result<(), String>>,
) {
    let result = (|| {
        let profile = git_core::remote::remote_auth_profile(repo, &remote_name)
            .map_err(|_| "could not read remote authentication settings".to_string())?;
        let Some(git_core::remote::RemoteAuthMode::HttpsToken { username }) = profile else {
            return Ok(());
        };
        let remote = git_core::remote::list_remotes(repo)
            .map_err(|_| "could not find remote".to_string())?
            .into_iter()
            .find(|remote| remote.name == remote_name)
            .ok_or_else(|| "could not find remote".to_string())?;
        credentials
            .forget_https(&remote.fetch_url, &username)
            .map_err(credential_operation_error)
    })();
    let _ = reply.send(result);
}

fn credential_operation_error(error: CredentialStoreError) -> String {
    match error {
        CredentialStoreError::Keychain(diagnostic) => {
            format!("credential keychain failure: {diagnostic}")
        }
        CredentialStoreError::InvalidHttpsUrl => error.to_string(),
    }
}
pub(super) fn set_auth_mode(
    repo: &git2::Repository,
    remote_name: String,
    mode: git_core::remote::RemoteAuthMode,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::remote::set_remote_auth_profile(repo, &remote_name, mode)
            .map_err(|_| "could not configure remote authentication".to_string()),
    );
}

pub(super) fn remove(
    repo: &git2::Repository,
    name: String,
    clear_upstreams: bool,
    reply: Sender<Result<(), String>>,
) {
    let result = if clear_upstreams {
        git_core::remote::remove_remote_and_clear_upstreams(repo, &name)
    } else {
        git_core::remote::remove_remote(repo, &name)
    }
    .map_err(|error| error.to_string());
    let _ = reply.send(result);
}

pub(super) fn set_current_upstream(
    repo: &git2::Repository,
    remote_name: String,
    remote_branch: String,
    reply: Sender<Result<(), String>>,
) {
    let _ = reply.send(
        git_core::remote::set_current_upstream(repo, &remote_name, &remote_branch)
            .map_err(|error| error.to_string()),
    );
}
pub(super) fn clear_current_upstream(repo: &git2::Repository, reply: Sender<Result<(), String>>) {
    let _ = reply
        .send(git_core::remote::clear_current_upstream(repo).map_err(|error| error.to_string()));
}

fn complete(
    events: &Sender<TransferEvent>,
    operation_id: String,
    operation: TransferOperation,
    result: Result<(), git_core::remote::RemoteError>,
) {
    let _ = events.send(TransferEvent::Completed {
        operation_id,
        operation,
        error: result.err().map(|error| error.transfer_error_kind()),
    });
}
pub(super) fn fetch<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    remote_name: String,
    operation_id: String,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, String>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: operation_id.clone(),
        operation: TransferOperation::Fetch,
    });
    let _ = reply.send(Ok(operation_id.clone()));
    let mut reporter = ChannelReporter {
        events: events.clone(),
        operation_id: operation_id.clone(),
    };
    let result = git_core::remote::remote_auth_profile(repo, &remote_name).and_then(|profile| {
        let mut provider = crate::credentials::RemoteCredentialProvider::new(credentials, profile);
        git_core::remote::fetch_remote(
            repo,
            &remote_name,
            operation_id.clone(),
            &mut provider,
            &mut reporter,
        )
    });
    complete(&events, operation_id, TransferOperation::Fetch, result);
}
pub(super) fn pull<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    operation_id: String,
    events: Sender<TransferEvent>,
    reply: Sender<Result<PullOutcome, String>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: operation_id.clone(),
        operation: TransferOperation::Pull,
    });
    let result = (|| -> Result<PullOutcome, git_core::remote::RemoteError> {
        let upstream = git_core::remote::current_upstream(repo)?
            .ok_or(git_core::remote::RemoteError::NoUpstream)?;
        let mut reporter = ChannelReporter {
            events: events.clone(),
            operation_id: operation_id.clone(),
        };
        let profile = git_core::remote::remote_auth_profile(repo, &upstream.remote_name)?;
        let mut provider = crate::credentials::RemoteCredentialProvider::new(credentials, profile);
        git_core::remote::fetch_remote(
            repo,
            &upstream.remote_name,
            operation_id.clone(),
            &mut provider,
            &mut reporter,
        )?;
        let upstream = git_core::remote::current_upstream(repo)?
            .ok_or(git_core::remote::RemoteError::NoUpstream)?;
        git_core::remote::pull_after_fetch(repo, &upstream.remote_name, &upstream.remote_branch)
    })();
    let error = result
        .as_ref()
        .err()
        .map(|error| error.transfer_error_kind());
    let _ = reply.send(result.map_err(|_| "pull failed".to_string()));
    let _ = events.send(TransferEvent::Completed {
        operation_id,
        operation: TransferOperation::Pull,
        error,
    });
}
pub(super) fn push_branch<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    remote_name: String,
    operation_id: String,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, String>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: operation_id.clone(),
        operation: TransferOperation::PushBranch,
    });
    let _ = reply.send(Ok(operation_id.clone()));
    let mut reporter = ChannelReporter {
        events: events.clone(),
        operation_id: operation_id.clone(),
    };
    let result = git_core::remote::remote_auth_profile(repo, &remote_name).and_then(|profile| {
        let mut provider = crate::credentials::RemoteCredentialProvider::new(credentials, profile);
        git_core::remote::push_current_branch(repo, &remote_name, &mut provider, &mut reporter)
    });
    complete(&events, operation_id, TransferOperation::PushBranch, result);
}
pub(super) fn push_tags<S: CredentialStore>(
    repo: &git2::Repository,
    credentials: &CredentialService<S>,
    remote_name: String,
    names: Vec<String>,
    operation_id: String,
    events: Sender<TransferEvent>,
    reply: Sender<Result<String, String>>,
) {
    let _ = events.send(TransferEvent::Started {
        operation_id: operation_id.clone(),
        operation: TransferOperation::PushTags,
    });
    let _ = reply.send(Ok(operation_id.clone()));
    let mut reporter = ChannelReporter {
        events: events.clone(),
        operation_id: operation_id.clone(),
    };
    let result = git_core::remote::remote_auth_profile(repo, &remote_name).and_then(|profile| {
        let mut provider = crate::credentials::RemoteCredentialProvider::new(credentials, profile);
        git_core::remote::push_tags(repo, &remote_name, &names, &mut provider, &mut reporter)
    });
    complete(&events, operation_id, TransferOperation::PushTags, result);
}

impl WorkerHandle {
    pub fn fetch_remote(
        &self,
        remote_name: String,
        events: Sender<TransferEvent>,
    ) -> Result<String, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        let operation_id = format!("fetch-{}", NEXT_TRANSFER_ID.fetch_add(1, Ordering::Relaxed));
        self.tx
            .send(Command::FetchRemote {
                remote_name,
                operation_id,
                events,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn pull_current_upstream(
        &self,
        events: Sender<TransferEvent>,
    ) -> Result<PullOutcome, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        let operation_id = format!("pull-{}", NEXT_TRANSFER_ID.fetch_add(1, Ordering::Relaxed));
        self.tx
            .send(Command::PullCurrentUpstream {
                operation_id,
                events,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn push_current_branch(
        &self,
        remote_name: String,
        events: Sender<TransferEvent>,
    ) -> Result<String, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        let operation_id = format!("push-{}", NEXT_TRANSFER_ID.fetch_add(1, Ordering::Relaxed));
        self.tx
            .send(Command::PushCurrentBranch {
                remote_name,
                operation_id,
                events,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn push_tags(
        &self,
        remote_name: String,
        names: Vec<String>,
        events: Sender<TransferEvent>,
    ) -> Result<String, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        let operation_id = format!("push-{}", NEXT_TRANSFER_ID.fetch_add(1, Ordering::Relaxed));
        self.tx
            .send(Command::PushTags {
                remote_name,
                names,
                operation_id,
                events,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn list_remotes(&self) -> Result<Vec<RemoteInfo>, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ListRemotes { reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn list_remote_branches(&self, remote_name: String) -> Result<Vec<String>, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ListRemoteBranches { remote_name, reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn get_current_upstream(&self) -> Result<Option<UpstreamInfo>, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetCurrentUpstream { reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn get_remote_upstreams(&self, name: String) -> Result<Vec<UpstreamInfo>, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetRemoteUpstreams { name, reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn get_remote_auth_mode(
        &self,
        name: String,
    ) -> Result<Option<git_core::remote::RemoteAuthMode>, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetRemoteAuthMode { name, reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn save_https_credential(
        &self,
        remote_name: String,
        username: String,
        token: String,
    ) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::SaveHttpsCredential {
                remote_name,
                username,
                token,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn forget_https_credential(&self, remote_name: String) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ForgetHttpsCredential { remote_name, reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn set_remote_auth_mode(
        &self,
        remote_name: String,
        mode: git_core::remote::RemoteAuthMode,
    ) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::SetRemoteAuthMode {
                remote_name,
                mode,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn add_remote(
        &self,
        name: String,
        fetch_url: String,
        push_url: Option<String>,
    ) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::AddRemote {
                name,
                fetch_url,
                push_url,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn rename_remote(&self, old_name: String, new_name: String) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::RenameRemote {
                old_name,
                new_name,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn update_remote_urls(
        &self,
        name: String,
        fetch_url: String,
        push_url: Option<String>,
    ) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::UpdateRemoteUrls {
                name,
                fetch_url,
                push_url,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn remove_remote(&self, name: String, clear_upstreams: bool) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::RemoveRemote {
                name,
                clear_upstreams,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn set_current_upstream(
        &self,
        remote_name: String,
        remote_branch: String,
    ) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::SetCurrentUpstream {
                remote_name,
                remote_branch,
                reply,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
    pub fn clear_current_upstream(&self) -> Result<(), String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::ClearCurrentUpstream { reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::credential_operation_error;
    use crate::credentials::CredentialStoreError;

    #[test]
    fn credential_operation_error_keeps_the_backend_diagnostic() {
        let error =
            CredentialStoreError::Keychain("CredWrite failed with Windows error 1312".to_owned());

        assert_eq!(
            credential_operation_error(error),
            "credential keychain failure: CredWrite failed with Windows error 1312"
        );
    }
}
