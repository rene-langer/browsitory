// crates/git-core/tests/leftover_state.rs
//! A merge, cherry-pick or revert whose commit already landed but whose state files stayed
//! behind (D7-A: the cleanup after the commit failed). Such a leftover is cleared before the next
//! commit (#183, #226, #232) and before an amend (#240). State that belongs to an operation still
//! in progress is never cleared: the commit finishes it and amend is refused, as in git.
mod common;

#[cfg(unix)]
use git_core::commit::commit;
use git_core::commit::{amend_head, CommitError};
use git_core::hooks::HookOptions;
use git_core::pick::{cherry_pick, revert, PickOutcome};
use git_core::repo::{head_info, OperationKind};
use std::path::Path;

fn stage(repo: &git2::Repository, path: &str) {
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();
}

fn head(repo: &git2::Repository) -> git2::Commit<'_> {
    repo.head().unwrap().peel_to_commit().unwrap()
}

/// base (a.txt "one", b.txt "b") then X (a.txt "two"). HEAD is X; returns X.
fn repo_with_x() -> (tempfile::TempDir, git2::Repository, git2::Oid) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "one\n");
    common::write_file(dir.path(), "b.txt", "b\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "a.txt", "two\n");
    common::commit_all(&repo, "X");
    let x = head(&repo).id();
    (dir, repo, x)
}

/// The files a revert of `reverted` leaves while paused: REVERT_HEAD and MERGE_MSG.
fn write_revert_state(repo: &git2::Repository, reverted: git2::Oid, message: &str) {
    std::fs::write(repo.path().join("REVERT_HEAD"), format!("{reverted}\n")).unwrap();
    std::fs::write(repo.path().join("MERGE_MSG"), message).unwrap();
    assert_eq!(repo.state(), git2::RepositoryState::Revert);
}

/// Reverts X cleanly, then puts its state files back as if the cleanup after the commit had
/// failed (D7-A). Stages a new `after.txt`. Returns the revert commit.
fn leave_a_revert_state_behind(dir: &Path, repo: &git2::Repository, x: git2::Oid) -> git2::Oid {
    let outcome = revert(repo, &x.to_string(), None, HookOptions::default()).unwrap();
    let PickOutcome::Committed { commit_id } = outcome else {
        panic!("expected Committed, got {outcome:?}")
    };
    let reverted = git2::Oid::from_str(&commit_id).unwrap();
    let message = head(repo).message().unwrap().to_string();
    write_revert_state(repo, x, &message);
    common::write_file(dir, "after.txt", "after\n");
    stage(repo, "after.txt");
    reverted
}

/// A commit F (c.txt) by "Other Author" at a fixed time, on a side line off HEAD; HEAD stays.
fn foreign_commit(repo: &git2::Repository) -> git2::Oid {
    let parent = head(repo);
    let mut builder = repo.treebuilder(Some(&parent.tree().unwrap())).unwrap();
    builder
        .insert("c.txt", repo.blob(b"c\n").unwrap(), 0o100644)
        .unwrap();
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    let other = git2::Signature::new(
        "Other Author",
        "other@example.com",
        &git2::Time::new(1_000_000, 0),
    )
    .unwrap();
    repo.commit(None, &other, &other, "foreign change\n", &tree, &[&parent])
        .unwrap()
}

/// Cherry-picks F cleanly, then puts CHERRY_PICK_HEAD and MERGE_MSG back (D7-A). Returns the
/// pick commit.
fn leave_a_pick_state_behind(repo: &git2::Repository) -> git2::Oid {
    let foreign = foreign_commit(repo);
    let outcome = cherry_pick(repo, &foreign.to_string(), None, HookOptions::default()).unwrap();
    let PickOutcome::Committed { commit_id } = outcome else {
        panic!("expected Committed, got {outcome:?}")
    };
    std::fs::write(repo.path().join("CHERRY_PICK_HEAD"), format!("{foreign}\n")).unwrap();
    std::fs::write(repo.path().join("MERGE_MSG"), "foreign change\n").unwrap();
    assert_eq!(repo.state(), git2::RepositoryState::CherryPick);
    git2::Oid::from_str(&commit_id).unwrap()
}

