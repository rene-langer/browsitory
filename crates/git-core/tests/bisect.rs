//! A `git bisect` started in a terminal (#249, #250). Each test runs Browsitory's operation on a
//! bisecting repository and, where it matters, a git CLI twin on a second copy, so git's own
//! result is recorded next to Browsitory's.
mod common;

use std::path::Path;

use git2::{Oid, Repository, RepositoryState};
use git_core::hooks::HookOptions;
use git_core::pick::{PickError, PickOutcome, SequenceOutcome};
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
fn cherry_pick_and_revert_run_during_a_bisect() {
    let (dir, repo, ids) = bisect_repo(true);
    let before = bisect_log(&repo);

    let picked =
        git_core::pick::cherry_pick(&repo, &ids.clean.to_string(), None, HookOptions::default())
            .unwrap();
    assert!(
        matches!(picked, PickOutcome::Committed { .. }),
        "{picked:?}"
    );
    assert_eq!(common::read(dir.path(), "clean.txt"), "clean\n");
    let reverted =
        git_core::pick::revert(&repo, &head(&repo), None, HookOptions::default()).unwrap();
    assert!(
        matches!(reverted, PickOutcome::Committed { .. }),
        "{reverted:?}"
    );
    assert!(!dir.path().join("clean.txt").exists());

    assert_eq!(bisect_log(&repo), before);
    assert_eq!(repo.state(), RepositoryState::Bisect);

    // git's twin runs both and keeps the bisect too.
    let (twin_dir, twin, twin_ids) = bisect_repo(true);
    let twin_before = bisect_log(&twin);
    git_ok(
        twin_dir.path(),
        &["cherry-pick", &twin_ids.clean.to_string()],
    );
    git_ok(twin_dir.path(), &["revert", "--no-edit", "HEAD"]);
    assert_eq!(bisect_log(&twin), twin_before);
}

/// `cherry_pick_many([clean, feature])`: clean applies, feature conflicts.
fn sequence_paused_during_a_bisect(repo: &Repository, ids: &Ids) -> git_core::pick::SequenceState {
    let run = git_core::pick::cherry_pick_many(
        repo,
        &[ids.clean.to_string(), ids.feature.to_string()],
        HookOptions::default(),
    );
    assert!(
        matches!(run.outcome, Ok(SequenceOutcome::Conflicted { .. })),
        "{:?}",
        run.outcome
    );
    run.state.expect("a stopped sequence")
}

#[test]
fn a_sequence_runs_and_aborts_during_a_bisect() {
    let (dir, repo, ids) = bisect_repo(true);
    let before = bisect_log(&repo);
    let original = head(&repo);
    let state = sequence_paused_during_a_bisect(&repo, &ids);

    let abort = git_core::pick::abort_sequence(&repo, &state).unwrap();

    assert!(abort.rewound);
    assert_eq!(head(&repo), original);
    assert!(repo.head_detached().unwrap());
    assert!(!dir.path().join("clean.txt").exists());
    assert_eq!(common::read(dir.path(), "f"), "main\n");
    assert_eq!(bisect_log(&repo), before);
    assert_eq!(repo.state(), RepositoryState::Bisect);

    // git's twin keeps the bisect too.
    let (twin_dir, twin, twin_ids) = bisect_repo(true);
    let twin_before = bisect_log(&twin);
    let twin_original = head(&twin);
    let output = git(
        twin_dir.path(),
        &[
            "cherry-pick",
            &twin_ids.clean.to_string(),
            &twin_ids.feature.to_string(),
        ],
    );
    assert!(!output.status.success());
    git_ok(twin_dir.path(), &["cherry-pick", "--abort"]);
    assert_eq!(head(&twin), twin_original);
    assert_eq!(bisect_log(&twin), twin_before);
}

/// The paused step committed with `git commit`, then Continue: the bisect no longer reads as
/// "another operation" (#250).
#[test]
fn a_sequence_continues_after_a_step_committed_by_hand_during_a_bisect() {
    let (dir, repo, ids) = bisect_repo(true);
    let before = bisect_log(&repo);
    let state = sequence_paused_during_a_bisect(&repo, &ids);
    common::write_file(dir.path(), "f", "resolved\n");
    stage(&repo, "f");
    git_ok(dir.path(), &["commit", "--no-edit"]);
    let repo = reopen(&repo);
    assert_eq!(repo.state(), RepositoryState::Bisect);

    let run = git_core::pick::continue_sequence(&repo, state, HookOptions::default());

    assert!(
        matches!(run.outcome, Ok(SequenceOutcome::Completed { .. })),
        "{:?}",
        run.outcome
    );
    assert!(run.state.is_none());
    assert_eq!(bisect_log(&repo), before);
}

