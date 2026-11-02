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

/// main: base (f, g = "base"), then "main change" (f = "m"). topic from base: A (g = "topic"),
/// merged with side (S: g = "side") as M (g = "merged"). `git rebase --rebase-merges main` on
/// topic replays S and A, then the merge, which stops on its conflict in g. git writes the
/// merge step's own MERGE_HEAD (S rebased), MERGE_MODE and MERGE_MSG.
fn rebase_merges_stop() -> (TempDir, Repository) {
    let (dir, repo) = common::init_repo();
    repo.set_head("refs/heads/main").unwrap();
    let path = dir.path();
    common::write_file(path, "f", "base\n");
    common::write_file(path, "g", "base\n");
    git_ok(path, &["add", "."]);
    git_ok(path, &["commit", "-qm", "base"]);
    git_ok(path, &["checkout", "-qb", "side"]);
    common::write_file(path, "g", "side\n");
    git_ok(path, &["commit", "-qam", "S"]);
    git_ok(path, &["checkout", "-q", "main"]);
    git_ok(path, &["checkout", "-qb", "topic"]);
    common::write_file(path, "g", "topic\n");
    git_ok(path, &["commit", "-qam", "A"]);
    assert!(!git(path, &["merge", "side"]).status.success());
    common::write_file(path, "g", "merged\n");
    git_ok(path, &["add", "g"]);
    git_ok(path, &["commit", "-qm", "M"]);
    git_ok(path, &["checkout", "-q", "main"]);
    common::write_file(path, "f", "m\n");
    git_ok(path, &["commit", "-qam", "main change"]);
    git_ok(path, &["checkout", "-q", "topic"]);
    let output = git(path, &["rebase", "--rebase-merges", "main"]);
    assert!(
        !output.status.success(),
        "the merge step must conflict: {output:?}"
    );
    let repo = reopen(&repo);
    assert!(repo.path().join("rebase-merge").is_dir());
    assert!(repo.path().join("MERGE_HEAD").exists());
    (dir, repo)
}

