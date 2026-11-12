//! The ahead/behind counts the worker keeps between reads (#112).
mod common;

use common::{commit_all, commit_at, init_repo, write_file};
use git2::{Oid, Repository};
use git_core::ahead_behind::AheadBehindCache;
use git_core::branch::{list_branches, list_branches_cached, BranchInfo};
use git_core::remote::{current_upstream_cached, remote_upstreams_cached};
use tempfile::TempDir;

/// A repository with `count` branches `topic<i>`, each tracking `origin/topic<i>`: two commits
/// ahead and one behind, every pair of tips different.
fn tracked_repo(count: usize) -> (TempDir, Repository, Oid) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "base");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().target().unwrap();
    git_core::remote::add_remote(&repo, "origin", "file:///tmp/never-fetched.git", None).unwrap();
    let mut config = repo.config().unwrap();
    for i in 0..count {
        let seconds = 1_000_000 + 10 * i as i64;
        let first = commit_at(&repo, &[base], &format!("local {i}"), seconds);
        let local = commit_at(&repo, &[first], &format!("local {i} again"), seconds + 1);
        let remote = commit_at(&repo, &[base], &format!("remote {i}"), seconds + 2);
        repo.reference(&format!("refs/heads/topic{i}"), local, true, "test")
            .unwrap();
        repo.reference(
            &format!("refs/remotes/origin/topic{i}"),
            remote,
            true,
            "test",
        )
        .unwrap();
        config
            .set_str(&format!("branch.topic{i}.remote"), "origin")
            .unwrap();
        config
            .set_str(
                &format!("branch.topic{i}.merge"),
                &format!("refs/heads/topic{i}"),
            )
            .unwrap();
    }
    (dir, repo, base)
}

fn counts(branches: &[BranchInfo], name: &str) -> (Option<usize>, Option<usize>) {
    let branch = branches.iter().find(|branch| branch.name == name).unwrap();
    (branch.ahead, branch.behind)
}

/// Moves `reference` to a new commit on top of where it points.
fn advance(repo: &Repository, reference: &str, seconds: i64) {
    let tip = repo.find_reference(reference).unwrap().target().unwrap();
    let next = commit_at(repo, &[tip], "advance", seconds);
    repo.reference(reference, next, true, "test").unwrap();
}

#[test]
fn a_second_branch_list_counts_nothing_again_and_reports_the_same() {
    let (_dir, repo, _) = tracked_repo(3);
    let mut cache = AheadBehindCache::default();

    let first = list_branches_cached(&repo, &mut cache).unwrap();
    assert_eq!(cache.stats().walks, 3);
    let second = list_branches_cached(&repo, &mut cache).unwrap();

    assert_eq!(cache.stats().walks, 3);
    assert_eq!(second, first);
    assert_eq!(first, list_branches(&repo).unwrap());
    assert_eq!(counts(&first, "topic1"), (Some(2), Some(1)));
}

#[test]
fn remote_upstreams_after_the_branch_list_count_nothing_again() {
    let (_dir, repo, _) = tracked_repo(3);
    let mut cache = AheadBehindCache::default();
    list_branches_cached(&repo, &mut cache).unwrap();

    let upstreams = remote_upstreams_cached(&repo, "origin", &mut cache).unwrap();

    assert_eq!(cache.stats().walks, 3);
    assert_eq!(upstreams.len(), 3);
    for upstream in &upstreams {
        assert_eq!((upstream.ahead, upstream.behind), (Some(2), Some(1)));
    }
}

#[test]
fn the_current_upstream_after_the_branch_list_counts_nothing_again() {
    let (_dir, repo, _) = tracked_repo(2);
    repo.set_head("refs/heads/topic1").unwrap();
    let mut cache = AheadBehindCache::default();
    list_branches_cached(&repo, &mut cache).unwrap();

    let upstream = current_upstream_cached(&repo, &mut cache).unwrap().unwrap();

    assert_eq!(cache.stats().walks, 2);
    assert_eq!(upstream.local_branch, "topic1");
    assert_eq!((upstream.ahead, upstream.behind), (Some(2), Some(1)));
}

