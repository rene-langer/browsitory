use std::path::{Path, PathBuf};

use config::{
    list_open_repos_at, list_repo_aliases_at, set_open_repos_at, set_repo_alias_at, OpenRepoEntry,
    RepoAlias,
};

fn alias(path: &str, name: &str) -> RepoAlias {
    RepoAlias {
        repo_path: PathBuf::from(path),
        alias: name.to_string(),
    }
}

#[test]
fn lists_no_aliases_for_a_missing_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    assert_eq!(list_repo_aliases_at(&file).unwrap(), Vec::new());
}

#[test]
fn round_trips_an_alias_per_repository() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");

    set_repo_alias_at(&file, Path::new("/work/app"), Some("app (main)")).unwrap();
    set_repo_alias_at(&file, Path::new("/work/app-2"), Some("app (review)")).unwrap();

    assert_eq!(
        list_repo_aliases_at(&file).unwrap(),
        vec![
            alias("/work/app", "app (main)"),
            alias("/work/app-2", "app (review)"),
        ]
    );
}

#[test]
fn replaces_the_alias_of_the_same_repository() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");

    set_repo_alias_at(&file, Path::new("/work/app"), Some("first")).unwrap();
    set_repo_alias_at(&file, Path::new("/work/app"), Some("second")).unwrap();

    assert_eq!(
        list_repo_aliases_at(&file).unwrap(),
        vec![alias("/work/app", "second")]
    );
}

#[test]
fn trims_the_alias_on_write() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");

    set_repo_alias_at(&file, Path::new("/work/app"), Some("  app  ")).unwrap();

    assert_eq!(
        list_repo_aliases_at(&file).unwrap(),
        vec![alias("/work/app", "app")]
    );
}

#[test]
fn none_or_a_blank_alias_removes_it_and_keeps_the_others() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    set_repo_alias_at(&file, Path::new("/work/a"), Some("A")).unwrap();
    set_repo_alias_at(&file, Path::new("/work/b"), Some("B")).unwrap();
    set_repo_alias_at(&file, Path::new("/work/c"), Some("C")).unwrap();

    set_repo_alias_at(&file, Path::new("/work/a"), None).unwrap();
    set_repo_alias_at(&file, Path::new("/work/b"), Some("   ")).unwrap();

    assert_eq!(
        list_repo_aliases_at(&file).unwrap(),
        vec![alias("/work/c", "C")]
    );
}

#[test]
fn removing_an_alias_that_was_never_set_is_a_no_op() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");

    set_repo_alias_at(&file, Path::new("/work/a"), None).unwrap();

    assert_eq!(list_repo_aliases_at(&file).unwrap(), Vec::new());
}

#[test]
fn aliases_outlive_the_open_tabs_and_coexist_with_other_keys() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    let entry = OpenRepoEntry {
        path: PathBuf::from("/work/app"),
        workspace_id: None,
    };
    set_open_repos_at(&file, &[entry], Some(Path::new("/work/app"))).unwrap();
    set_repo_alias_at(&file, Path::new("/work/app"), Some("app")).unwrap();
    config::set_fetch_settings_at(
        &file,
        &config::FetchSettings {
            auto_fetch_minutes: 5,
        },
    )
    .unwrap();

    // Closing every tab leaves the alias in place.
    set_open_repos_at(&file, &[], None).unwrap();

    assert_eq!(
        list_repo_aliases_at(&file).unwrap(),
        vec![alias("/work/app", "app")]
    );
    assert!(list_open_repos_at(&file).unwrap().0.is_empty());
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("[[repo_aliases]]\n"), "{text}");
}
