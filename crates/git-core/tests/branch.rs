mod common;

use common::{init_repo, write_file};
use git_core::{
    create_branch, create_commit, delete_branch, list_branches, rename_branch, stage_path,
    switch_branch,
};

/// Commits an initial file and returns (its oid, the initial branch's
/// shorthand name). The initial branch name depends on the machine's
/// `init.defaultBranch` config (`master` vs `main`), so tests read it back
/// rather than hardcoding either.
fn commit_initial(dir: &tempfile::TempDir, repo: &git2::Repository) -> (git2::Oid, String) {
    write_file(dir, "a.txt", "hello\n");
    stage_path(repo, "a.txt").unwrap();
    let oid = create_commit(repo, "initial commit").unwrap();
    let initial_branch = repo.head().unwrap().shorthand().unwrap().to_string();
    (oid, initial_branch)
}

#[test]
fn create_and_list_branches() {
    let (dir, repo) = init_repo();
    let (_oid, initial_branch) = commit_initial(&dir, &repo);

    create_branch(&repo, "feature", None).unwrap();

    let branches = list_branches(&repo).unwrap();
    let names: Vec<&str> = branches.iter().map(|b| b.name.as_str()).collect();
    assert!(names.contains(&"feature"));
    assert!(names.contains(&initial_branch.as_str()));

    let feature = branches.iter().find(|b| b.name == "feature").unwrap();
    assert!(!feature.is_head);
}

#[test]
fn create_branch_at_explicit_start_point() {
    let (dir, repo) = init_repo();
    let (first, _initial_branch) = commit_initial(&dir, &repo);

    write_file(&dir, "b.txt", "world\n");
    stage_path(&repo, "b.txt").unwrap();
    create_commit(&repo, "second commit").unwrap();

    create_branch(&repo, "from-first", Some(first)).unwrap();

    let branch = repo
        .find_branch("from-first", git2::BranchType::Local)
        .unwrap();
    assert_eq!(branch.get().target(), Some(first));
}

#[test]
fn switch_branch_moves_head_and_checks_out_tree() {
    let (dir, repo) = init_repo();
    let (_oid, initial_branch) = commit_initial(&dir, &repo);
    create_branch(&repo, "feature", None).unwrap();

    switch_branch(&repo, "feature").unwrap();

    let head = repo.head().unwrap();
    assert_eq!(head.shorthand(), Ok("feature"));
    let branches = list_branches(&repo).unwrap();
    let feature = branches.iter().find(|b| b.name == "feature").unwrap();
    assert!(feature.is_head);
    let original = branches.iter().find(|b| b.name == initial_branch).unwrap();
    assert!(!original.is_head);
}

#[test]
fn switch_branch_with_dirty_working_tree_is_refused() {
    let (dir, repo) = init_repo();
    let (first, initial_branch) = commit_initial(&dir, &repo);
    // Branch "feature" off the *first* commit, before a.txt's content
    // changes below — otherwise "feature" and the initial branch would
    // point at the same commit, switching would need to rewrite nothing,
    // and a dirty a.txt wouldn't actually be in the checkout's way.
    create_branch(&repo, "feature", Some(first)).unwrap();

    // Advance the initial branch with a second commit that changes a.txt,
    // so "feature"'s tree (still "hello\n") now genuinely differs from
    // HEAD's tree.
    write_file(&dir, "a.txt", "second commit content\n");
    stage_path(&repo, "a.txt").unwrap();
    create_commit(&repo, "second commit").unwrap();

    // Dirty the working tree with an uncommitted change that would be
    // clobbered by checking "feature"'s differing a.txt content out.
    write_file(&dir, "a.txt", "uncommitted change\n");

    let result = switch_branch(&repo, "feature");
    assert!(result.is_err());
    // HEAD must not have moved.
    assert_eq!(
        repo.head().unwrap().shorthand().ok(),
        Some(initial_branch.as_str())
    );
}

#[test]
fn delete_branch_removes_it_from_the_list() {
    let (dir, repo) = init_repo();
    commit_initial(&dir, &repo);
    create_branch(&repo, "feature", None).unwrap();

    delete_branch(&repo, "feature").unwrap();

    let branches = list_branches(&repo).unwrap();
    assert!(!branches.iter().any(|b| b.name == "feature"));
}

#[test]
fn delete_current_branch_is_refused() {
    let (dir, repo) = init_repo();
    commit_initial(&dir, &repo);
    create_branch(&repo, "feature", None).unwrap();
    switch_branch(&repo, "feature").unwrap();

    let result = delete_branch(&repo, "feature");
    assert!(result.is_err());

    let branches = list_branches(&repo).unwrap();
    assert!(branches.iter().any(|b| b.name == "feature"));
}

#[test]
fn rename_branch_keeps_the_same_commit() {
    let (dir, repo) = init_repo();
    let (oid, _initial_branch) = commit_initial(&dir, &repo);
    create_branch(&repo, "feature", None).unwrap();

    rename_branch(&repo, "feature", "feature-renamed").unwrap();

    let branches = list_branches(&repo).unwrap();
    assert!(!branches.iter().any(|b| b.name == "feature"));
    let renamed = branches
        .iter()
        .find(|b| b.name == "feature-renamed")
        .unwrap();
    assert!(!renamed.is_head);

    let branch = repo
        .find_branch("feature-renamed", git2::BranchType::Local)
        .unwrap();
    assert_eq!(branch.get().target(), Some(oid));
}
