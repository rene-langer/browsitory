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

/// The git CLI twin helper (#282) points HOME, the global config and GNUPGHOME at the hermetic
/// directories itself, whatever this process's environment says, and asks for English messages.
#[test]
fn git_command_runs_git_isolated_from_the_users_config_and_gpg() {
    let paths = test_support::paths();
    let dir = std::env::temp_dir();
    let command = test_support::git_command(&dir);
    let envs: std::collections::HashMap<_, _> = command
        .get_envs()
        .map(|(key, value)| (key.to_owned(), value.map(|v| v.to_owned())))
        .collect();
    let env = |name: &str| envs.get(std::ffi::OsStr::new(name)).cloned().flatten();
    assert_eq!(command.get_program(), "git");
    assert_eq!(command.get_current_dir(), Some(dir.as_path()));
    assert_eq!(env("HOME").as_deref(), Some(paths.home.as_os_str()));
    assert_eq!(
        env("GIT_CONFIG_GLOBAL").as_deref(),
        Some(paths.global_config.as_os_str())
    );
    assert_eq!(env("GIT_CONFIG_NOSYSTEM").as_deref(), Some("1".as_ref()));
    assert_eq!(
        env("GNUPGHOME").as_deref(),
        Some(paths.gnupg_home.as_os_str())
    );
    assert_eq!(env("LC_ALL").as_deref(), Some("C".as_ref()));
    assert!(paths.gnupg_home.is_dir());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&paths.gnupg_home)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o700,
            "gpg refuses a group/world-readable GNUPGHOME"
        );
    }
    assert_eq!(std::fs::read_to_string(&paths.global_config).unwrap(), "");

    if Command::new("git").arg("--version").output().is_err() {
        return;
    }
    let output = test_support::git(&dir, &["config", "--global", "--list"]);
    assert!(
        output.stdout.is_empty(),
        "global config: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
