mod common;

use common::{init_repo, write_file};
use git_core::{
    MergeOutcome, add_remote, create_commit, fetch, list_branches, pull, push, push_tag,
    stage_path, status,
};
use git2::{BranchType, Repository};
use tempfile::TempDir;

fn default_branch_name(repo: &git2::Repository) -> String {
    repo.branches(Some(BranchType::Local))
        .unwrap()
        .map(|b| b.unwrap().0)
        .find(|b| !matches!(b.name().unwrap(), Some("feature")))
        .unwrap()
        .name()
        .unwrap()
        .unwrap()
        .to_string()
}

/// Sets up a "remote" repo (just another real, non-bare temp repo — a git2
/// remote URL can point at any repo, bare or not, over the local `file`/
/// plain-path transport) with one commit on its default branch, and a
/// "local" repo with `origin` pointed at it, also with one commit (so both
/// share a common ancestor unless a test diverges them further).
fn setup_remote_and_local() -> (
    tempfile::TempDir,
    git2::Repository,
    tempfile::TempDir,
    git2::Repository,
    String,
) {
    let (remote_dir, mut remote_repo) = init_repo();
    write_file(&remote_dir, "a.txt", "one\n");
    stage_path(&remote_repo, "a.txt").unwrap();
    create_commit(&mut remote_repo, "base").unwrap();
    let branch = default_branch_name(&remote_repo);

    let (local_dir, mut local_repo) = init_repo();
    write_file(&local_dir, "a.txt", "one\n");
    stage_path(&local_repo, "a.txt").unwrap();
    create_commit(&mut local_repo, "base").unwrap();
    // Make sure the local repo's default branch has the same name as the
    // remote's, so `pull`'s remote-tracking-ref lookup (`refs/remotes/origin/{branch}`)
    // lines up with the branch actually being merged into.
    let local_default = default_branch_name(&local_repo);
    if local_default != branch {
        git_core::rename_branch(&local_repo, &local_default, &branch).unwrap();
    }

    add_remote(&local_repo, "origin", remote_dir.path().to_str().unwrap()).unwrap();

    (remote_dir, remote_repo, local_dir, local_repo, branch)
}

#[test]
fn fetch_updates_remote_tracking_ref_without_touching_local_branch() {
    let (remote_dir, mut remote_repo, _local_dir, local_repo, branch) = setup_remote_and_local();

    // Advance the remote past the shared base commit.
    write_file(&remote_dir, "a.txt", "one\ntwo\n");
    stage_path(&remote_repo, "a.txt").unwrap();
    let remote_tip = create_commit(&mut remote_repo, "advance remote").unwrap();

    let local_head_before = local_repo.head().unwrap().peel_to_commit().unwrap().id();

    let mut progress_calls = 0;
    fetch(&local_repo, "origin", |_update| progress_calls += 1).unwrap();

    let tracking_ref = local_repo
        .find_reference(&format!("refs/remotes/origin/{branch}"))
        .unwrap();
    assert_eq!(tracking_ref.target().unwrap(), remote_tip);

    // The local branch itself must be untouched by a plain fetch.
    assert_eq!(
        local_repo.head().unwrap().peel_to_commit().unwrap().id(),
        local_head_before
    );

    // A real transfer with at least one new object should report progress.
    assert!(progress_calls > 0);

    // Branch listing is unaffected by fetch (no new local branches created).
    assert_eq!(list_branches(&local_repo).unwrap().len(), 1);
}

#[test]
fn pull_fast_forwards_the_local_branch_when_there_is_no_divergence() {
    let (remote_dir, mut remote_repo, _local_dir, local_repo, branch) = setup_remote_and_local();

    write_file(&remote_dir, "a.txt", "one\ntwo\n");
    stage_path(&remote_repo, "a.txt").unwrap();
    let remote_tip = create_commit(&mut remote_repo, "advance remote").unwrap();

    let outcome = pull(&local_repo, "origin", &branch, |_update| {}).unwrap();

    assert_eq!(outcome, MergeOutcome::FastForward);
    assert_eq!(
        local_repo.head().unwrap().peel_to_commit().unwrap().id(),
        remote_tip
    );
    assert!(status(&local_repo).unwrap().is_empty());
}

#[test]
fn pull_on_diverging_histories_produces_a_conflict_outcome() {
    let (remote_dir, mut remote_repo, local_dir, mut local_repo, branch) = setup_remote_and_local();

    // Diverge: remote edits a.txt one way...
    write_file(&remote_dir, "a.txt", "one\nfrom remote\n");
    stage_path(&remote_repo, "a.txt").unwrap();
    create_commit(&mut remote_repo, "edit a.txt on remote").unwrap();

    // ...while the local branch edits the same line differently.
    write_file(&local_dir, "a.txt", "one\nfrom local\n");
    stage_path(&local_repo, "a.txt").unwrap();
    create_commit(&mut local_repo, "edit a.txt on local").unwrap();

    let outcome = pull(&local_repo, "origin", &branch, |_update| {}).unwrap();

    match outcome {
        MergeOutcome::Conflict(paths) => assert_eq!(paths, vec!["a.txt".to_string()]),
        other => panic!("expected Conflict, got {other:?}"),
    }

    let on_disk = std::fs::read_to_string(local_dir.path().join("a.txt")).unwrap();
    assert!(on_disk.contains("<<<<<<<"));
    assert!(on_disk.contains("======="));
    assert!(on_disk.contains(">>>>>>>"));
}

