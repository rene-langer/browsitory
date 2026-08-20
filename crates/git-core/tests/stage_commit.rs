mod common;

use common::{init_repo, write_file};
use git_core::commit::commit;
use git_core::stage::{stage_file, stage_hunk, unstage_file};
use git_core::status::StatusKind;

#[test]
fn stage_file_adds_a_new_file_to_the_index() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "new.txt", "hello");

    stage_file(&repo, "new.txt").unwrap();

    let entries = git_core::status::status(&repo).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "new.txt");
    assert!(entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::New);
}

#[test]
fn stage_file_stages_a_deletion() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "hello");
    stage_file(&repo, "tracked.txt").unwrap();
    commit(&mut repo, "add file").unwrap();

    std::fs::remove_file(dir.path().join("tracked.txt")).unwrap();
    stage_file(&repo, "tracked.txt").unwrap();

    let entries = git_core::status::status(&repo).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "tracked.txt");
    assert!(entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::Deleted);
}

#[test]
fn unstage_file_restores_the_index_entry_from_head() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "hello");
    stage_file(&repo, "tracked.txt").unwrap();
    commit(&mut repo, "add file").unwrap();

    write_file(dir.path(), "tracked.txt", "changed");
    stage_file(&repo, "tracked.txt").unwrap();

    unstage_file(&repo, "tracked.txt").unwrap();

    let entries = git_core::status::status(&repo).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "tracked.txt");
    assert!(!entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::Modified);
}

#[test]
fn unstage_file_on_a_newly_staged_file_makes_it_untracked_again() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "new.txt", "hello");
    stage_file(&repo, "new.txt").unwrap();

    unstage_file(&repo, "new.txt").unwrap();

    let entries = git_core::status::status(&repo).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "new.txt");
    assert!(!entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::New);
}

#[test]
fn stage_hunk_stages_only_the_targeted_hunk_leaving_the_other_unstaged() {
    let (dir, mut repo) = init_repo();
    let original: String = (1..=15).map(|n| format!("line {n}\n")).collect();
    write_file(dir.path(), "tracked.txt", &original);
    stage_file(&repo, "tracked.txt").unwrap();
    commit(&mut repo, "initial commit").unwrap();

    let mut lines: Vec<String> = (1..=15).map(|n| format!("line {n}")).collect();
    lines[1] = "line 2 changed".to_string();
    lines[13] = "line 14 changed".to_string();
    let changed = lines.join("\n") + "\n";
    write_file(dir.path(), "tracked.txt", &changed);

    let hunks = git_core::diff::working_diff(&repo, "tracked.txt", false).unwrap();
    assert_eq!(hunks.len(), 2, "expected two separate hunks from two far-apart edits");

    stage_hunk(&repo, "tracked.txt", hunks[0].old_start, hunks[0].new_start).unwrap();

    let staged = git_core::diff::working_diff(&repo, "tracked.txt", true).unwrap();
    let staged_text: String = staged.iter().flat_map(|h| h.lines.iter()).map(|l| l.content.clone()).collect();
    assert!(staged_text.contains("line 2 changed"));
    assert!(!staged_text.contains("line 14 changed"));

    let still_unstaged = git_core::diff::working_diff(&repo, "tracked.txt", false).unwrap();
    let unstaged_text: String = still_unstaged.iter().flat_map(|h| h.lines.iter()).map(|l| l.content.clone()).collect();
    assert!(unstaged_text.contains("line 14 changed"));
    assert!(!unstaged_text.contains("line 2 changed"));
}

#[test]
fn stage_hunk_on_a_hunk_that_no_longer_matches_returns_hunk_not_found() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\n");
    stage_file(&repo, "tracked.txt").unwrap();
    commit(&mut repo, "initial commit").unwrap();
    write_file(dir.path(), "tracked.txt", "line one changed\n");

    let result = stage_hunk(&repo, "tracked.txt", 999, 999);

    assert!(matches!(result, Err(git_core::stage::StageError::HunkNotFound)));
}

