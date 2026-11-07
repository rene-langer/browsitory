//! libgit2 misses a file/folder conflict when a name sorts between the file and the folder's
//! entries (#410): `c` < `c.txt` < `c/g`. The fixture: the base has `a.txt` and `c.txt`; HEAD
//! (the default branch) adds the file `c`; `side` deletes `c.txt` and adds `c/g`.
//!
//! What plain git does with each operation below (git 2.51): it reports "CONFLICT
//! (file/directory): directory in the way of c from HEAD; moving it to c~HEAD instead", merges
//! `c/g`, and leaves HEAD's file as the conflict `c~HEAD` (stage 2 only, "added by us"; a stash
//! apply names it `c~Updated upstream`), for the user to resolve.
//!
//! What libgit2 did before the guard: a cherry-pick, a revert and a rebase step committed a tree
//! without HEAD's file `c` (the index held `c` and `c/g` at stage 0 and wrote only `c/g`) while
//! the work tree kept the file `c` and never got `c/g`; a merge (also squash, also the diverged
//! pull's merge) and a stash apply stopped with libgit2's raw "failed to make directory 'c':
//! directory exists", the merge having already deleted `c.txt` from the index and work tree.
//! Now each refuses with `FileFolderConflict` before anything is written.

mod common;

use std::path::Path;

use common::{commit_all, commit_tree, init_repo, write_file};
use git2::{Oid, Repository};
use git_core::hooks::HookOptions;
use git_core::merge::{MergeError, MergeOptions};
use git_core::pick::PickError;
use git_core::rebase::{RebaseAction, RebaseError, RebasePlanEntry};
use git_core::stash::{StashError, StashOptions};
use git_core::FileFolderConflict;
use tempfile::TempDir;

struct Fixture {
    dir: TempDir,
    repo: Repository,
    /// The default branch's name; HEAD is on it, at `head`.
    branch: String,
    base: Oid,
    head: Oid,
    side: Oid,
}

const FILE: u32 = 0o100644;

/// The base (`a.txt`, `c.txt`), HEAD adding the file `c` (checked out), and the branch `side`
/// deleting `c.txt` and adding `c/g`.
fn fixture() -> Fixture {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), "c.txt", "c\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().target().unwrap();
    write_file(dir.path(), "c", "file\n");
    commit_all(&repo, "add the file c");
    let head = repo.head().unwrap().target().unwrap();
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    let side = commit_tree(
        &repo,
        &[("a.txt", "a\n", FILE), ("c/g", "g\n", FILE)],
        &[base],
        "c.txt becomes c/g",
    );
    repo.branch("side", &repo.find_commit(side).unwrap(), false)
        .unwrap();
    Fixture {
        dir,
        repo,
        branch,
        base,
        head,
        side,
    }
}

fn switch(repo: &Repository, branch: &str) {
    repo.set_head(&format!("refs/heads/{branch}")).unwrap();
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::new()
            .force()
            .remove_untracked(true),
    ))
    .unwrap();
}

fn expect_conflict(error: impl std::fmt::Display, conflict: Option<&FileFolderConflict>) {
    assert_eq!(
        conflict,
        Some(&FileFolderConflict {
            file: "c".into(),
            inside: "c/g".into(),
        }),
        "{error}"
    );
}

/// Nothing changed: HEAD on `branch` at `tip`, no operation in progress, the index and the work
/// tree exactly HEAD's tree (no `c~HEAD`, no stray `c/g`, nothing deleted).
fn assert_untouched(repo: &Repository, branch: &str, tip: Oid) {
    let head = repo.head().unwrap();
    assert_eq!(head.name().unwrap(), format!("refs/heads/{branch}"));
    assert_eq!(head.target(), Some(tip));
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    for name in ["MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD", "MERGE_MSG"] {
        assert!(!repo.path().join(name).exists(), "{name} left behind");
    }
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo.statuses(Some(&mut options)).unwrap();
    let changed: Vec<String> = statuses
        .iter()
        .map(|entry| format!("{:?} {}", entry.status(), entry.path().unwrap()))
        .collect();
    assert!(changed.is_empty(), "changed: {changed:?}");
}

fn read(dir: &Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap()
}

