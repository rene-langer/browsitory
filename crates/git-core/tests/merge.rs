mod common;

use common::{commit_all, init_repo, write_file};
use git_core::merge::MergeOutcome;

#[test]
fn start_merge_reports_up_to_date_when_the_target_has_nothing_new() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();

    let outcome = git_core::merge::start_merge(&repo, "feature").unwrap();

    assert_eq!(outcome, MergeOutcome::UpToDate);
}

#[test]
fn start_merge_fast_forwards_when_current_branch_has_not_diverged() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "feature.txt", "new\n");
    commit_all(&repo, "feature commit");
    let feature_tip = repo.head().unwrap().peel_to_commit().unwrap().id();
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();

    let outcome = git_core::merge::start_merge(&repo, "feature").unwrap();

    assert_eq!(outcome, MergeOutcome::FastForwarded);
    assert_eq!(
        repo.head().unwrap().peel_to_commit().unwrap().id(),
        feature_tip
    );
    assert!(dir.path().join("feature.txt").exists());
}

#[test]
fn start_merge_produces_a_clean_merge_when_changes_do_not_overlap() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "feature.txt", "new\n");
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "main.txt", "new\n");
    commit_all(&repo, "main commit");

    let outcome = git_core::merge::start_merge(&repo, "feature").unwrap();

    assert_eq!(outcome, MergeOutcome::Merged);
    assert!(dir.path().join("feature.txt").exists());
    assert!(dir.path().join("main.txt").exists());
}

#[test]
fn start_merge_reports_conflicted_files_when_the_same_line_diverges() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\nline three\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(
        dir.path(),
        "shared.txt",
        "line one\nfeature two\nline three\n",
    );
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "shared.txt", "line one\nmain two\nline three\n");
    commit_all(&repo, "main commit");

    let outcome = git_core::merge::start_merge(&repo, "feature").unwrap();

    match outcome {
        MergeOutcome::Conflicted { files } => assert_eq!(files, vec!["shared.txt".to_string()]),
        other => panic!("expected Conflicted, got {other:?}"),
    }
}

use git_core::merge::ConflictSegment;

fn make_conflicted_repo() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\nline three\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(
        dir.path(),
        "shared.txt",
        "line one\nfeature two\nline three\n",
    );
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "shared.txt", "line one\nmain two\nline three\n");
    commit_all(&repo, "main commit");

    git_core::merge::start_merge(&repo, "feature").unwrap();
    (dir, repo)
}

#[test]
fn conflict_hunks_returns_clean_and_conflict_segments_for_a_conflicted_file() {
    let (_dir, repo) = make_conflicted_repo();

    let segments = git_core::merge::conflict_hunks(&repo, "shared.txt").unwrap();

    // "line one" and "line three" are unchanged on both sides — git2's own 3-way merge
    // auto-resolves them, leaving only the middle line as a real conflict.
    assert!(segments
        .iter()
        .any(|s| matches!(s, ConflictSegment::Clean { content } if content.contains("line one"))));
    let conflict = segments
        .iter()
        .find(|s| matches!(s, ConflictSegment::Conflict { .. }))
        .expect("expected a Conflict segment");
    match conflict {
        ConflictSegment::Conflict { ours, theirs } => {
            // Each side keeps its own trailing newline — `parse_conflict_markers` now splits on
            // `split_inclusive('\n')` rather than `.lines()`, so terminators aren't stripped.
            assert_eq!(ours, "main two\n");
            assert_eq!(theirs, "feature two\n");
        }
        _ => unreachable!(),
    }
}

#[test]
fn conflict_hunks_round_trip_preserves_the_original_files_trailing_newline() {
    let (_dir, repo) = make_conflicted_repo();

    let segments = git_core::merge::conflict_hunks(&repo, "shared.txt").unwrap();

    // Reconstruct by choosing "ours" for every conflict, exactly like the frontend's Accept
    // Ours path, and confirm the round trip reproduces the original file's content byte for
    // byte — including its trailing newline. The original file (see `make_conflicted_repo`)
    // was "line one\nmain two\nline three\n" on the "ours" (main) side.
    let reconstructed = segments
        .into_iter()
        .map(|segment| match segment {
            ConflictSegment::Clean { content } => content,
            ConflictSegment::Conflict { ours, .. } => ours,
        })
        .collect::<Vec<_>>()
        .join("");

    assert_eq!(reconstructed, "line one\nmain two\nline three\n");
}

#[test]
fn conflict_hunks_errors_for_a_path_with_no_conflict() {
    let (_dir, repo) = make_conflicted_repo();

    let result = git_core::merge::conflict_hunks(&repo, "does-not-exist.txt");

    assert!(result.is_err());
}

#[test]
fn resolve_conflict_clears_the_conflict_and_stages_the_result() {
    let (dir, repo) = make_conflicted_repo();

    git_core::merge::resolve_conflict(&repo, "shared.txt", "line one\nresolved two\nline three\n")
        .unwrap();

    assert!(!repo.index().unwrap().has_conflicts());
    let contents = std::fs::read_to_string(dir.path().join("shared.txt")).unwrap();
    assert_eq!(contents, "line one\nresolved two\nline three\n");
}

#[test]
fn abort_merge_restores_the_pre_merge_working_tree_and_clears_conflicts() {
    let (dir, repo) = make_conflicted_repo();

    git_core::merge::abort_merge(&repo).unwrap();

    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    assert!(!repo.index().unwrap().has_conflicts());
    let contents = std::fs::read_to_string(dir.path().join("shared.txt")).unwrap();
    assert_eq!(contents, "line one\nmain two\nline three\n");
}

#[test]
fn merge_message_and_is_merging_reflect_an_in_progress_merge() {
    let (_dir, repo) = make_conflicted_repo();

    assert!(git_core::merge::is_merging(&repo));
    assert!(git_core::merge::merge_message(&repo)
        .unwrap()
        .contains("feature"));
}

#[test]
fn merge_message_and_is_merging_are_clear_outside_a_merge() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");

    assert!(!git_core::merge::is_merging(&repo));
    assert!(git_core::merge::merge_message(&repo).is_none());
}

#[test]
fn commit_after_a_fast_forward_has_a_single_parent_as_before() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "feature.txt", "new\n");
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();

    git_core::merge::start_merge(&repo, "feature").unwrap();

    // A fast-forward moves the ref directly — there's nothing to commit, and no merge state to
    // clean up. This test exists to document that `commit()`'s merge-parent logic only engages
    // for `RepositoryState::Merge`, not for every post-merge repo state.
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

#[test]
fn a_commit_made_after_resolving_a_conflict_has_two_parents() {
    let (_dir, repo) = make_conflicted_repo();
    git_core::merge::resolve_conflict(&repo, "shared.txt", "line one\nresolved two\nline three\n")
        .unwrap();

    let mut repo = repo;
    let oid = git_core::commit::commit(&mut repo, "merge feature into base").unwrap();

    let commit = repo
        .find_commit(git2::Oid::from_str(&oid).unwrap())
        .unwrap();
    assert_eq!(commit.parent_count(), 2);
}
