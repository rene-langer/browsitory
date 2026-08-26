mod common;

use common::{commit_all, init_repo, write_file};

#[test]
fn graph_log_returns_an_empty_vec_for_a_repository_with_no_commits() {
    let (_dir, repo) = init_repo();

    let result = git_core::graph::graph_log(&repo, 10, None).unwrap();

    assert!(result.is_empty());
}

#[test]
fn graph_log_shows_commits_from_every_local_branch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "feature commit");

    let commits = git_core::graph::graph_log(&repo, 10, None).unwrap();

    assert_eq!(commits.len(), 2);
    assert!(commits.iter().any(|c| c.summary == "feature commit"));
    assert!(commits.iter().any(|c| c.summary == "initial commit"));
}

#[test]
fn graph_log_reports_branch_refs_only_for_tip_commits() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let initial_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &initial_branch).unwrap();

    let commits = git_core::graph::graph_log(&repo, 10, None).unwrap();

    let feature_commit = commits
        .iter()
        .find(|c| c.summary == "feature commit")
        .unwrap();
    assert_eq!(feature_commit.branch_refs, vec!["feature".to_string()]);
    // "initial commit" is the initial branch's tip (feature has moved past it) — it should
    // carry the initial branch's name, not be empty.
    let initial_commit = commits
        .iter()
        .find(|c| c.summary == "initial commit")
        .unwrap();
    assert_eq!(initial_commit.branch_refs, vec![initial_branch]);
}

#[test]
fn graph_log_reports_empty_branch_refs_for_a_non_tip_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "middle commit");
    write_file(dir.path(), "file.txt", "v3");
    commit_all(&repo, "tip commit");
    let branch_name = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    let commits = git_core::graph::graph_log(&repo, 10, None).unwrap();

    let middle_commit = commits
        .iter()
        .find(|c| c.summary == "middle commit")
        .unwrap();
    assert!(middle_commit.branch_refs.is_empty());
    let tip_commit = commits.iter().find(|c| c.summary == "tip commit").unwrap();
    assert_eq!(tip_commit.branch_refs, vec![branch_name]);
}

#[test]
fn graph_log_with_selected_branches_only_walks_those_branches() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();

    let commits =
        git_core::graph::graph_log(&repo, 10, Some(std::slice::from_ref(&main_branch))).unwrap();

    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].summary, "initial commit");
}

#[test]
fn graph_log_respects_the_limit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "first commit");
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "second commit");
    write_file(dir.path(), "file.txt", "v3");
    commit_all(&repo, "third commit");

    let commits = git_core::graph::graph_log(&repo, 2, None).unwrap();

    assert_eq!(commits.len(), 2);
}

#[test]
fn graph_log_reports_multiple_parent_ids_for_a_merge_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "feature.txt", "v1");
    commit_all(&repo, "feature commit");
    let feature_commit_id = repo.head().unwrap().peel_to_commit().unwrap().id();

    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "main.txt", "v1");
    commit_all(&repo, "main commit");
    let main_commit = repo.head().unwrap().peel_to_commit().unwrap();
    let feature_commit = repo.find_commit(feature_commit_id).unwrap();

    // No merge.rs exists yet (a future Phase 2 subsystem) — construct a two-parent commit
    // directly via the same low-level `repo.commit()` primitive `commit_all` already uses
    // elsewhere in this test suite, rather than needing real merge machinery just to test that
    // `graph_log` correctly reports every parent.
    let tree_id = repo.index().unwrap().write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = repo.signature().unwrap();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "merge feature into main",
        &tree,
        &[&main_commit, &feature_commit],
    )
    .unwrap();

    let commits = git_core::graph::graph_log(&repo, 10, None).unwrap();

    let merge_commit = commits
        .iter()
        .find(|c| c.summary == "merge feature into main")
        .unwrap();
    assert_eq!(merge_commit.parent_ids.len(), 2);
}
