//! User commands (#893): app-wide and per repository, both in the app's config file.

use std::path::Path;

use config::{
    get_external_tool_settings_at, list_user_commands_at, set_external_tool_settings_at,
    set_user_commands_at, ConfigError, ExternalToolSettings, UserCommand, UserCommandTarget,
    UserCommands,
};

fn command(name: &str, command_line: &str) -> UserCommand {
    UserCommand {
        id: String::new(),
        name: name.to_string(),
        command_line: command_line.to_string(),
        working_directory: None,
        targets: Vec::new(),
        confirm: false,
        show_output: true,
        refresh: true,
    }
}

fn config_file() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    (dir, file)
}

#[test]
fn there_are_none_by_default() {
    let (_dir, file) = config_file();
    assert_eq!(
        list_user_commands_at(&file, Some(Path::new("/repos/a"))).unwrap(),
        UserCommands::default()
    );
}

#[test]
fn app_wide_commands_round_trip_in_order_with_new_ids() {
    let (_dir, file) = config_file();
    let mut lint = command("Lint", "make lint");
    lint.targets = vec![UserCommandTarget::File, UserCommandTarget::Commit];
    lint.confirm = true;
    lint.show_output = false;
    lint.refresh = false;
    lint.working_directory = Some("${repo}/web".to_string());
    let stored = set_user_commands_at(
        &file,
        None,
        &[lint.clone(), command("Log", "git log -1 ${commit}")],
    )
    .unwrap();

    assert_eq!(stored.len(), 2);
    assert!(!stored[0].id.is_empty());
    assert!(!stored[1].id.is_empty());
    assert_ne!(stored[0].id, stored[1].id);
    assert_eq!(stored[0].name, "Lint");
    assert_eq!(stored[0].command_line, "make lint");
    assert_eq!(stored[0].working_directory.as_deref(), Some("${repo}/web"));
    assert_eq!(
        stored[0].targets,
        [UserCommandTarget::Commit, UserCommandTarget::File]
    );
    assert!(stored[0].confirm);
    assert!(!stored[0].show_output);
    assert!(!stored[0].refresh);
    assert_eq!(stored[1].name, "Log");

    let listed = list_user_commands_at(&file, None).unwrap();
    assert_eq!(listed.global, stored);
    assert!(listed.repository.is_empty());
    // App-wide commands show for every repository.
    assert_eq!(
        list_user_commands_at(&file, Some(Path::new("/repos/a")))
            .unwrap()
            .global,
        stored
    );
}

#[test]
fn keeps_given_ids_and_replaces_repeated_ones() {
    let (_dir, file) = config_file();
    let mut first = command("One", "true");
    first.id = "keep-me".to_string();
    let mut second = command("Two", "true");
    second.id = "keep-me".to_string();
    let stored = set_user_commands_at(&file, None, &[first, second]).unwrap();
    assert_eq!(stored[0].id, "keep-me");
    assert!(!stored[1].id.is_empty());
    assert_ne!(stored[1].id, "keep-me");
}

#[test]
fn repository_commands_belong_to_their_repository_only() {
    let (_dir, file) = config_file();
    let a = Path::new("/repos/a");
    let stored =
        set_user_commands_at(&file, Some(a), &[command("Deploy", "deploy ${branch}")]).unwrap();

    let in_a = list_user_commands_at(&file, Some(a)).unwrap();
    assert_eq!(in_a.repository, stored);
    assert!(in_a.global.is_empty());
    assert!(list_user_commands_at(&file, Some(Path::new("/repos/b")))
        .unwrap()
        .repository
        .is_empty());
    assert!(list_user_commands_at(&file, None)
        .unwrap()
        .repository
        .is_empty());
}

#[test]
fn an_empty_list_removes_the_repository_entry() {
    let (_dir, file) = config_file();
    let a = Path::new("/repos/only-here");
    set_user_commands_at(&file, Some(a), &[command("X", "true")]).unwrap();
    assert!(std::fs::read_to_string(&file)
        .unwrap()
        .contains("/repos/only-here"));
    assert!(set_user_commands_at(&file, Some(a), &[])
        .unwrap()
        .is_empty());
    assert!(!std::fs::read_to_string(&file)
        .unwrap()
        .contains("/repos/only-here"));
    assert!(list_user_commands_at(&file, Some(a))
        .unwrap()
        .repository
        .is_empty());
}

