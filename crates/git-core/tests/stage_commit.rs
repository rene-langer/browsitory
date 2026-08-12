mod common;

use common::{init_repo, write_file};
use git_core::commit::commit;
use git_core::stage::{stage_file, unstage_file};
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
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "hello");
    stage_file(&repo, "tracked.txt").unwrap();
    commit(&repo, "add file").unwrap();

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
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "hello");
    stage_file(&repo, "tracked.txt").unwrap();
    commit(&repo, "add file").unwrap();

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
fn commit_creates_a_commit_with_the_given_message_and_staged_content() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "greeting.txt", "hello");
    stage_file(&repo, "greeting.txt").unwrap();

    let oid = commit(&repo, "add greeting").unwrap();

    assert!(git_core::status::status(&repo).unwrap().is_empty());
    let commit = repo
        .find_commit(git2::Oid::from_str(&oid).unwrap())
        .unwrap();
    assert_eq!(commit.message().unwrap(), "add greeting");
}

#[test]
fn commit_on_a_fresh_repo_creates_a_parentless_first_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "greeting.txt", "hello");
    stage_file(&repo, "greeting.txt").unwrap();

    let oid = commit(&repo, "add greeting").unwrap();

    let commit = repo
        .find_commit(git2::Oid::from_str(&oid).unwrap())
        .unwrap();
    assert_eq!(commit.parent_count(), 0);
}

#[test]
fn commit_without_a_configured_identity_returns_an_error() {
    // Isolate libgit2's global/system/XDG config search paths to an empty directory so
    // this test is deterministic regardless of whether the host machine has a real
    // `user.name`/`user.email` configured in `~/.gitconfig` — otherwise `repo.signature()`
    // would silently fall back to the host's identity and this assertion would flake.
    let empty_config_dir = tempfile::TempDir::new().unwrap();
    unsafe {
        git2::opts::set_search_path(git2::ConfigLevel::Global, empty_config_dir.path()).unwrap();
        git2::opts::set_search_path(git2::ConfigLevel::System, empty_config_dir.path()).unwrap();
        git2::opts::set_search_path(git2::ConfigLevel::XDG, empty_config_dir.path()).unwrap();
    }

    let dir = tempfile::TempDir::new().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    write_file(dir.path(), "greeting.txt", "hello");
    stage_file(&repo, "greeting.txt").unwrap();

    assert!(commit(&repo, "msg").is_err());
}
