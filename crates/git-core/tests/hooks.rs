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

// ---- commit / amend ------------------------------------------------------------------------

use git_core::commit::{amend_head, commit, CommitError};
use git_core::hooks::{HookError, HookOptions, HookReporter};

fn stage(dir: &std::path::Path, repo: &git2::Repository, path: &str, contents: &str) {
    common::write_file(dir, path, contents);
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new(path)).unwrap();
    index.write().unwrap();
}

fn head_message(repo: &git2::Repository) -> String {
    repo.head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .message()
        .unwrap()
        .to_string()
}

#[test]
fn failing_pre_commit_blocks_the_commit_and_returns_its_output() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "pre-commit",
        "echo checking; echo 'lint failed' >&2; exit 1",
    );

    let error = commit(&mut repo, "add a", HookOptions::default()).unwrap_err();

    match error {
        CommitError::HookFailed { hook, output } => {
            assert_eq!(hook, "pre-commit");
            assert_eq!(output, "checking\nlint failed\n");
        }
        other => panic!("expected HookFailed, got {other:?}"),
    }
    assert!(repo.head().is_err(), "no commit may be created");
}

#[test]
fn pre_commit_that_stages_a_file_gets_it_committed() {
    if !common::program_available("git") {
        eprintln!("git CLI not found; skipping");
        return;
    }
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::write_file(dir.path(), "generated.txt", "made by the hook");
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "pre-commit",
        "git add generated.txt",
    );

    commit(&mut repo, "add a", HookOptions::default()).unwrap();

    let tree = repo.head().unwrap().peel_to_tree().unwrap();
    assert!(tree.get_name("generated.txt").is_some());
    assert!(tree.get_name("a.txt").is_some());
}

#[test]
fn commit_msg_can_rewrite_the_message() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "commit-msg",
        r#"printf 'rewritten\n\nIssue: 42\n' > "$1""#,
    );

    commit(&mut repo, "original", HookOptions::default()).unwrap();

    assert_eq!(head_message(&repo), "rewritten\n\nIssue: 42\n");
}

#[test]
fn prepare_commit_msg_gets_the_file_and_source() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "prepare-commit-msg",
        r#"case "$1" in */COMMIT_EDITMSG) ;; *) exit 1 ;; esac; echo "[$2]" >> "$1""#,
    );

    commit(&mut repo, "subject\n", HookOptions::default()).unwrap();

    assert_eq!(head_message(&repo), "subject\n[message]\n");
}

#[test]
fn no_verify_skips_pre_commit_and_commit_msg_but_not_prepare_commit_msg() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    let hooks = hooks_dir(&repo).unwrap();
    common::install_hook(&hooks, "pre-commit", "exit 1");
    common::install_hook(&hooks, "commit-msg", "exit 1");
    common::install_hook(&hooks, "prepare-commit-msg", r#"echo prepared >> "$1""#);

    commit(
        &mut repo,
        "skip\n",
        HookOptions {
            no_verify: true,
            ..HookOptions::default()
        },
    )
    .unwrap();

    assert_eq!(head_message(&repo), "skip\nprepared\n");
}

#[test]
fn a_hook_that_empties_the_message_aborts() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::install_hook(&hooks_dir(&repo).unwrap(), "commit-msg", r#": > "$1""#);

    let error = commit(&mut repo, "gone", HookOptions::default()).unwrap_err();

    assert!(matches!(error, CommitError::EmptyMessage));
    assert_eq!(
        error.to_string(),
        "Aborting commit due to empty commit message."
    );
    assert!(repo.head().is_err());
}

#[test]
fn post_commit_failure_does_not_fail_the_commit() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::install_hook(&hooks_dir(&repo).unwrap(), "post-commit", "exit 1");

    commit(&mut repo, "kept", HookOptions::default()).unwrap();

    assert_eq!(head_message(&repo), "kept");
}

#[test]
fn amend_runs_the_commit_hooks() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    commit(&mut repo, "first", HookOptions::default()).unwrap();
    let before = repo.head().unwrap().target().unwrap();
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "pre-commit",
        "echo no >&2; exit 1",
    );

    let error = amend_head(&repo, "amended", HookOptions::default()).unwrap_err();
    assert!(matches!(error, CommitError::HookFailed { ref hook, .. } if hook == "pre-commit"));
    assert_eq!(repo.head().unwrap().target().unwrap(), before);

    amend_head(
        &repo,
        "amended",
        HookOptions {
            no_verify: true,
            ..HookOptions::default()
        },
    )
    .unwrap();
    assert_eq!(head_message(&repo), "amended");
}

#[test]
fn cancel_kills_a_running_pre_commit() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::install_hook(&hooks_dir(&repo).unwrap(), "pre-commit", "sleep 30");
    let started = std::time::Instant::now();

    let error = commit(
        &mut repo,
        "x",
        HookOptions {
            cancel: Some(&|| true),
            ..HookOptions::default()
        },
    )
    .unwrap_err();

    assert!(matches!(
        error,
        CommitError::Hook(HookError::Cancelled { .. })
    ));
    assert_eq!(error.to_string(), "The pre-commit hook was cancelled.");
    assert!(started.elapsed() < Duration::from_secs(20));
    assert!(repo.head().is_err());
}

#[derive(Default)]
struct Recorder(Vec<String>);
impl HookReporter for Recorder {
    fn started(&mut self, hook: &str) {
        self.0.push(format!("start {hook}"));
    }
    fn finished(&mut self, hook: &str, _result: &Result<HookOutcome, HookError>) {
        self.0.push(format!("end {hook}"));
    }
}

#[test]
fn the_reporter_sees_only_hooks_that_exist_in_order() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    let hooks = hooks_dir(&repo).unwrap();
    common::install_hook(&hooks, "pre-commit", "exit 0");
    common::install_hook(&hooks, "post-commit", "exit 0");
    let mut recorder = Recorder::default();

    commit(
        &mut repo,
        "x",
        HookOptions {
            reporter: Some(&mut recorder),
            ..HookOptions::default()
        },
    )
    .unwrap();

    assert_eq!(
        recorder.0,
        vec![
            "start pre-commit",
            "end pre-commit",
            "start post-commit",
            "end post-commit"
        ]
    );
}

#[test]
fn head_info_lists_installed_commit_hooks_even_when_unborn() {
    let (_dir, repo) = common::init_repo();
    common::install_hook(&hooks_dir(&repo).unwrap(), "pre-commit", "exit 0");
    assert_eq!(
        git_core::repo::head_info(&repo).unwrap().commit_hooks,
        vec!["pre-commit".to_string()]
    );
}

#[test]
fn prepare_commit_msg_source_is_merge_while_finishing_a_merge() {
    let (dir, mut repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a");
    common::commit_all(&repo, "first");
    let first = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &first, false).unwrap();
    common::write_file(dir.path(), "b.txt", "b");
    common::commit_all(&repo, "second");
    let first_id = first.id();
    drop(first);
    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{first_id}\n")).unwrap();
    assert_eq!(repo.state(), git2::RepositoryState::Merge);
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "prepare-commit-msg",
        r#"echo "[$2]" >> "$1""#,
    );

    commit(&mut repo, "merge\n", HookOptions::default()).unwrap();

    assert_eq!(head_message(&repo), "merge\n[merge]\n");
}
