// crates/git-core/tests/pick_sequence.rs
mod common;

use git2::{Oid, Repository, RepositoryState};
use git_core::hooks::HookOptions;
use git_core::merge::{resolve_whole_file, ConflictSide};
use git_core::pick::{
    abort_sequence, cherry_pick, cherry_pick_many, continue_sequence, revert_many, PickError,
    SequenceKind, SequenceOutcome, SequenceState,
};
use std::path::Path;

fn base_branch_ref(repo: &Repository) -> String {
    for name in ["main", "master"] {
        if let Ok(branch) = repo.find_branch(name, git2::BranchType::Local) {
            return branch.get().name().unwrap().to_string();
        }
    }
    // A global `init.defaultBranch` may name it anything: take the only other local branch.
    let head = repo.head().unwrap();
    head.name().unwrap().to_string()
}

fn head(repo: &Repository) -> String {
    repo.head().unwrap().target().unwrap().to_string()
}

/// Summaries of the last `n` first-parent commits, newest first.
fn summaries(repo: &Repository, n: usize) -> Vec<String> {
    let mut commit = repo.head().unwrap().peel_to_commit().unwrap();
    let mut out = vec![commit.summary().unwrap().unwrap().to_string()];
    while out.len() < n {
        commit = commit.parent(0).unwrap();
        out.push(commit.summary().unwrap().unwrap().to_string());
    }
    out
}

fn stage(repo: &Repository, path: &str) {
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();
}

/// base: shared.txt="base\n", notes.txt. side (from base): "side one" (+one.txt),
/// "side two" (shared.txt="theirs\n", plus two.txt when `two_adds_a_file`), "side three"
/// (+three.txt). The base branch (checked out) then commits "ours edit" (shared.txt="ours\n")
/// when `conflict`, else "main only" (+main.txt). Returns the side ids oldest-first.
fn side_branch(
    conflict: bool,
    two_adds_a_file: bool,
) -> (tempfile::TempDir, Repository, Vec<String>) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "shared.txt", "base\n");
    common::write_file(dir.path(), "notes.txt", "notes\n");
    common::commit_all(&repo, "base");
    let base_ref = base_branch_ref(&repo);
    {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("side", &base, false).unwrap();
    }
    repo.set_head("refs/heads/side").unwrap();
    let mut ids = Vec::new();
    let mut two: Vec<(&str, &str)> = vec![("shared.txt", "theirs\n")];
    if two_adds_a_file {
        two.push(("two.txt", "2\n"));
    }
    for (files, message) in [
        (vec![("one.txt", "1\n")], "side one"),
        (two, "side two"),
        (vec![("three.txt", "3\n")], "side three"),
    ] {
        for (path, contents) in files {
            common::write_file(dir.path(), path, contents);
        }
        common::commit_all(&repo, message);
        ids.push(head(&repo));
    }
    repo.set_head(&base_ref).unwrap();
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::new()
            .force()
            .remove_untracked(true),
    ))
    .unwrap();
    if conflict {
        common::write_file(dir.path(), "shared.txt", "ours\n");
        common::commit_all(&repo, "ours edit");
    } else {
        common::write_file(dir.path(), "main.txt", "main\n");
        common::commit_all(&repo, "main only");
    }
    (dir, repo, ids)
}

fn paused(
    conflict_adds_a_file: bool,
) -> (tempfile::TempDir, Repository, Vec<String>, SequenceState) {
    let (dir, repo, ids) = side_branch(true, conflict_adds_a_file);
    let state = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default())
        .state
        .unwrap();
    (dir, repo, ids, state)
}

#[test]
fn a_clean_sequence_applies_every_commit_oldest_first() {
    let (dir, repo, ids) = side_branch(false, true);
    let run = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default());
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 3,
            skipped: vec![]
        }
    );
    assert!(run.state.is_none());
    assert_eq!(
        summaries(&repo, 4),
        ["side three", "side two", "side one", "main only"]
    );
    assert!(dir.path().join("three.txt").exists());
    assert_eq!(repo.state(), RepositoryState::Clean);
}

#[test]
fn a_conflict_in_the_middle_stops_with_state_and_progress() {
    let (_dir, repo, ids) = side_branch(true, true);
    let original = head(&repo);
    let run = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default());
    let SequenceOutcome::Conflicted { files, progress } = run.outcome.unwrap() else {
        panic!("expected a conflict")
    };
    assert_eq!(files, ["shared.txt"]);
    assert_eq!(progress.kind, SequenceKind::CherryPick);
    assert_eq!((progress.current_step, progress.total_steps), (2, 3));
    assert_eq!(progress.current_commit, ids[1]);
    let state = run.state.expect("a stopped sequence returns its state");
    assert_eq!(state.next, 1);
    assert_eq!(state.commits, ids);
    assert_eq!(state.original_head, original);
    assert_eq!(state.expected_head, head(&repo));
    assert_eq!(summaries(&repo, 2), ["side one", "ours edit"]);
    assert_eq!(repo.state(), RepositoryState::CherryPick);
}

#[test]
fn an_already_applied_commit_is_skipped() {
    let (_dir, repo, ids) = side_branch(false, true);
    cherry_pick(
        &repo,
        &ids[0],
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    let run = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default());
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 2,
            skipped: vec![ids[0].clone()]
        }
    );
    assert_eq!(summaries(&repo, 3), ["side three", "side two", "side one"]);
}

#[test]
fn revert_many_applies_newest_first() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "base.txt", "base\n");
    common::commit_all(&repo, "base");
    let mut ids = Vec::new();
    for name in ["a", "b", "c"] {
        common::write_file(dir.path(), &format!("{name}.txt"), "x\n");
        common::commit_all(&repo, name);
        ids.push(head(&repo));
    }
    let run = revert_many(&repo, &ids, git_core::hooks::HookOptions::default());
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 3,
            skipped: vec![]
        }
    );
    assert_eq!(
        summaries(&repo, 3),
        ["Revert \"a\"", "Revert \"b\"", "Revert \"c\""]
    );
    for name in ["a", "b", "c"] {
        assert!(!dir.path().join(format!("{name}.txt")).exists());
    }
}

#[test]
fn a_merge_commit_in_a_sequence_is_refused_before_anything_changes() {
    let (dir, repo, ids) = side_branch(false, true);
    let before = head(&repo);
    let merge = {
        let ours = repo.head().unwrap().peel_to_commit().unwrap();
        let theirs = repo.find_commit(Oid::from_str(&ids[2]).unwrap()).unwrap();
        let mut index = repo.merge_commits(&ours, &theirs, None).unwrap();
        let tree = repo.find_tree(index.write_tree_to(&repo).unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(None, &sig, &sig, "merge", &tree, &[&ours, &theirs])
            .unwrap()
            .to_string()
    };
    let run = cherry_pick_many(
        &repo,
        &[ids[0].clone(), merge],
        git_core::hooks::HookOptions::default(),
    );
    assert!(matches!(run.outcome, Err(PickError::MainlineRequired)));
    assert!(run.state.is_none());
    assert_eq!(head(&repo), before);
    assert!(!dir.path().join("one.txt").exists());
}

#[test]
fn nothing_selected_is_refused() {
    let (_dir, repo, _) = side_branch(false, true);
    let run = cherry_pick_many(&repo, &[], git_core::hooks::HookOptions::default());
    assert!(matches!(run.outcome, Err(PickError::NothingSelected)));
    assert!(run.state.is_none());
    assert_eq!(
        PickError::NothingSelected.to_string(),
        "Select at least one commit."
    );
}

#[test]
fn a_paused_operation_or_staged_change_refuses_a_new_sequence() {
    let (dir, repo, ids) = side_branch(false, true);
    common::write_file(dir.path(), "notes.txt", "staged\n");
    stage(&repo, "notes.txt");
    let run = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default());
    assert!(matches!(run.outcome, Err(PickError::DirtyIndex)));
    assert!(run.state.is_none());

    let (_dir, repo, ids, _state) = paused(true);
    let run = revert_many(&repo, &ids[..1], git_core::hooks::HookOptions::default());
    assert!(matches!(run.outcome, Err(PickError::OperationInProgress)));
    assert!(run.state.is_none());
}

#[test]
fn an_error_on_the_first_step_returns_no_state() {
    let (dir, repo, ids) = side_branch(false, true);
    let before = head(&repo);
    // An untracked file in the way of the first pick's new file: libgit2's SAFE checkout refuses.
    common::write_file(dir.path(), "one.txt", "mine\n");
    let run = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default());
    assert!(run.outcome.is_err());
    assert!(run.state.is_none());
    assert_eq!(head(&repo), before);
    assert_eq!(repo.state(), RepositoryState::Clean);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("one.txt")).unwrap(),
        "mine\n"
    );
}

