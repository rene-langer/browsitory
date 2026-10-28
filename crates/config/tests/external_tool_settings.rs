use config::{
    get_diff_settings_at, get_external_tool_settings_at, set_diff_settings_at,
    set_external_tool_settings_at, DiffSettings, ExternalToolSettings,
};

#[test]
fn defaults_to_nothing_configured() {
    let dir = tempfile::TempDir::new().unwrap();
    assert_eq!(
        get_external_tool_settings_at(&dir.path().join("config.toml")).unwrap(),
        ExternalToolSettings::default()
    );
}

#[test]
fn round_trips_next_to_the_diff_table() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    let diff = DiffSettings {
        ignore_whitespace: true,
        context_lines: 5,
        syntax_highlight: true,
    };
    set_diff_settings_at(&file, &diff).unwrap();
    let wanted = ExternalToolSettings {
        editor: Some("code --goto \"{file}:{line}\"".into()),
        terminal: Some("kitty".into()),
    };
    set_external_tool_settings_at(&file, &wanted).unwrap();
    assert_eq!(get_external_tool_settings_at(&file).unwrap(), wanted);
    assert_eq!(get_diff_settings_at(&file).unwrap(), diff);
    assert!(std::fs::read_to_string(&file).unwrap().contains("[tools]"));
}

#[test]
fn blank_commands_are_stored_as_unset() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    set_external_tool_settings_at(
        &file,
        &ExternalToolSettings {
            editor: Some("  ".into()),
            terminal: Some(" xterm ".into()),
        },
    )
    .unwrap();
    assert_eq!(
        get_external_tool_settings_at(&file).unwrap(),
        ExternalToolSettings {
            editor: None,
            terminal: Some("xterm".into())
        }
    );
}
