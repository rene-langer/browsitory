//! #1064: the app sets libgit2's server timeout, so a fetch or push that stalls after connecting
//! fails once nothing arrived for that long instead of waiting forever. One test in its own
//! binary, because the setting is process-wide.

mod common;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use git_core::remote::{
    fetch_remote, CredentialProvider, RemoteError, TransferErrorKind, TransferProgress,
    TransferReporter,
};
use test_support::StalledServer;

struct NoCredentials;

impl CredentialProvider for NoCredentials {
    fn credential(
        &mut self,
        _url: &str,
        _username: Option<&str>,
        _allowed: git2::CredentialType,
    ) -> Result<git2::Cred, git2::Error> {
        Err(git2::Error::from_str("no credentials"))
    }
}

struct Quiet;

impl TransferReporter for Quiet {
    fn report(&mut self, _event: TransferProgress) {}
}

#[test]
fn configure_libgit2_sets_a_server_timeout_that_ends_a_stalled_fetch() {
    git_core::repo::configure_libgit2();
    // SAFETY: the test binary's only test; no other thread uses libgit2 now.
    let timeout = unsafe { git2::opts::get_server_timeout_in_milliseconds() }.unwrap();
    assert_eq!(timeout, 120_000);

    // The same option, short enough for a test: libgit2 honours it on a read after connecting.
    // SAFETY: as above.
    unsafe { git2::opts::set_server_timeout_in_milliseconds(300) }.unwrap();
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "README.md", "initial commit\n");
    common::commit_all(&repo, "initial commit");
    let server = StalledServer::start();
    repo.remote("stalled", &server.url).unwrap();
    drop(repo);
    let path = dir.path().to_path_buf();
    let (done_tx, done) = mpsc::channel();
    std::thread::spawn(move || {
        let repo = git2::Repository::open(path).unwrap();
        let started = Instant::now();
        let result = fetch_remote(
            &repo,
            "stalled",
            None,
            "fetch-1".to_string(),
            &mut NoCredentials,
            &mut Quiet,
            &|| false,
        );
        let _ = done_tx.send((result, started.elapsed()));
    });

    server
        .stalled
        .recv_timeout(Duration::from_secs(30))
        .expect("the fetch connects and asks for the branch");
    let (result, took) = done
        .recv_timeout(Duration::from_secs(20))
        .expect("the stalled fetch ends once the server timeout passes");
    let error = result.expect_err("a stalled fetch fails");
    assert!(matches!(error, RemoteError::Git(_)), "{error:?}");
    assert_eq!(
        error.transfer_error_kind(),
        TransferErrorKind::TransferFailed
    );
    assert!(took < Duration::from_secs(20), "{took:?}");
}
