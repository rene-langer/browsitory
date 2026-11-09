//! A tracked file that is a hard link to a file outside the work tree, written by libgit2's
//! checkouts and patch applies (#420). git's checkout and `git apply` replace the name, so the
//! other name keeps its content; libgit2 writes the file in place, which changed the other name
//! too. Each operation family now matches git: the work-tree file has the new content, the
//! other name the old. Runs on every platform.

mod common;

use std::path::{Path, PathBuf};

use git2::Repository;
use git_core::diff::{working_diff, DiffOptions};
use git_core::merge::{MergeOptions, MergeOutcome};
use git_core::reset::ResetMode;
use git_core::stash::StashOptions;
use git_core::undo::{capture, restore, SnapshotScope};
use tempfile::TempDir;

struct Fx {
    dir: TempDir,
    repo: Repository,
    /// The other name's folder, outside the work tree (on the same volume: both are temp
    /// dirs).
    out: TempDir,
}

impl Fx {
    /// A repository with `d/f.txt` committed as `content` on the main branch.
    fn new(content: &str) -> Fx {
        let (dir, repo) = common::init_repo();
        common::write_file(dir.path(), "d/f.txt", content);
        common::write_file(dir.path(), "e.txt", "e\n");
        common::commit_all(&repo, "base");
        let out = TempDir::new().unwrap();
        Fx { dir, repo, out }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn other(&self) -> PathBuf {
        self.out.path().join("other.txt")
    }

    fn main(&self) -> String {
        self.repo.head().unwrap().shorthand().unwrap().to_string()
    }

    /// Makes `d/f.txt` a hard link to `other.txt` outside the work tree, keeping its content.
    fn link(&self) {
        let path = self.path().join("d/f.txt");
        let _ = std::fs::remove_file(self.other());
        std::fs::write(self.other(), std::fs::read(&path).unwrap()).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::hard_link(self.other(), &path).unwrap();
        self.write_other_check();
    }

    /// The link is real: a write through the work-tree name shows at the other name.
    fn write_other_check(&self) {
        let path = self.path().join("d/f.txt");
        let content = std::fs::read(&path).unwrap();
        std::fs::write(&path, &content).unwrap();
        assert_eq!(std::fs::read(self.other()).unwrap(), content);
    }

    fn work(&self) -> String {
        common::read(self.path(), "d/f.txt")
    }

    fn other_content(&self) -> String {
        std::fs::read_to_string(self.other()).unwrap()
    }

    /// The branch `side` with `d/f.txt` as `content`; HEAD stays on the main branch.
    fn side(&self, content: &str) -> git2::Oid {
        let main = self.main();
        git_core::branch::create_branch(&self.repo, "side", "HEAD").unwrap();
        git_core::branch::switch_branch(&self.repo, "side").unwrap();
        common::write_file(self.path(), "d/f.txt", content);
        common::commit_all(&self.repo, "side");
        let side = self.repo.head().unwrap().target().unwrap();
        git_core::branch::switch_branch(&self.repo, &main).unwrap();
        side
    }

    /// The hunk lines of `d/f.txt`'s unstaged change as the UI shows them.
    fn shown(&self) -> Vec<String> {
        let opts = DiffOptions {
            ignore_whitespace: false,
            context_lines: 3,
            algorithm: git_core::diff::DiffAlgorithm::Default,
        };
        working_diff(&self.repo, "d/f.txt", false, &opts).unwrap()[0]
            .lines
            .iter()
            .map(|line| line.content.clone())
            .collect()
    }
}

#[test]
fn discarding_a_file_replaces_the_hardlinked_file() {
    let fx = Fx::new("committed\n");
    common::write_file(fx.path(), "d/f.txt", "edited\n");
    fx.link();
    git_core::stage::discard_file(&fx.repo, "d/f.txt").unwrap();
    assert_eq!(fx.work(), "committed\n");
    assert_eq!(fx.other_content(), "edited\n");
}

#[test]
fn discarding_a_hunk_replaces_the_hardlinked_file() {
    let fx = Fx::new("1\n2\n3\n");
    common::write_file(fx.path(), "d/f.txt", "1\nX\n3\n");
    fx.link();
    git_core::stage::discard_hunk(&fx.repo, "d/f.txt", 1, 1, common::hunk_ctx(3)).unwrap();
    assert_eq!(fx.work(), "1\n2\n3\n");
    assert_eq!(fx.other_content(), "1\nX\n3\n");
}

#[test]
fn discarding_lines_replaces_the_hardlinked_file() {
    let fx = Fx::new("1\n2\n3\n");
    common::write_file(fx.path(), "d/f.txt", "1\nX\n3\nY\n");
    fx.link();
    let shown = fx.shown();
    // " 1", "-2", "+X", " 3", "+Y": discard the change of line 2 only.
    git_core::stage::discard_lines(
        &fx.repo,
        "d/f.txt",
        1,
        1,
        common::hunk_ctx(3),
        &[1, 2],
        &shown,
    )
    .unwrap();
    assert_eq!(fx.work(), "1\n2\n3\nY\n");
    assert_eq!(fx.other_content(), "1\nX\n3\nY\n");
}

#[test]
fn a_patch_that_does_not_apply_leaves_the_hard_link() {
    let fx = Fx::new("1\n2\n3\n");
    common::write_file(fx.path(), "d/f.txt", "1\nX\n3\n");
    fx.link();
    // The hunk as displayed earlier, before the file changed again: nothing is discarded.
    let stale = vec!["1".to_string(), "2".into(), "Z".into(), "3".into()];
    let _ = git_core::stage::discard_lines(
        &fx.repo,
        "d/f.txt",
        1,
        1,
        common::hunk_ctx(3),
        &[1],
        &stale,
    );
    assert_eq!(fx.work(), "1\nX\n3\n");
    fx.write_other_check();
}

#[test]
fn switching_branches_replaces_the_hardlinked_file() {
    let fx = Fx::new("main\n");
    fx.side("side\n");
    fx.link();
    git_core::branch::switch_branch(&fx.repo, "side").unwrap();
    assert_eq!(fx.work(), "side\n");
    assert_eq!(fx.other_content(), "main\n");
}

#[test]
fn a_refused_branch_switch_leaves_the_hard_link() {
    // A local change to another file the switch would overwrite: libgit2 refuses while
    // planning, before writing anything, and nothing is separated either (as git).
    let fx = Fx::new("main\n");
    let main = fx.main();
    git_core::branch::create_branch(&fx.repo, "side", "HEAD").unwrap();
    git_core::branch::switch_branch(&fx.repo, "side").unwrap();
    common::write_file(fx.path(), "d/f.txt", "side\n");
    common::write_file(fx.path(), "e.txt", "side e\n");
    common::commit_all(&fx.repo, "side");
    git_core::branch::switch_branch(&fx.repo, &main).unwrap();
    common::write_file(fx.path(), "e.txt", "my edit\n");
    fx.link();
    assert!(git_core::branch::switch_branch(&fx.repo, "side").is_err());
    assert_eq!(fx.work(), "main\n");
    fx.write_other_check();
}

#[test]
fn a_hard_reset_replaces_the_hardlinked_file() {
    let fx = Fx::new("one\n");
    let first = fx.repo.head().unwrap().target().unwrap();
    common::write_file(fx.path(), "d/f.txt", "two\n");
    common::commit_all(&fx.repo, "two");
    common::write_file(fx.path(), "d/f.txt", "dirty\n");
    fx.link();
    git_core::reset::reset_to(&fx.repo, &first.to_string(), ResetMode::Hard).unwrap();
    assert_eq!(fx.work(), "one\n");
    assert_eq!(fx.other_content(), "dirty\n");
}

#[test]
fn a_cherry_pick_replaces_the_hardlinked_file() {
    let fx = Fx::new("base\n");
    let side = fx.side("side\n");
    common::write_file(fx.path(), "main.txt", "main\n");
    common::commit_all(&fx.repo, "main work");
    fx.link();
    let outcome =
        git_core::pick::cherry_pick(&fx.repo, &side.to_string(), None, Default::default()).unwrap();
    assert!(
        matches!(outcome, git_core::pick::PickOutcome::Committed { .. }),
        "{outcome:?}"
    );
    assert_eq!(fx.work(), "side\n");
    assert_eq!(fx.other_content(), "base\n");
}

#[test]
fn merges_replace_the_hardlinked_file() {
    // A fast-forward, then a real merge (both sides changed).
    let fx = Fx::new("base\n");
    fx.side("side\n");
    fx.link();
    let outcome = git_core::merge::start_merge(&fx.repo, "side", MergeOptions::default()).unwrap();
    assert!(
        matches!(outcome, MergeOutcome::FastForwarded),
        "{outcome:?}"
    );
    assert_eq!(fx.work(), "side\n");
    assert_eq!(fx.other_content(), "base\n");

    let fx = Fx::new("base\n");
    fx.side("side\n");
    common::write_file(fx.path(), "main.txt", "main\n");
    common::commit_all(&fx.repo, "main work");
    fx.link();
    let outcome = git_core::merge::start_merge(&fx.repo, "side", MergeOptions::default()).unwrap();
    assert!(matches!(outcome, MergeOutcome::Merged), "{outcome:?}");
    assert_eq!(fx.work(), "side\n");
    assert_eq!(fx.other_content(), "base\n");
}

/// libgit2 writes a file with conflict markers through a lock file renamed over the name, which
/// replaces it already; this stays so.
#[test]
fn a_conflicted_merge_replaces_the_hardlinked_file() {
    let fx = Fx::new("base\n");
    fx.side("side\n");
    common::write_file(fx.path(), "d/f.txt", "main\n");
    common::commit_all(&fx.repo, "main work");
    fx.link();
    let outcome = git_core::merge::start_merge(&fx.repo, "side", MergeOptions::default()).unwrap();
    assert!(
        matches!(outcome, MergeOutcome::Conflicted { .. }),
        "{outcome:?}"
    );
    assert!(fx.work().contains("<<<<<<<"), "{}", fx.work());
    assert_eq!(fx.other_content(), "main\n");
}

#[test]
fn stash_save_and_apply_replace_the_hardlinked_file() {
    let fx = Fx::new("base\n");
    let mut repo = Repository::open(fx.path()).unwrap();
    common::write_file(fx.path(), "d/f.txt", "stashed\n");
    fx.link();
    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();
    assert_eq!(fx.work(), "base\n");
    assert_eq!(fx.other_content(), "stashed\n");

    fx.link();
    git_core::stash::apply_stash(&mut repo, 0, None, false).unwrap();
    assert_eq!(fx.work(), "stashed\n");
    assert_eq!(fx.other_content(), "base\n");
}

#[test]
fn undo_of_a_path_the_index_holds_replaces_the_hardlinked_file() {
    // `before` matches the index, so restoring it checks the file out from the index.
    let fx = Fx::new("committed\n");
    let before = capture(&fx.repo, SnapshotScope::Full).unwrap();
    common::write_file(fx.path(), "d/f.txt", "changed\n");
    fx.link();
    let after = capture(&fx.repo, SnapshotScope::Full).unwrap();
    restore(&fx.repo, &after, &before, "undo").unwrap();
    assert_eq!(fx.work(), "committed\n");
    assert_eq!(fx.other_content(), "changed\n");
}