#[test]
fn a_conflicting_pick_during_a_bisect_can_be_aborted() {
    let (dir, repo, ids) = bisect_repo(true);
    let before = bisect_log(&repo);
    let original = head(&repo);
    let picked = git_core::pick::cherry_pick(
        &repo,
        &ids.feature.to_string(),
        None,
        HookOptions::default(),
    )
    .unwrap();
    assert!(
        matches!(picked, PickOutcome::Conflicted { .. }),
        "{picked:?}"
    );

    git_core::merge::abort_merge(&repo).unwrap();

    assert_eq!(head(&repo), original);
    assert_eq!(common::read(dir.path(), "f"), "main\n");
    assert_eq!(repo.state(), RepositoryState::Bisect);
    assert_eq!(bisect_log(&repo), before);

    // git's twin keeps the bisect too.
    let (twin_dir, twin, twin_ids) = bisect_repo(true);
    let twin_before = bisect_log(&twin);
    let output = git(
        twin_dir.path(),
        &["cherry-pick", &twin_ids.feature.to_string()],
    );
    assert!(!output.status.success());
    git_ok(twin_dir.path(), &["cherry-pick", "--abort"]);
    assert_eq!(bisect_log(&twin), twin_before);
}

/// A pick paused during a bisect is libgit2's `CherryPick` state (checked before `Bisect`), so it
/// still blocks what a paused pick blocks.
#[test]
fn a_pick_paused_during_a_bisect_still_blocks_a_new_pick() {
    let (_dir, repo, ids) = bisect_repo(true);
    let picked = git_core::pick::cherry_pick(
        &repo,
        &ids.feature.to_string(),
        None,
        HookOptions::default(),
    )
    .unwrap();
    assert!(
        matches!(picked, PickOutcome::Conflicted { .. }),
        "{picked:?}"
    );

    let again =
        git_core::pick::cherry_pick(&repo, &ids.clean.to_string(), None, HookOptions::default());
    assert!(
        matches!(again, Err(PickError::OperationInProgress)),
        "{again:?}"
    );
    let reset = git_core::reset::reset_to(
        &repo,
        &ids.base.to_string(),
        git_core::reset::ResetMode::Soft,
    );
    assert!(matches!(
        reset,
        Err(git_core::reset::ResetError::OperationInProgress)
    ));
}

#[test]
fn reset_and_amend_run_during_a_bisect() {
    let (dir, repo, ids) = bisect_repo(true);
    let before = bisect_log(&repo);

    let amended =
        git_core::commit::amend_head(&repo, "amended\n", None, HookOptions::default()).unwrap();
    assert_eq!(head(&repo), amended);
    assert_eq!(
        repo.head().unwrap().peel_to_commit().unwrap().message(),
        Ok("amended\n")
    );
    git_core::reset::reset_to(
        &repo,
        &ids.base.to_string(),
        git_core::reset::ResetMode::Hard,
    )
    .unwrap();
    assert_eq!(head(&repo), ids.base.to_string());
    assert_eq!(common::read(dir.path(), "f"), "base\n");
    assert_eq!(bisect_log(&repo), before);

    // git's twin keeps the bisect too.
    let (twin_dir, twin, twin_ids) = bisect_repo(true);
    let twin_before = bisect_log(&twin);
    git_ok(twin_dir.path(), &["commit", "--amend", "-m", "amended"]);
    git_ok(
        twin_dir.path(),
        &["reset", "--hard", &twin_ids.base.to_string()],
    );
    assert_eq!(bisect_log(&twin), twin_before);
}

/// libgit2's mixed reset ends with `cleanup_state`; Browsitory's keeps the bisect, a terminal
/// rebase's folder and a cherry-pick list, and writes git's reflog message (#249, #250).
#[test]
fn a_mixed_reset_keeps_a_terminal_bisect_and_moves_the_branch() {
    let (dir, repo, ids) = bisect_repo(false);
    let before = bisect_log(&repo);
    let sequencer = repo.path().join("sequencer");
    std::fs::create_dir(&sequencer).unwrap();
    std::fs::write(sequencer.join("todo"), "pick 1234567 B\n").unwrap();

    git_core::reset::reset_to(
        &repo,
        &ids.base.to_string(),
        git_core::reset::ResetMode::Mixed,
    )
    .unwrap();

    let main = repo.find_reference("refs/heads/main").unwrap();
    assert_eq!(main.target(), Some(ids.base));
    assert_eq!(repo.head().unwrap().name().unwrap(), "refs/heads/main");
    let reflog = repo.reflog("refs/heads/main").unwrap();
    assert_eq!(
        reflog.get(0).unwrap().message().unwrap(),
        Some(format!("reset: moving to {}", ids.base).as_str())
    );
    // The index is base's; the work tree keeps main's content.
    let staged = repo.index().unwrap().get_path(Path::new("f"), 0).unwrap();
    assert_eq!(
        repo.find_blob(staged.id).unwrap().content(),
        b"base\n".as_slice()
    );
    assert_eq!(common::read(dir.path(), "f"), "main\n");
    assert_eq!(bisect_log(&repo), before);
    assert_eq!(
        std::fs::read_to_string(sequencer.join("todo")).unwrap(),
        "pick 1234567 B\n"
    );
}

