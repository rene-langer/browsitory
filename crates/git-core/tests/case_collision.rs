//! #156: operations that would bring in a symbolic link and a folder whose names differ only in
//! case are refused when case matters (`core.ignorecase`, or always on macOS/Windows). The pair
//! is built with `TreeBuilder`, so no file-system link is needed and the tests run everywhere.

mod common;

use common::{commit_all, init_repo, write_file};
use git2::{Oid, Repository, Signature};
use tempfile::TempDir;

use git_core::branch::{self, BranchError};
use git_core::merge::{self, FastForward, MergeError, MergeOptions};
use git_core::pick::{self, PickError};
use git_core::rebase::{self, RebaseAction, RebaseError, RebasePlanEntry};
use git_core::remote::{self, RemoteError};
use git_core::reset::{self, ResetError, ResetMode};
use git_core::stage;
use git_core::stash::{self, StashError};
use git_core::CaseCollision;

const MESSAGE: &str = "'D' is a symbolic link and 'd' a folder whose names differ only in case. \
On this file system they are the same name, so Browsitory won't check them out. Rename one of \
them in the repository.";

fn sig() -> Signature<'static> {
    Signature::now("T", "t@example.com").unwrap()
}

/// `parent`'s tree plus the link `D -> /x` and the file `d/x` (= `x`).
fn with_pair(repo: &Repository, parent: Oid, x: &str) -> Oid {
    let base = repo.find_commit(parent).unwrap().tree().unwrap();
    let mut inner = repo.treebuilder(None).unwrap();
    inner
        .insert("x", repo.blob(x.as_bytes()).unwrap(), 0o100644)
        .unwrap();
    let inner = inner.write().unwrap();
    let mut outer = repo.treebuilder(Some(&base)).unwrap();
    outer
        .insert("D", repo.blob(b"/x").unwrap(), 0o120000)
        .unwrap();
    outer.insert("d", inner, 0o040000).unwrap();
    outer.write().unwrap()
}

fn commit_tree(repo: &Repository, tree: Oid, parents: &[Oid], message: &str) -> Oid {
    let tree = repo.find_tree(tree).unwrap();
    let parents: Vec<_> = parents
        .iter()
        .map(|id| repo.find_commit(*id).unwrap())
        .collect();
    let parents: Vec<_> = parents.iter().collect();
    repo.commit(None, &sig(), &sig(), message, &tree, &parents)
        .unwrap()
}

struct Fixture {
    dir: TempDir,
    repo: Repository,
    base: Oid,
    evil: Oid,
}

/// `main` = `a.txt`; branch `evil` = main + the pair; `core.ignorecase=true`.
fn fixture() -> Fixture {
    let (dir, repo) = init_repo();
    repo.set_head("refs/heads/main").unwrap();
    write_file(dir.path(), "a.txt", "base\n");
    commit_all(&repo, "base");
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    let base = repo.head().unwrap().target().unwrap();
    let evil = commit_tree(&repo, with_pair(&repo, base, "PWNED\n"), &[base], "evil");
    repo.reference("refs/heads/evil", evil, true, "").unwrap();
    Fixture {
        dir,
        repo,
        base,
        evil,
    }
}

/// Adds `b.txt` on `main` (a commit `evil` doesn't have).
fn diverge(fx: &Fixture) -> Oid {
    write_file(fx.dir.path(), "b.txt", "b\n");
    commit_all(&fx.repo, "b");
    fx.repo.head().unwrap().target().unwrap()
}

fn assert_untouched(fx: &Fixture, head: Oid) {
    assert_eq!(fx.repo.head().unwrap().target().unwrap(), head);
    assert_eq!(fx.repo.head().unwrap().name().ok(), Some("refs/heads/main"));
    assert!(std::fs::symlink_metadata(fx.dir.path().join("d")).is_err());
    assert!(std::fs::symlink_metadata(fx.dir.path().join("D")).is_err());
}

fn expected() -> CaseCollision {
    CaseCollision {
        link: "D".into(),
        folder: "d".into(),
    }
}

