//! Regression matrix for #114: every Browsitory operation that writes or deletes in the work
//! tree, against a tracked file replaced by a symlink to a file outside the repository ("leaf")
//! and a tracked folder replaced by a symlink to a folder outside it ("ancestor"). Whatever the
//! operation does — succeed after removing the link, or refuse — the outside files must stay
//! byte-identical and present. Rows mirror the probe table in
//! `docs/superpowers/specs/2026-10-28-security-bug-wave-design.md` §1.1; the "is_refused" rows
//! pin safe checkouts so a future switch to a forced one is caught.
#![cfg(unix)]

mod common;

use git2::{ErrorCode, Repository};
use git_core::branch::BranchError;
use git_core::merge::{FileConflictChoice, MergeError, MergeOptions, MergeOutcome};
use git_core::reset::{reset_to, ResetMode};
use git_core::stage::StageError;
use git_core::stash::{StashError, StashOptions};
use git_core::undo::{capture, restore, SnapshotScope, UndoError};
use tempfile::TempDir;

struct Fx {
    dir: TempDir,
    repo: Repository,
    out: TempDir,
    main: String,
}

impl Fx {
    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }

    /// `tc` → the outside `target.txt`.
    fn link_leaf(&self) {
        common::replace_with_link(self.path(), "tc", &self.out.path().join("target.txt"));
    }

    /// `d` → the outside folder (which holds its own `f`).
    fn link_ancestor(&self) {
        common::replace_with_link(self.path(), "d", self.out.path());
    }

    fn head(&self) -> String {
        self.repo.head().unwrap().target().unwrap().to_string()
    }

    fn untouched(&self) {
        common::assert_outside_untouched(self.out.path());
    }

    /// Commits `files` on a new branch `other` (from HEAD) and switches back.
    fn side_branch(&self, files: &[(&str, &str)]) -> String {
        git_core::branch::create_branch(&self.repo, "other", "HEAD").unwrap();
        for (path, contents) in files {
            common::write_file(self.path(), path, contents);
        }
        common::commit_all(&self.repo, "other");
        let id = self.head();
        git_core::branch::switch_branch(&self.repo, &self.main).unwrap();
        id
    }

    fn commit(&self, files: &[(&str, &str)], message: &str) -> String {
        for (path, contents) in files {
            common::write_file(self.path(), path, contents);
        }
        common::commit_all(&self.repo, message);
        self.head()
    }

    /// A merge of `other` stopped on an `a.txt` conflict; `other` also changes `tc` and `d/f`
    /// and adds `d/new.txt`.
    fn stopped_merge(&self) {
        self.side_branch(&[
            ("a.txt", "other\n"),
            ("tc", "other\n"),
            ("d/f", "other inner\n"),
            ("d/new.txt", "new\n"),
        ]);
        self.commit(&[("a.txt", "main\n")], "main");
        let outcome =
            git_core::merge::start_merge(&self.repo, "other", MergeOptions::default()).unwrap();
        assert!(matches!(outcome, MergeOutcome::Conflicted { .. }));
    }
}

/// `tc` = "plain\n", `d/f` = "inner\n", `a.txt` = "base\n" committed.
fn fx() -> Fx {
    let out = common::outside_dir();
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "tc", "plain\n");
    common::write_file(dir.path(), "d/f", "inner\n");
    common::write_file(dir.path(), "a.txt", "base\n");
    common::commit_all(&repo, "base");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    Fx {
        dir,
        repo,
        out,
        main,
    }
}

fn is_checkout_conflict(error: &git2::Error) -> bool {
    error.code() == ErrorCode::Conflict
}

// --- Stash ---------------------------------------------------------------------------------

#[test]
fn stash_all_leaf_never_writes_through() {
    let mut f = fx();
    f.link_leaf();
    git_core::stash::save_stash(&mut f.repo, &StashOptions::default()).unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "tc"), "plain\n");
}

#[test]
fn stash_all_tracked_only_leaf_never_writes_through() {
    let mut f = fx();
    f.link_leaf();
    let opts = StashOptions {
        include_untracked: false,
        ..StashOptions::default()
    };
    git_core::stash::save_stash(&mut f.repo, &opts).unwrap();
    f.untouched();
}

#[test]
fn stash_all_keep_index_leaf_never_writes_through() {
    let mut f = fx();
    f.link_leaf();
    let opts = StashOptions {
        keep_index: true,
        ..StashOptions::default()
    };
    git_core::stash::save_stash(&mut f.repo, &opts).unwrap();
    f.untouched();
}

#[test]
fn stash_all_ancestor_is_refused() {
    let mut f = fx();
    f.link_ancestor();
    let result = git_core::stash::save_stash(&mut f.repo, &StashOptions::default());
    assert!(
        matches!(result, Err(StashError::BeyondSymlink(_))),
        "{result:?}"
    );
    f.untouched();
}

#[test]
fn stash_paths_leaf_never_writes_through() {
    let mut f = fx();
    f.link_leaf();
    let opts = StashOptions {
        paths: vec!["tc".into()],
        ..StashOptions::default()
    };
    git_core::stash::save_stash(&mut f.repo, &opts).unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "tc"), "plain\n");
}

#[test]
fn stash_paths_ancestor_is_refused() {
    let mut f = fx();
    f.link_ancestor();
    let opts = StashOptions {
        paths: vec!["d/f".into()],
        ..StashOptions::default()
    };
    let result = git_core::stash::save_stash(&mut f.repo, &opts);
    assert!(
        matches!(result, Err(StashError::BeyondSymlink(_))),
        "{result:?}"
    );
    f.untouched();
}

#[test]
fn stash_apply_leaf_is_refused() {
    let mut f = fx();
    common::write_file(f.path(), "tc", "stashed\n");
    git_core::stash::save_stash(&mut f.repo, &StashOptions::default()).unwrap();
    f.link_leaf();
    let result = git_core::stash::apply_stash(&mut f.repo, 0, None, false);
    assert!(
        matches!(result, Err(StashError::WouldOverwrite)),
        "{result:?}"
    );
    f.untouched();
}

#[test]
fn stash_pop_ancestor_is_refused() {
    let mut f = fx();
    common::write_file(f.path(), "d/f", "stashed\n");
    git_core::stash::save_stash(&mut f.repo, &StashOptions::default()).unwrap();
    f.link_ancestor();
    let result = git_core::stash::pop_stash(&mut f.repo, 0, None, false);
    match result {
        Err(error @ StashError::LinkedFolder(_)) => assert_eq!(
            error.to_string(),
            "'d' is a symbolic link where the repository has a folder. Remove or rename it first."
        ),
        other => panic!("{other:?}"),
    }
    assert_eq!(git_core::stash::list_stashes(&mut f.repo).unwrap().len(), 1);
    f.untouched();
}

// --- Discard -------------------------------------------------------------------------------

#[test]
fn discard_file_leaf_never_writes_through() {
    let f = fx();
    f.link_leaf();
    git_core::stage::discard_file(&f.repo, "tc").unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "tc"), "plain\n");
}

#[test]
fn discard_all_unstaged_leaf_never_writes_through() {
    let f = fx();
    f.link_leaf();
    git_core::stage::discard_all_unstaged(&f.repo, &["tc".into()]).unwrap();
    f.untouched();
}

#[test]
fn discard_file_ancestor_is_refused() {
    let f = fx();
    f.link_ancestor();
    let result = git_core::stage::discard_file(&f.repo, "d/f");
    assert!(
        matches!(result, Err(StageError::BeyondSymlink(_))),
        "{result:?}"
    );
    f.untouched();
}

#[test]
fn discard_file_of_the_linked_folder_removes_only_the_link() {
    let f = fx();
    f.link_ancestor();
    git_core::stage::discard_file(&f.repo, "d").unwrap();
    f.untouched();
    assert!(std::fs::symlink_metadata(f.path().join("d")).is_err());
}

#[test]
fn discard_hunk_leaf_never_writes_through() {
    let f = fx();
    f.link_leaf();
    let hunks = git_core::diff::working_diff(&f.repo, "tc", false, &Default::default()).unwrap();
    for hunk in hunks {
        let _ = git_core::stage::discard_hunk(&f.repo, "tc", hunk.old_start, hunk.new_start, 3);
    }
    f.untouched();
}

// --- Reset ---------------------------------------------------------------------------------

#[test]
fn reset_hard_leaf_never_writes_through() {
    let f = fx();
    f.link_leaf();
    reset_to(&f.repo, &f.head(), ResetMode::Hard).unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "tc"), "plain\n");
}

