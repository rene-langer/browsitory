//! File names that hold glob characters (`[`, `]`, `*`, `?`) or start with `!` are paths, not
//! patterns, in every operation on a picked file (#1148): unstaging `pages/[id].tsx` must not
//! unstage `pages/i.tsx` too. git with `--literal-pathspecs` is the reference.

mod common;

use std::path::Path;

use common::{git_ok, init_repo};

fn git_out(dir: &Path, args: &[&str]) -> String {
    let output = test_support::git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn status(dir: &Path) -> String {
    git_out(dir, &["status", "--porcelain", "--untracked-files=all"])
}

fn write(dir: &Path, path: &str, content: &str) {
    let full = dir.join(path);
    std::fs::create_dir_all(full.parent().unwrap()).unwrap();
    std::fs::write(full, content).unwrap();
}

/// A repository with `base.txt` committed and `names` added to the index as new files, set up
/// with the git CLI.
fn staged_new(names: &[&str]) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write(d, "base.txt", "base\n");
    git_ok(d, &["add", "base.txt"]);
    git_ok(d, &["commit", "-q", "-m", "base"]);
    for name in names {
        write(d, name, &format!("{name}\n"));
    }
    git_ok(
        d,
        &["--literal-pathspecs", "add", "--"]
            .iter()
            .copied()
            .chain(names.iter().copied())
            .collect::<Vec<_>>(),
    );
    drop(repo);
    let repo = git2::Repository::open(d).unwrap();
    (dir, repo)
}

/// A repository with `names` committed, then each edited and the edits staged.
fn staged_modified(names: &[&str]) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    let d = dir.path();
    for name in names {
        write(d, name, "one\n");
    }
    git_ok(d, &["add", "-A"]);
    git_ok(d, &["commit", "-q", "-m", "base"]);
    for name in names {
        write(d, name, "two\n");
    }
    git_ok(d, &["add", "-A"]);
    drop(repo);
    let repo = git2::Repository::open(d).unwrap();
    (dir, repo)
}

/// What `git --literal-pathspecs restore --staged -- <path>` leaves in a repository set up by
/// `setup`.
fn git_unstaged(setup: impl Fn() -> (tempfile::TempDir, git2::Repository), path: &str) -> String {
    let (dir, _repo) = setup();
    git_ok(
        dir.path(),
        &["--literal-pathspecs", "restore", "--staged", "--", path],
    );
    status(dir.path())
}

fn unstage_file_unstages_only(names: &[&str], path: &str, expected: &str) {
    let (dir, repo) = staged_new(names);
    git_core::stage::unstage_file(&repo, path).unwrap();
    assert_eq!(status(dir.path()), expected);
    assert_eq!(git_unstaged(|| staged_new(names), path), expected);
}

#[test]
fn unstaging_a_file_named_like_a_bracket_glob_unstages_only_that_file() {
    unstage_file_unstages_only(
        &["pages/[id].tsx", "pages/i.tsx", "pages/d.tsx"],
        "pages/[id].tsx",
        "A  pages/d.tsx\nA  pages/i.tsx\n?? pages/[id].tsx\n",
    );
}

#[test]
fn unstaging_a_file_whose_name_starts_with_an_exclamation_mark_unstages_it() {
    unstage_file_unstages_only(&["!x", "x"], "!x", "A  x\n?? !x\n");
}

#[cfg(unix)]
#[test]
fn unstaging_a_file_named_like_a_star_glob_unstages_only_that_file() {
    unstage_file_unstages_only(
        &["a*", "a?", "ab", "ax"],
        "a*",
        "A  a?\nA  ab\nA  ax\n?? a*\n",
    );
}

#[cfg(unix)]
#[test]
fn unstaging_a_file_named_like_a_question_mark_glob_unstages_only_that_file() {
    unstage_file_unstages_only(&["a?", "ab", "ax"], "a?", "A  ab\nA  ax\n?? a?\n");
}

#[test]
fn unstaging_a_modified_file_named_like_a_glob_keeps_the_other_files_staged() {
    let names = ["[ab]", "a", "b"];
    let (dir, repo) = staged_modified(&names);
    git_core::stage::unstage_file(&repo, "[ab]").unwrap();
    let expected = " M [ab]\nM  a\nM  b\n";
    assert_eq!(status(dir.path()), expected);
    assert_eq!(git_unstaged(|| staged_modified(&names), "[ab]"), expected);
}

