mod common;

use common::{init_repo, write_file};
use git_core::{create_commit, graph_log, stage_path};

fn commit_file(repo: &git2::Repository, dir: &tempfile::TempDir, name: &str, message: &str) {
    write_file(dir, name, "content\n");
    stage_path(repo, name).unwrap();
    create_commit(repo, message).unwrap();
}

#[test]
fn empty_repo_returns_no_commits_and_does_not_panic() {
    let (_dir, repo) = init_repo();

    let commits = graph_log(&repo, 100).unwrap();

    assert!(commits.is_empty());
}

#[test]
fn linear_history_is_a_single_lane_newest_first() {
    let (dir, repo) = init_repo();
    commit_file(&repo, &dir, "a.txt", "commit 0");
    commit_file(&repo, &dir, "b.txt", "commit 1");
    commit_file(&repo, &dir, "c.txt", "commit 2");

    let commits = graph_log(&repo, 100).unwrap();

    assert_eq!(commits.len(), 3);
    assert_eq!(commits[0].summary, "commit 2");
    assert_eq!(commits[1].summary, "commit 1");
    assert_eq!(commits[2].summary, "commit 0");
    // Each non-root commit has exactly one parent -> one lane.
    assert_eq!(commits[0].parent_ids.len(), 1);
    assert_eq!(commits[1].parent_ids.len(), 1);
    assert!(commits[2].parent_ids.is_empty());
    // HEAD (default branch) points at the newest commit.
    assert!(commits[0].refs.iter().any(|r| r == "HEAD"));
}

#[test]
fn diverging_branches_are_both_reachable_with_correct_refs_per_tip() {
    let (dir, repo) = init_repo();
    commit_file(&repo, &dir, "a.txt", "base");
    let base_commit = repo.head().unwrap().peel_to_commit().unwrap();
    // Captured while HEAD is still on the original default branch — once we
    // switch to and commit on "feature" below, HEAD's shorthand would be
    // "feature" instead.
    let default_branch_name = default_branch_name(&repo);

    // Branch "feature" off of base, one commit ahead.
    repo.branch("feature", &base_commit, false).unwrap();
    repo.set_head("refs/heads/feature").unwrap();
    repo.checkout_head(None).unwrap();
    commit_file(&repo, &dir, "feature.txt", "feature work");

    // Back on the default branch, one different commit ahead.
    repo.set_head(&format!("refs/heads/{default_branch_name}"))
        .unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    commit_file(&repo, &dir, "main.txt", "main work");

    let commits = graph_log(&repo, 100).unwrap();

    // base + 2 divergent commits = 3 total, each branch tip reachable.
    assert_eq!(commits.len(), 3);
    let feature_tip = commits
        .iter()
        .find(|c| c.summary == "feature work")
        .expect("feature tip present");
    assert!(feature_tip.refs.iter().any(|r| r == "feature"));

    let main_tip = commits
        .iter()
        .find(|c| c.summary == "main work")
        .expect("main tip present");
    assert!(main_tip.refs.iter().any(|r| r == &default_branch_name));
    assert!(main_tip.refs.iter().any(|r| r == "HEAD"));
}

#[test]
fn merge_commit_has_two_parents_and_lanes_converge() {
    let (dir, repo) = init_repo();
    commit_file(&repo, &dir, "a.txt", "base");
    let base_commit = repo.head().unwrap().peel_to_commit().unwrap();
    let default_branch_name = default_branch_name(&repo);

    repo.branch("feature", &base_commit, false).unwrap();
    repo.set_head("refs/heads/feature").unwrap();
    repo.checkout_head(None).unwrap();
    commit_file(&repo, &dir, "feature.txt", "feature work");
    let feature_commit = repo.head().unwrap().peel_to_commit().unwrap();

    repo.set_head(&format!("refs/heads/{default_branch_name}"))
        .unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();

    // Manually construct a merge commit (base + feature -> two parents),
    // without depending on any other workstream's merge module. The tree
    // content doesn't matter for this test (only the parent graph does), so
    // this just reuses the current (default-branch) tree rather than
    // reconstructing feature.txt, which the force-checkout above removed
    // from the working directory.
    let mut index = repo.index().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let signature = repo.signature().unwrap();
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    let merge_oid = repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            "merge feature",
            &tree,
            &[&head_commit, &feature_commit],
        )
        .unwrap();

    let commits = graph_log(&repo, 100).unwrap();

    let merge = commits
        .iter()
        .find(|c| c.id == merge_oid)
        .expect("merge commit present");
    assert_eq!(merge.parent_ids.len(), 2);
    assert!(merge.parent_ids.contains(&head_commit.id()));
    assert!(merge.parent_ids.contains(&feature_commit.id()));
}

fn default_branch_name(repo: &git2::Repository) -> String {
    repo.head()
        .unwrap()
        .shorthand()
        .expect("HEAD has a shorthand name")
        .to_string()
}