#[test]
fn reset_hard_ancestor_never_writes_through() {
    let f = fx();
    f.link_ancestor();
    reset_to(&f.repo, &f.head(), ResetMode::Hard).unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "d/f"), "inner\n");
}

#[test]
fn reset_hard_to_another_commit_leaf_never_writes_through() {
    let f = fx();
    let base = f.head();
    f.commit(&[("tc", "second\n")], "second");
    f.link_leaf();
    reset_to(&f.repo, &base, ResetMode::Hard).unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "tc"), "plain\n");
}

#[test]
fn reset_hard_to_a_commit_without_the_folder_ancestor_never_deletes_through() {
    let f = fx();
    let base = f.head();
    f.commit(&[("d/g", "g\n")], "adds d/g");
    std::fs::write(f.out.path().join("g"), "OUTSIDE\n").unwrap();
    f.link_ancestor();
    reset_to(&f.repo, &base, ResetMode::Hard).unwrap();
    f.untouched();
    assert!(f.out.path().join("g").exists());
}

// --- Undo / redo ---------------------------------------------------------------------------

#[test]
fn undo_restore_leaf_never_writes_through() {
    let f = fx();
    let clean = capture(&f.repo, SnapshotScope::Full).unwrap();
    f.link_leaf();
    let dirty = capture(&f.repo, SnapshotScope::Full).unwrap();
    restore(&f.repo, &dirty, &clean, "undo").unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "tc"), "plain\n");
}

#[test]
fn undo_restore_ancestor_never_writes_through() {
    let f = fx();
    let clean = capture(&f.repo, SnapshotScope::Full).unwrap();
    f.link_ancestor();
    let dirty = capture(&f.repo, SnapshotScope::Full).unwrap();
    restore(&f.repo, &dirty, &clean, "undo").unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "d/f"), "inner\n");
}

#[test]
fn undo_restore_leaf_after_the_snapshot_is_refused() {
    let f = fx();
    let clean = capture(&f.repo, SnapshotScope::Full).unwrap();
    common::write_file(f.path(), "tc", "edited\n");
    let dirty = capture(&f.repo, SnapshotScope::Full).unwrap();
    f.link_leaf();
    let result = restore(&f.repo, &dirty, &clean, "undo");
    assert!(
        matches!(result, Err(UndoError::RepositoryChanged)),
        "{result:?}"
    );
    f.untouched();
}

#[test]
fn undo_restore_ancestor_after_the_snapshot_is_refused() {
    let f = fx();
    let clean = capture(&f.repo, SnapshotScope::Full).unwrap();
    common::write_file(f.path(), "d/f", "edited\n");
    let dirty = capture(&f.repo, SnapshotScope::Full).unwrap();
    f.link_ancestor();
    let result = restore(&f.repo, &dirty, &clean, "undo");
    assert!(
        matches!(result, Err(UndoError::RepositoryChanged)),
        "{result:?}"
    );
    f.untouched();
}

#[test]
fn undo_restore_delete_ancestor_never_deletes_through() {
    let f = fx();
    std::fs::write(f.out.path().join("x.txt"), "OUTSIDE\n").unwrap();
    common::write_file(f.path(), "d/x.txt", "untracked\n");
    let with_file = capture(&f.repo, SnapshotScope::Full).unwrap();
    std::fs::remove_file(f.path().join("d/x.txt")).unwrap();
    let without = capture(&f.repo, SnapshotScope::Full).unwrap();
    common::write_file(f.path(), "d/x.txt", "untracked\n");
    f.link_ancestor();
    let result = restore(&f.repo, &with_file, &without, "redo");
    assert!(
        matches!(result, Ok(()) | Err(UndoError::RepositoryChanged)),
        "{result:?}"
    );
    f.untouched();
    assert!(f.out.path().join("x.txt").exists());
}

// --- Safe checkouts (refuse) ---------------------------------------------------------------

#[test]
fn switch_branch_leaf_is_refused() {
    let f = fx();
    f.side_branch(&[("tc", "other\n")]);
    f.link_leaf();
    let result = git_core::branch::switch_branch(&f.repo, "other");
    assert!(
        matches!(&result, Err(BranchError::Git(e)) if is_checkout_conflict(e)),
        "{result:?}"
    );
    f.untouched();
}

#[test]
fn switch_branch_ancestor_is_refused() {
    let f = fx();
    f.side_branch(&[("d/f", "other inner\n")]);
    f.link_ancestor();
    let result = git_core::branch::switch_branch(&f.repo, "other");
    match result {
        Err(error @ BranchError::LinkedFolder(_)) => assert_eq!(
            error.to_string(),
            "'d' is a symbolic link where the repository has a folder. Remove or rename it first."
        ),
        other => panic!("{other:?}"),
    }
    f.untouched();
}

#[test]
fn checkout_commit_leaf_is_refused() {
    let f = fx();
    let other = f.side_branch(&[("tc", "other\n")]);
    f.link_leaf();
    let result = git_core::branch::checkout_commit(&f.repo, &other);
    assert!(
        matches!(&result, Err(BranchError::Git(e)) if is_checkout_conflict(e)),
        "{result:?}"
    );
    f.untouched();
}

#[test]
fn merge_fast_forward_leaf_is_refused() {
    let f = fx();
    f.side_branch(&[("tc", "other\n")]);
    f.link_leaf();
    let result = git_core::merge::start_merge(&f.repo, "other", MergeOptions::default());
    assert!(
        matches!(&result, Err(MergeError::Git(e)) if is_checkout_conflict(e)),
        "{result:?}"
    );
    f.untouched();
}

#[test]
fn merge_non_fast_forward_leaf_is_refused() {
    let f = fx();
    f.side_branch(&[("tc", "other\n")]);
    f.commit(&[("a.txt", "main\n")], "main");
    f.link_leaf();
    let result = git_core::merge::start_merge(&f.repo, "other", MergeOptions::default());
    assert!(
        matches!(&result, Err(MergeError::Git(e)) if is_checkout_conflict(e)),
        "{result:?}"
    );
    f.untouched();
}

#[test]
fn cherry_pick_leaf_is_refused() {
    let f = fx();
    let other = f.side_branch(&[("tc", "other\n")]);
    f.link_leaf();
    let result = git_core::pick::cherry_pick(
        &f.repo,
        &other,
        None,
        git_core::hooks::HookOptions::default(),
    );
    assert!(result.is_err(), "{result:?}");
    f.untouched();
}

#[test]
fn start_rebase_leaf_is_refused() {
    let f = fx();
    let base = f.head();
    let second = f.commit(&[("a.txt", "second\n")], "second");
    git_core::branch::create_branch(&f.repo, "onto", &base).unwrap();
    let onto = f.commit(&[("tc", "onto\n")], "onto");
    git_core::branch::switch_branch(&f.repo, &f.main).unwrap();
    f.link_leaf();
    let plan = vec![git_core::rebase::RebasePlanEntry {
        commit_id: second,
        action: git_core::rebase::RebaseAction::Pick,
        combined_message: None,
    }];
    let result = git_core::rebase::start_rebase(&f.repo, &onto, plan);
    assert!(result.is_err(), "{:?}", result.map(|(_, r)| r));
    f.untouched();
}

// --- Merge / cherry-pick / revert abort and conflict resolution ----------------------------

#[test]
fn abort_merge_conflicted_path_leaf_never_writes_through() {
    let f = fx();
    f.stopped_merge();
    common::replace_with_link(f.path(), "a.txt", &f.out.path().join("target.txt"));
    git_core::merge::abort_merge(&f.repo).unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "a.txt"), "main\n");
}

#[test]
fn abort_merge_merged_path_leaf_never_writes_through() {
    let f = fx();
    f.stopped_merge();
    f.link_leaf();
    git_core::merge::abort_merge(&f.repo).unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "tc"), "plain\n");
}

#[test]
fn abort_merge_ancestor_never_writes_through() {
    let f = fx();
    f.stopped_merge();
    f.link_ancestor();
    git_core::merge::abort_merge(&f.repo).unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "d/f"), "inner\n");
}

#[test]
fn abort_merge_added_path_ancestor_never_deletes_through() {
    let f = fx();
    std::fs::write(f.out.path().join("new.txt"), "OUTSIDE\n").unwrap();
    f.stopped_merge();
    f.link_ancestor();
    git_core::merge::abort_merge(&f.repo).unwrap();
    f.untouched();
    assert!(f.out.path().join("new.txt").exists());
}

