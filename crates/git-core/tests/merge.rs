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

    let outcome =
        git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
            .unwrap();

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

    let outcome =
        git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
            .unwrap();

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

    let outcome =
        git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
            .unwrap();

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

    let outcome =
        git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
            .unwrap();

    match outcome {
        MergeOutcome::Conflicted { files } => assert_eq!(files, vec!["shared.txt".to_string()]),
        other => panic!("expected Conflicted, got {other:?}"),
    }
}

use git_core::merge::ConflictSegment;

fn make_conflicted_repo() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = make_conflicting_branches();
    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();
    (dir, repo)
}

/// `shared.txt`'s middle line changed differently on `feature` and on the checked-out main
/// branch, nothing merged yet.
fn make_conflicting_branches() -> (tempfile::TempDir, git2::Repository) {
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
fn resolve_conflict_rejects_a_path_with_no_index_conflict() {
    let (dir, repo) = make_conflicted_repo();

    let result =
        git_core::merge::resolve_conflict(&repo, "does-not-exist.txt", "malicious content\n");

    assert!(matches!(
        result,
        Err(git_core::merge::MergeError::NoConflict(ref p)) if p == "does-not-exist.txt"
    ));
    // Nothing should have been written or staged — the rejection must happen before any
    // working-directory or index mutation.
    assert!(!dir.path().join("does-not-exist.txt").exists());
    assert!(repo.index().unwrap().has_conflicts());
}

#[test]
fn resolve_conflict_rejects_a_git_relative_path_even_though_it_resolves_under_the_workdir() {
    let (dir, repo) = make_conflicted_repo();

    // Regression for AUD-2026-09-05-GIT-001: `resolve_conflict` used to write
    // `resolved_content` straight to `workdir.join(path)` and stage it without first checking
    // `path` was an actual conflicted path in the index. A path like ".git/config" joins to a
    // real, sensitive file under the workdir but is never a conflicted index entry — it must be
    // rejected before any write happens.
    let result = git_core::merge::resolve_conflict(&repo, ".git/config", "[core]\n\tbogus = 1\n");

    assert!(matches!(
        result,
        Err(git_core::merge::MergeError::NoConflict(ref p)) if p == ".git/config"
    ));
    let git_config_contents = std::fs::read_to_string(dir.path().join(".git/config")).unwrap();
    assert!(!git_config_contents.contains("bogus"));
    assert!(repo.index().unwrap().has_conflicts());
}

#[test]
fn mark_resolved_stages_a_clean_file_and_clears_the_conflict() {
    let (dir, repo) = make_conflicted_repo();
    write_file(
        dir.path(),
        "shared.txt",
        "line one\nmerged two\nline three\n",
    );
    let outcome = git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    assert_eq!(outcome, git_core::merge::MarkResolvedOutcome::Resolved);
    assert!(!repo.index().unwrap().has_conflicts());
    let staged = repo
        .index()
        .unwrap()
        .get_path(std::path::Path::new("shared.txt"), 0)
        .unwrap();
    assert_eq!(
        repo.find_blob(staged.id).unwrap().content(),
        b"line one\nmerged two\nline three\n"
    );
}

#[test]
fn mark_resolved_reports_markers_and_keeps_the_conflict() {
    let (_dir, repo) = make_conflicted_repo();
    let outcome = git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    assert_eq!(outcome, git_core::merge::MarkResolvedOutcome::MarkersRemain);
    assert!(repo.index().unwrap().has_conflicts());
}

#[test]
fn mark_resolved_with_force_stages_despite_markers() {
    let (_dir, repo) = make_conflicted_repo();
    let outcome = git_core::merge::mark_resolved(&repo, "shared.txt", true).unwrap();
    assert_eq!(outcome, git_core::merge::MarkResolvedOutcome::Resolved);
    assert!(!repo.index().unwrap().has_conflicts());
}

#[test]
fn mark_resolved_does_not_mistake_a_markdown_underline_for_a_marker() {
    let (dir, repo) = make_conflicted_repo();
    write_file(dir.path(), "shared.txt", "Title\n=======\n\ntext\n");
    let outcome = git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    assert_eq!(outcome, git_core::merge::MarkResolvedOutcome::Resolved);
}

#[test]
fn mark_resolved_stages_a_deletion_when_the_file_was_removed() {
    let (dir, repo) = make_conflicted_repo();
    std::fs::remove_file(dir.path().join("shared.txt")).unwrap();
    let outcome = git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    assert_eq!(outcome, git_core::merge::MarkResolvedOutcome::Resolved);
    let index = repo.index().unwrap();
    assert!(!index.has_conflicts());
    assert!(index
        .get_path(std::path::Path::new("shared.txt"), 0)
        .is_none());
}

#[test]
fn mark_resolved_rejects_a_path_without_a_conflict() {
    let (dir, repo) = make_conflicted_repo();
    write_file(dir.path(), "other.txt", "x\n");
    let err = git_core::merge::mark_resolved(&repo, "other.txt", false).unwrap_err();
    assert_eq!(err.to_string(), "no conflict found for path 'other.txt'");
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
fn abort_merge_keeps_unrelated_unstaged_edits() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\n");
    write_file(dir.path(), "other.txt", "orig\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "shared.txt", "feature\n");
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "shared.txt", "main\n");
    commit_all(&repo, "main commit");
    write_file(dir.path(), "other.txt", "my unsaved work\n");
    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();
    assert!(repo.index().unwrap().has_conflicts());

    git_core::merge::abort_merge(&repo).unwrap();

    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("other.txt")).unwrap(),
        "my unsaved work\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
        "main\n"
    );
}

