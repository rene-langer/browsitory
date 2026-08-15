mod common;

use common::{commit_all, init_repo, write_file};
use git_core::worktree::{
    create_worktree, list_worktrees, prune_worktrees, remove_worktree, WorktreeError,
};

#[test]
fn creates_a_linked_worktree_for_an_existing_local_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "initial");
    commit_all(&repo, "initial commit");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    let linked = dir.path().join("feature-tree");

    let worktrees = list_worktrees(&repo).unwrap();

    assert_eq!(worktrees.len(), 1);
    let main = worktrees.iter().find(|worktree| worktree.is_main).unwrap();
    create_worktree(&repo, "feature-tree", &linked, "feature", None).unwrap();
    assert!(list_worktrees(&repo)
        .unwrap()
        .iter()
        .any(|worktree| worktree.path == linked));
    assert!(matches!(
        remove_worktree(&repo, &main.path),
        Err(WorktreeError::MainWorktree)
    ));
}

#[test]
fn lists_the_main_worktree_when_the_git_directory_is_separate() {
    let dir = tempfile::TempDir::new().unwrap();
    let workdir = dir.path().join("working-copy");
    let git_dir = dir.path().join("repository-metadata");
    let mut options = git2::RepositoryInitOptions::new();
    options.no_dotgit_dir(true).workdir_path(&workdir);
    let repo = git2::Repository::init_opts(&git_dir, &options).unwrap();

    let worktrees = list_worktrees(&repo).unwrap();

    assert_eq!(worktrees.len(), 1);
    let main = &worktrees[0];
    assert!(main.is_main);
    assert_eq!(main.path, workdir.canonicalize().unwrap());
}

#[test]
fn creates_a_missing_branch_at_the_requested_start_point() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "initial");
    commit_all(&repo, "initial commit");
    let start_point = repo.head().unwrap().peel_to_commit().unwrap().id();
    write_file(dir.path(), "file.txt", "later");
    commit_all(&repo, "later commit");
    let linked = dir.path().join("new-feature-tree");

    create_worktree(
        &repo,
        "new-feature-tree",
        &linked,
        "new-feature",
        Some(&start_point.to_string()),
    )
    .unwrap();

    let linked_repo = git2::Repository::open(&linked).unwrap();
    assert_eq!(linked_repo.head().unwrap().target(), Some(start_point));
}

#[test]
fn rejects_an_existing_worktree_destination_without_removing_it() {
    let (dir, repo) = init_repo();
    let destination = dir.path().join("existing-destination");
    std::fs::create_dir(&destination).unwrap();

    let result = create_worktree(
        &repo,
        "existing-tree",
        &destination,
        "new-feature",
        Some("HEAD"),
    );

    assert!(matches!(result, Err(WorktreeError::PathExists)));
    assert!(destination.exists());
}

#[test]
fn refuses_to_remove_a_dirty_linked_worktree_without_deleting_it() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "initial");
    commit_all(&repo, "initial commit");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    let linked = dir.path().join("dirty-feature-tree");
    create_worktree(&repo, "dirty-feature-tree", &linked, "feature", None).unwrap();
    write_file(&linked, "untracked.txt", "dirty");

    let result = remove_worktree(&repo, &linked);

    assert!(matches!(result, Err(WorktreeError::Dirty)));
    assert!(linked.exists());
}

#[test]
fn prunes_stale_worktree_metadata_idempotently() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "initial");
    commit_all(&repo, "initial commit");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    let linked = dir.path().join("stale-feature-tree");
    create_worktree(&repo, "stale-feature-tree", &linked, "feature", None).unwrap();
    std::fs::remove_dir_all(&linked).unwrap();
    let stale_worktree = list_worktrees(&repo)
        .unwrap()
        .into_iter()
        .find(|worktree| worktree.path == linked)
        .unwrap();
    assert!(stale_worktree.is_prunable);

    prune_worktrees(&repo).unwrap();
    assert_eq!(list_worktrees(&repo).unwrap().len(), 1);
    prune_worktrees(&repo).unwrap();
    assert_eq!(list_worktrees(&repo).unwrap().len(), 1);
}
