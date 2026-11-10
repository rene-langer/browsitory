mod common;

use common::{commit_all, commit_at, commit_tree, init_repo, write_file};
use git_core::graph::GraphSelection;

#[test]
fn graph_log_returns_an_empty_vec_for_a_repository_with_no_commits() {
    let (_dir, repo) = init_repo();

    let result = git_core::graph::graph_log(&repo, 10, &GraphSelection::default()).unwrap();

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

    let commits = git_core::graph::graph_log(&repo, 10, &GraphSelection::default()).unwrap();

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

    let commits = git_core::graph::graph_log(&repo, 10, &GraphSelection::default()).unwrap();

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

    let commits = git_core::graph::graph_log(&repo, 10, &GraphSelection::default()).unwrap();

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

    let commits = git_core::graph::graph_log(&repo, 10, &local(&[&main_branch])).unwrap();

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

    let commits = git_core::graph::graph_log(&repo, 2, &GraphSelection::default()).unwrap();

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

    let commits = git_core::graph::graph_log(&repo, 10, &GraphSelection::default()).unwrap();

    let merge_commit = commits
        .iter()
        .find(|c| c.summary == "merge feature into main")
        .unwrap();
    assert_eq!(merge_commit.parent_ids.len(), 2);
}

#[test]
fn commit_message_returns_the_full_message_including_the_body() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(
        &repo,
        "subject line\n\nbody paragraph one\n\nbody paragraph two\n",
    );
    let id = repo.head().unwrap().target().unwrap().to_string();

    let message = git_core::graph::commit_message(&repo, &id).unwrap();

    assert_eq!(
        message,
        "subject line\n\nbody paragraph one\n\nbody paragraph two\n"
    );
}

#[test]
fn commit_message_rejects_an_unknown_commit_id() {
    let (_dir, repo) = init_repo();

    let result = git_core::graph::commit_message(&repo, "0000000000000000000000000000000000000000");

    assert!(result.is_err());
}

#[test]
fn graph_log_reports_remote_branch_refs_for_a_tip_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let oid = repo.head().unwrap().target().unwrap();
    repo.reference("refs/remotes/origin/main", oid, true, "test")
        .unwrap();

    let commits = git_core::graph::graph_log(&repo, 10, &GraphSelection::default()).unwrap();

    assert_eq!(
        commits[0].remote_branch_refs,
        vec!["origin/main".to_string()]
    );
}

#[test]
fn graph_log_excludes_a_remote_symbolic_head_ref() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let oid = repo.head().unwrap().target().unwrap();
    repo.reference("refs/remotes/origin/main", oid, true, "test")
        .unwrap();
    repo.reference_symbolic(
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/main",
        true,
        "test",
    )
    .unwrap();

    let commits = git_core::graph::graph_log(&repo, 10, &GraphSelection::default()).unwrap();

    // Only the direct ref should show up — the symbolic HEAD alias must not produce a
    // duplicate or an "origin/HEAD" badge.
    assert_eq!(
        commits[0].remote_branch_refs,
        vec!["origin/main".to_string()]
    );
}

#[test]
fn graph_log_keeps_local_and_remote_branch_refs_independent() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    let oid = repo.head().unwrap().target().unwrap();
    repo.reference("refs/remotes/origin/main", oid, true, "test")
        .unwrap();

    let commits = git_core::graph::graph_log(&repo, 10, &GraphSelection::default()).unwrap();

    assert_eq!(commits[0].branch_refs, vec![main_branch]);
    assert_eq!(
        commits[0].remote_branch_refs,
        vec!["origin/main".to_string()]
    );
}

#[test]
fn graph_log_reports_lightweight_and_annotated_tags_sorted_on_their_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "first");
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "second");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    let signature = repo.signature().unwrap();
    repo.tag_lightweight("zeta", head.as_object(), false)
        .unwrap();
    repo.tag("alpha", head.as_object(), &signature, "annotated", false)
        .unwrap();

    let commits = git_core::graph::graph_log(&repo, 10, &GraphSelection::default()).unwrap();

    assert_eq!(commits[0].summary, "second");
    assert_eq!(
        commits[0].tags,
        vec!["alpha".to_string(), "zeta".to_string()]
    );
    assert!(commits[1].tags.is_empty());
}

