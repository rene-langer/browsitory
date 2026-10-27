//! Real-repo hook tests with shell-script hooks (Unix only: they need `sh` and an execute bit).
#![cfg(unix)]
mod common;

use std::time::Duration;

use git_core::hooks::{
    find_hook, hooks_dir, installed_hooks, run_hook, HookInvocation, HookLookup, HookOutcome,
    COMMIT_HOOKS,
};

fn run(repo: &git2::Repository, name: &str, args: &[&str]) -> HookOutcome {
    run_hook(
        repo,
        &HookInvocation {
            name,
            args,
            stdin: None,
            env: &[],
        },
        &|| false,
        Duration::from_secs(60),
    )
    .unwrap()
}

#[test]
fn hooks_dir_defaults_to_the_common_dir_hooks() {
    let (_dir, repo) = common::init_repo();
    repo.config().unwrap().remove("core.hooksPath").unwrap();
    if repo.config().unwrap().get_path("core.hooksPath").is_ok() {
        eprintln!("global core.hooksPath is set; skipping the default-path check");
        return;
    }
    assert_eq!(hooks_dir(&repo).unwrap(), repo.commondir().join("hooks"));
}

#[test]
fn relative_core_hooks_path_is_resolved_from_the_worktree_root() {
    let (dir, repo) = common::init_repo();
    repo.config()
        .unwrap()
        .set_str("core.hooksPath", ".githooks")
        .unwrap();
    assert_eq!(
        std::fs::canonicalize(hooks_dir(&repo).unwrap().parent().unwrap()).unwrap(),
        std::fs::canonicalize(dir.path()).unwrap()
    );
    assert!(hooks_dir(&repo).unwrap().ends_with(".githooks"));
    common::install_hook(
        &dir.path().join(".githooks"),
        "pre-commit",
        "echo from githooks",
    );
    assert!(
        matches!(run(&repo, "pre-commit", &[]), HookOutcome::Ran(run) if run.output == "from githooks\n")
    );
}

#[test]
fn a_missing_hook_is_not_installed_and_a_non_executable_one_is_not_run() {
    let (_dir, repo) = common::init_repo();
    assert_eq!(run(&repo, "pre-commit", &[]), HookOutcome::NotInstalled);
    let hooks = hooks_dir(&repo).unwrap();
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join("pre-commit"), "#!/bin/sh\nexit 1\n").unwrap();
    assert!(matches!(
        find_hook(&repo, "pre-commit").unwrap(),
        HookLookup::NotExecutable(_)
    ));
    assert_eq!(run(&repo, "pre-commit", &[]), HookOutcome::NotExecutable);
    assert!(installed_hooks(&repo, &COMMIT_HOOKS).is_empty());
}

#[test]
fn a_hook_runs_in_the_worktree_root_with_git_dir_and_args() {
    let (dir, repo) = common::init_repo();
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "post-checkout",
        r#"pwd; echo "$GIT_DIR"; echo "$@""#,
    );
    let HookOutcome::Ran(result) = run(&repo, "post-checkout", &["a", "b", "1"]) else {
        panic!("hook did not run");
    };
    assert!(result.success);
    let lines: Vec<&str> = result.output.lines().collect();
    assert_eq!(
        std::fs::canonicalize(lines[0]).unwrap(),
        std::fs::canonicalize(dir.path()).unwrap()
    );
    assert_eq!(
        std::fs::canonicalize(lines[1]).unwrap(),
        std::fs::canonicalize(repo.path()).unwrap()
    );
    assert_eq!(lines[2], "a b 1");
}

#[test]
fn a_hook_without_a_shebang_runs_through_sh() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, repo) = common::init_repo();
    let hooks = hooks_dir(&repo).unwrap();
    std::fs::create_dir_all(&hooks).unwrap();
    let path = hooks.join("pre-commit");
    std::fs::write(&path, "echo no shebang\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(run(&repo, "pre-commit", &[]), HookOutcome::Ran(run) if run.success && run.output == "no shebang\n")
    );
}

#[test]
fn installed_hooks_lists_executable_commit_hooks() {
    let (_dir, repo) = common::init_repo();
    let hooks = hooks_dir(&repo).unwrap();
    common::install_hook(&hooks, "commit-msg", "exit 0");
    common::install_hook(&hooks, "pre-push", "exit 0");
    assert_eq!(
        installed_hooks(&repo, &COMMIT_HOOKS),
        vec!["commit-msg".to_string()]
    );
}

#[test]
fn a_timed_out_hook_is_a_failure_with_a_note() {
    let (_dir, repo) = common::init_repo();
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "pre-commit",
        "echo slow; sleep 30",
    );
    let outcome = run_hook(
        &repo,
        &HookInvocation {
            name: "pre-commit",
            args: &[],
            stdin: None,
            env: &[],
        },
        &|| false,
        Duration::from_millis(500),
    )
    .unwrap();
    assert_eq!(
        git_core::hooks::failure_output(&outcome, Duration::from_secs(600)).unwrap(),
        "slow\nBrowsitory stopped the hook after 10 minutes."
    );
}
