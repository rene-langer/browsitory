mod common;

use git_core::stage::{discard_all_unstaged, discard_file, ignore_path, stage_file, StageError};

fn read(dir: &std::path::Path, p: &str) -> String {
    std::fs::read_to_string(dir.join(p))
        .unwrap()
        .replace("\r\n", "\n")
}

#[test]
fn discard_file_restores_a_modified_tracked_file() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "v1\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "v2\n");
    discard_file(&repo, "f.txt").unwrap();
    assert_eq!(read(dir.path(), "f.txt"), "v1\n");
}

#[test]
fn discard_file_keeps_staged_changes() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "v1\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "v2\n");
    stage_file(&repo, "f.txt").unwrap();
    common::write_file(dir.path(), "f.txt", "v3\n");

    discard_file(&repo, "f.txt").unwrap();

    assert_eq!(read(dir.path(), "f.txt"), "v2\n");
    let statuses = git_core::status::status(&repo).unwrap();
    assert!(statuses.iter().any(|e| e.path == "f.txt" && e.staged));
    assert!(!statuses.iter().any(|e| e.path == "f.txt" && !e.staged));
}

#[test]
fn discard_file_restores_a_deleted_tracked_file() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "v1\n");
    common::commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("f.txt")).unwrap();
    discard_file(&repo, "f.txt").unwrap();
    assert_eq!(read(dir.path(), "f.txt"), "v1\n");
}

#[test]
fn discard_file_deletes_an_untracked_file() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "seed.txt", "s\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "nested/new.txt", "n\n");
    discard_file(&repo, "nested/new.txt").unwrap();
    assert!(!dir.path().join("nested/new.txt").exists());
}

#[test]
fn discard_file_rejects_paths_outside_the_repo() {
    let (_dir, repo) = common::init_repo();
    assert!(matches!(
        discard_file(&repo, "../escape.txt"),
        Err(StageError::InvalidPath)
    ));
    assert!(matches!(
        discard_file(&repo, "/etc/passwd"),
        Err(StageError::InvalidPath)
    ));
}

#[test]
fn discard_all_unstaged_resets_tracked_deletes_untracked_and_keeps_the_index() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a1\n");
    common::write_file(dir.path(), "b.txt", "b1\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "a.txt", "a2\n");
    common::write_file(dir.path(), "b.txt", "b2\n");
    stage_file(&repo, "b.txt").unwrap();
    common::write_file(dir.path(), "u.txt", "u\n");

    discard_all_unstaged(&repo).unwrap();

    assert_eq!(read(dir.path(), "a.txt"), "a1\n");
    assert_eq!(read(dir.path(), "b.txt"), "b2\n");
    assert!(!dir.path().join("u.txt").exists());
    let statuses = git_core::status::status(&repo).unwrap();
    assert!(statuses.iter().all(|e| e.staged), "{statuses:?}");
}

#[test]
fn ignore_path_creates_gitignore() {
    let (dir, repo) = common::init_repo();
    ignore_path(&repo, "/build.log").unwrap();
    assert_eq!(read(dir.path(), ".gitignore"), "/build.log\n");
}

#[test]
fn ignore_path_appends_and_fixes_a_missing_trailing_newline() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), ".gitignore", "target");
    ignore_path(&repo, "*.log").unwrap();
    assert_eq!(read(dir.path(), ".gitignore"), "target\n*.log\n");
}

#[test]
fn ignore_path_skips_duplicates() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), ".gitignore", "*.log\n");
    ignore_path(&repo, "*.log").unwrap();
    assert_eq!(read(dir.path(), ".gitignore"), "*.log\n");
}

#[test]
fn ignore_path_rejects_empty_and_multiline_patterns() {
    let (_dir, repo) = common::init_repo();
    assert!(matches!(
        ignore_path(&repo, "  "),
        Err(StageError::InvalidPattern)
    ));
    assert!(matches!(
        ignore_path(&repo, "a\nb"),
        Err(StageError::InvalidPattern)
    ));
}
