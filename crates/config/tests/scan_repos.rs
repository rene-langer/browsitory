use std::fs;
use std::path::PathBuf;

use config::scan_repos_in_root;

#[test]
fn scan_repos_in_root_finds_only_immediate_children_with_a_dot_git_entry() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("repo-a/.git")).unwrap();
    fs::create_dir_all(root.join("repo-b/.git")).unwrap();
    fs::create_dir_all(root.join("not-a-repo")).unwrap();
    fs::write(root.join("a-file.txt"), "not a directory").unwrap();
    // Nested repo two levels down must NOT be found — scan is one level only.
    fs::create_dir_all(root.join("not-a-repo/nested-repo/.git")).unwrap();

    let found = scan_repos_in_root(root).unwrap();

    assert_eq!(found, vec![root.join("repo-a"), root.join("repo-b")]);
}

#[test]
fn scan_repos_in_root_counts_a_dot_git_file_as_well_as_a_dot_git_directory() {
    // Worktrees and submodules have a `.git` *file* (containing a `gitdir:` pointer), not a
    // directory — a root scan over a folder of worktree checkouts must still find them.
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    fs::create_dir_all(root.join("worktree-style")).unwrap();
    fs::write(
        root.join("worktree-style/.git"),
        "gitdir: /elsewhere/.git/worktrees/x\n",
    )
    .unwrap();

    let found = scan_repos_in_root(root).unwrap();

    assert_eq!(found, vec![root.join("worktree-style")]);
}

#[test]
fn scan_repos_in_root_returns_empty_for_a_root_with_no_repos() {
    let dir = tempfile::TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("plain-folder")).unwrap();

    assert_eq!(
        scan_repos_in_root(dir.path()).unwrap(),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn scan_repos_in_root_errors_on_a_nonexistent_root() {
    let dir = tempfile::TempDir::new().unwrap();
    let missing = dir.path().join("does-not-exist");

    assert!(scan_repos_in_root(&missing).is_err());
}