#[test]
fn abort_merge_errors_and_leaves_uncommitted_work_untouched_when_no_merge_is_in_progress() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    write_file(dir.path(), "base.txt", "v1\nuncommitted change\n");

    let result = git_core::merge::abort_merge(&repo);

    assert!(result.is_err());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    let contents = std::fs::read_to_string(dir.path().join("base.txt")).unwrap();
    assert_eq!(contents, "v1\nuncommitted change\n");
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

    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();

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
    let oid = git_core::commit::commit(
        &mut repo,
        "merge feature into base",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    let commit = repo
        .find_commit(git2::Oid::from_str(&oid).unwrap())
        .unwrap();
    assert_eq!(commit.parent_count(), 2);
}

use git_core::merge::FileConflictChoice;

fn make_delete_modify_conflict() -> (tempfile::TempDir, git2::Repository) {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "shared.txt", "v1\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();

    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "shared.txt", "v2\n");
    commit_all(&repo, "feature commit modifies");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    std::fs::remove_file(dir.path().join("shared.txt")).unwrap();
    git_core::stage::stage_file(&repo, "shared.txt").unwrap();
    git_core::commit::commit(
        &mut repo,
        "main commit deletes",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();
    (dir, repo)
}

#[test]
fn resolve_add_delete_conflict_with_theirs_restores_the_modified_file() {
    let (dir, repo) = make_delete_modify_conflict();

    git_core::merge::resolve_add_delete_conflict(&repo, "shared.txt", FileConflictChoice::Theirs)
        .unwrap();

    assert!(!repo.index().unwrap().has_conflicts());
    let contents = std::fs::read_to_string(dir.path().join("shared.txt")).unwrap();
    assert_eq!(contents, "v2\n");
}

#[test]
fn resolve_add_delete_conflict_with_ours_keeps_the_file_deleted() {
    let (dir, repo) = make_delete_modify_conflict();

    git_core::merge::resolve_add_delete_conflict(&repo, "shared.txt", FileConflictChoice::Ours)
        .unwrap();

    assert!(!repo.index().unwrap().has_conflicts());
    assert!(!dir.path().join("shared.txt").exists());
}

#[test]
fn resolve_add_delete_conflict_with_delete_removes_the_file_regardless() {
    let (dir, repo) = make_delete_modify_conflict();

    git_core::merge::resolve_add_delete_conflict(&repo, "shared.txt", FileConflictChoice::Delete)
        .unwrap();

    assert!(!repo.index().unwrap().has_conflicts());
    assert!(!dir.path().join("shared.txt").exists());
}

#[test]
fn start_merge_refuses_when_already_merging_and_leaves_conflict_state_untouched() {
    let (_dir, repo) = make_conflicted_repo();
    assert!(git_core::merge::is_merging(&repo));
    assert!(repo.index().unwrap().has_conflicts());

    // Try to start a second merge (e.g. clicking "Merge into current branch" on a different
    // branch) while the first is still conflicted.
    let result =
        git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default());

    assert!(result.is_err());
    // The refused attempt must not itself corrupt anything: still merging, still conflicted.
    assert!(git_core::merge::is_merging(&repo));
    assert!(repo.index().unwrap().has_conflicts());
}

