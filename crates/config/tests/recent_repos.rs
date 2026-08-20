use std::path::{Path, PathBuf};

use config::{add_recent_repo_at, list_recent_repos_at, list_open_repos_at, set_open_repos_at};

#[test]
fn config_file_path_env_override_is_used_when_set() {
    let dir = tempfile::TempDir::new().unwrap();
    // SAFETY (test-only): this crate's tests don't run this one in parallel with another
    // that also touches BROWSITORY_CONFIG_DIR — it's set and cleared within this single test.
    std::env::set_var("BROWSITORY_CONFIG_DIR", dir.path());
    let result = (|| -> Result<(), config::ConfigError> {
        config::add_recent_repo(std::path::Path::new("/repos/env-override-check"))?;
        let recent = config::list_recent_repos()?;
        assert_eq!(recent, vec![std::path::PathBuf::from("/repos/env-override-check")]);
        assert!(dir.path().join("config.toml").exists());
        Ok(())
    })();
    std::env::remove_var("BROWSITORY_CONFIG_DIR");
    result.unwrap();
}

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

#[test]
fn set_open_repos_at_persists_paths_and_active_repo() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    set_open_repos_at(
        &config_file,
        &[PathBuf::from("/repos/a"), PathBuf::from("/repos/b")],
        Some(&PathBuf::from("/repos/b")),
    )
    .unwrap();

    let (paths, active) = list_open_repos_at(&config_file).unwrap();
    assert_eq!(paths, vec![PathBuf::from("/repos/a"), PathBuf::from("/repos/b")]);
    assert_eq!(active, Some(PathBuf::from("/repos/b")));
}

#[test]
fn list_open_repos_at_on_a_missing_file_returns_empty() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    let (paths, active) = list_open_repos_at(&config_file).unwrap();
    assert!(paths.is_empty());
    assert_eq!(active, None);
}

#[test]
fn set_open_repos_at_does_not_disturb_recent_repos() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    add_recent_repo_at(&config_file, &PathBuf::from("/repos/recent")).unwrap();
    set_open_repos_at(&config_file, &[PathBuf::from("/repos/a")], None).unwrap();

    let recent = list_recent_repos_at(&config_file).unwrap();
    assert_eq!(recent, vec![PathBuf::from("/repos/recent")]);
}