#[test]
fn an_error_on_a_later_step_returns_the_state_at_that_step() {
    let (dir, repo, ids) = side_branch(false, true);
    common::write_file(dir.path(), "three.txt", "mine\n");
    let run = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default());
    assert!(run.outcome.is_err());
    let state = run.state.expect("later failure keeps the sequence");
    assert_eq!(state.next, 2);
    assert_eq!(state.expected_head, head(&repo));
    assert_eq!(summaries(&repo, 2), ["side two", "side one"]);
    assert_eq!(repo.state(), RepositoryState::Clean);
}

#[test]
fn continue_after_resolving_lands_the_rest_with_original_authors() {
    let (dir, repo, ids, state) = paused(true);
    common::write_file(dir.path(), "shared.txt", "resolved\n");
    stage(&repo, "shared.txt");
    let run = continue_sequence(&repo, state, HookOptions::default());
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 3,
            skipped: vec![]
        }
    );
    assert!(run.state.is_none());
    assert_eq!(
        summaries(&repo, 4),
        ["side three", "side two", "side one", "ours edit"]
    );
    let two = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .parent(0)
        .unwrap();
    let original = repo.find_commit(Oid::from_str(&ids[1]).unwrap()).unwrap();
    assert_eq!(two.author().when(), original.author().when());
    // git's message cleanup ends it with a newline.
    assert_eq!(
        two.message().unwrap().trim_end(),
        original.message().unwrap().trim_end()
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
        "resolved\n"
    );
    assert!(dir.path().join("two.txt").exists());
    assert_eq!(repo.state(), RepositoryState::Clean);
}

#[test]
fn continue_with_conflicts_left_is_refused_and_keeps_the_state() {
    let (_dir, repo, _ids, state) = paused(true);
    let run = continue_sequence(&repo, state.clone(), HookOptions::default());
    assert!(matches!(run.outcome, Err(PickError::UnresolvedConflicts)));
    assert_eq!(run.state, Some(state));
    assert_eq!(repo.state(), RepositoryState::CherryPick);
    assert_eq!(
        PickError::UnresolvedConflicts.to_string(),
        "Resolve all conflicts before continuing."
    );
}

#[test]
fn whole_file_ours_empties_the_step_and_it_is_skipped() {
    let (_dir, repo, ids, state) = paused(false);
    resolve_whole_file(&repo, "shared.txt", ConflictSide::Ours).unwrap();
    let run = continue_sequence(&repo, state, HookOptions::default());
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 2,
            skipped: vec![ids[1].clone()]
        }
    );
    assert_eq!(summaries(&repo, 3), ["side three", "side one", "ours edit"]);
    assert_eq!(repo.state(), RepositoryState::Clean);
}

#[test]
fn a_revert_sequence_paused_on_a_conflict_continues_with_gits_message() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "one\n");
    common::commit_all(&repo, "one");
    common::write_file(dir.path(), "a.txt", "two\n");
    common::commit_all(&repo, "two");
    let two = head(&repo);
    common::write_file(dir.path(), "b.txt", "b\n");
    common::commit_all(&repo, "add b");
    let add_b = head(&repo);
    common::write_file(dir.path(), "a.txt", "three\n");
    common::commit_all(&repo, "three");
    // Newest-first: "add b" reverts cleanly, then "two" conflicts with "three".
    let run = revert_many(
        &repo,
        &[two.clone(), add_b.clone()],
        git_core::hooks::HookOptions::default(),
    );
    let Ok(SequenceOutcome::Conflicted { progress, .. }) = run.outcome else {
        panic!("expected a conflict, got {:?}", run.outcome)
    };
    assert_eq!(progress.kind, SequenceKind::Revert);
    assert_eq!((progress.current_step, progress.total_steps), (2, 2));
    assert_eq!(repo.state(), RepositoryState::Revert);
    common::write_file(dir.path(), "a.txt", "resolved\n");
    stage(&repo, "a.txt");
    let run = continue_sequence(&repo, run.state.unwrap(), HookOptions::default());
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 2,
            skipped: vec![]
        }
    );
    let tip = repo.head().unwrap().peel_to_commit().unwrap();
    assert!(
        tip.message().unwrap().starts_with("Revert \"two\""),
        "{:?}",
        tip.message()
    );
    assert!(!dir.path().join("b.txt").exists());
}

#[test]
fn continue_retries_a_step_that_failed_without_a_conflict() {
    let (dir, repo, ids) = side_branch(false, true);
    common::write_file(dir.path(), "three.txt", "mine\n");
    let state = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default())
        .state
        .unwrap();
    std::fs::remove_file(dir.path().join("three.txt")).unwrap();
    let run = continue_sequence(&repo, state, HookOptions::default());
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 3,
            skipped: vec![]
        }
    );
}

#[test]
fn continue_after_the_step_was_aborted_outside_retries_it() {
    // `git cherry-pick --abort` in a terminal clears the paused step but not Browsitory's
    // sequence: Continue re-runs that step, which conflicts again.
    let (_dir, repo, ids, state) = paused(true);
    git_core::merge::abort_merge(&repo).unwrap();
    let run = continue_sequence(&repo, state, HookOptions::default());
    let Ok(SequenceOutcome::Conflicted { progress, .. }) = run.outcome else {
        panic!("expected the step to conflict again")
    };
    assert_eq!(progress.current_commit, ids[1]);
    assert_eq!(run.state.unwrap().next, 1);
}

#[test]
fn continue_refuses_staged_changes_before_retrying_a_step() {
    let (dir, repo, ids) = side_branch(false, true);
    common::write_file(dir.path(), "three.txt", "mine\n");
    let state = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default())
        .state
        .unwrap();
    std::fs::remove_file(dir.path().join("three.txt")).unwrap();
    common::write_file(dir.path(), "notes.txt", "staged\n");
    stage(&repo, "notes.txt");
    let run = continue_sequence(&repo, state.clone(), HookOptions::default());
    assert!(matches!(run.outcome, Err(PickError::DirtyIndex)));
    assert_eq!(run.state, Some(state));
}

#[test]
fn continue_counts_a_step_the_user_committed_elsewhere() {
    let (dir, mut repo, _ids, state) = paused(true);
    common::write_file(dir.path(), "shared.txt", "resolved\n");
    stage(&repo, "shared.txt");
    git_core::commit::commit(
        &mut repo,
        "side two",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    let run = continue_sequence(&repo, state, HookOptions::default());
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 3,
            skipped: vec![]
        }
    );
    assert_eq!(summaries(&repo, 3), ["side three", "side two", "side one"]);
}

#[test]
fn continue_refuses_a_different_paused_pick() {
    let (dir, repo, ids, state) = paused(true);
    git_core::merge::abort_merge(&repo).unwrap();
    // Someone starts another, unrelated pick from a terminal that conflicts too.
    let other = {
        let base = repo
            .find_commit(Oid::from_str(&ids[0]).unwrap())
            .unwrap()
            .parent(0)
            .unwrap();
        let sig = repo.signature().unwrap();
        let mut builder = repo.treebuilder(Some(&base.tree().unwrap())).unwrap();
        let blob = repo.blob(b"other\n").unwrap();
        builder.insert("shared.txt", blob, 0o100644).unwrap();
        let tree = repo.find_tree(builder.write().unwrap()).unwrap();
        repo.commit(None, &sig, &sig, "other", &tree, &[&base])
            .unwrap()
            .to_string()
    };
    assert!(matches!(
        cherry_pick(&repo, &other, None, git_core::hooks::HookOptions::default()).unwrap(),
        git_core::pick::PickOutcome::Conflicted { .. }
    ));
    let run = continue_sequence(&repo, state.clone(), HookOptions::default());
    assert!(matches!(run.outcome, Err(PickError::OperationInProgress)));
    assert_eq!(run.state, Some(state.clone()));

    // Abort leaves the unrelated pick alone and does not rewind under it.
    let landed = head(&repo);
    let abort = abort_sequence(&repo, &state).unwrap();
    assert!(!abort.rewound);
    assert_eq!(repo.state(), RepositoryState::CherryPick);
    assert_eq!(head(&repo), landed);
    let _ = dir;
}