#[test]
fn unstage_paths_of_a_glob_named_file_unstages_only_the_listed_paths() {
    let (dir, repo) = staged_new(&["pages/[id].tsx", "pages/i.tsx", "pages/d.tsx", "q.txt"]);
    git_core::stage::unstage_paths(&repo, &["pages/[id].tsx".into(), "q.txt".into()]).unwrap();
    assert_eq!(
        status(dir.path()),
        "A  pages/d.tsx\nA  pages/i.tsx\n?? pages/[id].tsx\n?? q.txt\n"
    );
}

#[test]
fn staging_and_discarding_a_glob_named_file_touch_only_that_file() {
    let names = ["[ab]", "a", "b"];
    let (dir, repo) = staged_modified(&names);
    git_ok(dir.path(), &["reset", "-q"]);
    git_core::stage::stage_file(&repo, "[ab]").unwrap();
    assert_eq!(status(dir.path()), "M  [ab]\n M a\n M b\n");
    git_core::stage::unstage_file(&repo, "[ab]").unwrap();
    git_core::stage::discard_file(&repo, "[ab]").unwrap();
    assert_eq!(status(dir.path()), " M a\n M b\n");
    git_core::stage::stage_paths(&repo, &["[ab]".into(), "a".into()]).unwrap();
    assert_eq!(status(dir.path()), "M  a\n M b\n");
    git_core::stage::discard_all_unstaged(&repo, &["b".into()]).unwrap();
    assert_eq!(status(dir.path()), "M  a\n");
}

/// Which of `paths` git (`check-ignore`) ignores, asserting that libgit2 (`is_path_ignored`)
/// agrees on each.
fn ignored(dir: &Path, repo: &git2::Repository, paths: &[&str]) -> Vec<String> {
    paths
        .iter()
        .filter(|path| {
            // check-ignore takes each path as it is (it refuses `--literal-pathspecs`).
            let by_git = test_support::git(dir, &["check-ignore", "-q", "--", path])
                .status
                .success();
            let by_libgit2 = repo.is_path_ignored(path).unwrap();
            assert_eq!(by_git, by_libgit2, "{path:?}: git, libgit2");
            by_git
        })
        .map(|path| path.to_string())
        .collect()
}

#[test]
fn ignoring_escaped_patterns_ignores_exactly_the_named_files() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    let files = [
        "pages/[id].tsx",
        "pages/i.tsx",
        "pages/d.tsx",
        "#hash",
        "!bang",
        "hash",
        "bang",
    ];
    for file in files {
        write(d, file, "x\n");
    }

    git_core::stage::ignore_paths(
        &repo,
        &["/pages/\\[id].tsx".into(), "/#hash".into(), "/!bang".into()],
    )
    .unwrap();

    assert_eq!(
        ignored(d, &repo, &files),
        ["pages/[id].tsx", "#hash", "!bang"]
    );
}

#[cfg(unix)]
#[test]
fn ignoring_an_escaped_star_question_mark_or_trailing_space_ignores_exactly_that_file() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    let files = ["report*.txt", "report1.txt", "a?", "ab", "trail ", "trail"];
    for file in files {
        write(d, file, "x\n");
    }

    git_core::stage::ignore_paths(
        &repo,
        &["/report\\*.txt".into(), "/a\\?".into(), "/trail\\ ".into()],
    )
    .unwrap();

    assert_eq!(ignored(d, &repo, &files), ["report*.txt", "a?", "trail "]);
    // Listed once: the escaped space belongs to the pattern.
    git_core::stage::ignore_paths(&repo, &["/trail\\ ".into()]).unwrap();
    assert_eq!(
        std::fs::read_to_string(d.join(".gitignore")).unwrap(),
        "/report\\*.txt\n/a\\?\n/trail\\ \n"
    );
}

#[test]
fn an_unescaped_trailing_space_is_still_refused() {
    let (dir, repo) = init_repo();
    for bad in ["/trail ", "/trail\\  ", "/trail\\\\ "] {
        let result = git_core::stage::ignore_paths(&repo, &[bad.to_string()]);
        assert!(
            matches!(result, Err(git_core::stage::StageError::InvalidPattern)),
            "{bad:?}: {result:?}"
        );
    }
    assert!(!dir.path().join(".gitignore").exists());
}
