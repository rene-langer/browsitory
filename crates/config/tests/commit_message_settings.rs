use config::{
    get_commit_message_settings_at, set_commit_message_settings_at, CommitMessageSettings,
};

#[test]
fn defaults_round_trip_and_clamp() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    assert_eq!(
        get_commit_message_settings_at(&file).unwrap(),
        CommitMessageSettings {
            summary_limit: 50,
            body_ruler: 72,
            show_ruler: true
        }
    );
    let custom = CommitMessageSettings {
        summary_limit: 60,
        body_ruler: 80,
        show_ruler: false,
    };
    set_commit_message_settings_at(&file, &custom).unwrap();
    assert_eq!(get_commit_message_settings_at(&file).unwrap(), custom);
    set_commit_message_settings_at(
        &file,
        &CommitMessageSettings {
            summary_limit: 1,
            body_ruler: 999,
            show_ruler: true,
        },
    )
    .unwrap();
    assert_eq!(
        get_commit_message_settings_at(&file).unwrap(),
        CommitMessageSettings {
            summary_limit: 10,
            body_ruler: 200,
            show_ruler: true
        }
    );
}

#[test]
fn commit_message_settings_coexist_with_other_tables_and_lists() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    config::add_recent_repo_at(&file, std::path::Path::new("/r")).unwrap();
    config::set_diff_settings_at(
        &file,
        &config::DiffSettings {
            ignore_whitespace: true,
            context_lines: 5,
            syntax_highlight: false,
        },
    )
    .unwrap();
    config::set_external_tool_settings_at(
        &file,
        &config::ExternalToolSettings {
            editor: Some("code".into()),
            terminal: None,
        },
    )
    .unwrap();
    config::save_workspace_at(&file, "w", std::path::Path::new("/w"), &[]).unwrap();
    set_commit_message_settings_at(
        &file,
        &CommitMessageSettings {
            summary_limit: 55,
            body_ruler: 72,
            show_ruler: true,
        },
    )
    .unwrap();
    assert_eq!(
        config::get_diff_settings_at(&file).unwrap().context_lines,
        5
    );
    assert_eq!(
        config::get_external_tool_settings_at(&file)
            .unwrap()
            .editor
            .as_deref(),
        Some("code")
    );
    assert_eq!(config::list_recent_repos_at(&file).unwrap().len(), 1);
    assert_eq!(
        get_commit_message_settings_at(&file).unwrap().summary_limit,
        55
    );
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("[commit_message]"), "{text}");
}

#[test]
fn a_partial_table_fills_in_defaults() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    std::fs::write(&file, "[commit_message]\nsummary_limit = 65\n").unwrap();
    assert_eq!(
        get_commit_message_settings_at(&file).unwrap(),
        CommitMessageSettings {
            summary_limit: 65,
            body_ruler: 72,
            show_ruler: true
        }
    );
}