#[test]
fn abort_restores_the_original_head_and_keeps_unrelated_edits() {
    let (dir, repo, ids) = side_branch(true, true);
    let original = head(&repo);
    let branch = repo.head().unwrap().shorthand().unwrap().to_string();
    common::write_file(dir.path(), "notes.txt", "local edit\n");
    let state = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default())
        .state
        .unwrap();

    let abort = abort_sequence(&repo, &state).unwrap();
    assert!(abort.rewound);
    assert_eq!(head(&repo), original);
    assert_eq!(repo.head().unwrap().shorthand().unwrap(), branch);
    assert_eq!(repo.state(), RepositoryState::Clean);
    assert!(!repo.index().unwrap().has_conflicts());
    assert!(!dir.path().join("one.txt").exists());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
        "ours\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("notes.txt")).unwrap(),
        "local edit\n"
    );
    let statuses = repo.statuses(None).unwrap();
    let changed: Vec<String> = statuses
        .iter()
        .map(|entry| entry.path().unwrap().to_string())
        .collect();
    assert_eq!(changed, ["notes.txt"]);
}

#[test]
fn abort_on_a_detached_head_moves_the_detached_head_back() {
    let (_dir, repo, ids) = side_branch(true, true);
    let original = head(&repo);
    repo.set_head_detached(Oid::from_str(&original).unwrap())
        .unwrap();
    let state = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default())
        .state
        .unwrap();
    let abort = abort_sequence(&repo, &state).unwrap();
    assert!(abort.rewound);
    assert!(repo.head_detached().unwrap());
    assert_eq!(head(&repo), original);
}

#[test]
fn abort_after_head_moved_does_not_rewind_and_continue_refuses() {
    let (dir, repo, _ids, state) = paused(true);
    git_core::merge::abort_merge(&repo).unwrap();
    // Two commits on top of the stop point: neither "still there" nor "one step further".
    for name in ["external one", "external two"] {
        common::write_file(dir.path(), &format!("{name}.txt"), "x\n");
        common::commit_all(&repo, name);
    }
    let moved_to = head(&repo);

    let run = continue_sequence(&repo, state.clone(), HookOptions::default());
    assert!(matches!(run.outcome, Err(PickError::HeadMoved)));
    assert_eq!(run.state, Some(state.clone()));
    assert_eq!(
        PickError::HeadMoved.to_string(),
        "HEAD moved since the cherry-pick paused. Abort it and start again."
    );

    let abort = abort_sequence(&repo, &state).unwrap();
    assert!(!abort.rewound);
    assert_eq!(head(&repo), moved_to);
}

#[test]
fn abort_after_an_error_stop_rewinds_the_landed_steps() {
    let (dir, repo, ids) = side_branch(false, true);
    let original = head(&repo);
    common::write_file(dir.path(), "three.txt", "mine\n");
    let state = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default())
        .state
        .unwrap();
    let abort = abort_sequence(&repo, &state).unwrap();
    assert!(abort.rewound);
    assert_eq!(head(&repo), original);
    // The untracked file that blocked step three is not ours to delete.
    assert_eq!(
        std::fs::read_to_string(dir.path().join("three.txt")).unwrap(),
        "mine\n"
    );
}

#[test]
fn abort_refuses_to_overwrite_a_local_edit_and_can_be_retried() {
    let (dir, repo, ids) = side_branch(false, true);
    let original = head(&repo);
    common::write_file(dir.path(), "three.txt", "mine\n");
    let state = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default())
        .state
        .unwrap();
    // A local edit to a file a landed step changed: rewinding would overwrite it.
    common::write_file(dir.path(), "one.txt", "edited\n");
    assert!(abort_sequence(&repo, &state).is_err());
    assert_eq!(head(&repo), state.expected_head);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("one.txt")).unwrap(),
        "edited\n"
    );
    std::fs::write(dir.path().join("one.txt"), "1\n").unwrap();
    assert!(abort_sequence(&repo, &state).unwrap().rewound);
    assert_eq!(head(&repo), original);
}

#[test]
fn head_info_from_git_core_never_reports_a_sequence() {
    let (_dir, repo, _ids, _state) = paused(true);
    assert_eq!(git_core::repo::head_info(&repo).unwrap().sequence, None);
}

// --- Review fixes (PR #132) ---

/// Commits whatever is in the work tree with an author distinct from every fixture commit.
fn commit_as_someone_else(repo: &Repository, message: &str) {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = git2::Signature::new(
        "Someone Else",
        "else@example.com",
        &git2::Time::new(1000, 0),
    )
    .unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&parent])
        .unwrap();
}

#[test]
fn an_unrelated_commit_after_a_terminal_abort_is_not_taken_for_the_step() {
    let (dir, repo, _ids, state) = paused(true);
    git_core::merge::abort_merge(&repo).unwrap(); // `git cherry-pick --abort`
    common::write_file(dir.path(), "unrelated.txt", "x\n");
    commit_as_someone_else(&repo, "unrelated");
    let run = continue_sequence(&repo, state.clone(), HookOptions::default());
    assert!(
        matches!(run.outcome, Err(PickError::HeadMoved)),
        "{:?}",
        run.outcome
    );
    assert_eq!(run.state, Some(state));
}

#[test]
fn after_an_error_stop_any_new_commit_is_head_moved() {
    let (dir, repo, ids) = side_branch(false, true);
    common::write_file(dir.path(), "three.txt", "mine\n");
    let state = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default())
        .state
        .unwrap();
    assert!(!state.stopped_on_conflict);
    std::fs::remove_file(dir.path().join("three.txt")).unwrap();
    common::write_file(dir.path(), "notes.txt", "user work\n");
    commit_as_someone_else(&repo, "user work");
    let run = continue_sequence(&repo, state, HookOptions::default());
    assert!(matches!(run.outcome, Err(PickError::HeadMoved)));
}

#[test]
fn a_revert_step_committed_by_hand_is_recognised_by_its_message() {
    let (dir, mut repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "one\n");
    common::commit_all(&repo, "one");
    common::write_file(dir.path(), "a.txt", "two\n");
    common::commit_all(&repo, "two");
    let two = head(&repo);
    common::write_file(dir.path(), "b.txt", "b\n");
    common::commit_all(&repo, "add b");
    let add_b = head(&repo);
    common::write_file(dir.path(), "a.txt", "three\n");
    common::commit_all(&repo, "three");
    let run = revert_many(
        &repo,
        &[two, add_b],
        git_core::hooks::HookOptions::default(),
    );
    let state = run.state.unwrap();
    assert!(state.stopped_on_conflict);
    common::write_file(dir.path(), "a.txt", "resolved\n");
    stage(&repo, "a.txt");
    // Finished by hand: with git's message (it names the reverted commit) it counts as the
    // step; with a message of the user's own it doesn't.
    let git_message = git_core::merge::merge_message(&repo).unwrap();
    let before = head(&repo);
    git_core::commit::commit(
        &mut repo,
        "my own words",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    let run = continue_sequence(&repo, state.clone(), HookOptions::default());
    assert!(matches!(run.outcome, Err(PickError::HeadMoved)));

    // Same stop, finished with git's message instead.
    repo.reset(
        repo.find_object(Oid::from_str(&before).unwrap(), None)
            .as_ref()
            .unwrap(),
        git2::ResetType::Soft,
        None,
    )
    .unwrap();
    let mut index = repo.index().unwrap();
    index.write().unwrap();
    git_core::commit::commit(
        &mut repo,
        &git_message,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    let run = continue_sequence(&repo, state, HookOptions::default());
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 2,
            skipped: vec![]
        }
    );
}

#[test]
fn continue_reads_an_index_staged_by_another_process() {
    let (dir, repo, _ids, state) = paused(true);
    // The index is loaded in `repo`; resolve and stage through a second handle, as a terminal
    // `git add` would.
    let other = Repository::open(dir.path()).unwrap();
    common::write_file(dir.path(), "shared.txt", "resolved\n");
    stage(&other, "shared.txt");
    let run = continue_sequence(&repo, state, HookOptions::default());
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 3,
            skipped: vec![]
        }
    );
}

#[test]
fn abort_does_not_rewind_a_branch_switched_to_after_the_stop() {
    let (_dir, repo, ids) = side_branch(true, true);
    let original = head(&repo);
    let base_ref = base_branch_ref(&repo);
    let state = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default())
        .state
        .unwrap();
    git_core::merge::abort_merge(&repo).unwrap();
    // `git switch -c other`: same commit, different branch.
    {
        let tip = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("other", &tip, false).unwrap();
    }
    repo.set_head("refs/heads/other").unwrap();
    let landed = head(&repo);

    let abort = abort_sequence(&repo, &state).unwrap();
    assert!(!abort.rewound);
    assert_eq!(head(&repo), landed);
    assert_eq!(
        repo.find_reference(&base_ref)
            .unwrap()
            .target()
            .unwrap()
            .to_string(),
        landed
    );
    assert_ne!(landed, original);
}

