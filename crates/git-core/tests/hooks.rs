//! Real-repo hook tests with shell-script hooks (Unix only: they need `sh` and an execute bit).
#![cfg(unix)]
mod common;

use std::time::Duration;

use git_core::hooks::{
    find_hook, hooks_dir, installed_hooks, run_hook, HookInvocation, HookLookup, HookOutcome,
    SKIPPABLE_COMMIT_HOOKS,
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
    assert!(installed_hooks(&repo, &SKIPPABLE_COMMIT_HOOKS).is_empty());
}

#[test]
fn a_hook_runs_in_the_worktree_root_without_git_dir_and_with_args() {
    let (dir, repo) = common::init_repo();
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "post-checkout",
        r#"pwd; echo "[${GIT_DIR-unset}] [${GIT_WORK_TREE-unset}]"; echo "$@""#,
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
    // Like git: no GIT_DIR, so a hook that cd's into a subdirectory still finds the repo there.
    assert_eq!(lines[1], "[unset] [unset]");
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
        installed_hooks(&repo, &SKIPPABLE_COMMIT_HOOKS),
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
fn a_pre_commit_reading_the_terminal_fails_instead_of_stopping() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "pre-commit",
        "exec < /dev/tty; read answer; exit 0",
    );

    let started = std::time::Instant::now();
    let error = commit(&mut repo, "add a", HookOptions::default()).unwrap_err();

    assert!(started.elapsed() < Duration::from_secs(10));
    match error {
        CommitError::HookFailed { hook, output } => {
            assert_eq!(hook, "pre-commit");
            assert!(output.contains("/dev/tty"), "{output}");
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
fn pre_commit_that_stages_from_a_subdirectory_stages_the_subdirectory_file() {
    // husky monorepo shape: `cd frontend && npx lint-staged`.
    if !common::program_available("git") {
        eprintln!("git CLI not found; skipping");
        return;
    }
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::write_file(dir.path(), "sub/x.txt", "in sub");
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "pre-commit",
        "cd sub && git add x.txt",
    );

    commit(&mut repo, "add a", HookOptions::default()).unwrap();

    let tree = repo.head().unwrap().peel_to_tree().unwrap();
    assert!(tree.get_path(std::path::Path::new("sub/x.txt")).is_ok());
    assert!(tree.get_name("x.txt").is_none());
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

    let error = amend_head(&repo, "amended", None, HookOptions::default()).unwrap_err();
    assert!(matches!(error, CommitError::HookFailed { ref hook, .. } if hook == "pre-commit"));
    assert_eq!(repo.head().unwrap().target().unwrap(), before);

    amend_head(
        &repo,
        "amended",
        None,
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
    // A side commit, not HEAD's ancestor: MERGE_HEAD naming an ancestor reads as a merge that
    // was already committed (#183).
    let side = common::commit_tree(
        &repo,
        &[("a.txt", "a", 0o100644), ("c.txt", "c", 0o100644)],
        &[first_id],
        "side",
    );
    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{side}\n")).unwrap();
    assert_eq!(repo.state(), git2::RepositoryState::Merge);
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "prepare-commit-msg",
        r#"echo "[$2]" >> "$1""#,
    );

    commit(&mut repo, "merge\n", HookOptions::default()).unwrap();

    assert_eq!(head_message(&repo), "merge\n[merge]\n");
}

/// A conflicted `git merge feature`, started in a terminal during a `git bisect start`, then
/// resolved and staged. Returns the repository, not yet committed.
fn merge_during_a_bisect() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "base\n");
    common::commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &base, false).unwrap();
    let base_id = base.id();
    drop(base);
    let feature = common::commit_tree(&repo, &[("a.txt", "feature\n", 0o100644)], &[base_id], "f");
    repo.find_reference("refs/heads/feature")
        .unwrap()
        .set_target(feature, "test")
        .unwrap();
    common::write_file(dir.path(), "a.txt", "main\n");
    common::commit_all(&repo, "main");
    let run = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .output()
            .unwrap()
    };
    assert!(run(&["bisect", "start"]).status.success());
    assert!(repo.path().join("BISECT_LOG").exists());
    assert!(!run(&["merge", "feature"]).status.success());
    stage(dir.path(), &repo, "a.txt", "resolved\n");
    assert_eq!(repo.state(), git2::RepositoryState::Merge);
    (dir, repo)
}

