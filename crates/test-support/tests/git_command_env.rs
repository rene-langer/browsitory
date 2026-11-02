//! `test_support::git_command` must not hand the git CLI the GIT_* environment this process
//! has, even where the hermetic guard didn't run or a test set it later (#282 review). This
//! binary has no `hermetic_git!` and exactly one test, so changing its own environment races
//! with nothing.

use std::path::Path;

#[test]
fn git_command_drops_inherited_git_env() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let root = test_support::paths().home.parent().unwrap().to_path_buf();
    let repo_dir = root.join(format!("git-command-env-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&repo_dir);
    git2::Repository::init(&repo_dir).unwrap();
    let elsewhere = root.join(format!("git-command-env-elsewhere-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&elsewhere);
    git2::Repository::init(&elsewhere).unwrap();

    // What a hook or `git rebase -x` in another repository hands its children.
    std::env::set_var("GIT_DIR", elsewhere.join(".git"));
    std::env::set_var("GIT_WORK_TREE", &elsewhere);
    std::env::set_var("GIT_INDEX_FILE", elsewhere.join(".git/index"));
    std::env::set_var("GIT_CONFIG_PARAMETERS", "'user.name'='From Parameters'");
    std::env::set_var("GIT_CONFIG_COUNT", "1");
    std::env::set_var("GIT_CONFIG_KEY_0", "user.email");
    std::env::set_var("GIT_CONFIG_VALUE_0", "from-count@example.com");

    let command = test_support::git_command(&repo_dir);
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_CONFIG_PARAMETERS",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_KEY_0",
        "GIT_CONFIG_VALUE_0",
    ] {
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == name && value.is_none()),
            "{name} is not removed"
        );
    }

    let git_dir = test_support::git(&repo_dir, &["rev-parse", "--absolute-git-dir"]);
    assert!(git_dir.status.success(), "{git_dir:?}");
    let git_dir = String::from_utf8_lossy(&git_dir.stdout).trim().to_string();
    assert_eq!(
        Path::new(&git_dir).canonicalize().unwrap(),
        repo_dir.join(".git").canonicalize().unwrap()
    );
    let config = test_support::git(&repo_dir, &["config", "--list", "--show-scope"]);
    let config = String::from_utf8_lossy(&config.stdout);
    assert!(
        !config.lines().any(|line| line.starts_with("command")),
        "{config}"
    );

    let _ = std::fs::remove_dir_all(&repo_dir);
    let _ = std::fs::remove_dir_all(&elsewhere);
}
