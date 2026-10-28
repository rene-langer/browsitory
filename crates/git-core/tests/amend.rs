mod common;

use git_core::commit::{amend_head, commit, CommitError};

fn repo_with_two_commits() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "first");
    common::write_file(dir.path(), "b.txt", "b\n");
    common::commit_all(&repo, "second");
    (dir, repo)
}

#[test]
fn amend_folds_staged_changes_and_new_message_into_head() {
    let (dir, repo) = repo_with_two_commits();
    let old_head = repo.head().unwrap().peel_to_commit().unwrap();
    let parent_id = old_head.parent_id(0).unwrap();
    common::write_file(dir.path(), "c.txt", "c\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("c.txt")).unwrap();
    index.write().unwrap();

    let new_id = amend_head(
        &repo,
        "second, amended",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.id().to_string(), new_id);
    assert_ne!(head.id(), old_head.id());
    assert_eq!(head.message().unwrap(), "second, amended");
    assert_eq!(head.parent_ids().collect::<Vec<_>>(), vec![parent_id]);
    assert_eq!(head.author().name(), old_head.author().name());
    assert!(head.tree().unwrap().get_name("c.txt").is_some());
    assert!(
        repo.head().unwrap().is_branch(),
        "branch ref moved, HEAD still attached"
    );
}

#[test]
fn message_only_amend_keeps_tree() {
    let (_dir, repo) = repo_with_two_commits();
    let old_tree = repo.head().unwrap().peel_to_commit().unwrap().tree_id();
    amend_head(&repo, "reworded", git_core::hooks::HookOptions::default()).unwrap();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.tree_id(), old_tree);
    assert_eq!(head.message().unwrap(), "reworded");
}

#[test]
fn amend_works_on_detached_head() {
    let (_dir, repo) = repo_with_two_commits();
    let oid = repo.head().unwrap().target().unwrap();
    repo.set_head_detached(oid).unwrap();
    let new_id = amend_head(
        &repo,
        "detached amend",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    assert_eq!(repo.head().unwrap().target().unwrap().to_string(), new_id);
    assert!(repo.head_detached().unwrap());
}

#[test]
fn amend_refuses_unborn_head() {
    let (_dir, repo) = common::init_repo();
    assert!(matches!(
        amend_head(&repo, "x", git_core::hooks::HookOptions::default()),
        Err(CommitError::NothingToAmend)
    ));
}

#[test]
fn amend_refuses_during_a_merge() {
    let (_dir, repo) = repo_with_two_commits();
    let oid = repo.head().unwrap().target().unwrap();
    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{oid}\n")).unwrap();
    assert!(matches!(
        amend_head(&repo, "x", git_core::hooks::HookOptions::default()),
        Err(CommitError::OperationInProgress)
    ));
}

#[test]
fn commit_finishes_a_paused_cherry_pick_with_a_single_parent_and_clean_state() {
    let (dir, mut repo) = repo_with_two_commits();
    let head = repo.head().unwrap().target().unwrap();
    std::fs::write(repo.path().join("CHERRY_PICK_HEAD"), format!("{head}\n")).unwrap();
    common::write_file(dir.path(), "d.txt", "d\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("d.txt")).unwrap();
    index.write().unwrap();

    commit(&mut repo, "picked", git_core::hooks::HookOptions::default()).unwrap();

    let new_head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(new_head.parent_count(), 1);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}