#[test]
fn trims_names_and_directories_and_sorts_targets_without_repeats() {
    let (_dir, file) = config_file();
    let mut messy = command("  Lint  ", "make lint");
    messy.working_directory = Some("   ".to_string());
    messy.targets = vec![
        UserCommandTarget::Repository,
        UserCommandTarget::Branch,
        UserCommandTarget::Repository,
        UserCommandTarget::Tag,
    ];
    let stored = set_user_commands_at(&file, None, &[messy]).unwrap();
    assert_eq!(stored[0].name, "Lint");
    assert_eq!(stored[0].working_directory, None);
    assert_eq!(
        stored[0].targets,
        [
            UserCommandTarget::Branch,
            UserCommandTarget::Tag,
            UserCommandTarget::Repository
        ]
    );
}

#[test]
fn refuses_a_command_with_an_unknown_variable_and_stores_nothing() {
    let (_dir, file) = config_file();
    set_user_commands_at(&file, None, &[command("Kept", "true")]).unwrap();
    let error = set_user_commands_at(
        &file,
        None,
        &[command("Fine", "true"), command("Broken", "echo ${foo}")],
    )
    .unwrap_err();
    assert!(
        matches!(error, ConfigError::InvalidUserCommand(_)),
        "{error:?}"
    );
    let message = error.to_string();
    assert!(message.starts_with("Broken: "), "{message}");
    assert!(message.contains("${foo}"), "{message}");
    let listed = list_user_commands_at(&file, None).unwrap().global;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "Kept");
}

#[test]
fn refuses_a_blank_name_a_blank_command_and_an_unclosed_quote() {
    let (_dir, file) = config_file();
    let blank_name = set_user_commands_at(&file, None, &[command("  ", "true")]).unwrap_err();
    assert_eq!(blank_name.to_string(), "A command: The name is empty.");
    let blank_line = set_user_commands_at(&file, None, &[command("Empty", "  ")]).unwrap_err();
    assert_eq!(blank_line.to_string(), "Empty: The command is empty.");
    let quote = set_user_commands_at(&file, None, &[command("Quote", "echo \"x")]).unwrap_err();
    assert_eq!(
        quote.to_string(),
        "Quote: The command has an unclosed quote."
    );
    let directory = {
        let mut bad = command("Dir", "make");
        bad.working_directory = Some("${paths}".to_string());
        set_user_commands_at(&file, None, &[bad]).unwrap_err()
    };
    assert_eq!(
        directory.to_string(),
        "Dir: The working directory can't use ${paths}."
    );
    assert!(!file.exists());
}

#[test]
fn lives_next_to_the_other_settings() {
    let (_dir, file) = config_file();
    let tools = ExternalToolSettings {
        editor: Some("code".into()),
        terminal: None,
    };
    set_external_tool_settings_at(&file, &tools).unwrap();
    set_user_commands_at(&file, None, &[command("A", "true")]).unwrap();
    set_user_commands_at(&file, Some(Path::new("/repos/a")), &[command("B", "true")]).unwrap();
    set_external_tool_settings_at(&file, &tools).unwrap();

    assert_eq!(get_external_tool_settings_at(&file).unwrap(), tools);
    let listed = list_user_commands_at(&file, Some(Path::new("/repos/a"))).unwrap();
    assert_eq!(listed.global[0].name, "A");
    assert_eq!(listed.repository[0].name, "B");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.contains("[[user_commands]]"), "{text}");
    assert!(text.contains("[[repo_user_commands]]"), "{text}");
}

#[test]
fn reads_hand_written_commands_with_defaults_and_listing_ids() {
    let (_dir, file) = config_file();
    std::fs::write(
        &file,
        r#"
[[user_commands]]
name = "Status"
command_line = "git status"

[[user_commands]]
id = "mine"
name = "Show"
command_line = "git show ${commit}"
targets = ["commit"]

[[repo_user_commands]]
repo_path = "/repos/a"

[[repo_user_commands.commands]]
name = "Here"
command_line = "make"
"#,
    )
    .unwrap();
    let listed = list_user_commands_at(&file, Some(Path::new("/repos/a"))).unwrap();
    assert_eq!(
        listed.global,
        [
            UserCommand {
                id: "global-1".to_string(),
                ..command("Status", "git status")
            },
            UserCommand {
                id: "mine".to_string(),
                targets: vec![UserCommandTarget::Commit],
                ..command("Show", "git show ${commit}")
            },
        ]
    );
    assert_eq!(
        listed.repository,
        [UserCommand {
            id: "repository-1".to_string(),
            ..command("Here", "make")
        }]
    );
}
