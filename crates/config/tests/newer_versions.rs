//! A `config.toml` written by a newer Browsitory (#1137): the desktop app and the VS Code
//! extension update on their own, so an older version regularly reads and writes a newer one's
//! file. It keeps what it doesn't know on a write, and a value it can't read costs that setting,
//! not the file.

use std::path::{Path, PathBuf};

use config::{
    get_appearance_settings_at, get_diff_settings_at, get_merge_preference_at,
    get_pull_preference_at, list_open_repos_at, list_user_commands_at, set_diff_settings_at,
    set_last_seen_version_at, set_open_repos_at, AppearanceSettings, DateFormat, DiffAlgorithm,
    DiffSettings, MergeFastForward, OpenRepoEntry, PullStrategyPreference, UiSize,
    UserCommandTarget, WordWrap,
};

fn file_with(contents: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    std::fs::write(&file, contents).unwrap();
    (dir, file)
}

fn parsed(file: &Path) -> toml::Table {
    let text = std::fs::read_to_string(file).unwrap();
    toml::from_str(&text).unwrap_or_else(|error| panic!("{error}\n{text}"))
}

fn at<'a>(table: &'a toml::Table, path: &[&str]) -> Option<&'a toml::Value> {
    let (first, rest) = path.split_first().unwrap();
    rest.iter()
        .try_fold(table.get(*first)?, |value, key| value.get(*key))
}

const NEWER_FILE: &str = r#"recent_repos = []
future_flag = true

[diff]
context_lines = 5
future_option = "x"

[future_table]
key = "value"
"#;

#[test]
fn a_write_keeps_keys_and_tables_it_does_not_know() {
    let (_dir, file) = file_with(NEWER_FILE);

    set_last_seen_version_at(&file, "0.18.0").unwrap();

    let table = parsed(&file);
    assert_eq!(at(&table, &["last_seen_version"]), Some(&"0.18.0".into()));
    assert_eq!(at(&table, &["future_flag"]), Some(&true.into()));
    assert_eq!(at(&table, &["diff", "context_lines"]), Some(&5.into()));
    assert_eq!(at(&table, &["diff", "future_option"]), Some(&"x".into()));
    assert_eq!(at(&table, &["future_table", "key"]), Some(&"value".into()));
}

#[test]
fn changing_a_table_keeps_its_keys_it_does_not_know() {
    let (_dir, file) = file_with(NEWER_FILE);

    let settings = get_diff_settings_at(&file).unwrap();
    set_diff_settings_at(
        &file,
        &DiffSettings {
            context_lines: 7,
            ..settings
        },
    )
    .unwrap();

    let table = parsed(&file);
    assert_eq!(at(&table, &["diff", "context_lines"]), Some(&7.into()));
    assert_eq!(at(&table, &["diff", "future_option"]), Some(&"x".into()));
}

#[test]
fn a_list_entry_keeps_its_keys_it_does_not_know_when_the_list_changes() {
    let (_dir, file) = file_with(
        "[[open_repos]]\npath = \"/repos/a\"\nfuture_tab_color = \"red\"\n\n\
         [[open_repos]]\npath = \"/repos/b\"\n",
    );
    let entry = |path: &str| OpenRepoEntry {
        path: PathBuf::from(path),
        workspace_id: None,
    };

    set_open_repos_at(
        &file,
        &[entry("/repos/b"), entry("/repos/a"), entry("/repos/c")],
        None,
    )
    .unwrap();

    let table = parsed(&file);
    let open = at(&table, &["open_repos"]).unwrap().as_array().unwrap();
    let paths: Vec<_> = open.iter().map(|e| e["path"].as_str().unwrap()).collect();
    assert_eq!(paths, ["/repos/b", "/repos/a", "/repos/c"]);
    assert_eq!(open[1].get("future_tab_color"), Some(&"red".into()));
    assert_eq!(open[0].get("future_tab_color"), None);
}

