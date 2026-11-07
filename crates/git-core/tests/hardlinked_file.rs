//! A tracked file that is a hard link to a file in another folder (#260): Browsitory's own
//! writes (Save resolution, Add to .gitignore, Undo/Redo of a work-tree file) replace the name
//! with a new file, as git's checkout does, so the other name keeps its content; reads (Undo's
//! snapshot, Mark resolved) see the file. On Windows these used to fail with "a folder of the
//! working tree changed while it was being written": the walker's in-the-held-folder check read
//! the handle's final path, which may be the other name. Runs on every platform.

mod common;

use std::path::{Path, PathBuf};

use git2::Repository;
use git_core::merge::{MarkResolvedOutcome, MergeOptions, MergeOutcome};
use git_core::undo::{capture, restore, SnapshotScope};
use tempfile::TempDir;

struct Fx {
    dir: TempDir,
    repo: Repository,
    /// The other folder, outside the work tree (on the same volume: both are temp dirs).
    out: TempDir,
}

impl Fx {
    fn new() -> Fx {
        let (dir, repo) = common::init_repo();
        let out = TempDir::new().unwrap();
        Fx { dir, repo, out }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn other(&self) -> PathBuf {
        self.out.path().join("other.txt")
    }

    /// Makes `rel` a hard link to `other.txt` in the other folder, keeping its content.
    fn link(&self, rel: &str) {
        let path = self.path().join(rel);
        std::fs::write(self.other(), std::fs::read(&path).unwrap()).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::hard_link(self.other(), &path).unwrap();
    }

    fn other_content(&self) -> String {
        std::fs::read_to_string(self.other()).unwrap()
    }
}

/// A merge of `other` stopped on a conflict in `d/f.txt`.
fn conflicted() -> Fx {
    let fx = Fx::new();
    common::write_file(fx.path(), "d/f.txt", "base\n");
    common::commit_all(&fx.repo, "base");
    let main = fx.repo.head().unwrap().shorthand().unwrap().to_string();
    git_core::branch::create_branch(&fx.repo, "other", "HEAD").unwrap();
    common::write_file(fx.path(), "d/f.txt", "theirs\n");
    common::commit_all(&fx.repo, "theirs");
    git_core::branch::switch_branch(&fx.repo, &main).unwrap();
    common::write_file(fx.path(), "d/f.txt", "ours\n");
    common::commit_all(&fx.repo, "ours");
    let outcome = git_core::merge::start_merge(&fx.repo, "other", MergeOptions::default()).unwrap();
    assert!(matches!(outcome, MergeOutcome::Conflicted { .. }));
    fx
}

#[test]
fn saving_a_resolution_replaces_the_hardlinked_file() {
    let fx = conflicted();
    fx.link("d/f.txt");
    let markers = fx.other_content();
    assert!(markers.contains("<<<<<<<"), "{markers}");
    git_core::merge::resolve_conflict(&fx.repo, "d/f.txt", "resolved\n").unwrap();
    assert_eq!(common::read(fx.path(), "d/f.txt"), "resolved\n");
    assert_eq!(fx.other_content(), markers);
    assert!(!fx.repo.index().unwrap().has_conflicts());
}

#[test]
fn a_hardlinked_file_is_marked_resolved() {
    let fx = conflicted();
    common::write_file(fx.path(), "d/f.txt", "by hand\n");
    fx.link("d/f.txt");
    let outcome = git_core::merge::mark_resolved(&fx.repo, "d/f.txt", false).unwrap();
    assert!(
        matches!(outcome, MarkResolvedOutcome::Resolved),
        "{outcome:?}"
    );
    assert!(!fx.repo.index().unwrap().has_conflicts());
    assert_eq!(fx.other_content(), "by hand\n");
}

#[test]
fn adding_to_a_hardlinked_gitignore_replaces_it() {
    let fx = Fx::new();
    common::write_file(fx.path(), ".gitignore", "x\n");
    common::commit_all(&fx.repo, "c");
    fx.link(".gitignore");
    git_core::stage::ignore_path(&fx.repo, "y").unwrap();
    assert_eq!(common::read(fx.path(), ".gitignore"), "x\ny\n");
    assert_eq!(fx.other_content(), "x\n");
}

#[test]
fn undo_reads_and_replaces_a_hardlinked_file() {
    // `before` has a work-tree change the index doesn't hold, so restoring it writes the file
    // itself (not libgit2's checkout); `after` is captured from the hard link.
    let fx = Fx::new();
    common::write_file(fx.path(), "d/f.txt", "committed\n");
    common::commit_all(&fx.repo, "c");
    common::write_file(fx.path(), "d/f.txt", "changed\n");
    let before = capture(&fx.repo, SnapshotScope::Full).unwrap();
    common::write_file(fx.path(), "d/f.txt", "committed\n");
    fx.link("d/f.txt");
    let after = capture(&fx.repo, SnapshotScope::Full).unwrap();
    restore(&fx.repo, &after, &before, "undo").unwrap();
    assert_eq!(common::read(fx.path(), "d/f.txt"), "changed\n");
    assert_eq!(fx.other_content(), "committed\n");
}