#[test]
fn abort_cleans_up_a_conflict_whose_pick_head_was_deleted() {
    let (dir, repo, _ids, state) = paused(true);
    let original = state.original_head.clone();
    std::fs::remove_file(repo.path().join("CHERRY_PICK_HEAD")).unwrap();
    assert_eq!(repo.state(), RepositoryState::Clean);
    assert!(repo.index().unwrap().has_conflicts());

    let run = continue_sequence(&repo, state.clone(), HookOptions::default());
    assert!(matches!(run.outcome, Err(PickError::UnresolvedConflicts)));

    let abort = abort_sequence(&repo, &state).unwrap();
    assert!(abort.rewound);
    assert_eq!(head(&repo), original);
    assert!(!repo.index().unwrap().has_conflicts());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
        "ours\n"
    );
}

#[test]
fn reverting_a_revert_reapplies_like_git() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "add a");
    let add_a = head(&repo);
    git_core::pick::revert(&repo, &add_a, None, git_core::hooks::HookOptions::default()).unwrap();
    let revert = head(&repo);
    git_core::pick::revert(
        &repo,
        &revert,
        None,
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    let tip = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        tip.message().unwrap(),
        format!("Reapply \"add a\"\n\nThis reverts commit {revert}.\n")
    );
    // In a sequence too; and a revert of a "Reapply" is a plain Revert again.
    let reapply = head(&repo);
    let run = revert_many(&repo, &[reapply], git_core::hooks::HookOptions::default());
    assert!(run.outcome.is_ok());
    let tip = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        tip.summary().unwrap().unwrap(),
        "Revert \"Reapply \"add a\"\""
    );
}

#[test]
fn reverting_a_revert_of_a_revert_stays_a_revert() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "a\n");
    common::commit_all(&repo, "Revert \"Revert \"x\"\"");
    let id = head(&repo);
    git_core::pick::revert(&repo, &id, None, git_core::hooks::HookOptions::default()).unwrap();
    let tip = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        tip.summary().unwrap().unwrap(),
        "Revert \"Revert \"Revert \"x\"\"\""
    );
}

/// The verifier's repro for sequences: at the conflict stop the pick is aborted outside and
/// the *next* commit of the sequence (same author and time) is committed instead. That is not
/// the stopped step, so Continue refuses rather than counting it.
#[test]
fn continue_does_not_count_another_commit_with_the_same_author() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "shared.txt", "base\n");
    common::commit_all(&repo, "base");
    let base_ref = base_branch_ref(&repo);
    {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("side", &base, false).unwrap();
    }
    repo.set_head("refs/heads/side").unwrap();
    let author = git2::Signature::new(
        "Same",
        "same@example.com",
        &git2::Time::new(1_700_000_000, 0),
    )
    .unwrap();
    let mut ids = Vec::new();
    for (path, contents, message) in [
        ("shared.txt", "theirs\n", "conflicts"),
        ("other.txt", "o\n", "other"),
    ] {
        common::write_file(dir.path(), path, contents);
        stage(&repo, path);
        let mut index = repo.index().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        ids.push(
            repo.commit(Some("HEAD"), &author, &author, message, &tree, &[&parent])
                .unwrap()
                .to_string(),
        );
    }
    repo.set_head(&base_ref).unwrap();
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::new()
            .force()
            .remove_untracked(true),
    ))
    .unwrap();
    common::write_file(dir.path(), "shared.txt", "ours\n");
    common::commit_all(&repo, "ours edit");
    let state = cherry_pick_many(&repo, &ids, git_core::hooks::HookOptions::default())
        .state
        .unwrap();
    git_core::merge::abort_merge(&repo).unwrap();
    let other = repo
        .find_commit(git2::Oid::from_str(&ids[1]).unwrap())
        .unwrap();
    repo.cherrypick(&other, None).unwrap();
    let mut index = repo.index().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let parent = repo.head().unwrap().peel_to_commit().unwrap();
    repo.commit(Some("HEAD"), &author, &author, "other", &tree, &[&parent])
        .unwrap();
    repo.cleanup_state().unwrap();

    let run = continue_sequence(&repo, state, HookOptions::default());

    assert!(run.outcome.is_err(), "{:?}", run.outcome);
    assert!(run.state.is_some());
}

// ---- commit hooks on Continue (#133) ---------------------------------------------------------

#[cfg(unix)]
const COMMIT_HOOKS: [&str; 4] = [
    "pre-commit",
    "prepare-commit-msg",
    "commit-msg",
    "post-commit",
];

/// `paused(false)` with the conflict resolved and staged.
#[cfg(unix)]
fn resolved() -> (tempfile::TempDir, Repository, Vec<String>, SequenceState) {
    let (dir, repo, ids, state) = paused(false);
    common::write_file(dir.path(), "shared.txt", "resolved\n");
    stage(&repo, "shared.txt");
    (dir, repo, ids, state)
}

#[cfg(unix)]
fn message_of(repo: &Repository, steps_back: usize) -> String {
    let mut commit = repo.head().unwrap().peel_to_commit().unwrap();
    for _ in 0..steps_back {
        commit = commit.parent(0).unwrap();
    }
    commit.message().unwrap().to_string()
}

#[cfg(unix)]
#[test]
fn continue_runs_the_commit_hooks_once_on_the_resolved_step() {
    let (_dir, repo, _ids, state) = resolved();
    common::log_hooks(&repo, &COMMIT_HOOKS);

    let run = continue_sequence(&repo, state, HookOptions::default());

    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 3,
            skipped: vec![]
        }
    );
    // The resolved step gets `git commit`'s hooks; the clean step after it only
    // `prepare-commit-msg … message` and `post-commit` (#183).
    assert_eq!(
        common::read_log(&repo),
        "pre-commit \nprepare-commit-msg <msg> merge\ncommit-msg <msg>\npost-commit \nprepare-commit-msg <msg> message\npost-commit \n"
    );
}

#[cfg(unix)]
#[test]
fn a_commit_msg_hook_rewrites_the_continued_steps_message() {
    let (_dir, repo, _ids, state) = resolved();
    common::install_hook(
        &repo.path().join("hooks"),
        "commit-msg",
        r#"printf '\nChange-Id: I123\n' >> "$1""#,
    );

    let run = continue_sequence(&repo, state, HookOptions::default());

    assert!(matches!(run.outcome, Ok(SequenceOutcome::Completed { .. })));
    assert_eq!(summaries(&repo, 3), ["side three", "side two", "side one"]);
    assert!(!message_of(&repo, 0).contains("Change-Id"));
    assert!(message_of(&repo, 1).trim_end().ends_with("Change-Id: I123"));
    assert!(!message_of(&repo, 2).contains("Change-Id"));
}

#[cfg(unix)]
#[test]
fn a_failing_commit_msg_keeps_the_step_paused_and_the_state_unchanged() {
    let (_dir, repo, _ids, state) = resolved();
    common::install_hook(
        &repo.path().join("hooks"),
        "commit-msg",
        "echo no change id; exit 1",
    );
    let before = state.clone();

    let run = continue_sequence(&repo, state, HookOptions::default());

    let SequenceOutcome::HookFailed {
        hook,
        output,
        progress,
    } = run.outcome.unwrap()
    else {
        panic!("expected HookFailed")
    };
    assert_eq!(hook, "commit-msg");
    assert_eq!(output, "no change id\n");
    assert_eq!(progress.current_step, 2);
    assert_eq!(run.state.as_ref(), Some(&before));
    assert_eq!(repo.state(), RepositoryState::CherryPick);
    assert_eq!(head(&repo), before.expected_head);
    assert!(!repo.index().unwrap().has_conflicts());

    let run = continue_sequence(
        &repo,
        run.state.unwrap(),
        HookOptions {
            no_verify: true,
            ..Default::default()
        },
    );
    assert!(matches!(run.outcome, Ok(SequenceOutcome::Completed { .. })));
}