#[test]
fn merge_refuses_a_file_folder_conflict_libgit2_misses() {
    let f = fixture();
    let error = git_core::merge::start_merge(&f.repo, "side", MergeOptions::default())
        .expect_err("libgit2 would merge c and c/g without a conflict");
    let conflict = match &error {
        MergeError::FileFolderConflict(conflict) => Some(conflict),
        _ => None,
    };
    expect_conflict(&error, conflict);
    assert_untouched(&f.repo, &f.branch, f.head);
    assert_eq!(read(f.dir.path(), "c.txt"), "c\n");
}

#[test]
fn squash_merge_refuses_a_file_folder_conflict_libgit2_misses() {
    let f = fixture();
    let options = MergeOptions {
        squash: true,
        ..MergeOptions::default()
    };
    let error = git_core::merge::start_merge(&f.repo, "side", options).unwrap_err();
    let conflict = match &error {
        MergeError::FileFolderConflict(conflict) => Some(conflict),
        _ => None,
    };
    expect_conflict(&error, conflict);
    assert_untouched(&f.repo, &f.branch, f.head);
    assert!(!f.repo.path().join("SQUASH_MSG").exists());
}

/// A diverged pull's "Merge" merges the remote-tracking branch through `start_merge`.
#[test]
fn merging_a_remote_tracking_branch_refuses_it_too() {
    let f = fixture();
    f.repo
        .reference("refs/remotes/origin/side", f.side, false, "test")
        .unwrap();
    let error =
        git_core::merge::start_merge(&f.repo, "refs/remotes/origin/side", MergeOptions::default())
            .unwrap_err();
    assert!(
        matches!(error, MergeError::FileFolderConflict(_)),
        "{error}"
    );
    assert_untouched(&f.repo, &f.branch, f.head);
}

/// The other way round: HEAD has the folder, the merged branch the file.
#[test]
fn merge_refuses_the_file_coming_in_over_a_folder_too() {
    let f = fixture();
    switch(&f.repo, "side");
    let error =
        git_core::merge::start_merge(&f.repo, &f.branch, MergeOptions::default()).unwrap_err();
    let conflict = match &error {
        MergeError::FileFolderConflict(conflict) => Some(conflict),
        _ => None,
    };
    expect_conflict(&error, conflict);
    assert_untouched(&f.repo, "side", f.side);
    assert_eq!(read(f.dir.path(), "c/g"), "g\n");
}

#[test]
fn cherry_pick_refuses_a_file_folder_conflict_libgit2_misses() {
    let f = fixture();
    let error =
        git_core::pick::cherry_pick(&f.repo, &f.side.to_string(), None, HookOptions::default())
            .expect_err("libgit2 would commit without the file c");
    let conflict = match &error {
        PickError::FileFolderConflict(conflict) => Some(conflict),
        _ => None,
    };
    expect_conflict(&error, conflict);
    assert_untouched(&f.repo, &f.branch, f.head);
    assert_eq!(read(f.dir.path(), "c"), "file\n");
}

#[test]
fn cherry_picking_several_commits_refuses_at_that_step() {
    let f = fixture();
    let run =
        git_core::pick::cherry_pick_many(&f.repo, &[f.side.to_string()], HookOptions::default());
    let error = run.outcome.unwrap_err();
    assert!(matches!(error, PickError::FileFolderConflict(_)), "{error}");
    assert_untouched(&f.repo, &f.branch, f.head);
}

#[test]
fn revert_refuses_a_file_folder_conflict_libgit2_misses() {
    let f = fixture();
    // Reverting a commit that turned `c/g` back into `c.txt` deletes `c.txt` and adds `c/g`.
    let undo = commit_tree(
        &f.repo,
        &[("a.txt", "a\n", FILE), ("c.txt", "c\n", FILE)],
        &[f.side],
        "c/g becomes c.txt",
    );
    let error = git_core::pick::revert(&f.repo, &undo.to_string(), None, HookOptions::default())
        .expect_err("libgit2 would commit without the file c");
    let conflict = match &error {
        PickError::FileFolderConflict(conflict) => Some(conflict),
        _ => None,
    };
    expect_conflict(&error, conflict);
    assert_untouched(&f.repo, &f.branch, f.head);
}

#[test]
fn rebase_step_refuses_a_file_folder_conflict_and_puts_the_branch_back() {
    let f = fixture();
    switch(&f.repo, "side");
    let plan = vec![RebasePlanEntry {
        commit_id: f.side.to_string(),
        action: RebaseAction::Pick,
        combined_message: None,
    }];
    let Err(error) = git_core::rebase::start_rebase(&f.repo, &f.branch, plan) else {
        panic!("libgit2 would land the step without the file c");
    };
    let conflict = match &error {
        RebaseError::FileFolderConflict(conflict) => Some(conflict),
        _ => None,
    };
    expect_conflict(&error, conflict);
    assert_untouched(&f.repo, "side", f.side);
    assert_eq!(
        f.repo
            .refname_to_id(&format!("refs/heads/{}", f.branch))
            .unwrap(),
        f.head
    );
}

