//! A `git rebase` or `git am` started in a terminal (#263, #265). Each fixture runs the git CLI
//! (isolated from the developer's config and gpg agent, #282) until it stops on a conflict;
//! Browsitory then reads and acts on that repository, and where git's own answer matters it is
//! recorded next to Browsitory's.
mod common;

use std::path::Path;

use git2::{Oid, Repository, RepositoryState};
use git_core::hooks::HookOptions;
use git_core::rebase::{RebaseAction, RebasePlanEntry, RebaseStepResult};
use git_core::repo::{TerminalRebase, TerminalRebaseKind};
use git_core::reset::ResetMode;
use tempfile::TempDir;

use test_support::git;

fn git_ok(dir: &Path, args: &[&str]) {
    let output = git(dir, args);
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

fn reopen(repo: &Repository) -> Repository {
    Repository::open(repo.workdir().unwrap()).unwrap()
}

const REBASE_REFUSAL: &str = "A rebase started in a terminal isn't finished. Continue it there \
                              with git rebase --continue, or abort it with git rebase --abort.";

/// The commits the fixture is built from.
struct Ids {
    /// main's tip: f="main\n" on base.
    main: Oid,
    /// topic's tip before any rebase: t1 (f="topic\n"), t2 (adds t2.txt), t3 (adds t3.txt).
    topic: Oid,
    /// The first commit on topic (t1), which conflicts with main.
    t1: Oid,
}

/// base (f="base\n"); main: "main" (f="main\n"); topic from base: t1 (f="topic\n"), t2, t3.
/// HEAD is topic.
fn fixture() -> (TempDir, Repository, Ids) {
    let (dir, repo) = common::init_repo();
    repo.set_head("refs/heads/main").unwrap();
    common::write_file(dir.path(), "f", "base\n");
    common::commit_all(&repo, "base");
    let base = repo.head().unwrap().target().unwrap();
    let t1 = common::commit_tree(&repo, &[("f", "topic\n", 0o100644)], &[base], "t1");
    let t2 = common::commit_tree(
        &repo,
        &[("f", "topic\n", 0o100644), ("t2.txt", "2\n", 0o100644)],
        &[t1],
        "t2",
    );
    let topic = common::commit_tree(
        &repo,
        &[
            ("f", "topic\n", 0o100644),
            ("t2.txt", "2\n", 0o100644),
            ("t3.txt", "3\n", 0o100644),
        ],
        &[t2],
        "t3",
    );
    repo.reference("refs/heads/topic", topic, false, "test")
        .unwrap();
    common::write_file(dir.path(), "f", "main\n");
    common::commit_all(&repo, "main");
    let main = repo.head().unwrap().target().unwrap();
    git_ok(dir.path(), &["checkout", "-q", "topic"]);
    let repo = reopen(&repo);
    (dir, repo, Ids { main, topic, t1 })
}

/// `git rebase main` on topic, stopped on t1's conflict in f.
fn terminal_rebase() -> (TempDir, Repository, Ids) {
    let (dir, repo, ids) = fixture();
    let output = git(dir.path(), &["rebase", "main"]);
    assert!(!output.status.success(), "the rebase must stop: {output:?}");
    assert!(repo.path().join("rebase-merge").is_dir());
    let repo = reopen(&repo);
    (dir, repo, ids)
}

fn terminal_rebase_info(repo: &Repository) -> Option<TerminalRebase> {
    git_core::repo::head_info(repo).unwrap().terminal_rebase
}

fn edit_plan(commit: Oid) -> Vec<RebasePlanEntry> {
    vec![RebasePlanEntry {
        commit_id: commit.to_string(),
        action: RebaseAction::Edit,
        combined_message: None,
    }]
}

#[test]
fn head_info_reports_a_terminal_rebase_paused_on_a_conflict() {
    let (_dir, repo, ids) = terminal_rebase();

    let info = git_core::repo::head_info(&repo).unwrap();

    assert_eq!(
        info.terminal_rebase,
        Some(TerminalRebase {
            kind: TerminalRebaseKind::Rebase,
            branch: "topic".into(),
            onto: ids.main.to_string(),
            current_step: 1,
            total_steps: 3,
        })
    );
    // Not a merge, though git wrote MERGE_MSG for the conflicted step.
    assert_eq!(info.operation, None);
    assert!(repo.path().join("MERGE_MSG").exists());
}

#[test]
fn head_info_reports_a_rebase_with_the_apply_backend() {
    let (dir, repo, ids) = fixture();
    let output = git(dir.path(), &["rebase", "--apply", "main"]);
    assert!(!output.status.success(), "the rebase must stop: {output:?}");
    let repo = reopen(&repo);
    assert!(repo.path().join("rebase-apply").is_dir());

    assert_eq!(
        terminal_rebase_info(&repo),
        Some(TerminalRebase {
            kind: TerminalRebaseKind::Rebase,
            branch: "topic".into(),
            onto: ids.main.to_string(),
            current_step: 1,
            total_steps: 3,
        })
    );
}

#[test]
fn head_info_reports_git_am() {
    let (dir, repo, ids) = fixture();
    let patches = TempDir::new().unwrap();
    git_ok(
        dir.path(),
        &[
            "format-patch",
            "-q",
            "-3",
            "topic",
            "-o",
            patches.path().to_str().unwrap(),
        ],
    );
    git_ok(dir.path(), &["checkout", "-q", "main"]);
    let mut files: Vec<String> = std::fs::read_dir(patches.path())
        .unwrap()
        .map(|entry| entry.unwrap().path().to_str().unwrap().to_string())
        .collect();
    files.sort();
    let mut args = vec!["am"];
    args.extend(files.iter().map(String::as_str));
    let output = git(dir.path(), &args);
    assert!(!output.status.success(), "git am must stop: {output:?}");
    let repo = reopen(&repo);
    assert_eq!(repo.state(), RepositoryState::ApplyMailbox);

    assert_eq!(
        terminal_rebase_info(&repo),
        Some(TerminalRebase {
            kind: TerminalRebaseKind::Am,
            branch: String::new(),
            onto: String::new(),
            current_step: 1,
            total_steps: 3,
        })
    );
    assert_eq!(
        git_core::pick::cherry_pick(&repo, &ids.t1.to_string(), None, HookOptions::default())
            .unwrap_err()
            .to_string(),
        "A git am started in a terminal isn't finished. Continue it there with git am \
         --continue, or abort it with git am --abort."
    );
}

#[test]
fn head_info_reports_no_terminal_rebase_otherwise() {
    let (_dir, repo, _ids) = fixture();
    assert_eq!(terminal_rebase_info(&repo), None);
}

/// Browsitory's own rebase replays steps with cherry-picks and never writes git's rebase
/// folders, so it is never taken for a terminal rebase.
#[test]
fn browsitorys_own_rebase_is_not_a_terminal_rebase() {
    let (_dir, repo, ids) = fixture();
    // topic's t2 again onto t1, pausing there.
    let t2 = repo.find_commit(ids.topic).unwrap().parent_id(0).unwrap();
    let (_state, step) =
        git_core::rebase::start_rebase(&repo, &ids.t1.to_string(), edit_plan(t2)).unwrap();
    assert_eq!(step, RebaseStepResult::PausedForEdit);

    assert!(!repo.path().join("rebase-merge").exists());
    assert!(!repo.path().join("rebase-apply").exists());
    assert_eq!(terminal_rebase_info(&repo), None);
}

/// #263: Abort merge used to reset the rebase's conflicted step and remove its MERGE_MSG. It now
/// refuses, naming the rebase, and changes nothing; `git rebase --abort` still puts topic back.
#[test]
fn abort_merge_refuses_a_terminal_rebase_and_changes_nothing() {
    let (dir, repo, ids) = terminal_rebase();
    let conflicted = std::fs::read_to_string(dir.path().join("f")).unwrap();
    let merge_msg = std::fs::read(repo.path().join("MERGE_MSG")).unwrap();
    let head = repo.head().unwrap().target().unwrap();

    let error = git_core::merge::abort_merge(&repo).unwrap_err();

    assert_eq!(error.to_string(), REBASE_REFUSAL);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("f")).unwrap(),
        conflicted
    );
    assert_eq!(
        std::fs::read(repo.path().join("MERGE_MSG")).unwrap(),
        merge_msg
    );
    assert!(reopen(&repo).index().unwrap().has_conflicts());
    assert_eq!(repo.head().unwrap().target(), Some(head));
    assert!(repo.path().join("rebase-merge").is_dir());

    // git refuses `git merge --abort` here too, and the rebase is still git's to abort.
    assert!(!git(dir.path(), &["merge", "--abort"]).status.success());
    git_ok(dir.path(), &["rebase", "--abort"]);
    let repo = reopen(&repo);
    assert_eq!(
        repo.head().unwrap().name().ok(),
        Some("refs/heads/topic"),
        "back on topic"
    );
    assert_eq!(repo.head().unwrap().target(), Some(ids.topic));
}

