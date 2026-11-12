use config::{
    get_appearance_settings_at, set_appearance_settings_at, set_last_seen_version_at,
    AppearanceSettings, UiSize,
};

#[test]
fn keeps_the_built_in_look_when_never_set() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    let settings = get_appearance_settings_at(&file).unwrap();
    assert_eq!(
        settings,
        AppearanceSettings {
            code_font: None,
            code_font_size: None,
            ui_size: UiSize::Normal,
            tab_size: 4,
        }
    );
    assert_eq!(settings, AppearanceSettings::default());
}

#[test]
fn round_trips_and_coexists_with_other_keys_and_tables() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    set_last_seen_version_at(&file, "1.0.0").unwrap();
    config::set_fetch_settings_at(
        &file,
        &config::FetchSettings {
            auto_fetch_minutes: 10,
        },
    )
    .unwrap();
    config::save_workspace_at(&file, "Work", dir.path(), &[]).unwrap();
    let wanted = AppearanceSettings {
        code_font: Some("Example Mono".to_string()),
        code_font_size: Some(15),
        ui_size: UiSize::Large,
        tab_size: 8,
    };

    set_appearance_settings_at(&file, &wanted).unwrap();

    assert_eq!(get_appearance_settings_at(&file).unwrap(), wanted);
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
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("[appearance]\n"));
    assert!(text.contains("code_font = \"Example Mono\"\n"));
    assert!(text.contains("ui_size = \"large\"\n"));
}

#[test]
fn clamps_out_of_range_sizes_and_drops_a_blank_font_on_write() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    let cases = [
        (1, 0, 10, 1),
        (99, 99, 24, 16),
        (10, 1, 10, 1),
        (24, 16, 24, 16),
    ];
    for (font_size, tab_size, want_font_size, want_tab_size) in cases {
        set_appearance_settings_at(
            &file,
            &AppearanceSettings {
                code_font: Some("   ".to_string()),
                code_font_size: Some(font_size),
                ui_size: UiSize::Small,
                tab_size,
            },
        )
        .unwrap();
        let got = get_appearance_settings_at(&file).unwrap();
        assert_eq!(got.code_font, None);
        assert_eq!(got.code_font_size, Some(want_font_size));
        assert_eq!(got.tab_size, want_tab_size);
        assert_eq!(got.ui_size, UiSize::Small);
    }
}

#[test]
fn trims_the_font_family_on_write() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    set_appearance_settings_at(
        &file,
        &AppearanceSettings {
            code_font: Some("  Other Mono ".to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let got = get_appearance_settings_at(&file).unwrap();
    assert_eq!(got.code_font.as_deref(), Some("Other Mono"));
}

#[test]
fn clamps_a_hand_edited_file_on_read() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    std::fs::write(
        &file,
        "[appearance]\ncode_font = \"\"\ncode_font_size = 200\ntab_size = 0\n",
    )
    .unwrap();
    assert_eq!(
        get_appearance_settings_at(&file).unwrap(),
        AppearanceSettings {
            code_font: None,
            code_font_size: Some(24),
            ui_size: UiSize::Normal,
            tab_size: 1,
        }
    );
}

#[test]
fn an_empty_appearance_table_means_the_defaults() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    std::fs::write(&file, "[appearance]\n").unwrap();
    assert_eq!(
        get_appearance_settings_at(&file).unwrap(),
        AppearanceSettings::default()
    );
}

// #675: the date format is stored as `date_format = "iso"` or `"system"`, ISO by default. Written
// as TOML text so the test reads the file the way a user (or an older version) would.
#[test]
fn keeps_the_date_format_and_writes_iso_by_default() {
    let dir = tempfile::TempDir::new().unwrap();
    let read = dir.path().join("read.toml");
    let written = dir.path().join("written.toml");
    std::fs::write(&read, "[appearance]\ndate_format = \"system\"\n").unwrap();

    let settings = get_appearance_settings_at(&read).unwrap();
    set_appearance_settings_at(&written, &settings).unwrap();
    let text = std::fs::read_to_string(&written).unwrap();
    assert!(text.contains("date_format = \"system\"\n"), "{text}");

    set_appearance_settings_at(&written, &AppearanceSettings::default()).unwrap();
    let text = std::fs::read_to_string(&written).unwrap();
    assert!(text.contains("date_format = \"iso\"\n"), "{text}");
}
