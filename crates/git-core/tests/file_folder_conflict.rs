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
//! Each then refused with `FileFolderConflict` before anything was written (#410); now each
//! writes git's conflict itself (#416). Only `git stash apply --index` still refuses, as git
//! does.

mod common;

use std::path::Path;

use common::{commit_all, commit_tree, init_repo, write_file};
use git2::{Oid, Repository};
use git_core::hooks::HookOptions;
use git_core::merge::MergeOptions;
use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseStepResult};
use git_core::stash::{StashApplyOutcome, StashError, StashOptions};
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

/// git's outcome for the merge: `c/g` merged, HEAD's file kept as the conflict `c~HEAD` ("added
/// by us", stage 2 only), `c.txt` gone, the merge in progress.
fn assert_merge_conflict(f: &Fixture, merge_head: Oid, squash: bool) {
    assert_eq!(read(f.dir.path(), "c~HEAD"), "file\n");
    assert_eq!(read(f.dir.path(), "c/g"), "g\n");
    assert!(!f.dir.path().join("c.txt").exists());
    assert!(!f.dir.path().join("c").is_file());
    let index = f.repo.index().unwrap();
    let entries: Vec<(String, u16)> = index
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
            ("c/g".to_string(), 0),
            ("c~HEAD".to_string(), 2)
        ]
    );
    assert_eq!(f.repo.head().unwrap().target(), Some(f.head));
    if squash {
        assert_eq!(f.repo.state(), git2::RepositoryState::Clean);
        assert!(f.repo.path().join("SQUASH_MSG").exists());
    } else {
        assert_eq!(f.repo.state(), git2::RepositoryState::Merge);
        let recorded = std::fs::read_to_string(f.repo.path().join("MERGE_HEAD")).unwrap();
        assert_eq!(recorded.trim(), merge_head.to_string());
    }
}

#[test]
fn merge_reports_a_file_folder_conflict_libgit2_misses_like_git() {
    let f = fixture();
    let outcome = git_core::merge::start_merge(&f.repo, "side", MergeOptions::default()).unwrap();
    assert_eq!(
        outcome,
        git_core::merge::MergeOutcome::Conflicted {
            files: vec!["c~HEAD".to_string()]
        }
    );
    assert_merge_conflict(&f, f.side, false);
}

#[test]
fn a_merge_with_a_file_folder_conflict_can_be_aborted() {
    let f = fixture();
    git_core::merge::start_merge(&f.repo, "side", MergeOptions::default()).unwrap();
    git_core::merge::abort_merge(&f.repo).unwrap();
    assert_untouched(&f.repo, &f.branch, f.head);
    assert_eq!(read(f.dir.path(), "c.txt"), "c\n");
    assert_eq!(read(f.dir.path(), "c"), "file\n");
    assert!(!f.dir.path().join("c~HEAD").exists());
}

#[test]
fn a_merge_with_a_file_folder_conflict_can_be_resolved() {
    let f = fixture();
    git_core::merge::start_merge(&f.repo, "side", MergeOptions::default()).unwrap();
    // Like git: drop the moved file (or keep it under another name) and stage that.
    std::fs::remove_file(f.dir.path().join("c~HEAD")).unwrap();
    let mut index = f.repo.index().unwrap();
    index.remove_path(std::path::Path::new("c~HEAD")).unwrap();
    index.write().unwrap();
    assert!(!index.has_conflicts());
    let tree = f.repo.find_tree(index.write_tree().unwrap()).unwrap();
    let names: Vec<String> = tree
        .iter()
        .map(|entry| entry.name().unwrap().to_string())
        .collect();
    assert_eq!(names, ["a.txt", "c"]);
}

#[test]
fn squash_merge_reports_a_file_folder_conflict_libgit2_misses_like_git() {
    let f = fixture();
    let options = MergeOptions {
        squash: true,
        ..MergeOptions::default()
    };
    let outcome = git_core::merge::start_merge(&f.repo, "side", options).unwrap();
    assert!(
        matches!(outcome, git_core::merge::MergeOutcome::Conflicted { .. }),
        "{outcome:?}"
    );
    assert_merge_conflict(&f, f.side, true);
}

