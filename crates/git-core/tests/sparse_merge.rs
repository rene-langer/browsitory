//! #1185: in a sparse checkout, Merge, Cherry-pick, Revert and Rebase apply a commit that
//! changes, adds or deletes files outside the cone as git does: a changed file keeps its
//! skip-worktree mark with the new blob in the index, a new one is marked, and neither is
//! written. A conflict in such a file is written with its markers, unmarked. The git CLI builds
//! every repository; a twin built the same way runs the same git command and is the reference:
//! the index with its marks, the status, HEAD's tree and the files on disk.

mod common;

use std::path::Path;

use common::{git_ok, write_file};
use git_core::hooks::HookOptions;
use git_core::merge::{start_merge, FastForward, MergeOptions, MergeOutcome};
use git_core::pick::{abort_sequence, cherry_pick, cherry_pick_many, revert, PickOutcome};
use git_core::rebase::{start_rebase, RebaseAction, RebasePlanEntry, RebaseStepResult};

fn git_out(dir: &Path, args: &[&str]) -> String {
    let output = test_support::git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// `git` in `dir`, which may fail (a conflict).
fn git_any(dir: &Path, args: &[&str]) {
    test_support::git(dir, args);
}

fn ls_files_v(dir: &Path) -> String {
    git_out(dir, &["ls-files", "-v"])
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
        git_out(dir, &["status", "--short"]),
        git_out(dir, &["rev-parse", "HEAD^{tree}"]),
        files(dir),
    )
}

/// Our repository `ours` ends as git left `twin`.
fn assert_same_as_git(ours: &Path, twin: &Path) {
    let (ours, twin) = (state(ours), state(twin));
    assert_eq!(ours.0, twin.0, "ls-files -v -s");
    assert_eq!(ours.1, twin.1, "status --short");
    assert_eq!(ours.2, twin.2, "HEAD's tree");
    assert_eq!(ours.3, twin.3, "files");
}

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

const MARKS: &str =
    "H app/main.rs\nS docs/clash.md\nS docs/guide.md\nS lib/other/o.rs\nH top.txt\n";

/// `app/main.rs`, `docs/guide.md`, `docs/clash.md`, `lib/other/o.rs` and `top.txt` committed
/// ("base"). From there:
///
/// - `side` changes `docs/guide.md`, adds `docs/new.md` and deletes `lib/other/o.rs`;
/// - `clash` changes `docs/clash.md`;
/// - `inside` changes `app/main.rs`;
/// - `grow` adds `app/added.rs` and `docs/more.md`;
/// - `main` changes `app/main.rs` and `docs/clash.md` (so `clash` and `inside` conflict with it);
/// - `ahead` is `main` plus `side`'s changes (a fast-forward from `main`).
///
/// Then `git sparse-checkout set app` (cone mode) on `main`: `docs/` and `lib/` are out.
fn build(d: &Path) {
    init(d);
    for (path, content) in [
        ("app/main.rs", "fn main() {}\n"),
        ("docs/guide.md", "guide\n"),
        ("docs/clash.md", "clash\n"),
        ("lib/other/o.rs", "other\n"),
        ("top.txt", "top\n"),
    ] {
        write_file(d, path, content);
    }
    commit(d, "base");
    git_ok(d, &["checkout", "-q", "-b", "side"]);
    side_changes(d);
    commit(d, "side");
    git_ok(d, &["checkout", "-q", "-b", "clash", "main"]);
    write_file(d, "docs/clash.md", "clash from clash\n");
    commit(d, "clash");
    git_ok(d, &["checkout", "-q", "-b", "inside", "main"]);
    write_file(d, "app/main.rs", "fn main() { inside() }\n");
    commit(d, "inside");
    git_ok(d, &["checkout", "-q", "-b", "grow", "main"]);
    write_file(d, "app/added.rs", "added\n");
    write_file(d, "docs/more.md", "more\n");
    commit(d, "grow");
    git_ok(d, &["checkout", "-q", "main"]);
    write_file(d, "app/main.rs", "fn main() { run() }\n");
    write_file(d, "docs/clash.md", "clash from main\n");
    commit(d, "main");
    git_ok(d, &["checkout", "-q", "-b", "ahead"]);
    side_changes(d);
    commit(d, "ahead");
    git_ok(d, &["checkout", "-q", "main"]);
    git_ok(d, &["sparse-checkout", "set", "app"]);
    assert_eq!(ls_files_v(d), MARKS);
}

fn side_changes(d: &Path) {
    write_file(d, "docs/guide.md", "guide from side\n");
    write_file(d, "docs/new.md", "new\n");
    std::fs::remove_file(d.join("lib/other/o.rs")).unwrap();
}

fn twins() -> (tempfile::TempDir, tempfile::TempDir) {
    let ours = tempfile::TempDir::new().unwrap();
    let twin = tempfile::TempDir::new().unwrap();
    build(ours.path());
    build(twin.path());
    (ours, twin)
}