/// `git merge` leaves `AUTO_MERGE` on a conflict; `git commit` removes it, and so does
/// Browsitory's commit (#241).
#[test]
fn committing_a_merge_started_in_a_terminal_removes_auto_merge() {
    let (_dir, mut repo) = merge_during_a_bisect();
    assert!(repo.path().join("AUTO_MERGE").exists());

    commit(&mut repo, "merge\n", HookOptions::default()).unwrap();

    assert!(!repo.path().join("AUTO_MERGE").exists());
    assert!(!repo.path().join("MERGE_HEAD").exists());

    // git's twin.
    let (twin_dir, twin) = merge_during_a_bisect();
    assert!(twin.path().join("AUTO_MERGE").exists());
    let output = std::process::Command::new("git")
        .args(["commit", "--no-edit"])
        .current_dir(twin_dir.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(!twin.path().join("AUTO_MERGE").exists());
}

/// `git commit` of a merge keeps a terminal bisect going (it only removes the merge's own files);
/// so does Browsitory's commit (#241).
#[test]
fn committing_a_merge_during_a_bisect_keeps_the_bisect() {
    let (_dir, mut repo) = merge_during_a_bisect();

    commit(&mut repo, "merge\n", HookOptions::default()).unwrap();

    assert_eq!(
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .parent_count(),
        2
    );
    assert!(repo.path().join("BISECT_LOG").exists());
    assert!(!repo.path().join("MERGE_HEAD").exists());
    assert!(!repo.path().join("MERGE_MSG").exists());

    // git's twin.
    let (twin_dir, twin) = merge_during_a_bisect();
    let output = std::process::Command::new("git")
        .args(["commit", "--no-edit"])
        .current_dir(twin_dir.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(twin.path().join("BISECT_LOG").exists());
}

// ---- pre-push ------------------------------------------------------------------------------

use git_core::remote::{
    delete_remote_branch, push_branch, push_current_branch, push_tags, CredentialProvider,
    RemoteError, TransferPhase, TransferProgress, TransferReporter,
};

#[derive(Default)]
struct Phases(Vec<TransferPhase>);
impl TransferReporter for Phases {
    fn report(&mut self, event: TransferProgress) {
        self.0.push(event.phase);
    }
}

struct NoCredentials;
impl CredentialProvider for NoCredentials {
    fn credential(
        &mut self,
        _: &str,
        _: Option<&str>,
        _: git2::CredentialType,
    ) -> Result<git2::Cred, git2::Error> {
        Err(git2::Error::from_str("no credentials for a local remote"))
    }
}

struct Pushable {
    local_dir: tempfile::TempDir,
    _remote_dir: tempfile::TempDir,
    remote_path: std::path::PathBuf,
    local: git2::Repository,
}

/// A local repo with one pushed commit on `main` and one more local commit to push.
fn pushable() -> Pushable {
    let (local_dir, local) = common::init_repo();
    local.set_head("refs/heads/main").unwrap();
    common::write_file(local_dir.path(), "a.txt", "a");
    common::commit_all(&local, "first");
    let remote_dir = tempfile::TempDir::new().unwrap();
    git2::Repository::init_bare(remote_dir.path()).unwrap();
    local
        .remote("origin", remote_dir.path().to_str().unwrap())
        .unwrap();
    local
        .find_remote("origin")
        .unwrap()
        .push(&["refs/heads/main:refs/heads/main"], None)
        .unwrap();
    common::write_file(local_dir.path(), "b.txt", "b");
    common::commit_all(&local, "second");
    let remote_path = remote_dir.path().to_path_buf();
    Pushable {
        local_dir,
        _remote_dir: remote_dir,
        remote_path,
        local,
    }
}

fn remote_ref(path: &std::path::Path, name: &str) -> Option<git2::Oid> {
    git2::Repository::open_bare(path)
        .unwrap()
        .refname_to_id(name)
        .ok()
}

fn remote_main(path: &std::path::Path) -> git2::Oid {
    remote_ref(path, "refs/heads/main").unwrap()
}

fn push_main(fixture: &Pushable, no_verify: bool, phases: &mut Phases) -> Result<(), RemoteError> {
    push_current_branch(
        &fixture.local,
        "origin",
        no_verify,
        &mut NoCredentials,
        phases,
        &|| false,
    )
}

#[test]
fn failing_pre_push_blocks_the_push() {
    let fixture = pushable();
    let before = remote_main(&fixture.remote_path);
    common::install_hook(
        &hooks_dir(&fixture.local).unwrap(),
        "pre-push",
        "echo 'tests failed' >&2; exit 1",
    );

    let error = push_main(&fixture, false, &mut Phases::default()).unwrap_err();

    match &error {
        RemoteError::HookFailed { hook, output } => {
            assert_eq!(hook, "pre-push");
            assert_eq!(output, "tests failed\n");
        }
        other => panic!("expected HookFailed, got {other:?}"),
    }
    assert_eq!(
        error.transfer_error_kind(),
        git_core::remote::TransferErrorKind::HookFailed
    );
    assert_eq!(remote_main(&fixture.remote_path), before);
}

#[test]
fn pre_push_receives_args_and_ref_lines() {
    let fixture = pushable();
    let old = remote_main(&fixture.remote_path);
    let new = fixture.local.head().unwrap().target().unwrap();
    let record = fixture.local_dir.path().join("pre-push.txt");
    common::install_hook(
        &hooks_dir(&fixture.local).unwrap(),
        "pre-push",
        &format!(r#"echo "$1 $2" > '{0}'; cat >> '{0}'"#, record.display()),
    );
    let mut phases = Phases::default();

    push_main(&fixture, false, &mut phases).unwrap();

    assert_eq!(
        std::fs::read_to_string(&record).unwrap(),
        format!(
            "origin {}\nrefs/heads/main {new} refs/heads/main {old}\n",
            fixture.remote_path.to_str().unwrap()
        )
    );
    assert!(phases.0.contains(&TransferPhase::RunningHook));
    assert_eq!(remote_main(&fixture.remote_path), new);
}

#[test]
fn up_to_date_refs_are_not_listed() {
    let fixture = pushable();
    push_main(&fixture, false, &mut Phases::default()).unwrap();
    let record = fixture.local_dir.path().join("pre-push.txt");
    common::install_hook(
        &hooks_dir(&fixture.local).unwrap(),
        "pre-push",
        &format!("cat > '{}'", record.display()),
    );

    push_main(&fixture, false, &mut Phases::default()).unwrap();

    // libgit2 may skip negotiation entirely when nothing changes; either way no line is listed.
    let recorded = std::fs::read_to_string(&record).unwrap_or_default();
    assert_eq!(recorded, "");
}

#[test]
fn no_verify_skips_pre_push() {
    let fixture = pushable();
    common::install_hook(&hooks_dir(&fixture.local).unwrap(), "pre-push", "exit 1");
    let mut phases = Phases::default();

    push_main(&fixture, true, &mut phases).unwrap();

    assert!(!phases.0.contains(&TransferPhase::RunningHook));
    assert_eq!(
        remote_main(&fixture.remote_path),
        fixture.local.head().unwrap().target().unwrap()
    );
}

#[test]
fn cancel_during_pre_push_is_a_cancellation() {
    let fixture = pushable();
    let before = remote_main(&fixture.remote_path);
    let flag = fixture.local_dir.path().join("hook-started");
    common::install_hook(
        &hooks_dir(&fixture.local).unwrap(),
        "pre-push",
        &format!("touch '{}'; sleep 30", flag.display()),
    );
    // Cancel only once the hook is running, so the pre-negotiation check passes.
    let cancel = || flag.exists();

    let error = push_current_branch(
        &fixture.local,
        "origin",
        false,
        &mut NoCredentials,
        &mut Phases::default(),
        &cancel,
    )
    .unwrap_err();

    assert!(matches!(error, RemoteError::Cancelled), "{error:?}");
    assert_eq!(remote_main(&fixture.remote_path), before);
}

#[test]
fn pushing_tags_runs_pre_push() {
    let fixture = pushable();
    git_core::remote::create_tag(&fixture.local, "v1", None).unwrap();
    common::install_hook(&hooks_dir(&fixture.local).unwrap(), "pre-push", "exit 1");

    let error = push_tags(
        &fixture.local,
        "origin",
        &["v1".to_string()],
        false,
        &mut NoCredentials,
        &mut Phases::default(),
        &|| false,
    )
    .unwrap_err();

    assert!(matches!(error, RemoteError::HookFailed { .. }));
    assert_eq!(remote_ref(&fixture.remote_path, "refs/tags/v1"), None);
}

#[test]
fn push_branch_and_delete_remote_branch_run_pre_push() {
    let fixture = pushable();
    let record = fixture.local_dir.path().join("pre-push.txt");
    common::install_hook(
        &hooks_dir(&fixture.local).unwrap(),
        "pre-push",
        &format!("cat >> '{}'", record.display()),
    );
    let head = fixture.local.head().unwrap().target().unwrap();
    let zero = git2::Oid::ZERO_SHA1;

    push_branch(
        &fixture.local,
        "origin",
        "main",
        "topic",
        false,
        false,
        &mut NoCredentials,
        &mut Phases::default(),
        &|| false,
    )
    .unwrap();
    delete_remote_branch(
        &fixture.local,
        "origin",
        "topic",
        false,
        &mut NoCredentials,
        &mut Phases::default(),
        &|| false,
    )
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(&record).unwrap(),
        format!(
            "refs/heads/main {head} refs/heads/topic {zero}\n(delete) {zero} refs/heads/topic {head}\n"
        )
    );
    assert_eq!(remote_ref(&fixture.remote_path, "refs/heads/topic"), None);
}

#[test]
fn a_failing_pre_push_blocks_a_remote_branch_delete() {
    let fixture = pushable();
    push_branch(
        &fixture.local,
        "origin",
        "main",
        "topic",
        false,
        false,
        &mut NoCredentials,
        &mut Phases::default(),
        &|| false,
    )
    .unwrap();
    common::install_hook(&hooks_dir(&fixture.local).unwrap(), "pre-push", "exit 1");

    let error = delete_remote_branch(
        &fixture.local,
        "origin",
        "topic",
        false,
        &mut NoCredentials,
        &mut Phases::default(),
        &|| false,
    )
    .unwrap_err();

    assert!(matches!(error, RemoteError::HookFailed { .. }));
    assert!(remote_ref(&fixture.remote_path, "refs/heads/topic").is_some());
}

fn install_raw_hook(repo: &git2::Repository, name: &str, contents: &str) {
    use std::os::unix::fs::PermissionsExt;
    let dir = hooks_dir(repo).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, contents).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn a_commit_hook_that_cannot_start_is_a_retryable_hook_failure() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    install_raw_hook(
        &repo,
        "pre-commit",
        "#!/browsitory/no/such/interpreter\nexit 0\n",
    );

    let error = commit(&mut repo, "x", HookOptions::default()).unwrap_err();
    match error {
        CommitError::HookFailed { hook, output } => {
            assert_eq!(hook, "pre-commit");
            assert!(
                output.starts_with("Could not run the pre-commit hook:"),
                "{output}"
            );
            assert!(output.contains("#!"), "{output}");
        }
        other => panic!("expected HookFailed, got {other:?}"),
    }

    commit(
        &mut repo,
        "x",
        HookOptions {
            no_verify: true,
            ..HookOptions::default()
        },
    )
    .unwrap();
    let error = amend_head(&repo, "y", None, HookOptions::default()).unwrap_err();
    assert!(matches!(error, CommitError::HookFailed { ref hook, .. } if hook == "pre-commit"));
}

#[test]
fn head_info_lists_only_commit_hooks_that_no_verify_skips() {
    let (_dir, repo) = common::init_repo();
    let hooks = hooks_dir(&repo).unwrap();
    common::install_hook(&hooks, "prepare-commit-msg", "exit 0");
    common::install_hook(&hooks, "post-commit", "exit 0");
    assert!(git_core::repo::head_info(&repo)
        .unwrap()
        .commit_hooks
        .is_empty());
    common::install_hook(&hooks, "commit-msg", "exit 0");
    assert_eq!(
        git_core::repo::head_info(&repo).unwrap().commit_hooks,
        vec!["commit-msg".to_string()]
    );
}

#[test]
fn prepare_commit_msg_source_is_merge_while_finishing_a_cherry_pick() {
    // `git commit` after a conflicted cherry-pick/revert takes its message from MERGE_MSG and
    // passes `merge`.
    let (dir, mut repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a");
    common::commit_all(&repo, "first");
    let first_id = repo.head().unwrap().target().unwrap();
    common::write_file(dir.path(), "b.txt", "b");
    common::commit_all(&repo, "second");
    std::fs::write(
        repo.path().join("CHERRY_PICK_HEAD"),
        format!("{first_id}\n"),
    )
    .unwrap();
    std::fs::write(repo.path().join("MERGE_MSG"), "first\n").unwrap();
    assert_eq!(repo.state(), git2::RepositoryState::CherryPick);
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "prepare-commit-msg",
        r#"echo "[$2]" >> "$1""#,
    );

    commit(&mut repo, "first\n", HookOptions::default()).unwrap();

    assert_eq!(head_message(&repo), "first\n[merge]\n");
}

// ---- pre-merge-commit (#124) -----------------------------------------------------------------

use git_core::commit::{commit_with, CommitHookSet};
use git_core::merge::{start_merge, FastForward, MergeOptions, MergeOutcome};

const ALL_COMMIT_HOOKS: [&str; 5] = [
    "pre-commit",
    "pre-merge-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
];

/// `main` and `feature` changed `base.txt` (or, with `conflict`, different files); HEAD is on
/// `main`, and `feature` has been merged into it with `--no-ff`, not yet committed.
fn merge_fixture(conflict: bool) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "base.txt", "base\n");
    common::commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap().id();
    let (main_file, feature_file) = if conflict {
        ("base.txt", "base.txt")
    } else {
        ("main.txt", "feature.txt")
    };
    common::write_file(dir.path(), main_file, "main\n");
    common::commit_all(&repo, "main side");
    let mut files = vec![("base.txt", "base\n", 0o100644)];
    if conflict {
        files[0].1 = "feature\n";
    } else {
        files.push((feature_file, "feature\n", 0o100644));
    }
    let feature = common::commit_tree(&repo, &files, &[base], "feature side");
    repo.reference("refs/heads/feature", feature, true, "test")
        .unwrap();
    (dir, repo)
}