#[test]
fn resolve_whole_file_leaf_never_writes_through() {
    let f = fx();
    f.stopped_merge();
    common::replace_with_link(f.path(), "a.txt", &f.out.path().join("target.txt"));
    git_core::merge::resolve_whole_file(&f.repo, "a.txt", git_core::merge::ConflictSide::Ours)
        .unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "a.txt"), "main\n");
}

#[test]
fn resolve_conflict_leaf_never_writes_through() {
    let f = fx();
    f.stopped_merge();
    common::replace_with_link(f.path(), "a.txt", &f.out.path().join("target.txt"));
    git_core::merge::resolve_conflict(&f.repo, "a.txt", "resolved\n").unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "a.txt"), "resolved\n");
}

#[test]
fn resolve_conflict_ancestor_is_refused() {
    let f = fx();
    f.side_branch(&[("d/f", "other inner\n")]);
    f.commit(&[("d/f", "main inner\n")], "main");
    let outcome = git_core::merge::start_merge(&f.repo, "other", MergeOptions::default()).unwrap();
    assert!(matches!(outcome, MergeOutcome::Conflicted { .. }));
    f.link_ancestor();
    let result = git_core::merge::resolve_conflict(&f.repo, "d/f", "resolved\n");
    assert!(
        matches!(result, Err(MergeError::SymlinkedFolder(_))),
        "{result:?}"
    );
    f.untouched();
}

#[test]
fn resolve_add_delete_leaf_never_writes_through() {
    let mut f = fx();
    f.side_branch(&[("a.txt", "other\n")]);
    std::fs::remove_file(f.path().join("a.txt")).unwrap();
    git_core::stage::stage_file(&f.repo, "a.txt").unwrap();
    git_core::commit::commit(&mut f.repo, "delete", Default::default()).unwrap();
    let outcome = git_core::merge::start_merge(&f.repo, "other", MergeOptions::default()).unwrap();
    assert!(matches!(outcome, MergeOutcome::Conflicted { .. }));
    common::replace_with_link(f.path(), "a.txt", &f.out.path().join("target.txt"));
    git_core::merge::resolve_add_delete_conflict(&f.repo, "a.txt", FileConflictChoice::Theirs)
        .unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "a.txt"), "other\n");
}

#[test]
fn abort_sequence_leaf_never_writes_through() {
    let f = fx();
    let first = f.commit(&[("tc", "picked\n")], "changes tc");
    let second = f.commit(&[("a.txt", "second\n")], "changes a");
    // Back to base on a new branch whose a.txt conflicts with `second`.
    let base = f
        .repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .parent(0)
        .unwrap()
        .parent(0)
        .unwrap()
        .id()
        .to_string();
    git_core::branch::create_branch(&f.repo, "seq", &base).unwrap();
    f.commit(&[("a.txt", "conflicting\n")], "seq");
    let run = git_core::pick::cherry_pick_many(
        &f.repo,
        &[first, second],
        git_core::hooks::HookOptions::default(),
    );
    let state = run.state.expect("the sequence stops on the conflict");
    f.link_leaf();
    let _ = git_core::pick::abort_sequence(&f.repo, &state);
    f.untouched();
}

// --- Rebase abort --------------------------------------------------------------------------

fn paused_rebase(f: &Fx) -> git_core::rebase::RebaseState {
    let base = f.head();
    f.commit(&[("a.txt", "on top\n")], "on top");
    let conflicting = f.commit(&[("a.txt", "again\n")], "again");
    let plan = vec![git_core::rebase::RebasePlanEntry {
        commit_id: conflicting,
        action: git_core::rebase::RebaseAction::Pick,
        combined_message: None,
    }];
    let (state, result) = git_core::rebase::start_rebase(&f.repo, &base, plan).unwrap();
    assert!(matches!(
        result,
        git_core::rebase::RebaseStepResult::Conflicted { .. }
    ));
    state
}

#[test]
fn abort_rebase_leaf_never_writes_through() {
    let f = fx();
    let state = paused_rebase(&f);
    f.link_leaf();
    git_core::rebase::abort_rebase(&f.repo, &state).unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "tc"), "plain\n");
}

#[test]
fn abort_rebase_ancestor_never_writes_through() {
    let f = fx();
    let state = paused_rebase(&f);
    f.link_ancestor();
    git_core::rebase::abort_rebase(&f.repo, &state).unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "d/f"), "inner\n");
}

// --- Safe checkouts that *add* a file below the linked folder ------------------------------
//
// libgit2's safe strategy does not count a new file below a symlinked tracked folder as a
// conflict and creates (or truncates) it through the link. Every safe operation refuses first.

/// `other` adds `d/new.txt`; the outside folder already holds a `new.txt` that must survive.
fn adds_below_linked_folder() -> (Fx, String) {
    let f = fx();
    let other = f.side_branch(&[("d/new.txt", "NEW\n")]);
    std::fs::write(f.out.path().join("new.txt"), "PRECIOUS\n").unwrap();
    f.link_ancestor();
    (f, other)
}

fn precious_kept(f: &Fx) {
    f.untouched();
    assert_eq!(
        std::fs::read_to_string(f.out.path().join("new.txt")).unwrap(),
        "PRECIOUS\n"
    );
}

#[test]
fn switch_branch_adding_below_the_ancestor_is_refused() {
    let (f, _) = adds_below_linked_folder();
    let result = git_core::branch::switch_branch(&f.repo, "other");
    assert!(
        matches!(&result, Err(BranchError::LinkedFolder(_))),
        "{result:?}"
    );
    precious_kept(&f);
}

#[test]
fn checkout_commit_adding_below_the_ancestor_is_refused() {
    let (f, other) = adds_below_linked_folder();
    let result = git_core::branch::checkout_commit(&f.repo, &other);
    assert!(result.is_err(), "{result:?}");
    precious_kept(&f);
}

#[test]
fn merge_fast_forward_adding_below_the_ancestor_is_refused() {
    let (f, _) = adds_below_linked_folder();
    let result = git_core::merge::start_merge(&f.repo, "other", MergeOptions::default());
    assert!(result.is_err(), "{result:?}");
    precious_kept(&f);
}

#[test]
fn merge_non_fast_forward_adding_below_the_ancestor_is_refused() {
    let f = fx();
    f.side_branch(&[("d/new.txt", "NEW\n")]);
    f.commit(&[("a.txt", "main\n")], "main");
    std::fs::write(f.out.path().join("new.txt"), "PRECIOUS\n").unwrap();
    f.link_ancestor();
    let result = git_core::merge::start_merge(&f.repo, "other", MergeOptions::default());
    assert!(result.is_err(), "{result:?}");
    precious_kept(&f);
}

#[test]
fn squash_merge_adding_below_the_ancestor_is_refused() {
    let f = fx();
    f.side_branch(&[("d/new.txt", "NEW\n")]);
    f.commit(&[("a.txt", "main\n")], "main");
    std::fs::write(f.out.path().join("new.txt"), "PRECIOUS\n").unwrap();
    f.link_ancestor();
    let opts = MergeOptions {
        squash: true,
        ..MergeOptions::default()
    };
    let result = git_core::merge::start_merge(&f.repo, "other", opts);
    assert!(result.is_err(), "{result:?}");
    precious_kept(&f);
}

#[test]
fn cherry_pick_adding_below_the_ancestor_is_refused() {
    let (f, other) = adds_below_linked_folder();
    let result = git_core::pick::cherry_pick(
        &f.repo,
        &other,
        None,
        git_core::hooks::HookOptions::default(),
    );
    assert!(result.is_err(), "{result:?}");
    precious_kept(&f);
}

#[test]
fn revert_adding_below_the_ancestor_is_refused() {
    let f = fx();
    let removes = f.commit(&[("d/new.txt", "NEW\n")], "adds");
    std::fs::remove_file(f.path().join("d/new.txt")).unwrap();
    let mut index = f.repo.index().unwrap();
    index
        .remove_path(std::path::Path::new("d/new.txt"))
        .unwrap();
    index.write().unwrap();
    let removal = {
        let sig = f.repo.signature().unwrap();
        let tree = f.repo.find_tree(index.write_tree().unwrap()).unwrap();
        let head = f.repo.head().unwrap().peel_to_commit().unwrap();
        f.repo
            .commit(Some("HEAD"), &sig, &sig, "removes", &tree, &[&head])
            .unwrap()
            .to_string()
    };
    let _ = removes;
    std::fs::write(f.out.path().join("new.txt"), "PRECIOUS\n").unwrap();
    f.link_ancestor();
    let result = git_core::pick::revert(
        &f.repo,
        &removal,
        None,
        git_core::hooks::HookOptions::default(),
    );
    assert!(result.is_err(), "{result:?}");
    precious_kept(&f);
}

