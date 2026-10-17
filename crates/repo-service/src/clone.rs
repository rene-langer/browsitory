//! Repo-less clone jobs. A clone has no repository (and so no per-repo worker) until it
//! finishes, so it runs here: one call per clone on the caller's thread, with its own cancel
//! registry. Progress goes out as `TransferEvent::Progress` only — never `Started`/`Completed`,
//! which an open repo tab's pending fetch/push would adopt as its own transfer.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use git_core::clone::{clone_repo, CloneError, CloneOptions, CloneOutcome};
use git_core::remote::{
    set_remote_auth_profile, CredentialProvider, RemoteAuthMode, TransferProgress, TransferReporter,
};

use crate::credentials::{CredentialService, CredentialStore, RemoteCredentialProvider};
use crate::worker::TransferEvent;

pub struct HttpsCredentialInput {
    pub username: String,
    pub token: String,
}

#[derive(Debug)]
pub struct ServiceCloneOutcome {
    pub outcome: CloneOutcome,
    pub credential_save_failed: bool,
}

type StoreFactory<S> = Box<dyn Fn() -> S + Send + Sync>;

pub struct CloneService<S: CredentialStore> {
    new_store: StoreFactory<S>,
    running: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

#[cfg(not(feature = "forge-fixture-override"))]
pub type DefaultCredentialStore = crate::credentials::KeyringCredentialStore;
#[cfg(feature = "forge-fixture-override")]
pub type DefaultCredentialStore = crate::credentials::InMemoryCredentialStore;

pub type DefaultCloneService = CloneService<DefaultCredentialStore>;

impl DefaultCloneService {
    /// Same store choice as `Worker::spawn`: the OS keychain, or the in-memory store in E2E
    /// builds.
    pub fn new() -> Self {
        #[cfg(not(feature = "forge-fixture-override"))]
        return Self::with_store_factory(|| crate::credentials::KeyringCredentialStore);
        #[cfg(feature = "forge-fixture-override")]
        return Self::with_store_factory(crate::credentials::InMemoryCredentialStore::default);
    }
}

impl Default for DefaultCloneService {
    fn default() -> Self {
        Self::new()
    }
}

/// The token the user typed into the clone dialog, offered once from memory. It reaches the
/// keychain only after the clone succeeds (`persist_https`), so a wrong token is never saved.
struct OneShotHttps<'a> {
    input: &'a HttpsCredentialInput,
}

impl CredentialProvider for OneShotHttps<'_> {
    fn credential(
        &mut self,
        _url: &str,
        _username: Option<&str>,
        _allowed: git2::CredentialType,
    ) -> Result<git2::Cred, git2::Error> {
        git2::Cred::userpass_plaintext(&self.input.username, &self.input.token)
    }
}

struct ChannelReporter<'a> {
    events: &'a Sender<TransferEvent>,
    operation_id: &'a str,
}

impl TransferReporter for ChannelReporter<'_> {
    fn report(&mut self, progress: TransferProgress) {
        let _ = self.events.send(TransferEvent::Progress(TransferProgress {
            operation_id: self.operation_id.to_string(),
            ..progress
        }));
    }
}

impl<S: CredentialStore> CloneService<S> {
    pub fn with_store_factory(factory: impl Fn() -> S + Send + Sync + 'static) -> Self {
        Self {
            new_store: Box::new(factory),
            running: Mutex::new(HashMap::new()),
        }
    }

    /// Registers `operation_id`'s cancel flag (idempotent: a flag set by an early `cancel`
    /// survives) and returns it.
    pub(crate) fn register(&self, operation_id: &str) -> Arc<AtomicBool> {
        Arc::clone(
            self.running
                .lock()
                .unwrap()
                .entry(operation_id.to_string())
                .or_insert_with(|| Arc::new(AtomicBool::new(false))),
        )
    }

    pub(crate) fn unregister(&self, operation_id: &str) {
        self.running.lock().unwrap().remove(operation_id);
    }

    #[cfg(test)]
    pub(crate) fn is_cancelled(&self, operation_id: &str) -> bool {
        self.running
            .lock()
            .unwrap()
            .get(operation_id)
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    }