#[test]
fn resolve_add_delete_conflict_on_a_genuine_text_conflict_is_rejected() {
    let (_dir, repo) = make_conflicted_repo();

    let result =
        git_core::merge::resolve_add_delete_conflict(&repo, "shared.txt", FileConflictChoice::Ours);

    assert!(result.is_err());
}

#[test]
fn resolve_add_delete_conflict_then_commit_has_two_parents() {
    let (_dir, repo) = make_delete_modify_conflict();
    git_core::merge::resolve_add_delete_conflict(&repo, "shared.txt", FileConflictChoice::Theirs)
        .unwrap();

    let mut repo = repo;
    let oid = git_core::commit::commit(
        &mut repo,
        "merge feature into main",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    let commit = repo
        .find_commit(git2::Oid::from_str(&oid).unwrap())
        .unwrap();
    assert_eq!(commit.parent_count(), 2);
}

use git_core::merge::{FastForward, MergeOptions};

fn opts(fast_forward: FastForward) -> MergeOptions {
    MergeOptions {
        fast_forward,
        squash: false,
    }
}

/// Base commit; `feature` adds `feature.txt` ("feature commit"); the main branch adds
/// `main.txt` ("main commit") and is checked out. Returns the main branch name.
fn diverged_repo() -> (tempfile::TempDir, git2::Repository, String) {
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
    (dir, repo, main_branch)
}

/// `feature` is one commit ahead of the checked-out main branch. Returns the main branch name
/// and the feature tip.
fn fast_forwardable_repo() -> (tempfile::TempDir, git2::Repository, String, git2::Oid) {
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
    (dir, repo, main_branch, feature_tip)
}

#[test]
fn never_creates_a_merge_commit_even_when_fast_forward_is_possible() {
    let (_dir, mut repo, _main, feature_tip) = fast_forwardable_repo();
    let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();

    let outcome = git_core::merge::start_merge(&repo, "feature", opts(FastForward::Never)).unwrap();

    assert_eq!(outcome, MergeOutcome::Merged);
    assert_eq!(repo.state(), git2::RepositoryState::Merge);
    assert_eq!(
        repo.head().unwrap().peel_to_commit().unwrap().id(),
        head_before
    );
    git_core::commit::commit(
        &mut repo,
        "Merge feature",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();
    let merge = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        merge.parent_ids().collect::<Vec<_>>(),
        vec![head_before, feature_tip]
    );
}

#[test]
fn only_fast_forwards_when_possible() {
    let (_dir, repo, _main, feature_tip) = fast_forwardable_repo();

    let outcome = git_core::merge::start_merge(&repo, "feature", opts(FastForward::Only)).unwrap();

    assert_eq!(outcome, MergeOutcome::FastForwarded);
    assert_eq!(
        repo.head().unwrap().peel_to_commit().unwrap().id(),
        feature_tip
    );
}

#[test]
fn only_refuses_a_diverged_branch_and_changes_nothing() {
    let (dir, repo, _main) = diverged_repo();
    let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();

    let error =
        git_core::merge::start_merge(&repo, "feature", opts(FastForward::Only)).unwrap_err();

    assert_eq!(
        error.to_string(),
        "Can't fast-forward: 'feature' has diverged from the current branch."
    );
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    assert_eq!(
        repo.head().unwrap().peel_to_commit().unwrap().id(),
        head_before
    );
    assert!(git_core::status::status(&repo).unwrap().is_empty());
    assert!(!dir.path().join("feature.txt").exists());
}

#[test]
fn only_names_a_remote_tracking_target_without_its_ref_prefix() {
    let (_dir, repo, _main) = diverged_repo();
    let feature_tip = repo
        .find_reference("refs/heads/feature")
        .unwrap()
        .target()
        .unwrap();
    repo.reference("refs/remotes/origin/feature", feature_tip, true, "test")
        .unwrap();

    let error = git_core::merge::start_merge(
        &repo,
        "refs/remotes/origin/feature",
        opts(FastForward::Only),
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "Can't fast-forward: 'origin/feature' has diverged from the current branch."
    );
}

#[test]
fn start_merge_accepts_a_full_remote_tracking_ref() {
    let (dir, repo, _main) = diverged_repo();
    let feature_tip = repo
        .find_reference("refs/heads/feature")
        .unwrap()
        .target()
        .unwrap();
    repo.reference("refs/remotes/origin/feature", feature_tip, true, "test")
        .unwrap();

    let outcome = git_core::merge::start_merge(
        &repo,
        "refs/remotes/origin/feature",
        MergeOptions::default(),
    )
    .unwrap();

    assert_eq!(outcome, MergeOutcome::Merged);
    assert!(dir.path().join("feature.txt").exists());
}

#[test]
fn merge_defaults_follow_merge_ff_in_the_repo_config() {
    let (_dir, repo) = init_repo();
    // Set in the repo's own config so a global `merge.ff` cannot leak in (issue #111).
    repo.config().unwrap().set_str("merge.ff", "false").unwrap();
    assert_eq!(
        git_core::merge::merge_defaults(&repo),
        opts(FastForward::Never)
    );
    repo.config().unwrap().set_str("merge.ff", "only").unwrap();
    assert_eq!(
        git_core::merge::merge_defaults(&repo),
        opts(FastForward::Only)
    );
    repo.config().unwrap().set_str("merge.ff", "true").unwrap();
    assert_eq!(
        git_core::merge::merge_defaults(&repo),
        opts(FastForward::Allow)
    );
}

#[test]
fn merge_ff_values_map_to_fast_forward_modes() {
    use git_core::merge::fast_forward_from_merge_ff as map;
    assert_eq!(map(None), FastForward::Allow);
    assert_eq!(map(Some("true")), FastForward::Allow);
    assert_eq!(map(Some("only")), FastForward::Only);
    assert_eq!(map(Some("ONLY")), FastForward::Only);
    for never in ["false", "no", "off", "0", "FALSE"] {
        assert_eq!(map(Some(never)), FastForward::Never, "{never}");
    }
    assert_eq!(map(Some("garbage")), FastForward::Allow);
}

fn squash() -> MergeOptions {
    MergeOptions {
        fast_forward: FastForward::Allow,
        squash: true,
    }
}

#[test]
fn squash_stages_the_changes_writes_squash_msg_and_leaves_no_merge_state() {
    let (dir, repo, _main) = diverged_repo();
    let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();
    let feature_tip = repo
        .find_reference("refs/heads/feature")
        .unwrap()
        .target()
        .unwrap();

    let outcome = git_core::merge::start_merge(&repo, "feature", squash()).unwrap();

    assert_eq!(outcome, MergeOutcome::Squashed);
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    assert!(!repo.path().join("MERGE_HEAD").exists());
    assert!(!repo.path().join("MERGE_MSG").exists());
    assert_eq!(
        repo.head().unwrap().peel_to_commit().unwrap().id(),
        head_before
    );
    let staged: Vec<_> = git_core::status::status(&repo)
        .unwrap()
        .into_iter()
        .filter(|e| e.staged)
        .collect();
    assert_eq!(staged.len(), 1);
    assert_eq!(staged[0].path, "feature.txt");
    assert!(dir.path().join("feature.txt").exists());
    let msg = git_core::merge::squash_message(&repo).unwrap();
    assert!(msg.starts_with("Squashed commit of the following:\n\ncommit "));
    assert!(msg.contains(&format!(
        "commit {feature_tip}\nAuthor: Test User <test@example.com>\n\n    feature commit\n"
    )));
    assert!(!msg.contains("main commit"));
    let info = git_core::repo::head_info(&repo).unwrap();
    assert_eq!(info.operation, None);
    assert_eq!(info.squash_message.as_deref(), Some(msg.as_str()));
}

#[test]
fn squash_message_lists_every_squashed_commit_newest_first() {
    let (dir, repo, main) = diverged_repo();
    git_core::branch::switch_branch(&repo, "feature").unwrap();
    write_file(dir.path(), "feature2.txt", "more\n");
    commit_all(&repo, "second feature commit\n\nwith a body");
    git_core::branch::switch_branch(&repo, &main).unwrap();

    git_core::merge::start_merge(&repo, "feature", squash()).unwrap();

    let msg = git_core::merge::squash_message(&repo).unwrap();
    let second = msg.find("    second feature commit\n    \n    with a body\n");
    let first = msg.find("    feature commit\n");
    assert!(second.is_some() && first.is_some(), "{msg}");
    assert!(second < first, "{msg}");
}

#[test]
fn squash_does_not_move_head_even_when_fast_forward_is_possible() {
    let (_dir, repo, _main, _feature_tip) = fast_forwardable_repo();
    let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();

    let outcome = git_core::merge::start_merge(&repo, "feature", squash()).unwrap();

    assert_eq!(outcome, MergeOutcome::Squashed);
    assert_eq!(
        repo.head().unwrap().peel_to_commit().unwrap().id(),
        head_before
    );
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    assert!(git_core::status::status(&repo)
        .unwrap()
        .iter()
        .any(|e| e.path == "feature.txt" && e.staged));
}

#[test]
fn squash_when_up_to_date_is_up_to_date_and_writes_nothing() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "v1\n");
    commit_all(&repo, "base commit");
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();

    let outcome = git_core::merge::start_merge(&repo, "feature", squash()).unwrap();

    assert_eq!(outcome, MergeOutcome::UpToDate);
    assert_eq!(git_core::merge::squash_message(&repo), None);
}