#[test]
fn a_moved_upstream_gets_new_counts_and_only_its_pair_is_counted_again() {
    let (_dir, repo, _) = tracked_repo(3);
    let mut cache = AheadBehindCache::default();
    list_branches_cached(&repo, &mut cache).unwrap();

    advance(&repo, "refs/remotes/origin/topic1", 2_000_000);
    let branches = list_branches_cached(&repo, &mut cache).unwrap();

    assert_eq!(counts(&branches, "topic1"), (Some(2), Some(2)));
    assert_eq!(counts(&branches, "topic0"), (Some(2), Some(1)));
    assert_eq!(cache.stats().walks, 4);
}

#[test]
fn a_moved_local_branch_gets_new_counts() {
    let (_dir, repo, _) = tracked_repo(3);
    let mut cache = AheadBehindCache::default();
    list_branches_cached(&repo, &mut cache).unwrap();

    advance(&repo, "refs/heads/topic2", 2_000_000);
    let branches = list_branches_cached(&repo, &mut cache).unwrap();

    assert_eq!(counts(&branches, "topic2"), (Some(3), Some(1)));
    assert_eq!(cache.stats().walks, 4);
}

#[test]
fn the_branch_list_keeps_only_the_counts_of_the_branches_it_lists() {
    let (_dir, repo, _) = tracked_repo(3);
    let mut cache = AheadBehindCache::default();
    list_branches_cached(&repo, &mut cache).unwrap();
    assert_eq!(cache.stats().kept, 3);

    advance(&repo, "refs/remotes/origin/topic1", 2_000_000);
    list_branches_cached(&repo, &mut cache).unwrap();
    assert_eq!(cache.stats().kept, 3);

    repo.find_reference("refs/heads/topic0")
        .unwrap()
        .delete()
        .unwrap();
    list_branches_cached(&repo, &mut cache).unwrap();
    assert_eq!(cache.stats().kept, 2);
}

#[test]
fn a_branch_moved_back_to_an_earlier_tip_is_counted_again_after_the_list_forgot_it() {
    let (_dir, repo, _) = tracked_repo(1);
    let mut cache = AheadBehindCache::default();
    list_branches_cached(&repo, &mut cache).unwrap();
    let tip = repo
        .find_reference("refs/heads/topic0")
        .unwrap()
        .target()
        .unwrap();

    advance(&repo, "refs/heads/topic0", 2_000_000);
    list_branches_cached(&repo, &mut cache).unwrap();
    repo.reference("refs/heads/topic0", tip, true, "back")
        .unwrap();
    let branches = list_branches_cached(&repo, &mut cache).unwrap();

    assert_eq!(counts(&branches, "topic0"), (Some(2), Some(1)));
    assert_eq!(cache.stats().walks, 3);
}

#[test]
fn a_new_shallow_boundary_counts_again() {
    let (_dir, repo, _) = tracked_repo(1);
    let mut cache = AheadBehindCache::default();
    list_branches_cached(&repo, &mut cache).unwrap();

    // Cut topic0's history below its first own commit, as a shallow fetch's boundary does:
    // the base is no longer shared, so it counts on the upstream's side.
    let local = repo
        .find_reference("refs/heads/topic0")
        .unwrap()
        .peel_to_commit()
        .unwrap();
    let first = local.parent_id(0).unwrap();
    std::fs::write(repo.commondir().join("shallow"), format!("{first}\n")).unwrap();
    // A handle libgit2 opened before keeps the commits it parsed with their old parents, so
    // the cache is asked through a new one, as after reopening the repository.
    let fresh = Repository::open(repo.path()).unwrap();
    let expected = counts(&list_branches(&fresh).unwrap(), "topic0");
    assert_ne!(
        expected,
        (Some(2), Some(1)),
        "libgit2 reads the shallow file"
    );

    let branches = list_branches_cached(&fresh, &mut cache).unwrap();

    assert_eq!(counts(&branches, "topic0"), expected);
}
