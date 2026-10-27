use std::path::{Path, PathBuf};

use config::{
    get_graph_branch_selection_at, get_merge_preference_at, set_graph_branch_selection_at,
    set_merge_preference_at, MergeFastForward, MergePreference,
};

#[test]
fn get_merge_preference_at_returns_none_for_a_repo_with_no_saved_preference() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    let result = get_merge_preference_at(&config_file, Path::new("/repo/a")).unwrap();

    assert_eq!(result, None);
}

#[test]
fn set_merge_preference_at_persists_and_round_trips() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let repo_path = PathBuf::from("/repo/a");

    set_merge_preference_at(&config_file, &repo_path, MergeFastForward::Never, true).unwrap();

    assert_eq!(
        get_merge_preference_at(&config_file, &repo_path).unwrap(),
        Some(MergePreference {
            repo_path: repo_path.clone(),
            fast_forward: MergeFastForward::Never,
            squash: true,
        })
    );
}

#[test]
fn set_merge_preference_at_overwrites_a_previous_preference_for_the_same_repo() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let repo_path = PathBuf::from("/repo/a");

    set_merge_preference_at(&config_file, &repo_path, MergeFastForward::Never, true).unwrap();
    set_merge_preference_at(&config_file, &repo_path, MergeFastForward::Only, false).unwrap();

    let result = get_merge_preference_at(&config_file, &repo_path)
        .unwrap()
        .unwrap();
    assert_eq!(result.fast_forward, MergeFastForward::Only);
    assert!(!result.squash);
}

#[test]
fn set_merge_preference_at_keeps_preferences_for_other_repos_separate() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    set_merge_preference_at(
        &config_file,
        Path::new("/repo/a"),
        MergeFastForward::Only,
        false,
    )
    .unwrap();
    set_merge_preference_at(
        &config_file,
        Path::new("/repo/b"),
        MergeFastForward::Allow,
        true,
    )
    .unwrap();

    let a = get_merge_preference_at(&config_file, Path::new("/repo/a"))
        .unwrap()
        .unwrap();
    let b = get_merge_preference_at(&config_file, Path::new("/repo/b"))
        .unwrap()
        .unwrap();
    assert_eq!((a.fast_forward, a.squash), (MergeFastForward::Only, false));
    assert_eq!((b.fast_forward, b.squash), (MergeFastForward::Allow, true));
}

#[test]
fn merge_preferences_coexist_with_graph_selections() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let repo_path = PathBuf::from("/repo/a");

    set_merge_preference_at(&config_file, &repo_path, MergeFastForward::Never, false).unwrap();
    set_graph_branch_selection_at(&config_file, &repo_path, &["main".to_string()]).unwrap();
    set_merge_preference_at(&config_file, &repo_path, MergeFastForward::Only, false).unwrap();

    assert_eq!(
        get_graph_branch_selection_at(&config_file, &repo_path).unwrap(),
        Some(vec!["main".to_string()])
    );
    assert_eq!(
        get_merge_preference_at(&config_file, &repo_path)
            .unwrap()
            .unwrap()
            .fast_forward,
        MergeFastForward::Only
    );
}
