mod common;

use common::{commit_all, commit_at, init_repo, write_file};

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
fn switch_branch_rejects_a_branch_checked_out_in_another_worktree() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let initial_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    let linked = dir.path().join("feature-tree");
    git_core::worktree::create_worktree(&repo, "feature-tree", &linked, "feature", None).unwrap();
    let linked_repo = git2::Repository::open(&linked).unwrap();
    write_file(&linked, "file.txt", "feature work");
    commit_all(&linked_repo, "feature commit");

    let result = git_core::branch::switch_branch(&repo, "feature");

    assert!(result.is_err());
    assert!(
        git_core::branch::list_branches(&repo)
            .unwrap()
            .iter()
            .find(|branch| branch.name == initial_branch)
            .unwrap()
            .is_current
    );
    let contents = std::fs::read_to_string(dir.path().join("file.txt")).unwrap();
    assert_eq!(contents, "v1");
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

/// `main` with one commit, `feature` from it with `extra` commits of its own, HEAD back on
/// `main`.
fn repo_with_feature(extra: usize) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    repo.set_head("refs/heads/main").unwrap();
    write_file(dir.path(), "file.txt", "v0");
    commit_all(&repo, "initial commit");
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    for n in 0..extra {
        write_file(dir.path(), "file.txt", &format!("v{}", n + 1));
        commit_all(&repo, &format!("feature commit {n}"));
    }
    git_core::branch::switch_branch(&repo, "main").unwrap();
    (dir, repo)
}

#[test]
fn commits_only_on_branch_counts_the_commits_no_other_branch_has() {
    // Delete branch asks first when commits would be left on no branch (#996).
    let (_dir, repo) = repo_with_feature(2);

    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        2
    );
}

#[test]
fn commits_only_on_branch_is_zero_for_a_branch_merged_into_head() {
    let (_dir, repo) = repo_with_feature(0);

    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        0
    );
}

#[test]
fn commits_only_on_branch_counts_another_local_branch_as_keeping_them() {
    // Not merged into HEAD, but `keep` still has the commits: deleting loses nothing.
    let (_dir, repo) = repo_with_feature(1);
    let tip = repo
        .find_branch("feature", git2::BranchType::Local)
        .unwrap()
        .get()
        .peel_to_commit()
        .unwrap();
    repo.branch("keep", &tip, false).unwrap();

    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        0
    );
}

#[test]
fn commits_only_on_branch_counts_a_remote_tracking_branch_as_keeping_them() {
    let (_dir, repo) = repo_with_feature(3);
    let feature = repo
        .find_branch("feature", git2::BranchType::Local)
        .unwrap()
        .get()
        .peel_to_commit()
        .unwrap();
    // origin/feature has all but the newest commit.
    let parent = feature.parent_id(0).unwrap();
    repo.reference("refs/remotes/origin/feature", parent, true, "test")
        .unwrap();

    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        1
    );
}

#[test]
fn commits_only_on_branch_fails_for_a_missing_branch() {
    let (_dir, repo) = repo_with_feature(0);

    assert!(git_core::branch::commits_only_on_branch(&repo, "nope").is_err());
}

#[test]
fn commits_only_on_branch_counts_a_tag_as_keeping_them() {
    // A tag keeps its commit and the history behind it reachable: deleting loses nothing there.
    let (_dir, repo) = repo_with_feature(3);
    let feature = repo
        .find_branch("feature", git2::BranchType::Local)
        .unwrap()
        .get()
        .peel_to_commit()
        .unwrap();
    let parent = repo.find_commit(feature.parent_id(0).unwrap()).unwrap();
    repo.tag_lightweight("v1", parent.as_object(), false)
        .unwrap();

    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        1
    );
}

#[test]
fn commits_only_on_branch_counts_an_annotated_tag_as_keeping_them() {
    let (_dir, repo) = repo_with_feature(2);
    let feature = repo
        .find_branch("feature", git2::BranchType::Local)
        .unwrap()
        .get()
        .peel_to_commit()
        .unwrap();
    let signature = repo.signature().unwrap();
    repo.tag("v2", feature.as_object(), &signature, "release", false)
        .unwrap();

    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        0
    );
}