#[test]
fn commit_creates_a_commit_with_the_given_message_and_staged_content() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "greeting.txt", "hello");
    stage_file(&repo, "greeting.txt").unwrap();

    let oid = commit(&mut repo, "add greeting").unwrap();

    assert!(git_core::status::status(&repo).unwrap().is_empty());
    let commit = repo
        .find_commit(git2::Oid::from_str(&oid).unwrap())
        .unwrap();
    assert_eq!(commit.message().unwrap(), "add greeting");
}

#[test]
fn commit_on_a_fresh_repo_creates_a_parentless_first_commit() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "greeting.txt", "hello");
    stage_file(&repo, "greeting.txt").unwrap();

    let oid = commit(&mut repo, "add greeting").unwrap();

    let commit = repo
        .find_commit(git2::Oid::from_str(&oid).unwrap())
        .unwrap();
    assert_eq!(commit.parent_count(), 0);
}

/// `git2::opts::{set,get,reset}_search_path` mutate process-global libgit2 state and are
/// documented as needing external synchronization. This mutex serializes access to that
/// global state across tests in this binary (`cargo test` runs tests multi-threaded by
/// default), and `ConfigSearchPathOverride` below saves/restores the prior value via RAII
/// so the mutation never leaks past the one test that needs it — even if that test panics.
static CONFIG_SEARCH_PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Points libgit2's Global/System/XDG config search paths at an empty directory for the
/// life of this guard, restoring the previous paths on drop. Holds `CONFIG_SEARCH_PATH_LOCK`
/// for the duration so no other test's `repo.signature()`/config lookups can observe the
/// overridden paths.
struct ConfigSearchPathOverride<'a> {
    _lock: std::sync::MutexGuard<'a, ()>,
    previous: Vec<(git2::ConfigLevel, std::ffi::CString)>,
}

impl ConfigSearchPathOverride<'_> {
    const LEVELS: [git2::ConfigLevel; 3] = [
        git2::ConfigLevel::Global,
        git2::ConfigLevel::System,
        git2::ConfigLevel::XDG,
    ];

    fn install(empty_dir: &std::path::Path) -> Self {
        let lock = CONFIG_SEARCH_PATH_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // SAFETY: `_lock` (held for the lifetime of `self`) serializes this against every
        // other `get_search_path`/`set_search_path` call in this test binary.
        let previous: Vec<_> = Self::LEVELS
            .iter()
            .map(|&level| {
                let path = unsafe { git2::opts::get_search_path(level).unwrap() };
                (level, path)
            })
            .collect();

        for &level in &Self::LEVELS {
            unsafe {
                git2::opts::set_search_path(level, empty_dir).unwrap();
            }
        }

        ConfigSearchPathOverride {
            _lock: lock,
            previous,
        }
    }
}

impl Drop for ConfigSearchPathOverride<'_> {
    fn drop(&mut self) {
        // SAFETY: still holding `_lock`.
        for (level, path) in &self.previous {
            unsafe {
                git2::opts::set_search_path(*level, path).unwrap();
            }
        }
    }
}

#[test]
fn commit_without_a_configured_identity_returns_an_error() {
    // Isolate libgit2's global/system/XDG config search paths to an empty directory so
    // this test is deterministic regardless of whether the host machine has a real
    // `user.name`/`user.email` configured in `~/.gitconfig` — otherwise `repo.signature()`
    // would silently fall back to the host's identity and this assertion would flake.
    let empty_config_dir = tempfile::TempDir::new().unwrap();
    let _config_override = ConfigSearchPathOverride::install(empty_config_dir.path());

    let dir = tempfile::TempDir::new().unwrap();
    let mut repo = git2::Repository::init(dir.path()).unwrap();
    write_file(dir.path(), "greeting.txt", "hello");
    stage_file(&repo, "greeting.txt").unwrap();

    assert!(commit(&mut repo, "msg").is_err());
}