#[test]
fn switch_to_a_branch_with_a_case_collision_is_refused() {
    let fx = fixture();
    let error = branch::switch_branch(&fx.repo, "evil").unwrap_err();
    assert!(matches!(&error, BranchError::CaseCollision(c) if *c == expected()));
    assert_eq!(error.to_string(), MESSAGE);
    assert_untouched(&fx, fx.base);
}

#[test]
fn checkout_commit_with_a_case_collision_is_refused() {
    let fx = fixture();
    let error = branch::checkout_commit(&fx.repo, &fx.evil.to_string()).unwrap_err();
    assert!(matches!(error, BranchError::CaseCollision(_)));
    assert_untouched(&fx, fx.base);
}

#[test]
fn merge_fast_forward_with_a_case_collision_is_refused() {
    let fx = fixture();
    let error = merge::start_merge(&fx.repo, "evil", MergeOptions::default()).unwrap_err();
    assert!(matches!(error, MergeError::CaseCollision(_)));
    assert_eq!(error.to_string(), MESSAGE);
    assert_untouched(&fx, fx.base);
}

#[test]
fn merge_commit_with_a_case_collision_is_refused() {
    let fx = fixture();
    let head = diverge(&fx);
    let error = merge::start_merge(
        &fx.repo,
        "evil",
        MergeOptions {
            fast_forward: FastForward::Allow,
            squash: false,
        },
    )
    .unwrap_err();
    assert!(matches!(error, MergeError::CaseCollision(_)));
    assert_untouched(&fx, head);
    assert!(!merge::is_merging(&fx.repo));
}

#[test]
fn cherry_pick_of_a_commit_adding_the_collision_is_refused() {
    let fx = fixture();
    let head = diverge(&fx);
    let error = pick::cherry_pick(&fx.repo, &fx.evil.to_string(), None).unwrap_err();
    assert!(matches!(error, PickError::CaseCollision(_)));
    assert_untouched(&fx, head);
}

#[test]
fn rebase_onto_a_case_collision_is_refused() {
    let fx = fixture();
    let head = diverge(&fx);
    let plan = vec![RebasePlanEntry {
        commit_id: head.to_string(),
        action: RebaseAction::Pick,
        combined_message: None,
    }];
    let Err(error) = rebase::start_rebase(&fx.repo, "evil", plan) else {
        panic!("rebase started");
    };
    assert!(matches!(error, RebaseError::CaseCollision(_)));
    assert_untouched(&fx, head);
}

#[test]
fn stash_apply_bringing_in_a_case_collision_is_refused() {
    let mut fx = fixture();
    let base_tree = fx.repo.find_commit(fx.base).unwrap().tree_id();
    let index_commit = commit_tree(&fx.repo, base_tree, &[fx.base], "index on main");
    let stash_tree = with_pair(&fx.repo, fx.base, "stashed\n");
    let stash = commit_tree(
        &fx.repo,
        stash_tree,
        &[fx.base, index_commit],
        "WIP on main",
    );
    fx.repo.reference_ensure_log("refs/stash").unwrap();
    fx.repo
        .reference("refs/stash", stash, true, "WIP on main")
        .unwrap();
    let error = stash::apply_stash(&mut fx.repo, 0, None, false).unwrap_err();
    assert!(matches!(error, StashError::CaseCollision(_)));
    assert_untouched(&fx, fx.base);
}

#[test]
fn pull_fast_forward_to_a_case_collision_is_refused() {
    let fx = fixture();
    fx.repo
        .reference("refs/remotes/origin/main", fx.evil, true, "")
        .unwrap();
    let error = remote::pull_after_fetch(&fx.repo, "origin", "main").unwrap_err();
    assert!(matches!(error, RemoteError::CaseCollision(_)));
    assert_eq!(error.to_string(), MESSAGE);
    assert_untouched(&fx, fx.base);
}

#[test]
fn reset_hard_to_a_collision_commit_is_refused() {
    let fx = fixture();
    let error = reset::reset_to(&fx.repo, &fx.evil.to_string(), ResetMode::Hard).unwrap_err();
    assert!(matches!(error, ResetError::CaseCollision(_)));
    assert_untouched(&fx, fx.base);
}

