mod common;

use common::{commit_tree, init_repo};
use git_core::graph::{graph_log, GraphSelection};
use git_core::locate::{locate_commit, CommitLocation};

const FILE: u32 = 0o100644;

/// `trunk`: base → direct → merge(direct, feature 3) → after; `feature`: base → feature 1..3;
/// `side`: base → side (never merged). Returns `[base, direct, f1, f2, f3, merge, after, side]`.
fn history(repo: &git2::Repository) -> Vec<git2::Oid> {
    let base = commit_tree(repo, &[("a.txt", "a", FILE)], &[], "base");
    let direct = commit_tree(repo, &[("a.txt", "a2", FILE)], &[base], "direct on trunk");
    let f1 = commit_tree(repo, &[("a.txt", "f1", FILE)], &[base], "feature 1");
    let f2 = commit_tree(repo, &[("a.txt", "f2", FILE)], &[f1], "feature 2");
    let f3 = commit_tree(repo, &[("a.txt", "f3", FILE)], &[f2], "feature 3");
    let merge = commit_tree(
        repo,
        &[("a.txt", "m", FILE)],
        &[direct, f3],
        "merge feature",
    );
    let after = commit_tree(repo, &[("a.txt", "m2", FILE)], &[merge], "after merge");
    let side = commit_tree(repo, &[("a.txt", "s", FILE)], &[base], "side work");
    repo.reference("refs/heads/trunk", after, true, "test")
        .unwrap();
    repo.reference("refs/heads/feature", f3, true, "test")
        .unwrap();
    repo.reference("refs/heads/side", side, true, "test")
        .unwrap();
    vec![base, direct, f1, f2, f3, merge, after, side]
}

fn local(names: &[&str]) -> GraphSelection {
    GraphSelection {
        local: Some(names.iter().map(|n| n.to_string()).collect()),
        remote: Vec::new(),
    }
}

#[test]
fn a_commit_in_the_graph_walk_reports_its_row_in_the_graph() {
    let (_dir, repo) = init_repo();
    let ids = history(&repo);
    let selection = GraphSelection::default();
    let graph = graph_log(&repo, 50, &selection).unwrap();

    for id in [ids[0], ids[3], ids[7]] {
        let location = locate_commit(&repo, &id.to_string(), &selection)
            .unwrap()
            .unwrap();
        let row = graph.iter().position(|c| c.id == id.to_string());
        assert_eq!(location.commit_id, id.to_string());
        assert_eq!(location.position, row);
        assert!(
            location.branches.is_empty(),
            "only filled for a hidden commit"
        );
    }
}

#[test]
fn a_sha_prefix_a_branch_name_and_a_tag_resolve_to_their_commit() {
    let (_dir, repo) = init_repo();
    let ids = history(&repo);
    let target = repo.find_commit(ids[2]).unwrap();
    repo.tag_lightweight("light", target.as_object(), false)
        .unwrap();
    let sig = repo.signature().unwrap();
    repo.tag(
        "annotated",
        target.as_object(),
        &sig,
        "an annotated tag",
        false,
    )
    .unwrap();
    let selection = GraphSelection::default();

    let by_prefix = locate_commit(&repo, &ids[2].to_string()[..7], &selection)
        .unwrap()
        .unwrap();
    let by_upper = locate_commit(&repo, &ids[2].to_string()[..7].to_uppercase(), &selection)
        .unwrap()
        .unwrap();
    let by_branch = locate_commit(&repo, "refs/heads/side", &selection)
        .unwrap()
        .unwrap();
    let by_light = locate_commit(&repo, "refs/tags/light", &selection)
        .unwrap()
        .unwrap();
    let by_annotated = locate_commit(&repo, "annotated", &selection)
        .unwrap()
        .unwrap();

    assert_eq!(by_prefix.commit_id, ids[2].to_string());
    assert_eq!(by_upper.commit_id, ids[2].to_string());
    assert_eq!(by_branch.commit_id, ids[7].to_string());
    assert_eq!(by_light.commit_id, ids[2].to_string());
    assert_eq!(by_annotated.commit_id, ids[2].to_string());
}

