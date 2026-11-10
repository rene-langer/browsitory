//! Hunk and line operations on a symbolic link (#586). libgit2 reads a link in the working tree
//! through it, so its apply compares the hunk (the link's target) with the content of the file
//! the link points to. Discarding writes the link itself instead, as `git checkout -p` does;
//! staging and unstaging work on the index only.
#![cfg(unix)]

mod common;

use std::path::{Path, PathBuf};

use git_core::diff::{working_diff, DiffOptions};
use git_core::stage::{self, StageError};

const LINK: u32 = 0o120000;

fn link_target(dir: &Path, rel: &str) -> PathBuf {
    std::fs::read_link(dir.join(rel)).unwrap()
}

fn retarget(dir: &Path, rel: &str, target: &str) {
    std::fs::remove_file(dir.join(rel)).unwrap();
    std::os::unix::fs::symlink(target, dir.join(rel)).unwrap();
}

/// The index entry of `rel`: its mode and content.
fn index_entry(repo: &git2::Repository, rel: &str) -> (u32, String) {
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    let entry = index.get_path(Path::new(rel), 0).unwrap();
    let blob = repo.find_blob(entry.id).unwrap();
    (
        entry.mode,
        String::from_utf8(blob.content().to_vec()).unwrap(),
    )
}

/// The only hunk of `l` with its displayed lines.
fn hunk(repo: &git2::Repository, staged: bool) -> (u32, u32, Vec<String>) {
    let hunks = working_diff(repo, "l", staged, &DiffOptions::default()).unwrap();
    assert_eq!(hunks.len(), 1);
    let h = &hunks[0];
    (
        h.old_start,
        h.new_start,
        h.lines.iter().map(|l| l.content.clone()).collect(),
    )
}

/// Indices of the `+`/`-` lines of the hunk, as the UI would select all of them.
fn changed_lines(repo: &git2::Repository, staged: bool) -> Vec<u32> {
    let hunks = working_diff(repo, "l", staged, &DiffOptions::default()).unwrap();
    hunks[0]
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.origin != git_core::diff::DiffLineOrigin::Context)
        .map(|(i, _)| i as u32)
        .collect()
}

/// `f.txt`, `h.txt` and the link `l -> f.txt` committed; `l` retargeted to `h.txt` in the
/// working tree.
fn retargeted() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "f\n");
    common::write_file(dir.path(), "h.txt", "h\n");
    std::os::unix::fs::symlink("f.txt", dir.path().join("l")).unwrap();
    common::commit_all(&repo, "init");
    retarget(dir.path(), "l", "h.txt");
    (dir, repo)
}

#[test]
fn discard_hunk_points_a_retargeted_link_back() {
    let (dir, repo) = retargeted();
    let (o, n, _) = hunk(&repo, false);

    stage::discard_hunk(&repo, "l", o, n, common::hunk_ctx(3)).unwrap();

    assert!(common::is_link(dir.path(), "l"));
    assert_eq!(link_target(dir.path(), "l"), PathBuf::from("f.txt"));
    assert_eq!(common::read(dir.path(), "h.txt"), "h\n");
    assert_eq!(common::read(dir.path(), "f.txt"), "f\n");
    assert!(working_diff(&repo, "l", false, &DiffOptions::default())
        .unwrap()
        .is_empty());
}

#[test]
fn discard_lines_points_a_retargeted_link_back() {
    let (dir, repo) = retargeted();
    let (o, n, expected) = hunk(&repo, false);
    let lines = changed_lines(&repo, false);

    stage::discard_lines(&repo, "l", o, n, common::hunk_ctx(3), &lines, &expected).unwrap();

    assert_eq!(link_target(dir.path(), "l"), PathBuf::from("f.txt"));
}

#[test]
fn discard_hunk_points_a_dangling_link_back() {
    let (dir, repo) = retargeted();
    retarget(dir.path(), "l", "nowhere");
    let (o, n, _) = hunk(&repo, false);

    stage::discard_hunk(&repo, "l", o, n, common::hunk_ctx(3)).unwrap();

    assert_eq!(link_target(dir.path(), "l"), PathBuf::from("f.txt"));
}

