use std::path::{Path, PathBuf};

use config::{add_recent_repo_at, list_recent_repos_at};

#[test]
fn list_recent_repos_at_returns_empty_for_a_missing_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    assert!(list_recent_repos_at(&config_file).unwrap().is_empty());
}

#[test]
fn add_recent_repo_at_persists_across_calls() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    add_recent_repo_at(&config_file, Path::new("/repo/a")).unwrap();

    assert_eq!(
        list_recent_repos_at(&config_file).unwrap(),
        vec![PathBuf::from("/repo/a")]
    );
}

#[test]
fn add_recent_repo_at_puts_the_newest_entry_first() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    add_recent_repo_at(&config_file, Path::new("/repo/a")).unwrap();
    add_recent_repo_at(&config_file, Path::new("/repo/b")).unwrap();

    assert_eq!(
        list_recent_repos_at(&config_file).unwrap(),
        vec![PathBuf::from("/repo/b"), PathBuf::from("/repo/a")]
    );
}

#[test]
fn add_recent_repo_at_deduplicates_and_moves_to_front() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    add_recent_repo_at(&config_file, Path::new("/repo/a")).unwrap();
    add_recent_repo_at(&config_file, Path::new("/repo/b")).unwrap();
    add_recent_repo_at(&config_file, Path::new("/repo/a")).unwrap();

    let result = list_recent_repos_at(&config_file).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(
        result,
        vec![PathBuf::from("/repo/a"), PathBuf::from("/repo/b")]
    );
}

#[test]
fn add_recent_repo_at_caps_at_ten_entries() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    for i in 0..=10 {
        add_recent_repo_at(&config_file, Path::new(&format!("/repo/{i}"))).unwrap();
    }

    let result = list_recent_repos_at(&config_file).unwrap();
    assert_eq!(result.len(), 10);
    assert!(!result.contains(&PathBuf::from("/repo/0")));
}
