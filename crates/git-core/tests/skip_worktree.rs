//! Index entries marked skip-worktree (`git update-index --skip-worktree`, and every file
//! outside a sparse checkout's cone): what git hides stays hidden, and nothing drops the mark
//! (#1145, #1146). The git CLI sets the repositories up and is the reference for the results.

mod common;

use std::path::Path;

use common::{commit_all, git_ok, init_repo, write_file};

fn git_out(dir: &Path, args: &[&str]) -> String {
    let output = test_support::git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// `git ls-files -v`: one `<tag> <path>` line per entry, `S` for skip-worktree, `H` otherwise.
fn ls_files_v(dir: &Path) -> Vec<String> {
    git_out(dir, &["ls-files", "-v"])
        .lines()
        .map(str::to_string)
        .collect()
}

fn status_short(dir: &Path) -> String {
    git_out(dir, &["status", "--short"])
}

/// `app/main.rs`, `docs/guide.md`, `vendor/lib/x.c` and `top.txt` committed, then `git
/// sparse-checkout set app`: `docs/` and `vendor/` are gone from the disk, marked skip-worktree.
fn sparse_checkout() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "app/main.rs", "fn main() {}\n");
    write_file(d, "docs/guide.md", "guide\n");
    write_file(d, "vendor/lib/x.c", "int x;\n");
    write_file(d, "top.txt", "top\n");
    commit_all(&repo, "base");
    git_ok(d, &["sparse-checkout", "set", "app"]);
    assert!(!d.join("docs/guide.md").exists());
    assert_eq!(
        ls_files_v(d),
        [
            "H app/main.rs",
            "S docs/guide.md",
            "H top.txt",
            "S vendor/lib/x.c"
        ]
    );
    assert_eq!(status_short(d), "");
    (dir, repo)
}

// --- Status and staging (#1146) ------------------------------------------------------------

#[test]
fn status_of_a_sparse_checkout_lists_no_file_outside_the_cone() {
    let (_dir, repo) = sparse_checkout();

    assert_eq!(git_core::status::status(&repo).unwrap(), Vec::new());
    assert_eq!(
        git_core::status::status_refreshing_index(&repo).unwrap(),
        Vec::new()
    );
}

#[test]
fn status_hides_a_removed_skip_worktree_file() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    write_file(d, "config.local", "default\n");
    commit_all(&repo, "base");
    git_ok(d, &["update-index", "--skip-worktree", "config.local"]);
    std::fs::remove_file(d.join("config.local")).unwrap();
    assert_eq!(status_short(d), "");

    assert_eq!(git_core::status::status(&repo).unwrap(), Vec::new());
}

/// Like `git add`, which refuses a path "outside of your sparse-checkout definition".
#[test]
fn staging_a_skip_worktree_path_is_refused_and_stages_nothing() {
    let (dir, repo) = sparse_checkout();
    let d = dir.path();

    let error = git_core::stage::stage_file(&repo, "docs/guide.md").unwrap_err();

    let message = error.to_string();
    assert!(
        message.contains("'docs/guide.md'") && message.contains("skip-worktree"),
        "{message}"
    );
    assert_eq!(status_short(d), "");
    assert!(ls_files_v(d).contains(&"S docs/guide.md".to_string()));
}

#[test]
fn stage_all_with_a_skip_worktree_path_stages_nothing() {
    let (dir, repo) = sparse_checkout();
    let d = dir.path();
    write_file(d, "app/main.rs", "fn main() { edited(); }\n");

    let paths = vec!["app/main.rs".to_string(), "vendor/lib/x.c".to_string()];
    assert!(git_core::stage::stage_paths(&repo, &paths).is_err());

    assert_eq!(status_short(d), " M app/main.rs\n");
    assert!(ls_files_v(d).contains(&"S vendor/lib/x.c".to_string()));
}
