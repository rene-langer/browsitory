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

// --- Reset and Abort (#1145) ---------------------------------------------------------------

/// The repository at `dir` opened again, so its index is the one the git CLI wrote.
fn reopen(dir: &Path) -> git2::Repository {
    git2::Repository::open(dir).unwrap()
}

fn rev(dir: &Path, name: &str) -> String {
    git_out(dir, &["rev-parse", name]).trim().to_string()
}

fn read(dir: &Path, path: &str) -> String {
    std::fs::read_to_string(dir.join(path)).unwrap()
}

/// Three commits: `one` (`config.local` "default", `a.txt` "a", `big/out.txt` "out"), `two`
/// (`config.local` "changed", `big/out.txt` "out2"), `three` (`a.txt` "third").
fn three_commits() -> tempfile::TempDir {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "config.local", "default\n");
    write_file(d, "a.txt", "a\n");
    write_file(d, "big/out.txt", "out\n");
    commit_all(&repo, "one");
    write_file(d, "config.local", "changed\n");
    write_file(d, "big/out.txt", "out2\n");
    commit_all(&repo, "two");
    write_file(d, "a.txt", "third\n");
    commit_all(&repo, "three");
    dir
}

/// [`three_commits`], with `config.local` marked skip-worktree and edited to "my local", and
/// `big/out.txt` marked and removed (as a sparse checkout leaves it).
fn local_settings() -> tempfile::TempDir {
    let dir = three_commits();
    let d = dir.path();
    git_ok(
        d,
        &[
            "update-index",
            "--skip-worktree",
            "config.local",
            "big/out.txt",
        ],
    );
    write_file(d, "config.local", "my local\n");
    std::fs::remove_file(d.join("big/out.txt")).unwrap();
    assert_eq!(status_short(d), "");
    dir
}

#[test]
fn a_mixed_reset_keeps_the_skip_worktree_marks() {
    let dir = local_settings();
    let d = dir.path();
    let repo = reopen(d);

    git_core::reset::reset_to(&repo, &rev(d, "HEAD~1"), git_core::reset::ResetMode::Mixed).unwrap();

    assert_eq!(
        ls_files_v(d),
        ["H a.txt", "S big/out.txt", "S config.local"]
    );
    assert_eq!(status_short(d), " M a.txt\n");
    assert_eq!(read(d, "config.local"), "my local\n");
}

/// `git reset --hard` leaves a skip-worktree file the target doesn't change alone: its local
/// content and its mark; a missing one stays missing.
#[test]
fn a_hard_reset_keeps_a_skip_worktree_file_the_target_does_not_change() {
    let dir = local_settings();
    let d = dir.path();
    let repo = reopen(d);

    git_core::reset::reset_to(&repo, &rev(d, "HEAD~1"), git_core::reset::ResetMode::Hard).unwrap();

    assert_eq!(read(d, "config.local"), "my local\n");
    assert!(!d.join("big/out.txt").exists());
    assert_eq!(read(d, "a.txt"), "a\n");
    assert_eq!(
        ls_files_v(d),
        ["H a.txt", "S big/out.txt", "S config.local"]
    );
    assert_eq!(status_short(d), "");
}

/// git refuses ("Entry 'config.local' not uptodate. Cannot merge.") rather than overwrite local
/// content that its status hides and that Undo couldn't bring back.
#[test]
fn a_hard_reset_that_would_overwrite_an_edited_skip_worktree_file_is_refused() {
    let dir = local_settings();
    let d = dir.path();
    let repo = reopen(d);
    let head = rev(d, "HEAD");

    let error =
        git_core::reset::reset_to(&repo, &rev(d, "HEAD~2"), git_core::reset::ResetMode::Hard)
            .unwrap_err();

    assert!(error.to_string().contains("'config.local'"), "{error}");
    assert_eq!(rev(d, "HEAD"), head);
    assert_eq!(read(d, "config.local"), "my local\n");
    assert_eq!(read(d, "a.txt"), "third\n");
    assert_eq!(
        ls_files_v(d),
        ["H a.txt", "S big/out.txt", "S config.local"]
    );
    assert_eq!(status_short(d), "");
}