/// t2 from base: "same" (f = "main\n" by another author), which main already has. `git rebase
/// -i --reapply-cherry-picks main` stops on it, now empty, with git's CHERRY_PICK_HEAD and
/// MERGE_MSG.
fn empty_pick_stop() -> (TempDir, Repository) {
    let (dir, repo, _ids) = fixture();
    let path = dir.path();
    git_ok(path, &["checkout", "-q", "main"]);
    git_ok(path, &["checkout", "-qb", "t2", "main~1"]);
    common::write_file(path, "f", "main\n");
    git_ok(
        path,
        &[
            "-c",
            "user.name=Other Author",
            "-c",
            "user.email=other@example.com",
            "commit",
            "-qam",
            "same",
        ],
    );
    let output = test_support::git_command(path)
        .env("GIT_SEQUENCE_EDITOR", "true")
        .args(["rebase", "-i", "--reapply-cherry-picks", "main"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "the pick must stop: {output:?}");
    let repo = reopen(&repo);
    assert!(repo.path().join("rebase-merge").is_dir());
    assert!(repo.path().join("CHERRY_PICK_HEAD").exists());
    (dir, repo)
}

/// The files of a paused operation in the git folder, and the rebase's own folder, by name.
fn state_files(repo: &Repository) -> (Vec<String>, Vec<String>) {
    let names = |dir: &Path| {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    };
    let operation = names(repo.path())
        .into_iter()
        .filter(|name| {
            [
                "MERGE_HEAD",
                "MERGE_MODE",
                "MERGE_MSG",
                "AUTO_MERGE",
                "CHERRY_PICK_HEAD",
                "REVERT_HEAD",
                "REBASE_HEAD",
            ]
            .contains(&name.as_str())
        })
        .collect();
    (operation, names(&repo.path().join("rebase-merge")))
}

/// #263 review: git writes MERGE_HEAD for a `--rebase-merges` merge step and CHERRY_PICK_HEAD
/// for an empty pick; both are the rebase's own step. Abort merge refuses them and changes
/// nothing (aborting the merge step would make `git rebase --continue` drop the side branch).
#[test]
fn abort_merge_refuses_the_rebases_own_merge_or_pick_step() {
    for (dir, repo) in [rebase_merges_stop(), empty_pick_stop()] {
        let before = state_files(&repo);
        let head = repo.head().unwrap().target();

        let error = git_core::merge::abort_merge(&repo).unwrap_err();

        assert_eq!(error.to_string(), REBASE_REFUSAL);
        assert_eq!(state_files(&repo), before);
        assert_eq!(repo.head().unwrap().target(), head);
        drop(dir);
    }
    let (_dir, repo) = rebase_merges_stop();
    assert!(reopen(&repo).index().unwrap().has_conflicts());
}

/// A merge started by hand during a terminal rebase can't be told from the rebase's own merge
/// step (both are just MERGE_HEAD), so Abort merge refuses it too; `git merge --abort` in the
/// terminal ends it.
#[test]
fn abort_merge_refuses_a_merge_started_during_a_terminal_rebase() {
    let (dir, repo, ids) = fixture();
    let output = git(
        dir.path(),
        &["rebase", "-f", "-x", "false", &ids.t1.to_string()],
    );
    assert!(!output.status.success(), "the exec must stop: {output:?}");
    let output = git(dir.path(), &["merge", "main"]);
    assert!(
        !output.status.success(),
        "the merge must conflict: {output:?}"
    );
    let repo = reopen(&repo);
    let before = state_files(&repo);

    let error = git_core::merge::abort_merge(&repo).unwrap_err();

    assert_eq!(error.to_string(), REBASE_REFUSAL);
    assert_eq!(state_files(&repo), before);
    assert!(reopen(&repo).index().unwrap().has_conflicts());
}

/// Starting a merge during the rebase's merge step is refused, naming the rebase.
#[test]
fn start_merge_refuses_during_a_terminal_rebases_merge_step() {
    let (dir, repo) = rebase_merges_stop();
    // Resolved, so nothing is conflicted any more.
    common::write_file(dir.path(), "g", "merged\n");
    git_ok(dir.path(), &["add", "g"]);
    let repo = reopen(&repo);

    let error =
        git_core::merge::start_merge(&repo, "side", git_core::merge::MergeOptions::default())
            .unwrap_err();

    assert_eq!(error.to_string(), REBASE_REFUSAL);
    assert!(repo.path().join("MERGE_HEAD").exists());
}

/// What `rebase --continue` made of topic: the tip's parent count, its tree, and the summaries
/// of its parents.
fn rebased_topic(repo: &Repository) -> (usize, Oid, Vec<String>) {
    let tip = repo
        .find_reference("refs/heads/topic")
        .unwrap()
        .peel_to_commit()
        .unwrap();
    let parents = tip
        .parents()
        .map(|parent| {
            parent
                .summary()
                .ok()
                .flatten()
                .unwrap_or_default()
                .to_string()
        })
        .collect();
    (tip.parent_count(), tip.tree_id(), parents)
}

/// #263 review: committing the resolved merge step in Browsitory does what `git commit` does
/// there: a merge commit with MERGE_HEAD as second parent, the merge's state cleared, so
/// `git rebase --continue` keeps the side branch. Twinned with the git CLI.
#[test]
fn committing_a_terminal_rebases_merge_step_makes_the_merge_like_git_commit() {
    let (ours_dir, ours) = rebase_merges_stop();
    let (twin_dir, twin) = rebase_merges_stop();
    for dir in [ours_dir.path(), twin_dir.path()] {
        common::write_file(dir, "g", "merged\n");
        git_ok(dir, &["add", "g"]);
    }
    let message = std::fs::read_to_string(ours.path().join("MERGE_MSG")).unwrap();
    let mut ours = reopen(&ours);

    git_core::commit::commit_with(
        &mut ours,
        &message,
        git_core::commit::CommitHookSet::Commit,
        HookOptions::default(),
    )
    .unwrap();
    git_ok(twin_dir.path(), &["commit", "--no-edit"]);

    let head = ours.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_count(), 2);
    let twin = reopen(&twin);
    assert_eq!(
        twin.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .parent_count(),
        2
    );
    assert_eq!(state_files(&ours), state_files(&twin));

    for dir in [ours_dir.path(), twin_dir.path()] {
        let output = git(dir, &["-c", "core.editor=true", "rebase", "--continue"]);
        assert!(output.status.success(), "{output:?}");
    }
    let ours_topic = rebased_topic(&reopen(&ours));
    assert_eq!(ours_topic.0, 2, "the side branch is kept");
    assert_eq!(ours_topic.2, vec!["A".to_string(), "S".to_string()]);
    assert_eq!(ours_topic, rebased_topic(&reopen(&twin)));
}