/// A diverged pull's "Merge" merges the remote-tracking branch through `start_merge`.
#[test]
fn merging_a_remote_tracking_branch_reports_it_too() {
    let f = fixture();
    f.repo
        .reference("refs/remotes/origin/side", f.side, false, "test")
        .unwrap();
    git_core::merge::start_merge(&f.repo, "refs/remotes/origin/side", MergeOptions::default())
        .unwrap();
    assert_merge_conflict(&f, f.side, false);
    let message = std::fs::read_to_string(f.repo.path().join("MERGE_MSG")).unwrap();
    assert!(
        message.starts_with("Merge remote-tracking branch 'origin/side'"),
        "{message}"
    );
}

/// The other way round: HEAD has the folder, the merged branch the file. The file is then
/// "added by them": it is moved to `c~<branch>`.
#[test]
fn merge_moves_the_file_coming_in_over_a_folder() {
    let f = fixture();
    switch(&f.repo, "side");
    git_core::merge::start_merge(&f.repo, &f.branch, MergeOptions::default()).unwrap();
    assert_eq!(read(f.dir.path(), "c/g"), "g\n");
    assert_eq!(read(f.dir.path(), &format!("c~{}", f.branch)), "file\n");
    assert_eq!(f.repo.state(), git2::RepositoryState::Merge);
}

/// git's outcome for a pick/revert step: `c/g` added, HEAD's file kept as the conflict
/// `c~HEAD`, the step paused with `head_file` (`CHERRY_PICK_HEAD`/`REVERT_HEAD`) and a message.
fn assert_step_conflict(f: &Fixture, head_file: &str, state: git2::RepositoryState) {
    assert_eq!(read(f.dir.path(), "c~HEAD"), "file\n");
    assert_eq!(read(f.dir.path(), "c/g"), "g\n");
    assert!(!f.dir.path().join("c.txt").exists());
    let index = f.repo.index().unwrap();
    let entries: Vec<(String, u16)> = index
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
            ("c/g".to_string(), 0),
            ("c~HEAD".to_string(), 2)
        ]
    );
    assert_eq!(f.repo.head().unwrap().target(), Some(f.head));
    assert_eq!(f.repo.state(), state);
    assert!(f.repo.path().join(head_file).exists());
    assert!(f.repo.path().join("MERGE_MSG").exists());
}

#[test]
fn cherry_pick_stops_at_a_file_folder_conflict_libgit2_misses_like_git() {
    let f = fixture();
    let outcome =
        git_core::pick::cherry_pick(&f.repo, &f.side.to_string(), None, HookOptions::default())
            .unwrap();
    assert_eq!(
        outcome,
        git_core::pick::PickOutcome::Conflicted {
            files: vec!["c~HEAD".to_string()]
        }
    );
    assert_step_conflict(&f, "CHERRY_PICK_HEAD", git2::RepositoryState::CherryPick);
    let message = std::fs::read_to_string(f.repo.path().join("MERGE_MSG")).unwrap();
    assert!(message.starts_with("c.txt becomes c/g\n"), "{message}");
}

#[test]
fn a_cherry_pick_stopped_at_a_file_folder_conflict_can_be_aborted() {
    let f = fixture();
    git_core::pick::cherry_pick(&f.repo, &f.side.to_string(), None, HookOptions::default())
        .unwrap();
    git_core::merge::abort_merge(&f.repo).unwrap();
    assert_untouched(&f.repo, &f.branch, f.head);
    assert_eq!(read(f.dir.path(), "c.txt"), "c\n");
    assert_eq!(read(f.dir.path(), "c"), "file\n");
    assert!(!f.dir.path().join("c~HEAD").exists());
}

#[test]
fn cherry_picking_several_commits_stops_at_that_step() {
    let f = fixture();
    let run =
        git_core::pick::cherry_pick_many(&f.repo, &[f.side.to_string()], HookOptions::default());
    assert!(run.outcome.is_ok(), "{:?}", run.outcome.err());
    assert_step_conflict(&f, "CHERRY_PICK_HEAD", git2::RepositoryState::CherryPick);
}

