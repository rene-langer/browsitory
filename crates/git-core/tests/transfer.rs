mod common;

use common::{init_repo, write_file};
use git_core::{add_remote, push, push_tag};
use git2::Repository;
use tempfile::TempDir;

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
