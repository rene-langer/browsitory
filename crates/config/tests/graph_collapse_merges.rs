use std::path::Path;

use config::{
    get_graph_branch_selection_at, get_graph_collapse_merges_at, set_graph_branch_selection_at,
    set_graph_collapse_merges_at,
};

#[test]
fn collapse_merges_is_off_for_a_repo_never_set() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    let collapse = get_graph_collapse_merges_at(&config_file, Path::new("/repo/a")).unwrap();

    assert!(!collapse);
}

#[test]
fn collapse_merges_round_trips_per_repo() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let (a, b) = (Path::new("/repo/a"), Path::new("/repo/b"));

    set_graph_collapse_merges_at(&config_file, a, true).unwrap();
    set_graph_collapse_merges_at(&config_file, a, true).unwrap();
    set_graph_collapse_merges_at(&config_file, b, true).unwrap();
    set_graph_collapse_merges_at(&config_file, b, false).unwrap();

    assert!(get_graph_collapse_merges_at(&config_file, a).unwrap());
    assert!(!get_graph_collapse_merges_at(&config_file, b).unwrap());
    let contents = std::fs::read_to_string(&config_file).unwrap();
    assert_eq!(contents.matches("/repo/a").count(), 1, "{contents}");
}

#[test]
fn collapse_merges_is_written_alongside_the_tables_of_other_settings() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let repo = Path::new("/repo/a");

    set_graph_branch_selection_at(&config_file, repo, &["main".to_string()]).unwrap();
    set_graph_collapse_merges_at(&config_file, repo, true).unwrap();

    assert!(get_graph_collapse_merges_at(&config_file, repo).unwrap());
    let selection = get_graph_branch_selection_at(&config_file, repo).unwrap();
    assert_eq!(selection, Some(vec!["main".to_string()]));
}
