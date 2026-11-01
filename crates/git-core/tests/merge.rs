mod common;

use std::path::Path;

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

/// #165: the marker check reads through the opened handle, so a FIFO (swapped in at any moment)
/// is never opened for reading and can't hang the worker.
#[cfg(unix)]
#[test]
fn mark_resolved_does_not_block_on_a_fifo() {
    let (dir, repo) = make_conflicted_repo();
    let fifo = dir.path().join("shared.txt");
    std::fs::remove_file(&fifo).unwrap();
    let c_path = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
    // SAFETY: a valid NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o644) }, 0);
    let path = dir.path().to_path_buf();
    drop(repo);
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let repo = git2::Repository::open(&path).unwrap();
        let result = git_core::merge::mark_resolved(&repo, "shared.txt", false);
        let _ = tx.send(matches!(
            result,
            Err(git_core::merge::MergeError::NotAFile(ref path)) if path == "shared.txt"
        ));
    });
    let finished = rx.recv_timeout(std::time::Duration::from_secs(5));
    if finished.is_err() {
        // Unblock the stuck reader so the test process can exit.
        let _ = std::fs::OpenOptions::new().write(true).open(&fifo);
    }
    assert_eq!(
        finished,
        Ok(true),
        "mark_resolved blocked on or staged a FIFO"
    );
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert!(repo.index().unwrap().has_conflicts());
    assert_eq!(
        git_core::merge::MergeError::NotAFile("shared.txt".into()).to_string(),
        "'shared.txt' is a FIFO, socket or device, not a regular file; replace it before \
         marking it resolved."
    );
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

/// Mode and content of `path`'s stage-0 index entry.
fn staged(repo: &git2::Repository, path: &str) -> (u32, Vec<u8>) {
    let entry = repo
        .index()
        .unwrap()
        .get_path(std::path::Path::new(path), 0)
        .expect("a stage-0 entry");
    (
        entry.mode,
        repo.find_blob(entry.id).unwrap().content().to_vec(),
    )
}

/// #199: the content is rewritten (with conflict markers) after the check; what was checked is
/// what gets staged, not the new content read a second time by path.
#[test]
fn mark_resolved_stages_the_content_it_checked() {
    let (dir, repo) = make_conflicted_repo();
    write_file(dir.path(), "shared.txt", "resolved\n");
    let root = dir.path().to_path_buf();
    let outcome =
        git_core::merge::mark_resolved_with_pause(&repo, "shared.txt", false, &mut || {
            std::fs::write(root.join("shared.txt"), "<<<<<<< x\nours\n").unwrap();
        })
        .unwrap();
    assert_eq!(outcome, git_core::merge::MarkResolvedOutcome::Resolved);
    assert!(!repo.index().unwrap().has_conflicts());
    assert_eq!(
        staged(&repo, "shared.txt"),
        (0o100644, b"resolved\n".to_vec())
    );
}

/// #199: a FIFO swapped in after the check is never opened for reading, so the worker can't
/// hang on it.
#[cfg(unix)]
#[test]
fn mark_resolved_does_not_block_on_a_fifo_swapped_in_after_the_check() {
    let (dir, repo) = make_conflicted_repo();
    write_file(dir.path(), "shared.txt", "resolved\n");
    let fifo = dir.path().join("shared.txt");
    let path = dir.path().to_path_buf();
    drop(repo);
    let (tx, rx) = std::sync::mpsc::channel();
    let swap = fifo.clone();
    std::thread::spawn(move || {
        let repo = git2::Repository::open(&path).unwrap();
        let result =
            git_core::merge::mark_resolved_with_pause(&repo, "shared.txt", false, &mut || {
                std::fs::remove_file(&swap).unwrap();
                let c_path = std::ffi::CString::new(swap.to_str().unwrap()).unwrap();
                // SAFETY: a valid NUL-terminated path.
                assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o644) }, 0);
            });
        let _ = tx.send(result.map_err(|error| error.to_string()));
    });
    let finished = rx.recv_timeout(std::time::Duration::from_secs(5));
    if finished.is_err() {
        // Unblock the stuck reader so the test process can exit.
        let _ = std::fs::OpenOptions::new().write(true).open(&fifo);
    }
    assert_eq!(
        finished,
        Ok(Ok(git_core::merge::MarkResolvedOutcome::Resolved)),
        "mark_resolved blocked on a FIFO swapped in after the check"
    );
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert_eq!(
        staged(&repo, "shared.txt"),
        (0o100644, b"resolved\n".to_vec())
    );
}

