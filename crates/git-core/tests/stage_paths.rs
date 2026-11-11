//! Staging, unstaging and ignoring several paths in one call (#528): the same result as the
//! per-path calls in a row, and all or nothing when one path fails.
mod common;

use common::{commit_all, init_repo, write_file};
use git_core::stage::{ignore_paths, stage_paths, unstage_paths, StageError};

fn staged(repo: &git2::Repository) -> Vec<String> {
    let mut paths: Vec<String> = git_core::status::status(repo)
        .unwrap()
        .into_iter()
        .filter(|entry| entry.staged)
        .map(|entry| entry.path)
        .collect();
    paths.sort();
    paths
}

fn read(dir: &std::path::Path, path: &str) -> String {
    std::fs::read_to_string(dir.join(path))
        .unwrap()
        .replace("\r\n", "\n")
}

#[test]
fn stage_paths_stages_every_listed_path_and_nothing_else() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "v1\n");
    write_file(dir.path(), "gone.txt", "v1\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "tracked.txt", "v2\n");
    std::fs::remove_file(dir.path().join("gone.txt")).unwrap();
    write_file(dir.path(), "new.txt", "n\n");
    write_file(dir.path(), "left.txt", "l\n");

    stage_paths(
        &repo,
        &["tracked.txt".into(), "gone.txt".into(), "new.txt".into()],
    )
    .unwrap();

    assert_eq!(staged(&repo), ["gone.txt", "new.txt", "tracked.txt"]);
}

#[test]
fn stage_paths_of_nothing_changes_nothing() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "new.txt", "n\n");

    stage_paths(&repo, &[]).unwrap();

    assert!(staged(&repo).is_empty());
}

#[test]
fn stage_paths_stages_nothing_when_one_path_is_outside_the_repository() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");

    let error = stage_paths(&repo, &["a.txt".into(), "../escape.txt".into()]).unwrap_err();

    assert!(matches!(error, StageError::InvalidPath), "{error:?}");
    assert!(staged(&repo).is_empty());
}

#[test]
fn stage_paths_puts_the_index_back_when_a_later_path_fails() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "seed.txt", "s\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "a.txt", "a\n");
    // A folder git doesn't track: the index can't take it as one entry.
    write_file(dir.path(), "folder/inner.txt", "i\n");
    let index_before = std::fs::read(repo.path().join("index")).unwrap();

    let result = stage_paths(&repo, &["a.txt".into(), "folder".into()]);

    assert!(result.is_err(), "{result:?}");
    assert!(staged(&repo).is_empty());
    assert_eq!(
        std::fs::read(repo.path().join("index")).unwrap(),
        index_before
    );
    // The repository's own index object was put back too, not only the file.
    let mut index = repo.index().unwrap();
    index.write().unwrap();
    assert!(staged(&repo).is_empty());
}

#[test]
fn unstage_paths_unstages_every_listed_path_and_nothing_else() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "v1\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "tracked.txt", "v2\n");
    write_file(dir.path(), "new.txt", "n\n");
    write_file(dir.path(), "kept.txt", "k\n");
    stage_paths(
        &repo,
        &["tracked.txt".into(), "new.txt".into(), "kept.txt".into()],
    )
    .unwrap();

    unstage_paths(&repo, &["tracked.txt".into(), "new.txt".into()]).unwrap();

    assert_eq!(staged(&repo), ["kept.txt"]);
}

#[test]
fn unstage_paths_works_before_the_first_commit() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), "b.txt", "b\n");
    stage_paths(&repo, &["a.txt".into(), "b.txt".into()]).unwrap();

    unstage_paths(&repo, &["a.txt".into(), "b.txt".into()]).unwrap();

    assert!(staged(&repo).is_empty());
}

#[test]
fn ignore_paths_appends_each_new_pattern_once() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), ".gitignore", "target");

    ignore_paths(
        &repo,
        &[
            "*.log".into(),
            "target".into(),
            "/out/".into(),
            "*.log".into(),
        ],
    )
    .unwrap();

    assert_eq!(read(dir.path(), ".gitignore"), "target\n*.log\n/out/\n");
}

#[test]
fn ignore_paths_creates_the_gitignore() {
    let (dir, repo) = init_repo();

    ignore_paths(&repo, &["a.log".into(), "b.log".into()]).unwrap();

    assert_eq!(read(dir.path(), ".gitignore"), "a.log\nb.log\n");
}

#[test]
fn ignore_paths_writes_nothing_when_one_pattern_is_invalid() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), ".gitignore", "target\n");

    let error = ignore_paths(&repo, &["*.log".into(), "a\nb".into()]).unwrap_err();

    assert!(matches!(error, StageError::InvalidPattern), "{error:?}");
    assert_eq!(read(dir.path(), ".gitignore"), "target\n");
}

#[test]
fn ignore_paths_of_nothing_creates_no_gitignore() {
    let (dir, repo) = init_repo();

    ignore_paths(&repo, &[]).unwrap();

    assert!(!dir.path().join(".gitignore").exists());
}

#[test]
fn stage_paths_leaves_no_index_file_when_there_was_none_and_a_path_fails() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), "folder/inner.txt", "i\n");
    assert!(!repo.path().join("index").exists());

    let result = stage_paths(&repo, &["a.txt".into(), "folder".into()]);

    assert!(result.is_err(), "{result:?}");
    assert!(!repo.path().join("index").exists());
    assert!(staged(&repo).is_empty());
}

#[test]
fn stage_paths_failing_before_any_index_write_reports_that_failure_alone() {
    // Nothing wrote an index file: putting "no index file" back is no failure of its own.
    let (dir, repo) = init_repo();
    write_file(dir.path(), "folder/inner.txt", "i\n");
    let single = git_core::stage::stage_file(&repo, "folder")
        .unwrap_err()
        .to_string();
    assert!(!repo.path().join("index").exists());

    let error = stage_paths(&repo, &["folder".into()]).unwrap_err();

    assert_eq!(error.to_string(), single);
    assert!(!repo.path().join("index").exists());
}