/// `stash@{0}`, made on the base: `c.txt` deleted and `c/g` added (staged). HEAD is back on the
/// default branch, which added the file `c`.
fn stashed_fixture() -> Fixture {
    let mut f = fixture();
    f.repo
        .branch("stash-base", &f.repo.find_commit(f.base).unwrap(), false)
        .unwrap();
    switch(&f.repo, "stash-base");
    std::fs::remove_file(f.dir.path().join("c.txt")).unwrap();
    write_file(f.dir.path(), "c/g", "g\n");
    let mut index = f.repo.index().unwrap();
    index.remove_path(Path::new("c.txt")).unwrap();
    index.add_path(Path::new("c/g")).unwrap();
    index.write().unwrap();
    git_core::stash::save_stash(&mut f.repo, &StashOptions::default()).unwrap();
    switch(&f.repo, &f.branch.clone());
    f
}

fn stash_count(repo: &mut Repository) -> usize {
    git_core::stash::list_stashes(repo).unwrap().len()
}

#[test]
fn stash_apply_refuses_a_file_folder_conflict_libgit2_misses() {
    for restore_index in [false, true] {
        let mut f = stashed_fixture();
        let error = git_core::stash::apply_stash(&mut f.repo, 0, None, restore_index).unwrap_err();
        let conflict = match &error {
            StashError::FileFolderConflict(conflict) => Some(conflict),
            _ => None,
        };
        expect_conflict(&error, conflict);
        assert_untouched(&f.repo, &f.branch, f.head);
        assert_eq!(stash_count(&mut f.repo), 1);
    }
}

#[test]
fn stash_pop_refuses_it_and_keeps_the_stash() {
    let mut f = stashed_fixture();
    let error = git_core::stash::pop_stash(&mut f.repo, 0, None, false).unwrap_err();
    assert!(
        matches!(error, StashError::FileFolderConflict(_)),
        "{error}"
    );
    assert_untouched(&f.repo, &f.branch, f.head);
    assert_eq!(stash_count(&mut f.repo), 1);
}

/// With no name between the file and the folder's entries libgit2 finds the conflict itself
/// (its conflict entry `ab` next to `ab/g`), and the pick stops at it as before.
#[test]
fn a_file_folder_conflict_libgit2_finds_still_stops_as_a_conflict() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().target().unwrap();
    write_file(dir.path(), "ab", "file\n");
    commit_all(&repo, "add the file ab");
    let side = commit_tree(
        &repo,
        &[("a.txt", "a\n", FILE), ("ab/g", "g\n", FILE)],
        &[base],
        "add ab/g",
    );
    let outcome =
        git_core::pick::cherry_pick(&repo, &side.to_string(), None, HookOptions::default())
            .unwrap();
    assert!(
        matches!(outcome, git_core::pick::PickOutcome::Conflicted { .. }),
        "{outcome:?}"
    );
    assert_eq!(read(dir.path(), "ab/g"), "g\n");
}

/// Pins the libgit2 behaviour the guard works around (#410). When this fails, libgit2 reports
/// the conflict itself: check whether the guard (`file_folder.rs`) and the "libgit2 misses a
/// file/folder conflict" note in docs/ARCHITECTURE.md can go, and let the operations stop at
/// the conflict like git.
#[test]
fn libgit2_still_merges_a_file_and_a_folder_without_a_conflict() {
    let f = fixture();
    let head = f.repo.find_commit(f.head).unwrap();
    let side = f.repo.find_commit(f.side).unwrap();
    let merged = f.repo.cherrypick_commit(&side, &head, 0, None).unwrap();
    assert!(!merged.has_conflicts());
    let entries: Vec<(String, u16)> = merged
        .iter()
        .map(|entry| {
            (
                String::from_utf8_lossy(&entry.path).into_owned(),
                (entry.flags >> 12) & 0x3,
            )
        })
        .collect();
    assert_eq!(
        entries,
        [
            ("a.txt".to_string(), 0),
            ("c".to_string(), 0),
            ("c/g".to_string(), 0)
        ]
    );
}
