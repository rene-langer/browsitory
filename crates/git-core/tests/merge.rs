mod common;

use common::{init_repo, write_file};
use git_core::{MergeOutcome, abort_merge, create_commit, merge_branch, stage_path, status};
use git2::BranchType;

fn checkout(repo: &git2::Repository, refname: &str) {
    repo.set_head(refname).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
}

#[test]
fn merge_already_up_to_date_when_their_branch_is_an_ancestor() {
    let (dir, mut repo) = init_repo();
    write_file(&dir, "a.txt", "one\n");
    stage_path(&repo, "a.txt").unwrap();
    create_commit(&mut repo, "base").unwrap();
    repo.branch(
        "old",
        &repo.head().unwrap().peel_to_commit().unwrap(),
        false,
    )
    .unwrap();

    let outcome = merge_branch(&repo, "old").unwrap();

    assert_eq!(outcome, MergeOutcome::UpToDate);
}

#[test]
fn merge_fast_forwards_when_head_has_not_diverged() {
    let (dir, mut repo) = init_repo();
    write_file(&dir, "a.txt", "one\n");
    stage_path(&repo, "a.txt").unwrap();
    create_commit(&mut repo, "base").unwrap();

    repo.branch(
        "feature",
        &repo.head().unwrap().peel_to_commit().unwrap(),
        false,
    )
    .unwrap();
    checkout(&repo, "refs/heads/feature");
    write_file(&dir, "a.txt", "one\ntwo\n");
    stage_path(&repo, "a.txt").unwrap();
    let feature_tip = create_commit(&mut repo, "add second line").unwrap();

    let default_branch = default_branch_name(&repo);
    checkout(&repo, &format!("refs/heads/{default_branch}"));

    let outcome = merge_branch(&repo, "feature").unwrap();

    assert_eq!(outcome, MergeOutcome::FastForward);
    assert_eq!(
        repo.head().unwrap().peel_to_commit().unwrap().id(),
        feature_tip
    );
    assert!(status(&repo).unwrap().is_empty());
}

#[test]
fn merge_produces_a_clean_three_way_merge_when_changes_do_not_overlap() {
    let (dir, mut repo) = init_repo();
    write_file(&dir, "a.txt", "one\n");
    write_file(&dir, "b.txt", "one\n");
    stage_path(&repo, "a.txt").unwrap();
    stage_path(&repo, "b.txt").unwrap();
    create_commit(&mut repo, "base").unwrap();

    repo.branch(
        "feature",
        &repo.head().unwrap().peel_to_commit().unwrap(),
        false,
    )
    .unwrap();
    checkout(&repo, "refs/heads/feature");
    write_file(&dir, "b.txt", "one\ntwo\n");
    stage_path(&repo, "b.txt").unwrap();
    create_commit(&mut repo, "edit b.txt on feature").unwrap();

    let default_branch = default_branch_name(&repo);
    checkout(&repo, &format!("refs/heads/{default_branch}"));
    write_file(&dir, "a.txt", "one\ntwo\n");
    stage_path(&repo, "a.txt").unwrap();
    create_commit(&mut repo, "edit a.txt on default").unwrap();

    let outcome = merge_branch(&repo, "feature").unwrap();

    assert_eq!(outcome, MergeOutcome::Merged);
    // The merge result is staged but not committed yet.
    let entries = status(&repo).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "b.txt");

    let oid = create_commit(&mut repo, "merge feature").unwrap();
    let commit = repo.find_commit(oid).unwrap();
    assert_eq!(commit.parent_ids().count(), 2);
    assert!(status(&repo).unwrap().is_empty());
}

#[test]
fn merge_reports_conflicts_and_leaves_conflict_markers_in_the_working_tree() {
    let (dir, mut repo) = init_repo();
    write_file(&dir, "a.txt", "one\n");
    stage_path(&repo, "a.txt").unwrap();
    create_commit(&mut repo, "base").unwrap();

    repo.branch(
        "feature",
        &repo.head().unwrap().peel_to_commit().unwrap(),
        false,
    )
    .unwrap();
    checkout(&repo, "refs/heads/feature");
    write_file(&dir, "a.txt", "one\nfrom feature\n");
    stage_path(&repo, "a.txt").unwrap();
    create_commit(&mut repo, "edit a.txt on feature").unwrap();

    let default_branch = default_branch_name(&repo);
    checkout(&repo, &format!("refs/heads/{default_branch}"));
    write_file(&dir, "a.txt", "one\nfrom default\n");
    stage_path(&repo, "a.txt").unwrap();
    create_commit(&mut repo, "edit a.txt on default").unwrap();

    let outcome = merge_branch(&repo, "feature").unwrap();

    match outcome {
        MergeOutcome::Conflict(paths) => assert_eq!(paths, vec!["a.txt".to_string()]),
        other => panic!("expected Conflict, got {other:?}"),
    }

    let on_disk = std::fs::read_to_string(dir.path().join("a.txt")).unwrap();
    assert!(on_disk.contains("<<<<<<<"));
    assert!(on_disk.contains("======="));
    assert!(on_disk.contains(">>>>>>>"));
}

#[test]
fn abort_merge_restores_the_exact_pre_merge_working_tree() {
    let (dir, mut repo) = init_repo();
    write_file(&dir, "a.txt", "one\n");
    stage_path(&repo, "a.txt").unwrap();
    create_commit(&mut repo, "base").unwrap();

    repo.branch(
        "feature",
        &repo.head().unwrap().peel_to_commit().unwrap(),
        false,
    )
    .unwrap();
    checkout(&repo, "refs/heads/feature");
    write_file(&dir, "a.txt", "one\nfrom feature\n");
    stage_path(&repo, "a.txt").unwrap();
    create_commit(&mut repo, "edit a.txt on feature").unwrap();

    let default_branch = default_branch_name(&repo);
    checkout(&repo, &format!("refs/heads/{default_branch}"));
    write_file(&dir, "a.txt", "one\nfrom default\n");
    stage_path(&repo, "a.txt").unwrap();
    create_commit(&mut repo, "edit a.txt on default").unwrap();
    let pre_merge_content = std::fs::read_to_string(dir.path().join("a.txt")).unwrap();

    let outcome = merge_branch(&repo, "feature").unwrap();
    assert!(matches!(outcome, MergeOutcome::Conflict(_)));

    abort_merge(&repo).unwrap();

    let restored = std::fs::read_to_string(dir.path().join("a.txt")).unwrap();
    assert_eq!(restored, pre_merge_content);
    assert!(status(&repo).unwrap().is_empty());
}

fn default_branch_name(repo: &git2::Repository) -> String {
    repo.branches(Some(BranchType::Local))
        .unwrap()
        .map(|b| b.unwrap().0)
        .find(|b| !matches!(b.name().unwrap(), Some("feature") | Some("old")))
        .unwrap()
        .name()
        .unwrap()
        .unwrap()
        .to_string()
}
