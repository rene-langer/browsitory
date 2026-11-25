//! #1151: Undo and Redo of Rename branch and Delete branch bring a branch's settings
//! (`branch.<name>.*`: upstream, description…) back with its ref, and leave no section behind
//! under the other name. The git CLI sets the repositories up and is the reference.

mod common;

use std::path::Path;

use common::{commit_all, git_ok, init_repo, write_file};
use git_core::undo::{capture, restore, SnapshotScope, UndoError};

fn git_out(dir: &Path, args: &[&str]) -> String {
    let output = test_support::git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// Every `branch.*` setting, as `git config --get-regexp` lists them (empty when there is none).
fn branch_config(dir: &Path) -> String {
    let output = test_support::git(dir, &["config", "--local", "--get-regexp", r"^branch\."]);
    String::from_utf8(output.stdout).unwrap()
}

/// `main` and `feature`, each tracking `origin/<name>`; `feature` has a description too.
fn tracked_branches() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    commit_all(&repo, "base");
    git_ok(d, &["branch", "-M", "main"]);
    git_ok(
        d,
        &["remote", "add", "origin", "https://example.invalid/r.git"],
    );
    git_ok(d, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    git_ok(d, &["update-ref", "refs/remotes/origin/feature", "HEAD"]);
    git_ok(d, &["branch", "-u", "origin/main", "main"]);
    git_ok(d, &["branch", "feature"]);
    git_ok(d, &["branch", "-u", "origin/feature", "feature"]);
    git_ok(d, &["config", "branch.feature.description", "my notes"]);
    assert_eq!(branch_config(d), BOTH);
    (dir, repo)
}

const BOTH: &str = "branch.main.remote origin\n\
branch.main.merge refs/heads/main\n\
branch.feature.remote origin\n\
branch.feature.merge refs/heads/feature\n\
branch.feature.description my notes\n";

/// The settings as `git config --get-regexp` lists them, `feature`'s under `name`.
fn feature_as(name: &str) -> String {
    format!(
        "branch.main.remote origin\n\
branch.main.merge refs/heads/main\n\
branch.{name}.remote origin\n\
branch.{name}.merge refs/heads/feature\n\
branch.{name}.description my notes\n"
    )
}

fn upstream_of(dir: &Path, branch: &str) -> String {
    git_out(
        dir,
        &[
            "for-each-ref",
            "--format=%(upstream:short)",
            &format!("refs/heads/{branch}"),
        ],
    )
}

#[test]
fn undo_and_redo_of_a_rename_move_the_settings_back_and_forth() {
    let (dir, repo) = tracked_branches();
    let d = dir.path();
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    git_core::branch::rename_branch(&repo, "feature", "feat2").unwrap();
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    assert_eq!(branch_config(d), feature_as("feat2"));

    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(upstream_of(d, "feature"), "origin/feature\n");
    assert_eq!(
        git_out(d, &["config", "branch.feature.description"]),
        "my notes\n"
    );
    assert!(!branch_config(d).contains("feat2"));

    restore(&repo, &before, &after, "browsitory: redo test").unwrap();
    assert_eq!(upstream_of(d, "feat2"), "origin/feature\n");
    assert!(!branch_config(d).contains("branch.feature."));
}

#[test]
fn undo_of_renaming_the_current_branch_gives_back_its_upstream() {
    let (dir, repo) = tracked_branches();
    let d = dir.path();
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    git_core::branch::rename_branch(&repo, "main", "trunk").unwrap();
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();

    restore(&repo, &after, &before, "browsitory: undo test").unwrap();

    assert_eq!(
        git_out(d, &["status", "-sb"]).lines().next(),
        Some("## main...origin/main")
    );
    assert!(!branch_config(d).contains("trunk"));
}

#[test]
fn undo_and_redo_of_a_delete_bring_the_settings_back_and_remove_them() {
    let (dir, repo) = tracked_branches();
    let d = dir.path();
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    git_core::branch::delete_branch(&repo, "feature", true).unwrap();
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    assert!(!branch_config(d).contains("feature"));

    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(branch_config(d), BOTH);
    assert_eq!(upstream_of(d, "feature"), "origin/feature\n");

    restore(&repo, &before, &after, "browsitory: redo test").unwrap();
    assert!(!branch_config(d).contains("feature"));
}

/// The settings under the new name changed since the rename: like a moved ref, Undo refuses and
/// writes nothing.
#[test]
fn undo_of_a_rename_refuses_when_the_settings_changed_since() {
    let (dir, repo) = tracked_branches();
    let d = dir.path();
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    git_core::branch::rename_branch(&repo, "feature", "feat2").unwrap();
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    git_ok(d, &["branch", "-u", "origin/main", "feat2"]);
    let config = branch_config(d);

    let result = restore(&repo, &after, &before, "browsitory: undo test");

    assert!(
        matches!(result, Err(UndoError::RepositoryChanged)),
        "{result:?}"
    );
    assert_eq!(branch_config(d), config);
    assert!(repo.find_branch("feat2", git2::BranchType::Local).is_ok());
}

/// A branch whose settings an unrecorded command changed (Set upstream) and whose ref the entry
/// doesn't create or delete keeps them: Undo of an unrelated change never touches them.
#[test]
fn undo_leaves_the_settings_of_branches_it_does_not_create_or_delete() {
    let (dir, repo) = tracked_branches();
    let d = dir.path();
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    git_ok(d, &["branch", "-u", "origin/main", "feature"]);
    git_ok(d, &["tag", "v1"]);
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();

    restore(&repo, &after, &before, "browsitory: undo test").unwrap();

    assert_eq!(upstream_of(d, "feature"), "origin/main\n");
    assert!(repo.find_reference("refs/tags/v1").is_err());
}

/// Appends a setting written without a value (`flag` alone, boolean true) for `branch`.
fn add_valueless_flag(repo: &git2::Repository, branch: &str) {
    let config = repo.path().join("config");
    let mut text = std::fs::read_to_string(&config).unwrap();
    text.push_str(&format!("[branch \"{branch}\"]\n\tflag\n"));
    std::fs::write(&config, text).unwrap();
}

/// A setting written without a value comes back true. (Deleted with the git CLI: libgit2's own
/// branch delete crashes on such a setting.) Redo would have to remove one again, which libgit2
/// can't do safely either, so it refuses and changes nothing.
#[test]
fn a_setting_without_a_value_comes_back_true_and_is_never_removed() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    commit_all(&repo, "base");
    git_ok(d, &["branch", "flagged"]);
    add_valueless_flag(&repo, "flagged");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    git_ok(d, &["branch", "-D", "flagged"]);
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();

    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(
        git_out(d, &["config", "--type=bool", "branch.flagged.flag"]),
        "true\n"
    );

    git_ok(d, &["config", "--unset", "branch.flagged.flag"]);
    add_valueless_flag(&repo, "flagged");
    let config = branch_config(d);
    let result = restore(&repo, &before, &after, "browsitory: redo test");
    assert!(
        matches!(result, Err(UndoError::RepositoryChanged)),
        "{result:?}"
    );
    assert_eq!(branch_config(d), config);
    assert!(repo.find_branch("flagged", git2::BranchType::Local).is_ok());
}

/// A branch name with a dot and settings with several values of one key round-trip in order.
#[test]
fn undo_of_a_delete_keeps_multi_valued_settings_of_a_dotted_branch() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    commit_all(&repo, "base");
    git_ok(d, &["branch", "release.1"]);
    git_ok(
        d,
        &["config", "--add", "branch.release.1.merge", "refs/heads/x"],
    );
    git_ok(
        d,
        &["config", "--add", "branch.release.1.merge", "refs/heads/y"],
    );
    git_ok(d, &["config", "branch.release.1.remote", "."]);
    let config = branch_config(d);
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    git_core::branch::delete_branch(&repo, "release.1", true).unwrap();
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();

    restore(&repo, &after, &before, "browsitory: undo test").unwrap();

    assert_eq!(branch_config(d), config);
}