    /// Sets `operation_id`'s cancel flag, creating it if the clone hasn't registered yet — the
    /// frontend can cancel before `clone_repo`'s blocking thread starts. A flag for an id that
    /// never starts is a few bytes (ids are UUIDs, created only by a user pressing Cancel).
    pub fn cancel(&self, operation_id: &str) {
        self.running
            .lock()
            .unwrap()
            .entry(operation_id.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .store(true, Ordering::Relaxed);
    }

    pub fn clone_repo(
        &self,
        operation_id: &str,
        opts: CloneOptions,
        https: Option<HttpsCredentialInput>,
        events: Sender<TransferEvent>,
    ) -> Result<ServiceCloneOutcome, CloneError> {
        let flag = self.register(operation_id);
        let cancel = || flag.load(Ordering::Relaxed);
        let mut reporter = ChannelReporter {
            events: &events,
            operation_id,
        };
        let service = CredentialService::new((self.new_store)());
        let result = match &https {
            Some(input) => clone_repo(&opts, &mut OneShotHttps { input }, &mut reporter, &cancel),
            None => {
                let mut provider = RemoteCredentialProvider::new(&service, None);
                clone_repo(&opts, &mut provider, &mut reporter, &cancel)
            }
        };
        self.unregister(operation_id);
        let outcome = result?;
        let credential_save_failed = match &https {
            Some(input) => persist_https(&service, opts.url.trim(), &outcome, input).is_err(),
            None => false,
        };
        Ok(ServiceCloneOutcome {
            outcome,
            credential_save_failed,
        })
    }
}

fn persist_https<S: CredentialStore>(
    service: &CredentialService<S>,
    url: &str,
    outcome: &CloneOutcome,
    input: &HttpsCredentialInput,
) -> Result<(), String> {
    service
        .save_https(url, &input.username, &input.token)
        .map_err(|error| error.to_string())?;
    let repo = git2::Repository::open(&outcome.path).map_err(|error| error.to_string())?;
    set_remote_auth_profile(
        &repo,
        "origin",
        RemoteAuthMode::HttpsToken {
            username: input.username.clone(),
        },
    )
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};

    use git_core::clone::{CloneError, CloneOptions};

    use super::{CloneService, HttpsCredentialInput};
    use crate::credentials::{CredentialKey, CredentialStore, CredentialStoreError};
    use crate::worker::TransferEvent;

    #[derive(Clone, Default)]
    struct SharedStore {
        entries: Arc<Mutex<HashMap<String, String>>>,
        fail_writes: bool,
    }

    impl CredentialStore for SharedStore {
        fn get(&self, key: &CredentialKey) -> Result<Option<String>, CredentialStoreError> {
            Ok(self
                .entries
                .lock()
                .unwrap()
                .get(&format!("{key:?}"))
                .cloned())
        }
        fn set(&self, key: &CredentialKey, token: &str) -> Result<(), CredentialStoreError> {
            if self.fail_writes {
                return Err(CredentialStoreError::Keychain("locked".into()));
            }
            self.entries
                .lock()
                .unwrap()
                .insert(format!("{key:?}"), token.to_string());
            Ok(())
        }
        fn delete(&self, key: &CredentialKey) -> Result<(), CredentialStoreError> {
            self.entries.lock().unwrap().remove(&format!("{key:?}"));
            Ok(())
        }
    }

    fn bare_source() -> (tempfile::TempDir, String) {
        let dir = tempfile::TempDir::new().unwrap();
        let work = git2::Repository::init(dir.path().join("work")).unwrap();
        std::fs::write(dir.path().join("work/a.txt"), "a\n").unwrap();
        let mut index = work.index().unwrap();
        index.add_path(std::path::Path::new("a.txt")).unwrap();
        let tree = work.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("Test", "test@example.com").unwrap();
        work.commit(Some("HEAD"), &sig, &sig, "first", &tree, &[])
            .unwrap();
        let bare = dir.path().join("src.git");
        git2::Repository::init_bare(&bare).unwrap();
        let head = work.head().unwrap().name().unwrap().to_string();
        work.remote("origin", bare.to_str().unwrap())
            .unwrap()
            .push(&[format!("{head}:{head}")], None)
            .unwrap();
        git2::Repository::open_bare(&bare)
            .unwrap()
            .set_head(&head)
            .unwrap();
        let url = format!("file://{}", bare.to_str().unwrap().replace('\\', "/"));
        (dir, url)
    }

    fn options(url: &str, dest: std::path::PathBuf) -> CloneOptions {
        CloneOptions {
            url: url.into(),
            dest,
            branch: None,
            recurse_submodules: false,
        }
    }

    fn https() -> Option<HttpsCredentialInput> {
        Some(HttpsCredentialInput {
            username: "alice".into(),
            token: "tok".into(),
        })
    }

    #[test]
    fn emits_only_progress_events_tagged_with_the_operation_id() {
        let (_src, url) = bare_source();
        let target = tempfile::TempDir::new().unwrap();
        let service = CloneService::with_store_factory(SharedStore::default);
        let (tx, rx) = mpsc::channel();

        service
            .clone_repo("clone-1", options(&url, target.path().join("c")), None, tx)
            .unwrap();

        let events: Vec<TransferEvent> = rx.try_iter().collect();
        assert!(!events.is_empty());
        for event in events {
            match event {
                TransferEvent::Progress(progress) => assert_eq!(progress.operation_id, "clone-1"),
                // A Started event would be adopted by an open repo tab's pending fetch/push.
                other => panic!("unexpected event {other:?}"),
            }
        }
    }

