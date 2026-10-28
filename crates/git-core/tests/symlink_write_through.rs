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
    assert!(
        matches!(&result, Err(StashError::Git(e)) if is_checkout_conflict(e)),
        "{result:?}"
    );
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
    assert!(matches!(result, Err(StageError::InvalidPath)), "{result:?}");
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
    assert!(
        matches!(&result, Err(BranchError::Git(e)) if is_checkout_conflict(e)),
        "{result:?}"
    );
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
    let result = git_core::pick::cherry_pick(&f.repo, &other, None);
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
    let run = git_core::pick::cherry_pick_many(&f.repo, &[first, second]);
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
    git_core::rebase::abort_rebase(&f.repo, state).unwrap();
    f.untouched();
    assert_eq!(common::read(f.path(), "tc"), "plain\n");
}

#[test]
fn abort_rebase_ancestor_never_writes_through() {
    let f = fx();
    let state = paused_rebase(&f);
    f.link_ancestor();
    git_core::rebase::abort_rebase(&f.repo, state).unwrap();
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
        matches!(&result, Err(BranchError::Git(e)) if is_checkout_conflict(e)),
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
    let result = git_core::pick::cherry_pick(&f.repo, &other, None);
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
    let result = git_core::pick::revert(&f.repo, &removal, None);
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