fn merge_feature(repo: &git2::Repository) -> MergeOutcome {
    start_merge(
        repo,
        "feature",
        MergeOptions {
            fast_forward: FastForward::Never,
            ..Default::default()
        },
    )
    .unwrap()
}

fn clean_merge_fixture() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = merge_fixture(false);
    assert_eq!(merge_feature(&repo), MergeOutcome::Merged);
    (dir, repo)
}

fn head_id(repo: &git2::Repository) -> git2::Oid {
    repo.head().unwrap().peel_to_commit().unwrap().id()
}

fn merge_heads(repo: &mut git2::Repository) -> Vec<git2::Oid> {
    let mut heads = Vec::new();
    repo.mergehead_foreach(|oid| {
        heads.push(*oid);
        true
    })
    .unwrap();
    heads
}

/// The clean-merge hook set for the merge `repo` shows now.
fn clean_set(repo: &mut git2::Repository) -> CommitHookSet {
    CommitHookSet::CleanMerge {
        head: head_id(repo),
        merge_heads: merge_heads(repo),
    }
}

#[test]
fn a_clean_merge_commit_runs_git_merges_hooks() {
    let (_dir, mut repo) = clean_merge_fixture();
    common::log_hooks(&repo, &ALL_COMMIT_HOOKS);

    let set = clean_set(&mut repo);

    let committed = commit_with(&mut repo, "Merge feature", set, HookOptions::default()).unwrap();

    assert!(
        committed.merge_hooks,
        "the caller runs post-merge only then"
    );
    assert_eq!(
        common::read_log(&repo),
        "pre-merge-commit \nprepare-commit-msg <MERGE_MSG> merge\ncommit-msg <MERGE_MSG>\n"
    );
    assert_eq!(
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .parent_count(),
        2
    );
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn a_failing_pre_merge_commit_keeps_the_merge_staged() {
    let (_dir, mut repo) = clean_merge_fixture();
    let before = head_id(&repo);
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "pre-merge-commit",
        "echo blocked; exit 1",
    );

    let set = clean_set(&mut repo);

    let error = commit_with(&mut repo, "Merge feature", set, HookOptions::default()).unwrap_err();

    match error {
        CommitError::HookFailed { hook, output } => {
            assert_eq!(hook, "pre-merge-commit");
            assert_eq!(output, "blocked\n");
        }
        other => panic!("expected HookFailed, got {other:?}"),
    }
    assert_eq!(repo.state(), git2::RepositoryState::Merge);
    assert_eq!(head_id(&repo), before);
    assert!(repo.path().join("MERGE_HEAD").exists());
}