#[test]
fn stash_apply_adding_below_the_ancestor_is_refused() {
    let mut f = fx();
    common::write_file(f.path(), "d/new.txt", "NEW\n");
    git_core::stash::save_stash(&mut f.repo, &StashOptions::default()).unwrap();
    std::fs::write(f.out.path().join("new.txt"), "PRECIOUS\n").unwrap();
    f.link_ancestor();
    let result = git_core::stash::apply_stash(&mut f.repo, 0, None, false);
    assert!(result.is_err(), "{result:?}");
    precious_kept(&f);
}

#[test]
fn start_rebase_adding_below_the_ancestor_is_refused() {
    let f = fx();
    let base = f.head();
    let second = f.commit(&[("a.txt", "second\n")], "second");
    git_core::branch::create_branch(&f.repo, "onto", &base).unwrap();
    let onto = f.commit(&[("d/new.txt", "NEW\n")], "onto");
    git_core::branch::switch_branch(&f.repo, &f.main).unwrap();
    std::fs::write(f.out.path().join("new.txt"), "PRECIOUS\n").unwrap();
    f.link_ancestor();
    let plan = vec![git_core::rebase::RebasePlanEntry {
        commit_id: second,
        action: git_core::rebase::RebaseAction::Pick,
        combined_message: None,
    }];
    let result = git_core::rebase::start_rebase(&f.repo, &onto, plan);
    assert!(result.is_err(), "{:?}", result.map(|(_, r)| r));
    precious_kept(&f);
}

// --- A folder that HEAD tracks *as a symlink* (review round 2) ------------------------------
//
// A committed link is libgit2's normal typechange case: a checkout to a commit with a folder
// there removes the link itself (never following it). Browsitory must not remove it first (that
// broke the hard reset), and nothing may land outside even after the user retargets the link.

/// Replaces the folder `d` with a link to `target` and commits it (c2). Returns c1 (with `d/f`),
/// also left as branch `old`.
fn commit_folder_link(f: &Fx, target: &std::path::Path) -> String {
    let c1 = f.head();
    git_core::branch::create_branch(&f.repo, "old", &c1).unwrap();
    git_core::branch::switch_branch(&f.repo, &f.main).unwrap();
    common::replace_with_link(f.path(), "d", target);
    let mut index = f.repo.index().unwrap();
    index.remove_dir(std::path::Path::new("d"), 0).unwrap();
    index.add_path(std::path::Path::new("d")).unwrap();
    index.write().unwrap();
    common::commit_all(&f.repo, "d is a link");
    c1
}

/// The committed link `d` (to a harmless in-repo name) now points at the outside folder.
fn retarget_link(f: &Fx) -> String {
    let c1 = commit_folder_link(f, std::path::Path::new("nowhere"));
    common::replace_with_link(f.path(), "d", f.out.path());
    c1
}

fn d_is_the_folder_again(f: &Fx) {
    assert!(std::fs::symlink_metadata(f.path().join("d"))
        .unwrap()
        .is_dir());
    assert_eq!(common::read(f.path(), "d/f"), "inner\n");
}

#[test]
fn reset_hard_from_a_committed_folder_link_restores_the_folder() {
    let f = fx();
    let c1 = commit_folder_link(&f, f.out.path());
    reset_to(&f.repo, &c1, ResetMode::Hard).unwrap();
    f.untouched();
    d_is_the_folder_again(&f);
}

#[test]
fn reset_hard_from_a_retargeted_committed_folder_link_never_writes_through() {
    let f = fx();
    let c1 = retarget_link(&f);
    reset_to(&f.repo, &c1, ResetMode::Hard).unwrap();
    f.untouched();
    d_is_the_folder_again(&f);
}

#[test]
fn reset_hard_to_head_with_a_committed_folder_link_keeps_the_link() {
    let f = fx();
    commit_folder_link(&f, f.out.path());
    common::write_file(f.path(), "a.txt", "edited\n");
    reset_to(&f.repo, &f.head(), ResetMode::Hard).unwrap();
    f.untouched();
    assert!(common::is_link(f.path(), "d"));
}

#[test]
fn switch_from_a_committed_folder_link_restores_the_folder() {
    let f = fx();
    commit_folder_link(&f, f.out.path());
    git_core::branch::switch_branch(&f.repo, "old").unwrap();
    f.untouched();
    d_is_the_folder_again(&f);
}

#[test]
fn switch_from_a_retargeted_committed_folder_link_never_writes_through() {
    let f = fx();
    retarget_link(&f);
    let _ = git_core::branch::switch_branch(&f.repo, "old");
    f.untouched();
}

#[test]
fn switch_to_a_committed_folder_link_and_back() {
    let f = fx();
    commit_folder_link(&f, f.out.path());
    let link_branch = f.main.clone();
    git_core::branch::switch_branch(&f.repo, "old").unwrap();
    git_core::branch::switch_branch(&f.repo, &link_branch).unwrap();
    f.untouched();
    assert!(common::is_link(f.path(), "d"));
}

#[test]
fn discard_a_retargeted_committed_folder_link_restores_the_link() {
    let f = fx();
    retarget_link(&f);
    git_core::stage::discard_file(&f.repo, "d").unwrap();
    f.untouched();
    assert_eq!(
        std::fs::read_link(f.path().join("d")).unwrap(),
        std::path::Path::new("nowhere")
    );
}

#[test]
fn undo_and_redo_across_a_committed_folder_link() {
    let f = fx();
    let folder = capture(&f.repo, SnapshotScope::Full).unwrap();
    commit_folder_link(&f, f.out.path());
    let link = capture(&f.repo, SnapshotScope::Full).unwrap();
    restore(&f.repo, &link, &folder, "undo").unwrap();
    f.untouched();
    d_is_the_folder_again(&f);
    restore(&f.repo, &folder, &link, "redo").unwrap();
    f.untouched();
    assert!(common::is_link(f.path(), "d"));
}

#[test]
fn undo_with_a_retargeted_committed_folder_link_never_writes_through() {
    let f = fx();
    let folder = capture(&f.repo, SnapshotScope::Full).unwrap();
    commit_folder_link(&f, std::path::Path::new("nowhere"));
    let link = capture(&f.repo, SnapshotScope::Full).unwrap();
    common::replace_with_link(f.path(), "d", f.out.path());
    let result = restore(&f.repo, &link, &folder, "undo");
    assert!(
        matches!(result, Ok(()) | Err(UndoError::RepositoryChanged)),
        "{result:?}"
    );
    f.untouched();
}

#[test]
fn stash_with_a_retargeted_committed_folder_link_never_writes_through() {
    let mut f = fx();
    retarget_link(&f);
    git_core::stash::save_stash(&mut f.repo, &StashOptions::default()).unwrap();
    f.untouched();
    assert_eq!(
        std::fs::read_link(f.path().join("d")).unwrap(),
        std::path::Path::new("nowhere")
    );
}

/// On the link commit, merges `old2` (c1 + `d/new.txt`): libgit2 would merge `d/new.txt` without
/// a conflict and write it through the committed link.
#[test]
fn merge_adding_below_a_committed_folder_link_is_refused() {
    let f = fx();
    commit_folder_link(&f, f.out.path());
    git_core::branch::switch_branch(&f.repo, "old").unwrap();
    f.commit(&[("d/new.txt", "NEW\n")], "old2");
    git_core::branch::switch_branch(&f.repo, &f.main).unwrap();
    let result = git_core::merge::start_merge(&f.repo, "old", MergeOptions::default());
    match result {
        Err(error @ MergeError::LinkedFolder(_)) => assert_eq!(
            error.to_string(),
            "'d' is a symbolic link where the repository has a folder. Remove or rename it first."
        ),
        other => panic!("{other:?}"),
    }
    f.untouched();
    assert!(!f.out.path().join("new.txt").exists());
}

#[test]
fn cherry_pick_adding_below_a_committed_folder_link_is_refused() {
    let f = fx();
    commit_folder_link(&f, f.out.path());
    git_core::branch::switch_branch(&f.repo, "old").unwrap();
    let picked = f.commit(&[("d/new.txt", "NEW\n")], "old2");
    git_core::branch::switch_branch(&f.repo, &f.main).unwrap();
    let result = git_core::pick::cherry_pick(
        &f.repo,
        &picked,
        None,
        git_core::hooks::HookOptions::default(),
    );
    assert!(
        matches!(result, Err(git_core::pick::PickError::LinkedFolder(_))),
        "{result:?}"
    );
    f.untouched();
    assert!(!f.out.path().join("new.txt").exists());
}

