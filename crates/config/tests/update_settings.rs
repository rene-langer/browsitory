use config::{get_update_settings_at, set_update_settings_at, UpdateSettings};

#[test]
fn checks_automatically_when_never_set() {
    let dir = tempfile::TempDir::new().unwrap();
    let settings = get_update_settings_at(&dir.path().join("config.toml")).unwrap();
    assert_eq!(
        settings,
        UpdateSettings {
            check_automatically: true
        }
    );
    assert_eq!(UpdateSettings::default(), settings);
}

#[test]
fn an_empty_updates_table_means_on() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    std::fs::write(&file, "[updates]\n").unwrap();
    assert!(get_update_settings_at(&file).unwrap().check_automatically);
}

#[test]
fn round_trips_off_and_back_on_and_coexists_with_other_keys_and_tables() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    config::set_last_seen_version_at(&file, "1.0.0").unwrap();
    config::set_fetch_settings_at(
        &file,
        &config::FetchSettings {
            auto_fetch_minutes: 10,
        },
    )
    .unwrap();
    config::save_workspace_at(&file, "Work", dir.path(), &[]).unwrap();

    set_update_settings_at(
        &file,
        &UpdateSettings {
            check_automatically: false,
        },
    )
    .unwrap();

    assert!(!get_update_settings_at(&file).unwrap().check_automatically);
    assert!(std::fs::read_to_string(&file)
        .unwrap()
        .contains("[updates]\ncheck_automatically = false"));
    assert_eq!(
        config::get_last_seen_version_at(&file).unwrap().as_deref(),
        Some("1.0.0")
    );
    assert_eq!(
        config::get_fetch_settings_at(&file)
            .unwrap()
            .auto_fetch_minutes,
        10
    );
    assert_eq!(config::list_workspaces_at(&file).unwrap().len(), 1);

    set_update_settings_at(
        &file,
        &UpdateSettings {
            check_automatically: true,
        },
    )
    .unwrap();
    assert!(get_update_settings_at(&file).unwrap().check_automatically);
}