#[test]
fn an_unknown_diff_value_costs_that_setting_only_and_stays_in_the_file() {
    let (_dir, file) = file_with(
        "[diff]\ncontext_lines = 5\nword_wrap = \"smart\"\nalgorithm = \"histogram\"\n\n\
         [[open_repos]]\npath = \"/repos/a\"\n",
    );

    let (open, _) = list_open_repos_at(&file).unwrap();
    assert_eq!(open.len(), 1);
    let settings = get_diff_settings_at(&file).unwrap();
    assert_eq!(settings.word_wrap, WordWrap::Auto);
    assert_eq!(settings.algorithm, DiffAlgorithm::Default);
    assert_eq!(settings.context_lines, 5);

    set_last_seen_version_at(&file, "0.18.0").unwrap();
    set_diff_settings_at(
        &file,
        &DiffSettings {
            context_lines: 6,
            ..settings
        },
    )
    .unwrap();

    let table = parsed(&file);
    assert_eq!(at(&table, &["diff", "context_lines"]), Some(&6.into()));
    assert_eq!(at(&table, &["diff", "word_wrap"]), Some(&"smart".into()));
    assert_eq!(
        at(&table, &["diff", "algorithm"]),
        Some(&"histogram".into())
    );
}

#[test]
fn choosing_a_value_replaces_the_unknown_one() {
    let (_dir, file) = file_with("[diff]\nword_wrap = \"smart\"\n");

    set_diff_settings_at(
        &file,
        &DiffSettings {
            word_wrap: WordWrap::Off,
            ..get_diff_settings_at(&file).unwrap()
        },
    )
    .unwrap();

    assert_eq!(
        get_diff_settings_at(&file).unwrap().word_wrap,
        WordWrap::Off
    );
}

#[test]
fn unknown_appearance_values_cost_those_settings_only() {
    let (_dir, file) =
        file_with("[appearance]\nui_size = \"huge\"\ndate_format = \"relative\"\ntab_size = 2\n");

    assert_eq!(
        get_appearance_settings_at(&file).unwrap(),
        AppearanceSettings {
            ui_size: UiSize::Normal,
            date_format: DateFormat::Iso,
            tab_size: 2,
            ..AppearanceSettings::default()
        }
    );
}

#[test]
fn a_remembered_merge_or_pull_choice_it_does_not_know_is_forgotten_alone() {
    let (_dir, file) = file_with(
        "[[merge_preferences]]\nrepo_path = \"/repos/a\"\nfast_forward = \"Rebase\"\nsquash = false\n\n\
         [[merge_preferences]]\nrepo_path = \"/repos/b\"\nfast_forward = \"Never\"\nsquash = true\n\n\
         [[pull_preferences]]\nrepo_path = \"/repos/a\"\nstrategy = \"Autostash\"\n\n\
         [[pull_preferences]]\nrepo_path = \"/repos/b\"\nstrategy = \"Rebase\"\n",
    );

    assert_eq!(
        get_merge_preference_at(&file, Path::new("/repos/a")).unwrap(),
        None
    );
    assert_eq!(
        get_merge_preference_at(&file, Path::new("/repos/b"))
            .unwrap()
            .map(|p| p.fast_forward),
        Some(MergeFastForward::Never)
    );
    assert_eq!(
        get_pull_preference_at(&file, Path::new("/repos/a")).unwrap(),
        None
    );
    assert_eq!(
        get_pull_preference_at(&file, Path::new("/repos/b")).unwrap(),
        Some(PullStrategyPreference::Rebase)
    );
}

#[test]
fn a_user_command_target_it_does_not_know_is_left_out() {
    let (_dir, file) = file_with(
        "[[user_commands]]\nid = \"c\"\nname = \"Show\"\ncommand_line = \"git show\"\n\
         targets = [\"commit\", \"stash\"]\n",
    );

    let commands = list_user_commands_at(&file, None).unwrap();
    assert_eq!(commands.global.len(), 1);
    assert_eq!(commands.global[0].targets, [UserCommandTarget::Commit]);
}
