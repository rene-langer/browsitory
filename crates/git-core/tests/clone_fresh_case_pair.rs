//! #512: a clone whose HEAD commit holds a case pair (`A` and `a`), with a global
//! `core.ignorecase=true` applying to the new repository on a case-sensitive file system.
//! libgit2's ignore-case index would keep only one of the two names. The clone is refused
//! (`CasePair`) and removed, as a clone with a case collision is (#156).
//!
//! One test only: it points libgit2's global config search path at its own `.gitconfig`, which
//! no other test in the same process may see.
#![cfg(target_os = "linux")]

mod common;

use common::{commit_all, init_repo, write_file};
use git2::ConfigLevel;
use git_core::clone::{clone_repo, CloneError, CloneOptions};
use git_core::remote::{CredentialProvider, TransferProgress, TransferReporter};

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

fn clone(src: &std::path::Path, dest: std::path::PathBuf) -> Result<(), CloneError> {
    let opts = CloneOptions {
        url: src.to_string_lossy().into_owned(),
        dest,
        branch: None,
        recurse_submodules: false,
    };
    clone_repo(&opts, &mut NoCredentials, &mut Quiet, &|| false).map(|_| ())
}

#[test]
fn a_clone_holding_a_case_pair_under_a_global_ignorecase_is_refused() {
    let global = test_support::paths()
        .search_dir
        .parent()
        .unwrap()
        .join("clone-ignorecase-global");
    std::fs::create_dir_all(&global).unwrap();
    std::fs::write(global.join(".gitconfig"), "[core]\n\tignorecase = true\n").unwrap();

    // Committed with the case folding off: the source repository's own config says so.
    let (pair_dir, pair_src) = init_repo();
    pair_src
        .config()
        .unwrap()
        .set_bool("core.ignorecase", false)
        .unwrap();
    write_file(pair_dir.path(), "A", "upper\n");
    write_file(pair_dir.path(), "a", "lower\n");
    commit_all(&pair_src, "pair");
    let (clean_dir, clean_src) = init_repo();
    write_file(clean_dir.path(), "a", "lower\n");
    commit_all(&clean_src, "clean");

    // SAFETY: this binary has exactly one test, so no other thread is using libgit2 while the
    // search path changes.
    unsafe {
        git2::opts::set_search_path(ConfigLevel::Global, &global).unwrap();
    }

    let parent = tempfile::TempDir::new().unwrap();
    let dest = parent.path().join("pair");
    let error = clone(pair_dir.path(), dest.clone()).unwrap_err();
    assert!(matches!(error, CloneError::CasePair(_)), "{error}");
    assert_eq!(
        error.to_string(),
        "'A' and 'a' differ only in case, which this repository's core.ignorecase setting \
         takes for one name, so the checkout would remove one of them. Rename one of the two \
         in the repository first."
    );
    assert!(!dest.exists());

    // A clone without a pair is checked out as before.
    let dest = parent.path().join("clean");
    clone(clean_dir.path(), dest.clone()).unwrap();
    assert_eq!(std::fs::read_to_string(dest.join("a")).unwrap(), "lower\n");
}
