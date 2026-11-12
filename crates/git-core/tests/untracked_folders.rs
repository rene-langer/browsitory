//! #845: an untracked folder with more than `UNTRACKED_FOLDER_THRESHOLD` files is listed as one
//! entry; smaller ones are listed file by file, as before. Staging or deleting the folder entry
//! acts on all its files, as `git add dir/` and `git clean dir/` do.

mod common;

use std::path::Path;

use common::{commit_all, init_repo, write_file};
use git2::Repository;
use git_core::change_stamp::{change_stamp, status_with_stamp};
use git_core::stage::{discard_file, stage_file, stage_paths};
use git_core::status::{
    status, status_refreshing_index, untracked_folder_files, StatusEntry, StatusKind,
    UNTRACKED_FOLDER_THRESHOLD,
};

fn committed_repo() -> (tempfile::TempDir, Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "one\n");
    write_file(dir.path(), ".gitignore", "*.log\n");
    commit_all(&repo, "first");
    (dir, repo)
}

/// `count` files in `folder`, half of them one level deeper.
fn fill(workdir: &Path, folder: &str, count: usize) {
    for i in 0..count {
        let sub = if i % 2 == 0 { "" } else { "deep/" };
        write_file(workdir, &format!("{folder}{sub}f{i}.txt"), "x\n");
    }
}

/// The untracked files libgit2 lists file by file: the status before #845.
fn untracked_file_by_file(repo: &Repository) -> Vec<String> {
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let mut paths: Vec<String> = repo
        .statuses(Some(&mut options))
        .unwrap()
        .iter()
        .filter(|entry| entry.status().is_wt_new())
        .map(|entry| entry.path().unwrap().to_string())
        .collect();
    paths.sort();
    paths
}

fn untracked_paths(entries: &[StatusEntry]) -> Vec<String> {
    let mut paths: Vec<String> = entries
        .iter()
        .filter(|entry| !entry.staged && entry.kind == StatusKind::New)
        .map(|entry| entry.path.clone())
        .collect();
    paths.sort();
    paths
}

fn folder_entry(path: &str) -> StatusEntry {
    StatusEntry {
        path: path.to_string(),
        old_path: None,
        staged: false,
        kind: StatusKind::New,
        untracked_folder: true,
    }
}

#[test]
fn the_threshold_is_500_files() {
    assert_eq!(UNTRACKED_FOLDER_THRESHOLD, 500);
}

#[test]
fn a_small_untracked_folder_is_listed_file_by_file() {
    let (dir, repo) = committed_repo();
    fill(dir.path(), "small/", 6);
    write_file(dir.path(), "loose.txt", "x\n");

    let entries = status(&repo).unwrap();

    assert_eq!(untracked_paths(&entries), untracked_file_by_file(&repo));
    assert!(entries.iter().all(|entry| !entry.untracked_folder));
    assert!(untracked_paths(&entries).contains(&"small/deep/f1.txt".to_string()));
}

#[test]
fn a_folder_of_exactly_the_threshold_is_listed_file_by_file() {
    let (dir, repo) = committed_repo();
    fill(dir.path(), "edge/", UNTRACKED_FOLDER_THRESHOLD);

    let entries = status(&repo).unwrap();

    assert_eq!(entries.len(), UNTRACKED_FOLDER_THRESHOLD);
    assert!(entries.iter().all(|entry| !entry.untracked_folder));
}

#[test]
fn a_larger_untracked_folder_is_one_entry() {
    let (dir, repo) = committed_repo();
    fill(dir.path(), "big/", UNTRACKED_FOLDER_THRESHOLD + 1);
    fill(dir.path(), "small/", 2);
    write_file(dir.path(), "tracked.txt", "changed\n");

    let entries = status(&repo).unwrap();

    assert!(entries.contains(&folder_entry("big/")), "{entries:?}");
    assert!(!entries.iter().any(|entry| entry.path.starts_with("big/f")));
    assert_eq!(
        untracked_paths(&entries),
        vec!["big/", "small/f0.txt", "small/deep/f1.txt"]
    );
    assert!(entries
        .iter()
        .any(|entry| entry.path == "tracked.txt" && entry.kind == StatusKind::Modified));
    // The app's own reads answer the same.
    assert_eq!(status_refreshing_index(&repo).unwrap(), entries);
    assert_eq!(status_with_stamp(&repo).unwrap().status, entries);
}

#[test]
fn ignored_files_in_a_folder_dont_count() {
    let (dir, repo) = committed_repo();
    fill(dir.path(), "mixed/", 400);
    for i in 0..200 {
        write_file(dir.path(), &format!("mixed/i{i}.log"), "x\n");
    }

    let entries = status(&repo).unwrap();

    assert_eq!(entries.len(), 400);
    assert_eq!(untracked_paths(&entries), untracked_file_by_file(&repo));
}

