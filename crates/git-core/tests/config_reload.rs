//! Core settings changed outside the app while a repository is open (#904): libgit2 caches
//! `core.autocrlf` and the other core settings per open repository and forgets them only when
//! the repository's own config object writes. `repo::reload_core_settings` makes it read them
//! again.

mod common;

use std::path::Path;

use common::{commit_all, init_repo, user_config, write_file};

fn git_config(dir: &Path, key: &str, value: &str) {
    common::git_ok(dir, &["config", key, value]);
}

/// The staged content of `path`, as git reads it.
fn staged(dir: &Path, path: &str) -> Vec<u8> {
    let output = test_support::git(dir, &["cat-file", "blob", &format!(":{path}")]);
    assert!(output.status.success(), "{output:?}");
    output.stdout
}

/// A repository with one commit whose `core.autocrlf=false` libgit2 already cached, by staging
/// `a.txt` with CRLF.
fn cached_autocrlf_off() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "base\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "a.txt", "one\r\n");
    git_core::stage::stage_file(&repo, "a.txt").unwrap();
    assert_eq!(staged(dir.path(), "a.txt"), b"one\r\n");
    (dir, repo)
}

#[test]
fn staging_after_a_reload_follows_core_autocrlf_set_by_git() {
    let (dir, repo) = cached_autocrlf_off();
    git_config(dir.path(), "core.autocrlf", "true");
    write_file(dir.path(), "b.txt", "two\r\n");

    git_core::repo::reload_core_settings(&repo).unwrap();
    git_core::stage::stage_file(&repo, "b.txt").unwrap();

    assert_eq!(staged(dir.path(), "b.txt"), b"two\n");
}

#[test]
fn a_reload_keeps_a_config_level_added_to_the_open_repository() {
    let (dir, repo) = cached_autocrlf_off();
    user_config(&repo).set_bool("core.autocrlf", true).unwrap();
    write_file(dir.path(), "b.txt", "two\r\n");
    // The local file's `false` beats the added global level; without it the global `true` counts.
    test_support::git(dir.path(), &["config", "--unset", "core.autocrlf"]);

    git_core::repo::reload_core_settings(&repo).unwrap();
    git_core::stage::stage_file(&repo, "b.txt").unwrap();

    assert_eq!(staged(dir.path(), "b.txt"), b"two\n");
}

#[test]
fn after_a_reload_the_repositorys_own_config_writes_still_take_effect() {
    let (dir, repo) = cached_autocrlf_off();
    git_core::repo::reload_core_settings(&repo).unwrap();
    git_core::stage::stage_file(&repo, "a.txt").unwrap();
    repo.config()
        .unwrap()
        .set_bool("core.autocrlf", true)
        .unwrap();
    write_file(dir.path(), "b.txt", "two\r\n");

    git_core::stage::stage_file(&repo, "b.txt").unwrap();

    assert_eq!(staged(dir.path(), "b.txt"), b"two\n");
}