/// A merge whose result has nothing below `d` is not blocked by the committed link; aborting its
/// conflict keeps the link.
#[test]
fn merge_and_abort_beside_a_committed_folder_link_work() {
    let f = fx();
    commit_folder_link(&f, f.out.path());
    f.commit(&[("a.txt", "main\n")], "main");
    git_core::branch::switch_branch(&f.repo, "old").unwrap();
    f.commit(&[("a.txt", "old\n")], "old2");
    git_core::branch::switch_branch(&f.repo, &f.main).unwrap();
    let result = git_core::merge::start_merge(&f.repo, "old", MergeOptions::default()).unwrap();
    assert!(
        matches!(result, MergeOutcome::Conflicted { .. }),
        "{result:?}"
    );
    git_core::merge::abort_merge(&f.repo).unwrap();
    f.untouched();
    assert!(common::is_link(f.path(), "d"));
}

/// `old` (folder `d`) gets a commit that conflicts with the link branch, which it is rebased
/// onto; aborting goes back from the link to the folder.
#[test]
fn abort_rebase_from_a_committed_folder_link_restores_the_folder() {
    let f = fx();
    commit_folder_link(&f, f.out.path());
    f.commit(&[("a.txt", "main\n")], "main");
    let onto = f.head();
    git_core::branch::switch_branch(&f.repo, "old").unwrap();
    let picked = f.commit(&[("a.txt", "old\n")], "old2");
    let plan = vec![git_core::rebase::RebasePlanEntry {
        commit_id: picked,
        action: git_core::rebase::RebaseAction::Pick,
        combined_message: None,
    }];
    let (state, step) = git_core::rebase::start_rebase(&f.repo, &onto, plan).unwrap();
    assert!(matches!(
        step,
        git_core::rebase::RebaseStepResult::Conflicted { .. }
    ));
    assert!(common::is_link(f.path(), "d"));
    git_core::rebase::abort_rebase(&f.repo, &state).unwrap();
    f.untouched();
    d_is_the_folder_again(&f);
}

// --- An *untracked* folder link below which the operation adds files (review round 2) ------
//
// libgit2's safe checkouts create a new file through a symlink nothing tracks (cherry-pick
// overwrote the outside `f`). Every safe operation scans what it brings in and refuses.

const LINKED: &str =
    "'e' is a symbolic link where the repository has a folder. Remove or rename it first.";

/// `other` adds `e/f` and `e/new.txt` (HEAD has no `e`); `e` is an untracked link to the outside
/// folder, whose `f` must survive and which must not gain a `new.txt`.
fn untracked_link_fixture() -> (Fx, String) {
    let f = fx();
    let other = f.side_branch(&[("e/f", "NEW\n"), ("e/new.txt", "NEW\n")]);
    f.link_e();
    (f, other)
}

impl Fx {
    fn link_e(&self) {
        common::replace_with_link(self.path(), "e", self.out.path());
    }

    fn nothing_outside(&self) {
        self.untouched();
        assert!(!self.out.path().join("new.txt").exists());
    }
}

#[test]
fn switch_branch_adding_below_an_untracked_link_is_refused() {
    let (f, _) = untracked_link_fixture();
    let result = git_core::branch::switch_branch(&f.repo, "other");
    match result {
        Err(error @ BranchError::LinkedFolder(_)) => assert_eq!(error.to_string(), LINKED),
        other => panic!("{other:?}"),
    }
    f.nothing_outside();
}

#[test]
fn checkout_commit_adding_below_an_untracked_link_is_refused() {
    let (f, other) = untracked_link_fixture();
    let result = git_core::branch::checkout_commit(&f.repo, &other);
    assert!(
        matches!(result, Err(BranchError::LinkedFolder(_))),
        "{result:?}"
    );
    f.nothing_outside();
}

#[test]
fn merge_fast_forward_adding_below_an_untracked_link_is_refused() {
    let (f, _) = untracked_link_fixture();
    let result = git_core::merge::start_merge(&f.repo, "other", MergeOptions::default());
    assert!(
        matches!(result, Err(MergeError::LinkedFolder(_))),
        "{result:?}"
    );
    f.nothing_outside();
}

#[test]
fn merge_non_fast_forward_adding_below_an_untracked_link_is_refused() {
    let f = fx();
    f.side_branch(&[("e/f", "NEW\n"), ("e/new.txt", "NEW\n")]);
    f.commit(&[("a.txt", "main\n")], "main");
    f.link_e();
    let result = git_core::merge::start_merge(&f.repo, "other", MergeOptions::default());
    assert!(
        matches!(result, Err(MergeError::LinkedFolder(_))),
        "{result:?}"
    );
    f.nothing_outside();
}

#[test]
fn squash_merge_adding_below_an_untracked_link_is_refused() {
    let f = fx();
    f.side_branch(&[("e/f", "NEW\n"), ("e/new.txt", "NEW\n")]);
    f.commit(&[("a.txt", "main\n")], "main");
    f.link_e();
    let opts = MergeOptions {
        squash: true,
        ..MergeOptions::default()
    };
    let result = git_core::merge::start_merge(&f.repo, "other", opts);
    assert!(
        matches!(result, Err(MergeError::LinkedFolder(_))),
        "{result:?}"
    );
    f.nothing_outside();
}