#[test]
fn graph_log_ignores_a_tag_that_does_not_point_at_a_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "first");
    let tree = repo.head().unwrap().peel_to_tree().unwrap();
    repo.tag_lightweight("tree-tag", tree.as_object(), false)
        .unwrap();

    let commits = git_core::graph::graph_log(&repo, 10, &GraphSelection::default()).unwrap();

    assert_eq!(commits.len(), 1);
    assert!(commits[0].tags.is_empty());
}

#[test]
fn graph_log_skips_a_selected_branch_that_no_longer_exists() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    git_core::branch::create_branch(&repo, "gone", "HEAD").unwrap();
    git_core::branch::switch_branch(&repo, &main).unwrap();
    git_core::branch::delete_branch(&repo, "gone", true).unwrap();

    let commits = git_core::graph::graph_log(&repo, 10, &local(&["gone", &main])).unwrap();

    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].summary, "initial commit");
    assert!(git_core::graph::graph_log(&repo, 10, &local(&["gone"]))
        .unwrap()
        .is_empty());
}

/// `trunk`: base → direct → merge(direct, feature 3) → after; `feature`: base → feature 1..3.
/// Returns the ids `[base, direct, feature 1, feature 2, feature 3, merge, after]`.
fn merged_feature_history(repo: &git2::Repository) -> Vec<git2::Oid> {
    const FILE: u32 = 0o100644;
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
    repo.reference("refs/heads/trunk", after, true, "test")
        .unwrap();
    repo.reference("refs/heads/feature", f3, true, "test")
        .unwrap();
    vec![base, direct, f1, f2, f3, merge, after]
}

#[test]
fn graph_log_of_one_selected_branch_follows_first_parents_only() {
    let (_dir, repo) = init_repo();
    let ids = merged_feature_history(&repo);

    let commits = git_core::graph::graph_log(&repo, 50, &local(&["trunk"])).unwrap();

    let summaries: Vec<&str> = commits.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(
        summaries,
        ["after merge", "merge feature", "direct on trunk", "base"]
    );
    // The merge keeps both parents, though its second parent is not in the list.
    assert_eq!(
        commits[1].parent_ids,
        [ids[1].to_string(), ids[4].to_string()]
    );
}

#[test]
fn graph_log_of_several_or_all_branches_still_walks_every_ancestor() {
    let (_dir, repo) = init_repo();
    merged_feature_history(&repo);

    let both = git_core::graph::graph_log(&repo, 50, &local(&["trunk", "feature"])).unwrap();
    let all = git_core::graph::graph_log(&repo, 50, &GraphSelection::default()).unwrap();

    for commits in [both, all] {
        assert_eq!(commits.len(), 7);
        assert!(commits.iter().any(|c| c.summary == "feature 2"));
        assert_eq!(commits.last().unwrap().summary, "base");
    }
}

/// Exactly the local branches `names`, no remote branch.
fn local(names: &[&str]) -> GraphSelection {
    selection(Some(names), &[])
}

fn selection(local: Option<&[&str]>, remote: &[&str]) -> GraphSelection {
    GraphSelection {
        local: local.map(|names| names.iter().map(|n| n.to_string()).collect()),
        remote: remote.iter().map(|n| n.to_string()).collect(),
        first_parent: false,
    }
}

/// [`selection`] with "Collapse merges" on (#617).
fn collapsed(local: Option<&[&str]>) -> GraphSelection {
    GraphSelection {
        first_parent: true,
        ..selection(local, &[])
    }
}

fn summaries(commits: &[git_core::graph::GraphCommit]) -> Vec<&str> {
    commits.iter().map(|c| c.summary.as_str()).collect()
}

