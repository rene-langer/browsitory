//! A file added with `git add -N` (intent to add): git lists it as an unstaged new file and
//! leaves it out of a commit (#1164). The git CLI sets the repositories up and is the reference
//! for the results.

mod common;

use std::path::Path;

use common::{commit_all, git_ok, init_repo, write_file};
use git_core::status::{StatusEntry, StatusKind};

fn git_out(dir: &Path, args: &[&str]) -> String {
    let output = test_support::git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn entry(path: &str, staged: bool, kind: StatusKind) -> StatusEntry {
    StatusEntry {
        path: path.to_string(),
        old_path: None,
        staged,
        kind,
        untracked_folder: false,
        external_filter: None,
        working_tree_encoding: None,
        intent_to_add: false,
    }
}

/// `a.txt` committed, then edited and staged; `new.txt` added with `git add -N`.
fn intent_to_add() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    commit_all(&repo, "base");
    write_file(d, "a.txt", "a, edited\n");
    git_ok(d, &["add", "a.txt"]);
    write_file(d, "new.txt", "my new content\n");
    git_ok(d, &["add", "-N", "new.txt"]);
    assert_eq!(git_out(d, &["status", "--short"]), "M  a.txt\n A new.txt\n");
    // Opened again, so its index is the one git wrote (as the worker re-reads it, #1167).
    drop(repo);
    let repo = git2::Repository::open(d).unwrap();
    (dir, repo)
}

#[test]
fn status_lists_an_intent_to_add_file_as_unstaged_new_only() {
    let (_dir, repo) = intent_to_add();
    let expected = vec![
        entry("a.txt", true, StatusKind::Modified),
        entry("new.txt", false, StatusKind::New),
    ];

    assert_eq!(git_core::status::status(&repo).unwrap(), expected);
    assert_eq!(
        git_core::status::status_refreshing_index(&repo).unwrap(),
        expected
    );
}

#[test]
fn status_lists_a_removed_intent_to_add_file_as_unstaged_deleted_only() {
    let (dir, repo) = intent_to_add();
    std::fs::remove_file(dir.path().join("new.txt")).unwrap();
    assert_eq!(
        git_out(dir.path(), &["status", "--short"]),
        "M  a.txt\n D new.txt\n"
    );

    assert_eq!(
        git_core::status::status(&repo).unwrap(),
        vec![
            entry("a.txt", true, StatusKind::Modified),
            entry("new.txt", false, StatusKind::Deleted),
        ]
    );
}

#[test]
fn a_commit_leaves_an_intent_to_add_file_out() {
    let (dir, mut repo) = intent_to_add();
    let d = dir.path();

    git_core::commit::commit(&mut repo, "second", git_core::hooks::HookOptions::default()).unwrap();

    assert_eq!(
        git_out(d, &["ls-tree", "-r", "--name-only", "HEAD"]),
        "a.txt\n"
    );
    assert_eq!(git_out(d, &["status", "--short"]), " A new.txt\n");
    assert_eq!(
        std::fs::read_to_string(d.join("new.txt")).unwrap(),
        "my new content\n"
    );
}

#[test]
fn an_amend_leaves_an_intent_to_add_file_out() {
    let (dir, repo) = intent_to_add();
    let d = dir.path();

    git_core::commit::amend_head(
        &repo,
        "base, amended",
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    assert_eq!(
        git_out(d, &["ls-tree", "-r", "--name-only", "HEAD"]),
        "a.txt\n"
    );
    assert_eq!(git_out(d, &["show", "HEAD:a.txt"]), "a, edited\n");
    assert_eq!(git_out(d, &["status", "--short"]), " A new.txt\n");
}

/// Every entry left in the index is intent-to-add: the commit records an empty tree.
#[test]
fn a_commit_of_an_index_holding_only_an_intent_to_add_entry_records_an_empty_tree() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    commit_all(&repo, "base");
    git_ok(d, &["rm", "-q", "--cached", "a.txt"]);
    write_file(d, "new.txt", "my new content\n");
    git_ok(d, &["add", "-N", "new.txt"]);
    drop(repo);
    let mut repo = git2::Repository::open(d).unwrap();

    git_core::commit::commit(&mut repo, "drop a", git_core::hooks::HookOptions::default()).unwrap();

    assert_eq!(git_out(d, &["ls-tree", "-r", "--name-only", "HEAD"]), "");
    assert_eq!(git_out(d, &["status", "--short"]), " A new.txt\n?? a.txt\n");
}

/// `a.txt` committed, then `new.txt` ("committed") in a second commit; the index and HEAD put
/// back to the first, and `new.txt` ("mine") added with `git add -N`. Returns the second commit.
fn intent_to_add_the_target_has(d: &Path) -> String {
    let repo = git2::Repository::init(d).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        config.set_bool("core.autocrlf", false).unwrap();
    }
    write_file(d, "a.txt", "a\n");
    commit_all(&repo, "one");
    write_file(d, "new.txt", "committed\n");
    commit_all(&repo, "two");
    let two = git_out(d, &["rev-parse", "HEAD"]).trim().to_string();
    git_ok(d, &["reset", "-q", "--mixed", "HEAD~1"]);
    write_file(d, "new.txt", "mine\n");
    git_ok(d, &["add", "-N", "new.txt"]);
    two
}

/// A hard reset to a commit that has the intent-to-add path does what `git reset --hard` does
/// in the same repository (the git CLI on a twin is the reference).
#[test]
fn a_hard_reset_to_a_commit_with_an_intent_to_add_path_matches_git() {
    let twin = tempfile::TempDir::new().unwrap();
    let ours = tempfile::TempDir::new().unwrap();
    let twin_target = intent_to_add_the_target_has(twin.path());
    let our_target = intent_to_add_the_target_has(ours.path());
    git_ok(twin.path(), &["reset", "-q", "--hard", &twin_target]);
    let repo = git2::Repository::open(ours.path()).unwrap();

    let ours_result =
        git_core::reset::reset_to(&repo, &our_target, git_core::reset::ResetMode::Hard);

    assert!(ours_result.is_ok(), "{ours_result:?}");
    for args in [
        &["ls-files", "-v", "-s"][..],
        &["status", "--short"][..],
        &["rev-parse", "HEAD^{tree}"][..],
    ] {
        assert_eq!(
            git_out(ours.path(), args),
            git_out(twin.path(), args),
            "{args:?}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(ours.path().join("new.txt")).unwrap(),
        std::fs::read_to_string(twin.path().join("new.txt")).unwrap()
    );
}