fn open(dir: &Path) -> git2::Repository {
    git2::Repository::open(dir).unwrap()
}

fn id(dir: &Path, rev: &str) -> String {
    git_out(dir, &["rev-parse", rev]).trim().to_string()
}

/// Nothing outside the cone is on disk.
fn assert_nothing_written_outside(d: &Path) {
    assert!(!d.join("docs").exists(), "{:?}", files(d));
    assert!(!d.join("lib").exists(), "{:?}", files(d));
}

#[test]
fn a_merge_that_changes_adds_and_deletes_files_outside_the_cone_matches_git() {
    let (ours, twin) = twins();
    git_ok(twin.path(), &["merge", "-q", "--no-commit", "side"]);

    let outcome = start_merge(&open(ours.path()), "side", MergeOptions::default()).unwrap();

    assert_eq!(outcome, MergeOutcome::Merged);
    assert_same_as_git(ours.path(), twin.path());
    assert_nothing_written_outside(ours.path());
    assert!(ls_files_v(ours.path()).contains("S docs/new.md\n"));
}

#[test]
fn a_fast_forward_outside_the_cone_matches_git() {
    let (ours, twin) = twins();
    git_ok(twin.path(), &["merge", "-q", "--ff-only", "ahead"]);

    let outcome = start_merge(&open(ours.path()), "ahead", MergeOptions::default()).unwrap();

    assert_eq!(outcome, MergeOutcome::FastForwarded);
    assert_same_as_git(ours.path(), twin.path());
    assert_nothing_written_outside(ours.path());
}

#[test]
fn a_merge_commit_where_a_fast_forward_was_possible_matches_git() {
    let (ours, twin) = twins();
    git_ok(
        twin.path(),
        &["merge", "-q", "--no-ff", "--no-commit", "ahead"],
    );
    let options = MergeOptions {
        fast_forward: FastForward::Never,
        squash: false,
    };

    let outcome = start_merge(&open(ours.path()), "ahead", options).unwrap();

    assert_eq!(outcome, MergeOutcome::Merged);
    assert_same_as_git(ours.path(), twin.path());
    assert_nothing_written_outside(ours.path());
}

#[test]
fn a_squash_merge_outside_the_cone_matches_git() {
    let (ours, twin) = twins();
    git_ok(twin.path(), &["merge", "-q", "--squash", "side"]);
    let options = MergeOptions {
        fast_forward: FastForward::Allow,
        squash: true,
    };

    let outcome = start_merge(&open(ours.path()), "side", options).unwrap();

    assert_eq!(outcome, MergeOutcome::Squashed);
    assert_same_as_git(ours.path(), twin.path());
    assert_nothing_written_outside(ours.path());
}

/// git writes the conflicted file, with its markers, and drops the mark.
#[test]
fn a_merge_conflict_outside_the_cone_is_written_unmarked_as_git_does() {
    let (ours, twin) = twins();
    git_any(twin.path(), &["merge", "-q", "--no-commit", "clash"]);

    let outcome = start_merge(&open(ours.path()), "clash", MergeOptions::default()).unwrap();

    assert_eq!(
        outcome,
        MergeOutcome::Conflicted {
            files: vec!["docs/clash.md".to_string()]
        }
    );
    assert_same_as_git(ours.path(), twin.path());
    assert!(!ours.path().join("docs/guide.md").exists());
}

#[test]
fn a_cherry_pick_that_changes_adds_and_deletes_files_outside_the_cone_matches_git() {
    let (ours, twin) = twins();
    git_ok(twin.path(), &["cherry-pick", "side"]);

    let outcome = cherry_pick(
        &open(ours.path()),
        &id(ours.path(), "side"),
        None,
        HookOptions::default(),
    )
    .unwrap();

    assert!(
        matches!(outcome, PickOutcome::Committed { .. }),
        "{outcome:?}"
    );
    assert_same_as_git(ours.path(), twin.path());
    assert_nothing_written_outside(ours.path());
}

#[test]
fn a_revert_of_a_change_outside_the_cone_matches_git() {
    let (ours, twin) = twins();
    git_ok(twin.path(), &["revert", "--no-edit", "HEAD"]);

    let outcome = revert(
        &open(ours.path()),
        &id(ours.path(), "HEAD"),
        None,
        HookOptions::default(),
    )
    .unwrap();

    assert!(
        matches!(outcome, PickOutcome::Committed { .. }),
        "{outcome:?}"
    );
    assert_same_as_git(ours.path(), twin.path());
    assert_nothing_written_outside(ours.path());
}

