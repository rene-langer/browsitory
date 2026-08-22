use std::path::PathBuf;

use config::{
    delete_workspace_at, list_open_repos_at, list_workspaces_at, save_workspace_at,
    set_open_repos_at, update_workspace_at, ConfigError,
};

#[test]
fn save_workspace_at_creates_a_workspace_returned_by_list_workspaces_at() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    let id = save_workspace_at(
        &config_file,
        "My Services",
        &PathBuf::from("/projects/root"),
        &[
            PathBuf::from("/projects/root/a"),
            PathBuf::from("/projects/root/b"),
        ],
    )
    .unwrap();

    let workspaces = list_workspaces_at(&config_file).unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].id, id);
    assert_eq!(workspaces[0].name, "My Services");
    assert_eq!(workspaces[0].root_path, PathBuf::from("/projects/root"));
    assert_eq!(
        workspaces[0].member_paths,
        vec![
            PathBuf::from("/projects/root/a"),
            PathBuf::from("/projects/root/b")
        ]
    );
}

#[test]
fn save_workspace_at_generates_distinct_ids_for_each_call() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    let id1 = save_workspace_at(&config_file, "One", &PathBuf::from("/a"), &[]).unwrap();
    let id2 = save_workspace_at(&config_file, "Two", &PathBuf::from("/b"), &[]).unwrap();

    assert_ne!(id1, id2);
}

#[test]
fn save_workspace_at_dedupes_a_colliding_name() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    save_workspace_at(&config_file, "project", &PathBuf::from("/clone-1"), &[]).unwrap();
    save_workspace_at(&config_file, "project", &PathBuf::from("/clone-2"), &[]).unwrap();
    save_workspace_at(&config_file, "project", &PathBuf::from("/clone-3"), &[]).unwrap();

    let mut names: Vec<String> = list_workspaces_at(&config_file)
        .unwrap()
        .into_iter()
        .map(|w| w.name)
        .collect();
    names.sort();
    assert_eq!(names, vec!["project", "project (2)", "project (3)"]);
}

#[test]
fn list_workspaces_at_on_a_missing_file_returns_empty() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    assert!(list_workspaces_at(&config_file).unwrap().is_empty());
}

#[test]
fn save_workspace_at_preserves_open_repos_from_a_legacy_config() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    std::fs::write(
        &config_file,
        "open_repos = [\"/repos/a\", \"/repos/b\"]\nactive_repo = \"/repos/b\"\n",
    )
    .unwrap();

    save_workspace_at(&config_file, "Workspace", &PathBuf::from("/repos"), &[]).unwrap();

    let (entries, active) = list_open_repos_at(&config_file).unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>(),
        vec![PathBuf::from("/repos/a"), PathBuf::from("/repos/b")]
    );
    assert!(entries.iter().all(|entry| entry.workspace_id.is_none()));
    assert_eq!(active, Some(PathBuf::from("/repos/b")));
}

#[test]
fn a_current_config_parse_error_is_not_rewritten_as_an_empty_legacy_config() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let malformed = r#"recent_repos = ["/repos/recent"]

[[workspaces]]
id = "ws-1"
name = "Services"
root_path = "/projects"
member_paths = "not-an-array"
"#;
    std::fs::write(&config_file, malformed).unwrap();

    let error = set_open_repos_at(&config_file, &[], None).unwrap_err();

    assert!(matches!(error, ConfigError::Parse(_)));
    assert_eq!(std::fs::read_to_string(&config_file).unwrap(), malformed);
}

#[test]
fn update_workspace_at_changes_name_and_members() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let id = save_workspace_at(
        &config_file,
        "Original",
        &PathBuf::from("/root"),
        &[PathBuf::from("/root/a")],
    )
    .unwrap();

    update_workspace_at(
        &config_file,
        &id,
        "Renamed",
        &[PathBuf::from("/root/a"), PathBuf::from("/root/c")],
    )
    .unwrap();

    let workspaces = list_workspaces_at(&config_file).unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].name, "Renamed");
    assert_eq!(
        workspaces[0].member_paths,
        vec![PathBuf::from("/root/a"), PathBuf::from("/root/c")]
    );
    assert_eq!(workspaces[0].root_path, PathBuf::from("/root"));
}

#[test]
fn update_workspace_at_on_an_unknown_id_is_a_no_op() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    save_workspace_at(&config_file, "Kept", &PathBuf::from("/root"), &[]).unwrap();

    update_workspace_at(&config_file, "nonexistent-id", "Ignored", &[]).unwrap();

    let workspaces = list_workspaces_at(&config_file).unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].name, "Kept");
}

#[test]
fn delete_workspace_at_removes_only_the_matching_workspace() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let keep_id = save_workspace_at(&config_file, "Keep", &PathBuf::from("/keep"), &[]).unwrap();
    let remove_id =
        save_workspace_at(&config_file, "Remove", &PathBuf::from("/remove"), &[]).unwrap();

    delete_workspace_at(&config_file, &remove_id).unwrap();

    let workspaces = list_workspaces_at(&config_file).unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].id, keep_id);
}

#[test]
fn delete_workspace_at_on_an_unknown_id_is_a_no_op() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    save_workspace_at(&config_file, "Kept", &PathBuf::from("/root"), &[]).unwrap();

    delete_workspace_at(&config_file, "nonexistent-id").unwrap();

    assert_eq!(list_workspaces_at(&config_file).unwrap().len(), 1);
}