/// A skip-worktree file as the index has it is written with the target's version, as `git
/// reset --hard` writes it, and keeps its mark.
#[test]
fn a_hard_reset_writes_an_unedited_skip_worktree_file_and_keeps_its_mark() {
    let dir = three_commits();
    let d = dir.path();
    git_ok(d, &["update-index", "--skip-worktree", "config.local"]);
    let repo = reopen(d);

    git_core::reset::reset_to(&repo, &rev(d, "HEAD~2"), git_core::reset::ResetMode::Hard).unwrap();

    assert_eq!(read(d, "config.local"), "default\n");
    assert_eq!(
        ls_files_v(d),
        ["H a.txt", "H big/out.txt", "S config.local"]
    );
    assert_eq!(status_short(d), "");
}

/// In a sparse checkout a hard reset neither brings back the files outside the cone nor drops
/// their marks, also where the target changes them.
#[test]
fn a_hard_reset_in_a_sparse_checkout_leaves_the_files_outside_the_cone_out() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "app/main.rs", "fn main() {}\n");
    write_file(d, "docs/guide.md", "guide\n");
    commit_all(&repo, "base");
    write_file(d, "docs/guide.md", "guide v2\n");
    write_file(d, "app/main.rs", "fn main() { v2(); }\n");
    commit_all(&repo, "second");
    git_ok(d, &["sparse-checkout", "set", "app"]);
    let repo = reopen(d);

    git_core::reset::reset_to(&repo, &rev(d, "HEAD~1"), git_core::reset::ResetMode::Hard).unwrap();

    assert!(!d.join("docs/guide.md").exists());
    assert_eq!(ls_files_v(d), ["H app/main.rs", "S docs/guide.md"]);
    assert_eq!(git_out(d, &["show", ":docs/guide.md"]), "guide\n");
    assert_eq!(read(d, "app/main.rs"), "fn main() {}\n");
    assert_eq!(status_short(d), "");
    assert_eq!(git_core::status::status(&repo).unwrap(), Vec::new());
}

#[test]
fn aborting_a_merge_keeps_the_skip_worktree_marks() {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "base\n");
    write_file(d, "config.local", "default\n");
    commit_all(&repo, "base");
    git_ok(d, &["checkout", "-q", "-b", "topic"]);
    write_file(d, "a.txt", "topic\n");
    commit_all(&repo, "topic");
    git_ok(d, &["checkout", "-q", "-"]);
    write_file(d, "a.txt", "main\n");
    commit_all(&repo, "main");
    git_ok(d, &["update-index", "--skip-worktree", "config.local"]);
    write_file(d, "config.local", "mine\n");
    let repo = reopen(d);

    let outcome =
        git_core::merge::start_merge(&repo, "topic", git_core::merge::MergeOptions::default())
            .unwrap();
    assert_eq!(
        outcome,
        git_core::merge::MergeOutcome::Conflicted {
            files: vec!["a.txt".to_string()]
        }
    );
    git_core::merge::abort_merge(&repo).unwrap();

    assert_eq!(ls_files_v(d), ["H a.txt", "S config.local"]);
    assert_eq!(read(d, "config.local"), "mine\n");
    assert_eq!(read(d, "a.txt"), "main\n");
    assert_eq!(status_short(d), "");
}

/// `git reset` drops an intent-to-add entry and keeps its file, untracked.
#[test]
fn a_reset_keeps_an_intent_to_add_file_as_untracked() {
    for mode in [
        git_core::reset::ResetMode::Mixed,
        git_core::reset::ResetMode::Hard,
    ] {
        let dir = three_commits();
        let d = dir.path();
        write_file(d, "new.txt", "my new content\n");
        git_ok(d, &["add", "-N", "new.txt"]);
        let repo = reopen(d);

        git_core::reset::reset_to(&repo, &rev(d, "HEAD"), mode).unwrap();

        assert_eq!(read(d, "new.txt"), "my new content\n", "{mode:?}");
        assert_eq!(status_short(d), "?? new.txt\n", "{mode:?}");
    }
}