/// `count` commits in a line on top of `parent` (none: a root), one second apart from `start`.
fn line_of_commits(
    repo: &git2::Repository,
    parent: Option<git2::Oid>,
    count: usize,
    start: i64,
) -> Vec<git2::Oid> {
    let mut ids = Vec::new();
    let mut parent = parent;
    for n in 0..count {
        let parents: Vec<git2::Oid> = parent.into_iter().collect();
        let id = commit_at(repo, &parents, &format!("commit {n}"), start + n as i64);
        ids.push(id);
        parent = Some(id);
    }
    ids
}

/// #1067: a long main line with a tag on every commit, a feature branch forked early, and tags
/// on and beyond the feature's own commits. The count stays exact however many refs there are.
#[test]
fn commits_only_on_branch_is_exact_with_a_tag_on_every_commit() {
    let (_dir, repo) = init_repo();
    let main = line_of_commits(&repo, None, 400, 1_000);
    repo.reference("refs/heads/main", main[399], true, "test")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
    for (n, id) in main.iter().enumerate() {
        let commit = repo.find_commit(*id).unwrap();
        repo.tag_lightweight(&format!("m{n}"), commit.as_object(), false)
            .unwrap();
    }
    // Forked at main's 50th commit; newer than all of main.
    let feature = line_of_commits(&repo, Some(main[49]), 6, 5_000);
    repo.reference("refs/heads/feature", feature[5], true, "test")
        .unwrap();
    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        6
    );

    // A tag on the feature's second commit keeps it and the first.
    let second = repo.find_commit(feature[1]).unwrap();
    repo.tag_lightweight("f2", second.as_object(), false)
        .unwrap();
    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        4
    );

    // A tag on a commit on no branch, made on top of the feature's fourth commit, keeps the
    // first four.
    let side = commit_at(&repo, &[feature[3]], "side", 9_000);
    let side = repo.find_commit(side).unwrap();
    repo.tag_lightweight("side", side.as_object(), false)
        .unwrap();
    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        2
    );
}

/// #1067: hundreds of branches and remote-tracking branches, some of them on the feature's
/// commits; a symbolic `origin/HEAD` and a tag of a tree keep nothing of their own.
#[test]
fn commits_only_on_branch_is_exact_with_many_branches() {
    let (_dir, repo) = init_repo();
    let main = line_of_commits(&repo, None, 300, 1_000);
    repo.reference("refs/heads/main", main[299], true, "test")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
    for (n, id) in main.iter().enumerate() {
        repo.reference(&format!("refs/heads/b{n}"), *id, true, "test")
            .unwrap();
        repo.reference(&format!("refs/remotes/origin/r{n}"), *id, true, "test")
            .unwrap();
    }
    let feature = line_of_commits(&repo, Some(main[10]), 8, 5_000);
    repo.reference("refs/heads/feature", feature[7], true, "test")
        .unwrap();
    repo.reference_symbolic(
        "refs/remotes/origin/HEAD",
        "refs/remotes/origin/r299",
        true,
        "test",
    )
    .unwrap();
    let tree = repo.find_commit(feature[7]).unwrap().tree().unwrap();
    repo.tag_lightweight("a-tree", tree.as_object(), false)
        .unwrap();
    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        8
    );

    // origin/feature has the first five.
    repo.reference("refs/remotes/origin/feature", feature[4], true, "test")
        .unwrap();
    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        3
    );
}