/// #265: starting a rebase during a terminal rebase names it (it used to say "HEAD is already
/// detached"); git refuses a second rebase too.
#[test]
fn start_rebase_refuses_a_terminal_rebase_naming_it() {
    let (dir, repo, ids) = terminal_rebase();

    let error = git_core::rebase::start_rebase(&repo, &ids.main.to_string(), edit_plan(ids.t1))
        .unwrap_err();

    assert_eq!(error.to_string(), REBASE_REFUSAL);
    assert!(!git(dir.path(), &["rebase", "main"]).status.success());
}

/// #265: a merge paused on a conflict is named, not reported as uncommitted changes.
#[test]
fn start_rebase_refuses_a_merge_in_progress_naming_it() {
    let (dir, repo, ids) = fixture();
    let output = git(dir.path(), &["merge", "main"]);
    assert!(
        !output.status.success(),
        "the merge must conflict: {output:?}"
    );
    let repo = reopen(&repo);
    assert_eq!(repo.state(), RepositoryState::Merge);

    let error = git_core::rebase::start_rebase(&repo, &ids.main.to_string(), edit_plan(ids.topic))
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "A merge is in progress. Commit or abort it before starting a rebase."
    );
}

/// #265: a merge whose conflict was resolved to HEAD's side (nothing conflicted, staged or
/// unstaged) used to get past every check of `start_rebase`, which then rebased under it.
#[test]
fn start_rebase_refuses_a_resolved_merge_in_progress() {
    let (dir, repo, ids) = fixture();
    let output = git(dir.path(), &["merge", "main"]);
    assert!(
        !output.status.success(),
        "the merge must conflict: {output:?}"
    );
    common::write_file(dir.path(), "f", "topic\n");
    git_ok(dir.path(), &["add", "f"]);
    let repo = reopen(&repo);
    assert_eq!(repo.state(), RepositoryState::Merge);

    let error = git_core::rebase::start_rebase(&repo, &ids.main.to_string(), edit_plan(ids.topic))
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "A merge is in progress. Commit or abort it before starting a rebase."
    );
    assert_eq!(repo.state(), RepositoryState::Merge);
}

