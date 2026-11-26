//! #1184: in a sparse checkout, Stash (all, without untracked files, keeping the index, a
//! file) leaves the files outside the cone alone, as git does: their skip-worktree entries are
//! unchanged, so the stash doesn't record them as deleted and its reset neither writes them nor
//! drops their marks. Apply and Pop then don't delete them either. The git CLI builds every
//! repository; a twin built the same way runs the same `git stash` command and is the
//! reference: the index with its marks, the status, the files on disk and the stash's trees.

mod common;

use std::path::Path;

use common::{git_ok, write_file};
use git_core::stash::{
    apply_stash, drop_stash, pop_stash, save_stash, StashApplyOutcome, StashOptions,
};

fn git_out(dir: &Path, args: &[&str]) -> String {
    let output = test_support::git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn ls_files_v(dir: &Path) -> String {
    git_out(dir, &["ls-files", "-v"])
}

fn status_short(dir: &Path) -> String {
    git_out(dir, &["status", "--short"])
}

/// Every file of the work tree but `.git`, with its content, sorted by path.
fn files(dir: &Path) -> Vec<(String, String)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap();
                let rel = rel.to_string_lossy().replace('\\', "/");
                out.push((rel, std::fs::read_to_string(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// The repository's index with its marks, its status, HEAD's tree and its files.
fn state(dir: &Path) -> (String, String, String, Vec<(String, String)>) {
    (
        git_out(dir, &["ls-files", "-v", "-s"]),
        status_short(dir),
        git_out(dir, &["rev-parse", "HEAD^{tree}"]),
        files(dir),
    )
}

/// The trees of each stash, newest first: its work-tree commit's, then its index and untracked
/// parents' (the commits themselves differ in time and message).
fn stash_trees(dir: &Path) -> Vec<Vec<String>> {
    let list = git_out(dir, &["stash", "list", "--format=%H"]);
    list.lines()
        .map(|stash| {
            let parents = git_out(dir, &["show", "-s", "--format=%P", stash]);
            std::iter::once(stash)
                .chain(parents.split_whitespace().skip(1))
                .map(|commit| {
                    git_out(dir, &["rev-parse", &format!("{commit}^{{tree}}")])
                        .trim()
                        .to_string()
                })
                .collect()
        })
        .collect()
}

/// Our repository `ours` ends as git left `twin`, its stashes included.
fn assert_same_as_git(ours: &Path, twin: &Path) {
    let (ours_state, twin_state) = (state(ours), state(twin));
    assert_eq!(ours_state.0, twin_state.0, "ls-files -v -s");
    assert_eq!(ours_state.1, twin_state.1, "status --short");
    assert_eq!(ours_state.2, twin_state.2, "HEAD's tree");
    assert_eq!(ours_state.3, twin_state.3, "files");
    assert_eq!(stash_trees(ours), stash_trees(twin), "stash trees");
}

/// A repository in `d` set up by the git CLI, with a user and no line-ending conversion.
fn init(d: &Path) {
    git_ok(d, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Test User"),
        ("user.email", "test@example.com"),
        ("core.autocrlf", "false"),
        ("commit.gpgSign", "false"),
    ] {
        git_ok(d, &["config", key, value]);
    }
}

fn commit(d: &Path, message: &str) {
    git_ok(d, &["add", "-A"]);
    git_ok(d, &["commit", "-q", "-m", message]);
}

const CONE_MARKS: &str = "H app/main.rs\nS docs/guide.md\nH lib/core/c.rs\nS lib/other/o.rs\n\
                          H lib/top.rs\nH top.txt\n";

/// `app/main.rs`, `docs/guide.md`, `lib/top.rs`, `lib/core/c.rs`, `lib/other/o.rs` and
/// `top.txt` committed on `main`; `other` adds `lib/core/new.rs`. Then `git sparse-checkout set
/// app lib/core` (cone mode): `docs/` and `lib/other/` are out.
fn cone(d: &Path) {
    init(d);
    for (path, content) in [
        ("app/main.rs", "fn main() {}\n"),
        ("docs/guide.md", "guide\n"),
        ("lib/top.rs", "top\n"),
        ("lib/core/c.rs", "core\n"),
        ("lib/other/o.rs", "other\n"),
        ("top.txt", "top\n"),
    ] {
        write_file(d, path, content);
    }
    commit(d, "one");
    git_ok(d, &["checkout", "-q", "-b", "other"]);
    write_file(d, "lib/core/new.rs", "new\n");
    commit(d, "other");
    git_ok(d, &["checkout", "-q", "main"]);
    git_ok(d, &["sparse-checkout", "set", "app", "lib/core"]);
    assert_eq!(ls_files_v(d), CONE_MARKS);
}

/// [`cone`], then local changes inside the cone: `app/main.rs` edited, `lib/top.rs` edited and
/// staged, `lib/core/c.rs` deleted, `app/u.rs` untracked.
fn cone_with_changes(d: &Path) {
    cone(d);
    write_file(d, "lib/top.rs", "top staged\n");
    git_ok(d, &["add", "lib/top.rs"]);
    write_file(d, "app/main.rs", "fn main() { run() }\n");
    std::fs::remove_file(d.join("lib/core/c.rs")).unwrap();
    write_file(d, "app/u.rs", "untracked\n");
}

fn twins(build: impl Fn(&Path)) -> (tempfile::TempDir, tempfile::TempDir) {
    let ours = tempfile::TempDir::new().unwrap();
    let twin = tempfile::TempDir::new().unwrap();
    build(ours.path());
    build(twin.path());
    (ours, twin)
}

fn open(dir: &Path) -> git2::Repository {
    git2::Repository::open(dir).unwrap()
}

fn stash_id(dir: &Path) -> String {
    git_out(dir, &["rev-parse", "stash@{0}"]).trim().to_string()
}

/// Neither outside file is on disk, both are still marked, and the status lists neither.
fn assert_outside_left_alone(d: &Path) {
    assert!(!d.join("docs/guide.md").exists());
    assert!(!d.join("lib/other/o.rs").exists());
    let marks = ls_files_v(d);
    assert!(marks.contains("S docs/guide.md\n"), "{marks}");
    assert!(marks.contains("S lib/other/o.rs\n"), "{marks}");
    let status = status_short(d);
    assert!(!status.contains("docs/"), "{status}");
    assert!(!status.contains("lib/other/"), "{status}");
}

#[test]
fn stash_all_in_a_sparse_checkout_matches_git() {
    let (ours, twin) = twins(cone_with_changes);
    git_ok(twin.path(), &["stash", "push", "-q", "-u"]);

    save_stash(&mut open(ours.path()), &StashOptions::default()).unwrap();

    assert_same_as_git(ours.path(), twin.path());
    assert_outside_left_alone(ours.path());
}

#[test]
fn stash_all_without_untracked_files_in_a_sparse_checkout_matches_git() {
    let (ours, twin) = twins(cone_with_changes);
    git_ok(twin.path(), &["stash", "push", "-q"]);
    let options = StashOptions {
        include_untracked: false,
        ..StashOptions::default()
    };

    save_stash(&mut open(ours.path()), &options).unwrap();

    assert_same_as_git(ours.path(), twin.path());
    assert_outside_left_alone(ours.path());
}

#[test]
fn stash_keeping_the_index_in_a_sparse_checkout_matches_git() {
    let (ours, twin) = twins(cone_with_changes);
    git_ok(twin.path(), &["stash", "push", "-q", "-u", "--keep-index"]);
    let options = StashOptions {
        keep_index: true,
        ..StashOptions::default()
    };

    save_stash(&mut open(ours.path()), &options).unwrap();

    assert_same_as_git(ours.path(), twin.path());
    assert_outside_left_alone(ours.path());
}

/// Stash file… on `app/main.rs` alone, without untracked files (the only staged change,
/// `lib/top.rs`, unstaged first: the path-limited stash's index commit holds only the picked
/// paths' entries, git's the whole index; and git's `-u` with paths writes an untracked commit
/// even when no untracked file matches).
#[test]
fn stashing_one_file_in_a_sparse_checkout_matches_git() {
    let (ours, twin) = twins(|d| {
        cone_with_changes(d);
        git_ok(d, &["reset", "-q", "--", "lib/top.rs"]);
    });
    git_ok(twin.path(), &["stash", "push", "-q", "--", "app/main.rs"]);
    let options = StashOptions {
        paths: vec!["app/main.rs".to_string()],
        include_untracked: false,
        ..StashOptions::default()
    };

    save_stash(&mut open(ours.path()), &options).unwrap();

    assert_same_as_git(ours.path(), twin.path());
    assert_outside_left_alone(ours.path());
}

#[test]
fn popping_a_stash_in_a_sparse_checkout_matches_git() {
    let (ours, twin) = twins(cone_with_changes);
    git_ok(twin.path(), &["stash", "push", "-q", "-u"]);
    git_ok(twin.path(), &["stash", "pop", "-q"]);
    let mut repo = open(ours.path());
    save_stash(&mut repo, &StashOptions::default()).unwrap();
    let id = stash_id(ours.path());

    let outcome = pop_stash(&mut repo, 0, Some(&id), false).unwrap();

    assert_eq!(outcome, StashApplyOutcome::Applied);
    assert_same_as_git(ours.path(), twin.path());
    assert_outside_left_alone(ours.path());
}

#[test]
fn applying_a_stash_with_its_staged_changes_in_a_sparse_checkout_matches_git() {
    let (ours, twin) = twins(cone_with_changes);
    git_ok(twin.path(), &["stash", "push", "-q", "-u"]);
    git_ok(twin.path(), &["stash", "apply", "-q", "--index"]);
    let mut repo = open(ours.path());
    save_stash(&mut repo, &StashOptions::default()).unwrap();
    let id = stash_id(ours.path());

    let outcome = apply_stash(&mut repo, 0, Some(&id), true).unwrap();

    assert_eq!(outcome, StashApplyOutcome::Applied);
    assert_same_as_git(ours.path(), twin.path());
    assert_outside_left_alone(ours.path());
}

#[test]
fn dropping_a_stash_after_applying_it_in_a_sparse_checkout_matches_git() {
    let (ours, twin) = twins(cone_with_changes);
    git_ok(twin.path(), &["stash", "push", "-q", "-u"]);
    git_ok(twin.path(), &["stash", "apply", "-q"]);
    git_ok(twin.path(), &["stash", "drop", "-q"]);
    let mut repo = open(ours.path());
    save_stash(&mut repo, &StashOptions::default()).unwrap();
    let id = stash_id(ours.path());
    assert_eq!(
        apply_stash(&mut repo, 0, Some(&id), false).unwrap(),
        StashApplyOutcome::Applied
    );

    drop_stash(&mut repo, 0, Some(&id)).unwrap();

    assert_same_as_git(ours.path(), twin.path());
    assert_outside_left_alone(ours.path());
}

/// A stash git made in the sparse checkout, popped by Browsitory and by git.
#[test]
fn popping_a_stash_git_made_in_a_sparse_checkout_matches_git() {
    let (ours, twin) = twins(|d| {
        cone_with_changes(d);
        git_ok(d, &["stash", "push", "-q", "-u"]);
    });
    git_ok(twin.path(), &["stash", "pop", "-q", "--index"]);
    let id = stash_id(ours.path());

    let outcome = pop_stash(&mut open(ours.path()), 0, Some(&id), true).unwrap();

    assert_eq!(outcome, StashApplyOutcome::Applied);
    assert_same_as_git(ours.path(), twin.path());
    assert_outside_left_alone(ours.path());
}

/// [`cone_with_changes`] with non-cone patterns (`/app/`, `/lib/top.rs`, `/lib/core/`,
/// `/top.txt`) that leave the same files out.
fn non_cone_with_changes(d: &Path) {
    cone(d);
    git_ok(d, &["sparse-checkout", "disable"]);
    git_ok(d, &["config", "core.sparseCheckoutCone", "false"]);
    git_ok(
        d,
        &[
            "sparse-checkout",
            "set",
            "--no-cone",
            "/app/",
            "/lib/top.rs",
            "/lib/core/",
            "/top.txt",
        ],
    );
    assert_eq!(ls_files_v(d), CONE_MARKS);
    write_file(d, "lib/top.rs", "top staged\n");
    git_ok(d, &["add", "lib/top.rs"]);
    write_file(d, "app/main.rs", "fn main() { run() }\n");
    std::fs::remove_file(d.join("lib/core/c.rs")).unwrap();
    write_file(d, "app/u.rs", "untracked\n");
}

#[test]
fn stash_all_and_pop_with_non_cone_patterns_leave_the_files_outside_alone() {
    let (ours, twin) = twins(non_cone_with_changes);
    git_ok(twin.path(), &["stash", "push", "-q", "-u"]);
    let mut repo = open(ours.path());

    save_stash(&mut repo, &StashOptions::default()).unwrap();

    assert_same_as_git(ours.path(), twin.path());
    assert_outside_left_alone(ours.path());
    git_ok(twin.path(), &["stash", "pop", "-q"]);
    let id = stash_id(ours.path());
    assert_eq!(
        pop_stash(&mut repo, 0, Some(&id), false).unwrap(),
        StashApplyOutcome::Applied
    );
    assert_same_as_git(ours.path(), twin.path());
    assert_outside_left_alone(ours.path());
}

/// Stash and switch (#653) is the stash operation Undo records: it stashes everything, switches
/// and pops the stash with its staged changes. The result matches git's `stash push -u`,
/// `switch`, `stash pop --index`, and Undo brings back the state before it, the files outside
/// the cone still missing and marked.
#[test]
fn stash_and_switch_and_its_undo_in_a_sparse_checkout_leave_the_files_outside_alone() {
    let (ours, twin) = twins(cone_with_changes);
    git_ok(twin.path(), &["stash", "push", "-q", "-u"]);
    git_ok(twin.path(), &["switch", "-q", "other"]);
    git_ok(twin.path(), &["stash", "pop", "-q", "--index"]);
    let d = ours.path();
    let mut repo = open(d);
    let before_state = state(d);
    let before = git_core::undo::capture(&repo, git_core::undo::SnapshotScope::Full).unwrap();

    let outcome = git_core::branch::stash_and_switch(&mut repo, "other", true).unwrap();

    assert_eq!(outcome, git_core::branch::StashSwitchOutcome::Reapplied);
    assert_same_as_git(d, twin.path());
    assert_outside_left_alone(d);
    let after = git_core::undo::capture(&repo, git_core::undo::SnapshotScope::Full).unwrap();
    git_core::undo::restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(state(d), before_state);
    assert_outside_left_alone(d);
}

/// [`cone_with_changes`], and `docs/guide.md`, outside the cone, has a staged change: its entry
/// holds another blob and is still marked, its file missing.
fn staged_change_outside(d: &Path) {
    cone_with_changes(d);
    write_file(d, "blob.tmp", "staged guide\n");
    let blob = git_out(d, &["hash-object", "-w", "blob.tmp"]);
    std::fs::remove_file(d.join("blob.tmp")).unwrap();
    let info = format!("100644,{},docs/guide.md", blob.trim());
    git_ok(d, &["update-index", "--cacheinfo", &info]);
    git_ok(d, &["update-index", "--skip-worktree", "docs/guide.md"]);
    assert!(ls_files_v(d).contains("S docs/guide.md\n"));
    assert!(!d.join("docs/guide.md").exists());
}

#[test]
fn stash_all_and_pop_of_a_staged_change_outside_the_cone_match_git() {
    let (ours, twin) = twins(staged_change_outside);
    git_ok(twin.path(), &["stash", "push", "-q", "-u"]);
    let mut repo = open(ours.path());

    save_stash(&mut repo, &StashOptions::default()).unwrap();

    assert_same_as_git(ours.path(), twin.path());
    git_ok(twin.path(), &["stash", "pop", "-q", "--index"]);
    let id = stash_id(ours.path());
    assert_eq!(
        pop_stash(&mut repo, 0, Some(&id), true).unwrap(),
        StashApplyOutcome::Applied
    );
    assert_same_as_git(ours.path(), twin.path());
}

/// [`cone_with_changes`], and `docs/new.md`, outside the cone, is a staged new file, marked,
/// its file missing.
fn staged_new_file_outside(d: &Path) {
    cone_with_changes(d);
    write_file(d, "blob.tmp", "new guide\n");
    let blob = git_out(d, &["hash-object", "-w", "blob.tmp"]);
    std::fs::remove_file(d.join("blob.tmp")).unwrap();
    let info = format!("100644,{},docs/new.md", blob.trim());
    git_ok(d, &["update-index", "--add", "--cacheinfo", &info]);
    git_ok(d, &["update-index", "--skip-worktree", "docs/new.md"]);
    assert!(ls_files_v(d).contains("S docs/new.md\n"));
    assert!(!d.join("docs/new.md").exists());
}

#[test]
fn stash_all_and_pop_of_a_staged_new_file_outside_the_cone_match_git() {
    let (ours, twin) = twins(staged_new_file_outside);
    git_ok(twin.path(), &["stash", "push", "-q", "-u"]);
    let mut repo = open(ours.path());

    save_stash(&mut repo, &StashOptions::default()).unwrap();

    assert_same_as_git(ours.path(), twin.path());
    git_ok(twin.path(), &["stash", "pop", "-q", "--index"]);
    let id = stash_id(ours.path());
    assert_eq!(
        pop_stash(&mut repo, 0, Some(&id), true).unwrap(),
        StashApplyOutcome::Applied
    );
    assert_same_as_git(ours.path(), twin.path());
}

/// No sparse checkout: `top.txt` marked with `git update-index --skip-worktree`, then edited,
/// beside an edit to `app/main.rs`. git's stash takes the marked file for unchanged.
fn marked_by_hand(d: &Path) {
    cone(d);
    git_ok(d, &["sparse-checkout", "disable"]);
    git_ok(d, &["update-index", "--skip-worktree", "top.txt"]);
    write_file(d, "top.txt", "top edited\n");
    write_file(d, "app/main.rs", "fn main() { run() }\n");
}

#[test]
fn stash_all_and_pop_with_a_file_marked_skip_worktree_by_hand_match_git() {
    let (ours, twin) = twins(marked_by_hand);
    git_ok(twin.path(), &["stash", "push", "-q", "-u"]);
    let mut repo = open(ours.path());

    save_stash(&mut repo, &StashOptions::default()).unwrap();

    assert_same_as_git(ours.path(), twin.path());
    git_ok(twin.path(), &["stash", "pop", "-q"]);
    let id = stash_id(ours.path());
    assert_eq!(
        pop_stash(&mut repo, 0, Some(&id), false).unwrap(),
        StashApplyOutcome::Applied
    );
    assert_same_as_git(ours.path(), twin.path());
}

/// Pops `stash@{0}` in `ours` (restoring its staged changes or not) and with `git stash pop`
/// in `twin`.
fn pop_both(ours: &Path, twin: &Path, restore_index: bool) -> StashApplyOutcome {
    let mut args = vec!["stash", "pop", "-q"];
    if restore_index {
        args.push("--index");
    }
    git_ok(twin, &args);
    let id = stash_id(ours);
    pop_stash(&mut open(ours), 0, Some(&id), restore_index).unwrap()
}

/// [`cone`] before the sparse checkout: `docs/guide.md` and `app/main.rs` edited and
/// `docs/untracked.md` added, stashed with `git stash push -u`; then `git sparse-checkout set app
/// lib/core`. The stash changes a file outside the cone.
fn stashed_before_the_sparse_checkout(d: &Path) {
    cone(d);
    git_ok(d, &["sparse-checkout", "disable"]);
    write_file(d, "docs/guide.md", "guide edited\n");
    write_file(d, "app/main.rs", "main edited\n");
    write_file(d, "docs/untracked.md", "untracked\n");
    git_ok(d, &["stash", "push", "-q", "-u"]);
    git_ok(d, &["sparse-checkout", "set", "app", "lib/core"]);
    assert_eq!(ls_files_v(d), CONE_MARKS);
}

/// git writes the stashed edit of the file outside the cone and drops its mark; libgit2's apply
/// left it out, and Pop then dropped the stash with it.
#[test]
fn popping_a_stash_made_before_the_sparse_checkout_matches_git() {
    for restore_index in [false, true] {
        let (ours, twin) = twins(stashed_before_the_sparse_checkout);

        let outcome = pop_both(ours.path(), twin.path(), restore_index);

        assert_eq!(outcome, StashApplyOutcome::Applied);
        assert_same_as_git(ours.path(), twin.path());
        assert_eq!(
            std::fs::read_to_string(ours.path().join("docs/guide.md")).unwrap(),
            "guide edited\n"
        );
    }
}

/// [`cone`] before the sparse checkout: `docs/guide.md` deleted and `app/main.rs` edited,
/// stashed; then the sparse checkout.
fn deletion_stashed_before_the_sparse_checkout(d: &Path) {
    cone(d);
    git_ok(d, &["sparse-checkout", "disable"]);
    std::fs::remove_file(d.join("docs/guide.md")).unwrap();
    write_file(d, "app/main.rs", "main edited\n");
    git_ok(d, &["stash", "push", "-q", "-u"]);
    git_ok(d, &["sparse-checkout", "set", "app", "lib/core"]);
}

#[test]
fn popping_a_stashed_deletion_of_a_file_outside_the_cone_matches_git() {
    for restore_index in [false, true] {
        let (ours, twin) = twins(deletion_stashed_before_the_sparse_checkout);

        let outcome = pop_both(ours.path(), twin.path(), restore_index);

        assert_eq!(outcome, StashApplyOutcome::Applied);
        assert_same_as_git(ours.path(), twin.path());
    }
}

/// [`cone`] before the sparse checkout: `docs/guide.md` changed and staged, then deleted;
/// `docs/new.md` added and staged; with `edit_other`, `lib/other/o.rs` edited (not staged).
/// Stashed, then the sparse checkout.
fn staged_changes_stashed_before_the_sparse_checkout(d: &Path, edit_other: bool) {
    cone(d);
    git_ok(d, &["sparse-checkout", "disable"]);
    write_file(d, "docs/guide.md", "guide staged\n");
    write_file(d, "docs/new.md", "new staged\n");
    git_ok(d, &["add", "docs"]);
    std::fs::remove_file(d.join("docs/guide.md")).unwrap();
    if edit_other {
        write_file(d, "lib/other/o.rs", "other edited\n");
    }
    git_ok(d, &["stash", "push", "-q", "-u"]);
    git_ok(d, &["sparse-checkout", "set", "app", "lib/core"]);
}

#[test]
fn popping_staged_changes_outside_the_cone_matches_git() {
    for restore_index in [false, true] {
        let (ours, twin) = twins(|d| staged_changes_stashed_before_the_sparse_checkout(d, false));

        let outcome = pop_both(ours.path(), twin.path(), restore_index);

        assert_eq!(outcome, StashApplyOutcome::Applied);
        assert_same_as_git(ours.path(), twin.path());
    }
}

/// Without restoring the staged changes, git writes the unstaged edit of `lib/other/o.rs`.
/// With them, git leaves it out (the file stays marked and missing, the edit is gone);
/// Browsitory writes it and drops the mark, as without them, and otherwise matches git.
#[test]
fn popping_staged_changes_keeps_an_unstaged_edit_outside_the_cone() {
    for restore_index in [false, true] {
        let (ours, twin) = twins(|d| staged_changes_stashed_before_the_sparse_checkout(d, true));

        let outcome = pop_both(ours.path(), twin.path(), restore_index);

        assert_eq!(outcome, StashApplyOutcome::Applied);
        if restore_index {
            assert!(ls_files_v(twin.path()).contains("S lib/other/o.rs\n"));
            write_file(twin.path(), "lib/other/o.rs", "other edited\n");
            git_ok(
                twin.path(),
                &["update-index", "--no-skip-worktree", "lib/other/o.rs"],
            );
        }
        assert_same_as_git(ours.path(), twin.path());
        assert_eq!(
            std::fs::read_to_string(ours.path().join("lib/other/o.rs")).unwrap(),
            "other edited\n"
        );
    }
}

/// A staged change to `docs/guide.md` outside the cone, stashed in the sparse checkout and
/// popped without its staged changes: git writes the file with the staged content, unmarked.
#[test]
fn popping_a_staged_change_outside_the_cone_as_unstaged_matches_git() {
    let (ours, twin) = twins(|d| {
        staged_change_outside(d);
        git_ok(d, &["stash", "push", "-q", "-u"]);
    });

    let outcome = pop_both(ours.path(), twin.path(), false);

    assert_eq!(outcome, StashApplyOutcome::Applied);
    assert_same_as_git(ours.path(), twin.path());
}

/// [`cone`] before the sparse checkout: `docs/guide.md` changed and staged, then its file put
/// back to the committed version, and `app/main.rs` edited; stashed, then the sparse checkout.
/// The stash changes `docs/guide.md` only in its index tree.
fn staged_change_reverted_before_the_sparse_checkout(d: &Path) {
    cone(d);
    git_ok(d, &["sparse-checkout", "disable"]);
    write_file(d, "docs/guide.md", "guide staged\n");
    git_ok(d, &["add", "docs/guide.md"]);
    write_file(d, "docs/guide.md", "guide\n");
    write_file(d, "app/main.rs", "main edited\n");
    git_ok(d, &["stash", "push", "-q", "-u"]);
    git_ok(d, &["sparse-checkout", "set", "app", "lib/core"]);
}

#[test]
fn popping_a_staged_change_its_work_tree_reverted_outside_the_cone_matches_git() {
    let (ours, twin) = twins(staged_change_reverted_before_the_sparse_checkout);

    let outcome = pop_both(ours.path(), twin.path(), true);

    assert_eq!(outcome, StashApplyOutcome::Applied);
    assert_same_as_git(ours.path(), twin.path());
}

/// A marked file outside the cone that is on disk with content of its own (another tool wrote
/// it) and that the stash changes: Pop refuses, as for any local edit, instead of writing the
/// stashed version over it, and changes nothing.
#[test]
fn a_pop_refuses_to_overwrite_a_marked_file_on_disk_outside_the_cone() {
    let dir = tempfile::TempDir::new().unwrap();
    let d = dir.path();
    stashed_before_the_sparse_checkout(d);
    write_file(d, "docs/guide.md", "written by another tool\n");
    // The index file keeps the mark until a git command rewrites it (git takes a file that is
    // there for unmarked: `ls-files -v` shows `H`, and `status` writes the index without the
    // mark), so the mark is read from the index file and `status` runs only at the end.
    let marked = |d: &Path| {
        let index = git2::Index::open(&d.join(".git/index")).unwrap();
        let entry = index.get_path(Path::new("docs/guide.md"), 0).unwrap();
        entry.flags_extended & git2::IndexEntryExtendedFlag::SKIP_WORKTREE.bits() != 0
    };
    assert!(marked(d));
    let index_before = git_out(d, &["ls-files", "-s"]);
    let files_before = files(d);
    let stashes = stash_trees(d);
    let id = stash_id(d);

    let refused = pop_stash(&mut open(d), 0, Some(&id), false);

    assert!(
        matches!(refused, Err(git_core::stash::StashError::WouldOverwrite)),
        "{refused:?}"
    );
    assert!(marked(d));
    assert_eq!(git_out(d, &["ls-files", "-s"]), index_before);
    assert_eq!(files(d), files_before);
    assert_eq!(stash_trees(d), stashes);
}

/// A staged new file inside the cone comes back staged and on disk: only paths outside the
/// sparse patterns stay outside.
#[test]
fn popping_a_staged_new_file_inside_the_cone_matches_git() {
    let (ours, twin) = twins(|d| {
        cone_with_changes(d);
        write_file(d, "app/new.rs", "new app\n");
        git_ok(d, &["add", "app/new.rs"]);
        git_ok(d, &["stash", "push", "-q", "-u"]);
    });

    let outcome = pop_both(ours.path(), twin.path(), true);

    assert_eq!(outcome, StashApplyOutcome::Applied);
    assert_same_as_git(ours.path(), twin.path());
    assert!(ours.path().join("app/new.rs").exists());
}

/// [`cone`] before the sparse checkout: `docs/guide.md` and `app/main.rs` edited and stashed;
/// then `docs/guide.md` changed and committed; then the sparse checkout. Popping conflicts on
/// `docs/guide.md`, which git writes with conflict markers.
fn conflicting_stash_outside_the_cone(d: &Path) {
    cone(d);
    git_ok(d, &["sparse-checkout", "disable"]);
    write_file(d, "docs/guide.md", "guide stashed\n");
    write_file(d, "app/main.rs", "main stashed\n");
    git_ok(d, &["stash", "push", "-q", "-u"]);
    write_file(d, "docs/guide.md", "guide committed\n");
    commit(d, "guide");
    git_ok(d, &["sparse-checkout", "set", "app", "lib/core"]);
}

#[test]
fn a_stash_conflicting_outside_the_cone_conflicts_as_with_git() {
    let (ours, twin) = twins(conflicting_stash_outside_the_cone);
    let output = test_support::git(twin.path(), &["stash", "pop", "-q"]);
    assert!(!output.status.success(), "{output:?}");
    let id = stash_id(ours.path());

    let outcome = pop_stash(&mut open(ours.path()), 0, Some(&id), false).unwrap();

    assert_eq!(
        outcome,
        StashApplyOutcome::Conflicted {
            files: vec!["docs/guide.md".to_string()]
        }
    );
    assert_same_as_git(ours.path(), twin.path());
}

/// A refused Pop (a local edit to `app/main.rs`, which the stash changes too) changes nothing:
/// the file outside the cone the stash changes stays marked and missing, and the stash is kept.
#[test]
fn a_refused_pop_leaves_the_files_outside_the_cone_alone() {
    let dir = tempfile::TempDir::new().unwrap();
    let d = dir.path();
    stashed_before_the_sparse_checkout(d);
    write_file(d, "app/main.rs", "local edit\n");
    let before = state(d);
    let stashes = stash_trees(d);
    let id = stash_id(d);

    let refused = pop_stash(&mut open(d), 0, Some(&id), false);

    assert!(
        matches!(refused, Err(git_core::stash::StashError::WouldOverwrite)),
        "{refused:?}"
    );
    assert_eq!(state(d), before);
    assert_eq!(stash_trees(d), stashes);
    assert_outside_left_alone(d);
}