/// #1067: a branch that reaches the feature's commits through a commit dated before them (a
/// skewed clock) still keeps them, as `git rev-list --count` sees it.
#[test]
fn commits_only_on_branch_follows_a_commit_dated_before_its_parent() {
    let (_dir, repo) = init_repo();
    let base = commit_at(&repo, &[], "base", 1_000);
    let first = commit_at(&repo, &[base], "first", 2_000);
    let second = commit_at(&repo, &[first], "second", 3_000);
    // `other`'s tip is a child of `first`, dated before it.
    let skewed = commit_at(&repo, &[first], "skewed", 1_500);
    repo.reference("refs/heads/main", base, true, "test")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
    repo.reference("refs/heads/feature", second, true, "test")
        .unwrap();
    repo.reference("refs/heads/other", skewed, true, "test")
        .unwrap();

    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        1
    );
}

/// #1067: merges on both sides. `feature` merged `main` in; `main` has a merge of its own.
#[test]
fn commits_only_on_branch_counts_through_merges() {
    let (_dir, repo) = init_repo();
    let base = commit_at(&repo, &[], "base", 1_000);
    let side = commit_at(&repo, &[base], "side", 1_100);
    let main_tip = commit_at(&repo, &[base, side], "merge on main", 1_200);
    let f1 = commit_at(&repo, &[base], "f1", 1_050);
    let f2 = commit_at(&repo, &[f1, main_tip], "merge main into feature", 1_300);
    let f3 = commit_at(&repo, &[f2], "f3", 1_400);
    repo.reference("refs/heads/main", main_tip, true, "test")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
    repo.reference("refs/heads/feature", f3, true, "test")
        .unwrap();

    // f1, the merge and f3.
    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        3
    );
}

/// #1067: a branch whose commits are all dated before every commit of the other branches. The
/// walk takes the newest commits first, so it meets only hidden ones for a while and must not stop
/// before it reaches the branch's own.
#[test]
fn commits_only_on_branch_counts_a_branch_dated_before_the_others() {
    let (_dir, repo) = init_repo();
    let root = commit_at(&repo, &[], "root", 1);
    let main = line_of_commits(&repo, Some(root), 50, 1_000);
    let feature = line_of_commits(&repo, Some(root), 10, 100);
    repo.reference("refs/heads/main", main[49], true, "test")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
    repo.reference("refs/heads/feature", feature[9], true, "test")
        .unwrap();

    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        10
    );
}

/// #1067: a commit the branch reaches along two paths (a merge of two of its own lines) counts
/// once, and stays counted.
#[test]
fn commits_only_on_branch_counts_a_commit_reached_twice_once() {
    let (_dir, repo) = init_repo();
    let base = commit_at(&repo, &[], "base", 1_000);
    let fork = commit_at(&repo, &[base], "fork", 1_100);
    let left = commit_at(&repo, &[fork], "left", 1_200);
    let right = commit_at(&repo, &[fork], "right", 1_300);
    let merge = commit_at(&repo, &[left, right], "merge", 1_400);
    repo.reference("refs/heads/main", base, true, "test")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
    repo.reference("refs/heads/feature", merge, true, "test")
        .unwrap();

    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        4
    );
}

/// #1067: the walk stops a few commits past where only hidden ones are left, as git's does,
/// instead of walking the other branches' whole history. Seen through the object store: a commit
/// far below the fork point can be missing.
#[test]
fn commits_only_on_branch_stops_soon_after_only_hidden_commits_are_left() {
    let (dir, repo) = init_repo();
    let main = line_of_commits(&repo, None, 200, 1_000);
    let feature = line_of_commits(&repo, Some(main[150]), 2, 5_000);
    repo.reference("refs/heads/main", main[199], true, "test")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
    repo.reference("refs/heads/feature", feature[1], true, "test")
        .unwrap();
    let old = main[10].to_string();
    let loose = repo.path().join("objects").join(&old[..2]).join(&old[2..]);
    let mut permissions = std::fs::metadata(&loose).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    permissions.set_readonly(false);
    std::fs::set_permissions(&loose, permissions).unwrap();
    std::fs::remove_file(&loose).unwrap();
    // A fresh handle: the first one has the commit in its object cache.
    let repo = git2::Repository::open(dir.path()).unwrap();

    assert_eq!(
        git_core::branch::commits_only_on_branch(&repo, "feature").unwrap(),
        2
    );
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

#[test]
fn list_branches_reports_no_upstream_or_counts_without_upstream_config() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "content");
    commit_all(&repo, "initial commit");

    let branch = git_core::branch::list_branches(&repo).unwrap().remove(0);

    assert_eq!(branch.upstream, None);
    assert_eq!((branch.ahead, branch.behind), (None, None));
}

