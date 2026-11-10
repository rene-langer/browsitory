use std::path::{Path, PathBuf};

use config::{
    get_merge_preference_at, get_pull_preference_at, set_merge_preference_at,
    set_pull_preference_at, MergeFastForward, PullStrategyPreference,
};

#[test]
fn a_repo_with_no_saved_pull_strategy_has_none() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    let result = get_pull_preference_at(&config_file, Path::new("/repo/a")).unwrap();

    assert_eq!(result, None);
}

#[test]
fn the_pull_strategy_is_kept_per_repo_and_the_last_choice_wins() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let a = PathBuf::from("/repo/a");
    let b = PathBuf::from("/repo/b");

    set_pull_preference_at(&config_file, &a, PullStrategyPreference::Merge).unwrap();
    set_pull_preference_at(&config_file, &b, PullStrategyPreference::FastForwardOnly).unwrap();
    set_pull_preference_at(&config_file, &a, PullStrategyPreference::Rebase).unwrap();

    assert_eq!(
        get_pull_preference_at(&config_file, &a).unwrap(),
        Some(PullStrategyPreference::Rebase)
    );
    assert_eq!(
        get_pull_preference_at(&config_file, &b).unwrap(),
        Some(PullStrategyPreference::FastForwardOnly)
    );
}

#[test]
fn saving_a_pull_strategy_keeps_the_merge_preference() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");
    let repo = PathBuf::from("/repo/a");

    set_merge_preference_at(&config_file, &repo, MergeFastForward::Never, false).unwrap();
    set_pull_preference_at(&config_file, &repo, PullStrategyPreference::Rebase).unwrap();

    let merge = get_merge_preference_at(&config_file, &repo)
        .unwrap()
        .unwrap();
    assert_eq!(merge.fast_forward, MergeFastForward::Never);
}