/// #199: a link swapped in after the check isn't staged in place of the file that was checked.
#[cfg(unix)]
#[test]
fn mark_resolved_does_not_stage_a_link_swapped_in_after_the_check() {
    let outside = common::outside_dir();
    let (dir, repo) = make_conflicted_repo();
    write_file(dir.path(), "shared.txt", "resolved\n");
    let file = dir.path().join("shared.txt");
    let target = outside.path().join("f");
    let outcome =
        git_core::merge::mark_resolved_with_pause(&repo, "shared.txt", false, &mut || {
            std::fs::remove_file(&file).unwrap();
            std::os::unix::fs::symlink(&target, &file).unwrap();
        })
        .unwrap();
    assert_eq!(outcome, git_core::merge::MarkResolvedOutcome::Resolved);
    assert_eq!(
        staged(&repo, "shared.txt"),
        (0o100644, b"resolved\n".to_vec())
    );
}

/// Staging what was read keeps `git add`'s clean filters (here `core.autocrlf`).
#[test]
fn mark_resolved_applies_the_clean_filter() {
    let (dir, repo) = make_conflicted_repo();
    repo.config()
        .unwrap()
        .set_bool("core.autocrlf", true)
        .unwrap();
    write_file(dir.path(), "shared.txt", "a\r\nb\r\n");
    let repo = {
        drop(repo);
        git2::Repository::open(dir.path()).unwrap()
    };
    let outcome = git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    assert_eq!(outcome, git_core::merge::MarkResolvedOutcome::Resolved);
    assert_eq!(staged(&repo, "shared.txt").1, b"a\nb\n");
}

#[cfg(unix)]
#[test]
fn mark_resolved_keeps_the_exec_bit() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = make_conflicted_repo();
    repo.config()
        .unwrap()
        .set_bool("core.filemode", true)
        .unwrap();
    write_file(dir.path(), "shared.txt", "resolved\n");
    std::fs::set_permissions(
        dir.path().join("shared.txt"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    let repo = {
        drop(repo);
        git2::Repository::open(dir.path()).unwrap()
    };
    git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    assert_eq!(staged(&repo, "shared.txt").0, 0o100755);
}

/// With `core.filemode=false` the exec bit on disk is ignored and "ours"' mode kept, as git does.
#[cfg(unix)]
#[test]
fn mark_resolved_keeps_ours_mode_without_filemode() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = init_repo();
    repo.config()
        .unwrap()
        .set_bool("core.filemode", true)
        .unwrap();
    let file = dir.path().join("shared.txt");
    write_file(dir.path(), "shared.txt", "base\n");
    commit_all(&repo, "base");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), "shared.txt", "feature\n");
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), "shared.txt", "main\n");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    commit_all(&repo, "main");
    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();
    repo.config()
        .unwrap()
        .set_bool("core.filemode", false)
        .unwrap();
    write_file(dir.path(), "shared.txt", "resolved\n");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let repo = {
        drop(repo);
        git2::Repository::open(dir.path()).unwrap()
    };
    git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    assert_eq!(
        staged(&repo, "shared.txt"),
        (0o100755, b"resolved\n".to_vec())
    );
}

/// `shared.txt` deleted on the checked-out branch; on `feature` changed by `change` (run with
/// the work tree at `feature`). The merge stops on the modify/delete conflict, so there is no
/// "ours" entry, only the base's and "theirs"'.
#[cfg(unix)]
fn deleted_by_us(change: impl FnOnce(&Path)) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    {
        let mut config = repo.config().unwrap();
        config.set_bool("core.filemode", true).unwrap();
        config.set_bool("core.symlinks", true).unwrap();
    }
    write_file(dir.path(), "shared.txt", "base\n");
    write_file(dir.path(), "other.txt", "x\n");
    commit_all(&repo, "base");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    change(dir.path());
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    let mut index = repo.index().unwrap();
    index.remove_path(Path::new("shared.txt")).unwrap();
    index.write().unwrap();
    let _ = std::fs::remove_file(dir.path().join("shared.txt"));
    commit_all(&repo, "delete");
    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();
    assert!(repo.index().unwrap().has_conflicts());
    (dir, repo)
}

