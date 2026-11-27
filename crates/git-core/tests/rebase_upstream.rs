//! Commits a rebase finds already upstream (#1150): `git rebase` leaves out a commit whose patch
//! the new base already has (patch-id, `--no-reapply-cherry-picks`) and drops one that becomes
//! empty (`--empty=drop`); a commit that was empty from the start is kept. Each case that git
//! has is compared with `git rebase` on a copy of the same repository.

mod common;

use std::path::Path;

use common::{commit_all, git_ok, init_repo, twin, write_file};
use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseStepResult};

/// `master` with a base commit, `topic` from it with commits A (adds `t.txt`) and B (adds
/// `u.txt`); `topic` is checked out.
fn topic_a_b() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "base\n");
    commit_all(&repo, "base");
    git_ok(dir.path(), &["branch", "-M", "master"]);
    git_ok(dir.path(), &["checkout", "-q", "-b", "topic"]);
    write_file(dir.path(), "t.txt", "t\n");
    commit_all(&repo, "A");
    write_file(dir.path(), "u.txt", "u\n");
    commit_all(&repo, "B");
    (dir, repo)
}

/// Commits `m.txt` on master, then cherry-picks `ids` there with git (on the base itself, the
/// pick made in the same second would be the very same commit), and checks `topic` out again.
fn cherry_pick_on_master(dir: &Path, repo: &git2::Repository, ids: &[&str]) {
    commit_on_master(dir, repo, "m.txt", "m\n", "master moves on");
    git_ok(dir, &["checkout", "-q", "master"]);
    let mut args = vec!["cherry-pick"];
    args.extend_from_slice(ids);
    git_ok(dir, &args);
    git_ok(dir, &["checkout", "-q", "topic"]);
}

fn commit_on_master(dir: &Path, repo: &git2::Repository, path: &str, content: &str, message: &str) {
    git_ok(dir, &["checkout", "-q", "master"]);
    write_file(dir, path, content);
    commit_all(repo, message);
    git_ok(dir, &["checkout", "-q", "topic"]);
}

fn picks(repo: &git2::Repository) -> Vec<RebasePlanEntry> {
    git_core::rebase::commits_since(repo, "master")
        .unwrap()
        .into_iter()
        .map(|commit| RebasePlanEntry {
            commit_id: commit.id,
            action: RebaseAction::Pick,
            combined_message: None,
        })
        .collect()
}

