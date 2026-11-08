use config::{
    get_fetch_settings_at, set_fetch_settings_at, set_last_seen_version_at, FetchSettings,
};

#[test]
fn background_fetch_is_off_when_never_set() {
    let dir = tempfile::TempDir::new().unwrap();
    let settings = get_fetch_settings_at(&dir.path().join("config.toml")).unwrap();
    assert_eq!(
        settings,
        FetchSettings {
            auto_fetch_minutes: 0
        }
    );
}

#[test]
fn round_trips_and_coexists_with_other_keys_and_tables() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    set_last_seen_version_at(&file, "1.0.0").unwrap();
    config::set_diff_settings_at(
        &file,
        &config::DiffSettings {
            ignore_whitespace: true,
            ..Default::default()
        },
    )
    .unwrap();
    config::save_workspace_at(&file, "Work", dir.path(), &[]).unwrap();

    set_fetch_settings_at(
        &file,
        &FetchSettings {
            auto_fetch_minutes: 10,
        },
    )
    .unwrap();

    assert_eq!(get_fetch_settings_at(&file).unwrap().auto_fetch_minutes, 10);
    assert_eq!(
        config::get_last_seen_version_at(&file).unwrap().as_deref(),
        Some("1.0.0")
    );
    assert!(
        config::get_diff_settings_at(&file)
            .unwrap()
            .ignore_whitespace
    );
    assert_eq!(config::list_workspaces_at(&file).unwrap().len(), 1);
    assert!(std::fs::read_to_string(&file)
        .unwrap()
        .contains("[fetch]\nauto_fetch_minutes = 10"));
}

#[test]
fn clamps_the_interval_to_at_most_a_day_on_write() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    set_fetch_settings_at(
        &file,
        &FetchSettings {
            auto_fetch_minutes: 100_000,
        },
    )
    .unwrap();
    assert_eq!(
        get_fetch_settings_at(&file).unwrap().auto_fetch_minutes,
        config::MAX_AUTO_FETCH_MINUTES
    );
}

#[test]
fn an_empty_fetch_table_means_off() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    std::fs::write(&file, "[fetch]\n").unwrap();
    assert_eq!(get_fetch_settings_at(&file).unwrap().auto_fetch_minutes, 0);
}
