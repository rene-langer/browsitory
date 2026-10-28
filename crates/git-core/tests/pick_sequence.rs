// crates/git-core/tests/pick_sequence.rs
mod common;

use git2::{Oid, Repository, RepositoryState};
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
    let state = cherry_pick_many(&repo, &ids).state.unwrap();
    (dir, repo, ids, state)
}

#[test]
fn a_clean_sequence_applies_every_commit_oldest_first() {
    let (dir, repo, ids) = side_branch(false, true);
    let run = cherry_pick_many(&repo, &ids);
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
    let run = cherry_pick_many(&repo, &ids);
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
    cherry_pick(&repo, &ids[0], None).unwrap();
    let run = cherry_pick_many(&repo, &ids);
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
    let run = revert_many(&repo, &ids);
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
    let run = cherry_pick_many(&repo, &[ids[0].clone(), merge]);
    assert!(matches!(run.outcome, Err(PickError::MainlineRequired)));
    assert!(run.state.is_none());
    assert_eq!(head(&repo), before);
    assert!(!dir.path().join("one.txt").exists());
}

#[test]
fn nothing_selected_is_refused() {
    let (_dir, repo, _) = side_branch(false, true);
    let run = cherry_pick_many(&repo, &[]);
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
    let run = cherry_pick_many(&repo, &ids);
    assert!(matches!(run.outcome, Err(PickError::DirtyIndex)));
    assert!(run.state.is_none());

    let (_dir, repo, ids, _state) = paused(true);
    let run = revert_many(&repo, &ids[..1]);
    assert!(matches!(run.outcome, Err(PickError::OperationInProgress)));
    assert!(run.state.is_none());
}

#[test]
fn an_error_on_the_first_step_returns_no_state() {
    let (dir, repo, ids) = side_branch(false, true);
    let before = head(&repo);
    // An untracked file in the way of the first pick's new file: libgit2's SAFE checkout refuses.
    common::write_file(dir.path(), "one.txt", "mine\n");
    let run = cherry_pick_many(&repo, &ids);
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
    let run = cherry_pick_many(&repo, &ids);
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
    let run = continue_sequence(&repo, state);
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
    let run = continue_sequence(&repo, state.clone());
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
    let run = continue_sequence(&repo, state);
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
    let run = revert_many(&repo, &[two.clone(), add_b.clone()]);
    let Ok(SequenceOutcome::Conflicted { progress, .. }) = run.outcome else {
        panic!("expected a conflict, got {:?}", run.outcome)
    };
    assert_eq!(progress.kind, SequenceKind::Revert);
    assert_eq!((progress.current_step, progress.total_steps), (2, 2));
    assert_eq!(repo.state(), RepositoryState::Revert);
    common::write_file(dir.path(), "a.txt", "resolved\n");
    stage(&repo, "a.txt");
    let run = continue_sequence(&repo, run.state.unwrap());
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
    let state = cherry_pick_many(&repo, &ids).state.unwrap();
    std::fs::remove_file(dir.path().join("three.txt")).unwrap();
    let run = continue_sequence(&repo, state);
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
    let run = continue_sequence(&repo, state);
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
    let state = cherry_pick_many(&repo, &ids).state.unwrap();
    std::fs::remove_file(dir.path().join("three.txt")).unwrap();
    common::write_file(dir.path(), "notes.txt", "staged\n");
    stage(&repo, "notes.txt");
    let run = continue_sequence(&repo, state.clone());
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
        "side two (by hand)",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    let run = continue_sequence(&repo, state);
    assert_eq!(
        run.outcome.unwrap(),
        SequenceOutcome::Completed {
            applied: 3,
            skipped: vec![]
        }
    );
    assert_eq!(
        summaries(&repo, 3),
        ["side three", "side two (by hand)", "side one"]
    );
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
        cherry_pick(&repo, &other, None).unwrap(),
        git_core::pick::PickOutcome::Conflicted { .. }
    ));
    let run = continue_sequence(&repo, state.clone());
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
    let state = cherry_pick_many(&repo, &ids).state.unwrap();

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
    let state = cherry_pick_many(&repo, &ids).state.unwrap();
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

    let run = continue_sequence(&repo, state.clone());
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
    let state = cherry_pick_many(&repo, &ids).state.unwrap();
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
    let state = cherry_pick_many(&repo, &ids).state.unwrap();
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
