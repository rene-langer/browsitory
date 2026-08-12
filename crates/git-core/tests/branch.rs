mod common;

use common::{commit_all, init_repo, write_file};

#[test]
fn list_branches_reports_the_current_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "content");
    commit_all(&repo, "initial commit");

    let branches = git_core::branch::list_branches(&repo).unwrap();

    assert_eq!(branches.len(), 1);
    assert!(branches[0].is_current);
}

#[test]
fn create_branch_from_head_adds_and_switches_to_it() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "content");
    commit_all(&repo, "initial commit");

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();

    let branches = git_core::branch::list_branches(&repo).unwrap();
    assert_eq!(branches.len(), 2);
    let feature = branches.iter().find(|b| b.name == "feature").unwrap();
    assert!(feature.is_current);
    assert_eq!(branches.iter().filter(|b| b.is_current).count(), 1);
}

#[test]
fn create_branch_from_a_specific_commit_uses_that_commit_as_start_point() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "first commit");
    let first_commit_id = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "second commit");

    git_core::branch::create_branch(&repo, "from-first", &first_commit_id).unwrap();

    let branch = repo
        .find_branch("from-first", git2::BranchType::Local)
        .unwrap();
    let branch_commit = branch.get().peel_to_commit().unwrap();
    assert_eq!(branch_commit.id().to_string(), first_commit_id);
}

#[test]
fn switch_branch_moves_head_and_updates_the_working_tree() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let initial_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "file.txt", "v2-on-feature");
    commit_all(&repo, "feature commit");

    git_core::branch::switch_branch(&repo, &initial_branch).unwrap();

    let contents = std::fs::read_to_string(dir.path().join("file.txt")).unwrap();
    assert_eq!(contents, "v1");
    let branches = git_core::branch::list_branches(&repo).unwrap();
    assert!(
        branches
            .iter()
            .find(|b| b.name == initial_branch)
            .unwrap()
            .is_current
    );
}

#[test]
fn switch_branch_is_blocked_by_a_conflicting_dirty_file() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let initial_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "file.txt", "v2-on-feature");
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &initial_branch).unwrap();
    // Dirty the file with content that differs from both branches' tips — the exact shape of
    // conflict libgit2's safe checkout refuses to silently overwrite.
    write_file(dir.path(), "file.txt", "uncommitted local edit");

    let result = git_core::branch::switch_branch(&repo, "feature");

    assert!(result.is_err());
    let contents = std::fs::read_to_string(dir.path().join("file.txt")).unwrap();
    assert_eq!(contents, "uncommitted local edit");
    assert!(
        git_core::branch::list_branches(&repo)
            .unwrap()
            .iter()
            .find(|b| b.name == initial_branch)
            .unwrap()
            .is_current
    );
}

#[test]
fn rename_branch_updates_head_when_renaming_the_current_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let initial_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    git_core::branch::rename_branch(&repo, &initial_branch, "renamed").unwrap();

    let branches = git_core::branch::list_branches(&repo).unwrap();
    assert_eq!(branches.len(), 1);
    assert_eq!(branches[0].name, "renamed");
    assert!(branches[0].is_current);
}

#[test]
fn delete_branch_without_force_fails_on_an_unmerged_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "feature commit");
    let initial_branch = git_core::branch::list_branches(&repo)
        .unwrap()
        .into_iter()
        .find(|b| b.name != "feature")
        .unwrap()
        .name;
    git_core::branch::switch_branch(&repo, &initial_branch).unwrap();

    let result = git_core::branch::delete_branch(&repo, "feature", false);

    assert!(matches!(
        result,
        Err(git_core::branch::BranchError::NotMerged(_))
    ));
    assert!(git_core::branch::list_branches(&repo)
        .unwrap()
        .iter()
        .any(|b| b.name == "feature"));
}

#[test]
fn delete_branch_with_force_deletes_an_unmerged_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "feature commit");
    let initial_branch = git_core::branch::list_branches(&repo)
        .unwrap()
        .into_iter()
        .find(|b| b.name != "feature")
        .unwrap()
        .name;
    git_core::branch::switch_branch(&repo, &initial_branch).unwrap();

    git_core::branch::delete_branch(&repo, "feature", true).unwrap();

    assert!(!git_core::branch::list_branches(&repo)
        .unwrap()
        .iter()
        .any(|b| b.name == "feature"));
}

#[test]
fn delete_branch_without_force_succeeds_when_fully_merged() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let initial_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    // "feature" has no commits of its own — its tip exactly matches the initial branch's tip,
    // the edge case libgit2's graph_descendant_of doesn't treat as "descendant" on its own.
    git_core::branch::switch_branch(&repo, &initial_branch).unwrap();

    git_core::branch::delete_branch(&repo, "feature", false).unwrap();

    assert!(!git_core::branch::list_branches(&repo)
        .unwrap()
        .iter()
        .any(|b| b.name == "feature"));
}

#[test]
fn delete_branch_fails_when_deleting_the_current_branch_even_with_force() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let initial_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    let result = git_core::branch::delete_branch(&repo, &initial_branch, true);

    assert!(result.is_err());
}