/// With `core.filemode=false` and no "ours" entry, "theirs"' mode is kept, as libgit2's
/// `index_existing_and_best` (and git) do.
#[cfg(unix)]
#[test]
fn mark_resolved_keeps_theirs_mode_when_ours_deleted_it() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, repo) = deleted_by_us(|root| {
        std::fs::write(root.join("shared.txt"), "theirs\n").unwrap();
        std::fs::set_permissions(
            root.join("shared.txt"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    });
    repo.config()
        .unwrap()
        .set_bool("core.filemode", false)
        .unwrap();
    write_file(dir.path(), "shared.txt", "resolved\n");
    std::fs::set_permissions(
        dir.path().join("shared.txt"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let repo = {
        drop(repo);
        git2::Repository::open(dir.path()).unwrap()
    };
    git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    assert_eq!(
        staged(&repo, "shared.txt"),
        (0o100755, b"resolved\n".to_vec())
    );
}

/// With `core.symlinks=false`, a file standing for "theirs"' link stays a link, and its content
/// goes through the clean filters, as git does.
#[cfg(unix)]
#[test]
fn mark_resolved_keeps_theirs_link_mode_without_symlinks() {
    let (dir, repo) = deleted_by_us(|root| {
        std::fs::remove_file(root.join("shared.txt")).unwrap();
        std::os::unix::fs::symlink("elsewhere.txt", root.join("shared.txt")).unwrap();
    });
    {
        let mut config = repo.config().unwrap();
        config.set_bool("core.symlinks", false).unwrap();
        config.set_bool("core.autocrlf", true).unwrap();
    }
    let _ = std::fs::remove_file(dir.path().join("shared.txt"));
    write_file(dir.path(), "shared.txt", "elsewhere.txt\r\n");
    let repo = {
        drop(repo);
        git2::Repository::open(dir.path()).unwrap()
    };
    git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    assert_eq!(
        staged(&repo, "shared.txt"),
        (0o120000, b"elsewhere.txt\n".to_vec())
    );
}

/// The staged entry records the file's stat data (as `git add` does), so status doesn't
/// re-hash the file on every refresh.
#[test]
fn mark_resolved_records_the_files_stat_data() {
    let (dir, repo) = make_conflicted_repo();
    write_file(dir.path(), "shared.txt", "resolved\n");
    git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    let entry = repo
        .index()
        .unwrap()
        .get_path(Path::new("shared.txt"), 0)
        .unwrap();
    let meta = std::fs::metadata(dir.path().join("shared.txt")).unwrap();
    let mtime = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    assert_eq!(entry.mtime.seconds(), mtime.as_secs() as i32);
    assert_eq!(entry.mtime.nanoseconds(), mtime.subsec_nanos());
    assert_eq!(entry.file_size as u64, meta.len());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(entry.ino, meta.ino() as u32);
        assert_eq!(entry.ctime.seconds(), meta.ctime() as i32);
    }
}

/// A file Mark resolved can't open isn't called a FIFO, socket or device (#199).
#[cfg(unix)]
#[test]
fn mark_resolved_reports_a_file_it_cannot_read() {
    use std::os::unix::fs::PermissionsExt;
    // SAFETY: no arguments, no side effects.
    if unsafe { libc::geteuid() } == 0 {
        eprintln!("SKIP: root reads files without permission");
        return;
    }
    let (dir, repo) = make_conflicted_repo();
    let file = dir.path().join("shared.txt");
    write_file(dir.path(), "shared.txt", "resolved\n");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
    let result = git_core::merge::mark_resolved(&repo, "shared.txt", false);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    let error = result.unwrap_err();
    assert!(matches!(error, MergeError::Unreadable(_)), "{error:?}");
    assert_eq!(
        error.to_string(),
        "'shared.txt' can't be read safely; check its permissions and try again."
    );
    assert!(repo.index().unwrap().has_conflicts());
}

/// A folder swapped for a link after the read is refused as one, never looked through (#199).
#[cfg(unix)]
#[test]
fn mark_resolved_rechecks_the_folders_before_staging_a_deletion() {
    let outside = common::outside_dir();
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
    std::fs::remove_file(dir.path().join("sub/shared.txt")).unwrap();
    std::fs::write(outside.path().join("shared.txt"), "outside\n").unwrap();
    let sub = dir.path().join("sub");
    let target = outside.path().to_path_buf();
    let result =
        git_core::merge::mark_resolved_with_pause(&repo, "sub/shared.txt", false, &mut || {
            std::fs::remove_dir(&sub).unwrap();
            std::os::unix::fs::symlink(&target, &sub).unwrap();
        });
    assert!(
        matches!(result, Err(MergeError::SymlinkedFolder(_))),
        "{result:?}"
    );
    assert!(repo.index().unwrap().has_conflicts());
}

/// Like `git add`, Mark resolved keeps the conflict as resolve-undo data (`git checkout -m`).
#[test]
fn mark_resolved_records_resolve_undo() {
    let (dir, repo) = make_conflicted_repo();
    write_file(dir.path(), "shared.txt", "resolved\n");
    git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    let index = std::fs::read(repo.path().join("index")).unwrap();
    assert!(index.windows(4).any(|window| window == b"REUC"));
    // The entry's zeroed stat data only makes status re-hash the file: no work tree change.
    let status = repo
        .status_file(std::path::Path::new("shared.txt"))
        .unwrap();
    assert_eq!(status, git2::Status::INDEX_MODIFIED);
}

/// A conflicted file the user replaced with a link is staged as that link, as `git add` does.
#[cfg(unix)]
#[test]
fn mark_resolved_stages_a_link_as_a_link() {
    let (dir, repo) = make_conflicted_repo();
    let file = dir.path().join("shared.txt");
    std::fs::remove_file(&file).unwrap();
    std::os::unix::fs::symlink("elsewhere.txt", &file).unwrap();
    let outcome = git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    assert_eq!(outcome, git_core::merge::MarkResolvedOutcome::Resolved);
    assert_eq!(
        staged(&repo, "shared.txt"),
        (0o120000, b"elsewhere.txt".to_vec())
    );
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

/// Windows: `sub/shared.txt` conflicts; then `sub` is replaced by a junction to a folder
/// outside the repository. Mark resolved refuses it like a symlinked folder (#199).
#[cfg(windows)]
#[test]
fn mark_resolved_refuses_a_path_below_a_junction() {
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
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(dir.path().join("sub"))
        .arg(outside.path())
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());

    for force in [false, true] {
        assert!(matches!(
            git_core::merge::mark_resolved(&repo, "sub/shared.txt", force),
            Err(MergeError::SymlinkedFolder(_))
        ));
    }
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

/// Makes `path` read-only (the Windows attribute; the Unix write bits) until dropped, so a
/// failed assertion doesn't leave a read-only file the temp folder can't delete on Windows.
struct ReadOnly(std::path::PathBuf);

impl ReadOnly {
    fn new(path: &Path) -> Self {
        let mut permissions = std::fs::metadata(path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(path, permissions).unwrap();
        ReadOnly(path.to_path_buf())
    }
}

impl Drop for ReadOnly {
    fn drop(&mut self) {
        if let Ok(meta) = std::fs::symlink_metadata(&self.0) {
            let mut permissions = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            let _ = std::fs::set_permissions(&self.0, permissions);
        }
    }
}

/// Whether the test runs as root, which writes read-only files on Unix.
fn runs_as_root() -> bool {
    #[cfg(unix)]
    {
        // SAFETY: no arguments, no side effects.
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("SKIP: root writes read-only files");
            return true;
        }
    }
    false
}

/// The conflict stages of `path` as (ancestor, ours, theirs) blob ids.
fn conflict_stages(repo: &git2::Repository, path: &str) -> Vec<Option<git2::Oid>> {
    let index = repo.index().unwrap();
    let conflict = index
        .conflicts()
        .unwrap()
        .map(|c| c.unwrap())
        .find(|c| {
            [&c.ancestor, &c.our, &c.their]
                .iter()
                .any(|e| e.as_ref().is_some_and(|e| e.path == path.as_bytes()))
        })
        .expect("conflict");
    [conflict.ancestor, conflict.our, conflict.their]
        .into_iter()
        .map(|e| e.map(|e| e.id))
        .collect()
}

/// Resolving a conflict in a read-only file says so and changes nothing (#209).
#[test]
fn resolving_a_conflict_in_a_read_only_file_is_refused() {
    if runs_as_root() {
        return;
    }
    let (dir, repo) = make_conflicted_repo();
    let file = dir.path().join("shared.txt");
    let before = std::fs::read(&file).unwrap();
    let stages = conflict_stages(&repo, "shared.txt");
    let _read_only = ReadOnly::new(&file);

    let result = git_core::merge::resolve_conflict(&repo, "shared.txt", "resolved\n");

    let error = result.unwrap_err();
    assert!(
        matches!(error, MergeError::ReadOnlyFile(ref p) if p == "shared.txt"),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "'shared.txt' is read-only. Make it writable, then try again."
    );
    assert_eq!(std::fs::read(&file).unwrap(), before);
    assert!(std::fs::metadata(&file).unwrap().permissions().readonly());
    assert_eq!(conflict_stages(&repo, "shared.txt"), stages);
}

/// Taking a side of an add/delete conflict over a read-only file says so and changes nothing
/// (#209).
#[test]
fn taking_a_side_of_an_add_delete_conflict_over_a_read_only_file_is_refused() {
    if runs_as_root() {
        return;
    }
    let (dir, repo) = make_delete_modify_conflict();
    let file = dir.path().join("shared.txt");
    std::fs::write(&file, "kept\n").unwrap();
    let stages = conflict_stages(&repo, "shared.txt");
    let _read_only = ReadOnly::new(&file);

    let result = git_core::merge::resolve_add_delete_conflict(
        &repo,
        "shared.txt",
        FileConflictChoice::Theirs,
    );

    let error = result.unwrap_err();
    assert!(
        matches!(error, MergeError::ReadOnlyFile(ref p) if p == "shared.txt"),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "'shared.txt' is read-only. Make it writable, then try again."
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "kept\n");
    assert!(std::fs::metadata(&file).unwrap().permissions().readonly());
    assert_eq!(conflict_stages(&repo, "shared.txt"), stages);
}

/// Use ours / Use theirs (whole file) on a read-only file says so and changes nothing: the
/// forced checkout would replace it (#209).
#[test]
fn taking_a_whole_side_of_a_read_only_file_is_refused() {
    if runs_as_root() {
        return;
    }
    for side in [ConflictSide::Ours, ConflictSide::Theirs] {
        let (dir, repo) = make_conflicted_repo();
        let file = dir.path().join("shared.txt");
        let before = std::fs::read(&file).unwrap();
        let stages = conflict_stages(&repo, "shared.txt");
        let _read_only = ReadOnly::new(&file);

        let result = git_core::merge::resolve_whole_file(&repo, "shared.txt", side);

        let error = result.unwrap_err();
        assert!(
            matches!(error, MergeError::ReadOnlyFile(ref p) if p == "shared.txt"),
            "{side:?}: {error:?}"
        );
        assert_eq!(
            error.to_string(),
            "'shared.txt' is read-only. Make it writable, then try again."
        );
        assert_eq!(std::fs::read(&file).unwrap(), before, "{side:?}");
        assert!(std::fs::metadata(&file).unwrap().permissions().readonly());
        assert_eq!(conflict_stages(&repo, "shared.txt"), stages, "{side:?}");
    }
}

/// A read-only file where the chosen side deleted it is still deleted, as discards and undo
/// delete read-only files (#194).
#[test]
fn taking_the_deleting_side_still_deletes_a_read_only_file() {
    let (dir, repo) = make_delete_modify_conflict();
    let file = dir.path().join("shared.txt");
    let _read_only = ReadOnly::new(&file);

    git_core::merge::resolve_whole_file(&repo, "shared.txt", ConflictSide::Ours).unwrap();

    assert!(std::fs::symlink_metadata(&file).is_err());
    assert!(!repo.index().unwrap().has_conflicts());
}

/// A file with no permissions at all (0o000, the user's own) is read-only for every resolution:
/// Use ours/theirs refuses it like Save resolution does, without needing to open it (#209).
#[cfg(unix)]
#[test]
fn taking_a_whole_side_of_a_file_without_any_permission_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    if runs_as_root() {
        return;
    }
    for side in [ConflictSide::Ours, ConflictSide::Theirs] {
        let (dir, repo) = make_conflicted_repo();
        let file = dir.path().join("shared.txt");
        let before = std::fs::read(&file).unwrap();
        let stages = conflict_stages(&repo, "shared.txt");
        let _read_only = ReadOnly::new(&file);
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();

        let whole = git_core::merge::resolve_whole_file(&repo, "shared.txt", side);
        let saved = git_core::merge::resolve_conflict(&repo, "shared.txt", "resolved\n");

        for error in [whole.unwrap_err(), saved.unwrap_err()] {
            assert!(
                matches!(error, MergeError::ReadOnlyFile(ref p) if p == "shared.txt"),
                "{side:?}: {error:?}"
            );
        }
        let mode = std::fs::symlink_metadata(&file)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o7777, 0, "{side:?}");
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o444)).unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), before, "{side:?}");
        assert_eq!(conflict_stages(&repo, "shared.txt"), stages, "{side:?}");
    }
}

