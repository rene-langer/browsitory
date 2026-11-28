//! A symbolic link kept as a plain file holding its target (`core.symlinks` off, #802). git
//! sends such a file through the clean filters (`core.autocrlf`, `text`/`eol`, `ident`) like any
//! regular file when it hashes it (`index_path` filters every `S_IFREG`), whatever mode the index
//! entry keeps; it checks it out raw. Every path that hashes one agrees with git. The git CLI sets
//! the repositories up and is the reference for the results.

mod common;

use std::path::Path;

use common::{git_ok, init_repo, twin, write_file};
use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseStepResult};
use git_core::stash::StashOptions;

fn git_out(dir: &Path, args: &[&str]) -> String {
    let output = test_support::git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn head(dir: &Path) -> String {
    git_out(dir, &["rev-parse", "HEAD"])
}

/// `a.txt` and the link `l` (target `target`) committed, then `core.symlinks` off and
/// `core.autocrlf` on: `l` is checked out as a file holding `target`.
fn link_kept_as_file(target: &[u8]) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    let d = dir.path();
    write_file(d, "a.txt", "a\n");
    let blob = repo.blob(target).unwrap().to_string();
    git_ok(d, &["add", "a.txt"]);
    git_ok(
        d,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("120000,{blob},l"),
        ],
    );
    git_ok(d, &["commit", "-q", "-m", "one"]);
    git_ok(d, &["config", "core.symlinks", "false"]);
    git_ok(d, &["config", "core.autocrlf", "true"]);
    let _ = std::fs::remove_file(d.join("l"));
    git_ok(d, &["checkout", "--", "l"]);
    assert_eq!(std::fs::read(d.join("l")).unwrap(), target);
    assert_eq!(
        git_out(d, &["ls-files", "-s", "l"]),
        format!("120000 {blob} 0\tl")
    );
    drop(repo);
    let repo = git2::Repository::open(d).unwrap();
    (dir, repo)
}

/// The blob `git hash-object` (clean filters applied) gives the work-tree file `path`.
fn cleaned(dir: &Path, path: &str) -> String {
    git_out(dir, &["hash-object", path])
}

/// The stash's work-tree entry for `path`: `<mode> <blob>`.
fn stashed(dir: &Path, path: &str) -> String {
    git_out(dir, &["ls-tree", "stash@{0}", path])
        .split('\t')
        .next()
        .unwrap()
        .replace(" blob ", " ")
}

fn stash_paths(paths: &[&str]) -> StashOptions {
    StashOptions {
        paths: paths.iter().map(|path| path.to_string()).collect(),
        ..StashOptions::default()
    }
}

/// Stash file… on the file stores its target through the clean filters, as `git stash push --
/// l` does: `t\r\n` is stored as `t\n`.
#[test]
fn stash_file_of_a_link_kept_as_file_stores_the_cleaned_target_as_git_does() {
    let (dir, mut repo) = link_kept_as_file(b"t");
    let d = dir.path();
    std::fs::write(d.join("l"), "t\r\n").unwrap();
    let expected = cleaned(d, "l");
    assert_eq!(expected, repo.blob(b"t\n").unwrap().to_string());
    let (twin_dir, _twin) = twin(d);
    git_ok(twin_dir.path(), &["stash", "push", "-q", "--", "l"]);

    git_core::stash::save_stash(&mut repo, &stash_paths(&["l"])).unwrap();

    assert_eq!(stashed(d, "l"), format!("120000 {expected}"));
    assert_eq!(stashed(d, "l"), stashed(twin_dir.path(), "l"));
    assert_eq!(std::fs::read(d.join("l")).unwrap(), b"t");
}

/// Stash all agrees with Stash file… and with `git stash push`.
#[test]
fn stash_all_of_a_link_kept_as_file_stores_the_cleaned_target_as_git_does() {
    let (dir, mut repo) = link_kept_as_file(b"t");
    let d = dir.path();
    std::fs::write(d.join("l"), "t\r\n").unwrap();
    let expected = cleaned(d, "l");
    let (twin_dir, _twin) = twin(d);
    git_ok(twin_dir.path(), &["stash", "push", "-q"]);

    git_core::stash::save_stash(&mut repo, &StashOptions::default()).unwrap();

    assert_eq!(stashed(d, "l"), format!("120000 {expected}"));
    assert_eq!(stashed(d, "l"), stashed(twin_dir.path(), "l"));
}