#[cfg(unix)]
#[test]
fn continue_with_no_verify_skips_pre_commit_and_commit_msg() {
    let (_dir, repo, _ids, state) = resolved();
    let hooks = repo.path().join("hooks");
    common::install_hook(&hooks, "pre-commit", "exit 1");
    common::install_hook(&hooks, "commit-msg", "exit 1");
    common::log_hooks(&repo, &["prepare-commit-msg", "post-commit"]);

    let run = continue_sequence(
        &repo,
        state,
        HookOptions {
            no_verify: true,
            ..Default::default()
        },
    );

    assert!(matches!(run.outcome, Ok(SequenceOutcome::Completed { .. })));
    assert_eq!(
        common::read_log(&repo),
        "prepare-commit-msg <msg> merge\npost-commit \nprepare-commit-msg <msg> message\npost-commit \n"
    );
}

#[cfg(unix)]
#[test]
fn a_pre_commit_that_stages_a_file_is_part_of_the_continued_step() {
    if !common::program_available("git") {
        eprintln!("git CLI not found; skipping");
        return;
    }
    let (_dir, repo, _ids, state) = resolved();
    common::install_hook(
        &repo.path().join("hooks"),
        "pre-commit",
        "echo extra > extra.txt && git add extra.txt",
    );

    let run = continue_sequence(&repo, state, HookOptions::default());

    assert!(matches!(run.outcome, Ok(SequenceOutcome::Completed { .. })));
    let step_two = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .parent(0)
        .unwrap();
    assert_eq!(step_two.summary().unwrap(), Some("side two"));
    assert!(step_two.tree().unwrap().get_name("extra.txt").is_some());
    let step_one = step_two.parent(0).unwrap();
    assert!(step_one.tree().unwrap().get_name("extra.txt").is_none());
}

#[cfg(unix)]
#[test]
fn cancelling_a_continue_hook_keeps_the_sequence_paused() {
    let (_dir, repo, _ids, state) = resolved();
    common::install_hook(&repo.path().join("hooks"), "pre-commit", "sleep 30");
    let before = state.clone();
    let started = std::time::Instant::now();

    let run = continue_sequence(
        &repo,
        state,
        HookOptions {
            cancel: Some(&|| true),
            ..Default::default()
        },
    );

    assert!(
        matches!(
            run.outcome,
            Err(PickError::Hook(git_core::hooks::HookError::Cancelled { ref hook })) if hook == "pre-commit"
        ),
        "{:?}",
        run.outcome
    );
    assert_eq!(run.state, Some(before));
    assert_eq!(repo.state(), RepositoryState::CherryPick);
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
}

#[cfg(unix)]
#[test]
fn an_empty_resolution_is_skipped_without_running_hooks() {
    let (_dir, repo, ids, state) = paused(false);
    resolve_whole_file(&repo, "shared.txt", ConflictSide::Ours).unwrap();
    common::install_hook(&repo.path().join("hooks"), "pre-commit", "exit 1");
    common::log_hooks(&repo, &["prepare-commit-msg", "commit-msg", "post-commit"]);

    let run = continue_sequence(&repo, state, HookOptions::default());

    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 2,
            skipped: vec![ids[1].clone()]
        }
    );
    // Only the clean step after the skipped one runs hooks (#183).
    assert_eq!(
        common::read_log(&repo),
        "prepare-commit-msg <msg> message\npost-commit \n"
    );
}

#[cfg(unix)]
#[test]
fn a_revert_sequence_continue_runs_the_commit_hooks() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "a.txt", "one\n");
    common::commit_all(&repo, "one");
    common::write_file(dir.path(), "a.txt", "two\n");
    common::commit_all(&repo, "two");
    let two = head(&repo);
    common::write_file(dir.path(), "b.txt", "b\n");
    common::commit_all(&repo, "add b");
    let add_b = head(&repo);
    common::write_file(dir.path(), "a.txt", "three\n");
    common::commit_all(&repo, "three");
    let run = revert_many(
        &repo,
        &[two, add_b],
        git_core::hooks::HookOptions::default(),
    );
    assert!(matches!(
        run.outcome,
        Ok(SequenceOutcome::Conflicted { .. })
    ));
    common::write_file(dir.path(), "a.txt", "resolved\n");
    stage(&repo, "a.txt");
    common::log_hooks(&repo, &COMMIT_HOOKS);

    let run = continue_sequence(&repo, run.state.unwrap(), HookOptions::default());

    assert!(matches!(run.outcome, Ok(SequenceOutcome::Completed { .. })));
    assert_eq!(
        common::read_log(&repo),
        "pre-commit \nprepare-commit-msg <msg> merge\ncommit-msg <msg>\npost-commit \n"
    );
}

/// A `pre-commit` that puts the step back to HEAD's tree empties it: skipped, like an empty
/// resolution, rather than committed empty.
#[cfg(unix)]
#[test]
fn a_pre_commit_that_empties_the_step_skips_it() {
    if !common::program_available("git") {
        eprintln!("git CLI not found; skipping");
        return;
    }
    let (_dir, repo, ids, state) = resolved();
    common::install_hook(
        &repo.path().join("hooks"),
        "pre-commit",
        "git checkout HEAD -- shared.txt",
    );
    common::log_hooks(&repo, &["post-commit"]);

    let run = continue_sequence(&repo, state, HookOptions::default());

    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 2,
            skipped: vec![ids[1].clone()]
        }
    );
    assert_eq!(summaries(&repo, 3), ["side three", "side one", "ours edit"]);
    // The emptied step runs no post-commit; the clean step after it does (#183).
    assert_eq!(common::read_log(&repo), "post-commit \n");
}

// ---- hooks on clean steps (#183) -------------------------------------------------------------

/// Installs `names` as hooks logging `<name> <basename of $1> <$2> <P if CHERRY_PICK_HEAD><M if
/// MERGE_MSG>` to `<git dir>/hook-log`, so the log reads the same whether git passes the message
/// file as an absolute or relative path, and shows the pick state each hook sees.
#[cfg(unix)]
fn log_hooks_by_file_name(repo: &Repository, names: &[&str]) {
    let log = repo.path().join("hook-log");
    for name in names {
        common::install_hook(
            &repo.path().join("hooks"),
            name,
            &format!(
                r#"echo "{name} ${{1:+$(basename "$1")}} $2 $(test -f .git/CHERRY_PICK_HEAD && echo P)$(test -f .git/MERGE_MSG && echo M)" >> '{}'"#,
                log.display()
            ),
        );
    }
}

#[cfg(unix)]
fn hook_log(repo: &Repository) -> String {
    std::fs::read_to_string(repo.path().join("hook-log")).unwrap_or_default()
}

#[cfg(unix)]
const CLEAN_STEP_LOG: &str = "prepare-commit-msg COMMIT_EDITMSG message PM\npost-commit   PM\n";

/// A clean revert step: git keeps MERGE_MSG but writes no CHERRY_PICK_HEAD.
#[cfg(unix)]
const CLEAN_REVERT_LOG: &str = "prepare-commit-msg COMMIT_EDITMSG message M\npost-commit   M\n";

#[cfg(unix)]
#[test]
fn clean_sequence_steps_run_prepare_commit_msg_and_post_commit() {
    let (_dir, repo, ids) = side_branch(false, true);
    log_hooks_by_file_name(&repo, &COMMIT_HOOKS);

    let run = cherry_pick_many(&repo, &ids, HookOptions::default());

    assert!(matches!(
        run.outcome,
        Ok(SequenceOutcome::Completed { applied: 3, .. })
    ));
    assert_eq!(hook_log(&repo), CLEAN_STEP_LOG.repeat(3));
}

#[cfg(unix)]
#[test]
fn git_cherry_pick_runs_the_same_hooks_on_clean_steps() {
    if !common::program_available("git") {
        eprintln!("git CLI not found; skipping");
        return;
    }
    let (dir, repo, ids) = side_branch(false, true);
    log_hooks_by_file_name(&repo, &COMMIT_HOOKS);

    let status = std::process::Command::new("git")
        .arg("cherry-pick")
        .args(&ids)
        .current_dir(dir.path())
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();

    assert!(status.success());
    assert_eq!(hook_log(&repo), CLEAN_STEP_LOG.repeat(3));
}

#[cfg(unix)]
#[test]
fn clean_revert_steps_run_prepare_commit_msg_and_post_commit() {
    let (_dir, repo, ids) = side_branch(false, true);
    cherry_pick_many(&repo, &ids, HookOptions::default());
    let picked: Vec<String> = {
        let mut commit = repo.head().unwrap().peel_to_commit().unwrap();
        let mut out = Vec::new();
        for _ in 0..2 {
            out.push(commit.id().to_string());
            commit = commit.parent(0).unwrap();
        }
        out.reverse();
        out
    };
    log_hooks_by_file_name(&repo, &COMMIT_HOOKS);

    let run = revert_many(&repo, &picked, HookOptions::default());

    assert!(matches!(
        run.outcome,
        Ok(SequenceOutcome::Completed { applied: 2, .. })
    ));
    assert_eq!(hook_log(&repo), CLEAN_REVERT_LOG.repeat(2));
}