#[test]
fn squash_with_a_conflict_reports_it_and_leaves_clean_state() {
    let (_dir, repo) = make_conflicting_branches();

    let outcome = git_core::merge::start_merge(&repo, "feature", squash()).unwrap();

    assert_eq!(
        outcome,
        MergeOutcome::Conflicted {
            files: vec!["shared.txt".into()]
        }
    );
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    assert!(repo.index().unwrap().has_conflicts());
    assert!(git_core::merge::squash_message(&repo).is_some());
    assert!(!repo.path().join("MERGE_HEAD").exists());
}

#[test]
fn commit_after_squash_has_one_parent_and_clears_squash_msg() {
    let (_dir, mut repo, _main) = diverged_repo();
    let head_before = repo.head().unwrap().peel_to_commit().unwrap().id();
    git_core::merge::start_merge(&repo, "feature", squash()).unwrap();

    git_core::commit::commit(
        &mut repo,
        "Squash feature",
        git_core::hooks::HookOptions::default(),
    )
    .unwrap();

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.parent_ids().collect::<Vec<_>>(), vec![head_before]);
    assert_eq!(git_core::merge::squash_message(&repo), None);
}

#[test]
fn abort_after_a_conflicted_squash_clears_squash_msg() {
    let (dir, repo) = make_conflicting_branches();
    git_core::merge::start_merge(&repo, "feature", squash()).unwrap();

    git_core::merge::abort_merge(&repo).unwrap();

    assert_eq!(git_core::merge::squash_message(&repo), None);
    assert!(!repo.index().unwrap().has_conflicts());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
        "line one\nmain two\nline three\n"
    );
}

