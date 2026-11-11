use std::path::{Path, PathBuf};

use config::{
    add_recent_repo_at, list_missing_recent_repos_at, list_open_repos_at, list_pinned_repos_at,
    list_recent_repos_at, pin_recent_repo_at, remove_recent_repo_at, set_open_repos_at,
    unpin_recent_repo_at, OpenRepoEntry,
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

#[test]
fn remove_recent_repo_at_removes_only_that_entry_and_keeps_the_order() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    for repo in ["/repo/a", "/repo/b", "/repo/c"] {
        add_recent_repo_at(&config_file, Path::new(repo)).unwrap();
    }

    remove_recent_repo_at(&config_file, Path::new("/repo/b")).unwrap();

    assert_eq!(
        list_recent_repos_at(&config_file).unwrap(),
        vec![PathBuf::from("/repo/c"), PathBuf::from("/repo/a")]
    );
}

#[test]
fn remove_recent_repo_at_ignores_a_path_that_isnt_listed() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    add_recent_repo_at(&config_file, Path::new("/repo/a")).unwrap();

    remove_recent_repo_at(&config_file, Path::new("/repo/other")).unwrap();

    assert_eq!(
        list_recent_repos_at(&config_file).unwrap(),
        vec![PathBuf::from("/repo/a")]
    );
}

#[test]
fn list_missing_recent_repos_at_lists_the_entries_whose_folder_is_gone() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let present = dir.path().join("present");
    std::fs::create_dir(&present).unwrap();
    let file = dir.path().join("a-file");
    std::fs::write(&file, "").unwrap();
    let gone = dir.path().join("gone");
    for repo in [&gone, &present, &file] {
        add_recent_repo_at(&config_file, repo).unwrap();
    }

    assert_eq!(
        list_missing_recent_repos_at(&config_file).unwrap(),
        vec![file, gone]
    );
}

#[test]
fn list_missing_recent_repos_at_is_empty_without_a_config_file() {
    let dir = tempfile::TempDir::new().unwrap();

    assert!(
        list_missing_recent_repos_at(&dir.path().join("config.toml"))
            .unwrap()
            .is_empty()
    );
}

/// `/repo/0` (oldest) .. `/repo/{n-1}` (newest) added in order.
fn with_recent(config_file: &Path, n: usize) {
    for i in 0..n {
        add_recent_repo_at(config_file, &PathBuf::from(format!("/repo/{i}"))).unwrap();
    }
}

fn paths(names: &[&str]) -> Vec<PathBuf> {
    names.iter().map(PathBuf::from).collect()
}

#[test]
fn a_pinned_repo_comes_first_in_pin_order() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    with_recent(&config_file, 4);

    pin_recent_repo_at(&config_file, Path::new("/repo/1")).unwrap();
    pin_recent_repo_at(&config_file, Path::new("/repo/2")).unwrap();

    assert_eq!(
        list_recent_repos_at(&config_file).unwrap(),
        paths(&["/repo/1", "/repo/2", "/repo/3", "/repo/0"])
    );
    assert_eq!(
        list_pinned_repos_at(&config_file).unwrap(),
        paths(&["/repo/1", "/repo/2"])
    );
}

#[test]
fn pinning_twice_keeps_one_entry_in_its_place() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    with_recent(&config_file, 3);
    pin_recent_repo_at(&config_file, Path::new("/repo/0")).unwrap();
    pin_recent_repo_at(&config_file, Path::new("/repo/1")).unwrap();

    pin_recent_repo_at(&config_file, Path::new("/repo/0")).unwrap();

    assert_eq!(
        list_pinned_repos_at(&config_file).unwrap(),
        paths(&["/repo/0", "/repo/1"])
    );
    assert_eq!(
        list_recent_repos_at(&config_file).unwrap(),
        paths(&["/repo/0", "/repo/1", "/repo/2"])
    );
}

#[test]
fn pinned_repos_do_not_count_against_the_recent_cap() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    with_recent(&config_file, 10);
    for i in 0..3 {
        pin_recent_repo_at(&config_file, &PathBuf::from(format!("/repo/{i}"))).unwrap();
    }

    with_recent_from(&config_file, 10, 20);

    let listed = list_recent_repos_at(&config_file).unwrap();
    assert_eq!(listed.len(), 13, "{listed:?}");
    assert_eq!(listed[..3], paths(&["/repo/0", "/repo/1", "/repo/2"]));
    assert_eq!(listed[3], PathBuf::from("/repo/19"));
    assert_eq!(listed[12], PathBuf::from("/repo/10"));
}