#[test]
fn no_verify_skips_pre_merge_commit_and_commit_msg() {
    let (_dir, mut repo) = clean_merge_fixture();
    let hooks = hooks_dir(&repo).unwrap();
    common::install_hook(&hooks, "pre-merge-commit", "exit 1");
    common::install_hook(&hooks, "commit-msg", "exit 1");
    common::log_hooks(&repo, &["prepare-commit-msg"]);

    let set = clean_set(&mut repo);

    commit_with(
        &mut repo,
        "Merge feature",
        set,
        HookOptions {
            no_verify: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(
        common::read_log(&repo),
        "prepare-commit-msg <MERGE_MSG> merge\n"
    );
    assert_eq!(
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .parent_count(),
        2
    );
}

#[test]
fn a_pre_merge_commit_that_stages_a_file_gets_it_committed() {
    if !common::program_available("git") {
        eprintln!("git CLI not found; skipping");
        return;
    }
    let (dir, mut repo) = clean_merge_fixture();
    common::write_file(dir.path(), "generated.txt", "made by the hook");
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "pre-merge-commit",
        "git add generated.txt",
    );

    let set = clean_set(&mut repo);

    commit_with(&mut repo, "Merge feature", set, HookOptions::default()).unwrap();

    let tree = repo.head().unwrap().peel_to_tree().unwrap();
    assert!(tree.get_name("generated.txt").is_some());
    assert!(tree.get_name("feature.txt").is_some());
}

#[test]
fn a_merge_commit_after_conflicts_runs_the_commit_hooks() {
    let (dir, mut repo) = merge_fixture(true);
    assert!(matches!(
        merge_feature(&repo),
        MergeOutcome::Conflicted { .. }
    ));
    stage(dir.path(), &repo, "base.txt", "resolved\n");
    common::log_hooks(&repo, &ALL_COMMIT_HOOKS);

    commit(&mut repo, "Merge feature", HookOptions::default()).unwrap();

    assert_eq!(
        common::read_log(&repo),
        "pre-commit \nprepare-commit-msg <msg> merge\ncommit-msg <msg>\npost-commit \n"
    );
    assert_eq!(
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .parent_count(),
        2
    );
}

#[test]
fn the_clean_merge_hook_set_outside_a_merge_runs_the_commit_hooks() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::log_hooks(&repo, &ALL_COMMIT_HOOKS);

    let committed = commit_with(
        &mut repo,
        "add a",
        CommitHookSet::CleanMerge {
            head: git2::Oid::ZERO_SHA1,
            merge_heads: Vec::new(),
        },
        HookOptions::default(),
    )
    .unwrap();

    assert!(!committed.merge_hooks, "no merge, so no post-merge either");

    assert_eq!(
        common::read_log(&repo),
        "pre-commit \nprepare-commit-msg <msg> message\ncommit-msg <msg>\npost-commit \n"
    );
}