#[test]
fn a_nested_repository_in_a_small_folder_stays_one_entry() {
    let (dir, repo) = committed_repo();
    write_file(dir.path(), "outer/f.txt", "x\n");
    Repository::init(dir.path().join("outer/inner")).unwrap();
    write_file(dir.path(), "outer/inner/g.txt", "x\n");

    let entries = status(&repo).unwrap();

    assert_eq!(untracked_paths(&entries), untracked_file_by_file(&repo));
    assert!(untracked_paths(&entries).contains(&"outer/inner/".to_string()));
}

#[test]
fn the_stamp_follows_files_in_a_small_folder_and_matches_the_combined_read() {
    let (dir, repo) = committed_repo();
    fill(dir.path(), "small/", 4);
    fill(dir.path(), "big/", UNTRACKED_FOLDER_THRESHOLD + 1);
    let before = change_stamp(&repo).unwrap();
    assert_eq!(status_with_stamp(&repo).unwrap().stamp, before);

    write_file(dir.path(), "small/deep/f1.txt", "longer content\n");

    let after = change_stamp(&repo).unwrap();
    assert_ne!(after.work_tree, before.work_tree);
    assert_eq!(status_with_stamp(&repo).unwrap().stamp, after);
}

#[test]
fn the_files_of_a_large_folder_are_listed_on_request() {
    let (dir, repo) = committed_repo();
    fill(dir.path(), "big/", UNTRACKED_FOLDER_THRESHOLD + 1);
    write_file(dir.path(), "big/skip.log", "x\n");

    let files = untracked_folder_files(&repo, "big/").unwrap();

    assert_eq!(files.len(), UNTRACKED_FOLDER_THRESHOLD + 1);
    assert!(files
        .iter()
        .all(|entry| entry.kind == StatusKind::New && !entry.staged && !entry.untracked_folder));
    let mut paths: Vec<String> = files.into_iter().map(|entry| entry.path).collect();
    paths.sort();
    let mut expected: Vec<String> = untracked_file_by_file(&repo);
    expected.retain(|path| path.starts_with("big/"));
    assert_eq!(paths, expected);
}

#[test]
fn listing_refuses_a_path_outside_the_work_tree_or_not_a_folder() {
    let (dir, repo) = committed_repo();
    fill(dir.path(), "big/", 3);
    assert!(untracked_folder_files(&repo, "../big/").is_err());
    assert!(untracked_folder_files(&repo, "/big/").is_err());
    assert!(untracked_folder_files(&repo, "tracked.txt").is_err());
}

#[test]
fn staging_the_folder_entry_stages_all_its_files_but_not_ignored_ones() {
    let (dir, repo) = committed_repo();
    fill(dir.path(), "big/", UNTRACKED_FOLDER_THRESHOLD + 1);
    write_file(dir.path(), "big/skip.log", "x\n");

    stage_file(&repo, "big/").unwrap();

    let index = git2::Index::open(&repo.path().join("index")).unwrap();
    assert!(index.get_path(Path::new("big/f0.txt"), 0).is_some());
    assert!(index.get_path(Path::new("big/deep/f1.txt"), 0).is_some());
    assert!(index.get_path(Path::new("big/skip.log"), 0).is_none());
    assert_eq!(index.len(), 2 + UNTRACKED_FOLDER_THRESHOLD + 1);
}

#[test]
fn staging_several_paths_takes_a_folder_entry_too() {
    let (dir, repo) = committed_repo();
    fill(dir.path(), "big/", UNTRACKED_FOLDER_THRESHOLD + 1);
    write_file(dir.path(), "loose.txt", "x\n");

    stage_paths(&repo, &["big/".to_string(), "loose.txt".to_string()]).unwrap();

    assert!(status(&repo).unwrap().iter().all(|entry| entry.staged));
}

#[test]
fn deleting_the_folder_entry_deletes_its_files_and_keeps_ignored_ones() {
    let (dir, repo) = committed_repo();
    fill(dir.path(), "big/", UNTRACKED_FOLDER_THRESHOLD + 1);
    write_file(dir.path(), "big/deep/keep.log", "x\n");
    write_file(dir.path(), "tracked.txt", "changed\n");

    discard_file(&repo, "big/").unwrap();

    assert!(!dir.path().join("big/f0.txt").exists());
    assert!(!dir.path().join("big/deep/f1.txt").exists());
    assert!(dir.path().join("big/deep/keep.log").exists());
    // Tracked changes stay.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("tracked.txt")).unwrap(),
        "changed\n"
    );
}

#[test]
fn deleting_a_folder_entry_removes_the_emptied_folders() {
    let (dir, repo) = committed_repo();
    fill(dir.path(), "big/", UNTRACKED_FOLDER_THRESHOLD + 1);

    discard_file(&repo, "big/").unwrap();

    assert!(!dir.path().join("big").exists());
}
