//! #1181: Delete branch, Rename branch, Rename remote and Remove remote change exactly the
//! settings git's `git branch -D`, `git branch -m`, `git remote rename` and `git remote rm`
//! change. libgit2 picked a section's settings by `<section>.` anywhere in a setting's name and
//! in every config file: deleting branch `release` took branch `release.1`'s settings too,
//! renaming remote `origin` garbled Browsitory's own `browsitory.remote.origin.*`, and a
//! setting of the section in the user's own config failed the command halfway. Each test runs
//! the git command on a copy of the repository (the reference) and compares.

mod common;

use std::path::Path;

use common::{commit_all, git_ok, init_repo, write_file};
use git2::Repository;
use git_core::remote::RemoteAuthMode;
use tempfile::TempDir;

/// Every setting of the repository's own config file, as `git config --local --list` prints
/// them, sorted: git keeps a renamed section where it was, Browsitory writes it at the end.
fn local_config(dir: &Path) -> Vec<String> {
    let output = test_support::git(dir, &["config", "--local", "--list"]);
    assert!(output.status.success(), "git config --list: {output:?}");
    let mut lines: Vec<String> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    lines.sort();
    lines
}

/// Every ref with its target (a symbolic ref's target ref, else its commit).
fn refs(dir: &Path) -> String {
    let output = test_support::git(
        dir,
        &[
            "for-each-ref",
            "--format=%(refname) %(symref) %(objectname)",
        ],
    );
    assert!(output.status.success(), "git for-each-ref: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// A copy of the repository in `dir`, for the git CLI to run the reference command on.
fn twin(dir: &Path) -> TempDir {
    let copy = TempDir::new().unwrap();
    copy_dir(dir, copy.path());
    copy
}

fn copy_dir(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn has_branch(repo: &Repository, name: &str) -> bool {
    repo.find_branch(name, git2::BranchType::Local).is_ok()
}

/// A commit on `main` and branches `release`, `release.1` and `release.1.x`, each with
/// settings; a URL rewrite whose name holds `branch.release.` in the middle.
fn dotted_branches() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    commit_all(&repo, "base");
    git_ok(d, &["branch", "-M", "main"]);
    for branch in ["release", "release.1", "release.1.x"] {
        git_ok(d, &["branch", branch]);
    }
    for (name, value) in [
        ("branch.release.description", "own"),
        ("branch.release.remote", "."),
        ("branch.release.merge", "refs/heads/main"),
        ("branch.release.1.description", "dotted"),
        ("branch.release.1.remote", "."),
        ("branch.release.1.merge", "refs/heads/release"),
        ("branch.release.1.x.description", "dotted twice"),
        (
            "url.https://example.invalid/branch.release.git.insteadOf",
            "short:",
        ),
    ] {
        git_ok(d, &["config", name, value]);
    }
    (dir, repo)
}

#[test]
fn delete_branch_removes_only_its_own_settings_as_git_does() {
    let (dir, repo) = dotted_branches();
    let d = dir.path();
    let reference = twin(d);
    git_ok(reference.path(), &["branch", "-D", "release"]);

    git_core::branch::delete_branch(&repo, "release", true).unwrap();

    assert_eq!(local_config(d), local_config(reference.path()));
    assert_eq!(refs(d), refs(reference.path()));
    assert!(local_config(d).contains(&"branch.release.1.description=dotted".to_string()));
    assert!(!has_branch(&repo, "release"));
}

#[test]
fn rename_branch_moves_only_its_own_settings_as_git_does() {
    let (dir, repo) = dotted_branches();
    let d = dir.path();
    let reference = twin(d);
    git_ok(reference.path(), &["branch", "-m", "release", "renamed"]);

    git_core::branch::rename_branch(&repo, "release", "renamed").unwrap();

    assert_eq!(local_config(d), local_config(reference.path()));
    assert_eq!(refs(d), refs(reference.path()));
    let config = local_config(d);
    assert!(config.contains(&"branch.renamed.description=own".to_string()));
    assert!(config.contains(&"branch.release.1.description=dotted".to_string()));
    assert!(!config
        .iter()
        .any(|line| line.starts_with("branch.renamed.1")));
}

/// A branch with settings in the repository's file and one in the user's own config.
fn branch_with_user_setting() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    commit_all(&repo, "base");
    git_ok(d, &["branch", "-M", "main"]);
    git_ok(d, &["branch", "release"]);
    git_ok(d, &["config", "branch.release.description", "own"]);
    let mut user = common::user_config(&repo);
    user.set_str("branch.release.rebase", "true").unwrap();
    (dir, repo)
}

fn user_config_text(repo: &Repository) -> String {
    std::fs::read_to_string(repo.path().join("user.gitconfig")).unwrap()
}

/// git rewrites only the repository's own file; the user's setting stays where it is.
#[test]
fn delete_branch_leaves_a_setting_of_the_users_own_config_alone() {
    let (dir, repo) = branch_with_user_setting();
    let user = user_config_text(&repo);

    git_core::branch::delete_branch(&repo, "release", true).unwrap();

    assert!(!has_branch(&repo, "release"));
    assert!(!local_config(dir.path())
        .iter()
        .any(|line| line.starts_with("branch.release.")));
    assert_eq!(user_config_text(&repo), user);
}

/// The repository's settings move; the user's isn't copied into the repository's file.
#[test]
fn rename_branch_moves_only_the_settings_of_the_repositorys_own_file() {
    let (dir, repo) = branch_with_user_setting();
    let user = user_config_text(&repo);

    git_core::branch::rename_branch(&repo, "release", "renamed").unwrap();

    assert!(has_branch(&repo, "renamed"));
    let renamed: Vec<String> = local_config(dir.path())
        .into_iter()
        .filter(|line| line.starts_with("branch.re"))
        .collect();
    assert_eq!(renamed, ["branch.renamed.description=own"]);
    assert_eq!(user_config_text(&repo), user);
}

/// Remotes `origin` (two fetch refspecs, a push refspec, `origin/main` and `origin/HEAD`) and
/// `origin.mirror` (`origin.mirror/main`); branch `topic` pushes to `origin`, as does
/// `remote.pushDefault`; the settings of a deleted branch `gone.1` track `origin`, those of a
/// deleted branch `gone` another remote.
fn remotes() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    commit_all(&repo, "base");
    git_ok(d, &["branch", "-M", "main"]);
    git_ok(
        d,
        &["remote", "add", "origin", "https://example.invalid/r.git"],
    );
    git_ok(
        d,
        &[
            "remote",
            "add",
            "origin.mirror",
            "https://example.invalid/m.git",
        ],
    );
    git_ok(d, &["branch", "topic"]);
    for (name, value) in [
        ("branch.topic.pushRemote", "origin"),
        ("remote.pushDefault", "origin"),
        ("branch.gone.1.remote", "origin"),
        ("branch.gone.1.merge", "refs/heads/x"),
        ("branch.gone.remote", "other"),
        ("branch.gone.merge", "refs/heads/y"),
        ("remote.origin.push", "refs/heads/topic:refs/heads/topic"),
    ] {
        git_ok(d, &["config", name, value]);
    }
    git_ok(
        d,
        &[
            "config",
            "--add",
            "remote.origin.fetch",
            "+refs/heads/main:refs/remotes/origin/main-only",
        ],
    );
    git_ok(d, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    git_ok(
        d,
        &["update-ref", "refs/remotes/origin.mirror/main", "HEAD"],
    );
    git_ok(
        d,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    (dir, repo)
}

/// git moves the section (its fetch refspecs now naming `upstream`), the remote's refs, and
/// every `branch.*.remote`, `branch.*.pushRemote` and `remote.pushDefault` naming it; remote
/// `origin.mirror` stays as it is.
#[test]
fn rename_remote_moves_what_git_moves() {
    let (dir, repo) = remotes();
    let d = dir.path();
    let reference = twin(d);
    git_ok(
        reference.path(),
        &["remote", "rename", "origin", "upstream"],
    );

    git_core::remote::rename_remote(&repo, "origin", "upstream").unwrap();

    assert_eq!(local_config(d), local_config(reference.path()));
    assert_eq!(refs(d), refs(reference.path()));
    assert!(local_config(d)
        .contains(&"remote.origin.mirror.url=https://example.invalid/m.git".to_string()));
}

/// git removes the section, the remote's remote-tracking refs (never a local branch its push
/// refspec names, nor another remote's refs), and the settings that track or push to it;
/// remote `origin.mirror` and branch `gone`'s settings stay.
#[test]
fn remove_remote_removes_what_git_removes() {
    let (dir, repo) = remotes();
    let d = dir.path();
    let reference = twin(d);
    git_ok(reference.path(), &["remote", "rm", "origin"]);

    git_core::remote::remove_remote(&repo, "origin").unwrap();

    assert_eq!(local_config(d), local_config(reference.path()));
    assert_eq!(refs(d), refs(reference.path()));
    assert!(has_branch(&repo, "topic"));
    assert!(local_config(d).contains(&"branch.gone.remote=other".to_string()));
}

/// The upstreams of existing branches are cleared as `git remote rm` clears them; the rest as
/// Remove remote.
#[test]
fn remove_remote_and_clear_upstreams_removes_what_git_removes() {
    let (dir, repo) = remotes();
    let d = dir.path();
    git_ok(d, &["branch", "-u", "origin/main", "main"]);
    git_ok(d, &["config", "branch.main.description", "kept"]);
    let reference = twin(d);
    git_ok(reference.path(), &["remote", "rm", "origin"]);

    git_core::remote::remove_remote_and_clear_upstreams(&repo, "origin").unwrap();

    assert_eq!(local_config(d), local_config(reference.path()));
    assert_eq!(refs(d), refs(reference.path()));
}

/// Browsitory's own `browsitory.remote.<name>.*` moves to the new name, and nothing else is
/// written: libgit2 also took it for a setting of remote `origin` and wrote it under a remote
/// `upstream.ote.origin`.
#[test]
fn rename_remote_moves_its_sign_in_settings_and_writes_nothing_else() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    git_ok(
        d,
        &["remote", "add", "origin", "https://example.invalid/r.git"],
    );
    git_core::remote::set_remote_auth_profile(
        &repo,
        "origin",
        RemoteAuthMode::HttpsToken {
            username: "user".to_string(),
        },
    )
    .unwrap();

    git_core::remote::rename_remote(&repo, "origin", "upstream").unwrap();

    let remote_settings: Vec<String> = local_config(d)
        .into_iter()
        .filter(|line| line.starts_with("remote.") || line.starts_with("browsitory."))
        .collect();
    assert_eq!(
        remote_settings,
        [
            "browsitory.remote.upstream.auth-mode=https-token",
            "browsitory.remote.upstream.username=user",
            "remote.upstream.fetch=+refs/heads/*:refs/remotes/upstream/*",
            "remote.upstream.url=https://example.invalid/r.git",
        ]
    );
}

/// A remote with a setting in the user's own config too (`remote.origin.prune`).
fn remote_with_user_setting() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    git_ok(
        dir.path(),
        &["remote", "add", "origin", "https://example.invalid/r.git"],
    );
    let mut user = common::user_config(&repo);
    user.set_bool("remote.origin.prune", true).unwrap();
    (dir, repo)
}

#[test]
fn rename_remote_leaves_a_setting_of_the_users_own_config_alone() {
    let (dir, repo) = remote_with_user_setting();
    let user = user_config_text(&repo);

    git_core::remote::rename_remote(&repo, "origin", "upstream").unwrap();

    let remote_settings: Vec<String> = local_config(dir.path())
        .into_iter()
        .filter(|line| line.starts_with("remote."))
        .collect();
    assert_eq!(
        remote_settings,
        [
            "remote.upstream.fetch=+refs/heads/*:refs/remotes/upstream/*",
            "remote.upstream.url=https://example.invalid/r.git",
        ]
    );
    assert_eq!(user_config_text(&repo), user);
}

#[test]
fn remove_remote_leaves_a_setting_of_the_users_own_config_alone() {
    let (dir, repo) = remote_with_user_setting();
    let user = user_config_text(&repo);

    git_core::remote::remove_remote(&repo, "origin").unwrap();

    assert!(!local_config(dir.path())
        .iter()
        .any(|line| line.starts_with("remote.")));
    assert_eq!(user_config_text(&repo), user);
}