#[test]
fn an_unknown_revision_or_a_non_commit_object_is_none() {
    let (_dir, repo) = init_repo();
    history(&repo);
    let selection = GraphSelection::default();

    assert_eq!(locate_commit(&repo, "deadbeef", &selection).unwrap(), None);
    assert_eq!(
        locate_commit(&repo, "no-such-branch", &selection).unwrap(),
        None
    );
    assert_eq!(locate_commit(&repo, "", &selection).unwrap(), None);
    assert_eq!(locate_commit(&repo, "  ", &selection).unwrap(), None);
    assert_eq!(
        locate_commit(&repo, "trunk^{tree}", &selection).unwrap(),
        None
    );
    assert_eq!(
        locate_commit(&repo, "not a ^ valid :: spec", &selection).unwrap(),
        None
    );
}

#[test]
fn a_commit_only_on_a_hidden_branch_names_that_branch() {
    let (_dir, repo) = init_repo();
    let ids = history(&repo);

    let location = locate_commit(&repo, &ids[7].to_string(), &local(&["trunk", "feature"]))
        .unwrap()
        .unwrap();

    assert_eq!(
        location,
        CommitLocation {
            commit_id: ids[7].to_string(),
            position: None,
            branches: vec!["side".to_string()],
            remote_branches: Vec::new(),
        }
    );
}

#[test]
fn a_commit_only_on_a_remote_branch_names_that_remote_branch() {
    let (_dir, repo) = init_repo();
    let ids = history(&repo);
    let remote_only = commit_tree(&repo, &[("a.txt", "r", FILE)], &[ids[6]], "remote work");
    repo.reference("refs/remotes/origin/trunk", remote_only, true, "test")
        .unwrap();
    repo.reference_symbolic(
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/trunk",
        true,
        "test",
    )
    .unwrap();

    let hidden = locate_commit(&repo, &remote_only.to_string(), &GraphSelection::default())
        .unwrap()
        .unwrap();
    let shown = GraphSelection {
        local: None,
        remote: vec!["origin/trunk".to_string()],
    };
    let walked = locate_commit(&repo, &remote_only.to_string(), &shown)
        .unwrap()
        .unwrap();

    assert_eq!(hidden.position, None);
    assert!(hidden.branches.is_empty());
    assert_eq!(hidden.remote_branches, vec!["origin/trunk".to_string()]);
    // Its row in that graph (commits made in the same second may come in either order).
    let graph = graph_log(&repo, 50, &shown).unwrap();
    let row = graph.iter().position(|c| c.id == remote_only.to_string());
    assert!(row.is_some());
    assert_eq!(walked.position, row);
}

#[test]
fn a_side_branch_commit_of_an_isolated_branch_names_that_branch_too() {
    let (_dir, repo) = init_repo();
    let ids = history(&repo);

    // One selected branch walks first parents only, so `feature 2` is not in the graph although
    // `trunk` contains it.
    let location = locate_commit(&repo, &ids[3].to_string(), &local(&["trunk"]))
        .unwrap()
        .unwrap();

    assert_eq!(location.position, None);
    assert_eq!(
        location.branches,
        vec!["feature".to_string(), "trunk".to_string()]
    );
}

#[test]
fn a_commit_on_no_branch_has_no_position_and_no_branches() {
    let (_dir, repo) = init_repo();
    let ids = history(&repo);
    let dangling = commit_tree(&repo, &[("a.txt", "d", FILE)], &[ids[0]], "dangling");

    let location = locate_commit(&repo, &dangling.to_string(), &GraphSelection::default())
        .unwrap()
        .unwrap();

    assert_eq!(location.position, None);
    assert!(location.branches.is_empty());
    assert!(location.remote_branches.is_empty());
}
