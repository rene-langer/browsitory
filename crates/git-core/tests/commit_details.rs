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

/// A commit with an empty tree at `time` (Unix seconds) on `parents`, written without moving
/// any ref.
fn commit_at(
    repo: &git2::Repository,
    parents: &[git2::Oid],
    time: i64,
    message: &str,
) -> git2::Oid {
    let tree = {
        let builder = repo.treebuilder(None).unwrap();
        let id = builder.write().unwrap();
        repo.find_tree(id).unwrap()
    };
    let signature =
        git2::Signature::new("Test User", "test@example.com", &git2::Time::new(time, 0)).unwrap();
    let parents: Vec<git2::Commit> = parents
        .iter()
        .map(|id| repo.find_commit(*id).unwrap())
        .collect();
    let parents: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(None, &signature, &signature, message, &tree, &parents)
        .unwrap()
}

/// What the details list for `target`, checked ref by ref with libgit2's
/// `graph_descendant_of`: `(branches, remote_branches, tags)`, each sorted.
fn expected_containment(
    repo: &git2::Repository,
    target: git2::Oid,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let contains = |tip: git2::Oid| tip == target || repo.graph_descendant_of(tip, target).unwrap();
    let mut branches = Vec::new();
    let mut remote_branches = Vec::new();
    let mut tags = Vec::new();
    for reference in repo.references().unwrap() {
        let reference = reference.unwrap();
        if reference.kind() != Some(git2::ReferenceType::Direct) {
            continue;
        }
        let name = reference.name().unwrap().to_string();
        let Ok(tip) = reference.peel_to_commit() else {
            continue;
        };
        if !contains(tip.id()) {
            continue;
        }
        if let Some(short) = name.strip_prefix("refs/heads/") {
            branches.push(short.to_string());
        } else if let Some(short) = name.strip_prefix("refs/remotes/") {
            remote_branches.push(short.to_string());
        } else if let Some(short) = name.strip_prefix("refs/tags/") {
            tags.push(short.to_string());
        }
    }
    branches.sort();
    remote_branches.sort();
    tags.sort();
    (branches, remote_branches, tags)
}

/// A history with merges, criss-cross merges, a side line whose commit times run backwards
/// (clock skew), an unrelated root and refs of every kind on many of its commits. Returns
/// every commit.
fn merge_history(repo: &git2::Repository) -> Vec<git2::Oid> {
    let mut all = Vec::new();
    let mut main = commit_at(repo, &[], 1_000_000, "root");
    all.push(main);
    for step in 0..12_i64 {
        let time = 1_000_000 + step * 100;
        let fork = main;
        let mut side = fork;
        for n in 0..3_i64 {
            // Every other side line is committed with a clock running behind its fork point.
            let skew = if step % 2 == 0 { 50 + n } else { -5_000 - n };
            side = commit_at(repo, &[side], time + skew, &format!("side {step}.{n}"));
            all.push(side);
        }
        main = commit_at(repo, &[main], time + 10, &format!("main {step}"));
        all.push(main);
        if step % 3 == 2 {
            // A side line left unmerged, with a branch on it.
            repo.reference(&format!("refs/heads/topic-{step}"), side, true, "test")
                .unwrap();
        } else if step == 6 {
            // Criss-cross: main and the side line merge each other, then main merges both.
            let criss = commit_at(repo, &[main, side], time + 60, "criss");
            let cross = commit_at(repo, &[side, main], time + 70, "cross");
            all.push(criss);
            all.push(cross);
            main = commit_at(repo, &[criss, cross], time + 80, "criss-cross merge");
            all.push(main);
        } else {
            main = commit_at(repo, &[main, side], time + 60, &format!("merge {step}"));
            all.push(main);
        }
    }
    repo.reference("refs/heads/main", main, true, "test")
        .unwrap();
    let unrelated = commit_at(repo, &[], 2_000_000, "unrelated root");
    let unrelated_tip = commit_at(repo, &[unrelated], 2_000_001, "unrelated tip");
    all.push(unrelated);
    all.push(unrelated_tip);
    repo.reference("refs/heads/pages", unrelated_tip, true, "test")
        .unwrap();
    let signature = repo.signature().unwrap();
    for (index, id) in all.clone().into_iter().enumerate() {
        let object = repo.find_object(id, None).unwrap();
        match index % 5 {
            0 => {
                repo.tag_lightweight(&format!("light-{index}"), &object, false)
                    .unwrap();
            }
            1 => {
                repo.tag(
                    &format!("annotated-{index}"),
                    &object,
                    &signature,
                    "t",
                    false,
                )
                .unwrap();
            }
            2 => {
                repo.reference(&format!("refs/remotes/origin/r-{index}"), id, true, "test")
                    .unwrap();
            }
            3 => {
                repo.reference(&format!("refs/heads/b-{index}"), id, true, "test")
                    .unwrap();
            }
            _ => {}
        }
    }
    all
}

