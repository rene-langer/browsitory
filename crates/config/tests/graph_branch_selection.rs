use std::path::{Path, PathBuf};

use config::{get_graph_branch_selection_at, set_graph_branch_selection_at};

#[test]
fn get_graph_branch_selection_at_returns_none_for_a_repo_with_no_saved_selection() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    let result = get_graph_branch_selection_at(&config_file, Path::new("/repo/a")).unwrap();

    assert_eq!(result, None);
}

#[test]
fn set_graph_branch_selection_at_persists_and_round_trips() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let repo_path = PathBuf::from("/repo/a");

    set_graph_branch_selection_at(&config_file, &repo_path, &["main".to_string()]).unwrap();

    let result = get_graph_branch_selection_at(&config_file, &repo_path).unwrap();
    assert_eq!(result, Some(vec!["main".to_string()]));
}

#[test]
fn set_graph_branch_selection_at_overwrites_a_previous_selection_for_the_same_repo() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let repo_path = PathBuf::from("/repo/a");

    set_graph_branch_selection_at(&config_file, &repo_path, &["main".to_string()]).unwrap();
    set_graph_branch_selection_at(
        &config_file,
        &repo_path,
        &["main".to_string(), "feature".to_string()],
    )
    .unwrap();

    let result = get_graph_branch_selection_at(&config_file, &repo_path).unwrap();
    assert_eq!(
        result,
        Some(vec!["main".to_string(), "feature".to_string()])
    );
}

#[test]
fn set_graph_branch_selection_at_keeps_selections_for_other_repos_separate() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    set_graph_branch_selection_at(&config_file, Path::new("/repo/a"), &["main".to_string()])
        .unwrap();
    set_graph_branch_selection_at(&config_file, Path::new("/repo/b"), &["dev".to_string()])
        .unwrap();

    assert_eq!(
        get_graph_branch_selection_at(&config_file, Path::new("/repo/a")).unwrap(),
        Some(vec!["main".to_string()])
    );
    assert_eq!(
        get_graph_branch_selection_at(&config_file, Path::new("/repo/b")).unwrap(),
        Some(vec!["dev".to_string()])
    );
}