#[test]
fn cherry_pick_adding_below_an_untracked_link_is_refused() {
    let (f, other) = untracked_link_fixture();
    let result = git_core::pick::cherry_pick(
        &f.repo,
        &other,
        None,
        git_core::hooks::HookOptions::default(),
    );
    assert!(
        matches!(result, Err(git_core::pick::PickError::LinkedFolder(_))),
        "{result:?}"
    );
    f.nothing_outside();
    assert_eq!(f.repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn cherry_pick_many_adding_below_an_untracked_link_is_refused() {
    let (f, other) = untracked_link_fixture();
    let run = git_core::pick::cherry_pick_many(
        &f.repo,
        &[other],
        git_core::hooks::HookOptions::default(),
    );
    assert!(
        matches!(run.outcome, Err(git_core::pick::PickError::LinkedFolder(_))),
        "{:?}",
        run.outcome
    );
    f.nothing_outside();
}

#[test]
fn revert_adding_below_an_untracked_link_is_refused() {
    let f = fx();
    f.commit(&[("e/f", "NEW\n"), ("e/new.txt", "NEW\n")], "adds e");
    std::fs::remove_dir_all(f.path().join("e")).unwrap();
    let mut index = f.repo.index().unwrap();
    index.remove_dir(std::path::Path::new("e"), 0).unwrap();
    index.write().unwrap();
    common::commit_all(&f.repo, "removes e");
    let removal = f.head();
    f.link_e();
    let result = git_core::pick::revert(
        &f.repo,
        &removal,
        None,
        git_core::hooks::HookOptions::default(),
    );
    assert!(
        matches!(result, Err(git_core::pick::PickError::LinkedFolder(_))),
        "{result:?}"
    );
    f.nothing_outside();
}

#[test]
fn rebase_step_adding_below_an_untracked_link_is_refused() {
    let (f, other) = untracked_link_fixture();
    let plan = vec![git_core::rebase::RebasePlanEntry {
        commit_id: other,
        action: git_core::rebase::RebaseAction::Pick,
        combined_message: None,
    }];
    let onto = f.head();
    let result = git_core::rebase::start_rebase(&f.repo, &onto, plan);
    assert!(
        matches!(result, Err(git_core::rebase::RebaseError::LinkedFolder(_))),
        "{:?}",
        result.map(|(_, r)| r)
    );
    f.nothing_outside();
}

#[test]
fn start_rebase_onto_a_commit_adding_below_an_untracked_link_is_refused() {
    let f = fx();
    let base = f.head();
    let second = f.commit(&[("a.txt", "second\n")], "second");
    git_core::branch::create_branch(&f.repo, "onto", &base).unwrap();
    let onto = f.commit(&[("e/f", "NEW\n"), ("e/new.txt", "NEW\n")], "onto");
    git_core::branch::switch_branch(&f.repo, &f.main).unwrap();
    f.link_e();
    let plan = vec![git_core::rebase::RebasePlanEntry {
        commit_id: second,
        action: git_core::rebase::RebaseAction::Pick,
        combined_message: None,
    }];
    let result = git_core::rebase::start_rebase(&f.repo, &onto, plan);
    assert!(
        matches!(result, Err(git_core::rebase::RebaseError::LinkedFolder(_))),
        "{:?}",
        result.map(|(_, r)| r)
    );
    f.nothing_outside();
}

/// The original HEAD has `e/f`; the sequence's first pick deletes it, the second conflicts, and
/// rewinding to the original HEAD would bring `e/f` back through the link.
#[test]
fn sequence_rewind_adding_below_an_untracked_link_is_refused() {
    let f = fx();
    let original = f.commit(&[("e/f", "ORIG\n"), ("e/new.txt", "ORIG\n")], "adds e");
    git_core::branch::create_branch(&f.repo, "work", &original).unwrap();
    std::fs::remove_dir_all(f.path().join("e")).unwrap();
    let mut index = f.repo.index().unwrap();
    index.remove_dir(std::path::Path::new("e"), 0).unwrap();
    index.write().unwrap();
    common::commit_all(&f.repo, "removes e");
    let removal = f.head();
    let conflicting = f.commit(&[("a.txt", "picked\n")], "a");
    git_core::branch::switch_branch(&f.repo, &f.main).unwrap();
    f.commit(&[("a.txt", "local\n")], "local a");
    let run = git_core::pick::cherry_pick_many(
        &f.repo,
        &[removal, conflicting],
        git_core::hooks::HookOptions::default(),
    );
    let state = run.state.expect("the sequence stops on the conflict");
    assert!(!f.path().join("e").exists());
    f.link_e();
    let result = git_core::pick::abort_sequence(&f.repo, &state);
    assert!(
        matches!(result, Err(git_core::pick::PickError::LinkedFolder(_))),
        "{result:?}"
    );
    f.nothing_outside();
}

#[test]
fn stash_apply_adding_below_an_untracked_link_is_refused() {
    let mut f = fx();
    common::write_file(f.path(), "e/f", "NEW\n");
    common::write_file(f.path(), "e/new.txt", "NEW\n");
    git_core::stash::save_stash(&mut f.repo, &StashOptions::default()).unwrap();
    f.link_e();
    let result = git_core::stash::apply_stash(&mut f.repo, 0, None, false);
    match result {
        Err(error @ StashError::LinkedFolder(_)) => assert_eq!(error.to_string(), LINKED),
        other => panic!("{other:?}"),
    }
    f.nothing_outside();
}

#[test]
fn stash_pop_adding_below_an_untracked_link_is_refused() {
    let mut f = fx();
    common::write_file(f.path(), "e/f", "NEW\n");
    git_core::stash::save_stash(&mut f.repo, &StashOptions::default()).unwrap();
    f.link_e();
    let result = git_core::stash::pop_stash(&mut f.repo, 0, None, false);
    assert!(
        matches!(result, Err(StashError::LinkedFolder(_))),
        "{result:?}"
    );
    assert_eq!(git_core::stash::list_stashes(&mut f.repo).unwrap().len(), 1);
    f.nothing_outside();
}

/// `e` is excluded, so the worktree counts as clean for a pull.
#[test]
fn pull_fast_forward_adding_below_an_ignored_link_is_refused() {
    let f = fx();
    let other = f.side_branch(&[("e/f", "NEW\n"), ("e/new.txt", "NEW\n")]);
    std::fs::create_dir_all(f.repo.path().join("info")).unwrap();
    std::fs::write(f.repo.path().join("info/exclude"), "e\n").unwrap();
    f.repo
        .reference(
            &format!("refs/remotes/origin/{}", f.main),
            git2::Oid::from_str(&other).unwrap(),
            true,
            "fetched",
        )
        .unwrap();
    f.link_e();
    let result = git_core::remote::pull_after_fetch(&f.repo, "origin", &f.main);
    match result {
        Err(error @ git_core::remote::RemoteError::LinkedFolder(_)) => {
            assert_eq!(error.to_string(), LINKED)
        }
        other => panic!("{other:?}"),
    }
    f.nothing_outside();
}

#[test]
fn switch_branch_with_an_untracked_link_nothing_brings_in_still_works() {
    let f = fx();
    f.side_branch(&[("a.txt", "other\n")]);
    f.link_e();
    git_core::branch::switch_branch(&f.repo, "other").unwrap();
    f.nothing_outside();
    assert!(common::is_link(f.path(), "e"));
}

// --- Submodule update (review round 2) -----------------------------------------------------

/// Adds a submodule at `path` (cloned from a fresh repo), commits it, then removes its checkout
/// and module dir and replaces `link` (the path itself or a folder above it) with a link to the
/// outside folder.
fn submodule_behind_link(path: &str, link: &str) -> Fx {
    let f = fx();
    let source = TempDir::new().unwrap();
    {
        let child = Repository::init(source.path()).unwrap();
        let mut config = child.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        common::write_file(source.path(), "subfile", "s\n");
        common::commit_all(&child, "child");
    }
    let mut sub = f
        .repo
        .submodule(
            source.path().to_str().unwrap(),
            std::path::Path::new(path),
            true,
        )
        .unwrap();
    sub.clone(None).unwrap();
    sub.add_finalize().unwrap();
    drop(sub);
    common::commit_all(&f.repo, "adds the submodule");
    std::fs::remove_dir_all(f.path().join(path)).unwrap();
    let _ = std::fs::remove_dir_all(f.repo.path().join("modules"));
    common::replace_with_link(f.path(), link, f.out.path());
    std::mem::forget(source);
    f
}

fn outside_gained_nothing(f: &Fx) {
    f.untouched();
    let names: Vec<_> = std::fs::read_dir(f.out.path())
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
}

#[test]
fn submodule_update_through_a_linked_submodule_path_is_refused() {
    let f = submodule_behind_link("sub", "sub");
    git_core::submodule::init_submodule(&f.repo, "sub").unwrap();
    let result = git_core::submodule::update_submodule(&f.repo, "sub", false);
    match result {
        Err(error @ git_core::submodule::SubmoduleError::LinkedFolder(_)) => assert_eq!(
            error.to_string(),
            "'sub' is a symbolic link where the repository has a folder. Remove or rename it first."
        ),
        other => panic!("{other:?}"),
    }
    outside_gained_nothing(&f);
}

#[test]
fn submodule_update_below_a_linked_folder_is_refused() {
    let f = submodule_behind_link("a/sub", "a");
    git_core::submodule::init_submodule(&f.repo, "a/sub").unwrap();
    let result = git_core::submodule::update_submodule(&f.repo, "a/sub", false);
    assert!(
        matches!(
            result,
            Err(git_core::submodule::SubmoduleError::LinkedFolder(_))
        ),
        "{result:?}"
    );
    outside_gained_nothing(&f);
}

// --- Staging below a symlinked folder (review round 2; #157) ------------------------------
//
// Staging must never read the outside file. A whole file below the link is staged as deleted
// (git sees no file there, #157 D3-A); hunks and lines can't be staged from it and are refused
// ("beyond a symbolic link").

fn d_f_still_staged(f: &Fx) {
    let index = f.repo.index().unwrap();
    let entry = index.get_path(std::path::Path::new("d/f"), 0).unwrap();
    assert_eq!(entry.id, f.repo.blob(b"inner\n").unwrap());
}

#[test]
fn stage_file_ancestor_stages_the_deletion_without_reading_through() {
    let f = fx();
    f.link_ancestor();
    git_core::stage::stage_file(&f.repo, "d/f").unwrap();
    let index = f.repo.index().unwrap();
    assert!(index.get_path(std::path::Path::new("d/f"), 0).is_none());
    // The outside folder holds its own `f`; nothing of it reached the index.
    assert!(index.iter().all(|entry| !entry.path.starts_with(b"d")));
    f.untouched();
}

#[test]
fn stage_hunk_ancestor_is_refused() {
    let f = fx();
    f.link_ancestor();
    let hunks = git_core::diff::working_diff(&f.repo, "d/f", false, &Default::default()).unwrap();
    for hunk in hunks {
        let result = git_core::stage::stage_hunk(&f.repo, "d/f", hunk.old_start, hunk.new_start, 3);
        assert!(
            matches!(result, Err(StageError::BeyondSymlink(_))),
            "{result:?}"
        );
    }
    d_f_still_staged(&f);
    f.untouched();
}

// --- Stage all over a linked folder, discard below one (review round 2, user decisions) ----

/// "Stage all" sends every unstaged path, in status order, to `stage_file`.
fn stage_all(f: &Fx) {
    let paths: Vec<String> = git_core::status::status(&f.repo)
        .unwrap()
        .into_iter()
        .filter(|entry| !entry.staged)
        .map(|entry| entry.path)
        .collect();
    assert_eq!(paths, ["d", "d/f"]);
    for path in paths {
        git_core::stage::stage_file(&f.repo, &path).unwrap();
    }
}

#[test]
fn stage_all_with_a_linked_folder_stages_the_link_in_place_of_the_folder() {
    let f = fx();
    f.link_ancestor();
    stage_all(&f);
    let index = f.repo.index().unwrap();
    assert_eq!(
        index.get_path(std::path::Path::new("d"), 0).unwrap().mode,
        0o120000
    );
    assert!(index.iter().all(|entry| !entry.path.starts_with(b"d/")));
    let status = git_core::status::status(&f.repo).unwrap();
    let summary: Vec<_> = status
        .iter()
        .map(|entry| (entry.path.as_str(), entry.staged, entry.kind))
        .collect();
    use git_core::status::StatusKind;
    assert_eq!(
        summary,
        [
            ("d", true, StatusKind::New),
            ("d/f", true, StatusKind::Deleted)
        ]
    );
    f.untouched();
}

/// #157 (D3-A): staging `d/f` below the link stages its deletion, so Stage all works in any
/// order.
#[test]
fn stage_all_in_reverse_status_order_still_ends_with_the_link_staged() {
    let f = fx();
    f.link_ancestor();
    for path in ["d/f", "d"] {
        git_core::stage::stage_file(&f.repo, path).unwrap();
    }
    let index = f.repo.index().unwrap();
    assert_eq!(
        index.get_path(std::path::Path::new("d"), 0).unwrap().mode,
        0o120000
    );
    assert!(index.iter().all(|entry| !entry.path.starts_with(b"d/")));
    f.untouched();
}

#[test]
fn staging_one_file_below_the_link_leaves_its_siblings_alone() {
    let f = fx();
    f.commit(&[("d/g", "g\n")], "g");
    f.link_ancestor();

    git_core::stage::stage_file(&f.repo, "d/f").unwrap();

    let index = f.repo.index().unwrap();
    assert!(index.get_path(std::path::Path::new("d/f"), 0).is_none());
    assert!(index.get_path(std::path::Path::new("d"), 0).is_none());
    assert_eq!(
        index.get_path(std::path::Path::new("d/g"), 0).unwrap().id,
        f.repo.blob(b"g\n").unwrap()
    );
    f.untouched();
}

#[test]
fn staging_a_conflicted_file_below_a_link_is_still_refused() {
    let f = fx();
    let mut index = f.repo.index().unwrap();
    let entry = index.get_path(std::path::Path::new("d/f"), 0).unwrap();
    index.remove_path(std::path::Path::new("d/f")).unwrap();
    for stage in 1..=3u16 {
        let conflict = git2::IndexEntry {
            ctime: entry.ctime,
            mtime: entry.mtime,
            dev: entry.dev,
            ino: entry.ino,
            mode: entry.mode,
            uid: entry.uid,
            gid: entry.gid,
            file_size: entry.file_size,
            id: entry.id,
            flags: (entry.flags & !0x3000) | (stage << 12),
            flags_extended: entry.flags_extended,
            path: entry.path.clone(),
        };
        index.add(&conflict).unwrap();
    }
    index.write().unwrap();
    f.link_ancestor();

    let result = git_core::stage::stage_file(&f.repo, "d/f");

    assert!(
        matches!(result, Err(StageError::BeyondSymlink(ref p)) if p == "d/f"),
        "{result:?}"
    );
    let index = f.repo.index().unwrap();
    assert!(index.get_path(std::path::Path::new("d/f"), 2).is_some());
    f.untouched();
}

// --- Unstaging inside a folder/link pair (#157, D4-A) --------------------------------------

/// HEAD has `d/f` and `d/g`; `d` is now a link to the outside folder.
fn linked_pair() -> Fx {
    let f = fx();
    f.commit(&[("d/g", "g\n")], "g");
    f.link_ancestor();
    f
}

/// The index entries at or below `root`, as (path, mode).
fn entries_under(repo: &Repository, root: &str) -> Vec<(String, u32)> {
    let prefix = format!("{root}/");
    repo.index()
        .unwrap()
        .iter()
        .map(|entry| (String::from_utf8(entry.path).unwrap(), entry.mode))
        .filter(|(path, _)| path == root || path.starts_with(&prefix))
        .collect()
}

fn assert_pair_restored(f: &Fx) {
    let index = f.repo.index().unwrap();
    assert!(index.get_path(std::path::Path::new("d"), 0).is_none());
    assert_eq!(
        index.get_path(std::path::Path::new("d/f"), 0).unwrap().id,
        f.repo.blob(b"inner\n").unwrap()
    );
    assert_eq!(
        index.get_path(std::path::Path::new("d/g"), 0).unwrap().id,
        f.repo.blob(b"g\n").unwrap()
    );
    use git_core::status::StatusKind;
    let summary: Vec<_> = git_core::status::status(&f.repo)
        .unwrap()
        .into_iter()
        .map(|entry| (entry.path, entry.staged, entry.kind))
        .collect();
    assert_eq!(
        summary,
        [
            ("d".to_string(), false, StatusKind::New),
            ("d/f".to_string(), false, StatusKind::Deleted),
            ("d/g".to_string(), false, StatusKind::Deleted),
        ]
    );
    f.untouched();
}

#[test]
fn unstaging_a_file_after_its_folders_link_was_staged_restores_the_whole_pair() {
    let f = linked_pair();
    git_core::stage::stage_file(&f.repo, "d").unwrap();

    git_core::stage::unstage_file(&f.repo, "d/f").unwrap();

    assert_pair_restored(&f);
}

#[test]
fn unstaging_the_link_itself_restores_the_pair() {
    let f = linked_pair();
    git_core::stage::stage_file(&f.repo, "d").unwrap();

    git_core::stage::unstage_file(&f.repo, "d").unwrap();

    assert_pair_restored(&f);
}

#[test]
fn unstaging_a_file_whose_deletion_was_staged_below_the_link_restores_only_head() {
    let f = linked_pair();
    git_core::stage::stage_file(&f.repo, "d/f").unwrap();

    git_core::stage::unstage_file(&f.repo, "d/f").unwrap();

    assert_pair_restored(&f);
}

#[test]
fn unstage_all_over_the_pair_in_any_order_restores_it() {
    for order in [["d/f", "d/g", "d"], ["d", "d/g", "d/f"]] {
        let f = linked_pair();
        git_core::stage::stage_file(&f.repo, "d").unwrap();
        for path in order {
            git_core::stage::unstage_file(&f.repo, path).unwrap();
        }
        assert_eq!(
            entries_under(&f.repo, "d"),
            [("d/f".to_string(), 0o100644), ("d/g".to_string(), 0o100644)],
            "{order:?}"
        );
        f.untouched();
    }
}

#[test]
fn unstaging_a_file_in_a_folder_that_replaced_a_committed_link_restores_the_link() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "base\n");
    std::os::unix::fs::symlink("x", dir.path().join("l")).unwrap();
    common::commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("l")).unwrap();
    common::write_file(dir.path(), "l/a", "a\n");
    git_core::stage::stage_file(&repo, "l/a").unwrap();
    assert_eq!(entries_under(&repo, "l"), [("l/a".to_string(), 0o100644)]);

    git_core::stage::unstage_file(&repo, "l/a").unwrap();

    assert_eq!(entries_under(&repo, "l"), [("l".to_string(), 0o120000)]);
}