/// Creates a bare repo in a fresh temp dir to act as a fake remote — there's
/// no real network in integration tests, but a plain filesystem path works
/// fine as a git2 remote URL (no `file://` prefix needed), so a bare local
/// repo exercises the exact same `Remote::push()` code path a real remote
/// would.
fn init_bare_remote() -> (TempDir, Repository) {
    let dir = TempDir::new().expect("create temp dir");
    let repo = Repository::init_bare(dir.path()).expect("init bare repo");
    (dir, repo)
}

fn commit_file(dir: &TempDir, repo: &Repository, relative_path: &str, contents: &str) {
    write_file(dir, relative_path, contents);
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new(relative_path)).unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let sig = repo.signature().unwrap();
    let parents: Vec<git2::Commit> = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok())
        .into_iter()
        .collect();
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &format!("commit {relative_path}"),
        &tree,
        &parent_refs,
    )
    .unwrap();
}

#[test]
fn push_updates_the_bare_remote_ref() {
    let (dir, repo) = init_repo();
    commit_file(&dir, &repo, "a.txt", "one");
    let (bare_dir, bare_repo) = init_bare_remote();
    add_remote(&repo, "origin", bare_dir.path().to_str().unwrap()).unwrap();

    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    push(&repo, "origin", &[refspec], |_| {}).unwrap();

    let remote_ref = bare_repo
        .find_reference(&format!("refs/heads/{branch}"))
        .unwrap();
    let local_oid = repo.head().unwrap().target().unwrap();
    assert_eq!(remote_ref.target().unwrap(), local_oid);
}

#[test]
fn non_fast_forward_push_without_force_is_rejected() {
    let (dir, repo) = init_repo();
    commit_file(&dir, &repo, "a.txt", "one");
    let (bare_dir, bare_repo) = init_bare_remote();
    add_remote(&repo, "origin", bare_dir.path().to_str().unwrap()).unwrap();

    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    push(&repo, "origin", std::slice::from_ref(&refspec), |_| {}).unwrap();

    // Move the bare repo's ref forward independently of the local repo, by
    // committing directly against the bare repo's own index/tree — this
    // simulates someone else having pushed in the meantime, so the local
    // repo's next push would no longer be a fast-forward.
    let sig = git2::Signature::now("Other User", "other@example.com").unwrap();
    let parent = bare_repo
        .find_reference(&format!("refs/heads/{branch}"))
        .unwrap()
        .peel_to_commit()
        .unwrap();
    let tree = parent.tree().unwrap();
    bare_repo
        .commit(
            Some(&format!("refs/heads/{branch}")),
            &sig,
            &sig,
            "someone else's commit",
            &tree,
            &[&parent],
        )
        .unwrap();

    // Local repo hasn't moved, so pushing again (non-force) should be
    // rejected as a non-fast-forward update.
    let result = push(&repo, "origin", &[refspec], |_| {});
    assert!(result.is_err());
}

#[test]
fn force_push_overwrites_the_remote_ref() {
    let (dir, repo) = init_repo();
    commit_file(&dir, &repo, "a.txt", "one");
    let (bare_dir, bare_repo) = init_bare_remote();
    add_remote(&repo, "origin", bare_dir.path().to_str().unwrap()).unwrap();

    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
    push(&repo, "origin", &[refspec], |_| {}).unwrap();

    // Diverge the bare repo's ref, same as the rejection test above.
    let sig = git2::Signature::now("Other User", "other@example.com").unwrap();
    let parent = bare_repo
        .find_reference(&format!("refs/heads/{branch}"))
        .unwrap()
        .peel_to_commit()
        .unwrap();
    let tree = parent.tree().unwrap();
    bare_repo
        .commit(
            Some(&format!("refs/heads/{branch}")),
            &sig,
            &sig,
            "someone else's commit",
            &tree,
            &[&parent],
        )
        .unwrap();

    let force_refspec = format!("+refs/heads/{branch}:refs/heads/{branch}");
    push(&repo, "origin", &[force_refspec], |_| {}).unwrap();

    let remote_ref = bare_repo
        .find_reference(&format!("refs/heads/{branch}"))
        .unwrap();
    let local_oid = repo.head().unwrap().target().unwrap();
    assert_eq!(remote_ref.target().unwrap(), local_oid);
}

#[test]
fn push_tag_creates_the_tag_ref_on_the_remote() {
    let (dir, repo) = init_repo();
    commit_file(&dir, &repo, "a.txt", "one");
    let (bare_dir, bare_repo) = init_bare_remote();
    add_remote(&repo, "origin", bare_dir.path().to_str().unwrap()).unwrap();

    let head_oid = repo.head().unwrap().target().unwrap();
    let commit = repo.find_commit(head_oid).unwrap();
    let sig = repo.signature().unwrap();
    repo.tag("v1.0.0", commit.as_object(), &sig, "release v1.0.0", false)
        .unwrap();

    push_tag(&repo, "origin", "v1.0.0", |_| {}).unwrap();

    let remote_tag = bare_repo.find_reference("refs/tags/v1.0.0").unwrap();
    assert!(remote_tag.target().is_some());
}