/// #263 review: committing at an empty pick's stop keeps the picked commit's author, as
/// `git commit` does with CHERRY_PICK_HEAD, and clears the pick's state like it. Twinned.
#[test]
fn committing_a_terminal_rebases_empty_pick_keeps_its_author_like_git_commit() {
    let (ours_dir, ours) = empty_pick_stop();
    let (twin_dir, twin) = empty_pick_stop();
    let author = |commit: &git2::Commit<'_>| {
        let author = commit.author();
        (
            author.name().unwrap().to_string(),
            author.email().unwrap().to_string(),
            author.when().seconds(),
        )
    };
    let picked = |repo: &Repository| {
        author(
            &repo
                .find_reference("CHERRY_PICK_HEAD")
                .unwrap()
                .peel_to_commit()
                .unwrap(),
        )
    };
    let (ours_picked, twin_picked) = (picked(&ours), picked(&twin));
    let message = std::fs::read_to_string(ours.path().join("MERGE_MSG")).unwrap();
    let mut ours = reopen(&ours);

    git_core::commit::commit_with(
        &mut ours,
        &message,
        git_core::commit::CommitHookSet::Commit,
        HookOptions::default(),
    )
    .unwrap();
    git_ok(twin_dir.path(), &["commit", "--allow-empty", "--no-edit"]);

    let twin = reopen(&twin);
    let head_author = |repo: &Repository| author(&repo.head().unwrap().peel_to_commit().unwrap());
    assert_eq!(twin_picked.0, "Other Author");
    assert_eq!(
        head_author(&twin),
        twin_picked,
        "git keeps the picked author"
    );
    assert_eq!(head_author(&ours), ours_picked);
    assert_eq!(state_files(&ours), state_files(&twin));
    drop((ours_dir, twin_dir));
}

/// #263 review: git won't delete, rename or add a worktree for a branch being rebased, from any
/// worktree of the repository. Browsitory refuses too (HEAD is detached, so libgit2's own
/// checked-out check doesn't see it), naming the rebase.
#[test]
fn the_branch_being_rebased_cant_be_deleted_renamed_or_checked_out_in_a_new_worktree() {
    let (dir, repo, ids) = terminal_rebase();
    let elsewhere = TempDir::new().unwrap();
    let linked_path = elsewhere.path().join("linked");
    git_ok(
        dir.path(),
        &[
            "worktree",
            "add",
            "-q",
            linked_path.to_str().unwrap(),
            "main",
        ],
    );
    let linked = Repository::open(&linked_path).unwrap();
    let refusal = "topic is being rebased by a rebase started in a terminal. Continue it there \
                   with git rebase --continue, or abort it with git rebase --abort.";
    assert!(!git(dir.path(), &["branch", "-D", "topic"]).status.success());
    assert!(!git(&linked_path, &["branch", "-D", "topic"])
        .status
        .success());

    for (i, handle) in [&repo, &linked].into_iter().enumerate() {
        assert_eq!(
            git_core::branch::delete_branch(handle, "topic", true)
                .unwrap_err()
                .to_string(),
            refusal
        );
        assert_eq!(
            git_core::branch::rename_branch(handle, "topic", "renamed")
                .unwrap_err()
                .to_string(),
            refusal
        );
        let path = elsewhere.path().join(format!("new{i}"));
        assert_eq!(
            git_core::worktree::create_worktree(handle, &format!("new{i}"), &path, "topic", None)
                .unwrap_err()
                .to_string(),
            refusal
        );
        assert!(!path.exists());
    }
    let topic = repo.find_reference("refs/heads/topic").unwrap();
    assert_eq!(topic.target(), Some(ids.topic));
    // Other branches stay manageable.
    repo.branch("spare", &repo.find_commit(ids.main).unwrap(), false)
        .unwrap();
    git_core::branch::rename_branch(&repo, "spare", "spare2").unwrap();
    git_core::branch::delete_branch(&linked, "spare2", true).unwrap();
}

