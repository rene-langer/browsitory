//! The webview's capability grants only what the frontend calls from JavaScript (#1053). The
//! Rust commands call the dialog and opener plugins directly, which needs no permission; a
//! grant would let any script in the webview open URLs and reveal paths past
//! `open_external_url`'s checks.

use std::path::Path;

fn permissions() -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities/default.json");
    let text = std::fs::read_to_string(&path).unwrap();
    let capability: serde_json::Value = serde_json::from_str(&text).unwrap();
    capability["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|permission| permission.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_default_capability_grants_only_what_the_frontend_calls() {
    assert_eq!(
        permissions(),
        [
            "core:default",
            "updater:default",
            "process:allow-restart",
            "log:default",
        ]
    );
}

#[test]
fn the_capabilities_folder_holds_only_the_default_capability() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("capabilities");
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["default.json"]);
}
