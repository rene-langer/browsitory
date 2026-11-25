//! #1172: in a sparse checkout, Reset, the aborts and Undo mark a path new to the index
//! skip-worktree when it lies outside the sparse patterns, and don't write its file, as git
//! does. The git CLI builds every repository; where git has the same operation, a twin built
//! the same way runs it and is the reference.

mod common;

use std::path::Path;

use common::{git_ok, write_file};
use git_core::reset::{reset_to, ResetMode};

fn git_out(dir: &Path, args: &[&str]) -> String {
    let output = test_support::git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn rev(dir: &Path, name: &str) -> String {
    git_out(dir, &["rev-parse", name]).trim().to_string()
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

/// Our repository `ours` ends as git left `twin`: the index with its marks, the status, HEAD's
/// tree and the work tree's files.
fn assert_same_as_git(ours: &Path, twin: &Path) {
    for args in [
        &["ls-files", "-v", "-s"][..],
        &["status", "--short"][..],
        &["rev-parse", "HEAD^{tree}"][..],
    ] {
        assert_eq!(git_out(ours, args), git_out(twin, args), "{args:?}");
    }
    assert_eq!(files(ours), files(twin));
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

/// `one`: `app/main.rs`, `docs/guide.md`, `lib/top.rs`, `lib/core/c.rs`, `lib/other/o.rs`,
/// `top.txt`. `two` adds `app/new.rs`, `docs/new.md`, `docs/sub/s.md`, `lib/new.rs`,
/// `lib/core/deep/d.rs`, `lib/other/n.rs`, `top2.txt` and changes `docs/guide.md`. HEAD is
/// back at `one`; returns `two`.
fn history(d: &Path) -> String {
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
    for (path, content) in [
        ("app/new.rs", "new app\n"),
        ("docs/new.md", "new doc\n"),
        ("docs/sub/s.md", "sub doc\n"),
        ("lib/new.rs", "new lib\n"),
        ("lib/core/deep/d.rs", "deep\n"),
        ("lib/other/n.rs", "new other\n"),
        ("top2.txt", "top two\n"),
        ("docs/guide.md", "guide v2\n"),
    ] {
        write_file(d, path, content);
    }
    commit(d, "two");
    let two = rev(d, "HEAD");
    git_ok(d, &["reset", "-q", "--hard", "HEAD~1"]);
    two
}

/// [`history`], then `git sparse-checkout set app lib/core` (cone mode): `lib/` is a parent
/// folder (its own files are in), `lib/core/` is recursive, `docs/` and `lib/other/` are out.
fn cone(d: &Path) -> String {
    let two = history(d);
    git_ok(d, &["sparse-checkout", "set", "app", "lib/core"]);
    assert_eq!(
        ls_files_v(d),
        "H app/main.rs\nS docs/guide.md\nH lib/core/c.rs\nS lib/other/o.rs\nH lib/top.rs\n\
         H top.txt\n"
    );
    two
}

fn twins(
    build: impl Fn(&Path) -> String,
) -> (tempfile::TempDir, String, tempfile::TempDir, String) {
    let ours = tempfile::TempDir::new().unwrap();
    let twin = tempfile::TempDir::new().unwrap();
    let our_target = build(ours.path());
    let twin_target = build(twin.path());
    (ours, our_target, twin, twin_target)
}

#[test]
fn a_mixed_reset_marks_the_new_paths_outside_the_cone_as_git_does() {
    let (ours, our_two, twin, twin_two) = twins(cone);
    git_ok(twin.path(), &["reset", "-q", "--mixed", &twin_two]);
    let repo = git2::Repository::open(ours.path()).unwrap();

    reset_to(&repo, &our_two, ResetMode::Mixed).unwrap();

    assert_same_as_git(ours.path(), twin.path());
    let listed: Vec<String> = git_core::status::status(&repo)
        .unwrap()
        .into_iter()
        .map(|entry| entry.path)
        .filter(|path| path.starts_with("docs/") || path.starts_with("lib/other/"))
        .collect();
    assert_eq!(listed, Vec::<String>::new());
}

#[test]
fn a_hard_reset_writes_only_the_new_paths_inside_the_cone_as_git_does() {
    let (ours, our_two, twin, twin_two) = twins(cone);
    git_ok(twin.path(), &["reset", "-q", "--hard", &twin_two]);
    let repo = git2::Repository::open(ours.path()).unwrap();

    reset_to(&repo, &our_two, ResetMode::Hard).unwrap();

    assert_same_as_git(ours.path(), twin.path());
}

/// git compares the cone's folders ignoring case under `core.ignorecase`.
#[test]
fn the_cone_matches_folders_ignoring_case_under_core_ignorecase() {
    let build = |d: &Path| {
        let two = history(d);
        git_ok(d, &["checkout", "-q", &two]);
        for (path, content) in [
            ("LIB/CORE/up.rs", "up core\n"),
            ("LIB/up.rs", "up lib\n"),
            ("LIB/OTHER/up.rs", "up other\n"),
            ("DOCS/up.md", "up doc\n"),
        ] {
            write_file(d, path, content);
        }
        commit(d, "three");
        let three = rev(d, "HEAD");
        git_ok(d, &["checkout", "-q", "main"]);
        git_ok(d, &["sparse-checkout", "set", "app", "lib/core"]);
        git_ok(d, &["config", "core.ignorecase", "true"]);
        three
    };
    let (ours, our_three, twin, twin_three) = twins(build);
    git_ok(twin.path(), &["reset", "-q", "--mixed", &twin_three]);
    let repo = git2::Repository::open(ours.path()).unwrap();

    reset_to(&repo, &our_three, ResetMode::Mixed).unwrap();

    assert_same_as_git(ours.path(), twin.path());
}

/// A linked worktree has its own patterns (`.git/worktrees/<name>/info/sparse-checkout`) and
/// `core.sparseCheckout` in its `config.worktree`; the main worktree isn't sparse.
#[test]
fn a_reset_in_a_linked_worktree_reads_its_own_patterns() {
    let build = |d: &Path| {
        let main = d.join("main");
        std::fs::create_dir(&main).unwrap();
        let two = history(&main);
        git_ok(
            &main,
            &["worktree", "add", "-q", "--detach", "../wt", "HEAD"],
        );
        git_ok(&d.join("wt"), &["sparse-checkout", "set", "app"]);
        two
    };
    let (ours, our_two, twin, twin_two) = twins(build);
    git_ok(
        &twin.path().join("wt"),
        &["reset", "-q", "--mixed", &twin_two],
    );
    let repo = git2::Repository::open(ours.path().join("wt")).unwrap();

    reset_to(&repo, &our_two, ResetMode::Mixed).unwrap();

    assert_same_as_git(&ours.path().join("wt"), &twin.path().join("wt"));
    assert!(ls_files_v(&ours.path().join("wt")).contains("S docs/new.md\n"));
}

/// [`cone`], then `info/sparse-checkout` deleted, `core.sparseCheckout` still on.
fn no_patterns_file(d: &Path) -> String {
    let two = cone(d);
    std::fs::remove_file(d.join(".git/info/sparse-checkout")).unwrap();
    two
}

/// Without a patterns file git marks nothing new.
#[test]
fn without_a_patterns_file_a_mixed_reset_marks_nothing_new_as_git_does() {
    let (ours, our_two, twin, twin_two) = twins(no_patterns_file);
    git_ok(twin.path(), &["reset", "-q", "--mixed", &twin_two]);
    let repo = git2::Repository::open(ours.path()).unwrap();

    reset_to(&repo, &our_two, ResetMode::Mixed).unwrap();

    assert_same_as_git(ours.path(), twin.path());
}

/// Without a patterns file `git reset --hard` doesn't apply the sparse checkout: it writes the
/// missing marked file the target changes (`docs/guide.md`) and every new path.
#[test]
fn without_a_patterns_file_a_hard_reset_writes_as_git_does() {
    let (ours, our_two, twin, twin_two) = twins(no_patterns_file);
    git_ok(twin.path(), &["reset", "-q", "--hard", &twin_two]);
    let repo = git2::Repository::open(ours.path()).unwrap();

    reset_to(&repo, &our_two, ResetMode::Hard).unwrap();

    assert_same_as_git(ours.path(), twin.path());
}

/// [`history`] with the non-cone patterns `app/` and `/top*.txt`.
fn non_cone(d: &Path) -> String {
    let two = history(d);
    git_ok(
        d,
        &["sparse-checkout", "set", "--no-cone", "app/", "/top*.txt"],
    );
    assert_eq!(
        ls_files_v(d),
        "H app/main.rs\nS docs/guide.md\nS lib/core/c.rs\nS lib/other/o.rs\nS lib/top.rs\n\
         H top.txt\n"
    );
    two
}

/// Every new path of `two` marked, whether the non-cone patterns hold it or not.
const NON_CONE_MARKS: &str = "H app/main.rs\nS app/new.rs\nS docs/guide.md\nS docs/new.md\n\
    S docs/sub/s.md\nS lib/core/c.rs\nS lib/core/deep/d.rs\nS lib/new.rs\nS lib/other/n.rs\n\
    S lib/other/o.rs\nS lib/top.rs\nH top.txt\nS top2.txt\n";

/// Non-cone patterns aren't matched: a new path whose file is missing is marked, so nothing is
/// listed as deleted. Outside the patterns that is what git does.
#[test]
fn a_mixed_reset_with_non_cone_patterns_marks_the_new_paths_whose_files_are_missing() {
    let (ours, our_two, twin, twin_two) = twins(non_cone);
    git_ok(twin.path(), &["reset", "-q", "--mixed", &twin_two]);
    let repo = git2::Repository::open(ours.path()).unwrap();

    reset_to(&repo, &our_two, ResetMode::Mixed).unwrap();

    assert_eq!(ls_files_v(ours.path()), NON_CONE_MARKS);
    assert_eq!(status_short(ours.path()), "");
    assert_eq!(git_core::status::status(&repo).unwrap(), Vec::new());
    let outside = [
        "docs/new.md",
        "docs/sub/s.md",
        "lib/new.rs",
        "lib/other/n.rs",
    ];
    let twin_marks = ls_files_v(twin.path());
    for path in outside {
        assert!(twin_marks.contains(&format!("S {path}\n")), "{twin_marks}");
    }
}

/// Non-cone patterns, Reset hard: no new file is written.
#[test]
fn a_hard_reset_with_non_cone_patterns_writes_no_new_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let d = dir.path();
    let two = non_cone(d);
    let repo = git2::Repository::open(d).unwrap();

    reset_to(&repo, &two, ResetMode::Hard).unwrap();

    assert_eq!(ls_files_v(d), NON_CONE_MARKS);
    assert_eq!(status_short(d), "");
    assert_eq!(
        files(d),
        [
            ("app/main.rs".to_string(), "fn main() {}\n".to_string()),
            ("top.txt".to_string(), "top\n".to_string()),
        ]
    );
}

/// `core.sparseCheckoutCone` on, but a pattern git doesn't accept in cone mode: git reads the
/// file as non-cone patterns ("disabling cone pattern matching"), and so does Browsitory.
#[test]
fn a_pattern_outside_the_cone_format_turns_cone_matching_off() {
    let dir = tempfile::TempDir::new().unwrap();
    let d = dir.path();
    let two = cone(d);
    std::fs::write(
        d.join(".git/info/sparse-checkout"),
        "/*\n!/*/\n/app/\n/lib/\n!/lib/*/\n/lib/core/\n/*.md\n",
    )
    .unwrap();
    let repo = git2::Repository::open(d).unwrap();

    reset_to(&repo, &two, ResetMode::Mixed).unwrap();

    assert!(
        ls_files_v(d).contains("S app/new.rs\n"),
        "{}",
        ls_files_v(d)
    );
    assert_eq!(status_short(d), "");
}

/// Undo of a Reset mixed from `two` back to `one` puts the paths it adds back to the index
/// outside the cone back marked.
#[test]
fn undo_of_a_reset_marks_the_paths_it_adds_back_outside_the_cone() {
    let dir = tempfile::TempDir::new().unwrap();
    let d = dir.path();
    let two = cone(d);
    git_ok(d, &["reset", "-q", "--hard", &two]);
    let marks = git_out(d, &["ls-files", "-v", "-s"]);
    let status = status_short(d);
    let repo = git2::Repository::open(d).unwrap();
    let before =
        git_core::undo::capture(&repo, git_core::undo::SnapshotScope::RefsAndIndex).unwrap();
    reset_to(&repo, &rev(d, "HEAD~1"), ResetMode::Mixed).unwrap();
    let after =
        git_core::undo::capture(&repo, git_core::undo::SnapshotScope::RefsAndIndex).unwrap();

    git_core::undo::restore(&repo, &after, &before, "browsitory: undo test").unwrap();

    assert_eq!(git_out(d, &["ls-files", "-v", "-s"]), marks);
    assert_eq!(status_short(d), status);
}

/// `main`: `mine1` changes `app/main.rs` against `onto`, then `mine2` adds `docs/only.md`
/// (outside the cone). Sparse `app`.
fn rebase_history(d: &Path) -> String {
    init(d);
    write_file(d, "app/main.rs", "base\n");
    write_file(d, "docs/guide.md", "guide\n");
    write_file(d, "top.txt", "top\n");
    commit(d, "base");
    git_ok(d, &["checkout", "-q", "-b", "onto"]);
    write_file(d, "app/main.rs", "onto\n");
    commit(d, "onto");
    git_ok(d, &["checkout", "-q", "main"]);
    write_file(d, "app/main.rs", "mine\n");
    commit(d, "mine1");
    write_file(d, "docs/only.md", "only\n");
    commit(d, "mine2");
    git_ok(d, &["sparse-checkout", "set", "app"]);
    assert_eq!(
        ls_files_v(d),
        "H app/main.rs\nS docs/guide.md\nS docs/only.md\nH top.txt\n"
    );
    rev(d, "onto")
}

#[test]
fn aborting_a_rebase_in_a_sparse_checkout_matches_git() {
    let (ours, our_onto, twin, _) = twins(rebase_history);
    let output = test_support::git(twin.path(), &["rebase", "onto"]);
    assert!(!output.status.success(), "{output:?}");
    git_ok(twin.path(), &["rebase", "--abort"]);
    let repo = git2::Repository::open(ours.path()).unwrap();
    let plan = git_core::rebase::commits_since(&repo, &our_onto)
        .unwrap()
        .into_iter()
        .map(|commit| git_core::rebase::RebasePlanEntry {
            commit_id: commit.id,
            action: git_core::rebase::RebaseAction::Pick,
            combined_message: None,
        })
        .collect();
    let (state, result) = git_core::rebase::start_rebase(&repo, &our_onto, plan).unwrap();
    assert_eq!(
        result,
        git_core::rebase::RebaseStepResult::Conflicted {
            files: vec!["app/main.rs".to_string()]
        }
    );

    git_core::rebase::abort_rebase(&repo, &state).unwrap();

    assert_same_as_git(ours.path(), twin.path());
    assert_eq!(rev(ours.path(), "HEAD"), rev(ours.path(), "main"));
}

/// `topic` changes `app/main.rs`, deletes `docs/guide.md`, adds `docs/new.md` and `docs/h.md`;
/// `main` changes `app/main.rs` and adds another `docs/h.md`. Sparse `app`.
fn merge_history(d: &Path) -> String {
    init(d);
    write_file(d, "app/main.rs", "base\n");
    write_file(d, "docs/guide.md", "guide\n");
    write_file(d, "top.txt", "top\n");
    commit(d, "base");
    git_ok(d, &["checkout", "-q", "-b", "topic"]);
    write_file(d, "app/main.rs", "topic\n");
    std::fs::remove_file(d.join("docs/guide.md")).unwrap();
    write_file(d, "docs/new.md", "new\n");
    write_file(d, "docs/h.md", "topic h\n");
    commit(d, "topic");
    git_ok(d, &["checkout", "-q", "main"]);
    write_file(d, "app/main.rs", "main\n");
    write_file(d, "docs/h.md", "main h\n");
    commit(d, "main");
    git_ok(d, &["sparse-checkout", "set", "app"]);
    assert_eq!(
        ls_files_v(d),
        "H app/main.rs\nS docs/guide.md\nS docs/h.md\nH top.txt\n"
    );
    "topic".to_string()
}

/// A merge started in a terminal (libgit2's merge refuses to change a file missing from the
/// work tree), aborted in Browsitory: `docs/guide.md`, which the merge took out of the index,
/// comes back marked and missing; the conflict on `docs/h.md`, which git wrote, comes back
/// unmarked with HEAD's version.
#[test]
fn aborting_a_merge_in_a_sparse_checkout_matches_git() {
    let (ours, _, twin, _) = twins(merge_history);
    for dir in [ours.path(), twin.path()] {
        let output = test_support::git(dir, &["merge", "topic"]);
        assert!(!output.status.success(), "{output:?}");
    }
    git_ok(twin.path(), &["merge", "--abort"]);
    let repo = git2::Repository::open(ours.path()).unwrap();

    git_core::merge::abort_merge(&repo).unwrap();

    assert_same_as_git(ours.path(), twin.path());
    assert!(ls_files_v(ours.path()).contains("S docs/guide.md\n"));
}