#[test]
fn head_info_lists_pre_merge_commit_only_while_merging() {
    let (_dir, repo) = merge_fixture(false);
    common::install_hook(&hooks_dir(&repo).unwrap(), "pre-merge-commit", "exit 0");
    assert!(git_core::repo::head_info(&repo)
        .unwrap()
        .commit_hooks
        .is_empty());

    assert_eq!(merge_feature(&repo), MergeOutcome::Merged);

    assert_eq!(
        git_core::repo::head_info(&repo).unwrap().commit_hooks,
        vec!["pre-merge-commit".to_string()]
    );
}

// ---- clean merge residuals (#183) --------------------------------------------------------------

/// `clean_merge_fixture` plus a branch `side2` (adds `side2.txt` on the base commit).
fn clean_merge_with_side2() -> (tempfile::TempDir, git2::Repository, git2::Oid) {
    let (dir, repo) = clean_merge_fixture();
    let base = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .parent_id(0)
        .unwrap();
    let side2 = common::commit_tree(
        &repo,
        &[
            ("base.txt", "base\n", 0o100644),
            ("side2.txt", "side2\n", 0o100644),
        ],
        &[base],
        "side2",
    );
    repo.reference("refs/heads/side2", side2, true, "test")
        .unwrap();
    (dir, repo, side2)
}