#[test]
fn revert_stops_at_a_file_folder_conflict_libgit2_misses_like_git() {
    let f = fixture();
    // Reverting a commit that turned `c/g` back into `c.txt` deletes `c.txt` and adds `c/g`.
    let undo = commit_tree(
        &f.repo,
        &[("a.txt", "a\n", FILE), ("c.txt", "c\n", FILE)],
        &[f.side],
        "c/g becomes c.txt",
    );
    let outcome =
        git_core::pick::revert(&f.repo, &undo.to_string(), None, HookOptions::default()).unwrap();
    assert!(
        matches!(outcome, git_core::pick::PickOutcome::Conflicted { .. }),
        "{outcome:?}"
    );
    assert_step_conflict(&f, "REVERT_HEAD", git2::RepositoryState::Revert);
}

/// The index's entries as (path, stage), in index order.
fn index_entries(repo: &Repository) -> Vec<(String, u16)> {
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    index
        .iter()
        .map(|entry| {
            (
                String::from_utf8_lossy(&entry.path).into_owned(),
                (entry.flags >> 12) & 0x3,
            )
        })
        .collect()
}

/// `side` rebased onto the default branch: its one step stops at git's conflict.
fn rebase_side(f: &Fixture) -> git_core::rebase::RebaseState {
    switch(&f.repo, "side");
    let plan = vec![RebasePlanEntry {
        commit_id: f.side.to_string(),
        action: RebaseAction::Pick,
        combined_message: None,
    }];
    let (state, result) = git_core::rebase::start_rebase(&f.repo, &f.branch, plan).unwrap();
    assert_eq!(
        result,
        RebaseStepResult::Conflicted {
            files: vec!["c~HEAD".to_string()]
        }
    );
    state
}

/// git (`git rebase main` on `side`): "CONFLICT (file/directory): directory in the way of c from
/// HEAD; moving it to c~HEAD instead", the step paused with `c/g` merged.
#[test]
fn rebase_step_stops_at_a_file_folder_conflict_libgit2_misses_like_git() {
    let f = fixture();
    let state = rebase_side(&f);
    assert_eq!(state.stop, git_core::rebase::RebaseStop::Conflict);
    assert_eq!(read(f.dir.path(), "c~HEAD"), "file\n");
    assert_eq!(read(f.dir.path(), "c/g"), "g\n");
    assert!(!f.dir.path().join("c.txt").exists());
    assert_eq!(
        index_entries(&f.repo),
        [
            ("a.txt".to_string(), 0),
            ("c/g".to_string(), 0),
            ("c~HEAD".to_string(), 2)
        ]
    );
    assert!(f.repo.head_detached().unwrap());
    assert_eq!(f.repo.head().unwrap().target(), Some(f.head));
    assert_eq!(f.repo.state(), git2::RepositoryState::CherryPick);
    let picked = std::fs::read_to_string(f.repo.path().join("CHERRY_PICK_HEAD")).unwrap();
    assert_eq!(picked.trim(), f.side.to_string());
    let message = std::fs::read_to_string(f.repo.path().join("MERGE_MSG")).unwrap();
    assert!(
        message.starts_with("c.txt becomes c/g\n\n# Conflicts:\n#\tc~HEAD\n"),
        "{message}"
    );
    assert_eq!(
        git_core::rebase::rebase_health(&f.repo, &state).unwrap(),
        git_core::rebase::RebaseHealth::Paused
    );
}