// --- Whole-file resolution (batch 4g) ---

use git_core::merge::{ConflictSide, MergeError};

fn staged_blob(repo: &git2::Repository, path: &str) -> Option<Vec<u8>> {
    let index = repo.index().unwrap();
    index
        .get_path(std::path::Path::new(path), 0)
        .map(|entry| repo.find_blob(entry.id).unwrap().content().to_vec())
}

#[test]
fn resolve_whole_file_takes_ours_or_theirs_for_a_content_conflict() {
    for (side, expected) in [
        (ConflictSide::Ours, "line one\nmain two\nline three\n"),
        (ConflictSide::Theirs, "line one\nfeature two\nline three\n"),
    ] {
        let (dir, repo) = make_conflicted_repo();
        git_core::merge::resolve_whole_file(&repo, "shared.txt", side).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
            expected,
            "{side:?}"
        );
        assert!(!repo.index().unwrap().has_conflicts());
        assert_eq!(
            staged_blob(&repo, "shared.txt").unwrap(),
            expected.as_bytes()
        );
    }
}

#[test]
fn resolve_whole_file_on_a_modify_delete_conflict_deletes_or_keeps() {
    // ours (main) deleted shared.txt, theirs (feature) changed it to "v2\n".
    let (dir, repo) = make_delete_modify_conflict();
    git_core::merge::resolve_whole_file(&repo, "shared.txt", ConflictSide::Ours).unwrap();
    assert!(!dir.path().join("shared.txt").exists());
    assert!(staged_blob(&repo, "shared.txt").is_none());
    assert!(!repo.index().unwrap().has_conflicts());

    let (dir, repo) = make_delete_modify_conflict();
    git_core::merge::resolve_whole_file(&repo, "shared.txt", ConflictSide::Theirs).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
        "v2\n"
    );
    assert_eq!(staged_blob(&repo, "shared.txt").unwrap(), b"v2\n");
    assert!(!repo.index().unwrap().has_conflicts());
}