/// [`link_kept_as_file`] with target `t\n`, then `u\r\n` written into `l` and committed with `git
/// add` (stored as `u\n`, the link mode kept), and `l` marked skip-worktree: the file differs
/// from its blob only before the clean filters.
fn skip_worktree_link_differing_only_before_the_filters() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = link_kept_as_file(b"t\n");
    let d = dir.path();
    std::fs::write(d.join("l"), "u\r\n").unwrap();
    git_ok(d, &["add", "l"]);
    git_ok(d, &["commit", "-q", "-m", "two"]);
    assert_eq!(
        git_out(d, &["ls-tree", "HEAD", "l"]),
        format!("120000 blob {}\tl", repo.blob(b"u\n").unwrap())
    );
    git_ok(d, &["update-index", "--skip-worktree", "l"]);
    assert_eq!(git_out(d, &["ls-files", "-v", "l"]), "S l");
    (dir, repo)
}

/// git's `reset --hard` finds the file up to date (it compares the stat data `git add`
/// recorded, then the cleaned content) and writes the target's version; Browsitory's check for
/// an edited skip-worktree file, which compares content only, agrees. git runs on a second repository
/// made the same way: a copy would have other stat data, which git takes for an edit.
#[test]
fn a_hard_reset_over_a_skip_worktree_link_kept_as_file_compares_it_cleaned_as_git_does() {
    let (dir, repo) = skip_worktree_link_differing_only_before_the_filters();
    let d = dir.path();
    let (git_dir, _git_repo) = skip_worktree_link_differing_only_before_the_filters();
    let g = git_dir.path();
    git_ok(g, &["reset", "-q", "--hard", "HEAD~1"]);
    assert_eq!(std::fs::read(g.join("l")).unwrap(), b"t\n");

    git_core::reset::reset_to(
        &repo,
        &git_out(d, &["rev-parse", "HEAD~1"]),
        git_core::reset::ResetMode::Hard,
    )
    .unwrap();

    assert_eq!(git_out(d, &["log", "--format=%s"]), "one");
    assert_eq!(std::fs::read(d.join("l")).unwrap(), b"t\n");
    assert_eq!(
        git_out(d, &["ls-files", "-s", "-v", "l"]),
        git_out(g, &["ls-files", "-s", "-v", "l"])
    );
}

/// A rebase counts an edited skip-worktree file as a local change (#170); one that differs only
/// before the clean filters isn't edited, as for a regular file
/// (`a_skip_worktree_file_differing_only_by_line_endings_does_not_block_a_rebase`).
#[test]
fn a_skip_worktree_link_kept_as_file_differing_only_before_the_filters_does_not_block_a_rebase() {
    let (dir, repo) = skip_worktree_link_differing_only_before_the_filters();
    let d = dir.path();
    let onto = {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        let mut tree = repo.treebuilder(Some(&base.tree().unwrap())).unwrap();
        tree.insert("h.txt", repo.blob(b"h\n").unwrap(), 0o100644)
            .unwrap();
        let tree = repo.find_tree(tree.write().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(None, &sig, &sig, "h", &tree, &[&base])
            .unwrap()
            .to_string()
    };
    write_file(d, "g.txt", "g\n");
    git_ok(d, &["add", "g.txt"]);
    git_ok(d, &["commit", "-q", "-m", "g"]);
    let pick = RebasePlanEntry {
        commit_id: head(d),
        action: RebaseAction::Pick,
        combined_message: None,
    };

    let (_, result) = git_core::rebase::start_rebase(&repo, &onto, vec![pick]).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(std::fs::read(d.join("l")).unwrap(), b"u\r\n");
    assert_eq!(git_out(d, &["ls-files", "-v", "l"]), "S l");
}
