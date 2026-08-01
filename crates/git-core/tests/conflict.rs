mod common;

use common::{init_repo, write_file};
use git_core::{conflicted_paths, create_commit, read_conflict, stage_path};
use git2::{BranchType, Repository};

/// Creates two branches that both edit the same line of the same file
/// starting from a shared commit, then merges the second into the first so
/// the repo is left in a real conflicted-merge state (MERGE_HEAD set,
/// conflicted index entries) — the fixture every test in this file starts
/// from.
fn conflicting_merge_fixture() -> (tempfile::TempDir, Repository) {
    let (dir, mut repo) = init_repo();
    write_file(&dir, "a.txt", "line one\nline two\nline three\n");
    stage_path(&repo, "a.txt").unwrap();
    create_commit(&mut repo, "base").unwrap();

    repo.branch(
        "theirs",
        &repo.head().unwrap().peel_to_commit().unwrap(),
        false,
    )
    .unwrap();

    write_file(&dir, "a.txt", "line one\nOURS\nline three\n");
    stage_path(&repo, "a.txt").unwrap();
    create_commit(&mut repo, "ours edit").unwrap();

    repo.set_head("refs/heads/theirs").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    write_file(&dir, "a.txt", "line one\nTHEIRS\nline three\n");
    stage_path(&repo, "a.txt").unwrap();
    create_commit(&mut repo, "theirs edit").unwrap();

    // Switch back to the default branch (master/main) before merging
    // "theirs" into it.
    let default_ref = format!("refs/heads/{}", default_branch_name(&repo));
    repo.set_head(&default_ref).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();

    let outcome = git_core::merge_branch(&repo, "theirs").unwrap();
    assert!(matches!(outcome, git_core::MergeOutcome::Conflict(_)));

    (dir, repo)
}

#[test]
fn conflicted_paths_lists_the_one_conflicting_file() {
    let (_dir, repo) = conflicting_merge_fixture();

    let paths = conflicted_paths(&repo).unwrap();

    assert_eq!(paths, vec!["a.txt".to_string()]);
}

#[test]
fn read_conflict_surfaces_ancestor_ours_and_theirs_blobs() {
    let (_dir, repo) = conflicting_merge_fixture();

    let sides = read_conflict(&repo, "a.txt").unwrap();

    assert_eq!(sides.path, "a.txt");
    assert_eq!(
        String::from_utf8(sides.ancestor.unwrap()).unwrap(),
        "line one\nline two\nline three\n"
    );
    assert_eq!(
        String::from_utf8(sides.ours.unwrap()).unwrap(),
        "line one\nOURS\nline three\n"
    );
    assert_eq!(
        String::from_utf8(sides.theirs.unwrap()).unwrap(),
        "line one\nTHEIRS\nline three\n"
    );
}

#[test]
fn read_conflict_on_added_by_us_has_no_ancestor_or_theirs() {
    let (dir, mut repo) = init_repo();
    write_file(&dir, "base.txt", "base\n");
    stage_path(&repo, "base.txt").unwrap();
    create_commit(&mut repo, "base").unwrap();

    repo.branch(
        "theirs",
        &repo.head().unwrap().peel_to_commit().unwrap(),
        false,
    )
    .unwrap();

    // "ours" adds a brand new file with no common ancestor version.
    write_file(&dir, "new.txt", "added by us\n");
    stage_path(&repo, "new.txt").unwrap();
    create_commit(&mut repo, "add new.txt on ours").unwrap();

    repo.set_head("refs/heads/theirs").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    // "theirs" adds a *different* file at the same path — a genuine
    // add/add conflict with no shared ancestor for that path.
    write_file(&dir, "new.txt", "added by them\n");
    stage_path(&repo, "new.txt").unwrap();
    create_commit(&mut repo, "add new.txt on theirs").unwrap();

    let default_branch = repo
        .branches(Some(BranchType::Local))
        .unwrap()
        .map(|b| b.unwrap().0)
        .find(|b| b.name().unwrap() != Some("theirs"))
        .unwrap();
    let default_ref = format!("refs/heads/{}", default_branch.name().unwrap().unwrap());
    repo.set_head(&default_ref).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();

    let outcome = git_core::merge_branch(&repo, "theirs").unwrap();
    assert!(matches!(outcome, git_core::MergeOutcome::Conflict(_)));

    let sides = read_conflict(&repo, "new.txt").unwrap();
    assert!(sides.ancestor.is_none());
    assert_eq!(
        String::from_utf8(sides.ours.unwrap()).unwrap(),
        "added by us\n"
    );
    assert_eq!(
        String::from_utf8(sides.theirs.unwrap()).unwrap(),
        "added by them\n"
    );
}

fn default_branch_name(repo: &Repository) -> String {
    repo.branches(Some(BranchType::Local))
        .unwrap()
        .map(|b| b.unwrap().0)
        .find(|b| b.name().unwrap() != Some("theirs"))
        .unwrap()
        .name()
        .unwrap()
        .unwrap()
        .to_string()
}