/// A repository whose `origin` (a second local repository, fetched) has `trunk` and `feature`
/// from [`merged_feature_history`] plus `colleague` (base → colleague 1 → colleague 2). The
/// only local branch is `main`, at `base`; every other commit exists only on `origin`.
fn fetched_remote() -> (tempfile::TempDir, tempfile::TempDir, git2::Repository) {
    const FILE: u32 = 0o100644;
    let (upstream_dir, upstream) = init_repo();
    let ids = merged_feature_history(&upstream);
    let c1 = commit_tree(&upstream, &[("c.txt", "1", FILE)], &[ids[0]], "colleague 1");
    let c2 = commit_tree(&upstream, &[("c.txt", "2", FILE)], &[c1], "colleague 2");
    upstream
        .reference("refs/heads/colleague", c2, true, "test")
        .unwrap();

    let (dir, repo) = init_repo();
    repo.remote("origin", upstream_dir.path().to_str().unwrap())
        .unwrap()
        .fetch(&[] as &[&str], None, None)
        .unwrap();
    repo.reference("refs/heads/main", ids[0], true, "test")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
    (upstream_dir, dir, repo)
}

#[test]
fn the_default_graph_walks_no_remote_branch() {
    let (_upstream, _dir, repo) = fetched_remote();

    let commits = git_core::graph::graph_log(&repo, 50, &GraphSelection::default()).unwrap();

    assert_eq!(summaries(&commits), ["base"]);
}

#[test]
fn a_selected_remote_branch_adds_its_commits_to_the_graph() {
    let (_upstream, _dir, repo) = fetched_remote();

    let commits =
        git_core::graph::graph_log(&repo, 50, &selection(None, &["origin/colleague"])).unwrap();

    assert_eq!(summaries(&commits), ["colleague 2", "colleague 1", "base"]);
    assert_eq!(commits[0].remote_branch_refs, ["origin/colleague"]);
    assert_eq!(commits[2].branch_refs, ["main"]);
}

#[test]
fn one_remote_branch_alone_follows_first_parents_only() {
    let (_upstream, _dir, repo) = fetched_remote();

    let commits =
        git_core::graph::graph_log(&repo, 50, &selection(Some(&[]), &["origin/trunk"])).unwrap();

    assert_eq!(
        summaries(&commits),
        ["after merge", "merge feature", "direct on trunk", "base"]
    );
}

#[test]
fn a_local_and_a_remote_branch_walk_every_ancestor() {
    let (_upstream, _dir, repo) = fetched_remote();

    let commits =
        git_core::graph::graph_log(&repo, 50, &selection(Some(&["main"]), &["origin/trunk"]))
            .unwrap();

    assert_eq!(commits.len(), 7);
    assert!(commits.iter().any(|c| c.summary == "feature 2"));
}

#[test]
fn a_selected_remote_branch_that_no_longer_exists_is_skipped() {
    let (_upstream, _dir, repo) = fetched_remote();

    let commits = git_core::graph::graph_log(
        &repo,
        50,
        &selection(Some(&[]), &["origin/gone", "origin/colleague"]),
    )
    .unwrap();

    assert_eq!(summaries(&commits), ["colleague 2", "colleague 1", "base"]);
    assert!(
        git_core::graph::graph_log(&repo, 50, &selection(Some(&[]), &["origin/gone"]))
            .unwrap()
            .is_empty()
    );
}

/// `base` on `main`, then HEAD detached at `base` and two commits on it that no branch
/// contains (#467).
fn detached_ahead_of_every_branch() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().target().unwrap();
    repo.reference("refs/heads/main", base, true, "test")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
    repo.set_head_detached(base).unwrap();
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "detached 1");
    write_file(dir.path(), "file.txt", "v3");
    commit_all(&repo, "detached 2");
    (dir, repo)
}

#[test]
fn the_default_graph_includes_a_detached_head_that_no_branch_contains() {
    let (_dir, repo) = detached_ahead_of_every_branch();

    let commits = git_core::graph::graph_log(&repo, 50, &GraphSelection::default()).unwrap();

    assert_eq!(summaries(&commits), ["detached 2", "detached 1", "base"]);
    assert!(commits[0].branch_refs.is_empty());
}

