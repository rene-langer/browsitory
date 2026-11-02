//! A `git bisect` started in a terminal (#249, #250). Each test runs Browsitory's operation on a
//! bisecting repository and, where it matters, a git CLI twin on a second copy, so git's own
//! result is recorded next to Browsitory's.
mod common;

use std::path::Path;

use git2::{Oid, Repository, RepositoryState};
use git_core::hooks::HookOptions;
use git_core::pick::PickError;
use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseStepResult};
use tempfile::TempDir;

/// The commits a bisect fixture is built from.
#[allow(dead_code)]
struct Ids {
    /// f="base\n"; the bisect's good commit.
    base: Oid,
    /// Branch `feature`: f="feature\n" on base, so merging or picking it conflicts with main.
    feature: Oid,
    /// Branch `clean`: adds clean.txt on base, so it applies anywhere without a conflict.
    clean: Oid,
}

/// Runs the git CLI in `dir`.
fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn git_ok(dir: &Path, args: &[&str]) {
    let output = git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

/// main: base (f="base\n"), "main" (f="main\n"), then n1..n5 (each adds `n<i>.txt`). Branches
/// `feature` and `clean` start at base. Then `git bisect start`, and when `detached`,
/// `git bisect bad` + `git bisect good <base>`, which detaches HEAD in the middle of main.
fn bisect_repo(detached: bool) -> (TempDir, Repository, Ids) {
    let (dir, repo) = common::init_repo();
    repo.set_head("refs/heads/main").unwrap();
    common::write_file(dir.path(), "f", "base\n");
    common::commit_all(&repo, "base");
    let base = repo.head().unwrap().target().unwrap();
    let feature = common::commit_tree(&repo, &[("f", "feature\n", 0o100644)], &[base], "feature");
    repo.reference("refs/heads/feature", feature, false, "test")
        .unwrap();
    let clean = common::commit_tree(
        &repo,
        &[
            ("f", "base\n", 0o100644),
            ("clean.txt", "clean\n", 0o100644),
        ],
        &[base],
        "clean",
    );
    repo.reference("refs/heads/clean", clean, false, "test")
        .unwrap();
    common::write_file(dir.path(), "f", "main\n");
    common::commit_all(&repo, "main");
    for i in 1..=5 {
        common::write_file(dir.path(), &format!("n{i}.txt"), &format!("{i}\n"));
        common::commit_all(&repo, &format!("n{i}"));
    }
    git_ok(dir.path(), &["bisect", "start"]);
    if detached {
        git_ok(dir.path(), &["bisect", "bad"]);
        git_ok(dir.path(), &["bisect", "good", &base.to_string()]);
        assert!(repo.head_detached().unwrap());
    }
    // A fresh handle: the git CLI rewrote the index this one has loaded.
    let repo = reopen(&repo);
    assert!(repo.path().join("BISECT_LOG").exists());
    assert_eq!(repo.state(), RepositoryState::Bisect);
    (
        dir,
        repo,
        Ids {
            base,
            feature,
            clean,
        },
    )
}

/// A new handle on `repo`, as the app opens one per command: the git CLI changed the index
/// behind the old handle's back.
fn reopen(repo: &Repository) -> Repository {
    Repository::open(repo.workdir().unwrap()).unwrap()
}

fn bisect_log(repo: &Repository) -> Option<Vec<u8>> {
    std::fs::read(repo.path().join("BISECT_LOG")).ok()
}

fn head(repo: &Repository) -> String {
    repo.head().unwrap().target().unwrap().to_string()
}

fn stage(repo: &Repository, path: &str) {
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();
}

fn pick(commit_id: &str) -> RebasePlanEntry {
    RebasePlanEntry {
        commit_id: commit_id.to_string(),
        action: RebaseAction::Pick,
        combined_message: None,
    }
}

/// A plan picking every commit of main since `onto`.
fn plan_onto(repo: &Repository, onto: &str) -> Vec<RebasePlanEntry> {
    git_core::rebase::commits_since(repo, onto)
        .unwrap()
        .iter()
        .map(|commit| pick(&commit.id))
        .collect()
}

#[test]
fn cherry_pick_and_revert_during_a_bisect_are_refused_today() {
    let (_dir, repo, ids) = bisect_repo(true);
    let before = bisect_log(&repo);

    let picked =
        git_core::pick::cherry_pick(&repo, &ids.clean.to_string(), None, HookOptions::default());
    assert!(matches!(picked, Err(PickError::OperationInProgress)));
    let reverted = git_core::pick::revert(&repo, &head(&repo), None, HookOptions::default());
    assert!(matches!(reverted, Err(PickError::OperationInProgress)));
    let many = git_core::pick::cherry_pick_many(
        &repo,
        &[ids.clean.to_string(), ids.feature.to_string()],
        HookOptions::default(),
    );
    assert!(matches!(many.outcome, Err(PickError::OperationInProgress)));
    assert_eq!(bisect_log(&repo), before);

    // git's twin runs both and keeps the bisect.
    let (twin_dir, twin, twin_ids) = bisect_repo(true);
    let twin_before = bisect_log(&twin);
    git_ok(
        twin_dir.path(),
        &["cherry-pick", &twin_ids.clean.to_string()],
    );
    git_ok(twin_dir.path(), &["revert", "--no-edit", "HEAD"]);
    assert_eq!(bisect_log(&twin), twin_before);
}

#[test]
fn reset_and_amend_during_a_bisect_are_refused_today() {
    let (_dir, repo, ids) = bisect_repo(true);
    let reset = git_core::reset::reset_to(
        &repo,
        &ids.base.to_string(),
        git_core::reset::ResetMode::Soft,
    );
    assert!(matches!(
        reset,
        Err(git_core::reset::ResetError::OperationInProgress)
    ));
    let amended = git_core::commit::amend_head(&repo, "amended\n", None, HookOptions::default());
    assert!(matches!(
        amended,
        Err(git_core::commit::CommitError::OperationInProgress)
    ));
}

#[test]
fn undo_restore_during_a_bisect_is_refused_today() {
    use git_core::undo::{capture, restore, SnapshotScope, UndoError};
    let (dir, repo, _ids) = bisect_repo(true);
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    common::write_file(dir.path(), "x.txt", "x\n");
    common::commit_all(&repo, "x");
    let after = capture(&repo, SnapshotScope::Full).unwrap();

    let result = restore(&repo, &after, &before, "undo: x");

    assert!(matches!(result, Err(UndoError::OperationInProgress)));
}

#[test]
fn aborting_a_merge_keeps_a_terminal_bisect() {
    let (dir, repo, _ids) = bisect_repo(true);
    let before = bisect_log(&repo);
    let head_before = head(&repo);
    assert!(!git(dir.path(), &["merge", "feature"]).status.success());
    let repo = reopen(&repo);
    assert_eq!(repo.state(), RepositoryState::Merge);

    git_core::merge::abort_merge(&repo).unwrap();

    assert_eq!(bisect_log(&repo), before);
    assert_eq!(repo.state(), RepositoryState::Bisect);
    assert_eq!(head(&repo), head_before);
    assert!(!repo.path().join("MERGE_HEAD").exists());
    assert!(!repo.path().join("MERGE_MSG").exists());
    assert!(!repo.path().join("AUTO_MERGE").exists());
    assert_eq!(common::read(dir.path(), "f"), "main\n");

    // git's twin keeps the bisect too.
    let (twin_dir, twin, _) = bisect_repo(true);
    let twin_before = bisect_log(&twin);
    assert!(!git(twin_dir.path(), &["merge", "feature"]).status.success());
    git_ok(twin_dir.path(), &["merge", "--abort"]);
    assert_eq!(bisect_log(&twin), twin_before);
}

#[test]
fn a_rebase_step_committed_by_hand_during_a_bisect_shows_as_interrupted_today() {
    use git_core::rebase::{RebaseDivergence, RebaseHealth};
    let (dir, repo, _ids) = bisect_repo(false);
    let plan = plan_onto(&repo, "feature");
    let (state, result) = git_core::rebase::start_rebase(&repo, "feature", plan).unwrap();
    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));
    common::write_file(dir.path(), "f", "resolved\n");
    stage(&repo, "f");
    git_ok(dir.path(), &["commit", "--no-edit"]);

    assert_eq!(
        git_core::rebase::rebase_health(&repo, &state).unwrap(),
        RebaseHealth::Diverged(RebaseDivergence::OtherOperation)
    );
}

