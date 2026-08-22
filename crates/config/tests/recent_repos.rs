use std::path::{Path, PathBuf};

use config::{
    add_recent_repo_at, list_open_repos_at, list_recent_repos_at, set_open_repos_at, OpenRepoEntry,
};

#[test]
fn config_file_path_env_override_is_used_when_set() {
    let dir = tempfile::TempDir::new().unwrap();
    // SAFETY (test-only): this crate's tests don't run this one in parallel with another
    // that also touches BROWSITORY_CONFIG_DIR — it's set and cleared within this single test.
    std::env::set_var("BROWSITORY_CONFIG_DIR", dir.path());
    let result = (|| -> Result<(), config::ConfigError> {
        config::add_recent_repo(std::path::Path::new("/repos/env-override-check"))?;
        let recent = config::list_recent_repos()?;
        assert_eq!(
            recent,
            vec![std::path::PathBuf::from("/repos/env-override-check")]
        );
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
fn set_open_repos_at_persists_entries_and_active_repo() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    set_open_repos_at(
        &config_file,
        &[
            OpenRepoEntry {
                path: PathBuf::from("/repos/a"),
                workspace_id: None,
            },
            OpenRepoEntry {
                path: PathBuf::from("/repos/b"),
                workspace_id: Some("ws-1".into()),
            },
        ],
        Some(&PathBuf::from("/repos/b")),
    )
    .unwrap();

    let (entries, active) = list_open_repos_at(&config_file).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].path, PathBuf::from("/repos/a"));
    assert_eq!(entries[0].workspace_id, None);
    assert_eq!(entries[1].path, PathBuf::from("/repos/b"));
    assert_eq!(entries[1].workspace_id, Some("ws-1".to_string()));
    assert_eq!(active, Some(PathBuf::from("/repos/b")));
}

#[test]
fn list_open_repos_at_on_a_missing_file_returns_empty() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    let (entries, active) = list_open_repos_at(&config_file).unwrap();
    assert!(entries.is_empty());
    assert_eq!(active, None);
}

#[test]
fn set_open_repos_at_does_not_disturb_recent_repos() {
    let dir = dir_with_recent_repo();
    let config_file = dir.0;

    set_open_repos_at(
        &config_file,
        &[OpenRepoEntry {
            path: PathBuf::from("/repos/a"),
            workspace_id: None,
        }],
        None,
    )
    .unwrap();

    let recent = list_recent_repos_at(&config_file).unwrap();
    assert_eq!(recent, vec![PathBuf::from("/repos/recent")]);
}

fn dir_with_recent_repo() -> (PathBuf, tempfile::TempDir) {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    add_recent_repo_at(&config_file, &PathBuf::from("/repos/recent")).unwrap();
    (config_file, dir)
}

#[test]
fn open_repos_round_trip_preserves_a_none_workspace_id() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    set_open_repos_at(
        &config_file,
        &[OpenRepoEntry {
            path: PathBuf::from("/repos/a"),
            workspace_id: None,
        }],
        None,
    )
    .unwrap();

    let (entries, _) = list_open_repos_at(&config_file).unwrap();
    assert_eq!(entries[0].workspace_id, None);
}

#[test]
fn list_open_repos_at_parses_a_pre_workspaces_config_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    std::fs::write(
        &config_file,
        "open_repos = [\"/repos/a\", \"/repos/b\"]\nactive_repo = \"/repos/a\"\n",
    )
    .unwrap();

    let (entries, active) = list_open_repos_at(&config_file).unwrap();

    assert_eq!(
        entries.iter().map(|e| e.path.clone()).collect::<Vec<_>>(),
        vec![PathBuf::from("/repos/a"), PathBuf::from("/repos/b")]
    );
    assert!(entries.iter().all(|e| e.workspace_id.is_none()));
    assert_eq!(active, Some(PathBuf::from("/repos/a")));
}