/// A side commit (d.txt) off base, merged into X by a merge commit that HEAD moves to; then
/// MERGE_HEAD, MERGE_MODE and MERGE_MSG are put back (D7-A). Returns the merge commit.
fn leave_a_merge_state_behind(repo: &git2::Repository, x: git2::Oid) -> git2::Oid {
    let base = head(repo).parent_id(0).unwrap();
    let side = common::commit_tree(
        repo,
        &[
            ("a.txt", "one\n", 0o100644),
            ("b.txt", "b\n", 0o100644),
            ("d.txt", "d\n", 0o100644),
        ],
        &[base],
        "side",
    );
    let merge = common::commit_tree(
        repo,
        &[
            ("a.txt", "two\n", 0o100644),
            ("b.txt", "b\n", 0o100644),
            ("d.txt", "d\n", 0o100644),
        ],
        &[x, side],
        "Merge side",
    );
    repo.reference("refs/heads/merged", merge, true, "test")
        .unwrap();
    repo.set_head("refs/heads/merged").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    std::fs::write(repo.path().join("MERGE_HEAD"), format!("{side}\n")).unwrap();
    std::fs::write(repo.path().join("MERGE_MODE"), "no-ff").unwrap();
    std::fs::write(repo.path().join("MERGE_MSG"), "Merge side\n").unwrap();
    assert_eq!(repo.state(), git2::RepositoryState::Merge);
    merge
}

/// A cherry-pick of a commit that changes a.txt to "three", paused on its conflict with X and
/// resolved (staged): a pick still in progress.
fn pause_a_pick_on_a_conflict(dir: &Path, repo: &git2::Repository) {
    let base = head(repo).parent_id(0).unwrap();
    let theirs = common::commit_tree(
        repo,
        &[("a.txt", "three\n", 0o100644), ("b.txt", "b\n", 0o100644)],
        &[base],
        "three",
    );
    let outcome = cherry_pick(repo, &theirs.to_string(), None, HookOptions::default()).unwrap();
    assert!(
        matches!(outcome, PickOutcome::Conflicted { .. }),
        "{outcome:?}"
    );
    common::write_file(dir, "a.txt", "resolved\n");
    stage(repo, "a.txt");
    assert_eq!(repo.state(), git2::RepositoryState::CherryPick);
}

