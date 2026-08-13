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
            assert_eq!(ours, "main two");
            assert_eq!(theirs, "feature two");
        }
        _ => unreachable!(),
    }
}

#[test]
fn conflict_hunks_errors_for_a_path_with_no_conflict() {
    let (_dir, repo) = make_conflicted_repo();

    let result = git_core::merge::conflict_hunks(&repo, "does-not-exist.txt");

    assert!(result.is_err());
}