/// `master..topic`, newest first: each commit's subject and tree.
fn topic_commits(dir: &Path) -> String {
    let output = test_support::git(dir, &["log", "--format=%s %T", "master..topic"]);
    assert!(output.status.success(), "git log: {output:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// Rebases `topic` onto `master` with every commit picked, here and with `git rebase master` on
/// a copy, and returns both results (`topic_commits`), the app's first.
fn rebase_both(dir: &Path, repo: &git2::Repository) -> (String, String) {
    let (twin_dir, _twin_repo) = twin(dir);
    git_ok(twin_dir.path(), &["rebase", "-q", "master"]);
    let (_, result) = git_core::rebase::start_rebase(repo, "master", picks(repo)).unwrap();
    assert_eq!(result, RebaseStepResult::Done);
    (topic_commits(dir), topic_commits(twin_dir.path()))
}

#[test]
fn a_commit_cherry_picked_upstream_is_left_out_as_git_rebase_does() {
    let (dir, repo) = topic_a_b();
    let a = repo.revparse_single("topic~1").unwrap().id().to_string();
    cherry_pick_on_master(dir.path(), &repo, &[&a]);

    let (ours, gits) = rebase_both(dir.path(), &repo);

    assert_eq!(ours, gits);
    assert!(
        ours.starts_with("B ") && ours.lines().count() == 1,
        "{ours}"
    );
}

#[test]
fn a_commit_upstream_changed_again_is_left_out_without_a_conflict_as_git_rebase_does() {
    let (dir, repo) = topic_a_b();
    let a = repo.revparse_single("topic~1").unwrap().id().to_string();
    cherry_pick_on_master(dir.path(), &repo, &[&a]);
    // Picking A again would now add t.txt where master changed it: an add/add conflict.
    commit_on_master(
        dir.path(),
        &repo,
        "t.txt",
        "t changed upstream\n",
        "change t",
    );

    let (ours, gits) = rebase_both(dir.path(), &repo);

    assert_eq!(ours, gits);
    assert!(
        ours.starts_with("B ") && ours.lines().count() == 1,
        "{ours}"
    );
}

#[test]
fn a_commit_that_becomes_empty_is_dropped_as_git_rebase_drops_it() {
    let (dir, repo) = topic_a_b();
    // master makes A's change along with another one: not the same patch, but A adds nothing.
    git_ok(dir.path(), &["checkout", "-q", "master"]);
    write_file(dir.path(), "t.txt", "t\n");
    write_file(dir.path(), "v.txt", "v\n");
    commit_all(&repo, "A and more");
    git_ok(dir.path(), &["checkout", "-q", "topic"]);

    let (ours, gits) = rebase_both(dir.path(), &repo);

    assert_eq!(ours, gits);
    assert!(
        ours.starts_with("B ") && ours.lines().count() == 1,
        "{ours}"
    );
}

#[test]
fn a_commit_empty_from_the_start_is_kept_as_git_rebase_keeps_it() {
    let (dir, repo) = topic_a_b();
    git_ok(
        dir.path(),
        &["commit", "-q", "--allow-empty", "-m", "empty on purpose"],
    );
    commit_on_master(dir.path(), &repo, "m.txt", "m\n", "master moves on");

    let (ours, gits) = rebase_both(dir.path(), &repo);

    assert_eq!(ours, gits);
    assert_eq!(ours.lines().count(), 3, "{ours}");
}

#[test]
fn a_fixup_that_becomes_empty_leaves_the_leader_as_it_is() {
    let (dir, repo) = topic_a_b();
    let a = repo.revparse_single("topic~1").unwrap().id().to_string();
    cherry_pick_on_master(dir.path(), &repo, &[&a]);
    // Plan: B, then A folded into it.
    let mut plan = picks(&repo);
    plan.reverse();
    plan[1].action = RebaseAction::Fixup;

    let (_, result) = git_core::rebase::start_rebase(&repo, "master", plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let topic = topic_commits(dir.path());
    assert!(
        topic.starts_with("B ") && topic.lines().count() == 1,
        "{topic}"
    );
    let changed = test_support::git(dir.path(), &["diff", "--name-only", "master", "topic"]);
    assert_eq!(String::from_utf8(changed.stdout).unwrap(), "u.txt\n");
}

#[test]
fn a_group_that_adds_nothing_leaves_no_commit() {
    let (dir, repo) = topic_a_b();
    let a = repo.revparse_single("topic~1").unwrap().id().to_string();
    let b = repo.revparse_single("topic").unwrap().id().to_string();
    cherry_pick_on_master(dir.path(), &repo, &[&a, &b]);
    let mut plan = picks(&repo);
    plan[1].action = RebaseAction::Fixup;

    let (_, result) = git_core::rebase::start_rebase(&repo, "master", plan).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    assert_eq!(topic_commits(dir.path()), "");
    assert_eq!(
        repo.revparse_single("topic").unwrap().id(),
        repo.revparse_single("master").unwrap().id()
    );
}

#[test]
fn continuing_after_a_conflict_resolved_to_nothing_drops_the_step_as_git_rebase_does() {
    let (dir, repo) = topic_a_b();
    commit_on_master(dir.path(), &repo, "t.txt", "master's t\n", "master adds t");
    let (twin_dir, _twin_repo) = twin(dir.path());
    let started = test_support::git(twin_dir.path(), &["rebase", "-q", "master"]);
    assert!(!started.status.success(), "git's rebase should stop at A");
    git_ok(twin_dir.path(), &["checkout", "--ours", "t.txt"]);
    git_ok(twin_dir.path(), &["add", "t.txt"]);
    git_ok(
        twin_dir.path(),
        &["-c", "core.editor=true", "rebase", "--continue"],
    );

    let (mut state, result) =
        git_core::rebase::start_rebase(&repo, "master", picks(&repo)).unwrap();
    assert!(matches!(result, RebaseStepResult::Conflicted { .. }));
    git_ok(dir.path(), &["checkout", "--ours", "t.txt"]);
    git_ok(dir.path(), &["add", "t.txt"]);
    let result = git_core::rebase::rebase_continue(&repo, &mut state).unwrap();

    assert_eq!(result, RebaseStepResult::Done);
    let ours = topic_commits(dir.path());
    assert_eq!(ours, topic_commits(twin_dir.path()));
    assert!(
        ours.starts_with("B ") && ours.lines().count() == 1,
        "{ours}"
    );
}
