//! Repo-less clone jobs. A clone has no repository (and so no per-repo worker) until it
//! finishes, so it runs here: one call per clone, with its own cancel registry. Progress goes out
//! as `TransferEvent::Progress` only — never `Started`/`Completed`, which an open repo tab's
//! pending fetch/push would adopt as its own transfer.
//!
//! The clone runs on a helper thread and the caller's thread waits for it (#1159), as a fetch
//! does on the worker (`worker::transfer_thread`): a clone still connecting, or one whose server
//! went silent (preparing a big pack, or the network gone without a reset), calls none of
//! libgit2's callbacks, so the cancel flag can't reach it. When a cancel goes unanswered for
//! `STALLED_CANCEL_GRACE`, the caller gives up on the helper and answers `Cancelled`. The helper
//! stays blocked until its connection fails (libgit2's server timeout or the operating system),
//! then fails as cancelled and removes what it made; a clone that completes after the caller
//! gave up is removed too (`TransferReporter::clone_complete`), and its token isn't saved.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use git_core::clone::{clone_repo, CloneError, CloneOptions, CloneOutcome};
use git_core::remote::{
    set_remote_auth_profile, CredentialProvider, RemoteAuthMode, TransferProgress, TransferReporter,
};

use crate::credentials::{CredentialService, CredentialStore, RemoteCredentialProvider};
use crate::worker::{TransferEvent, STALLED_CANCEL_GRACE};

/// How often the caller looks at a helper that sends nothing.
const POLL: Duration = Duration::from_millis(25);

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

/// How many cancels for clones that aren't running `CloneService` keeps for a clone still to
/// start. Such a cancel may also be a late one, for a clone that already finished, so the list
/// is capped rather than kept forever (#219).
const EARLY_CANCELS: usize = 8;

pub struct CloneService<S: CredentialStore> {
    new_store: StoreFactory<S>,
    clones: Mutex<Clones>,
}

/// The running clones' cancel flags, and the latest cancels for IDs that weren't running.
#[derive(Default)]
struct Clones {
    running: HashMap<String, Arc<AtomicBool>>,
    early: VecDeque<String>,
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

/// The token the user typed into the clone dialog, offered at most once, from memory, and only
/// over `https://` with a plaintext user/pass credential allowed. libgit2 re-invokes the
/// callback after every 401, so any later call (a wrong token) or a cleartext/other-scheme URL
/// gets a missing-credential error instead, which the dialog turns into a re-prompt. The token
/// reaches the keychain only after the clone succeeds and the token was actually offered
/// (`offered`; a public repo never asks, so a mistyped token is never saved).
///
/// The token goes only to the clone URL's scheme, host and port (#1126): libgit2 follows a
/// redirect of the first request to another host and then asks for that host's credential.
struct OneShotHttps<'a> {
    input: &'a HttpsCredentialInput,
    clone_origin: Option<url::Origin>,
    offered: bool,
}

/// `url`'s scheme, host and port, when its scheme is https.
fn https_origin(url: &str) -> Option<url::Origin> {
    let parsed = url::Url::parse(url).ok()?;
    (parsed.scheme() == "https").then(|| parsed.origin())
}

impl<'a> OneShotHttps<'a> {
    fn new(input: &'a HttpsCredentialInput, clone_url: &str) -> Self {
        Self {
            input,
            clone_origin: https_origin(clone_url),
            offered: false,
        }
    }
}

impl CredentialProvider for OneShotHttps<'_> {
    fn credential(
        &mut self,
        url: &str,
        _username: Option<&str>,
        allowed: git2::CredentialType,
    ) -> Result<git2::Cred, git2::Error> {
        let clone_host = matches!(
            (&self.clone_origin, https_origin(url)),
            (Some(clone), Some(asked)) if *clone == asked
        );
        if !self.offered
            && clone_host
            && allowed.contains(git2::CredentialType::USER_PASS_PLAINTEXT)
        {
            self.offered = true;
            git2::Cred::userpass_plaintext(&self.input.username, &self.input.token)
        } else {
            Err(git2::Error::from_str(
                git_core::remote::MISSING_CREDENTIAL_ERROR,
            ))
        }
    }
}

/// Where a helper's clone is, as the caller and the helper agree on it under one lock: the
/// caller gives up only on a clone still `Running`, and a clone is handed over only while the
/// caller hasn't given up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Running,
    Complete,
    Abandoned,
}

#[derive(Clone)]
struct SharedPhase(Arc<Mutex<Phase>>);

