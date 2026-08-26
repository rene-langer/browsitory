use config::{get_last_seen_version_at, set_last_seen_version_at};

#[test]
fn get_last_seen_version_at_returns_none_when_never_set() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    let result = get_last_seen_version_at(&config_file).unwrap();

    assert_eq!(result, None);
}

#[test]
fn set_last_seen_version_at_persists_and_round_trips() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    set_last_seen_version_at(&config_file, "0.5.0").unwrap();

    let result = get_last_seen_version_at(&config_file).unwrap();
    assert_eq!(result, Some("0.5.0".to_string()));
}

#[test]
fn set_last_seen_version_at_overwrites_a_previous_value() {
    let dir = tempfile::TempDir::new().unwrap();
    let config_file = dir.path().join("config.toml");

    set_last_seen_version_at(&config_file, "0.5.0").unwrap();
    set_last_seen_version_at(&config_file, "0.6.0").unwrap();

    let result = get_last_seen_version_at(&config_file).unwrap();
    assert_eq!(result, Some("0.6.0".to_string()));
}