#[test]
fn a_branch_selection_still_includes_the_detached_head() {
    let (_dir, repo) = detached_ahead_of_every_branch();

    for selection in [local(&["main"]), local(&[]), local(&["gone"])] {
        let commits = git_core::graph::graph_log(&repo, 50, &selection).unwrap();

        assert_eq!(
            summaries(&commits),
            ["detached 2", "detached 1", "base"],
            "{selection:?}"
        );
    }
}

#[test]
fn a_detached_head_at_a_branch_tip_adds_no_duplicate_row() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "base");
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "tip");
    let tip = repo.head().unwrap().target().unwrap();
    repo.set_head_detached(tip).unwrap();

    let commits = git_core::graph::graph_log(&repo, 50, &GraphSelection::default()).unwrap();

    assert_eq!(summaries(&commits), ["tip", "base"]);
}

/// [`merged_feature_history`] plus `side`: base → side work (never merged). Returns its ids
/// followed by `side work`'s.
fn merged_feature_and_side(repo: &git2::Repository) -> Vec<git2::Oid> {
    const FILE: u32 = 0o100644;
    let mut ids = merged_feature_history(repo);
    let side = commit_tree(repo, &[("s.txt", "s", FILE)], &[ids[0]], "side work");
    repo.reference("refs/heads/side", side, true, "test")
        .unwrap();
    ids.push(side);
    ids
}

fn sorted(mut names: Vec<&str>) -> Vec<&str> {
    names.sort_unstable();
    names
}

#[test]
fn collapse_merges_follows_first_parents_from_every_selected_branch() {
    let (_dir, repo) = init_repo();
    merged_feature_and_side(&repo);

    for selection in [collapsed(None), collapsed(Some(&["trunk", "side"]))] {
        let commits = git_core::graph::graph_log(&repo, 50, &selection).unwrap();

        // `feature` is walked too when every branch is: its tip is `feature 3`, the first
        // parents of which are `feature 2`, `feature 1`.
        let expected: Vec<&str> = if selection.local.is_none() {
            vec![
                "after merge",
                "merge feature",
                "direct on trunk",
                "feature 3",
                "feature 2",
                "feature 1",
                "side work",
                "base",
            ]
        } else {
            vec![
                "after merge",
                "merge feature",
                "direct on trunk",
                "side work",
                "base",
            ]
        };
        assert_eq!(
            sorted(summaries(&commits)),
            sorted(expected),
            "{selection:?}"
        );
        assert_eq!(commits.last().unwrap().summary, "base");
    }
}

#[test]
fn a_merge_row_of_a_first_parent_walk_counts_the_commits_it_brought_in() {
    let (_dir, repo) = init_repo();
    merged_feature_and_side(&repo);

    let both = collapsed(Some(&["trunk", "side"]));
    let collapsed_rows = git_core::graph::graph_log(&repo, 50, &both).unwrap();
    let isolated = git_core::graph::graph_log(&repo, 50, &local(&["trunk"])).unwrap();
    let expanded = git_core::graph::graph_log(&repo, 50, &GraphSelection::default()).unwrap();

    for commits in [&collapsed_rows, &isolated] {
        let merge = commits
            .iter()
            .find(|c| c.summary == "merge feature")
            .unwrap();
        assert_eq!(merge.merged_count, Some(3));
        // Only merges carry a count.
        let others_uncounted = commits
            .iter()
            .filter(|c| c.summary != "merge feature")
            .all(|c| c.merged_count.is_none());
        assert!(others_uncounted);
    }
    // A graph that walks every ancestor already shows them.
    assert!(expanded.iter().all(|c| c.merged_count.is_none()));
}

#[test]
fn merged_commits_lists_first_to_other_parent_children_first() {
    let (_dir, repo) = init_repo();
    let ids = merged_feature_history(&repo);

    let merged = git_core::graph::merged_commits(&repo, &ids[5].to_string(), 50).unwrap();
    let limited = git_core::graph::merged_commits(&repo, &ids[5].to_string(), 2).unwrap();

    assert_eq!(summaries(&merged), ["feature 3", "feature 2", "feature 1"]);
    assert_eq!(summaries(&limited), ["feature 3", "feature 2"]);
    // Decorated like graph rows: `feature` points at its tip.
    assert_eq!(merged[0].branch_refs, vec!["feature".to_string()]);
    assert_eq!(merged[0].merged_count, None);
}

