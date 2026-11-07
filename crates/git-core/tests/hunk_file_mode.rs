//! Hunk and line operations keep the mode of the side they patch (#427): staging, unstaging or
//! discarding part of an executable file never drops (or adds) its executable bit, and never
//! stages or reverts a pending mode change on its own.
#![cfg(unix)]

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use git_core::diff::{working_diff, DiffLineOrigin, DiffOptions};
use git_core::stage;

const EXEC: u32 = 0o100755;
const PLAIN: u32 = 0o100644;

fn chmod(dir: &Path, rel: &str, mode: u32) {
    std::fs::set_permissions(dir.join(rel), std::fs::Permissions::from_mode(mode)).unwrap();
}

fn workdir_is_executable(dir: &Path, rel: &str) -> bool {
    std::fs::metadata(dir.join(rel))
        .unwrap()
        .permissions()
        .mode()
        & 0o111
        != 0
}

fn index_mode(repo: &git2::Repository, rel: &str) -> u32 {
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    index.get_path(Path::new(rel), 0).unwrap().mode
}

fn index_content(repo: &git2::Repository, rel: &str) -> String {
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    let id = index.get_path(Path::new(rel), 0).unwrap().id;
    String::from_utf8(repo.find_blob(id).unwrap().content().to_vec()).unwrap()
}

/// The only hunk of `f.sh` with its displayed lines.
fn hunk(repo: &git2::Repository, staged: bool) -> (u32, u32, Vec<String>) {
    let hunks = working_diff(repo, "f.sh", staged, &DiffOptions::default()).unwrap();
    assert_eq!(hunks.len(), 1);
    let h = &hunks[0];
    (
        h.old_start,
        h.new_start,
        h.lines.iter().map(|l| l.content.clone()).collect(),
    )
}

/// Index of the line `"{origin}{content}"` in the hunk, as the UI would select it.
fn line_index(repo: &git2::Repository, staged: bool, origin: DiffLineOrigin, content: &str) -> u32 {
    let hunks = working_diff(repo, "f.sh", staged, &DiffOptions::default()).unwrap();
    hunks[0]
        .lines
        .iter()
        .position(|l| l.origin == origin && l.content == content)
        .unwrap() as u32
}

/// `f.sh` committed as `head_mode` with "1\n2\n3\n", then the work tree changes line 2 to "two"
/// and gets `work_mode`.
fn setup(head_mode: u32, work_mode: u32) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.sh", "1\n2\n3\n");
    chmod(dir.path(), "f.sh", head_mode & 0o777);
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.sh", "1\ntwo\n3\n");
    chmod(dir.path(), "f.sh", work_mode & 0o777);
    (dir, repo)
}

/// Like `setup`, with the whole work tree change (content and mode) staged.
fn setup_staged(head_mode: u32, work_mode: u32) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = setup(head_mode, work_mode);
    stage::stage_file(&repo, "f.sh").unwrap();
    assert_eq!(index_mode(&repo, "f.sh"), work_mode);
    (dir, repo)
}

#[test]
fn staging_a_hunk_of_an_executable_file_keeps_its_index_mode() {
    let (_dir, repo) = setup(EXEC, EXEC);
    let (o, n, _) = hunk(&repo, false);
    stage::stage_hunk(&repo, "f.sh", o, n, 3).unwrap();
    assert_eq!(index_content(&repo, "f.sh"), "1\ntwo\n3\n");
    assert_eq!(index_mode(&repo, "f.sh"), EXEC);
}

#[test]
fn staging_lines_of_an_executable_file_keeps_its_index_mode() {
    let (_dir, repo) = setup(EXEC, EXEC);
    let (o, n, shown) = hunk(&repo, false);
    let add = line_index(&repo, false, DiffLineOrigin::Add, "two\n");
    let remove = line_index(&repo, false, DiffLineOrigin::Remove, "2\n");
    stage::stage_lines(&repo, "f.sh", o, n, 3, &[remove, add], &shown).unwrap();
    assert_eq!(index_content(&repo, "f.sh"), "1\ntwo\n3\n");
    assert_eq!(index_mode(&repo, "f.sh"), EXEC);
}

#[test]
fn unstaging_a_hunk_of_an_executable_file_keeps_its_index_mode() {
    let (_dir, repo) = setup_staged(EXEC, EXEC);
    let (o, n, _) = hunk(&repo, true);
    stage::unstage_hunk(&repo, "f.sh", o, n, 3).unwrap();
    assert_eq!(index_content(&repo, "f.sh"), "1\n2\n3\n");
    assert_eq!(index_mode(&repo, "f.sh"), EXEC);
}