#[test]
fn a_merge_restarted_during_pre_merge_commit_is_not_committed() {
    if !common::program_available("git") {
        eprintln!("git CLI not found; skipping");
        return;
    }
    let (_dir, mut repo, side2) = clean_merge_with_side2();
    let before = head_id(&repo);
    let set = clean_set(&mut repo);
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "pre-merge-commit",
        "git merge --abort && git merge --no-ff --no-commit side2 >/dev/null",
    );

    let error = commit_with(&mut repo, "Merge feature", set, HookOptions::default()).unwrap_err();

    assert!(
        matches!(error, CommitError::MergeChanged),
        "expected MergeChanged, got {error:?}"
    );
    assert_eq!(
        error.to_string(),
        "The merge changed while its hooks ran. Nothing was committed; check the merge and commit again."
    );
    assert_eq!(head_id(&repo), before);
    assert_eq!(repo.state(), git2::RepositoryState::Merge);
    assert_eq!(merge_heads(&mut repo), vec![side2]);
}

#[test]
fn commit_with_runs_pre_commit_when_the_expected_merge_is_not_the_current_one() {
    let (_dir, mut repo, side2) = clean_merge_with_side2();
    common::log_hooks(&repo, &ALL_COMMIT_HOOKS);
    let set = CommitHookSet::CleanMerge {
        head: head_id(&repo),
        merge_heads: vec![side2],
    };

    let committed = commit_with(&mut repo, "Merge feature", set, HookOptions::default()).unwrap();

    assert!(!committed.merge_hooks);
    assert_eq!(
        common::read_log(&repo),
        "pre-commit \nprepare-commit-msg <msg> merge\ncommit-msg <msg>\npost-commit \n"
    );
    assert_eq!(
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .parent_count(),
        2
    );
}

