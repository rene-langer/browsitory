//! Non-cone sparse patterns (#1187), gitignore-style, matched as git matches them (`dir.c`'s
//! `path_in_sparse_checkout`: the last matching pattern decides, a negative one leaves the path
//! out, an undecided path takes its folder's verdict; `wildmatch`). For each patterns file,
//! `git sparse-checkout reapply` marks the paths it leaves out; Browsitory must say the same for
//! every path, whatever is on disk.

mod common;

use std::path::Path;

use common::{git_ok, write_file};

/// Paths for the patterns below to tell apart. No name uses a character Windows refuses, and no
/// two differ only in case.
const PATHS: &[&str] = &[
    "top.txt",
    "top2.txt",
    "Top3.TXT",
    "README.md",
    "a.md",
    "docs/guide.md",
    "docs/new.md",
    "docs/sub/s.md",
    "docs/sub/deep/d.md",
    "app/main.rs",
    "app/new.rs",
    "app/App.rs",
    "apps/x.rs",
    "lib/top.rs",
    "lib/core/c.rs",
    "lib/core/deep/d.rs",
    "lib/other/o.rs",
    "src/a1.rs",
    "src/b2.rs",
    "src/c3.rs",
    "src/x-y.rs",
    "src/[br].rs",
    "src/br.rs",
    "src/b.rs",
    "src/r.rs",
    "src/^x.rs",
    "src/!x.rs",
    "src/]x.rs",
    "src/-x.rs",
    "src/ab",
    "src/abc",
    "src/A9.rs",
    "src/z z.rs",
    "foo/bar/baz.txt",
    "foo/baz.txt",
    "foo/x/y/bar/baz.txt",
    "foobar/baz.txt",
    "xfoo/bar/baz.txt",
    "space name.txt",
    "dir.with.dots/f",
    "Mixed/Case/File.Txt",
    "deep/a/b/c/d/e.rs",
    "#hash.txt",
    "q/a.b.c",
    "q/abc",
    "q/a",
];

/// Patterns files, each line as git reads it.
const PATTERNS: &[&str] = &[
    "app/\n/top*.txt\n",
    "/app/\n",
    "*.md\n!docs/sub/\n",
    "/*\n!/*/\napp/\n/lib/\n!/lib/*/\n/lib/core/\n",
    "/*\n!/*/\n/app/\n/lib/\n!/lib/*/\n/lib/core/\n/*.md\n",
    "*.rs\n!lib/\n",
    "*\n!*.md\n",
    "*\n!docs/\n",
    "*\n!docs/**\n",
    "docs/**\n",
    "**/deep/**\n",
    "foo/**/baz.txt\n",
    "**/baz.txt\n",
    "foo/**\n",
    "/**/c.rs\n",
    "**\n!**/*.rs\n",
    "a**/c.rs\nlib**\n",
    "lib**/c.rs\n",
    "deep/**.rs\n",
    "lib/**/\n",
    "foo/*/baz.txt\n",
    "foo/*\n",
    "*/baz.txt\n",
    "x*/baz.txt\n",
    "src/?2.rs\nsrc/[ab]*.rs\n",
    "src/[!a]1.rs\nsrc/[^b]2.rs\n",
    "src/[a-b]*\n",
    "src/*[[:digit:]].rs\n",
    "src/[[:upper:]]*\n",
    "src/[[:lower:]][[:digit:]]*\n",
    "src/[[:alpha:][:punct:]]*\n",
    "src/[[:alnum:]]?.rs\n",
    "src/*[[:space:]]*\nsrc/[[:blank:]]\n",
    "src/[[:xdigit:]]*\n",
    "src/[[:print:]]?.rs\nsrc/[[:graph:]]x.rs\n",
    "src/[[:cntrl:]]*\n",
    "src/[[:bogus:]]*\napp/\n",
    "src/[[:alpha:]\napp/\n",
    "src/[[:alpha]*\n",
    "src/[a\napp/\n",
    "src/\\[br].rs\n",
    "src/[]]x.rs\nsrc/[!]]1.rs\n",
    "src/[\\]]x.rs\nsrc/[\\-]x.rs\n",
    "src/[-]x.rs\nsrc/[a-]1.rs\n",
    "src/[\\^]x.rs\nsrc/[\\!]x.rs\n",
    "src/[x-\\z]*\n",
    "src/a?\n",
    "src/a*\n",
    "src/*c\n",
    "src/a\\bc\n",
    "*.TXT\n",
    "TOP*.TXT\nApp/\n",
    "*.txt\n!top*\n",
    "space\\ name.txt\n",
    "space name.txt   \n",
    "#comment\n\\#hash.txt\n",
    "!\n/\napp/\n",
    "\u{feff}app/\r\n/top.txt\r\n",
    "dir.with.dots/\n",
    "Mixed/**/*.Txt\n",
    "mixed/\n",
    "deep/*/b/**/e.rs\n",
    "deep/**/**/e.rs\n",
    "deep/a/**\n!deep/a/b/c/\n",
    "q/a*\n!q/a.*\n",
    "q/a.?.c\n",
    "*/\n!src/\n",
    "a*\n",
    "*s/\n",
    "**/\n",
    "\\app/\n",
    "app\n",
];

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

