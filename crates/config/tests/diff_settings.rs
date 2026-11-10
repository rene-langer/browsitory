use config::{
    get_diff_settings_at, set_diff_settings_at, set_last_seen_version_at, DiffAlgorithm,
    DiffSettings, WordWrap,
};

#[test]
fn defaults_when_never_set() {
    let dir = tempfile::TempDir::new().unwrap();
    let s = get_diff_settings_at(&dir.path().join("config.toml")).unwrap();
    assert_eq!(
        s,
        DiffSettings {
            ignore_whitespace: false,
            context_lines: 3,
            syntax_highlight: true,
            word_wrap: WordWrap::Auto,
            algorithm: DiffAlgorithm::Default,
            show_whitespace: false,
            expand_untracked: true,
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
        word_wrap: WordWrap::Off,
        algorithm: DiffAlgorithm::Patience,
        show_whitespace: true,
        expand_untracked: false,
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
            word_wrap: WordWrap::On,
            algorithm: DiffAlgorithm::Default,
            show_whitespace: false,
            expand_untracked: true,
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
    assert_eq!(s.word_wrap, WordWrap::Auto);
    assert_eq!(s.algorithm, DiffAlgorithm::Default);
    assert!(!s.show_whitespace);
    assert!(s.expand_untracked);
}

#[test]
fn word_wrap_is_stored_as_a_lowercase_word() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    set_diff_settings_at(
        &file,
        &DiffSettings {
            word_wrap: WordWrap::Off,
            ..DiffSettings::default()
        },
    )
    .unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("word_wrap = \"off\""), "{text}");
    std::fs::write(&file, "[diff]\nword_wrap = \"on\"\n").unwrap();
    assert_eq!(get_diff_settings_at(&file).unwrap().word_wrap, WordWrap::On);
}

#[test]
fn algorithm_is_stored_as_a_lowercase_word() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    set_diff_settings_at(
        &file,
        &DiffSettings {
            algorithm: DiffAlgorithm::Patience,
            ..DiffSettings::default()
        },
    )
    .unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("algorithm = \"patience\""), "{text}");
    std::fs::write(&file, "[diff]\nalgorithm = \"minimal\"\n").unwrap();
    assert_eq!(
        get_diff_settings_at(&file).unwrap().algorithm,
        DiffAlgorithm::Minimal
    );
}

#[test]
fn show_whitespace_round_trips_and_an_older_table_loads_it_as_off() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    std::fs::write(&file, "[diff]\nignore_whitespace = true\n").unwrap();
    assert!(!get_diff_settings_at(&file).unwrap().show_whitespace);
    set_diff_settings_at(
        &file,
        &DiffSettings {
            show_whitespace: true,
            ..DiffSettings::default()
        },
    )
    .unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("show_whitespace = true"), "{text}");
    assert!(get_diff_settings_at(&file).unwrap().show_whitespace);
}

#[test]
fn expand_untracked_round_trips_and_an_older_table_loads_it_as_on() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    std::fs::write(&file, "[diff]\nshow_whitespace = true\n").unwrap();
    assert!(get_diff_settings_at(&file).unwrap().expand_untracked);
    set_diff_settings_at(
        &file,
        &DiffSettings {
            expand_untracked: false,
            ..DiffSettings::default()
        },
    )
    .unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("expand_untracked = false"), "{text}");
    assert!(!get_diff_settings_at(&file).unwrap().expand_untracked);
}