#[test]
fn resolve_whole_file_rejects_a_path_with_no_conflict() {
    let (_dir, repo) = make_conflicted_repo();
    assert!(matches!(
        git_core::merge::resolve_whole_file(&repo, "nope.txt", ConflictSide::Ours),
        Err(MergeError::NoConflict(_))
    ));
    assert!(matches!(
        git_core::merge::resolve_whole_file(&repo, "../shared.txt", ConflictSide::Ours),
        Err(MergeError::NoConflict(_))
    ));
}

#[test]
fn resolve_whole_file_leaves_other_conflicts_alone() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "base\n");
    write_file(dir.path(), "b.txt", "base\n");
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "a.txt", "feature\n");
    write_file(dir.path(), "b.txt", "feature\n");
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "a.txt", "main\n");
    write_file(dir.path(), "b.txt", "main\n");
    commit_all(&repo, "main commit");
    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();
    let b_before = std::fs::read_to_string(dir.path().join("b.txt")).unwrap();

    git_core::merge::resolve_whole_file(&repo, "a.txt", ConflictSide::Theirs).unwrap();

    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "feature\n"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b.txt")).unwrap(),
        b_before
    );
    assert!(git_core::merge::conflict_hunks(&repo, "b.txt").is_ok());
}

#[test]
fn resolve_whole_file_resolves_a_stash_pop_conflict() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "a.txt", "base\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "a.txt", "stashed\n");
    git_core::stash::save_stash(&mut repo, &git_core::stash::StashOptions::default()).unwrap();
    write_file(dir.path(), "a.txt", "committed\n");
    commit_all(&repo, "committed");
    let outcome = git_core::stash::pop_stash(&mut repo, 0, None, false).unwrap();
    assert!(matches!(
        outcome,
        git_core::stash::StashApplyOutcome::Conflicted { .. }
    ));

    git_core::merge::resolve_whole_file(&repo, "a.txt", ConflictSide::Theirs).unwrap();

    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "stashed\n"
    );
    assert!(!repo.index().unwrap().has_conflicts());
}

#[cfg(unix)]
#[test]
fn resolve_whole_file_keeps_the_chosen_sides_executable_bit() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = init_repo();
    let script = dir.path().join("run.sh");
    write_file(dir.path(), "run.sh", "echo base\n");
    commit_all(&repo, "base");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "run.sh", "echo feature\n");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "run.sh", "echo main\n");
    commit_all(&repo, "main");
    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();
    assert!(
        repo.index().unwrap().has_conflicts(),
        "fixture must conflict"
    );

    git_core::merge::resolve_whole_file(&repo, "run.sh", ConflictSide::Theirs).unwrap();
    assert_eq!(std::fs::read_to_string(&script).unwrap(), "echo feature\n");
    assert_eq!(
        std::fs::metadata(&script).unwrap().permissions().mode() & 0o111,
        0o111
    );
    let index = repo.index().unwrap();
    assert_eq!(
        index
            .get_path(std::path::Path::new("run.sh"), 0)
            .unwrap()
            .mode,
        0o100755
    );
}

#[cfg(unix)]
#[test]
fn resolve_whole_file_refuses_a_symlink_conflict() {
    let (dir, repo) = init_repo();
    std::os::unix::fs::symlink("a", dir.path().join("link")).unwrap();
    commit_all(&repo, "base");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    std::fs::remove_file(dir.path().join("link")).unwrap();
    std::os::unix::fs::symlink("b", dir.path().join("link")).unwrap();
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    std::fs::remove_file(dir.path().join("link")).unwrap();
    std::os::unix::fs::symlink("c", dir.path().join("link")).unwrap();
    commit_all(&repo, "main");
    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();
    assert!(
        repo.index().unwrap().has_conflicts(),
        "fixture must conflict"
    );

    assert!(matches!(
        git_core::merge::resolve_whole_file(&repo, "link", ConflictSide::Ours),
        Err(MergeError::UnsupportedEntry(path)) if path == "link"
    ));
    assert!(repo.index().unwrap().has_conflicts());
    assert_eq!(
        MergeError::UnsupportedEntry("link".into()).to_string(),
        "'link' is a symlink or submodule conflict. Resolve it with git."
    );
}