#[test]
fn finishing_a_rebase_keeps_a_terminal_bisect() {
    let (_dir, repo, _ids) = bisect_repo(false);
    let before = bisect_log(&repo);
    let plan = plan_onto(&repo, "clean");
    let (_state, result) = git_core::rebase::start_rebase(&repo, "clean", plan).unwrap();
    assert_eq!(result, RebaseStepResult::Done);

    assert_eq!(bisect_log(&repo), before);
    assert_eq!(repo.state(), RepositoryState::Bisect);
    assert!(!repo.head_detached().unwrap());

    // git's twin keeps the bisect too.
    let (twin_dir, twin, _) = bisect_repo(false);
    let twin_before = bisect_log(&twin);
    git_ok(twin_dir.path(), &["rebase", "clean"]);
    assert_eq!(bisect_log(&twin), twin_before);
}

#[test]
fn continuing_a_rebase_keeps_a_terminal_bisect() {
    let (dir, repo, _ids) = bisect_repo(false);
    let before = bisect_log(&repo);
    let plan = plan_onto(&repo, "feature");
    let (mut state, result) = git_core::rebase::start_rebase(&repo, "feature", plan).unwrap();
    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));
    common::write_file(dir.path(), "f", "resolved\n");
    stage(&repo, "f");

    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(bisect_log(&repo), before);
    assert_eq!(repo.state(), RepositoryState::Bisect);
    assert_eq!(common::read(dir.path(), "f"), "resolved\n");

    // git's twin keeps the bisect too.
    let (twin_dir, twin, _) = bisect_repo(false);
    let twin_before = bisect_log(&twin);
    assert!(!git(twin_dir.path(), &["rebase", "feature"])
        .status
        .success());
    common::write_file(twin_dir.path(), "f", "resolved\n");
    git_ok(twin_dir.path(), &["add", "f"]);
    let output = std::process::Command::new("git")
        .args(["rebase", "--continue"])
        .env("GIT_EDITOR", "true")
        .current_dir(twin_dir.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(bisect_log(&twin), twin_before);
}