#[test]
fn a_clean_merges_message_hooks_get_merge_msg_like_git_merge() {
    let (_dir, mut repo) = clean_merge_fixture();
    let log = repo.path().join("hook-log");
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "prepare-commit-msg",
        &format!(
            "echo \"$(basename \"$1\") $2\" >> '{}'; echo added >> \"$1\"",
            log.display()
        ),
    );

    let set = clean_set(&mut repo);
    commit_with(&mut repo, "Merge feature\n", set, HookOptions::default()).unwrap();

    assert_eq!(std::fs::read_to_string(&log).unwrap(), "MERGE_MSG merge\n");
    assert_eq!(head_message(&repo), "Merge feature\nadded\n");
    assert!(!repo.path().join("MERGE_MSG").exists());
}

#[test]
fn git_merge_also_gives_prepare_commit_msg_merge_msg() {
    if !common::program_available("git") {
        eprintln!("git CLI not found; skipping");
        return;
    }
    let (dir, repo) = merge_fixture(false);
    let log = repo.path().join("hook-log");
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "prepare-commit-msg",
        &format!("echo \"$(basename \"$1\") $2\" >> '{}'", log.display()),
    );

    let status = std::process::Command::new("git")
        .args(["merge", "--no-ff", "-m", "Merge feature", "feature"])
        .current_dir(dir.path())
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();

    assert!(status.success());
    assert_eq!(std::fs::read_to_string(&log).unwrap(), "MERGE_MSG merge\n");
}