/// The root sorts before a sibling (`z.txt`): libgit2's reset of the root alone fails with
/// "index does not contain l/a" (review finding).
#[test]
fn unstaging_a_file_in_a_folder_that_replaced_a_committed_link_with_a_later_sibling() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "z.txt", "z\n");
    std::os::unix::fs::symlink("x", dir.path().join("l")).unwrap();
    common::commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("l")).unwrap();
    common::write_file(dir.path(), "l/a", "a\n");
    git_core::stage::stage_file(&repo, "l/a").unwrap();

    git_core::stage::unstage_file(&repo, "l/a").unwrap();

    assert_eq!(entries_under(&repo, "l"), [("l".to_string(), 0o120000)]);
    assert!(repo
        .index()
        .unwrap()
        .get_path(std::path::Path::new("z.txt"), 0)
        .is_some());
}

#[test]
fn unstaging_a_file_in_a_nested_folder_that_replaced_a_committed_file() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a/b", "b\n");
    common::write_file(dir.path(), "a/c", "c\n");
    common::commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("a/b")).unwrap();
    common::write_file(dir.path(), "a/b/c", "inner\n");
    git_core::stage::stage_file(&repo, "a/b/c").unwrap();
    assert_eq!(
        entries_under(&repo, "a/b"),
        [("a/b/c".to_string(), 0o100644)]
    );

    git_core::stage::unstage_file(&repo, "a/b/c").unwrap();

    assert_eq!(entries_under(&repo, "a/b"), [("a/b".to_string(), 0o100644)]);
    assert_eq!(entries_under(&repo, "a/c"), [("a/c".to_string(), 0o100644)]);
}