#[test]
fn resolve_whole_file_handles_a_binary_conflict() {
    let (dir, repo) = init_repo();
    std::fs::write(dir.path().join("img.bin"), b"\0base\0").unwrap();
    commit_all(&repo, "base");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    std::fs::write(dir.path().join("img.bin"), b"\0feature\0").unwrap();
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    std::fs::write(dir.path().join("img.bin"), b"\0main\0").unwrap();
    commit_all(&repo, "main");
    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();
    assert!(
        repo.index().unwrap().has_conflicts(),
        "fixture must conflict"
    );

    git_core::merge::resolve_whole_file(&repo, "img.bin", ConflictSide::Theirs).unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("img.bin")).unwrap(),
        b"\0feature\0"
    );
    assert_eq!(staged_blob(&repo, "img.bin").unwrap(), b"\0feature\0");
    assert!(!repo.index().unwrap().has_conflicts());
}

#[test]
fn resolve_whole_file_handles_an_add_add_conflict() {
    for (side, expected) in [
        (ConflictSide::Ours, "main\n"),
        (ConflictSide::Theirs, "feature\n"),
    ] {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "base.txt", "base\n");
        commit_all(&repo, "base");
        let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
            .name
            .clone();
        git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
        write_file(dir.path(), "new.txt", "feature\n");
        commit_all(&repo, "feature adds");
        git_core::branch::switch_branch(&repo, &main_branch).unwrap();
        write_file(dir.path(), "new.txt", "main\n");
        commit_all(&repo, "main adds");
        git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
            .unwrap();
        assert!(
            repo.index().unwrap().has_conflicts(),
            "fixture must conflict"
        );

        git_core::merge::resolve_whole_file(&repo, "new.txt", side).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("new.txt")).unwrap(),
            expected
        );
        assert!(!repo.index().unwrap().has_conflicts());
    }
}

/// `sub/shared.txt` conflicts; then `sub` is replaced by a symlink to a folder outside the
/// repository. No resolution may write or delete through it.
#[cfg(unix)]
#[test]
fn conflict_resolution_never_writes_through_a_symlinked_folder() {
    let outside = tempfile::TempDir::new().unwrap();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "sub/shared.txt", "base\n");
    commit_all(&repo, "base");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "sub/shared.txt", "feature\n");
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "sub/shared.txt", "main\n");
    commit_all(&repo, "main");
    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();
    std::fs::remove_dir_all(dir.path().join("sub")).unwrap();
    std::fs::write(outside.path().join("shared.txt"), "outside\n").unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("sub")).unwrap();

    for side in [ConflictSide::Ours, ConflictSide::Theirs] {
        assert!(matches!(
            git_core::merge::resolve_whole_file(&repo, "sub/shared.txt", side),
            Err(MergeError::SymlinkedFolder(_))
        ));
    }
    assert!(matches!(
        git_core::merge::resolve_conflict(&repo, "sub/shared.txt", "x\n"),
        Err(MergeError::SymlinkedFolder(_))
    ));
    assert!(matches!(
        git_core::merge::mark_resolved(&repo, "sub/shared.txt", true),
        Err(MergeError::SymlinkedFolder(_))
    ));
    assert_eq!(
        std::fs::read_to_string(outside.path().join("shared.txt")).unwrap(),
        "outside\n"
    );
    assert!(repo.index().unwrap().has_conflicts());
}

/// `shared.txt` conflicts; `feature` also changes `tc` and `d/f` and adds `d/new.txt`, which
/// merge cleanly. The merge is stopped on the conflict.
#[cfg(unix)]
fn stopped_merge_with_clean_paths() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "shared.txt", "line one\nline two\nline three\n");
    write_file(dir.path(), "tc", "plain\n");
    write_file(dir.path(), "d/f", "inner\n");
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
    write_file(dir.path(), "tc", "feature\n");
    write_file(dir.path(), "d/f", "feature inner\n");
    write_file(dir.path(), "d/new.txt", "new\n");
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "shared.txt", "line one\nmain two\nline three\n");
    commit_all(&repo, "main commit");
    let outcome =
        git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
            .unwrap();
    assert!(matches!(outcome, MergeOutcome::Conflicted { .. }));
    (dir, repo)
}