#[test]
fn every_commit_lists_the_same_refs_as_checking_each_ref_on_its_own() {
    let (_dir, repo) = init_repo();
    let all = merge_history(&repo);

    for id in all {
        let details = commit_details(&repo, &id.to_string(), BUDGET).unwrap();
        let (branches, remote_branches, tags) = expected_containment(&repo, id);
        assert!(details.complete, "{id}");
        assert_eq!(details.branches, branches, "branches of {id}");
        assert_eq!(
            details.remote_branches, remote_branches,
            "remote branches of {id}"
        );
        assert_eq!(details.tags, tags, "tags of {id}");
    }
}

#[test]
fn a_tag_on_a_tag_counts_by_the_commit_it_finally_names() {
    let (_dir, repo) = init_repo();
    let ids = history(&repo);
    let signature = repo.signature().unwrap();
    let middle = repo.find_object(ids[1], None).unwrap();
    let inner = repo
        .tag("inner", &middle, &signature, "inner", false)
        .unwrap();
    let inner = repo.find_object(inner, None).unwrap();
    repo.tag("outer", &inner, &signature, "outer", false)
        .unwrap();

    let base = commit_details(&repo, &ids[0].to_string(), BUDGET).unwrap();
    assert_eq!(base.tags, vec!["inner", "outer"]);
    let side = commit_details(&repo, &ids[3].to_string(), BUDGET).unwrap();
    assert!(side.tags.is_empty());
}

/// #842: 3,000 tags on a 3,000-commit line, the commit asked about at its root. Checked one
/// ref at a time this walks about 4.5 million commits and runs far past a second; one walk for
/// all refs covers the 3,000 commits once.
#[test]
fn many_tags_above_an_old_commit_are_all_checked_within_the_budget() {
    let (_dir, repo) = init_repo();
    let root = commit_at(&repo, &[], 1_000_000, "root");
    let mut tip = root;
    repo.tag_lightweight("t-0000", &repo.find_object(root, None).unwrap(), false)
        .unwrap();
    for n in 1..3_000_i64 {
        tip = commit_at(&repo, &[tip], 1_000_000 + n, &format!("c{n}"));
        repo.tag_lightweight(
            &format!("t-{n:04}"),
            &repo.find_object(tip, None).unwrap(),
            false,
        )
        .unwrap();
    }
    repo.reference("refs/heads/main", tip, true, "test")
        .unwrap();

    let details = commit_details(&repo, &root.to_string(), Duration::from_secs(1)).unwrap();
    assert!(details.complete, "the scan ran out of its budget");
    assert_eq!(details.tags.len(), 3_000);
    assert_eq!(details.branches, vec!["main"]);
}

/// #842: the walk follows no commit dated more than a day before the commit asked about (git
/// name-rev's slop), so a ref above such a commit is not listed; one dated exactly a day
/// before is still followed. The owner accepted this cut-off for the one walk.
#[test]
fn commits_dated_more_than_a_day_before_the_commit_are_not_followed() {
    let (_dir, repo) = init_repo();
    const DAY: i64 = 24 * 60 * 60;
    let target = commit_at(&repo, &[], 1_000_000, "target");
    let too_old = commit_at(
        &repo,
        &[target],
        1_000_000 - DAY - 1,
        "a day and a second before",
    );
    let above_too_old = commit_at(&repo, &[too_old], 1_000_100, "above it");
    let edge = commit_at(&repo, &[target], 1_000_000 - DAY, "exactly a day before");
    let above_edge = commit_at(&repo, &[edge], 1_000_100, "above the edge");
    for (name, id) in [
        ("too-old", too_old),
        ("above-too-old", above_too_old),
        ("edge", edge),
        ("above-edge", above_edge),
    ] {
        repo.reference(&format!("refs/heads/{name}"), id, true, "test")
            .unwrap();
    }

    let details = commit_details(&repo, &target.to_string(), BUDGET).unwrap();
    assert!(details.complete);
    assert_eq!(details.branches, vec!["above-edge", "edge"]);
}
