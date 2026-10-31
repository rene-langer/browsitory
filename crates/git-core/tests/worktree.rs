mod common;

use common::{commit_all, init_repo, write_file};
use git_core::worktree::{
    create_worktree, create_worktree_checking, list_worktrees, prune_worktrees, remove_worktree,
    WorktreeError,
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
    let linked = linked.canonicalize().unwrap();
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
    let linked = linked.canonicalize().unwrap();
    std::fs::remove_dir_all(&linked).unwrap();
    let stale_worktree = list_worktrees(&repo)
        .unwrap()
        .into_iter()
        .find(|worktree| worktree.path.file_name() == linked.file_name())
        .unwrap();
    assert!(stale_worktree.is_prunable);

    prune_worktrees(&repo).unwrap();
    assert_eq!(list_worktrees(&repo).unwrap().len(), 1);
    prune_worktrees(&repo).unwrap();
    assert_eq!(list_worktrees(&repo).unwrap().len(), 1);
}

/// Adds the branch `evil`: HEAD's tree plus the link `D` and the file `d/x`, built without a
/// file-system link.
fn add_evil_branch(repo: &git2::Repository) {
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let mut inner = repo.treebuilder(None).unwrap();
    inner
        .insert("x", repo.blob(b"x").unwrap(), 0o100644)
        .unwrap();
    let mut outer = repo.treebuilder(Some(&head.tree().unwrap())).unwrap();
    outer.insert("d", inner.write().unwrap(), 0o040000).unwrap();
    outer
        .insert("D", repo.blob(b"/x").unwrap(), 0o120000)
        .unwrap();
    let tree = repo.find_tree(outer.write().unwrap()).unwrap();
    let sig = repo.signature().unwrap();
    let evil = repo
        .commit(None, &sig, &sig, "evil", &tree, &[&head])
        .unwrap();
    repo.branch("evil", &repo.find_commit(evil).unwrap(), false)
        .unwrap();
}

#[test]
fn creating_a_worktree_on_a_branch_with_a_collision_is_refused() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "initial");
    commit_all(&repo, "initial commit");
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    add_evil_branch(&repo);
    let linked = dir.path().join("evil-tree");

    let error = create_worktree(&repo, "evil-tree", &linked, "evil", None).unwrap_err();
    assert!(matches!(error, WorktreeError::CaseCollision(_)), "{error}");
    assert!(!linked.exists());
    assert!(!repo.path().join("worktrees").join("evil-tree").exists());

    // A new branch at such a start point isn't created either.
    let error = create_worktree(
        &repo,
        "new-tree",
        &dir.path().join("new-tree"),
        "new",
        Some("evil"),
    )
    .unwrap_err();
    assert!(matches!(error, WorktreeError::CaseCollision(_)));
    assert!(repo.find_branch("new", git2::BranchType::Local).is_err());
}

/// #162: the repository's config says case matters (it was probed where the repository lives),
/// but the worktree's destination folds case (a USB stick, a casefold folder).
#[cfg(target_os = "linux")]
#[test]
fn a_collision_is_refused_when_the_destination_folds_case() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "initial");
    commit_all(&repo, "initial commit");
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", false)
        .unwrap();
    add_evil_branch(&repo);
    let linked = dir.path().join("evil-tree");

    let error =
        create_worktree_checking(&repo, "evil-tree", &linked, "evil", None, true).unwrap_err();
    assert!(matches!(error, WorktreeError::CaseCollision(_)), "{error}");
    assert!(!linked.exists());
    assert!(!repo.path().join("worktrees").join("evil-tree").exists());

    // A destination that keeps case apart takes the tree as before.
    create_worktree_checking(&repo, "evil-tree", &linked, "evil", None, false).unwrap();
    assert!(linked.join("d").join("x").is_file());
}

/// Through the real probe: a Linux temp dir keeps case apart, so the repository's setting
/// decides, as before #162.
#[cfg(target_os = "linux")]
#[test]
fn a_case_sensitive_destination_keeps_the_repository_setting() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "initial");
    commit_all(&repo, "initial commit");
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", false)
        .unwrap();
    add_evil_branch(&repo);
    let linked = dir.path().join("evil-tree");
    create_worktree(&repo, "evil-tree", &linked, "evil", None).unwrap();
    assert!(linked.join("d").join("x").is_file());
}