fn no_state_files(repo: &git2::Repository) {
    for name in [
        "MERGE_HEAD",
        "MERGE_MODE",
        "MERGE_MSG",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
    ] {
        assert!(!repo.path().join(name).exists(), "{name} is still there");
    }
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

// ---- #232: a leftover REVERT_HEAD --------------------------------------------------------------

/// The next commit isn't taken for the revert: `prepare-commit-msg` gets `message`, not `merge`,
/// and the state is gone afterwards.
#[cfg(unix)]
#[test]
fn a_commit_after_a_leftover_revert_is_an_ordinary_commit() {
    let (dir, repo, x) = repo_with_x();
    let reverted = leave_a_revert_state_behind(dir.path(), &repo, x);
    common::log_hooks(&repo, &["prepare-commit-msg"]);
    let mut repo = repo;

    commit(&mut repo, "mine\n", HookOptions::default()).unwrap();

    assert_eq!(
        common::read_log(&repo),
        "prepare-commit-msg <msg> message\n"
    );
    let head = head(&repo);
    assert_eq!(head.message().unwrap(), "mine\n");
    assert_eq!(head.parent_ids().collect::<Vec<_>>(), [reverted]);
    no_state_files(&repo);
}

/// HEAD undoes X exactly, but its message doesn't say "This reverts commit X": it isn't the
/// revert's own commit, so REVERT_HEAD is a revert in progress and the commit finishes it.
#[cfg(unix)]
#[test]
fn a_look_alike_of_the_revert_does_not_clear_a_revert_in_progress() {
    let (dir, repo, x) = repo_with_x();
    common::write_file(dir.path(), "a.txt", "one\n");
    common::commit_all(&repo, "undo two by hand");
    write_revert_state(
        &repo,
        x,
        &format!("Revert \"X\"\n\nThis reverts commit {x}.\n"),
    );
    common::log_hooks(&repo, &["prepare-commit-msg"]);
    let mut repo = repo;

    assert!(!head_info(&repo).unwrap().operation_leftover);
    commit(&mut repo, "finish\n", HookOptions::default()).unwrap();

    assert_eq!(common::read_log(&repo), "prepare-commit-msg <msg> merge\n");
    no_state_files(&repo);
}

#[cfg(unix)]
#[test]
fn a_leftover_revert_that_cannot_be_cleared_refuses_the_commit() {
    let (dir, repo, x) = repo_with_x();
    let reverted = leave_a_revert_state_behind(dir.path(), &repo, x);
    let mut repo = repo;
    let Some(read_only) = common::ReadOnlyGitDir::new(&repo) else {
        return;
    };

    let error = commit(&mut repo, "mine\n", HookOptions::default()).unwrap_err();
    drop(read_only);

    assert!(
        matches!(error, CommitError::LeftoverRevertState),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "This revert was already committed, but git's revert state can't be cleared. \
         Check the permissions of the .git folder, then commit again."
    );
    assert_eq!(head(&repo).id(), reverted);
    assert_eq!(repo.state(), git2::RepositoryState::Revert);
}

// ---- #240: amend with a leftover ---------------------------------------------------------------

#[test]
fn amend_after_a_leftover_pick_amends_the_pick() {
    let (_dir, repo, x) = repo_with_x();
    let pick = leave_a_pick_state_behind(&repo);

    amend_head(&repo, "reworded", None, HookOptions::default()).unwrap();

    let head = head(&repo);
    assert_ne!(head.id(), pick);
    assert_eq!(head.message().unwrap(), "reworded");
    assert_eq!(head.parent_ids().collect::<Vec<_>>(), [x]);
    // Amend keeps HEAD's author: the picked commit's.
    assert_eq!(head.author().name().unwrap(), "Other Author");
    no_state_files(&repo);
}

#[test]
fn amend_after_a_leftover_merge_keeps_both_parents() {
    let (_dir, repo, x) = repo_with_x();
    let merge = leave_a_merge_state_behind(&repo, x);
    let parents: Vec<_> = repo.find_commit(merge).unwrap().parent_ids().collect();

    amend_head(&repo, "Merge side, reworded", None, HookOptions::default()).unwrap();

    let head = head(&repo);
    assert_eq!(head.message().unwrap(), "Merge side, reworded");
    assert_eq!(head.parent_ids().collect::<Vec<_>>(), parents);
    no_state_files(&repo);
}

#[cfg(unix)]
#[test]
fn amend_after_a_leftover_revert_amends_the_revert() {
    let (dir, repo, x) = repo_with_x();
    let reverted = leave_a_revert_state_behind(dir.path(), &repo, x);
    common::log_hooks(&repo, &["prepare-commit-msg"]);

    amend_head(&repo, "reworded\n", None, HookOptions::default()).unwrap();

    assert_eq!(
        common::read_log(&repo),
        "prepare-commit-msg <msg> message\n"
    );
    let head = head(&repo);
    assert_eq!(head.message().unwrap(), "reworded\n");
    assert_eq!(
        head.parent_ids().collect::<Vec<_>>(),
        repo.find_commit(reverted)
            .unwrap()
            .parent_ids()
            .collect::<Vec<_>>()
    );
    assert!(head.tree().unwrap().get_name("after.txt").is_some());
    no_state_files(&repo);
}

#[cfg(unix)]
#[test]
fn a_leftover_that_cannot_be_cleared_refuses_the_amend() {
    let (_dir, repo, _) = repo_with_x();
    let pick = leave_a_pick_state_behind(&repo);
    let Some(read_only) = common::ReadOnlyGitDir::new(&repo) else {
        return;
    };

    let error = amend_head(&repo, "reworded", None, HookOptions::default()).unwrap_err();
    drop(read_only);

    assert!(matches!(error, CommitError::LeftoverPickState), "{error:?}");
    assert_eq!(head(&repo).id(), pick);
    assert_eq!(repo.state(), git2::RepositoryState::CherryPick);
}

/// A pick still in progress (paused on a conflict, resolved) is not a leftover: amend is refused
/// and the pick stays, as `git commit --amend` refuses ("You are in the middle of a cherry-pick").
#[test]
fn amend_refuses_while_a_pick_is_in_progress_like_git() {
    let (dir, repo, _) = repo_with_x();
    pause_a_pick_on_a_conflict(dir.path(), &repo);
    let before = head(&repo).id();

    let git = test_support::git_command(dir.path())
        .args(["commit", "--amend", "-m", "amended by git"])
        .output()
        .unwrap();
    assert!(!git.status.success(), "git refuses too");
    assert!(
        String::from_utf8_lossy(&git.stderr).contains("cherry-pick"),
        "{}",
        String::from_utf8_lossy(&git.stderr)
    );

    assert!(matches!(
        amend_head(&repo, "amended", None, HookOptions::default()),
        Err(CommitError::OperationInProgress)
    ));
    assert_eq!(head(&repo).id(), before);
    assert_eq!(repo.state(), git2::RepositoryState::CherryPick);
}

/// git amends with a REVERT_HEAD present (it only refuses for MERGE_HEAD and CHERRY_PICK_HEAD)
/// and clears it with MERGE_MSG; Browsitory's amend after a leftover revert ends the same way.
#[cfg(unix)]
#[test]
fn amend_after_a_leftover_revert_ends_like_git_commit_amend() {
    let run = |use_git: bool| -> (String, String) {
        let (dir, repo, x) = repo_with_x();
        let reverted = leave_a_revert_state_behind(dir.path(), &repo, x);
        common::log_hooks(&repo, &["prepare-commit-msg"]);
        if use_git {
            let output = test_support::git_command(dir.path())
                .args(["commit", "--amend", "-m", "reworded"])
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:?}");
        } else {
            amend_head(&repo, "reworded\n", None, HookOptions::default()).unwrap();
        }
        no_state_files(&repo);
        let head = head(&repo);
        assert_eq!(head.message().unwrap(), "reworded\n");
        assert_eq!(
            head.parent_id(0).unwrap(),
            repo.find_commit(reverted).unwrap().parent_id(0).unwrap()
        );
        // git passes the message file relative to the work tree.
        let log = common::read_log(&repo).replace(".git/COMMIT_EDITMSG", "<msg>");
        (log, common::read(dir.path(), "after.txt"))
    };
    let git = run(true);
    assert_eq!(git.0, "prepare-commit-msg <msg> message\n");
    assert_eq!(run(false), git);
}

