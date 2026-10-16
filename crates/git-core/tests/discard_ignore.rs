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

#[test]
fn ignore_path_keeps_a_non_utf8_gitignore_untouched() {
    let (dir, repo) = common::init_repo();
    let bytes: &[u8] = b"caf\xe9\n";
    std::fs::write(dir.path().join(".gitignore"), bytes).unwrap();
    assert!(ignore_path(&repo, "*.log").is_err());
    assert_eq!(std::fs::read(dir.path().join(".gitignore")).unwrap(), bytes);
}

#[cfg(unix)]
#[test]
fn discard_file_does_not_delete_through_a_symlinked_directory() {
    let (dir, repo) = common::init_repo();
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("victim.txt"), "keep\n").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
    assert!(matches!(
        discard_file(&repo, "link/victim.txt"),
        Err(StageError::InvalidPath)
    ));
    assert!(outside.path().join("victim.txt").exists());
}

// --- Final-review fixes: symlinked parents, symlinked .gitignore, pattern validation ---

/// Tracked `d/f.txt`, then `d` replaced by a symlink to an outside dir holding its own `f.txt`.
#[cfg(unix)]
fn setup_symlinked_parent() -> (tempfile::TempDir, git2::Repository, tempfile::TempDir) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "d/f.txt", "inside\n");
    common::commit_all(&repo, "base");
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("f.txt"), "outside\n").unwrap();
    std::fs::remove_dir_all(dir.path().join("d")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("d")).unwrap();
    (dir, repo, outside)
}

#[cfg(unix)]
#[test]
fn discard_file_does_not_write_a_tracked_file_through_a_symlinked_parent() {
    let (_dir, repo, outside) = setup_symlinked_parent();
    let result = discard_file(&repo, "d/f.txt");
    assert!(matches!(result, Err(StageError::InvalidPath)), "{result:?}");
    assert_eq!(read(outside.path(), "f.txt"), "outside\n");
}

#[cfg(unix)]
#[test]
fn ignore_path_refuses_a_symlinked_gitignore() {
    let (dir, repo) = common::init_repo();
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("target"), "keep\n").unwrap();
    std::os::unix::fs::symlink(outside.path().join("target"), dir.path().join(".gitignore"))
        .unwrap();
    let result = ignore_path(&repo, "*.log");
    assert!(matches!(result, Err(StageError::InvalidPath)), "{result:?}");
    assert_eq!(read(outside.path(), "target"), "keep\n");
}

#[test]
fn ignore_path_rejects_surrounding_whitespace_instead_of_trimming() {
    let (dir, repo) = common::init_repo();
    for bad in ["", " *.log", "*.log ", "\t*.log", "a\rb"] {
        let result = ignore_path(&repo, bad);
        assert!(
            matches!(result, Err(StageError::InvalidPattern)),
            "{bad:?}: {result:?}"
        );
    }
    assert!(!dir.path().join(".gitignore").exists());
}

#[test]
fn discard_file_on_an_untracked_folder_explains_instead_of_failing_raw() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "seed.txt", "s\n");
    common::commit_all(&repo, "base");
    let _nested = git2::Repository::init(dir.path().join("nested")).unwrap();
    common::write_file(dir.path(), "nested/inner.txt", "i\n");
    let result = discard_file(&repo, "nested/");
    match result {
        Err(e @ StageError::IsFolder(_)) => assert_eq!(
            e.to_string(),
            "nested/ is a folder (for example a nested repository) and wasn't discarded."
        ),
        other => panic!("{other:?}"),
    }
    assert!(dir.path().join("nested/inner.txt").exists());
    assert!(dir.path().join("nested/.git").exists());
}