#[cfg(unix)]
#[test]
fn git_revert_runs_the_same_hooks_on_clean_steps() {
    if !common::program_available("git") {
        eprintln!("git CLI not found; skipping");
        return;
    }
    let (dir, repo, ids) = side_branch(false, true);
    cherry_pick_many(&repo, &ids, HookOptions::default());
    log_hooks_by_file_name(&repo, &COMMIT_HOOKS);

    // `git revert A B` reverts in the order given: newest first, like `revert_many`.
    let status = std::process::Command::new("git")
        .args(["revert", "--no-edit", "HEAD", "HEAD~1"])
        .current_dir(dir.path())
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();

    assert!(status.success());
    assert_eq!(hook_log(&repo), CLEAN_REVERT_LOG.repeat(2));
}

/// D7-A on a clean step (#216): a post-commit hook makes `.git` read-only once the middle step's
/// commit landed, so its pick state can't be cleared. The step still counts; the sequence stops
/// on the next step (which can't write its state) with `next`/`expected_head` past the middle
/// step, and Continue clears the leftover state and finishes.
#[cfg(unix)]
#[test]
fn a_cleanup_failure_after_a_middle_step_still_counts_it() {
    let (dir, repo, ids) = side_branch(false, true);
    let Some(restore) = common::ReadOnlyGitDir::restore_later(&repo) else {
        return;
    };
    let counter = tempfile::NamedTempFile::new().unwrap();
    common::install_hook(
        &repo.path().join("hooks"),
        "post-commit",
        &format!(
            "n=$(($(cat '{counter}' 2>/dev/null || echo 0) + 1)); echo $n > '{counter}'; \
             if [ $n = 2 ]; then chmod 555 '{git_dir}'; fi",
            counter = counter.path().display(),
            git_dir = repo.path().display(),
        ),
    );

    let run = cherry_pick_many(&repo, &ids, HookOptions::default());
    drop(restore);

    assert!(run.outcome.is_err(), "{:?}", run.outcome);
    let state = run.state.unwrap();
    assert_eq!(state.next, 2, "the middle step counts");
    assert_eq!(state.expected_head, head(&repo));
    assert_eq!(summaries(&repo, 2), ["side two", "side one"]);
    assert_eq!(
        repo.state(),
        RepositoryState::CherryPick,
        "the leftover state"
    );

    std::fs::remove_file(repo.path().join("hooks").join("post-commit")).unwrap();
    let run = continue_sequence(&repo, state, HookOptions::default());
    assert!(
        matches!(run.outcome, Ok(SequenceOutcome::Completed { .. })),
        "{:?}",
        run.outcome
    );
    assert_eq!(
        summaries(&repo, 4),
        ["side three", "side two", "side one", "main only"]
    );
    assert_eq!(repo.state(), RepositoryState::Clean);
    assert!(dir.path().join("three.txt").exists());
}

#[cfg(unix)]
#[test]
fn prepare_commit_msg_can_rewrite_a_picked_message() {
    let (_dir, repo, ids) = side_branch(false, true);
    common::install_hook(
        &repo.path().join("hooks"),
        "prepare-commit-msg",
        r#"printf '\nTicket: 42\n' >> "$1""#,
    );

    let run = cherry_pick_many(&repo, &ids, HookOptions::default());

    assert!(matches!(run.outcome, Ok(SequenceOutcome::Completed { .. })));
    for steps_back in 0..3 {
        assert!(
            message_of(&repo, steps_back)
                .trim_end()
                .ends_with("Ticket: 42"),
            "{}",
            message_of(&repo, steps_back)
        );
    }
}

#[cfg(unix)]
#[test]
fn a_failing_prepare_commit_msg_pauses_the_step_staged() {
    let (_dir, repo, ids) = side_branch(false, true);
    let hook = common::install_hook(
        &repo.path().join("hooks"),
        "prepare-commit-msg",
        r#"if grep -q "side two" "$1"; then echo "no ticket"; exit 1; fi"#,
    );

    let run = cherry_pick_many(&repo, &ids, HookOptions::default());

    let SequenceOutcome::HookFailed {
        hook: failed,
        output,
        progress,
    } = run.outcome.unwrap()
    else {
        panic!("expected HookFailed")
    };
    assert_eq!(failed, "prepare-commit-msg");
    assert_eq!(output, "no ticket\n");
    assert_eq!(progress.current_step, 2);
    let state = run.state.unwrap();
    assert!(state.stopped_on_conflict);
    assert_eq!(state.next, 1);
    assert_eq!(repo.state(), RepositoryState::CherryPick);
    assert_eq!(
        repo.find_reference("CHERRY_PICK_HEAD")
            .unwrap()
            .target()
            .unwrap()
            .to_string(),
        ids[1]
    );
    assert_eq!(summaries(&repo, 1), ["side one"]);
    let staged = repo
        .diff_tree_to_index(
            Some(&repo.head().unwrap().peel_to_tree().unwrap()),
            None,
            None,
        )
        .unwrap();
    assert!(staged.deltas().len() > 0, "the step stays staged");

    std::fs::remove_file(hook).unwrap();
    let run = continue_sequence(&repo, state, HookOptions::default());
    assert!(matches!(run.outcome, Ok(SequenceOutcome::Completed { .. })));
    assert_eq!(summaries(&repo, 3), ["side three", "side two", "side one"]);
    assert_eq!(repo.state(), RepositoryState::Clean);
}

#[cfg(unix)]
#[test]
fn continue_runs_the_hooks_for_later_clean_steps() {
    let (_dir, repo, _ids, state) = resolved();
    log_hooks_by_file_name(&repo, &COMMIT_HOOKS);

    let run = continue_sequence(&repo, state, HookOptions::default());

    assert!(matches!(run.outcome, Ok(SequenceOutcome::Completed { .. })));
    assert_eq!(
        hook_log(&repo),
        format!(
            "pre-commit   PM\nprepare-commit-msg COMMIT_EDITMSG merge PM\ncommit-msg COMMIT_EDITMSG  PM\npost-commit   \n{CLEAN_STEP_LOG}"
        )
    );
}

#[cfg(unix)]
#[test]
fn cancel_during_a_steps_post_commit_keeps_it_and_stops_after_it() {
    let (dir, repo, ids) = side_branch(false, true);
    let marker = dir.path().join("..").join(format!(
        "{}-post-commit-started",
        dir.path().file_name().unwrap().to_string_lossy()
    ));
    common::install_hook(
        &repo.path().join("hooks"),
        "post-commit",
        &format!("touch '{}'; sleep 30", marker.display()),
    );
    let cancel = || marker.exists();
    let started = std::time::Instant::now();

    let run = cherry_pick_many(
        &repo,
        &ids,
        HookOptions {
            cancel: Some(&cancel),
            ..Default::default()
        },
    );
    let _ = std::fs::remove_file(&marker);

    assert!(started.elapsed() < std::time::Duration::from_secs(20));
    let Ok(SequenceOutcome::Stopped { kept, progress }) = &run.outcome else {
        panic!("expected Stopped, got {:?}", run.outcome)
    };
    assert_eq!(kept, "side one");
    assert_eq!(progress.current_step, 2);
    let state = run.state.unwrap();
    assert_eq!(state.next, 1);
    assert_eq!(state.expected_head, head(&repo));
    assert_eq!(summaries(&repo, 2), ["side one", "main only"]);
    assert_eq!(repo.state(), RepositoryState::Clean);

    std::fs::remove_file(repo.path().join("hooks").join("post-commit")).unwrap();
    let run = continue_sequence(&repo, state, HookOptions::default());
    assert!(matches!(run.outcome, Ok(SequenceOutcome::Completed { .. })));
    assert_eq!(
        summaries(&repo, 4),
        ["side three", "side two", "side one", "main only"]
    );
}

#[cfg(unix)]
#[test]
fn a_cleanup_failure_after_continue_still_counts_the_step() {
    // Pause on the last step, so nothing runs after the step Continue commits.
    let (dir, repo, ids) = side_branch(true, false);
    let state = cherry_pick_many(&repo, &ids[1..2], HookOptions::default())
        .state
        .unwrap();
    common::write_file(dir.path(), "shared.txt", "resolved\n");
    stage(&repo, "shared.txt");
    let Some(read_only) = common::ReadOnlyGitDir::new(&repo) else {
        return;
    };

    let run = continue_sequence(&repo, state, HookOptions::default());
    drop(read_only);

    assert!(
        matches!(
            run.outcome,
            Ok(SequenceOutcome::Completed { applied: 1, .. })
        ),
        "{:?}",
        run.outcome
    );
    assert_eq!(summaries(&repo, 1), ["side two"]);
}