#[test]
fn a_cleanup_failure_after_a_merge_commit_still_reports_it() {
    let (_dir, mut repo) = clean_merge_fixture();
    let set = clean_set(&mut repo);
    let Some(read_only) = common::ReadOnlyGitDir::new(&repo) else {
        return;
    };

    let committed = commit_with(&mut repo, "Merge feature", set, HookOptions::default());
    drop(read_only);

    let committed = committed.expect("the commit landed, so it counts");
    assert!(committed.merge_hooks, "the caller still owes post-merge");
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.id().to_string(), committed.commit_id);
    assert_eq!(head.parent_count(), 2);
}

#[test]
fn a_merge_restarted_during_pre_merge_commit_keeps_the_new_merge_msg() {
    if !common::program_available("git") {
        eprintln!("git CLI not found; skipping");
        return;
    }
    let (_dir, mut repo, _side2) = clean_merge_with_side2();
    let set = clean_set(&mut repo);
    let hooks = hooks_dir(&repo).unwrap();
    common::install_hook(
        &hooks,
        "pre-merge-commit",
        "git merge --abort && git merge --no-ff --no-commit side2 >/dev/null",
    );
    let log = repo.path().join("hook-log");
    common::install_hook(
        &hooks,
        "prepare-commit-msg",
        &format!("echo ran >> '{}'", log.display()),
    );

    let error = commit_with(&mut repo, "Merge feature", set, HookOptions::default()).unwrap_err();

    assert!(matches!(error, CommitError::MergeChanged), "{error:?}");
    let merge_msg = std::fs::read_to_string(repo.path().join("MERGE_MSG")).unwrap();
    assert!(
        merge_msg.contains("side2"),
        "the restarted merge's MERGE_MSG was replaced: {merge_msg:?}"
    );
    assert!(
        !log.exists(),
        "no message hook runs for a merge that changed"
    );
}

/// D7-A left MERGE_HEAD behind after the merge commit landed; the next commit must not create a
/// second merge commit: the leftover state is cleared and it commits normally (#183).
#[test]
fn a_commit_after_a_leftover_merge_state_is_not_a_second_merge_commit() {
    let (dir, mut repo) = clean_merge_fixture();
    let set = clean_set(&mut repo);
    let Some(read_only) = common::ReadOnlyGitDir::new(&repo) else {
        return;
    };
    let merge_commit = commit_with(&mut repo, "Merge feature", set, HookOptions::default());
    drop(read_only);
    merge_commit.unwrap();
    assert_eq!(repo.state(), git2::RepositoryState::Merge, "leftover state");

    stage(dir.path(), &repo, "after.txt", "after\n");
    commit(&mut repo, "after the merge", HookOptions::default()).unwrap();

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_count(), 1, "not a second merge commit");
    assert_eq!(head.parent(0).unwrap().parent_count(), 2);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn a_leftover_merge_state_that_cannot_be_cleared_refuses_the_commit() {
    let (dir, mut repo) = clean_merge_fixture();
    let set = clean_set(&mut repo);
    let Some(read_only) = common::ReadOnlyGitDir::new(&repo) else {
        return;
    };
    commit_with(&mut repo, "Merge feature", set, HookOptions::default()).unwrap();
    let merge_commit = head_id(&repo);
    drop(read_only);
    stage(dir.path(), &repo, "after.txt", "after\n");
    let Some(read_only) = common::ReadOnlyGitDir::new(&repo) else {
        return;
    };

    let error = commit(&mut repo, "after the merge", HookOptions::default()).unwrap_err();
    drop(read_only);

    assert!(
        matches!(error, CommitError::LeftoverMergeState),
        "{error:?}"
    );
    assert_eq!(head_id(&repo), merge_commit);
}