/// A staged rename into the pair unstages its source too (#137 inside #157).
#[test]
fn unstaging_a_rename_into_a_folder_that_replaced_a_committed_link_restores_its_source() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "old.txt", "moved content\nline 2\nline 3\n");
    std::os::unix::fs::symlink("x", dir.path().join("l")).unwrap();
    common::commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("l")).unwrap();
    std::fs::create_dir(dir.path().join("l")).unwrap();
    std::fs::rename(dir.path().join("old.txt"), dir.path().join("l/f")).unwrap();
    git_core::stage::stage_file(&repo, "old.txt").unwrap();
    git_core::stage::stage_file(&repo, "l/f").unwrap();
    let renamed = git_core::status::status(&repo)
        .unwrap()
        .into_iter()
        .any(|e| e.staged && e.kind == git_core::status::StatusKind::Renamed && e.path == "l/f");
    assert!(renamed, "fixture: l/f must show as a staged rename");

    git_core::stage::unstage_file(&repo, "l/f").unwrap();

    assert_eq!(entries_under(&repo, "l"), [("l".to_string(), 0o120000)]);
    assert!(repo
        .index()
        .unwrap()
        .get_path(std::path::Path::new("old.txt"), 0)
        .is_some());
}

/// HEAD has a link `l` (and `z.txt`, which sorts after it); the work tree has a folder `l/a`.
fn folder_over_committed_link() -> (TempDir, Repository) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "z.txt", "z\n");
    std::os::unix::fs::symlink("x", dir.path().join("l")).unwrap();
    common::commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("l")).unwrap();
    common::write_file(dir.path(), "l/a", "a\n");
    (dir, repo)
}

/// HEAD has a file `d` (and `z.txt`); the work tree has a folder `d/u`.
fn folder_over_committed_file() -> (TempDir, Repository) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "d", "file\n");
    common::write_file(dir.path(), "z.txt", "z\n");
    common::commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("d")).unwrap();
    common::write_file(dir.path(), "d/u", "u\n");
    (dir, repo)
}

fn staged_paths(repo: &Repository) -> Vec<String> {
    git_core::status::status(repo)
        .unwrap()
        .into_iter()
        .filter(|entry| entry.staged)
        .map(|entry| entry.path)
        .collect()
}

fn unstaged_paths(repo: &Repository) -> Vec<String> {
    git_core::status::status(repo)
        .unwrap()
        .into_iter()
        .filter(|entry| !entry.staged)
        .map(|entry| entry.path)
        .collect()
}

/// #157 M1: the clicked path is the root itself (`l`, a link in HEAD, with `l/a` staged).
#[test]
fn unstaging_the_root_of_a_folder_that_replaced_a_committed_link_restores_the_link() {
    let (_dir, repo) = folder_over_committed_link();
    git_core::stage::stage_file(&repo, "l/a").unwrap();

    git_core::stage::unstage_file(&repo, "l").unwrap();

    assert_eq!(entries_under(&repo, "l"), [("l".to_string(), 0o120000)]);
    assert_eq!(
        entries_under(&repo, "z.txt"),
        [("z.txt".to_string(), 0o100644)]
    );
}

#[test]
fn unstaging_the_root_of_a_folder_that_replaced_a_committed_file_restores_the_file() {
    let (_dir, repo) = folder_over_committed_file();
    git_core::stage::stage_file(&repo, "d/u").unwrap();

    git_core::stage::unstage_file(&repo, "d").unwrap();

    assert_eq!(entries_under(&repo, "d"), [("d".to_string(), 0o100644)]);
}

/// Stage all, then Unstage all, both in status order, over a folder that replaced a committed
/// link or file (#157 M1, m1).
#[test]
fn stage_all_then_unstage_all_over_a_folder_that_replaced_a_committed_link_or_file() {
    for (fixture, root, mode) in [
        (
            folder_over_committed_link as fn() -> (TempDir, Repository),
            "l",
            0o120000,
        ),
        (folder_over_committed_file, "d", 0o100644),
    ] {
        let (_dir, repo) = fixture();
        let leaf = format!("{root}/{}", if root == "l" { "a" } else { "u" });
        for path in unstaged_paths(&repo) {
            git_core::stage::stage_file(&repo, &path).unwrap();
        }
        assert_eq!(entries_under(&repo, root), [(leaf.clone(), 0o100644)]);
        assert!(
            unstaged_paths(&repo).is_empty(),
            "{:?}",
            unstaged_paths(&repo)
        );

        for path in staged_paths(&repo) {
            git_core::stage::unstage_file(&repo, &path).unwrap();
        }
        assert_eq!(entries_under(&repo, root), [(root.to_string(), mode)]);
        assert!(staged_paths(&repo).is_empty(), "{:?}", staged_paths(&repo));
    }
}

/// m1: Stage all in reverse status order still stages both halves.
#[test]
fn staging_the_root_after_its_folders_file_is_a_no_op() {
    let (_dir, repo) = folder_over_committed_link();
    git_core::stage::stage_file(&repo, "l/a").unwrap();

    git_core::stage::stage_file(&repo, "l").unwrap();

    assert_eq!(entries_under(&repo, "l"), [("l/a".to_string(), 0o100644)]);
}

/// M2: a rename whose source is inside a staged folder/link pair. Unstaging the rename's new
/// path leaves the pair staged (its source's deletion belongs to the pair), like
/// `git restore --staged e.txt`.
#[test]
fn unstaging_a_rename_out_of_a_staged_link_pair_leaves_the_pair_staged() {
    let f = fx();
    f.commit(
        &[
            ("d/f", "moved content\nline 2\nline 3\nline 4\n"),
            ("d/g", "g\n"),
        ],
        "content",
    );
    common::write_file(f.path(), "e.txt", "moved content\nline 2\nline 3\nline 4\n");
    f.link_ancestor();
    for path in unstaged_paths(&f.repo) {
        git_core::stage::stage_file(&f.repo, &path).unwrap();
    }
    let renamed = git_core::status::status(&f.repo)
        .unwrap()
        .into_iter()
        .any(|e| {
            e.staged
                && e.kind == git_core::status::StatusKind::Renamed
                && e.path == "e.txt"
                && e.old_path.as_deref() == Some("d/f")
        });
    assert!(renamed, "fixture: e.txt must be a staged rename of d/f");

    git_core::stage::unstage_file(&f.repo, "e.txt").unwrap();

    assert_eq!(entries_under(&f.repo, "d"), [("d".to_string(), 0o120000)]);
    assert!(entries_under(&f.repo, "e.txt").is_empty());
    f.untouched();
}

#[test]
fn stage_file_of_the_linked_folder_itself_replaces_its_entries() {
    let f = fx();
    f.link_ancestor();
    git_core::stage::stage_file(&f.repo, "d").unwrap();
    let index = f.repo.index().unwrap();
    assert!(index.get_path(std::path::Path::new("d/f"), 0).is_none());
    assert_eq!(
        index.get_path(std::path::Path::new("d"), 0).unwrap().mode,
        0o120000
    );
    f.untouched();
}

#[test]
fn discard_file_below_the_linked_folder_is_beyond_a_symbolic_link() {
    let f = fx();
    f.link_ancestor();
    let error = git_core::stage::discard_file(&f.repo, "d/f").unwrap_err();
    assert_eq!(error.to_string(), "'d/f' is beyond a symbolic link.");
    f.untouched();
}

#[test]
fn discard_hunk_below_the_linked_folder_is_beyond_a_symbolic_link() {
    let f = fx();
    f.link_ancestor();
    let hunks = git_core::diff::working_diff(&f.repo, "d/f", false, &Default::default()).unwrap();
    assert!(!hunks.is_empty());
    for hunk in hunks {
        let result =
            git_core::stage::discard_hunk(&f.repo, "d/f", hunk.old_start, hunk.new_start, 3);
        assert!(
            matches!(&result, Err(StageError::BeyondSymlink(path)) if path == "d/f"),
            "{result:?}"
        );
    }
    f.untouched();
}