#[test]
fn a_rebase_stopped_at_a_file_folder_conflict_continues_once_resolved() {
    let f = fixture();
    let mut state = rebase_side(&f);
    // Like git: keep the moved file under the name git gave it, and stage it.
    let mut index = f.repo.index().unwrap();
    index.add_path(Path::new("c~HEAD")).unwrap();
    index.write().unwrap();
    let result = git_core::rebase::rebase_continue(&f.repo, &mut state).unwrap();
    assert_eq!(result, RebaseStepResult::Done);
    let head = f.repo.head().unwrap();
    assert_eq!(head.name().unwrap(), "refs/heads/side");
    let commit = head.peel_to_commit().unwrap();
    assert_eq!(commit.parent_id(0).unwrap(), f.head);
    assert_eq!(commit.summary().unwrap(), Some("c.txt becomes c/g"));
    let mut tree = git2::Index::new().unwrap();
    tree.read_tree(&commit.tree().unwrap()).unwrap();
    let names: Vec<String> = tree
        .iter()
        .map(|entry| String::from_utf8_lossy(&entry.path).into_owned())
        .collect();
    assert_eq!(names, ["a.txt", "c/g", "c~HEAD"]);
    assert_eq!(f.repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn a_rebase_stopped_at_a_file_folder_conflict_can_be_aborted() {
    let f = fixture();
    let state = rebase_side(&f);
    let abort = git_core::rebase::abort_rebase(&f.repo, &state).unwrap();
    assert!(abort.restored);
    assert_untouched(&f.repo, "side", f.side);
    assert_eq!(read(f.dir.path(), "c/g"), "g\n");
    assert!(!f.dir.path().join("c~HEAD").exists());
    assert_eq!(
        f.repo
            .refname_to_id(&format!("refs/heads/{}", f.branch))
            .unwrap(),
        f.head
    );
}

/// The other way round (`git rebase side` on the default branch): the step brings the file, so
/// it is "added by them" and moved to `c~<short id> (<subject>)`, git's label for the step.
#[test]
fn a_rebase_step_bringing_the_file_moves_it_to_the_steps_label() {
    let f = fixture();
    let plan = vec![RebasePlanEntry {
        commit_id: f.head.to_string(),
        action: RebaseAction::Pick,
        combined_message: None,
    }];
    let (_, result) = git_core::rebase::start_rebase(&f.repo, "side", plan).unwrap();
    let short = f.head.to_string()[..7].to_string();
    let moved = format!("c~{short} (add the file c)");
    assert_eq!(
        result,
        RebaseStepResult::Conflicted {
            files: vec![moved.clone()]
        }
    );
    assert_eq!(read(f.dir.path(), &moved), "file\n");
    assert_eq!(read(f.dir.path(), "c/g"), "g\n");
    assert_eq!(
        index_entries(&f.repo),
        [("a.txt".to_string(), 0), ("c/g".to_string(), 0), (moved, 3)]
    );
    assert_eq!(f.repo.head().unwrap().target(), Some(f.side));
}

/// A `/` in git's label becomes `_` (git: `git merge topic/file` moves the file to
/// `c~topic_file`).
#[test]
fn a_slash_in_the_label_is_flattened_like_git() {
    let f = fixture();
    f.repo
        .branch("topic/file", &f.repo.find_commit(f.head).unwrap(), false)
        .unwrap();
    switch(&f.repo, "side");
    git_core::merge::start_merge(&f.repo, "topic/file", MergeOptions::default()).unwrap();
    assert_eq!(read(f.dir.path(), "c~topic_file"), "file\n");
    assert!(!f.dir.path().join("c~topic").exists());
}

/// git's label for a revert's other side is `parent of <short id> (<subject>)`.
#[test]
fn a_revert_moves_the_file_to_gits_parent_of_label() {
    let f = fixture();
    // On HEAD (which added `c`), a commit dropping `c`; reverting it onto `side` brings `c`
    // back, while `side` deleted `c.txt` and added `c/g`.
    let drop = commit_tree(
        &f.repo,
        &[("a.txt", "a\n", FILE), ("c.txt", "c\n", FILE)],
        &[f.head],
        "drop c",
    );
    switch(&f.repo, "side");
    git_core::pick::revert(&f.repo, &drop.to_string(), None, HookOptions::default()).unwrap();
    let short = drop.to_string()[..7].to_string();
    assert_eq!(
        read(f.dir.path(), &format!("c~parent of {short} (drop c)")),
        "file\n"
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

/// git's outcome for `git stash apply` (and pop): "CONFLICT (file/directory): directory in the
/// way of c from Updated upstream; moving it to c~Updated upstream instead", `c/g` merged and
/// staged, no operation in progress.
fn assert_stash_conflict(f: &Fixture) {
    assert_eq!(read(f.dir.path(), "c~Updated upstream"), "file\n");
    assert_eq!(read(f.dir.path(), "c/g"), "g\n");
    assert!(!f.dir.path().join("c.txt").exists());
    assert_eq!(
        index_entries(&f.repo),
        [
            ("a.txt".to_string(), 0),
            ("c/g".to_string(), 0),
            ("c~Updated upstream".to_string(), 2)
        ]
    );
    assert_eq!(f.repo.head().unwrap().target(), Some(f.head));
    assert_eq!(f.repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn stash_apply_reports_a_file_folder_conflict_libgit2_misses_like_git() {
    let mut f = stashed_fixture();
    let outcome = git_core::stash::apply_stash(&mut f.repo, 0, None, false).unwrap();
    assert_eq!(
        outcome,
        StashApplyOutcome::Conflicted {
            files: vec!["c~Updated upstream".to_string()]
        }
    );
    assert_stash_conflict(&f);
    assert_eq!(stash_count(&mut f.repo), 1);
}

#[test]
fn stash_pop_reports_it_and_keeps_the_stash() {
    let mut f = stashed_fixture();
    let outcome = git_core::stash::pop_stash(&mut f.repo, 0, None, false).unwrap();
    assert!(
        matches!(outcome, StashApplyOutcome::Conflicted { .. }),
        "{outcome:?}"
    );
    assert_stash_conflict(&f);
    assert_eq!(stash_count(&mut f.repo), 1);
}

/// git: `git stash apply --index` refuses ("Conflicts in index. Try without --index.") and
/// changes nothing, since the staged part (`c/g`) can't be applied to the index either.
#[test]
fn stash_apply_restoring_the_index_refuses_it_like_git() {
    for pop in [false, true] {
        let mut f = stashed_fixture();
        let error = if pop {
            git_core::stash::pop_stash(&mut f.repo, 0, None, true).unwrap_err()
        } else {
            git_core::stash::apply_stash(&mut f.repo, 0, None, true).unwrap_err()
        };
        assert!(matches!(error, StashError::IndexConflict), "{error}");
        assert_untouched(&f.repo, &f.branch, f.head);
        assert_eq!(stash_count(&mut f.repo), 1);
    }
}

/// `stashed_fixture` with the untracked file `u.txt` in the stash too.
fn stashed_with_untracked_fixture() -> Fixture {
    let mut f = fixture();
    f.repo
        .branch("stash-base", &f.repo.find_commit(f.base).unwrap(), false)
        .unwrap();
    switch(&f.repo, "stash-base");
    std::fs::remove_file(f.dir.path().join("c.txt")).unwrap();
    write_file(f.dir.path(), "c/g", "g\n");
    write_file(f.dir.path(), "u.txt", "untracked\n");
    let mut index = f.repo.index().unwrap();
    index.remove_path(Path::new("c.txt")).unwrap();
    index.add_path(Path::new("c/g")).unwrap();
    index.write().unwrap();
    let options = StashOptions {
        include_untracked: true,
        ..StashOptions::default()
    };
    git_core::stash::save_stash(&mut f.repo, &options).unwrap();
    assert!(!f.dir.path().join("u.txt").exists());
    switch(&f.repo, &f.branch.clone());
    f
}

/// git restores the stash's untracked files next to the conflict, untracked.
#[test]
fn stash_apply_restores_untracked_files_beside_the_conflict() {
    let mut f = stashed_with_untracked_fixture();
    git_core::stash::apply_stash(&mut f.repo, 0, None, false).unwrap();
    assert_stash_conflict(&f);
    assert_eq!(read(f.dir.path(), "u.txt"), "untracked\n");
    let status = f.repo.status_file(Path::new("u.txt")).unwrap();
    assert_eq!(status, git2::Status::WT_NEW);
}

/// An untracked file of the stash already in the work tree: refused like libgit2's apply
/// refuses it, nothing written.
#[test]
fn stash_apply_refuses_an_untracked_file_in_the_way() {
    let mut f = stashed_with_untracked_fixture();
    write_file(f.dir.path(), "u.txt", "mine\n");
    let error = git_core::stash::apply_stash(&mut f.repo, 0, None, false).unwrap_err();
    assert!(matches!(error, StashError::WouldOverwrite), "{error}");
    assert_eq!(read(f.dir.path(), "u.txt"), "mine\n");
    assert_eq!(read(f.dir.path(), "c"), "file\n");
    assert!(!f.dir.path().join("c/g").exists());
    assert!(!f.dir.path().join("c~Updated upstream").exists());
    assert_eq!(
        index_entries(&f.repo),
        [
            ("a.txt".to_string(), 0),
            ("c".to_string(), 0),
            ("c.txt".to_string(), 0)
        ]
    );
    assert_eq!(stash_count(&mut f.repo), 1);
}

/// Staged changes: libgit2's apply refuses before its merge (`StagedChanges`), so the
/// file/folder path, which merges against HEAD's tree, must not run either; nothing written.
#[test]
fn stash_apply_with_staged_changes_is_refused_before_the_file_folder_path() {
    let mut f = stashed_fixture();
    write_file(f.dir.path(), "new.txt", "staged\n");
    let mut index = f.repo.index().unwrap();
    index.add_path(Path::new("new.txt")).unwrap();
    index.write().unwrap();
    let error = git_core::stash::apply_stash(&mut f.repo, 0, None, false).unwrap_err();
    assert!(matches!(error, StashError::StagedChanges), "{error}");
    assert_eq!(read(f.dir.path(), "c"), "file\n");
    assert!(!f.dir.path().join("c/g").exists());
    assert!(!f.dir.path().join("c~Updated upstream").exists());
    assert_eq!(
        index_entries(&f.repo),
        [
            ("a.txt".to_string(), 0),
            ("c".to_string(), 0),
            ("c.txt".to_string(), 0),
            ("new.txt".to_string(), 0)
        ]
    );
    assert_eq!(stash_count(&mut f.repo), 1);
}

/// A local change to a file the stash changes too: refused (the safe checkout would overwrite
/// it), nothing written.
#[test]
fn stash_apply_refuses_to_overwrite_a_local_change() {
    let mut f = fixture();
    f.repo
        .branch("stash-base", &f.repo.find_commit(f.base).unwrap(), false)
        .unwrap();
    switch(&f.repo, "stash-base");
    std::fs::remove_file(f.dir.path().join("c.txt")).unwrap();
    write_file(f.dir.path(), "c/g", "g\n");
    write_file(f.dir.path(), "a.txt", "stashed\n");
    let mut index = f.repo.index().unwrap();
    index.remove_path(Path::new("c.txt")).unwrap();
    index.add_path(Path::new("c/g")).unwrap();
    index.write().unwrap();
    git_core::stash::save_stash(&mut f.repo, &StashOptions::default()).unwrap();
    switch(&f.repo, &f.branch.clone());
    write_file(f.dir.path(), "a.txt", "mine\n");
    let error = git_core::stash::apply_stash(&mut f.repo, 0, None, false).unwrap_err();
    assert!(matches!(error, StashError::WouldOverwrite), "{error}");
    assert_eq!(read(f.dir.path(), "a.txt"), "mine\n");
    assert_eq!(read(f.dir.path(), "c"), "file\n");
    assert!(!f.dir.path().join("c~Updated upstream").exists());
    assert_eq!(stash_count(&mut f.repo), 1);
}

/// Unix: a checkout that stops part-way (at the read-only `z/m.txt`, after it wrote `c/g`,
/// `c~Updated upstream` and the untracked `u.txt`) is rolled back (#321): nothing is left, the
/// stash is kept, and the apply goes through once the file can be written.
#[cfg(unix)]
#[test]
fn a_stash_apply_stopped_part_way_is_rolled_back() {
    use std::os::unix::fs::PermissionsExt;
    if unsafe { libc::geteuid() } == 0 {
        assert!(
            std::env::var_os("CI").is_none(),
            "root writes read-only files"
        );
        return;
    }
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), "c.txt", "c\n");
    write_file(dir.path(), "z/m.txt", "m\n");
    commit_all(&repo, "base");
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    let base = repo.head().unwrap().target().unwrap();
    repo.branch("stash-base", &repo.find_commit(base).unwrap(), false)
        .unwrap();
    write_file(dir.path(), "c", "file\n");
    commit_all(&repo, "add the file c");
    let head = repo.head().unwrap().target().unwrap();
    switch(&repo, "stash-base");
    std::fs::remove_file(dir.path().join("c.txt")).unwrap();
    write_file(dir.path(), "c/g", "g\n");
    write_file(dir.path(), "z/m.txt", "stashed\n");
    write_file(dir.path(), "u.txt", "untracked\n");
    let mut index = repo.index().unwrap();
    index.remove_path(Path::new("c.txt")).unwrap();
    index.add_path(Path::new("c/g")).unwrap();
    index.write().unwrap();
    let options = StashOptions {
        include_untracked: true,
        ..StashOptions::default()
    };
    git_core::stash::save_stash(&mut repo, &options).unwrap();
    switch(&repo, &branch);

    let modes = [("z/m.txt", 0o444), ("z", 0o555)];
    for (rel, mode) in modes {
        std::fs::set_permissions(dir.path().join(rel), std::fs::Permissions::from_mode(mode))
            .unwrap();
    }
    let error = git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap_err();
    for (rel, mode) in [("z", 0o755), ("z/m.txt", 0o644)] {
        std::fs::set_permissions(dir.path().join(rel), std::fs::Permissions::from_mode(mode))
            .unwrap();
    }
    assert!(!matches!(error, StashError::WouldOverwrite), "{error}");
    assert_untouched(&repo, &branch, head);
    assert!(!dir.path().join("u.txt").exists());
    assert!(!dir.path().join("c~Updated upstream").exists());
    assert_eq!(read(dir.path(), "c"), "file\n");
    assert_eq!(stash_count(&mut repo), 1);

    let outcome = git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap();
    assert!(
        matches!(outcome, StashApplyOutcome::Conflicted { .. }),
        "{outcome:?}"
    );
    assert_eq!(read(dir.path(), "z/m.txt"), "stashed\n");
    assert_eq!(read(dir.path(), "u.txt"), "untracked\n");
}

/// Makes `stash@{0}` the stash it is with an untracked parent holding `files`.
fn add_untracked_parent(repo: &Repository, files: &[(&str, &str, u32)]) {
    let stash = repo
        .revparse_single("refs/stash")
        .unwrap()
        .peel_to_commit()
        .unwrap();
    let untracked = commit_tree(repo, files, &[], "untracked");
    let parents = [
        stash.parent(0).unwrap(),
        stash.parent(1).unwrap(),
        repo.find_commit(untracked).unwrap(),
    ];
    let parents: Vec<&git2::Commit> = parents.iter().collect();
    let signature = stash.author();
    let tree = stash.tree().unwrap();
    let id = repo
        .commit(None, &signature, &signature, "stash", &tree, &parents)
        .unwrap();
    repo.reference("refs/stash", id, true, "with untracked")
        .unwrap();
}

/// The stash's untracked file at a path the merge holds (`a.txt`, tracked since): git would
/// leave the conflict without restoring it, so Browsitory refuses and changes nothing.
#[test]
fn stash_apply_refuses_an_untracked_file_the_merge_holds() {
    let mut f = stashed_fixture();
    add_untracked_parent(&f.repo, &[("a.txt", "other\n", FILE)]);
    let error = git_core::stash::apply_stash(&mut f.repo, 0, None, false).unwrap_err();
    let conflict = match &error {
        StashError::FileFolderConflict(conflict) => Some(conflict),
        _ => None,
    };
    expect_conflict(&error, conflict);
    assert_untouched(&f.repo, &f.branch, f.head);
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