#[test]
fn discard_hunk_never_writes_the_file_a_link_points_to() {
    let (dir, repo) = retargeted();
    let outside = common::outside_dir();
    let victim = outside.path().join("victim");
    // The victim holds exactly the link's old target, so a write through the link would match.
    std::fs::write(&victim, "f.txt").unwrap();
    retarget(dir.path(), "l", victim.to_str().unwrap());
    let (o, n, _) = hunk(&repo, false);

    stage::discard_hunk(&repo, "l", o, n, common::hunk_ctx(3)).unwrap();

    assert_eq!(link_target(dir.path(), "l"), PathBuf::from("f.txt"));
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "f.txt");
}

#[test]
fn discard_hunk_of_a_stale_hunk_changes_nothing() {
    let (dir, repo) = retargeted();
    let (o, n, expected) = hunk(&repo, false);
    let lines = changed_lines(&repo, false);
    // Retargeted again after the hunk was shown: the hunk sits at the same place.
    retarget(dir.path(), "l", "f2.txt");

    let err =
        stage::discard_lines(&repo, "l", o, n, common::hunk_ctx(3), &lines, &expected).unwrap_err();

    assert!(matches!(err, StageError::HunkNotFound), "{err:?}");
    assert_eq!(link_target(dir.path(), "l"), PathBuf::from("f2.txt"));
}

#[test]
fn discard_hunk_keeps_the_index() {
    let (dir, repo) = retargeted();
    let (o, n, _) = hunk(&repo, false);

    stage::discard_hunk(&repo, "l", o, n, common::hunk_ctx(3)).unwrap();

    assert_eq!(index_entry(&repo, "l"), (LINK, "f.txt".to_string()));
    assert!(common::is_link(dir.path(), "l"));
}

#[test]
fn stage_hunk_stages_the_new_target_as_a_link() {
    let (dir, repo) = retargeted();
    let (o, n, _) = hunk(&repo, false);

    stage::stage_hunk(&repo, "l", o, n, common::hunk_ctx(3)).unwrap();

    assert_eq!(index_entry(&repo, "l"), (LINK, "h.txt".to_string()));
    assert_eq!(link_target(dir.path(), "l"), PathBuf::from("h.txt"));
    assert_eq!(common::read(dir.path(), "h.txt"), "h\n");
}

#[test]
fn stage_lines_stages_the_new_target_as_a_link() {
    let (_dir, repo) = retargeted();
    let (o, n, expected) = hunk(&repo, false);
    let lines = changed_lines(&repo, false);

    stage::stage_lines(&repo, "l", o, n, common::hunk_ctx(3), &lines, &expected).unwrap();

    assert_eq!(index_entry(&repo, "l"), (LINK, "h.txt".to_string()));
}

#[test]
fn unstage_hunk_puts_the_old_target_back_as_a_link() {
    let (dir, repo) = retargeted();
    stage::stage_file(&repo, "l").unwrap();
    let (o, n, _) = hunk(&repo, true);

    stage::unstage_hunk(&repo, "l", o, n, common::hunk_ctx(3)).unwrap();

    assert_eq!(index_entry(&repo, "l"), (LINK, "f.txt".to_string()));
    assert_eq!(link_target(dir.path(), "l"), PathBuf::from("h.txt"));
}

#[test]
fn unstage_lines_puts_the_old_target_back_as_a_link() {
    let (_dir, repo) = retargeted();
    stage::stage_file(&repo, "l").unwrap();
    let (o, n, expected) = hunk(&repo, true);
    let lines = changed_lines(&repo, true);

    stage::unstage_lines(&repo, "l", o, n, common::hunk_ctx(3), &lines, &expected).unwrap();

    assert_eq!(index_entry(&repo, "l"), (LINK, "f.txt".to_string()));
}

#[test]
fn discard_hunk_refuses_to_bring_a_link_back_over_a_file() {
    let (dir, repo) = retargeted();
    std::fs::remove_file(dir.path().join("l")).unwrap();
    common::write_file(dir.path(), "l", "now a file\n");
    // The link's removal and the file's addition are two hunks; this is the removal.
    let hunks = working_diff(&repo, "l", false, &DiffOptions::default()).unwrap();
    let removal = hunks.iter().find(|h| h.new_start == 0).unwrap();

    let err = stage::discard_hunk(
        &repo,
        "l",
        removal.old_start,
        removal.new_start,
        common::hunk_ctx(3),
    )
    .unwrap_err();

    assert!(matches!(err, StageError::SelectionNotApplicable), "{err:?}");
    assert!(!common::is_link(dir.path(), "l"));
    assert_eq!(common::read(dir.path(), "l"), "now a file\n");
}
