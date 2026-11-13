// The user's keyboard shortcuts (#898): `[keybindings]` in config.toml, a shortcut id to its keys.
use std::collections::BTreeMap;

use config::{get_keybinding_settings_at, set_keybinding_settings_at, KeybindingSettings};

fn settings(entries: &[(&str, &[&str])]) -> KeybindingSettings {
    KeybindingSettings {
        overrides: entries
            .iter()
            .map(|(id, keys)| {
                (
                    id.to_string(),
                    keys.iter().map(|key| key.to_string()).collect(),
                )
            })
            .collect::<BTreeMap<_, _>>(),
    }
}

#[test]
fn no_overrides_when_never_set() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    assert_eq!(
        get_keybinding_settings_at(&file).unwrap(),
        KeybindingSettings::default()
    );
    assert!(get_keybinding_settings_at(&file)
        .unwrap()
        .overrides
        .is_empty());
}

#[test]
fn round_trips_as_a_table_next_to_the_other_settings() {
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

    let rebound = settings(&[
        ("refresh", &["F4"]),
        ("push", &[]),
        ("next-pane", &["F6", "mod+alt+p"]),
    ]);
    set_keybinding_settings_at(&file, &rebound).unwrap();

    assert_eq!(get_keybinding_settings_at(&file).unwrap(), rebound);
    assert_eq!(
        config::get_fetch_settings_at(&file)
            .unwrap()
            .auto_fetch_minutes,
        10
    );
    assert_eq!(
        config::get_last_seen_version_at(&file).unwrap().as_deref(),
        Some("1.0.0")
    );
    assert_eq!(config::list_workspaces_at(&file).unwrap().len(), 1);
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains(
            "[keybindings]\nnext-pane = [\"F6\", \"mod+alt+p\"]\npush = []\nrefresh = [\"F4\"]\n"
        ),
        "{text}"
    );
}

#[test]
fn no_overrides_write_no_table() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    set_keybinding_settings_at(&file, &settings(&[("refresh", &["F4"])])).unwrap();
    set_keybinding_settings_at(&file, &KeybindingSettings::default()).unwrap();
    assert!(!std::fs::read_to_string(&file)
        .unwrap()
        .contains("keybindings"));
    assert!(get_keybinding_settings_at(&file)
        .unwrap()
        .overrides
        .is_empty());
}

#[test]
fn a_hand_edited_entry_that_is_no_list_of_keys_is_dropped_and_the_rest_kept() {
    let dir = tempfile::TempDir::new().unwrap();
    let file = dir.path().join("config.toml");
    std::fs::write(
        &file,
        "last_seen_version = \"1.0.0\"\n\n[keybindings]\nrefresh = \"F4\"\npush = [1, 2]\npull = [\"mod+shift+p\"]\nfetch = { key = \"F8\" }\n",
    )
    .unwrap();
    assert_eq!(
        get_keybinding_settings_at(&file).unwrap(),
        settings(&[("pull", &["mod+shift+p"])])
    );
    assert_eq!(
        config::get_last_seen_version_at(&file).unwrap().as_deref(),
        Some("1.0.0")
    );
}
