//! The hermetic-git guard must hide every global/XDG/system git config from both libgit2 (in
//! this process) and spawned `git` subprocesses, whatever the developer's or CI runner's own
//! config says (#111).

test_support::hermetic_git!();

use std::process::Command;

#[test]
fn libgit2_sees_no_global_xdg_or_system_config() {
    let config = git2::Config::open_default().expect("open default config");
    let mut entries = config.entries(None).expect("list entries");
    let mut leaked = Vec::new();
    while let Some(entry) = entries.next() {
        let entry = entry.expect("entry");
        leaked.push(format!(
            "{:?} {}={}",
            entry.level(),
            entry.name().unwrap_or("?"),
            entry.value().unwrap_or("?")
        ));
    }
    assert!(leaked.is_empty(), "leaked config: {leaked:?}");
}

#[test]
fn spawned_git_sees_no_global_or_system_config() {
    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let output = Command::new("git")
        .args(["config", "--list", "--show-scope"])
        .output()
        .expect("run git config");
    let listed = String::from_utf8_lossy(&output.stdout);
    let leaked: Vec<&str> = listed
        .lines()
        .filter(|line| line.starts_with("global") || line.starts_with("system"))
        .collect();
    assert!(leaked.is_empty(), "leaked config: {leaked:?}");
}

#[test]
fn the_guard_is_idempotent_and_keeps_its_directories() {
    test_support::install();
    let paths = test_support::paths();
    test_support::install();
    assert!(paths.search_dir.is_dir());
    assert!(paths.home.is_dir());
    assert!(std::fs::read_dir(&paths.search_dir)
        .unwrap()
        .next()
        .is_none());
    assert_eq!(
        std::env::var_os("HOME").as_deref(),
        Some(paths.home.as_os_str())
    );
}