fn write_paths(d: &Path) {
    for path in PATHS {
        write_file(d, path, &format!("{path}\n"));
    }
}

/// The paths `git ls-files -v` marks skip-worktree.
fn marked_by_git(d: &Path) -> Vec<String> {
    let output = test_support::git(d, &["ls-files", "-v"]);
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("S "))
        .map(str::to_string)
        .collect()
}

/// For each of [`PATTERNS`] under `core.ignorecase=<ignore_case>`, the paths where Browsitory
/// and git disagree.
fn disagreements(ignore_case: bool) -> Vec<String> {
    let dir = tempfile::TempDir::new().unwrap();
    let d = dir.path();
    init(d);
    git_ok(d, &["config", "core.ignorecase", &ignore_case.to_string()]);
    write_paths(d);
    git_ok(d, &["add", "-A"]);
    git_ok(d, &["commit", "-q", "-m", "all"]);
    let mut wrong = Vec::new();
    for patterns in PATTERNS {
        git_ok(d, &["sparse-checkout", "init", "--no-cone"]);
        std::fs::write(d.join(".git/info/sparse-checkout"), patterns).unwrap();
        let output = test_support::git(d, &["sparse-checkout", "reapply"]);
        assert!(output.status.success(), "{patterns:?}: {output:?}");
        let marked = marked_by_git(d);
        // Every file back on disk, so the patterns alone decide, not what git removed; git
        // brings them back clean, so its next reapply starts from a full work tree too.
        git_ok(d, &["sparse-checkout", "disable"]);
        git_ok(d, &["config", "--worktree", "core.sparseCheckout", "true"]);
        std::fs::write(d.join(".git/info/sparse-checkout"), patterns).unwrap();
        let repo = git2::Repository::open(d).unwrap();
        for path in PATHS {
            let ours = git_core::sparse::leaves_out(&repo, path).unwrap();
            let git = marked.iter().any(|marked| marked == path);
            if ours != git {
                wrong.push(format!(
                    "{patterns:?}: {path} git out={git}, ours out={ours}"
                ));
            }
        }
        git_ok(d, &["config", "--worktree", "core.sparseCheckout", "false"]);
    }
    wrong
}

#[test]
fn non_cone_patterns_leave_out_what_git_leaves_out() {
    assert_eq!(disagreements(false), Vec::<String>::new());
}

#[test]
fn non_cone_patterns_under_core_ignorecase_leave_out_what_git_leaves_out() {
    assert_eq!(disagreements(true), Vec::<String>::new());
}