/// The names in the work tree's top folder, `.git` left out, sorted.
fn top_level_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name != ".git")
        .collect();
    names.sort();
    names
}

fn untracked_paths(repo: &git2::Repository) -> Vec<String> {
    let mut options = git2::StatusOptions::new();
    options.include_untracked(true).recurse_untracked_dirs(true);
    repo.statuses(Some(&mut options))
        .unwrap()
        .iter()
        .filter(|entry| entry.status().contains(git2::Status::WT_NEW))
        .map(|entry| entry.path().unwrap().to_string())
        .collect()
}

/// A resolved conflict replaces the file whole: no temporary file is left next to it, and an
/// executable file stays executable (#239).
#[test]
fn resolving_a_conflict_leaves_no_temporary_file() {
    let (dir, repo) = make_conflicted_repo();
    let file = dir.path().join("shared.txt");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    git_core::merge::resolve_conflict(&repo, "shared.txt", "resolved\n").unwrap();

    assert_eq!(std::fs::read_to_string(&file).unwrap(), "resolved\n");
    assert_eq!(top_level_names(dir.path()), ["shared.txt"]);
    assert_eq!(untracked_paths(&repo), Vec::<String>::new());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }
}

/// Taking theirs of an add/delete conflict over an existing file replaces it whole (#239).
#[test]
fn taking_a_side_of_an_add_delete_conflict_leaves_no_temporary_file() {
    let (dir, repo) = make_delete_modify_conflict();
    let file = dir.path().join("shared.txt");
    std::fs::write(&file, "kept\n").unwrap();

    git_core::merge::resolve_add_delete_conflict(&repo, "shared.txt", FileConflictChoice::Theirs)
        .unwrap();

    assert_eq!(std::fs::read_to_string(&file).unwrap(), "v2\n");
    assert_eq!(top_level_names(dir.path()), ["shared.txt"]);
    assert_eq!(untracked_paths(&repo), Vec::<String>::new());
}

/// Keeps `path` open the way some editors do: sharing read and write, not delete, so it can't
/// be replaced (#239).
#[cfg(windows)]
fn hold_open_without_share_delete(path: &Path) -> std::fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .open(path)
        .unwrap()
}

/// Resolving a conflict in a file another program keeps open says so and changes nothing
/// (#239).
#[cfg(windows)]
#[test]
fn resolving_a_conflict_in_a_file_open_in_another_program_is_refused() {
    let (dir, repo) = make_conflicted_repo();
    let file = dir.path().join("shared.txt");
    let before = std::fs::read(&file).unwrap();
    let stages = conflict_stages(&repo, "shared.txt");
    let editor = hold_open_without_share_delete(&file);

    let error = git_core::merge::resolve_conflict(&repo, "shared.txt", "resolved\n").unwrap_err();

    assert!(
        matches!(error, MergeError::FileInUse(ref p) if p == "shared.txt"),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "'shared.txt' is open in another program. Close it, then try again."
    );
    drop(editor);
    assert_eq!(std::fs::read(&file).unwrap(), before);
    assert_eq!(conflict_stages(&repo, "shared.txt"), stages);
    assert_eq!(top_level_names(dir.path()), ["shared.txt"]);
}