/// #263 re-review: another worktree can't switch to the branch a terminal rebase is rebasing
/// (git: "'topic' is already used by worktree at …"). A commit there would be lost when the
/// rebase is aborted (it resets topic) or finished.
#[test]
fn another_worktree_cant_switch_to_the_branch_being_rebased() {
    let (dir, repo, ids) = terminal_rebase();
    let elsewhere = TempDir::new().unwrap();
    let linked_path = elsewhere.path().join("linked");
    git_ok(
        dir.path(),
        &[
            "worktree",
            "add",
            "-q",
            linked_path.to_str().unwrap(),
            "main",
        ],
    );
    let linked = Repository::open(&linked_path).unwrap();
    assert!(!git(&linked_path, &["switch", "topic"]).status.success());

    let error = git_core::branch::switch_branch(&linked, "topic").unwrap_err();

    assert_eq!(
        error.to_string(),
        "topic is being rebased by a rebase started in a terminal. Continue it there with git \
         rebase --continue, or abort it with git rebase --abort."
    );
    assert_eq!(linked.head().unwrap().name().ok(), Some("refs/heads/main"));
    assert_eq!(
        repo.find_reference("refs/heads/topic").unwrap().target(),
        Some(ids.topic)
    );
}

/// #263 re-review: Undo doesn't put another worktree's HEAD back on the branch a terminal
/// rebase is rebasing, as it doesn't for a branch checked out elsewhere.
#[test]
fn undo_doesnt_check_out_the_branch_being_rebased_in_another_worktree() {
    let (dir, repo, _ids) = fixture();
    let elsewhere = TempDir::new().unwrap();
    let linked_path = elsewhere.path().join("linked");
    git_ok(dir.path(), &["checkout", "-q", "main"]);
    git_ok(
        dir.path(),
        &[
            "worktree",
            "add",
            "-q",
            linked_path.to_str().unwrap(),
            "topic",
        ],
    );
    let linked = Repository::open(&linked_path).unwrap();
    let on_topic = git_core::undo::capture(&linked, git_core::undo::SnapshotScope::Full).unwrap();
    git_ok(&linked_path, &["checkout", "-q", "-b", "spare"]);
    let linked = Repository::open(&linked_path).unwrap();
    let on_spare = git_core::undo::capture(&linked, git_core::undo::SnapshotScope::Full).unwrap();
    git_ok(dir.path(), &["checkout", "-q", "topic"]);
    assert!(!git(dir.path(), &["rebase", "main"]).status.success());
    drop(repo);

    let error = git_core::undo::restore(&linked, &on_spare, &on_topic, "undo").unwrap_err();

    assert!(
        matches!(error, git_core::undo::UndoError::RepositoryChanged),
        "{error}"
    );
    assert_eq!(linked.head().unwrap().name().ok(), Some("refs/heads/spare"));
}

/// #263 re-review: a commit concluding a revert started during a terminal rebase clears
/// REVERT_HEAD, as `git commit` does. Twinned with the git CLI.
#[test]
fn committing_a_revert_during_a_terminal_rebase_clears_it_like_git_commit() {
    let setup = || {
        let (dir, repo, ids) = fixture();
        let path = dir.path();
        let output = git(path, &["rebase", "-f", "-x", "false", &ids.t1.to_string()]);
        assert!(!output.status.success(), "the exec must stop: {output:?}");
        common::write_file(path, "f", "changed\n");
        git_ok(path, &["commit", "-qam", "changed"]);
        let output = git(path, &["revert", "--no-edit", &ids.t1.to_string()]);
        assert!(
            !output.status.success(),
            "the revert must conflict: {output:?}"
        );
        common::write_file(path, "f", "resolved\n");
        git_ok(path, &["add", "f"]);
        let repo = reopen(&repo);
        assert!(repo.path().join("REVERT_HEAD").exists());
        (dir, repo)
    };
    let (ours_dir, ours) = setup();
    let (twin_dir, twin) = setup();
    let message = std::fs::read_to_string(ours.path().join("MERGE_MSG")).unwrap();
    let mut ours = reopen(&ours);

    git_core::commit::commit_with(
        &mut ours,
        &message,
        git_core::commit::CommitHookSet::Commit,
        HookOptions::default(),
    )
    .unwrap();
    git_ok(twin_dir.path(), &["commit", "--no-edit"]);

    assert!(!ours.path().join("REVERT_HEAD").exists());
    assert_eq!(state_files(&ours), state_files(&reopen(&twin)));
    drop(ours_dir);
}