impl SharedPhase {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Phase::Running)))
    }

    fn lock(&self) -> MutexGuard<'_, Phase> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// The helper's clone is complete: it is handed over unless the caller gave up on it.
    fn complete(&self) -> bool {
        let mut phase = self.lock();
        if *phase == Phase::Abandoned {
            return false;
        }
        *phase = Phase::Complete;
        true
    }

    /// The caller gives up on a clone still running; one that is complete is about to answer.
    fn abandon(&self) -> bool {
        let mut phase = self.lock();
        if *phase != Phase::Running {
            return false;
        }
        *phase = Phase::Abandoned;
        true
    }
}

enum HelperMessage {
    Progress(TransferProgress),
    Done(Result<ServiceCloneOutcome, CloneError>),
}

/// Sends a helper's progress to the caller, which passes it on while it still waits. The helper
/// never holds the caller's event channel, so an abandoned helper keeps no listener waiting.
struct HelperReporter {
    messages: Sender<HelperMessage>,
    phase: SharedPhase,
}

impl TransferReporter for HelperReporter {
    fn report(&mut self, progress: TransferProgress) {
        let _ = self.messages.send(HelperMessage::Progress(progress));
    }

    fn clone_complete(&mut self) -> bool {
        self.phase.complete()
    }
}

fn helper_lost() -> CloneError {
    CloneError::Io(std::io::Error::other("the clone thread ended unexpectedly"))
}

/// The clone with the credential provider `https` calls for, then the typed token saved when it
/// was offered. Runs on the helper thread.
fn clone_and_save<S: CredentialStore>(
    service: &CredentialService<S>,
    opts: &CloneOptions,
    https: Option<&HttpsCredentialInput>,
    reporter: &mut dyn TransferReporter,
    cancel: &dyn Fn() -> bool,
) -> Result<ServiceCloneOutcome, CloneError> {
    let mut token_offered = false;
    let result = match https {
        Some(input) => {
            let mut provider = OneShotHttps::new(input, opts.url.trim());
            let result = clone_repo(opts, &mut provider, reporter, cancel);
            token_offered = provider.offered;
            result
        }
        None => {
            let mut provider = RemoteCredentialProvider::new(service, None);
            clone_repo(opts, &mut provider, reporter, cancel)
        }
    };
    let outcome = result?;
    let credential_save_failed =
        save_offered_token(service, opts.url.trim(), &outcome, https, token_offered);
    Ok(ServiceCloneOutcome {
        outcome,
        credential_save_failed,
    })
}

/// Saves the token typed for the clone when the clone offered it; whether saving failed. A
/// token never offered (a public repository never asks) is never saved.
fn save_offered_token<S: CredentialStore>(
    service: &CredentialService<S>,
    url: &str,
    outcome: &CloneOutcome,
    https: Option<&HttpsCredentialInput>,
    offered: bool,
) -> bool {
    match https {
        Some(input) if offered => persist_https(service, url, outcome, input).is_err(),
        _ => false,
    }
}

impl<S: CredentialStore> CloneService<S> {
    pub fn with_store_factory(factory: impl Fn() -> S + Send + Sync + 'static) -> Self {
        Self {
            new_store: Box::new(factory),
            clones: Mutex::new(Clones::default()),
        }
    }

    fn clones(&self) -> std::sync::MutexGuard<'_, Clones> {
        self.clones
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    /// Registers `operation_id`'s cancel flag and returns it, set when a cancel for it arrived
    /// before it started.
    pub(crate) fn register(&self, operation_id: &str) -> Arc<AtomicBool> {
        let mut clones = self.clones();
        let early = clones.early.iter().position(|id| id == operation_id);
        let cancelled = early.is_some();
        if let Some(index) = early {
            clones.early.remove(index);
        }
        Arc::clone(
            clones
                .running
                .entry(operation_id.to_string())
                .or_insert_with(|| Arc::new(AtomicBool::new(cancelled))),
        )
    }

    pub(crate) fn unregister(&self, operation_id: &str) {
        self.clones().running.remove(operation_id);
    }