#[test]
fn discarding_d_x_alone_is_allowed() {
    let fx = fixture();
    let evil_tree = fx.repo.find_commit(fx.evil).unwrap().tree().unwrap();
    let mut index = fx.repo.index().unwrap();
    index.read_tree(&evil_tree).unwrap();
    index.write().unwrap();
    write_file(fx.dir.path(), "d/x", "edited\n");

    stage::discard_file(&fx.repo, "d/x").unwrap();
    assert_eq!(
        std::fs::read_to_string(fx.dir.path().join("d/x")).unwrap(),
        "PWNED\n"
    );
}

#[test]
#[cfg(target_os = "linux")]
fn a_repo_whose_head_has_a_collision_still_switches_to_a_clean_branch() {
    let fx = fixture();
    let config = || fx.repo.config().unwrap();
    // Linux (case-sensitive) writes both `D` and `d/x`.
    config().set_bool("core.ignorecase", false).unwrap();
    branch::switch_branch(&fx.repo, "evil").unwrap();
    config().set_bool("core.ignorecase", true).unwrap();

    let evil_tree = fx.repo.find_commit(fx.evil).unwrap().tree().unwrap();
    let mut clean = fx.repo.treebuilder(Some(&evil_tree)).unwrap();
    clean.remove("D").unwrap();
    let clean = commit_tree(&fx.repo, clean.write().unwrap(), &[fx.evil], "clean");
    fx.repo
        .reference("refs/heads/clean", clean, true, "")
        .unwrap();
    let evil2 = commit_tree(
        &fx.repo,
        with_pair(&fx.repo, fx.evil, "PWNED2\n"),
        &[fx.evil],
        "evil2",
    );
    fx.repo
        .reference("refs/heads/evil2", evil2, true, "")
        .unwrap();

    branch::switch_branch(&fx.repo, "clean").unwrap();
    assert!(fx.dir.path().join("d/x").is_file());
    assert!(matches!(
        branch::switch_branch(&fx.repo, "evil2"),
        Err(BranchError::CaseCollision(_))
    ));
}

#[test]
#[cfg(target_os = "linux")]
fn without_ignorecase_linux_does_not_refuse() {
    let fx = fixture();
    fx.repo
        .config()
        .unwrap()
        .set_bool("core.ignorecase", false)
        .unwrap();
    branch::switch_branch(&fx.repo, "evil").unwrap();
    assert!(fx.dir.path().join("d/x").is_file());
}

#[test]
fn undo_restoring_a_case_collision_is_refused_before_writing() {
    use git_core::undo::{self, SnapshotScope, UndoError};
    let fx = fixture();
    let now = undo::capture(&fx.repo, SnapshotScope::Full).unwrap();
    let evil_tree = fx.repo.find_commit(fx.evil).unwrap().tree_id().to_string();
    let mut target = now.clone();
    target
        .refs
        .insert("refs/heads/main".into(), fx.evil.to_string());
    target.index_tree = evil_tree.clone();
    target.worktree_tree = Some(evil_tree);
    let error = undo::restore(&fx.repo, &now, &target, "undo").unwrap_err();
    assert!(matches!(error, UndoError::CaseCollision(_)));
    assert_untouched(&fx, fx.base);
}

#[test]
#[cfg(target_os = "linux")]
fn a_head_with_a_collision_can_still_be_reset_and_discarded() {
    let fx = fixture();
    fx.repo
        .config()
        .unwrap()
        .set_bool("core.ignorecase", false)
        .unwrap();
    branch::switch_branch(&fx.repo, "evil").unwrap();
    fx.repo
        .config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    write_file(fx.dir.path(), "a.txt", "edited\n");
    reset::reset_to(&fx.repo, &fx.evil.to_string(), ResetMode::Hard).unwrap();
    assert_eq!(
        std::fs::read_to_string(fx.dir.path().join("a.txt")).unwrap(),
        "base\n"
    );
    write_file(fx.dir.path(), "d/x", "edited\n");
    stage::discard_all_unstaged(&fx.repo, &["d/x".to_string(), "D".to_string()]).unwrap();
    assert_eq!(
        std::fs::read_to_string(fx.dir.path().join("d/x")).unwrap(),
        "PWNED\n"
    );
}