#[test]
fn unstaging_lines_of_an_executable_file_keeps_its_index_mode() {
    let (_dir, repo) = setup_staged(EXEC, EXEC);
    let (o, n, shown) = hunk(&repo, true);
    let add = line_index(&repo, true, DiffLineOrigin::Add, "two\n");
    let remove = line_index(&repo, true, DiffLineOrigin::Remove, "2\n");
    stage::unstage_lines(&repo, "f.sh", o, n, 3, &[remove, add], &shown).unwrap();
    assert_eq!(index_content(&repo, "f.sh"), "1\n2\n3\n");
    assert_eq!(index_mode(&repo, "f.sh"), EXEC);
}

#[test]
fn discarding_a_hunk_of_an_executable_file_keeps_its_exec_bit() {
    let (dir, repo) = setup(EXEC, EXEC);
    let (o, n, _) = hunk(&repo, false);
    stage::discard_hunk(&repo, "f.sh", o, n, 3).unwrap();
    assert_eq!(common::read(dir.path(), "f.sh"), "1\n2\n3\n");
    assert!(workdir_is_executable(dir.path(), "f.sh"));
}

#[test]
fn discarding_lines_of_an_executable_file_keeps_its_exec_bit() {
    let (dir, repo) = setup(EXEC, EXEC);
    let (o, n, shown) = hunk(&repo, false);
    let add = line_index(&repo, false, DiffLineOrigin::Add, "two\n");
    let remove = line_index(&repo, false, DiffLineOrigin::Remove, "2\n");
    stage::discard_lines(&repo, "f.sh", o, n, 3, &[remove, add], &shown).unwrap();
    assert_eq!(common::read(dir.path(), "f.sh"), "1\n2\n3\n");
    assert!(workdir_is_executable(dir.path(), "f.sh"));
}

#[test]
fn staging_a_hunk_leaves_an_unstaged_exec_bit_change_unstaged() {
    // Work tree turned the file executable; staging the content hunk keeps the index at 644.
    let (dir, repo) = setup(PLAIN, EXEC);
    let (o, n, _) = hunk(&repo, false);
    stage::stage_hunk(&repo, "f.sh", o, n, 3).unwrap();
    assert_eq!(index_content(&repo, "f.sh"), "1\ntwo\n3\n");
    assert_eq!(index_mode(&repo, "f.sh"), PLAIN);
    assert!(workdir_is_executable(dir.path(), "f.sh"));
}

#[test]
fn staging_a_hunk_leaves_an_unstaged_exec_bit_removal_unstaged() {
    let (dir, repo) = setup(EXEC, PLAIN);
    let (o, n, _) = hunk(&repo, false);
    stage::stage_hunk(&repo, "f.sh", o, n, 3).unwrap();
    assert_eq!(index_mode(&repo, "f.sh"), EXEC);
    assert!(!workdir_is_executable(dir.path(), "f.sh"));
}

#[test]
fn discarding_a_hunk_does_not_revert_an_unstaged_exec_bit_change() {
    let (dir, repo) = setup(PLAIN, EXEC);
    let (o, n, _) = hunk(&repo, false);
    stage::discard_hunk(&repo, "f.sh", o, n, 3).unwrap();
    assert_eq!(common::read(dir.path(), "f.sh"), "1\n2\n3\n");
    assert!(workdir_is_executable(dir.path(), "f.sh"));
    assert_eq!(index_mode(&repo, "f.sh"), PLAIN);
}

#[test]
fn unstaging_a_hunk_does_not_unstage_a_staged_exec_bit_change() {
    let (_dir, repo) = setup_staged(PLAIN, EXEC);
    let (o, n, _) = hunk(&repo, true);
    stage::unstage_hunk(&repo, "f.sh", o, n, 3).unwrap();
    assert_eq!(index_content(&repo, "f.sh"), "1\n2\n3\n");
    assert_eq!(index_mode(&repo, "f.sh"), EXEC);
}

#[test]
fn staging_lines_of_an_untracked_executable_file_stages_it_executable() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "seed", "x\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.sh", "1\n2\n");
    chmod(dir.path(), "f.sh", 0o755);
    let (o, n, shown) = hunk(&repo, false);
    stage::stage_lines(&repo, "f.sh", o, n, 3, &[0], &shown).unwrap();
    assert_eq!(index_content(&repo, "f.sh"), "1\n");
    assert_eq!(index_mode(&repo, "f.sh"), EXEC);
}