#[test]
#[cfg(unix)]
fn abort_merge_restores_a_conflicted_path_replaced_by_a_symlink() {
    let out = common::outside_dir();
    let (dir, repo) = stopped_merge_with_clean_paths();
    common::replace_with_link(dir.path(), "shared.txt", &out.path().join("target.txt"));

    git_core::merge::abort_merge(&repo).unwrap();

    common::assert_outside_untouched(out.path());
    assert!(!common::is_link(dir.path(), "shared.txt"));
    assert_eq!(
        common::read(dir.path(), "shared.txt"),
        "line one\nmain two\nline three\n"
    );
}

#[test]
#[cfg(unix)]
fn abort_merge_restores_a_merged_path_replaced_by_a_symlink() {
    let out = common::outside_dir();
    let (dir, repo) = stopped_merge_with_clean_paths();
    common::replace_with_link(dir.path(), "tc", &out.path().join("target.txt"));

    git_core::merge::abort_merge(&repo).unwrap();

    common::assert_outside_untouched(out.path());
    assert_eq!(common::read(dir.path(), "tc"), "plain\n");
}

#[test]
#[cfg(unix)]
fn abort_merge_restores_a_directory_replaced_by_a_symlink() {
    let out = common::outside_dir();
    let (dir, repo) = stopped_merge_with_clean_paths();
    common::replace_with_link(dir.path(), "d", out.path());

    git_core::merge::abort_merge(&repo).unwrap();

    common::assert_outside_untouched(out.path());
    assert!(!common::is_link(dir.path(), "d"));
    assert_eq!(common::read(dir.path(), "d/f"), "inner\n");
}

#[test]
#[cfg(unix)]
fn abort_merge_never_deletes_an_added_file_through_a_symlinked_directory() {
    let out = common::outside_dir();
    std::fs::write(out.path().join("new.txt"), "OUTSIDE\n").unwrap();
    let (dir, repo) = stopped_merge_with_clean_paths();
    common::replace_with_link(dir.path(), "d", out.path());

    git_core::merge::abort_merge(&repo).unwrap();

    common::assert_outside_untouched(out.path());
    assert!(out.path().join("new.txt").exists());
}

#[test]
#[cfg(unix)]
fn resolve_conflict_replaces_a_symlink_instead_of_writing_through_it() {
    let out = common::outside_dir();
    let (dir, repo) = stopped_merge_with_clean_paths();
    common::replace_with_link(dir.path(), "shared.txt", &out.path().join("target.txt"));

    git_core::merge::resolve_conflict(&repo, "shared.txt", "resolved\n").unwrap();

    common::assert_outside_untouched(out.path());
    assert!(!common::is_link(dir.path(), "shared.txt"));
    assert_eq!(common::read(dir.path(), "shared.txt"), "resolved\n");
    assert!(!repo.index().unwrap().has_conflicts());
}

#[test]
#[cfg(unix)]
fn resolve_add_delete_conflict_replaces_a_symlink() {
    let out = common::outside_dir();
    let (dir, repo) = make_delete_modify_conflict();
    common::replace_with_link(dir.path(), "shared.txt", &out.path().join("target.txt"));

    git_core::merge::resolve_add_delete_conflict(&repo, "shared.txt", FileConflictChoice::Theirs)
        .unwrap();

    common::assert_outside_untouched(out.path());
    assert!(!common::is_link(dir.path(), "shared.txt"));
    assert_eq!(common::read(dir.path(), "shared.txt"), "v2\n");
}

#[test]
#[cfg(unix)]
fn resolve_whole_file_replaces_a_symlink() {
    let out = common::outside_dir();
    let (dir, repo) = stopped_merge_with_clean_paths();
    common::replace_with_link(dir.path(), "shared.txt", &out.path().join("target.txt"));

    git_core::merge::resolve_whole_file(&repo, "shared.txt", git_core::merge::ConflictSide::Theirs)
        .unwrap();

    common::assert_outside_untouched(out.path());
    assert!(!common::is_link(dir.path(), "shared.txt"));
    assert_eq!(
        common::read(dir.path(), "shared.txt"),
        "line one\nfeature two\nline three\n"
    );
}