// ---- head_info ----------------------------------------------------------------------------------

#[test]
fn head_info_tells_a_leftover_from_an_operation_in_progress() {
    let (_dir, repo, _) = repo_with_x();
    leave_a_pick_state_behind(&repo);
    let info = head_info(&repo).unwrap();
    assert_eq!(info.operation, Some(OperationKind::CherryPick));
    assert!(info.operation_leftover);

    let (dir, repo, x2) = repo_with_x();
    leave_a_revert_state_behind(dir.path(), &repo, x2);
    let info = head_info(&repo).unwrap();
    assert_eq!(info.operation, Some(OperationKind::Revert));
    assert!(info.operation_leftover);

    let (_dir, repo, x3) = repo_with_x();
    leave_a_merge_state_behind(&repo, x3);
    let info = head_info(&repo).unwrap();
    assert_eq!(info.operation, Some(OperationKind::Merge));
    assert!(info.operation_leftover);

    let (dir, repo, _) = repo_with_x();
    pause_a_pick_on_a_conflict(dir.path(), &repo);
    let info = head_info(&repo).unwrap();
    assert_eq!(info.operation, Some(OperationKind::CherryPick));
    assert!(!info.operation_leftover);
}

/// The commit after a leftover merge is an ordinary commit, so it runs `pre-commit`, not
/// `pre-merge-commit`: the "Skip hooks" checkbox doesn't list the latter.
#[cfg(unix)]
#[test]
fn head_info_does_not_list_pre_merge_commit_for_a_leftover_merge() {
    let (_dir, repo, x) = repo_with_x();
    leave_a_merge_state_behind(&repo, x);
    let hooks = repo.path().join("hooks");
    common::install_hook(&hooks, "pre-merge-commit", "exit 0");
    common::install_hook(&hooks, "pre-commit", "exit 0");

    assert_eq!(
        head_info(&repo).unwrap().commit_hooks,
        vec!["pre-commit".to_string()]
    );
}