#[test]
fn aborting_a_rebase_keeps_a_terminal_bisect() {
    let (dir, repo, _ids) = bisect_repo(false);
    let before = bisect_log(&repo);
    let tip = head(&repo);
    let plan = plan_onto(&repo, "feature");
    let (state, result) = git_core::rebase::start_rebase(&repo, "feature", plan).unwrap();
    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));

    git_core::rebase::abort_rebase(&repo, &state).unwrap();

    assert_eq!(bisect_log(&repo), before);
    assert_eq!(head(&repo), tip);
    assert_eq!(common::read(dir.path(), "f"), "main\n");

    // git's twin keeps the bisect too.
    let (twin_dir, twin, _) = bisect_repo(false);
    let twin_before = bisect_log(&twin);
    assert!(!git(twin_dir.path(), &["rebase", "feature"])
        .status
        .success());
    git_ok(twin_dir.path(), &["rebase", "--abort"]);
    assert_eq!(bisect_log(&twin), twin_before);
}

/// libgit2's `cleanup_state` also deletes `BISECT_LOG`, `rebase-*` and `sequencer/`, which
/// belong to operations run in a terminal (#249). git-core clears an operation's own files with
/// `commit::clear_operation_files` instead; this keeps it that way.
#[test]
fn no_cleanup_state_in_git_core_src() {
    fn visit(dir: &Path, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let text = std::fs::read_to_string(&path).unwrap();
                for (number, line) in text.lines().enumerate() {
                    let code = line.split("//").next().unwrap_or("");
                    if code.contains("cleanup_state(") {
                        found.push(format!("{}:{}", path.display(), number + 1));
                    }
                }
            }
        }
    }
    let mut found = Vec::new();
    visit(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut found,
    );
    assert!(found.is_empty(), "cleanup_state called at {found:?}");
}

#[test]
fn head_info_during_a_bisect_shows_nothing_today() {
    let (_dir, repo, _ids) = bisect_repo(true);
    let info = git_core::repo::head_info(&repo).unwrap();
    assert_eq!(info.operation, None);
}