/// `a_cleanup_failure_after_a_middle_step_still_counts_it`, then Abort instead of Continue: the
/// leftover pick state of the committed middle step is ours, so Abort clears it and rewinds the
/// whole sequence.
#[cfg(unix)]
#[test]
fn abort_after_a_cleanup_failure_rewinds_the_whole_sequence() {
    let (dir, repo, ids) = side_branch(false, true);
    let original = head(&repo);
    let Some(restore) = common::ReadOnlyGitDir::restore_later(&repo) else {
        return;
    };
    let counter = tempfile::NamedTempFile::new().unwrap();
    common::install_hook(
        &repo.path().join("hooks"),
        "post-commit",
        &format!(
            "n=$(($(cat '{counter}' 2>/dev/null || echo 0) + 1)); echo $n > '{counter}'; \
             if [ $n = 2 ]; then chmod 555 '{git_dir}'; fi",
            counter = counter.path().display(),
            git_dir = repo.path().display(),
        ),
    );
    let run = cherry_pick_many(&repo, &ids, HookOptions::default());
    drop(restore);
    let state = run.state.unwrap();
    assert_eq!(state.next, 2);
    assert_eq!(
        repo.state(),
        RepositoryState::CherryPick,
        "the leftover state"
    );

    let abort = abort_sequence(&repo, &state).unwrap();

    assert!(abort.rewound);
    assert_eq!(head(&repo), original);
    assert_eq!(repo.state(), RepositoryState::Clean);
    assert!(!dir.path().join("one.txt").exists());
    assert!(!dir.path().join("two.txt").exists());
    assert_eq!(summaries(&repo, 1), ["main only"]);
}

/// A commit selected twice is applied once, so no two steps share an id (and a leftover pick
/// state can never be mistaken for the paused step's).
#[test]
fn a_commit_selected_twice_is_applied_once() {
    let (_dir, repo, ids) = side_branch(false, true);
    let run = cherry_pick_many(
        &repo,
        &[ids[0].clone(), ids[0].clone(), ids[2].clone()],
        HookOptions::default(),
    );

    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 2,
            skipped: vec![]
        }
    );
    assert_eq!(summaries(&repo, 3), ["side three", "side one", "main only"]);
}

/// #226: the last step's pick state can't be cleared (a `post-commit` hook makes `.git`
/// read-only on that step, D7-A). The sequence completes; the next commit is the user's, not the
/// last picked commit's author's, and clears the leftover.
#[cfg(unix)]
#[test]
fn a_leftover_after_the_last_step_does_not_author_the_next_commit() {
    let (dir, repo, mut ids) = side_branch(false, false);
    let base_ref = base_branch_ref(&repo);
    repo.set_head("refs/heads/side").unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
    common::write_file(dir.path(), "four.txt", "4\n");
    commit_as_someone_else(&repo, "side four");
    ids.push(head(&repo));
    repo.set_head(&base_ref).unwrap();
    repo.checkout_head(Some(
        git2::build::CheckoutBuilder::new()
            .force()
            .remove_untracked(true),
    ))
    .unwrap();

    let Some(restore) = common::ReadOnlyGitDir::restore_later(&repo) else {
        return;
    };
    let counter = tempfile::TempDir::new().unwrap();
    let count = counter.path().join("count");
    common::install_hook(
        &repo.path().join("hooks"),
        "post-commit",
        &format!(
            "echo x >> '{}'; [ \"$(wc -l < '{}')\" -eq 4 ] && chmod 555 '{}'; exit 0",
            count.display(),
            count.display(),
            repo.path().display()
        ),
    );
    let run = cherry_pick_many(&repo, &ids, HookOptions::default());
    drop(restore);
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 4,
            skipped: vec![]
        }
    );
    let last_step = head(&repo);
    assert_eq!(repo.state(), RepositoryState::CherryPick, "leftover");

    std::fs::remove_dir_all(repo.path().join("hooks")).unwrap();
    common::write_file(dir.path(), "after.txt", "after\n");
    stage(&repo, "after.txt");
    let mut repo = repo;
    git_core::commit::commit(&mut repo, "after the sequence", HookOptions::default()).unwrap();

    let commit = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(commit.author().name().unwrap(), "Test User");
    assert_eq!(commit.parent_id(0).unwrap().to_string(), last_step);
    assert_eq!(repo.state(), RepositoryState::Clean);
}

// --- #241: committing a step of a terminal `git cherry-pick A B C` ---

/// Runs the git CLI in `dir`.
fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

/// base: f="base\n". side, by "Side Author": A (f="A\n"), B (+b.txt), C (+c.txt). Then a commit "main" sets
/// f="main\n": on the base branch (checked out) for a cherry-pick, on side (checked out) for a
/// revert, so picking or reverting A conflicts and B, C apply cleanly. Returns A, B, C.
fn terminal_sequence_repo(command: &str) -> (tempfile::TempDir, Repository, [String; 3]) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f", "base\n");
    common::commit_all(&repo, "base");
    let base_ref = base_branch_ref(&repo);
    {
        let base = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("side", &base, false).unwrap();
    }
    repo.set_head("refs/heads/side").unwrap();
    let mut ids = Vec::new();
    repo.config()
        .unwrap()
        .set_str("user.name", "Side Author")
        .unwrap();
    for (path, contents, message) in [
        ("f", "A\n", "A"),
        ("b.txt", "b\n", "B"),
        ("c.txt", "c\n", "C"),
    ] {
        common::write_file(dir.path(), path, contents);
        common::commit_all(&repo, message);
        ids.push(head(&repo));
    }
    repo.config()
        .unwrap()
        .set_str("user.name", "Test User")
        .unwrap();
    if command != "revert" {
        repo.set_head(&base_ref).unwrap();
        repo.checkout_head(Some(
            git2::build::CheckoutBuilder::new()
                .force()
                .remove_untracked(true),
        ))
        .unwrap();
    }
    common::write_file(dir.path(), "f", "main\n");
    common::commit_all(&repo, "main");
    let ids: [String; 3] = ids.try_into().unwrap();
    (dir, repo, ids)
}

/// A terminal `git <command> A B C` (`cherry-pick` or `revert`) paused on A's conflict, resolved
/// (f="resolved\n") and staged, not yet committed. `last` runs `B A`, so A is the last step.
fn terminal_sequence(command: &str, last: bool) -> (tempfile::TempDir, Repository, [String; 3]) {
    let (dir, repo, ids) = terminal_sequence_repo(command);
    let [a, b, c] = &ids;
    let mut args = vec![command];
    if command == "revert" {
        args.push("--no-edit");
    }
    if last {
        args.extend([b.as_str(), a.as_str()]);
    } else {
        args.extend([a.as_str(), b.as_str(), c.as_str()]);
    }
    let output = git(dir.path(), &args);
    assert!(!output.status.success(), "the sequence must pause on A");
    assert!(repo.path().join("sequencer").join("todo").exists());
    common::write_file(dir.path(), "f", "resolved\n");
    stage(&repo, "f");
    (dir, repo, ids)
}

/// The same paused sequence, finished by `git commit --no-edit` (git's own result). Also returns
/// the twin's todo from before that commit (its commit ids differ from the original's).
fn paused_twin(command: &str, last: bool) -> (tempfile::TempDir, Repository, [String; 3], String) {
    let (dir, repo, ids) = terminal_sequence(command, last);
    let before = todo(&repo).unwrap();
    let output = git(dir.path(), &["commit", "--no-edit", "--cleanup=strip"]);
    assert!(output.status.success(), "{output:?}");
    (dir, repo, ids, before)
}

fn todo(repo: &Repository) -> Option<String> {
    std::fs::read_to_string(repo.path().join("sequencer").join("todo")).ok()
}

/// The paused step committed by Browsitory's Commit button, as `git commit` would: the pick's own
/// files go, the todo stays.
fn commit_the_step(repo: Repository, message: &str) -> Repository {
    let mut repo = repo;
    git_core::commit::commit(&mut repo, message, HookOptions::default()).unwrap();
    assert!(!repo.path().join("CHERRY_PICK_HEAD").exists());
    assert!(!repo.path().join("REVERT_HEAD").exists());
    assert!(!repo.path().join("MERGE_MSG").exists());
    assert_eq!(repo.state(), RepositoryState::Clean);
    repo
}

