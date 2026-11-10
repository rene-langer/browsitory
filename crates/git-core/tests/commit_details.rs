mod common;

use std::time::Duration;

use common::{commit_tree, init_repo};
use git_core::commit_details::{commit_details, CommitDetailsError};

const FILE: u32 = 0o100644;
const BUDGET: Duration = Duration::from_secs(10);

/// `base` → `middle` → `tip` on `main`; `side` from `base` on `side`. Returns
/// `[base, middle, tip, side]`.
fn history(repo: &git2::Repository) -> Vec<git2::Oid> {
    let base = commit_tree(repo, &[("a.txt", "a", FILE)], &[], "base");
    let middle = commit_tree(repo, &[("a.txt", "b", FILE)], &[base], "middle");
    let tip = commit_tree(repo, &[("a.txt", "c", FILE)], &[middle], "tip");
    let side = commit_tree(repo, &[("a.txt", "s", FILE)], &[base], "side");
    repo.reference("refs/heads/main", tip, true, "test")
        .unwrap();
    repo.reference("refs/heads/side", side, true, "test")
        .unwrap();
    vec![base, middle, tip, side]
}

#[test]
fn lists_the_local_branches_whose_tip_is_or_contains_the_commit() {
    let (_dir, repo) = init_repo();
    let ids = history(&repo);

    let base = commit_details(&repo, &ids[0].to_string(), BUDGET).unwrap();
    assert_eq!(base.branches, vec!["main", "side"]);
    let middle = commit_details(&repo, &ids[1].to_string(), BUDGET).unwrap();
    assert_eq!(middle.branches, vec!["main"]);
    let tip = commit_details(&repo, &ids[2].to_string(), BUDGET).unwrap();
    assert_eq!(tip.branches, vec!["main"], "a branch's own tip counts");
    assert!(tip.complete);
}

#[test]
fn lists_remote_branches_and_skips_symbolic_remote_heads() {
    let (_dir, repo) = init_repo();
    let ids = history(&repo);
    repo.reference("refs/remotes/origin/main", ids[1], true, "test")
        .unwrap();
    repo.reference("refs/remotes/origin/side", ids[3], true, "test")
        .unwrap();
    repo.reference_symbolic(
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
        true,
        "test",
    )
    .unwrap();

    let middle = commit_details(&repo, &ids[1].to_string(), BUDGET).unwrap();
    assert_eq!(middle.remote_branches, vec!["origin/main"]);
    let base = commit_details(&repo, &ids[0].to_string(), BUDGET).unwrap();
    assert_eq!(base.remote_branches, vec!["origin/main", "origin/side"]);
    let tip = commit_details(&repo, &ids[2].to_string(), BUDGET).unwrap();
    assert!(tip.remote_branches.is_empty());
}

#[test]
fn lists_lightweight_and_annotated_tags_that_contain_the_commit() {
    let (_dir, repo) = init_repo();
    let ids = history(&repo);
    let middle = repo.find_commit(ids[1]).unwrap();
    repo.tag_lightweight("v1.0", middle.as_object(), false)
        .unwrap();
    let tip = repo.find_commit(ids[2]).unwrap();
    let sig = repo.signature().unwrap();
    repo.tag("v2.0", tip.as_object(), &sig, "release", false)
        .unwrap();
    // A tag on a tree names no commit: skipped, not an error.
    let tree = tip.tree().unwrap();
    repo.tag_lightweight("a-tree", tree.as_object(), false)
        .unwrap();
    let side = repo.find_commit(ids[3]).unwrap();
    repo.tag_lightweight("side-tag", side.as_object(), false)
        .unwrap();

    let base = commit_details(&repo, &ids[0].to_string(), BUDGET).unwrap();
    assert_eq!(base.tags, vec!["side-tag", "v1.0", "v2.0"]);
    let at_middle = commit_details(&repo, &ids[1].to_string(), BUDGET).unwrap();
    assert_eq!(at_middle.tags, vec!["v1.0", "v2.0"]);
    let at_tip = commit_details(&repo, &ids[2].to_string(), BUDGET).unwrap();
    assert_eq!(at_tip.tags, vec!["v2.0"]);
}

#[test]
fn reports_the_author_and_the_committer_with_their_own_times() {
    let (_dir, repo) = init_repo();
    let tree = {
        let mut index = repo.index().unwrap();
        repo.find_tree(index.write_tree().unwrap()).unwrap()
    };
    let author =
        git2::Signature::new("Ada", "ada@example.com", &git2::Time::new(1_000, 60)).unwrap();
    let committer =
        git2::Signature::new("Bob", "bob@example.com", &git2::Time::new(2_000, -120)).unwrap();
    let id = repo
        .commit(Some("HEAD"), &author, &committer, "applied", &tree, &[])
        .unwrap();

    let details = commit_details(&repo, &id.to_string(), BUDGET).unwrap();
    assert_eq!(details.author.name, "Ada");
    assert_eq!(details.author.email, "ada@example.com");
    assert_eq!(details.author.timestamp, 1_000);
    assert_eq!(details.committer.name, "Bob");
    assert_eq!(details.committer.email, "bob@example.com");
    assert_eq!(details.committer.timestamp, 2_000);
}

#[test]
fn a_spent_budget_stops_the_ref_scan_and_says_the_lists_are_incomplete() {
    let (_dir, repo) = init_repo();
    let ids = history(&repo);
    let middle = repo.find_commit(ids[1]).unwrap();
    repo.tag_lightweight("v1.0", middle.as_object(), false)
        .unwrap();

    let details = commit_details(&repo, &ids[0].to_string(), Duration::ZERO).unwrap();
    assert!(!details.complete);
    assert!(details.branches.is_empty());
    assert!(details.remote_branches.is_empty());
    assert!(details.tags.is_empty());
    // The signatures don't depend on the scan.
    assert_eq!(details.author.name, "Test User");
}

#[test]
fn a_repository_without_refs_is_complete_with_a_zero_budget() {
    let (_dir, repo) = init_repo();
    let id = commit_tree(&repo, &[("a.txt", "a", FILE)], &[], "loose");

    let details = commit_details(&repo, &id.to_string(), Duration::ZERO).unwrap();
    assert!(details.complete, "nothing was left unchecked");
}

#[test]
fn an_unknown_or_malformed_commit_id_is_an_error() {
    let (_dir, repo) = init_repo();
    history(&repo);

    let missing = commit_details(&repo, &"1".repeat(40), BUDGET);
    assert!(matches!(missing, Err(CommitDetailsError::Git(_))));
    let malformed = commit_details(&repo, "not-a-sha", BUDGET);
    assert!(matches!(malformed, Err(CommitDetailsError::Git(_))));
}