#[test]
fn list_branches_reports_the_upstream_but_no_counts_before_the_first_fetch() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "content");
    commit_all(&repo, "initial commit");
    let name = repo.head().unwrap().shorthand().unwrap().to_string();
    git_core::remote::add_remote(&repo, "origin", "file:///tmp/never-fetched.git", None).unwrap();
    git_core::remote::set_current_upstream(&repo, "origin", "trunk").unwrap();

    let branch = git_core::branch::list_branches(&repo)
        .unwrap()
        .into_iter()
        .find(|branch| branch.name == name)
        .unwrap();

    assert_eq!(branch.upstream.as_deref(), Some("origin/trunk"));
    assert_eq!((branch.ahead, branch.behind), (None, None));
}

#[test]
fn list_branches_counts_commits_ahead_and_behind_the_tracking_ref() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "1");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().target().unwrap();
    write_file(dir.path(), "file.txt", "remote");
    commit_all(&repo, "remote only");
    let remote_tip = repo.head().unwrap().target().unwrap();
    let head_ref = repo.head().unwrap().name().unwrap().to_string();
    let name = repo.head().unwrap().shorthand().unwrap().to_string();
    repo.reference(&head_ref, base, true, "rewind").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    write_file(dir.path(), "file.txt", "local 1");
    commit_all(&repo, "local 1");
    write_file(dir.path(), "file.txt", "local 2");
    commit_all(&repo, "local 2");
    git_core::remote::add_remote(&repo, "origin", "file:///tmp/origin.git", None).unwrap();
    repo.reference("refs/remotes/origin/main", remote_tip, true, "test")
        .unwrap();
    git_core::remote::set_current_upstream(&repo, "origin", "main").unwrap();

    let branch = git_core::branch::list_branches(&repo)
        .unwrap()
        .into_iter()
        .find(|branch| branch.name == name)
        .unwrap();

    assert_eq!(branch.upstream.as_deref(), Some("origin/main"));
    assert_eq!((branch.ahead, branch.behind), (Some(2), Some(1)));
}

/// Windows: a switch whose checkout stops at a file another program keeps open says so, not
/// libgit2's raw error, and leaves HEAD where it was; once the file is closed, switching again
/// finishes it (#303).
#[cfg(windows)]
#[test]
fn switching_with_a_file_open_in_another_program_says_so_and_finishes_when_tried_again() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "1\n");
    write_file(dir.path(), "z.txt", "1\n");
    commit_all(&repo, "base");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    git_core::branch::create_branch(&repo, "side", "HEAD").unwrap();
    write_file(dir.path(), "a.txt", "2\n");
    write_file(dir.path(), "z.txt", "2\n");
    commit_all(&repo, "side");
    git_core::branch::switch_branch(&repo, &main).unwrap();
    let editor = common::hold_open_without_share_delete(&dir.path().join("z.txt"));

    let error = git_core::branch::switch_branch(&repo, "side").unwrap_err();

    assert!(
        matches!(
            error,
            git_core::branch::BranchError::HeldOpen(ref held) if held.path == "z.txt"
        ),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "'z.txt' is open in another program. Some files were already switched. Close it, then \
         try again to finish."
    );
    assert_eq!(repo.head().unwrap().shorthand().ok(), Some(main.as_str()));
    assert_eq!(common::read(dir.path(), "z.txt"), "1\n");
    drop(editor);
    git_core::branch::switch_branch(&repo, "side").unwrap();
    assert_eq!(repo.head().unwrap().shorthand().ok(), Some("side"));
    assert_eq!(common::read(dir.path(), "a.txt"), "2\n");
    assert_eq!(common::read(dir.path(), "z.txt"), "2\n");
    assert!(repo.statuses(None).unwrap().is_empty());
}

