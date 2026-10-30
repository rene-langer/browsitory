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

#[test]
#[cfg(target_os = "linux")]
fn a_target_adding_a_second_collision_is_refused_even_when_head_holds_one() {
    let fx = fixture();
    let config = || fx.repo.config().unwrap();
    config().set_bool("core.ignorecase", false).unwrap();
    branch::switch_branch(&fx.repo, "evil").unwrap();
    config().set_bool("core.ignorecase", true).unwrap();

    // `evil` (D + d/x) plus a second pair Z + z/x.
    let evil_tree = fx.repo.find_commit(fx.evil).unwrap().tree().unwrap();
    let mut inner = fx.repo.treebuilder(None).unwrap();
    inner
        .insert("x", fx.repo.blob(b"PWNED\n").unwrap(), 0o100644)
        .unwrap();
    let mut outer = fx.repo.treebuilder(Some(&evil_tree)).unwrap();
    outer.insert("z", inner.write().unwrap(), 0o040000).unwrap();
    outer
        .insert("Z", fx.repo.blob(b"/x").unwrap(), 0o120000)
        .unwrap();
    let both = commit_tree(&fx.repo, outer.write().unwrap(), &[fx.evil], "both");

    let error = reset::reset_to(&fx.repo, &both.to_string(), ResetMode::Hard).unwrap_err();
    match error {
        ResetError::CaseCollision(collision) => assert_eq!(
            collision,
            CaseCollision {
                link: "Z".into(),
                folder: "z".into()
            }
        ),
        other => panic!("{other:?}"),
    }
    assert_eq!(fx.repo.head().unwrap().target(), Some(fx.evil));
    assert!(std::fs::symlink_metadata(fx.dir.path().join("z")).is_err());
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// `parent`'s tree plus the file `name` (= `content`), as a commit (no ref moved).
fn with_file(repo: &Repository, parent: Oid, name: &str, content: &str) -> Oid {
    let base = repo.find_commit(parent).unwrap().tree().unwrap();
    let mut tree = repo.treebuilder(Some(&base)).unwrap();
    tree.insert(name, repo.blob(content.as_bytes()).unwrap(), 0o100644)
        .unwrap();
    commit_tree(repo, tree.write().unwrap(), &[parent], name)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// `parent`'s tree plus a second pair, the link `Z -> /x` and the file `z/x`.
fn with_second_pair(repo: &Repository, parent: Oid) -> Oid {
    let base = repo.find_commit(parent).unwrap().tree().unwrap();
    let mut inner = repo.treebuilder(None).unwrap();
    inner
        .insert("x", repo.blob(b"PWNED\n").unwrap(), 0o100644)
        .unwrap();
    let mut outer = repo.treebuilder(Some(&base)).unwrap();
    outer.insert("z", inner.write().unwrap(), 0o040000).unwrap();
    outer
        .insert("Z", repo.blob(b"/x").unwrap(), 0o120000)
        .unwrap();
    commit_tree(repo, outer.write().unwrap(), &[parent], "second pair")
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// Points `branch` at `tip` and switches to it with case folding off (Linux writes both `D` and
/// `d/x`), then turns folding back on: HEAD holds the pair.
fn head_with_pair_at(fx: &Fixture, branch: &str, tip: Oid) {
    // Detach first, so moving the current branch doesn't leave the switch a no-op.
    fx.repo.set_head_detached(head(fx)).unwrap();
    fx.repo
        .reference(&format!("refs/heads/{branch}"), tip, true, "")
        .unwrap();
    let config = || fx.repo.config().unwrap();
    config().set_bool("core.ignorecase", false).unwrap();
    branch::switch_branch(&fx.repo, branch).unwrap();
    config().set_bool("core.ignorecase", true).unwrap();
    assert!(fx.dir.path().join("d/x").is_file());
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// HEAD on `evil`, holding the pair.
fn head_with_pair(fx: &Fixture) -> Oid {
    head_with_pair_at(fx, "evil", fx.evil);
    fx.evil
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn read(fx: &Fixture, rel: &str) -> String {
    std::fs::read_to_string(fx.dir.path().join(rel)).unwrap()
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn head(fx: &Fixture) -> Oid {
    fx.repo.head().unwrap().target().unwrap()
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// HEAD on `work` = evil + `c.txt`; branch `side` = evil + `b.txt`. Returns `side`.
fn diverged_on_pair(fx: &Fixture) -> Oid {
    let side = with_file(&fx.repo, fx.evil, "b.txt", "b\n");
    fx.repo
        .reference("refs/heads/side", side, true, "")
        .unwrap();
    let work = with_file(&fx.repo, fx.evil, "c.txt", "c\n");
    head_with_pair_at(fx, "work", work);
    side
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const NON_FF: MergeOptions = MergeOptions {
    fast_forward: FastForward::Allow,
    squash: false,
};

#[test]
#[cfg(target_os = "linux")]
fn a_head_with_a_collision_merges_an_unrelated_branch() {
    let fx = fixture();
    diverged_on_pair(&fx);
    let outcome = merge::start_merge(&fx.repo, "side", NON_FF).unwrap();
    assert_eq!(outcome, merge::MergeOutcome::Merged);
    assert_eq!(read(&fx, "b.txt"), "b\n");
    assert_eq!(read(&fx, "d/x"), "PWNED\n");
}

#[test]
#[cfg(target_os = "linux")]
fn a_head_with_a_collision_fast_forwards_to_an_unrelated_descendant() {
    let fx = fixture();
    head_with_pair(&fx);
    let side = with_file(&fx.repo, fx.evil, "b.txt", "b\n");
    fx.repo
        .reference("refs/heads/side", side, true, "")
        .unwrap();
    let outcome = merge::start_merge(&fx.repo, "side", MergeOptions::default()).unwrap();
    assert_eq!(outcome, merge::MergeOutcome::FastForwarded);
    assert_eq!(head(&fx), side);
    assert_eq!(read(&fx, "b.txt"), "b\n");
}

#[test]
#[cfg(target_os = "linux")]
fn a_head_with_a_collision_cherry_picks_an_unrelated_commit() {
    let fx = fixture();
    let side = diverged_on_pair(&fx);
    pick::cherry_pick(&fx.repo, &side.to_string(), None).unwrap();
    assert_eq!(read(&fx, "b.txt"), "b\n");
    assert_eq!(read(&fx, "c.txt"), "c\n");
}

#[test]
#[cfg(target_os = "linux")]
fn a_head_with_a_collision_reverts_an_unrelated_commit() {
    let fx = fixture();
    diverged_on_pair(&fx);
    let work = head(&fx);
    pick::revert(&fx.repo, &work.to_string(), None).unwrap();
    assert!(!fx.dir.path().join("c.txt").exists());
    assert_eq!(read(&fx, "d/x"), "PWNED\n");
}

#[test]
#[cfg(target_os = "linux")]
fn a_head_with_a_collision_applies_an_unrelated_stash() {
    let mut fx = fixture();
    head_with_pair(&fx);
    write_file(fx.dir.path(), "a.txt", "edited\n");
    // libgit2's stash diff fails on Linux with both `D` and `d/` on disk under a simulated
    // `core.ignorecase` ("merge diffs created with conflicting options"); save without it. The
    // apply, which is what the guard checks, runs folded.
    let fold = |repo: &Repository, on| repo.config().unwrap().set_bool("core.ignorecase", on);
    fold(&fx.repo, false).unwrap();
    stash::save_stash(&mut fx.repo, &stash::StashOptions::default()).unwrap();
    fold(&fx.repo, true).unwrap();
    assert_eq!(read(&fx, "a.txt"), "base\n");
    stash::apply_stash(&mut fx.repo, 0, None, false).unwrap();
    assert_eq!(read(&fx, "a.txt"), "edited\n");
}

#[test]
#[cfg(target_os = "linux")]
fn a_head_with_a_collision_pulls_fast_forward() {
    let fx = fixture();
    head_with_pair(&fx);
    let side = with_file(&fx.repo, fx.evil, "b.txt", "b\n");
    fx.repo
        .reference("refs/remotes/origin/evil", side, true, "")
        .unwrap();
    remote::pull_after_fetch(&fx.repo, "origin", "evil").unwrap();
    assert_eq!(head(&fx), side);
    assert_eq!(read(&fx, "b.txt"), "b\n");
}

#[test]
#[cfg(target_os = "linux")]
fn a_head_with_a_collision_rebases_onto_an_unrelated_commit() {
    let fx = fixture();
    let side = diverged_on_pair(&fx);
    let work = head(&fx);
    let plan = vec![RebasePlanEntry {
        commit_id: work.to_string(),
        action: RebaseAction::Pick,
        combined_message: None,
    }];
    let (_, result) = rebase::start_rebase(&fx.repo, &side.to_string(), plan).unwrap();
    assert_eq!(result, rebase::RebaseStepResult::Done);
    assert_eq!(fx.repo.head().unwrap().name().ok(), Some("refs/heads/work"));
    assert_eq!(read(&fx, "b.txt"), "b\n");
    assert_eq!(read(&fx, "c.txt"), "c\n");
    assert_eq!(read(&fx, "d/x"), "PWNED\n");
}

#[test]
#[cfg(target_os = "linux")]
fn switching_between_branches_holding_the_same_collision_is_allowed() {
    let fx = fixture();
    head_with_pair(&fx);
    let evil2 = commit_tree(
        &fx.repo,
        with_pair(&fx.repo, fx.evil, "PWNED2\n"),
        &[fx.evil],
        "evil2",
    );
    fx.repo
        .reference("refs/heads/evil2", evil2, true, "")
        .unwrap();
    branch::switch_branch(&fx.repo, "evil2").unwrap();
    assert_eq!(read(&fx, "d/x"), "PWNED2\n");
}

#[test]
#[cfg(target_os = "linux")]
fn a_merge_bringing_a_second_collision_is_refused_even_when_head_holds_one() {
    let fx = fixture();
    diverged_on_pair(&fx);
    let work = head(&fx);
    let second = with_second_pair(&fx.repo, fx.evil);
    fx.repo
        .reference("refs/heads/second", second, true, "")
        .unwrap();
    let error = merge::start_merge(&fx.repo, "second", NON_FF).unwrap_err();
    match error {
        MergeError::CaseCollision(collision) => assert_eq!(
            collision,
            CaseCollision {
                link: "Z".into(),
                folder: "z".into()
            }
        ),
        other => panic!("{other:?}"),
    }
    assert_eq!(head(&fx), work);
    assert!(std::fs::symlink_metadata(fx.dir.path().join("z")).is_err());
    assert!(std::fs::symlink_metadata(fx.dir.path().join("Z")).is_err());
    assert!(!merge::is_merging(&fx.repo));
}

#[test]
#[cfg(target_os = "linux")]
fn a_cherry_pick_bringing_a_second_collision_is_refused_even_when_head_holds_one() {
    let fx = fixture();
    diverged_on_pair(&fx);
    let work = head(&fx);
    let second = with_second_pair(&fx.repo, fx.evil);
    let error = pick::cherry_pick(&fx.repo, &second.to_string(), None).unwrap_err();
    match error {
        PickError::CaseCollision(collision) => assert_eq!(collision.link, "Z"),
        other => panic!("{other:?}"),
    }
    assert_eq!(head(&fx), work);
    assert!(std::fs::symlink_metadata(fx.dir.path().join("z")).is_err());
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// `parent`'s tree without the link `D` (the pair is gone).
fn without_link(repo: &Repository, parent: Oid) -> Oid {
    let base = repo.find_commit(parent).unwrap().tree().unwrap();
    let mut tree = repo.treebuilder(Some(&base)).unwrap();
    tree.remove("D").unwrap();
    commit_tree(repo, tree.write().unwrap(), &[parent], "rm D")
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn assert_pair_back(fx: &Fixture) {
    assert!(std::fs::symlink_metadata(fx.dir.path().join("D"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(read(fx, "d/x"), "PWNED\n");
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// `main` = evil → `e` (edits `a.txt`), checked out holding the pair; returns `(e, rm)`, where
/// `rm` = evil without `D`.
fn rebase_fixture(fx: &Fixture) -> (Oid, Oid) {
    let e = with_file(&fx.repo, fx.evil, "a.txt", "e\n");
    head_with_pair_at(fx, "main", e);
    let rm = without_link(&fx.repo, fx.evil);
    (e, rm)
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn plan_of(id: Oid, action: RebaseAction) -> Vec<RebasePlanEntry> {
    vec![RebasePlanEntry {
        commit_id: id.to_string(),
        action,
        combined_message: None,
    }]
}

#[test]
#[cfg(target_os = "linux")]
fn rebase_abort_restores_an_original_tree_holding_a_collision() {
    let fx = fixture();
    let (e, rm) = rebase_fixture(&fx);
    let (state, result) =
        rebase::start_rebase(&fx.repo, &rm.to_string(), plan_of(e, RebaseAction::Edit)).unwrap();
    assert_eq!(result, rebase::RebaseStepResult::PausedForEdit);
    assert!(fx.repo.head_detached().unwrap());
    assert!(std::fs::symlink_metadata(fx.dir.path().join("D")).is_err());

    let abort = rebase::abort_rebase(&fx.repo, &state).unwrap();
    assert!(abort.restored);
    assert_eq!(fx.repo.head().unwrap().name().ok(), Some("refs/heads/main"));
    assert_eq!(head(&fx), e);
    assert_pair_back(&fx);
}

#[test]
// Linux only, like the other HEAD-holds-the-pair tests: a case-insensitive file system can't
// hold both `D` and `d/` (gpg `/bin/false` also needs unix).
#[cfg(target_os = "linux")]
fn a_failed_rebase_start_rolls_back_to_an_original_tree_holding_a_collision() {
    let fx = fixture();
    let (e, rm) = rebase_fixture(&fx);
    let mut config = fx.repo.config().unwrap();
    config.set_bool("commit.gpgSign", true).unwrap();
    config.set_str("gpg.program", "/bin/false").unwrap();
    let Err(error) =
        rebase::start_rebase(&fx.repo, &rm.to_string(), plan_of(e, RebaseAction::Pick))
    else {
        panic!("rebase started");
    };
    assert!(!matches!(error, RebaseError::CaseCollision(_)), "{error:?}");
    assert!(!fx.repo.head_detached().unwrap());
    assert_eq!(fx.repo.head().unwrap().name().ok(), Some("refs/heads/main"));
    assert_eq!(head(&fx), e);
    assert_pair_back(&fx);
}

#[test]
#[cfg(target_os = "linux")]
fn sequence_abort_rewinds_to_an_original_tree_holding_a_collision() {
    let fx = fixture();
    let local = with_file(&fx.repo, fx.evil, "a.txt", "local\n");
    head_with_pair_at(&fx, "main", local);
    let rm = without_link(&fx.repo, fx.evil);
    let conflicting = with_file(&fx.repo, fx.evil, "a.txt", "theirs\n");

    let run = pick::cherry_pick_many(&fx.repo, &[rm.to_string(), conflicting.to_string()]);
    assert!(
        matches!(run.outcome, Ok(pick::SequenceOutcome::Conflicted { .. })),
        "{:?}",
        run.outcome
    );
    assert!(std::fs::symlink_metadata(fx.dir.path().join("D")).is_err());
    let state = run.state.unwrap();

    let abort = pick::abort_sequence(&fx.repo, &state).unwrap();
    assert!(abort.rewound);
    assert_eq!(head(&fx), local);
    assert_pair_back(&fx);
}
