use config::{get_diff_settings_at, set_diff_settings_at, set_last_seen_version_at, DiffSettings};

#[test]
fn defaults_when_never_set() {
    let dir = tempfile::TempDir::new().unwrap();
    let s = get_diff_settings_at(&dir.path().join("config.toml")).unwrap();
    assert_eq!(
        s,
        DiffSettings {
            ignore_whitespace: false,
            context_lines: 3,
            syntax_highlight: true
        }
    );
}

#[test]
fn round_trips_and_coexists_with_other_keys() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    set_last_seen_version_at(&file, "1.0.0").unwrap();
    let wanted = DiffSettings {
        ignore_whitespace: true,
        context_lines: 7,
        syntax_highlight: false,
    };
    set_diff_settings_at(&file, &wanted).unwrap();
    assert_eq!(get_diff_settings_at(&file).unwrap(), wanted);
    assert_eq!(
        config::get_last_seen_version_at(&file).unwrap().as_deref(),
        Some("1.0.0")
    );
}

#[test]
fn clamps_context_lines_on_write() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    set_diff_settings_at(
        &file,
        &DiffSettings {
            ignore_whitespace: false,
            context_lines: 99,
            syntax_highlight: true,
        },
    )
    .unwrap();
    assert_eq!(get_diff_settings_at(&file).unwrap().context_lines, 20);
}

#[test]
fn a_partial_diff_table_fills_missing_fields_with_defaults() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    std::fs::write(&file, "[diff]\nignore_whitespace = true\n").unwrap();
    let s = get_diff_settings_at(&file).unwrap();
    assert!(s.ignore_whitespace);
    assert_eq!(s.context_lines, 3);
    assert!(s.syntax_highlight);
}