/// Windows: as above, but the branch deletes the held file (present in HEAD, absent in the
/// target), which libgit2 removes before it writes anything (#303).
#[cfg(windows)]
#[test]
fn switching_to_a_branch_that_deletes_a_file_open_in_another_program_says_so() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "1\n");
    write_file(dir.path(), "z.txt", "1\n");
    commit_all(&repo, "base");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    git_core::branch::create_branch(&repo, "side", "HEAD").unwrap();
    write_file(dir.path(), "a.txt", "a longer line\n");
    let mut index = repo.index().unwrap();
    index.remove_path(std::path::Path::new("z.txt")).unwrap();
    index.write().unwrap();
    std::fs::remove_file(dir.path().join("z.txt")).unwrap();
    commit_all(&repo, "side");
    git_core::branch::switch_branch(&repo, &main).unwrap();
    let editor = common::hold_open_without_share_delete(&dir.path().join("z.txt"));

    let error = git_core::branch::switch_branch(&repo, "side").unwrap_err();

    assert!(
        matches!(
            error,
            git_core::branch::BranchError::HeldOpen(ref held) if held.path == "z.txt"
        ),
        "{error:?}"
    );
    assert_eq!(repo.head().unwrap().shorthand().ok(), Some(main.as_str()));
    assert!(dir.path().join("z.txt").exists());
    drop(editor);
    git_core::branch::switch_branch(&repo, "side").unwrap();
    assert_eq!(repo.head().unwrap().shorthand().ok(), Some("side"));
    assert!(!dir.path().join("z.txt").exists());
    assert_eq!(common::read(dir.path(), "a.txt"), "a longer line\n");
    assert!(repo.statuses(None).unwrap().is_empty());
}

/// Windows: what libgit2 itself reports for a file another program holds, when a checkout
/// writes over it and when it deletes it. `worktree_guard` finds the file by its quoted full
/// path; this pins that libgit2 quotes it (#303).
#[cfg(windows)]
#[test]
fn libgit2_quotes_the_full_path_of_a_held_file() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "1\n");
    write_file(dir.path(), "z.txt", "1\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    write_file(dir.path(), "z.txt", "a longer line\n");
    commit_all(&repo, "updated");
    let updated = repo.head().unwrap().peel_to_commit().unwrap();
    let mut index = repo.index().unwrap();
    index.remove_path(std::path::Path::new("z.txt")).unwrap();
    index.write().unwrap();
    std::fs::remove_file(dir.path().join("z.txt")).unwrap();
    commit_all(&repo, "deleted");
    let deleted = repo.head().unwrap().peel_to_commit().unwrap();
    let workdir = repo.workdir().unwrap().to_string_lossy().replace('\\', "/");
    let quoted = format!("'{}/z.txt'", workdir.trim_end_matches('/'));
    let mut seen = Vec::new();
    for target in [&updated, &deleted] {
        let mut force = git2::build::CheckoutBuilder::new();
        force.force();
        repo.checkout_tree(base.as_object(), Some(&mut force))
            .unwrap();
        repo.set_head_detached(base.id()).unwrap();
        let editor = common::hold_open_without_share_delete(&dir.path().join("z.txt"));
        let error = repo.checkout_tree(target.as_object(), None).unwrap_err();
        drop(editor);
        seen.push(format!(
            "{:?}/{:?}: {}",
            error.class(),
            error.code(),
            error.message()
        ));
        // windows-latest, both cases: "could not remove 'C:/Users/runneradmin/AppData/Local/
        // Temp/.tmpJhLclX/z.txt': The process cannot access the file because it is being used
        // by another process." (Os/GenericError); the reason is in the system's language.
        assert!(
            error.class() == git2::ErrorClass::Os
                && error
                    .message()
                    .replace('\\', "/")
                    .starts_with(&format!("could not remove {quoted}: ")),
            "{seen:?}"
        );
    }
}