fn with_recent_from(config_file: &Path, from: usize, to: usize) {
    for i in from..to {
        add_recent_repo_at(config_file, &PathBuf::from(format!("/repo/{i}"))).unwrap();
    }
}

#[test]
fn opening_a_pinned_repo_keeps_it_pinned_in_its_place() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    with_recent(&config_file, 3);
    pin_recent_repo_at(&config_file, Path::new("/repo/0")).unwrap();
    pin_recent_repo_at(&config_file, Path::new("/repo/1")).unwrap();

    add_recent_repo_at(&config_file, Path::new("/repo/1")).unwrap();

    assert_eq!(
        list_recent_repos_at(&config_file).unwrap(),
        paths(&["/repo/0", "/repo/1", "/repo/2"])
    );
}

#[test]
fn unpinning_puts_the_repo_at_the_top_of_the_recent_ones() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    with_recent(&config_file, 3);
    pin_recent_repo_at(&config_file, Path::new("/repo/0")).unwrap();
    pin_recent_repo_at(&config_file, Path::new("/repo/1")).unwrap();

    unpin_recent_repo_at(&config_file, Path::new("/repo/0")).unwrap();

    assert_eq!(
        list_pinned_repos_at(&config_file).unwrap(),
        paths(&["/repo/1"])
    );
    assert_eq!(
        list_recent_repos_at(&config_file).unwrap(),
        paths(&["/repo/1", "/repo/0", "/repo/2"])
    );
}

#[test]
fn unpinning_keeps_the_recent_cap() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    with_recent(&config_file, 11);
    pin_recent_repo_at(&config_file, Path::new("/repo/10")).unwrap();
    with_recent_from(&config_file, 11, 12);

    unpin_recent_repo_at(&config_file, Path::new("/repo/10")).unwrap();

    let listed = list_recent_repos_at(&config_file).unwrap();
    assert_eq!(listed.len(), 10, "{listed:?}");
    assert_eq!(listed[0], PathBuf::from("/repo/10"));
    assert_eq!(listed[9], PathBuf::from("/repo/2"));
}

#[test]
fn unpinning_a_repo_that_isnt_pinned_changes_nothing() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    with_recent(&config_file, 3);

    unpin_recent_repo_at(&config_file, Path::new("/repo/0")).unwrap();
    unpin_recent_repo_at(&config_file, Path::new("/repo/other")).unwrap();

    assert_eq!(
        list_recent_repos_at(&config_file).unwrap(),
        paths(&["/repo/2", "/repo/1", "/repo/0"])
    );
}

#[test]
fn removing_a_pinned_repo_unpins_it_and_drops_it() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    with_recent(&config_file, 2);
    pin_recent_repo_at(&config_file, Path::new("/repo/0")).unwrap();

    remove_recent_repo_at(&config_file, Path::new("/repo/0")).unwrap();

    assert!(list_pinned_repos_at(&config_file).unwrap().is_empty());
    assert_eq!(
        list_recent_repos_at(&config_file).unwrap(),
        paths(&["/repo/1"])
    );
}

#[test]
fn a_pinned_repo_whose_folder_is_gone_stays_pinned_and_is_listed_as_missing() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let present = dir.path().join("present");
    std::fs::create_dir(&present).unwrap();
    let gone = dir.path().join("gone");
    let gone_too = dir.path().join("gone-too");
    for repo in [&gone_too, &present, &gone] {
        add_recent_repo_at(&config_file, repo).unwrap();
    }
    pin_recent_repo_at(&config_file, &gone_too).unwrap();

    assert_eq!(
        list_missing_recent_repos_at(&config_file).unwrap(),
        vec![gone_too.clone(), gone]
    );
    assert_eq!(list_pinned_repos_at(&config_file).unwrap(), vec![gone_too]);
}

#[test]
fn pins_are_stored_in_the_config_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    with_recent(&config_file, 2);

    pin_recent_repo_at(&config_file, Path::new("/repo/0")).unwrap();

    let text = std::fs::read_to_string(&config_file).unwrap();
    assert!(text.contains("pinned_repos = [\"/repo/0\"]"), "{text}");
    assert!(text.contains("recent_repos = [\"/repo/1\"]"), "{text}");
}

#[test]
fn a_config_file_without_pins_lists_none() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    std::fs::write(&config_file, "recent_repos = [\"/repo/a\"]\n").unwrap();

    assert!(list_pinned_repos_at(&config_file).unwrap().is_empty());
    assert_eq!(
        list_recent_repos_at(&config_file).unwrap(),
        paths(&["/repo/a"])
    );
}