/// Whether `git status` still reports a cherry-pick or revert in progress.
fn git_reports_a_pick(dir: &Path) -> bool {
    let output = git(dir, &["status"]);
    let text = String::from_utf8_lossy(&output.stdout);
    text.contains("cherry-pick") || text.contains("revert")
}

#[test]
fn committing_a_terminal_sequence_step_keeps_its_todo() {
    let (dir, repo, _) = terminal_sequence("cherry-pick", false);
    let before = todo(&repo).unwrap();

    let repo = commit_the_step(repo, "A\n");

    assert_eq!(todo(&repo).as_deref(), Some(before.as_str()));
    let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head_commit.author().name().unwrap(), "Side Author");
    assert_eq!(common::read(dir.path(), "f"), "resolved\n");
    let output = git(dir.path(), &["cherry-pick", "--continue"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(summaries(&repo, 4), ["C", "B", "A", "main"]);
    assert!(!repo.path().join("sequencer").exists());

    // git's twin: `git commit` keeps the todo too.
    let (twin_dir, twin, _, twin_before) = paused_twin("cherry-pick", false);
    assert_eq!(todo(&twin).as_deref(), Some(twin_before.as_str()));
    let output = git(twin_dir.path(), &["cherry-pick", "--continue"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(summaries(&twin, 4), ["C", "B", "A", "main"]);
}

#[test]
fn committing_the_last_step_of_a_terminal_sequence_ends_it() {
    let (dir, repo, _) = terminal_sequence("cherry-pick", true);

    let repo = commit_the_step(repo, "A\n");

    assert!(!repo.path().join("sequencer").exists());
    assert!(!git_reports_a_pick(dir.path()));
    assert_eq!(summaries(&repo, 3), ["A", "B", "main"]);

    let (twin_dir, twin, _, _) = paused_twin("cherry-pick", true);
    assert!(!twin.path().join("sequencer").exists());
    assert!(!git_reports_a_pick(twin_dir.path()));
}

#[test]
fn committing_a_terminal_revert_sequence_step_keeps_its_todo() {
    let (dir, repo, _) = terminal_sequence("revert", false);
    let before = todo(&repo).unwrap();

    let repo = commit_the_step(repo, "Revert \"A\"\n");

    assert_eq!(todo(&repo).as_deref(), Some(before.as_str()));
    let output = git(dir.path(), &["revert", "--continue", "--no-edit"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        summaries(&repo, 4),
        ["Revert \"C\"", "Revert \"B\"", "Revert \"A\"", "main"]
    );
    assert!(!repo.path().join("sequencer").exists());

    let (twin_dir, twin, _, twin_before) = paused_twin("revert", false);
    assert_eq!(todo(&twin).as_deref(), Some(twin_before.as_str()));
    assert!(git(twin_dir.path(), &["revert", "--continue", "--no-edit"])
        .status
        .success());
}

#[test]
fn committing_the_last_step_of_a_terminal_revert_sequence_ends_it() {
    let (dir, repo, _) = terminal_sequence("revert", true);

    let repo = commit_the_step(repo, "Revert \"A\"\n");

    assert!(!repo.path().join("sequencer").exists());
    assert!(!git_reports_a_pick(dir.path()));
}

#[test]
fn a_single_cherry_pick_keeps_a_terminal_sequences_todo() {
    let (_dir, repo, [_, b, _]) = terminal_sequence("cherry-pick", false);
    let before = todo(&repo).unwrap();
    let repo = commit_the_step(repo, "A\n");

    let outcome = cherry_pick(&repo, &b, None, HookOptions::default()).unwrap();

    assert!(matches!(
        outcome,
        git_core::pick::PickOutcome::Committed { .. }
    ));
    assert_eq!(todo(&repo).as_deref(), Some(before.as_str()));
    assert_eq!(repo.state(), RepositoryState::Clean);
    // Picking it again changes nothing: refused as empty, the todo still kept.
    assert!(matches!(
        cherry_pick(&repo, &b, None, HookOptions::default()),
        Err(PickError::EmptyResult)
    ));
    assert_eq!(todo(&repo).as_deref(), Some(before.as_str()));
    assert_eq!(repo.state(), RepositoryState::Clean);

    // git's twin keeps it as well.
    let (twin_dir, twin, [_, b, _], twin_before) = paused_twin("cherry-pick", false);
    let output = git(twin_dir.path(), &["cherry-pick", &b]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(todo(&twin).as_deref(), Some(twin_before.as_str()));
}

#[test]
fn a_single_revert_keeps_a_terminal_sequences_todo() {
    let (_dir, repo, _) = terminal_sequence("cherry-pick", false);
    let before = todo(&repo).unwrap();
    let repo = commit_the_step(repo, "A\n");
    let resolved = head(&repo);

    let outcome = git_core::pick::revert(&repo, &resolved, None, HookOptions::default()).unwrap();

    assert!(matches!(
        outcome,
        git_core::pick::PickOutcome::Committed { .. }
    ));
    assert_eq!(todo(&repo).as_deref(), Some(before.as_str()));
    assert_eq!(repo.state(), RepositoryState::Clean);

    let (twin_dir, twin, _, twin_before) = paused_twin("cherry-pick", false);
    let output = git(twin_dir.path(), &["revert", "--no-edit", "HEAD"]);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(todo(&twin).as_deref(), Some(twin_before.as_str()));
}

#[test]
fn a_squash_merge_keeps_a_terminal_sequences_todo() {
    let (dir, repo, _) = terminal_sequence("cherry-pick", false);
    let before = todo(&repo).unwrap();
    let repo = commit_the_step(repo, "A\n");
    let extra = common::commit_tree(
        &repo,
        &[("f", "resolved\n", 0o100644), ("e.txt", "e\n", 0o100644)],
        &[repo.head().unwrap().target().unwrap()],
        "extra",
    );
    repo.branch("extra", &repo.find_commit(extra).unwrap(), false)
        .unwrap();

    let outcome = git_core::merge::start_merge(
        &repo,
        "extra",
        git_core::merge::MergeOptions {
            squash: true,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(outcome, git_core::merge::MergeOutcome::Squashed);
    assert_eq!(todo(&repo).as_deref(), Some(before.as_str()));
    assert!(repo.path().join("SQUASH_MSG").exists());
    assert_eq!(common::read(dir.path(), "e.txt"), "e\n");
}

/// A one-line todo goes only with the commit that finishes a pick: a merge commit (state cleared,
/// but no CHERRY_PICK_HEAD/REVERT_HEAD) keeps it, as `git commit` does (#241).
#[test]
fn a_commit_without_a_pick_keeps_a_one_line_todo() {
    let setup = || {
        let (dir, repo) = common::init_repo();
        common::write_file(dir.path(), "f", "base\n");
        common::commit_all(&repo, "base");
        let base = repo.head().unwrap().target().unwrap();
        // A side commit, not HEAD's ancestor, so the merge isn't taken for one already committed.
        let side = common::commit_tree(
            &repo,
            &[("f", "base\n", 0o100644), ("s.txt", "s\n", 0o100644)],
            &[base],
            "side",
        );
        common::write_file(dir.path(), "f", "edit\n");
        common::commit_all(&repo, "edit");
        common::write_file(dir.path(), "s.txt", "s\n");
        stage(&repo, "s.txt");
        std::fs::write(repo.path().join("MERGE_HEAD"), format!("{side}\n")).unwrap();
        let sequencer = repo.path().join("sequencer");
        std::fs::create_dir(&sequencer).unwrap();
        let line = format!("pick {side} side\n");
        std::fs::write(sequencer.join("todo"), &line).unwrap();
        assert_eq!(repo.state(), RepositoryState::Merge);
        (dir, repo, line)
    };
    let (_dir, mut repo, line) = setup();

    git_core::commit::commit(&mut repo, "merge\n", HookOptions::default()).unwrap();

    assert_eq!(
        repo.head()
            .unwrap()
            .peel_to_commit()
            .unwrap()
            .parent_count(),
        2
    );
    assert!(!repo.path().join("MERGE_HEAD").exists());
    assert_eq!(todo(&repo).as_deref(), Some(line.as_str()));

    let (twin_dir, twin, line) = setup();
    let output = git(twin_dir.path(), &["commit", "-m", "merge"]);
    assert!(output.status.success(), "{output:?}");
    assert!(!twin.path().join("MERGE_HEAD").exists());
    assert_eq!(todo(&twin).as_deref(), Some(line.as_str()));
}
