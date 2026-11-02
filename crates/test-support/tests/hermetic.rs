//! The hermetic-git guard must hide every global/XDG/system git config from both libgit2 (in
//! this process) and spawned `git` subprocesses, whatever the developer's or CI runner's own
//! config says (#111).

test_support::hermetic_git!();

use std::process::Command;

/// Not a test on its own: the child process of
/// `the_guard_clears_inherited_git_env_and_isolates_gpg_before_main`. Prints what the guard left
/// of the environment it was started with.
#[test]
#[ignore = "child process of the_guard_clears_inherited_git_env_and_isolates_gpg_before_main"]
fn hermetic_env_child() {
    if std::env::var_os("BROWSITORY_HERMETIC_ENV_CHILD").is_none() {
        return;
    }
    let show = |name: &str| match std::env::var_os(name) {
        Some(value) => format!("{name}={}", value.to_string_lossy()),
        None => format!("{name} unset"),
    };
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
        "GIT_AUTHOR_NAME",
        "GNUPGHOME",
        "LC_ALL",
    ] {
        println!("env: {}", show(name));
    }
}

/// #282 review: started with the GIT_* environment of a hook (or `rebase -x`) in a real
/// repository and the developer's GNUPGHOME, a test binary sees none of it from `main` on, so
/// neither its own `git`, nor hook scripts, nor child processes act on the real repository or
/// talk to the real gpg agent.
#[test]
fn the_guard_clears_inherited_git_env_and_isolates_gpg_before_main() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "hermetic_env_child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("BROWSITORY_HERMETIC_ENV_CHILD", "1")
        .env("GIT_DIR", "/nonexistent/real-repo/.git")
        .env("GIT_WORK_TREE", "/nonexistent/real-repo")
        .env("GIT_INDEX_FILE", "/nonexistent/real-repo/.git/index")
        .env("GIT_CONFIG_PARAMETERS", "'commit.gpgsign'='true'")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_AUTHOR_NAME", "Hook Author")
        .env("GNUPGHOME", "/nonexistent/real-gnupg")
        .env("LC_ALL", "de_DE.UTF-8")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let env: Vec<&str> = stdout
        .lines()
        // The harness's "test hermetic_env_child ... " may share the first line.
        .filter_map(|line| line.split_once("env: ").map(|(_, rest)| rest))
        .collect();
    let gnupg = format!(
        "GNUPGHOME={}",
        test_support::paths().gnupg_home.to_string_lossy()
    );
    assert_eq!(
        env,
        [
            "GIT_DIR unset",
            "GIT_WORK_TREE unset",
            "GIT_INDEX_FILE unset",
            "GIT_CONFIG_PARAMETERS unset",
            "GIT_CONFIG_COUNT unset",
            "GIT_AUTHOR_NAME unset",
            gnupg.as_str(),
            "LC_ALL=C",
        ]
    );
}

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