    #[cfg(test)]
    pub(crate) fn is_idle(&self) -> bool {
        self.clones().running.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn is_cancelled(&self, operation_id: &str) -> bool {
        self.clones()
            .running
            .get(operation_id)
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    }

    #[cfg(test)]
    fn early_cancels(&self) -> usize {
        self.clones().early.len()
    }

    /// Sets `operation_id`'s cancel flag. The frontend makes up the ID and can cancel before
    /// `clone_repo`'s blocking thread registers it, so a cancel for an ID that isn't running is
    /// kept for `register`, but only the latest `EARLY_CANCELS` of them: it may as well be a
    /// late click for a clone that already finished.
    pub fn cancel(&self, operation_id: &str) {
        let mut clones = self.clones();
        if let Some(flag) = clones.running.get(operation_id) {
            flag.store(true, Ordering::Relaxed);
            return;
        }
        if clones.early.iter().any(|id| id == operation_id) {
            return;
        }
        if clones.early.len() == EARLY_CANCELS {
            clones.early.pop_front();
        }
        clones.early.push_back(operation_id.to_string());
    }

    /// Clones on a helper thread and waits for it (see the module docs), passing its progress
    /// on to `events` stamped with `operation_id`.
    pub fn clone_repo(
        &self,
        operation_id: &str,
        opts: CloneOptions,
        https: Option<HttpsCredentialInput>,
        events: Sender<TransferEvent>,
    ) -> Result<ServiceCloneOutcome, CloneError>
    where
        S: Send + 'static,
    {
        let flag = self.register(operation_id);
        let result = self.run(operation_id, &flag, opts, https, &events);
        self.unregister(operation_id);
        result
    }

    fn run(
        &self,
        operation_id: &str,
        flag: &Arc<AtomicBool>,
        opts: CloneOptions,
        https: Option<HttpsCredentialInput>,
        events: &Sender<TransferEvent>,
    ) -> Result<ServiceCloneOutcome, CloneError>
    where
        S: Send + 'static,
    {
        let phase = SharedPhase::new();
        let (tx, rx) = mpsc::channel();
        let store = (self.new_store)();
        let spawned = {
            let flag = Arc::clone(flag);
            let phase = phase.clone();
            std::thread::Builder::new()
                .name("clone".to_string())
                .spawn(move || {
                    let cancel = || flag.load(Ordering::Relaxed);
                    let mut reporter = HelperReporter {
                        messages: tx.clone(),
                        phase,
                    };
                    let service = CredentialService::new(store);
                    let result =
                        clone_and_save(&service, &opts, https.as_ref(), &mut reporter, &cancel);
                    let _ = tx.send(HelperMessage::Done(result));
                })
        };
        if spawned.is_err() {
            return Err(helper_lost());
        }
        // When the caller first saw the cancel.
        let mut cancelled_at: Option<Instant> = None;
        loop {
            match rx.recv_timeout(POLL) {
                Ok(HelperMessage::Progress(progress)) => {
                    let _ = events.send(TransferEvent::Progress(TransferProgress {
                        operation_id: operation_id.to_string(),
                        ..progress
                    }));
                }
                Ok(HelperMessage::Done(result)) => return result,
                Err(RecvTimeoutError::Timeout) => {}
                // The helper panicked.
                Err(RecvTimeoutError::Disconnected) => return Err(helper_lost()),
            }
            if cancelled_at.is_none() && flag.load(Ordering::Relaxed) {
                cancelled_at = Some(Instant::now());
            }
            if cancelled_at.is_some_and(|at| at.elapsed() >= STALLED_CANCEL_GRACE)
                && phase.abandon()
            {
                return Err(CloneError::Cancelled);
            }
        }
    }
}

fn persist_https<S: CredentialStore>(
    service: &CredentialService<S>,
    url: &str,
    outcome: &CloneOutcome,
    input: &HttpsCredentialInput,
) -> Result<(), String> {
    // Profile first: if it fails, no orphan keychain entry is left behind.
    let repo = git2::Repository::open(&outcome.path).map_err(|error| error.to_string())?;
    set_remote_auth_profile(
        &repo,
        "origin",
        RemoteAuthMode::HttpsToken {
            username: input.username.clone(),
        },
    )
    .map_err(|error| error.to_string())?;
    service
        .save_https(url, &input.username, &input.token)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::Ordering;
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
        let path = bare.to_str().unwrap().replace('\\', "/");
        // Windows paths (`C:/...`) need the extra slash: `file:///C:/...`.
        let url = if path.starts_with('/') {
            format!("file://{path}")
        } else {
            format!("file:///{path}")
        };
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
        assert!(service.is_idle());

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
    fn a_token_that_was_never_offered_is_not_saved_and_not_a_failure() {
        // The file:// fixture never challenges for credentials, like a public repo.
        let (_src, url) = bare_source();
        let target = tempfile::TempDir::new().unwrap();
        let dest = target.path().join("c");
        let store = SharedStore::default();
        let shared = store.clone();
        let service = CloneService::with_store_factory(move || shared.clone());

        let outcome = service
            .clone_repo(
                "clone-1",
                options(&url, dest.clone()),
                https(),
                mpsc::channel().0,
            )
            .unwrap();

        assert!(!outcome.credential_save_failed);
        assert!(store.entries.lock().unwrap().is_empty());
        let repo = git2::Repository::open(&dest).unwrap();
        assert_eq!(
            git_core::remote::remote_auth_profile(&repo, "origin").unwrap(),
            None
        );
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
        assert!(service.is_idle());
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

    /// #1159: a clone whose server went silent after its ref advertisement calls none of
    /// libgit2's callbacks, so the cancel flag alone never reaches it. Cancel still ends it
    /// within a few seconds; the abandoned clone removes what it made once its connection fails.
    #[test]
    fn cancel_ends_a_clone_that_stalled_after_connecting() {
        use std::time::{Duration, Instant};

        let server = test_support::StalledServer::start();
        let target = tempfile::TempDir::new().unwrap();
        let dest = target.path().join("c");
        let service = Arc::new(CloneService::with_store_factory(SharedStore::default));
        let (done_tx, done) = mpsc::channel();
        {
            let service = Arc::clone(&service);
            let options = options(&server.url, dest.clone());
            std::thread::spawn(move || {
                let result = service.clone_repo("clone-stalled", options, None, mpsc::channel().0);
                let _ = done_tx.send(result);
            });
        }
        server
            .stalled
            .recv_timeout(Duration::from_secs(30))
            .expect("the clone connects and then waits for the server");

        let cancelled = Instant::now();
        service.cancel("clone-stalled");
        let result = done
            .recv_timeout(Duration::from_secs(15))
            .unwrap_or_else(|_| {
                panic!(
                    "the clone was still running {:?} after Cancel",
                    cancelled.elapsed()
                )
            });

        assert!(matches!(result, Err(CloneError::Cancelled)), "{result:?}");
        // A clone still receiving answers the cancel at its next callback and removes what it
        // made itself: only one that left the cancel unanswered for the grace is given up on.
        assert!(
            cancelled.elapsed() >= crate::worker::STALLED_CANCEL_GRACE,
            "given up after {:?}",
            cancelled.elapsed()
        );
        assert!(service.is_idle());
        drop(server);
        let removed = Instant::now();
        while dest.exists() {
            assert!(
                removed.elapsed() < Duration::from_secs(30),
                "the abandoned clone left {}",
                dest.display()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn late_cancels_for_finished_clones_are_capped() {
        let service = CloneService::with_store_factory(SharedStore::default);
        for n in 0..100 {
            let id = format!("clone-late-{n}");
            service.register(&id);
            service.unregister(&id);
            service.cancel(&id);
        }

        assert!(service.is_idle());
        assert_eq!(service.early_cancels(), super::EARLY_CANCELS);
    }

    #[test]
    fn an_early_cancel_is_used_once() {
        let service = CloneService::with_store_factory(SharedStore::default);
        service.cancel("clone-11");
        service.cancel("clone-11");

        assert!(service.register("clone-11").load(Ordering::Relaxed));
        assert_eq!(service.early_cancels(), 0);
        service.unregister("clone-11");
        assert!(!service.register("clone-11").load(Ordering::Relaxed));
    }

    #[test]
    fn register_cancel_and_unregister_manage_the_flag() {
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

    fn provider_input() -> HttpsCredentialInput {
        HttpsCredentialInput {
            username: "alice".into(),
            token: "tok".into(),
        }
    }

    #[test]
    fn one_shot_offers_the_token_once_over_https_then_reports_missing() {
        use git_core::remote::{CredentialProvider, MISSING_CREDENTIAL_ERROR};
        let input = provider_input();
        let mut provider = super::OneShotHttps::new(&input, "https://example.test/r.git");
        let allowed = git2::CredentialType::USER_PASS_PLAINTEXT;

        assert!(provider
            .credential("https://example.test/r.git", None, allowed)
            .is_ok());
        let second = provider.credential("https://example.test/r.git", None, allowed);
        assert_eq!(second.err().unwrap().message(), MISSING_CREDENTIAL_ERROR);
    }

    #[test]
    fn one_shot_refusal_does_not_consume_the_offer() {
        use git_core::remote::{CredentialProvider, MISSING_CREDENTIAL_ERROR};
        let input = provider_input();
        let mut provider = super::OneShotHttps::new(&input, "https://example.test/r.git");
        let url = "https://example.test/r.git";

        let refused = provider.credential(url, None, git2::CredentialType::SSH_KEY);
        assert_eq!(refused.err().unwrap().message(), MISSING_CREDENTIAL_ERROR);
        assert!(!provider.offered);

        assert!(provider
            .credential(url, None, git2::CredentialType::USER_PASS_PLAINTEXT)
            .is_ok());
        assert!(provider.offered);
        let again = provider.credential(url, None, git2::CredentialType::USER_PASS_PLAINTEXT);
        assert_eq!(again.err().unwrap().message(), MISSING_CREDENTIAL_ERROR);
    }

    #[test]
    fn one_shot_refuses_cleartext_http() {
        use git_core::remote::{CredentialProvider, MISSING_CREDENTIAL_ERROR};
        let input = provider_input();
        let mut provider = super::OneShotHttps::new(&input, "https://example.test/r.git");

        let result = provider.credential(
            "http://example.test/r.git",
            None,
            git2::CredentialType::USER_PASS_PLAINTEXT,
        );

        assert_eq!(result.err().unwrap().message(), MISSING_CREDENTIAL_ERROR);
    }

    #[test]
    fn one_shot_refuses_when_plaintext_credentials_are_not_allowed() {
        use git_core::remote::{CredentialProvider, MISSING_CREDENTIAL_ERROR};
        let input = provider_input();
        let mut provider = super::OneShotHttps::new(&input, "https://example.test/r.git");

        let result = provider.credential(
            "https://example.test/r.git",
            None,
            git2::CredentialType::SSH_KEY,
        );

        assert_eq!(result.err().unwrap().message(), MISSING_CREDENTIAL_ERROR);
    }

    /// #1126: libgit2 follows a redirect of the first request to another host and then asks
    /// for that host's credential; the token typed for the clone URL's host never goes there.
    #[test]
    fn one_shot_offers_the_token_only_to_the_clone_urls_host() {
        use git_core::remote::{CredentialProvider, MISSING_CREDENTIAL_ERROR};
        let input = provider_input();
        let mut provider =
            super::OneShotHttps::new(&input, "https://forge.example.test/team/r.git");
        let allowed = git2::CredentialType::USER_PASS_PLAINTEXT;

        for other in [
            "https://other.example.test/team/r.git/info/refs?service=git-upload-pack",
            "https://forge.example.test.other.example/team/r.git",
            "https://forge.example.test:8443/team/r.git",
            "not a url",
        ] {
            let refused = provider.credential(other, None, allowed);
            assert_eq!(
                refused.err().unwrap().message(),
                MISSING_CREDENTIAL_ERROR,
                "{other}"
            );
            assert!(!provider.offered, "{other}");
        }

        assert!(provider
            .credential(
                "https://FORGE.example.test:443/team/r.git/info/refs?service=git-upload-pack",
                None,
                allowed,
            )
            .is_ok());
        assert!(provider.offered);
    }

    #[test]
    fn one_shot_never_offers_the_token_for_a_clone_url_it_cannot_read() {
        use git_core::remote::{CredentialProvider, MISSING_CREDENTIAL_ERROR};
        let input = provider_input();
        let mut provider = super::OneShotHttps::new(&input, "forge.example.test/r.git");

        let result = provider.credential(
            "https://forge.example.test/r.git",
            None,
            git2::CredentialType::USER_PASS_PLAINTEXT,
        );

        assert_eq!(result.err().unwrap().message(), MISSING_CREDENTIAL_ERROR);
        assert!(!provider.offered);
    }

    /// #1159: the caller gives up only on a clone still running, and a clone is handed over
    /// only while the caller hasn't given up.
    #[test]
    fn a_clone_is_either_handed_over_or_given_up_on() {
        use super::{HelperReporter, Phase, SharedPhase};
        use git_core::remote::TransferReporter;

        let running = SharedPhase::new();
        assert!(running.abandon());
        assert_eq!(*running.lock(), Phase::Abandoned);
        assert!(!running.abandon());
        assert!(!running.complete());
        assert_eq!(*running.lock(), Phase::Abandoned);

        let finished = SharedPhase::new();
        let mut reporter = HelperReporter {
            messages: mpsc::channel().0,
            phase: finished.clone(),
        };
        assert!(reporter.clone_complete());
        assert_eq!(*finished.lock(), Phase::Complete);
        assert!(!finished.abandon());
        assert_eq!(*finished.lock(), Phase::Complete);
    }
}