#[test]
fn undo_restores_during_a_bisect() {
    use git_core::undo::{capture, restore, SnapshotScope};
    let (dir, repo, _ids) = bisect_repo(true);
    let before_log = bisect_log(&repo);
    let original = head(&repo);
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    common::write_file(dir.path(), "x.txt", "x\n");
    common::commit_all(&repo, "x");
    let after = capture(&repo, SnapshotScope::Full).unwrap();

    restore(&repo, &after, &before, "undo: x").unwrap();

    assert_eq!(head(&repo), original);
    assert!(!dir.path().join("x.txt").exists());
    assert_eq!(bisect_log(&repo), before_log);
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
fn a_rebase_during_a_bisect_is_paused_not_interrupted() {
    // A rebase step committed with `git commit` during a bisect is adopted like any other (the
    // bisect is not "another operation", #250): the rebase stays paused, Continue finishes it,
    // and Abort puts the branch back.
    use git_core::rebase::RebaseHealth;
    let committed_by_hand = || {
        let (dir, repo, _ids) = bisect_repo(false);
        let tip = head(&repo);
        let plan = plan_onto(&repo, "feature");
        let (state, result) = git_core::rebase::start_rebase(&repo, "feature", plan).unwrap();
        assert!(matches!(result, RebaseStepResult::Conflicted { .. }));
        common::write_file(dir.path(), "f", "resolved\n");
        stage(&repo, "f");
        git_ok(dir.path(), &["commit", "--no-edit"]);
        let repo = reopen(&repo);
        assert_eq!(repo.state(), RepositoryState::Bisect);
        (dir, repo, state, tip)
    };

    let (dir, repo, mut state, _tip) = committed_by_hand();
    let before = bisect_log(&repo);
    assert_eq!(
        git_core::rebase::rebase_health(&repo, &state).unwrap(),
        RebaseHealth::Paused
    );
    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();
    assert_eq!(result, RebaseStepResult::Done);
    assert!(!repo.head_detached().unwrap());
    assert_eq!(common::read(dir.path(), "f"), "resolved\n");
    assert_eq!(bisect_log(&repo), before);

    let (dir, repo, state, tip) = committed_by_hand();
    let before = bisect_log(&repo);
    let abort = git_core::rebase::abort_rebase(&repo, &state).unwrap();
    assert!(abort.restored);
    assert_eq!(head(&repo), tip);
    assert!(!repo.head_detached().unwrap());
    assert_eq!(common::read(dir.path(), "f"), "main\n");
    assert_eq!(bisect_log(&repo), before);
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
fn a_terminal_bisect_is_reported_with_its_start() {
    use git_core::repo::{head_info, BisectInfo, OperationKind};
    let started_from_main = Some(BisectInfo {
        started_from: "main".to_string(),
    });

    let (_dir, repo, _ids) = bisect_repo(false);
    let info = head_info(&repo).unwrap();
    assert_eq!(info.operation, None);
    assert_eq!(info.bisect, started_from_main);

    let (dir, repo, _ids) = bisect_repo(true);
    let info = head_info(&repo).unwrap();
    assert_eq!(info.operation, None);
    assert_eq!(info.bisect, started_from_main);

    // A merge paused during the bisect: both.
    assert!(!git(dir.path(), &["merge", "feature"]).status.success());
    let info = head_info(&reopen(&repo)).unwrap();
    assert_eq!(info.operation, Some(OperationKind::Merge));
    assert_eq!(info.bisect, started_from_main);

    // Ended with `git bisect reset`: none.
    git_ok(dir.path(), &["merge", "--abort"]);
    git_ok(dir.path(), &["bisect", "reset"]);
    assert_eq!(head_info(&reopen(&repo)).unwrap().bisect, None);
}

/// A bisect started from a detached HEAD records the commit id it started from.
#[test]
fn a_bisect_started_from_a_detached_head_names_the_commit() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f", "base\n");
    common::commit_all(&repo, "base");
    let id = head(&repo);
    repo.set_head_detached(repo.head().unwrap().target().unwrap())
        .unwrap();
    git_ok(dir.path(), &["bisect", "start"]);

    let info = git_core::repo::head_info(&repo).unwrap();

    assert_eq!(info.bisect.unwrap().started_from, id);
}

#[test]
fn no_bisect_is_reported_without_one() {
    let (dir, repo) = common::init_repo();
    assert_eq!(git_core::repo::head_info(&repo).unwrap().bisect, None);
    common::write_file(dir.path(), "f", "base\n");
    common::commit_all(&repo, "base");
    assert_eq!(git_core::repo::head_info(&repo).unwrap().bisect, None);
}