#[test]
fn merged_commits_of_an_octopus_merge_lists_every_other_parent() {
    const FILE: u32 = 0o100644;
    let (_dir, repo) = init_repo();
    let base = commit_tree(&repo, &[("a.txt", "a", FILE)], &[], "base");
    let main = commit_tree(&repo, &[("a.txt", "m", FILE)], &[base], "on main");
    let one = commit_tree(&repo, &[("b.txt", "1", FILE)], &[base], "topic one");
    let two_a = commit_tree(&repo, &[("c.txt", "2", FILE)], &[base], "topic two a");
    let two_b = commit_tree(&repo, &[("c.txt", "3", FILE)], &[two_a], "topic two b");
    let octopus = commit_tree(
        &repo,
        &[("a.txt", "o", FILE)],
        &[main, one, two_b],
        "octopus",
    );
    repo.reference("refs/heads/main", octopus, true, "test")
        .unwrap();

    let merged = git_core::graph::merged_commits(&repo, &octopus.to_string(), 50).unwrap();
    let rows = git_core::graph::graph_log(&repo, 50, &collapsed(None)).unwrap();

    assert_eq!(
        sorted(summaries(&merged)),
        ["topic one", "topic two a", "topic two b"]
    );
    let b = merged.iter().position(|c| c.summary == "topic two b");
    let a = merged.iter().position(|c| c.summary == "topic two a");
    assert!(b < a, "children before parents: {:?}", summaries(&merged));
    assert_eq!(summaries(&rows), ["octopus", "on main", "base"]);
    assert_eq!(rows[0].merged_count, Some(3));
}

#[test]
fn merged_commits_include_a_side_branch_merged_into_the_side_branch() {
    const FILE: u32 = 0o100644;
    let (_dir, repo) = init_repo();
    let base = commit_tree(&repo, &[("a.txt", "a", FILE)], &[], "base");
    let main = commit_tree(&repo, &[("a.txt", "m", FILE)], &[base], "on main");
    let topic = commit_tree(&repo, &[("b.txt", "t", FILE)], &[base], "topic");
    let sub = commit_tree(&repo, &[("c.txt", "s", FILE)], &[base], "sub topic");
    let inner = commit_tree(&repo, &[], &[topic, sub], "merge sub");
    let outer = commit_tree(&repo, &[], &[main, inner], "merge topic");
    repo.reference("refs/heads/main", outer, true, "test")
        .unwrap();

    let merged = git_core::graph::merged_commits(&repo, &outer.to_string(), 50).unwrap();

    assert_eq!(summaries(&merged)[0], "merge sub");
    assert_eq!(
        sorted(summaries(&merged)),
        ["merge sub", "sub topic", "topic"]
    );
}

#[test]
fn merged_commits_of_a_commit_that_is_no_merge_is_empty() {
    let (_dir, repo) = init_repo();
    let ids = merged_feature_history(&repo);

    let merged = git_core::graph::merged_commits(&repo, &ids[6].to_string(), 50).unwrap();

    assert!(merged.is_empty());
    let malformed = git_core::graph::merged_commits(&repo, "not a sha", 50);
    assert!(malformed.is_err());
}

#[test]
fn merged_commits_list_unrelated_commits_newest_first() {
    // The octopus brings in three tips; their order is neither the order of the merge's parents
    // nor its reverse, only their commit time.
    let (_dir, repo) = init_repo();
    let base = commit_at(&repo, &[], "base", 100);
    let main = commit_at(&repo, &[base], "on main", 150);
    let middle = commit_at(&repo, &[base], "middle", 300);
    let newest = commit_at(&repo, &[base], "newest", 400);
    let oldest = commit_at(&repo, &[base], "oldest", 200);
    let octopus = commit_at(&repo, &[main, middle, newest, oldest], "octopus", 500);

    let merged = git_core::graph::merged_commits(&repo, &octopus.to_string(), 50).unwrap();

    assert_eq!(summaries(&merged), ["newest", "middle", "oldest"]);
}