    #[test]
    fn persist_https_saves_the_token_and_sets_the_origin_profile() {
        // `clone_repo` can only reach `persist_https` over a real HTTPS host, which tests don't
        // have; exercise the save step directly on a real cloned repo.
        let (_src, url) = bare_source();
        let target = tempfile::TempDir::new().unwrap();
        let dest = target.path().join("c");
        let cloned = CloneService::with_store_factory(SharedStore::default)
            .clone_repo(
                "clone-1",
                options(&url, dest.clone()),
                None,
                mpsc::channel().0,
            )
            .unwrap();
        let store = SharedStore::default();
        let service = crate::credentials::CredentialService::new(store.clone());
        let input = HttpsCredentialInput {
            username: "alice".into(),
            token: "tok".into(),
        };

        super::persist_https(
            &service,
            "https://example.test/r.git",
            &cloned.outcome,
            &input,
        )
        .unwrap();

        assert_eq!(store.entries.lock().unwrap().len(), 1);
        let repo = git2::Repository::open(&dest).unwrap();
        assert_eq!(
            git_core::remote::remote_auth_profile(&repo, "origin").unwrap(),
            Some(git_core::remote::RemoteAuthMode::HttpsToken {
                username: "alice".into()
            })
        );
    }

    #[test]
    fn a_token_for_a_non_https_url_is_reported_unsaved_but_the_clone_succeeds() {
        let (_src, url) = bare_source();
        let target = tempfile::TempDir::new().unwrap();
        let store = SharedStore::default();
        let shared = store.clone();
        let service = CloneService::with_store_factory(move || shared.clone());

        let outcome = service
            .clone_repo(
                "clone-1",
                options(&url, target.path().join("c")),
                https(),
                mpsc::channel().0,
            )
            .unwrap();

        assert!(outcome.credential_save_failed);
        assert!(store.entries.lock().unwrap().is_empty());
    }

    #[test]
    fn a_failed_clone_saves_nothing() {
        let target = tempfile::TempDir::new().unwrap();
        let store = SharedStore::default();
        let shared = store.clone();
        let service = CloneService::with_store_factory(move || shared.clone());
        let missing = format!(
            "file://{}/missing.git",
            target.path().to_str().unwrap().replace('\\', "/")
        );

        let result = service.clone_repo(
            "clone-1",
            options(&missing, target.path().join("c")),
            https(),
            mpsc::channel().0,
        );

        assert!(result.is_err());
        assert!(store.entries.lock().unwrap().is_empty());
    }

    #[test]
    fn a_keychain_failure_is_an_error_from_persist_https() {
        let (_src, url) = bare_source();
        let target = tempfile::TempDir::new().unwrap();
        let cloned = CloneService::with_store_factory(SharedStore::default)
            .clone_repo(
                "clone-1",
                options(&url, target.path().join("c")),
                None,
                mpsc::channel().0,
            )
            .unwrap();
        let service = crate::credentials::CredentialService::new(SharedStore {
            fail_writes: true,
            ..Default::default()
        });
        let input = HttpsCredentialInput {
            username: "alice".into(),
            token: "tok".into(),
        };

        let result = super::persist_https(
            &service,
            "https://example.test/r.git",
            &cloned.outcome,
            &input,
        );

        assert!(result.is_err());
    }

    #[test]
    fn cancel_before_start_is_honoured() {
        // The frontend may call cancel_clone before clone_repo's thread has registered its flag.
        let (_src, url) = bare_source();
        let target = tempfile::TempDir::new().unwrap();
        let service = CloneService::with_store_factory(SharedStore::default);
        service.cancel("clone-3");

        let result = service.clone_repo(
            "clone-3",
            options(&url, target.path().join("c")),
            None,
            mpsc::channel().0,
        );

        assert!(matches!(result, Err(CloneError::Cancelled)), "{result:?}");
    }

    #[test]
    fn cancel_flag_is_registered_while_running_and_removed_after() {
        let service = CloneService::with_store_factory(SharedStore::default);
        service.register("clone-9");
        service.cancel("clone-9");
        assert!(service.is_cancelled("clone-9"));
        service.unregister("clone-9");
        assert!(!service.is_cancelled("clone-9"));
    }

    #[test]
    fn a_pre_cancelled_clone_reports_cancelled() {
        let (_src, url) = bare_source();
        let target = tempfile::TempDir::new().unwrap();
        let service = CloneService::with_store_factory(SharedStore::default);
        service.register("clone-2");
        service.cancel("clone-2");

        let result = service.clone_repo(
            "clone-2",
            options(&url, target.path().join("c")),
            None,
            mpsc::channel().0,
        );

        assert!(matches!(result, Err(CloneError::Cancelled)), "{result:?}");
    }
}