/// git's cherry-pick leaves a conflicted file outside the cone unwritten (its merge writes it):
/// it is written here, with its markers and unmarked, as a merge does, so it can be resolved.
/// The index is git's.
#[test]
fn a_cherry_pick_conflict_outside_the_cone_is_written_unmarked() {
    let (ours, twin) = twins();
    git_any(twin.path(), &["cherry-pick", "clash"]);

    let outcome = cherry_pick(
        &open(ours.path()),
        &id(ours.path(), "clash"),
        None,
        HookOptions::default(),
    )
    .unwrap();

    assert_eq!(
        outcome,
        PickOutcome::Conflicted {
            files: vec!["docs/clash.md".to_string()]
        }
    );
    let (ours_state, twin_state) = (state(ours.path()), state(twin.path()));
    assert_eq!(ours_state.0, twin_state.0, "ls-files -v -s");
    assert_eq!(ours_state.1, twin_state.1, "status --short");
    assert_eq!(ours_state.2, twin_state.2, "HEAD's tree");
    let clash = std::fs::read_to_string(ours.path().join("docs/clash.md")).unwrap();
    assert!(clash.contains("<<<<<<< "), "{clash}");
    assert!(clash.contains("clash from main\n"), "{clash}");
    assert!(clash.contains("clash from clash\n"), "{clash}");
    assert!(!ours.path().join("docs/guide.md").exists());
}

/// A rebase of `side` onto `main`: the checkout of `main` changes `docs/clash.md`, the step
/// changes, adds and deletes files outside the cone.
#[test]
fn a_rebase_whose_step_changes_files_outside_the_cone_matches_git() {
    let (ours, twin) = twins();
    for d in [ours.path(), twin.path()] {
        git_ok(d, &["switch", "-q", "side"]);
    }
    git_ok(twin.path(), &["rebase", "-q", "main"]);
    let plan = vec![RebasePlanEntry {
        commit_id: id(ours.path(), "side"),
        action: RebaseAction::Pick,
        combined_message: None,
    }];

    let (_, result) = start_rebase(&open(ours.path()), "main", plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_same_as_git(ours.path(), twin.path());
    assert_nothing_written_outside(ours.path());
}

/// A cherry-pick of `side` (outside the cone) then `inside` (a conflict inside), aborted: the
/// rewind to where it started undoes `side`'s changes without writing them.
#[test]
fn aborting_a_cherry_pick_sequence_outside_the_cone_matches_git() {
    let (ours, twin) = twins();
    git_any(twin.path(), &["cherry-pick", "side", "inside"]);
    git_ok(twin.path(), &["cherry-pick", "--abort"]);
    let repo = open(ours.path());
    let ids = [id(ours.path(), "side"), id(ours.path(), "inside")];
    let state = cherry_pick_many(&repo, &ids, HookOptions::default())
        .state
        .unwrap();

    let abort = abort_sequence(&repo, &state).unwrap();

    assert!(abort.rewound);
    assert_same_as_git(ours.path(), twin.path());
    assert_eq!(ls_files_v(ours.path()), MARKS);
    assert_nothing_written_outside(ours.path());
}

/// Non-cone patterns aren't matched: a path new to the index is outside them when its file is
/// missing, so `docs/new.md` is marked as git marks it here.
#[test]
fn a_merge_with_non_cone_patterns_matches_git() {
    let (ours, twin) = twins();
    for d in [ours.path(), twin.path()] {
        git_ok(d, &["sparse-checkout", "set", "--no-cone", "/app/"]);
        assert_eq!(
            ls_files_v(d),
            "H app/main.rs\nS docs/clash.md\nS docs/guide.md\nS lib/other/o.rs\nS top.txt\n"
        );
    }
    git_ok(twin.path(), &["merge", "-q", "--no-commit", "side"]);

    let outcome = start_merge(&open(ours.path()), "side", MergeOptions::default()).unwrap();

    assert_eq!(outcome, MergeOutcome::Merged);
    assert_same_as_git(ours.path(), twin.path());
    assert_nothing_written_outside(ours.path());
}

/// A new file inside the cone is written, unmarked; one outside is marked and not written.
#[test]
fn a_merge_that_adds_files_inside_and_outside_the_cone_matches_git() {
    let (ours, twin) = twins();
    git_ok(twin.path(), &["merge", "-q", "--no-commit", "grow"]);

    let outcome = start_merge(&open(ours.path()), "grow", MergeOptions::default()).unwrap();

    assert_eq!(outcome, MergeOutcome::Merged);
    assert_same_as_git(ours.path(), twin.path());
    assert!(ours.path().join("app/added.rs").exists());
    assert_nothing_written_outside(ours.path());
}

/// A marked file that is on disk anyway, as the index has it: git drops the mark of a file it
/// finds on disk, then its merge marks the changed file again and removes it.
#[test]
fn a_merge_that_changes_a_marked_file_found_on_disk_matches_git() {
    let (ours, twin) = twins();
    for d in [ours.path(), twin.path()] {
        write_file(d, "docs/guide.md", "guide\n");
    }
    git_ok(twin.path(), &["merge", "-q", "--no-commit", "side"]);

    let outcome = start_merge(&open(ours.path()), "side", MergeOptions::default()).unwrap();

    assert_eq!(outcome, MergeOutcome::Merged);
    assert_same_as_git(ours.path(), twin.path());
    assert_nothing_written_outside(ours.path());
}