/// #265: a cherry-pick and a revert paused on a conflict are named.
#[test]
fn start_rebase_refuses_a_paused_cherry_pick_or_revert_naming_it() {
    for noun in ["cherry-pick", "revert"] {
        let (dir, repo, ids) = fixture();
        let main = ids.main.to_string();
        let output = if noun == "revert" {
            // On main, f changed again after "main", so reverting "main" conflicts.
            git_ok(dir.path(), &["checkout", "-q", "main"]);
            common::write_file(dir.path(), "f", "later\n");
            git_ok(dir.path(), &["commit", "-qam", "later"]);
            git(dir.path(), &["revert", "--no-edit", &main])
        } else {
            // topic's f differs from main's.
            git(dir.path(), &["cherry-pick", &main])
        };
        assert!(!output.status.success(), "{noun} must conflict: {output:?}");
        let repo = reopen(&repo);

        let error =
            git_core::rebase::start_rebase(&repo, &ids.main.to_string(), edit_plan(ids.topic))
                .unwrap_err();

        assert_eq!(
            error.to_string(),
            format!("A {noun} is in progress. Commit or abort it before starting a rebase.")
        );
    }
}

/// #263: the commands that refuse during any paused operation name the terminal rebase.
#[test]
fn picks_reset_and_amend_name_a_terminal_rebase() {
    let (_dir, repo, ids) = terminal_rebase();

    let pick =
        git_core::pick::cherry_pick(&repo, &ids.t1.to_string(), None, HookOptions::default())
            .unwrap_err();
    assert_eq!(pick.to_string(), REBASE_REFUSAL);
    let revert = git_core::pick::revert(&repo, &ids.t1.to_string(), None, HookOptions::default())
        .unwrap_err();
    assert_eq!(revert.to_string(), REBASE_REFUSAL);
    let many =
        git_core::pick::cherry_pick_many(&repo, &[ids.t1.to_string()], HookOptions::default());
    assert_eq!(many.outcome.unwrap_err().to_string(), REBASE_REFUSAL);
    assert!(many.state.is_none());
    let reset =
        git_core::reset::reset_to(&repo, &ids.main.to_string(), ResetMode::Hard).unwrap_err();
    assert_eq!(reset.to_string(), REBASE_REFUSAL);
    let amend =
        git_core::commit::amend_head(&repo, "amended", None, HookOptions::default()).unwrap_err();
    assert_eq!(amend.to_string(), REBASE_REFUSAL);
    assert!(repo.path().join("rebase-merge").is_dir());
}

/// A merge started during a terminal rebase (stopped by a failing `exec`) has its own
/// MERGE_HEAD: Abort merge aborts it as `git merge --abort` does, and the rebase goes on.
#[test]
fn abort_merge_aborts_a_merge_paused_during_a_terminal_rebase() {
    let (dir, repo, ids) = fixture();
    let output = git(
        dir.path(),
        &["rebase", "-f", "-x", "false", &ids.t1.to_string()],
    );
    assert!(!output.status.success(), "the exec must stop: {output:?}");
    let stopped_at = reopen(&repo).head().unwrap().target().unwrap();
    let output = git(dir.path(), &["merge", "main"]);
    assert!(
        !output.status.success(),
        "the merge must conflict: {output:?}"
    );
    let repo = reopen(&repo);
    assert!(repo.path().join("MERGE_HEAD").exists());
    let rebase = terminal_rebase_info(&repo).unwrap();
    assert_eq!(rebase.kind, TerminalRebaseKind::Rebase);
    assert_eq!(rebase.branch, "topic");

    git_core::merge::abort_merge(&repo).unwrap();

    assert!(!repo.path().join("MERGE_HEAD").exists());
    assert!(!reopen(&repo).index().unwrap().has_conflicts());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("f")).unwrap(),
        "topic\n"
    );
    assert_eq!(repo.head().unwrap().target(), Some(stopped_at));
    assert!(repo.path().join("rebase-merge").is_dir());
    assert_eq!(terminal_rebase_info(&repo), Some(rebase));
}
