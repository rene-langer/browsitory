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
        ConflictSegment::Conflict { ours, base, theirs } => {
            // Each side keeps its own trailing newline — `parse_conflict_markers` now splits on
            // `split_inclusive('\n')` rather than `.lines()`, so terminators aren't stripped.
            assert_eq!(ours, "main two\n");
            // The ancestor's lines for the block (#720).
            assert_eq!(base, "line two\n");
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

/// Merges `feature` into the checked-out branch after both changed `path` from `base` (or both
/// added it, when `base` is `None`), and returns the conflict's segments.
fn conflict_segments_for(
    path: &str,
    base: Option<&str>,
    ours: &str,
    theirs: &str,
) -> Vec<ConflictSegment> {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "other.txt", "other\n");
    if let Some(base) = base {
        write_file(dir.path(), path, base);
    }
    commit_all(&repo, "base commit");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), path, theirs);
    commit_all(&repo, "feature commit");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), path, ours);
    commit_all(&repo, "main commit");
    let outcome =
        git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
            .unwrap();
    assert!(matches!(outcome, MergeOutcome::Conflicted { .. }));
    git_core::merge::conflict_hunks(&repo, path).unwrap()
}

#[test]
fn conflict_hunks_of_an_add_add_conflict_have_an_empty_base() {
    let segments = conflict_segments_for("added.txt", None, "one\nours\n", "one\ntheirs\n");

    assert_eq!(
        segments,
        vec![
            ConflictSegment::Clean {
                content: "one\n".to_string()
            },
            ConflictSegment::Conflict {
                ours: "ours\n".to_string(),
                base: String::new(),
                theirs: "theirs\n".to_string(),
            },
        ]
    );
}

#[test]
fn conflict_hunks_keep_lines_that_look_like_conflict_markers() {
    // Runs of seven (and eight) marker characters in the file itself: the markers get longer,
    // so none of these lines is read as one.
    let base = "<<<<<<<< a\nx\n=======\n|||||||\n>>>>>>> b\n";
    let ours = "<<<<<<<< a\nours\n=======\n|||||||\n>>>>>>> b\n";
    let theirs = "<<<<<<<< a\ntheirs\n=======\n|||||||\n>>>>>>> b\n";

    let segments = conflict_segments_for("markers.txt", Some(base), ours, theirs);

    assert_eq!(
        segments,
        vec![
            ConflictSegment::Clean {
                content: "<<<<<<<< a\n".to_string()
            },
            ConflictSegment::Conflict {
                ours: "ours\n".to_string(),
                base: "x\n".to_string(),
                theirs: "theirs\n".to_string(),
            },
            ConflictSegment::Clean {
                content: "=======\n|||||||\n>>>>>>> b\n".to_string()
            },
        ]
    );
}

#[test]
fn conflict_hunks_cut_the_blocks_as_tightly_as_the_default_style() {
    // zdiff3 keeps lines both sides changed the same way out of the block; plain diff3 would
    // make the whole changed region one block.
    let segments = conflict_segments_for(
        "tight.txt",
        Some("a\nb\nc\n"),
        "a\nsame\nours\nc\n",
        "a\nsame\ntheirs\nc\n",
    );

    assert_eq!(
        segments,
        vec![
            ConflictSegment::Clean {
                content: "a\nsame\n".to_string()
            },
            ConflictSegment::Conflict {
                ours: "ours\n".to_string(),
                base: "b\n".to_string(),
                theirs: "theirs\n".to_string(),
            },
            ConflictSegment::Clean {
                content: "c\n".to_string()
            },
        ]
    );
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
        unblock_fifo(&fifo);
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
        unblock_fifo(&fifo);
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
    // Status sees the work tree file as staged, nothing more...
    let status = repo.status_file(Path::new("shared.txt")).unwrap();
    assert_eq!(status, git2::Status::INDEX_MODIFIED, "{status:?}");
    // ...and still notices a rewrite of the same size with a later mtime: the recorded stat
    // data doesn't hide a change.
    write_file(dir.path(), "shared.txt", "RESOLVED\n");
    let file = std::fs::File::options()
        .write(true)
        .open(dir.path().join("shared.txt"))
        .unwrap();
    file.set_modified(meta.modified().unwrap() + std::time::Duration::from_secs(5))
        .unwrap();
    drop(file);
    let status = repo.status_file(Path::new("shared.txt")).unwrap();
    assert_eq!(
        status,
        git2::Status::INDEX_MODIFIED | git2::Status::WT_MODIFIED,
        "{status:?}"
    );
}

/// A staged link records its own stat data (`lstat`) too, so status trusts it instead of
/// reading the link again on every refresh (#225).
#[cfg(unix)]
#[test]
fn mark_resolved_records_a_links_stat_data() {
    use std::os::unix::fs::MetadataExt;
    let (dir, repo) = make_conflicted_repo();
    let link = dir.path().join("shared.txt");
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("elsewhere.txt", &link).unwrap();
    git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    let entry = repo
        .index()
        .unwrap()
        .get_path(Path::new("shared.txt"), 0)
        .unwrap();
    let meta = std::fs::symlink_metadata(&link).unwrap();
    assert_eq!(entry.mode, 0o120000);
    assert_eq!(entry.ino, meta.ino() as u32);
    assert_eq!(entry.file_size as u64, meta.len());
    assert_eq!(entry.mtime.seconds(), meta.mtime() as i32);
    assert_eq!(entry.mtime.nanoseconds(), meta.mtime_nsec() as u32);
    assert_eq!(entry.ctime.seconds(), meta.ctime() as i32);
    let status = repo.status_file(Path::new("shared.txt")).unwrap();
    assert_eq!(status, git2::Status::INDEX_TYPECHANGE, "{status:?}");
}

/// The repository's HEAD, index file and `name`'s link target, to show nothing changed.
#[cfg(unix)]
fn snapshot(
    dir: &Path,
    repo: &git2::Repository,
    name: &str,
) -> (git2::Oid, Vec<u8>, std::path::PathBuf) {
    (
        repo.head().unwrap().target().unwrap(),
        std::fs::read(repo.path().join("index")).unwrap(),
        std::fs::read_link(dir.join(name)).unwrap(),
    )
}

/// An invalid `core.ignorecase` refuses a forced checkout before anything is written, cleanup
/// included (#225): libgit2's checkout would count it as false, but its attribute and ignore
/// parsing refuses it, so the links removed first could be gone while the checkout stopped.
#[cfg(unix)]
#[test]
fn abort_and_reset_hard_refuse_an_invalid_core_ignorecase_untouched() {
    let (dir, repo) = make_conflicted_repo();
    std::fs::remove_file(dir.path().join("shared.txt")).unwrap();
    std::os::unix::fs::symlink("elsewhere", dir.path().join("shared.txt")).unwrap();
    repo.config()
        .unwrap()
        .set_str("core.ignorecase", "maybe")
        .unwrap();
    let before = snapshot(dir.path(), &repo, "shared.txt");
    assert!(git_core::merge::abort_merge(&repo).is_err());
    assert_eq!(snapshot(dir.path(), &repo, "shared.txt"), before);
    assert!(repo.path().join("MERGE_HEAD").exists());

    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), "f", "f\n");
    commit_all(&repo, "base");
    std::fs::remove_file(dir.path().join("f")).unwrap();
    std::os::unix::fs::symlink("elsewhere", dir.path().join("f")).unwrap();
    write_file(dir.path(), "a.txt", "edited\n");
    repo.config()
        .unwrap()
        .set_str("core.ignorecase", "maybe")
        .unwrap();
    let head = repo.head().unwrap().target().unwrap().to_string();
    let before = snapshot(dir.path(), &repo, "f");
    let error =
        git_core::reset::reset_to(&repo, &head, git_core::reset::ResetMode::Hard).unwrap_err();
    assert!(
        matches!(error, git_core::reset::ResetError::CaseConfig(_)),
        "{error}"
    );
    assert_eq!(snapshot(dir.path(), &repo, "f"), before);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "edited\n"
    );

    // A valid value (or none) resets as before: the link is replaced by the file.
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", false)
        .unwrap();
    git_core::reset::reset_to(&repo, &head, git_core::reset::ResetMode::Hard).unwrap();
    assert!(!dir.path().join("f").is_symlink());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("f")).unwrap(),
        "f\n"
    );
    repo.config().unwrap().remove("core.ignorecase").unwrap();
    write_file(dir.path(), "a.txt", "again\n");
    git_core::reset::reset_to(&repo, &head, git_core::reset::ResetMode::Hard).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "a\n"
    );
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

/// `rel` conflicts (merge of `feature` into the checked-out branch).
fn make_conflicted_repo_at(rel: &str) -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), rel, "base\n");
    commit_all(&repo, "base");
    let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
        .name
        .clone();
    git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
    write_file(dir.path(), rel, "feature\n");
    commit_all(&repo, "feature");
    git_core::branch::switch_branch(&repo, &main_branch).unwrap();
    write_file(dir.path(), rel, "main\n");
    commit_all(&repo, "main");
    git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
        .unwrap();
    (dir, repo)
}

/// `path`'s index entries as `git ls-files -s` lists them: (mode, id, stage).
fn index_entries(repo: &git2::Repository, path: &str) -> Vec<(u32, git2::Oid, i32)> {
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    index
        .iter()
        .filter(|entry| entry.path == path.as_bytes())
        .map(|entry| (entry.mode, entry.id, ((entry.flags >> 12) & 3) as i32))
        .collect()
}

/// #213: a file where a folder on the way should be, right above the conflicted file or two
/// levels up. `git add <path>` (the twin) and Mark resolved must leave the index the same: what
/// git does there is what Mark resolved does.
#[test]
fn mark_resolved_matches_git_add_when_a_folder_on_the_way_is_a_file() {
    for (rel, file) in [("sub/shared.txt", "sub"), ("a/b/c.txt", "a")] {
        let run = |use_git: bool| {
            let (dir, repo) = make_conflicted_repo_at(rel);
            std::fs::remove_dir_all(dir.path().join(file)).unwrap();
            std::fs::write(dir.path().join(file), "a file\n").unwrap();
            let ok = if use_git {
                test_support::git(dir.path(), &["add", rel])
                    .status
                    .success()
            } else {
                let result = git_core::merge::mark_resolved(&repo, rel, false);
                assert!(result.is_ok(), "{rel}: {result:?}");
                true
            };
            (ok, index_entries(&repo, rel))
        };
        let git = run(true);
        let ours = run(false);
        // git sees no file there (`lstat` fails with ENOTDIR, which git counts as removed) and
        // stages the deletion.
        assert_eq!(git, (true, Vec::new()), "{rel}");
        assert_eq!(ours.0, git.0, "{rel}: succeeds or refuses as git does");
        // The ids differ between the two runs (timestamps), so compare modes and stages.
        let shape = |entries: &[(u32, git2::Oid, i32)]| -> Vec<(u32, i32)> {
            entries
                .iter()
                .map(|(mode, _, stage)| (*mode, *stage))
                .collect()
        };
        assert_eq!(shape(&ours.1), shape(&git.1), "{rel}");
    }
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

/// The worker keeps one `Repository` per repo. A conflicting `git merge` run in a terminal after
/// that handle loaded its index must still be aborted from the index on disk, or the conflicted
/// file keeps its markers.
#[test]
fn abort_merge_reads_an_index_changed_behind_the_handle() {
    let (dir, repo) = make_conflicting_branches();
    let loaded = repo.index().unwrap();
    assert!(!loaded.has_conflicts());
    drop(loaded);
    let output = test_support::git_command(dir.path())
        .args(["merge", "feature"])
        .output()
        .unwrap();
    assert!(!output.status.success(), "the merge must conflict");
    assert!(std::fs::read_to_string(dir.path().join("shared.txt"))
        .unwrap()
        .contains("<<<<<<<"));

    git_core::merge::abort_merge(&repo).unwrap();

    assert_eq!(
        std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
        "line one\nmain two\nline three\n"
    );
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    assert!(!index.has_conflicts());
}

/// A terminal rebase's folder is never removed by Abort (#249). During a terminal rebase even a
/// MERGE_HEAD is the rebase's (a `--rebase-merges` merge step), so Abort refuses and changes
/// nothing (#263 review, `tests/terminal_rebase.rs`).
#[test]
fn aborting_a_merge_during_a_terminal_rebase_refuses_and_keeps_its_dir() {
    let (dir, repo) = make_conflicted_repo();
    let rebase_dir = repo.path().join("rebase-merge");
    std::fs::create_dir(&rebase_dir).unwrap();
    std::fs::write(rebase_dir.join("head-name"), "refs/heads/topic\n").unwrap();
    let conflicted = std::fs::read_to_string(dir.path().join("shared.txt")).unwrap();

    let error = git_core::merge::abort_merge(&repo).unwrap_err();

    assert!(error
        .to_string()
        .starts_with("A rebase started in a terminal"));
    assert_eq!(
        std::fs::read_to_string(rebase_dir.join("head-name")).unwrap(),
        "refs/heads/topic\n"
    );
    assert!(repo.path().join("MERGE_HEAD").exists());
    assert!(repo.index().unwrap().has_conflicts());
    assert_eq!(
        std::fs::read_to_string(dir.path().join("shared.txt")).unwrap(),
        conflicted
    );
}

/// ...and a terminal cherry-pick's list (`.git/sequencer`) stays too (#249).
#[test]
fn aborting_a_merge_keeps_a_sequencer_todo() {
    let (_dir, repo) = make_conflicted_repo();
    let sequencer = repo.path().join("sequencer");
    std::fs::create_dir(&sequencer).unwrap();
    std::fs::write(sequencer.join("todo"), "pick 1234567 B\n").unwrap();

    git_core::merge::abort_merge(&repo).unwrap();

    assert_eq!(
        std::fs::read_to_string(sequencer.join("todo")).unwrap(),
        "pick 1234567 B\n"
    );
    assert!(!repo.path().join("MERGE_HEAD").exists());
    assert_eq!(repo.state(), git2::RepositoryState::Clean);
}

/// A state file Abort can't remove (here a folder stands at `MERGE_MODE`) is reported as such,
/// not as "failed to write resolved file" (#249). The other files are still removed.
#[test]
fn abort_merge_reports_a_state_file_it_cant_remove() {
    let (_dir, repo) = make_conflicted_repo();
    let blocker = repo.path().join("MERGE_MODE");
    let _ = std::fs::remove_file(&blocker);
    std::fs::create_dir(&blocker).unwrap();
    std::fs::write(blocker.join("keep"), "x").unwrap();

    let error = git_core::merge::abort_merge(&repo).unwrap_err();

    assert!(
        matches!(error, git_core::merge::MergeError::StateFiles(_)),
        "{error:?}"
    );
    assert!(
        error
            .to_string()
            .starts_with("Couldn't remove git's merge state:"),
        "{error}"
    );
    assert!(!repo.path().join("MERGE_HEAD").exists());
    assert!(!repo.path().join("MERGE_MSG").exists());
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
fn abort_merge_refuses_a_merged_path_replaced_by_a_symlink() {
    // A merged (staged) path changed again in the work tree is a change Abort would lose, so
    // it refuses, as `git merge --abort` does ("not uptodate", #281 review), and writes
    // nothing, through the link or otherwise.
    let out = common::outside_dir();
    let (dir, repo) = stopped_merge_with_clean_paths();
    common::replace_with_link(dir.path(), "tc", &out.path().join("target.txt"));

    let error = git_core::merge::abort_merge(&repo).unwrap_err();

    assert!(
        matches!(&error, git_core::merge::MergeError::WouldDiscard(path) if path == "tc"),
        "{error:?}"
    );
    common::assert_outside_untouched(out.path());
    assert!(common::is_link(dir.path(), "tc"));
    assert_eq!(repo.state(), git2::RepositoryState::Merge);
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

/// Taking one side's whole file goes through libgit2's checkout: a file another program keeps
/// open gets the same message, not libgit2's raw error, and the conflict stays (#268).
#[cfg(windows)]
#[test]
fn taking_a_whole_side_of_a_file_open_in_another_program_is_refused() {
    let (dir, repo) = make_conflicted_repo();
    let file = dir.path().join("shared.txt");
    let before = std::fs::read(&file).unwrap();
    let stages = conflict_stages(&repo, "shared.txt");
    let editor = hold_open_without_share_delete(&file);

    let error =
        git_core::merge::resolve_whole_file(&repo, "shared.txt", ConflictSide::Theirs).unwrap_err();

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
    git_core::merge::resolve_whole_file(&repo, "shared.txt", ConflictSide::Theirs).unwrap();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "line one\nfeature two\nline three\n"
    );
}

/// Windows: a fast-forward whose checkout stops at a file another program keeps open says so,
/// not libgit2's raw error, and leaves the branch where it was; once the file is closed,
/// merging again finishes it (#303).
#[cfg(windows)]
#[test]
fn fast_forward_with_a_file_open_in_another_program_says_so_and_finishes_when_tried_again() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "1\n");
    write_file(dir.path(), "z.txt", "1\n");
    commit_all(&repo, "base");
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    let base = repo.head().unwrap().peel_to_commit().unwrap().id();
    git_core::branch::create_branch(&repo, "side", "HEAD").unwrap();
    write_file(dir.path(), "a.txt", "a longer line\n");
    write_file(dir.path(), "z.txt", "a longer line\n");
    commit_all(&repo, "side");
    git_core::branch::switch_branch(&repo, &main).unwrap();
    let editor = hold_open_without_share_delete(&dir.path().join("z.txt"));

    let error = git_core::merge::start_merge(&repo, "side", Default::default()).unwrap_err();

    assert!(
        matches!(error, MergeError::HeldOpen(ref held) if held.path == "z.txt"),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "'z.txt' is open in another program. Some files were already updated. Close it, then \
         try again to finish."
    );
    assert_eq!(repo.head().unwrap().peel_to_commit().unwrap().id(), base);
    drop(editor);
    assert_eq!(
        git_core::merge::start_merge(&repo, "side", Default::default()).unwrap(),
        MergeOutcome::FastForwarded
    );
    assert_eq!(common::read(dir.path(), "z.txt"), "a longer line\n");
    assert!(repo.statuses(None).unwrap().is_empty());
}

/// Replaces the conflicted `shared.txt` with a nested repository holding one commit; its id.
fn nested_repo_at_shared(dir: &Path) -> git2::Oid {
    nested_repo_at(dir, "shared.txt")
}

/// Replaces the conflicted file `rel` with a nested repository holding one commit; its id.
fn nested_repo_at(dir: &Path, rel: &str) -> git2::Oid {
    std::fs::remove_file(dir.join(rel)).unwrap();
    let nested = git2::Repository::init(dir.join(rel)).unwrap();
    {
        let mut config = nested.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    write_file(&dir.join(rel), "inner.txt", "inner\n");
    commit_all(&nested, "inner");
    let head = nested.head().unwrap().target().unwrap();
    head
}

/// Mode and id of `path`'s stage-0 index entry.
fn staged_entry(repo: &git2::Repository, path: &str) -> (u32, git2::Oid) {
    let entry = repo
        .index()
        .unwrap()
        .get_path(Path::new(path), 0)
        .expect("a stage-0 entry");
    (entry.mode, entry.id)
}

/// A conflicted path that is now a nested repository is staged as a gitlink to its HEAD
/// commit, like `git add` (#213).
#[test]
fn mark_resolved_stages_a_nested_repository_as_a_gitlink_of_its_head() {
    let (dir, repo) = make_conflicted_repo();
    let head = nested_repo_at_shared(dir.path());
    let outcome = git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    assert_eq!(outcome, git_core::merge::MarkResolvedOutcome::Resolved);
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert!(!repo.index().unwrap().has_conflicts());
    assert_eq!(staged_entry(&repo, "shared.txt"), (0o160000, head));
}

/// The nested repository's branch is found in `packed-refs` too, and a detached HEAD is used
/// as it is (#213).
#[test]
fn mark_resolved_reads_a_packed_branch_and_a_detached_head_of_a_nested_repository() {
    let (dir, repo) = make_conflicted_repo();
    let head = nested_repo_at_shared(dir.path());
    let git_dir = dir.path().join("shared.txt/.git");
    let branch = std::fs::read_to_string(git_dir.join("HEAD")).unwrap();
    let branch = branch.trim().strip_prefix("ref: ").unwrap().to_string();
    std::fs::remove_file(git_dir.join(&branch)).unwrap();
    std::fs::write(
        git_dir.join("packed-refs"),
        format!("# pack-refs with: peeled fully-peeled sorted \n{head} {branch}\n"),
    )
    .unwrap();
    git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert_eq!(staged_entry(&repo, "shared.txt"), (0o160000, head));

    let (dir, repo) = make_conflicted_repo();
    let head = nested_repo_at_shared(dir.path());
    std::fs::write(dir.path().join("shared.txt/.git/HEAD"), format!("{head}\n")).unwrap();
    git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert_eq!(staged_entry(&repo, "shared.txt"), (0o160000, head));
}

/// A nested repository whose `.git` is a `gitdir:` file, as a submodule's is, is staged from
/// the git folder it names in the work tree: relative (here in the outer `.git/modules`) or
/// absolute. A git folder outside the work tree is refused unopened, and the message says how
/// to stage it instead (#213).
#[test]
fn mark_resolved_follows_a_gitdir_file_of_a_nested_repository() {
    let (dir, repo) = make_conflicted_repo();
    let head = nested_repo_at_shared(dir.path());
    let modules = dir.path().join(".git/modules");
    std::fs::create_dir_all(&modules).unwrap();
    std::fs::rename(
        dir.path().join("shared.txt/.git"),
        modules.join("shared.txt"),
    )
    .unwrap();
    std::fs::write(
        dir.path().join("shared.txt/.git"),
        "gitdir: ../.git/modules/shared.txt\n",
    )
    .unwrap();
    git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert_eq!(staged_entry(&repo, "shared.txt"), (0o160000, head));

    // Absolute, spelled as the work tree root (as libgit2 reports it).
    let (dir, repo) = make_conflicted_repo();
    let head = nested_repo_at_shared(dir.path());
    let modules = dir.path().join(".git/modules");
    std::fs::create_dir_all(&modules).unwrap();
    std::fs::rename(
        dir.path().join("shared.txt/.git"),
        modules.join("shared.txt"),
    )
    .unwrap();
    let absolute = repo.workdir().unwrap().join(".git/modules/shared.txt");
    std::fs::write(
        dir.path().join("shared.txt/.git"),
        format!("gitdir: {}\n", absolute.display()),
    )
    .unwrap();
    git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert_eq!(staged_entry(&repo, "shared.txt"), (0o160000, head));

    let elsewhere = tempfile::TempDir::new().unwrap();
    let (dir, _repo) = make_conflicted_repo();
    nested_repo_at_shared(dir.path());
    let git_dir = elsewhere.path().join("nested.git");
    std::fs::rename(dir.path().join("shared.txt/.git"), &git_dir).unwrap();
    std::fs::write(
        dir.path().join("shared.txt/.git"),
        format!("gitdir: {}\n", git_dir.display()),
    )
    .unwrap();
    assert_eq!(
        assert_not_staged(dir.path(), NotStagedReason::GitFolderOutside),
        OUTSIDE_MESSAGE
    );
}

/// `GitFolderOutside`'s message for `shared.txt`.
const OUTSIDE_MESSAGE: &str = "'shared.txt' has its git folder outside the work tree, or at a \
                               path Browsitory can't match to it. Stage it with `git add \
                               shared.txt` in a terminal.";

use git_core::merge::NotStagedReason;

/// Mark resolved on `shared.txt` refuses to stage the folder there for `reason`; nothing staged.
fn assert_not_staged(dir: &Path, reason: NotStagedReason) -> String {
    let repo = git2::Repository::open(dir).unwrap();
    let error = git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap_err();
    assert!(
        matches!(error, MergeError::FolderNotStaged(ref p, r) if p == "shared.txt" && r == reason),
        "{error:?}"
    );
    assert!(repo.index().unwrap().has_conflicts());
    error.to_string()
}

/// A nested repository with no commit yet, and a folder of plain files, aren't staged: git
/// stages a folder only as a repository's commit (#213). Each says why.
#[test]
fn mark_resolved_refuses_a_folder_without_a_commit_checked_out() {
    let (dir, _repo) = make_conflicted_repo();
    std::fs::remove_file(dir.path().join("shared.txt")).unwrap();
    git2::Repository::init(dir.path().join("shared.txt")).unwrap();
    assert_eq!(
        assert_not_staged(dir.path(), NotStagedReason::NoCommit),
        "'shared.txt' is a repository without a commit checked out. Commit in it first, then \
         try again."
    );

    let (dir, _repo) = make_conflicted_repo();
    std::fs::remove_file(dir.path().join("shared.txt")).unwrap();
    write_file(dir.path(), "shared.txt/a.txt", "a\n");
    write_file(dir.path(), "shared.txt/b/c.txt", "c\n");
    assert_eq!(
        assert_not_staged(dir.path(), NotStagedReason::NotARepository),
        "'shared.txt' is a folder Browsitory can't read as a repository: it has no `.git`, or \
         its `.git` is a link or names a git folder that can't be found. To stage it anyway, run \
         `git add shared.txt` in a terminal."
    );
}

/// Writes `content` to the file `rel` inside `shared.txt`'s git folder.
fn write_in_git_dir(dir: &Path, rel: &str, content: &str) {
    let path = dir.join("shared.txt/.git").join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
}

/// The branch the nested repository at `shared.txt` has checked out (`refs/heads/…`).
fn nested_branch(dir: &Path) -> String {
    let head = std::fs::read_to_string(dir.join("shared.txt/.git/HEAD")).unwrap();
    head.trim().strip_prefix("ref: ").unwrap().to_string()
}

/// `packed-refs` as git writes it, with a comment, a peeled tag (`^` line) and CRLF line ends,
/// is read for the branch (#213).
#[test]
fn mark_resolved_reads_packed_refs_with_peeled_lines_comments_and_crlf() {
    let (dir, _repo) = make_conflicted_repo();
    let head = nested_repo_at_shared(dir.path());
    let branch = nested_branch(dir.path());
    std::fs::remove_file(dir.path().join("shared.txt/.git").join(&branch)).unwrap();
    let other = "1111111111111111111111111111111111111111";
    write_in_git_dir(
        dir.path(),
        "packed-refs",
        &format!(
            "# pack-refs with: peeled fully-peeled sorted \r\n{other} refs/heads/a\r\n\
             {other} refs/tags/v1\r\n^{other}\r\n{head} {branch}\r\n"
        ),
    );
    let repo = git2::Repository::open(dir.path()).unwrap();
    git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    assert_eq!(staged_entry(&repo, "shared.txt"), (0o160000, head));
}

/// A branch neither loose nor in `packed-refs` is an unborn branch: no commit (#213).
#[test]
fn mark_resolved_reports_a_branch_missing_from_loose_and_packed_refs_as_no_commit() {
    let (dir, _repo) = make_conflicted_repo();
    let head = nested_repo_at_shared(dir.path());
    let branch = nested_branch(dir.path());
    std::fs::remove_file(dir.path().join("shared.txt/.git").join(&branch)).unwrap();
    write_in_git_dir(
        dir.path(),
        "packed-refs",
        &format!("{head} refs/heads/other\n"),
    );
    assert_not_staged(dir.path(), NotStagedReason::NoCommit);
}

/// Symbolic refs are followed as deep as git follows them (HEAD and 4 more), no deeper (#213).
#[test]
fn mark_resolved_follows_symbolic_refs_up_to_gits_depth() {
    let chain = |links: usize| {
        let (dir, repo) = make_conflicted_repo();
        let head = nested_repo_at_shared(dir.path());
        write_in_git_dir(dir.path(), "HEAD", "ref: refs/heads/l1\n");
        for n in 1..links {
            write_in_git_dir(
                dir.path(),
                &format!("refs/heads/l{n}"),
                &format!("ref: refs/heads/l{}\n", n + 1),
            );
        }
        write_in_git_dir(
            dir.path(),
            &format!("refs/heads/l{links}"),
            &format!("{head}\n"),
        );
        drop(repo);
        (dir, head)
    };
    let (dir, head) = chain(4);
    let repo = git2::Repository::open(dir.path()).unwrap();
    git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    assert_eq!(staged_entry(&repo, "shared.txt"), (0o160000, head));

    let (dir, _) = chain(5);
    assert_not_staged(
        dir.path(),
        NotStagedReason::Unsupported("symbolic refs nested too deeply"),
    );
}

/// Only a full 40-digit hexadecimal id is taken (git2 would take a prefix); a SHA-256 id, the
/// reftable stub and a per-worktree ref say they aren't supported, never "no commit" (#213).
#[test]
fn mark_resolved_says_which_repository_formats_it_cannot_read() {
    let cases: [(&str, &str, &str); 7] = [
        (
            "HEAD",
            "0123456789abcdef0123456789abcdef0123456\n",
            "a malformed HEAD or ref",
        ),
        (
            "HEAD",
            "0123456789abcdef0123456789abcdef0123456z\n",
            "a malformed HEAD or ref",
        ),
        (
            "HEAD",
            &format!("{}\n", "ab".repeat(32)),
            "SHA-256 object ids",
        ),
        ("HEAD", "ref: refs/heads/.invalid\n", "reftable refs"),
        (
            "HEAD",
            "ref: refs/worktree/x\n",
            "HEAD on a per-worktree ref",
        ),
        ("HEAD", "ref: HEAD\n", "HEAD outside refs/"),
        (
            "HEAD",
            &"0".repeat(5000),
            "a file in its git folder is too large",
        ),
    ];
    for (file, content, what) in cases {
        let (dir, _repo) = make_conflicted_repo();
        nested_repo_at_shared(dir.path());
        write_in_git_dir(dir.path(), file, content);
        assert_not_staged(dir.path(), NotStagedReason::Unsupported(what));
    }
    let (dir, _repo) = make_conflicted_repo();
    nested_repo_at_shared(dir.path());
    let branch = nested_branch(dir.path());
    write_in_git_dir(dir.path(), &branch, "0123\n");
    assert_eq!(
        assert_not_staged(
            dir.path(),
            NotStagedReason::Unsupported("a malformed HEAD or ref")
        ),
        "Browsitory can't read which commit the repository 'shared.txt' has checked out (a \
         malformed HEAD or ref). Stage it with `git add shared.txt` in a terminal."
    );
    let (dir, _repo) = make_conflicted_repo();
    nested_repo_at_shared(dir.path());
    write_in_git_dir(dir.path(), "HEAD", &format!("{}\n", "ab".repeat(32)));
    assert_eq!(
        assert_not_staged(
            dir.path(),
            NotStagedReason::Unsupported("SHA-256 object ids")
        ),
        "Browsitory can't read which commit the repository 'shared.txt' has checked out \
         (SHA-256 object ids). Stage it with `git add shared.txt` in a terminal."
    );
}

/// An oversized `.git` file is refused unread past its limit (#213).
#[test]
fn mark_resolved_refuses_an_oversized_gitdir_file() {
    let (dir, _repo) = make_conflicted_repo();
    nested_repo_at_shared(dir.path());
    std::fs::remove_dir_all(dir.path().join("shared.txt/.git")).unwrap();
    std::fs::write(
        dir.path().join("shared.txt/.git"),
        format!("gitdir: {}\n", "a/".repeat(4000)),
    )
    .unwrap();
    assert_not_staged(
        dir.path(),
        NotStagedReason::Unsupported("a file in its git folder is too large"),
    );
}

/// A `gitdir:` to a folder that is gone (a submodule's, after the repository moved) is no
/// repository, not an I/O error (#213). An absolute one outside the work tree is refused as
/// outside without being looked at.
#[test]
fn mark_resolved_refuses_a_gitdir_file_whose_folder_is_gone() {
    let gone = tempfile::TempDir::new().unwrap();
    let cases = [
        (
            "../.git/modules/missing".to_string(),
            NotStagedReason::NotARepository,
        ),
        (
            gone.path().join("missing.git").display().to_string(),
            NotStagedReason::GitFolderOutside,
        ),
    ];
    for (target, reason) in cases {
        let (dir, _repo) = make_conflicted_repo();
        nested_repo_at_shared(dir.path());
        std::fs::remove_dir_all(dir.path().join("shared.txt/.git")).unwrap();
        std::fs::write(
            dir.path().join("shared.txt/.git"),
            format!("gitdir: {target}\n"),
        )
        .unwrap();
        assert_not_staged(dir.path(), reason);
    }
}

/// Makes `shared.txt` a linked worktree (branch `wt`) of the nested repository `main` in the
/// work tree, laid out as git writes it with relative paths: `.git` is
/// `gitdir: ../main/.git/worktrees/wt`, whose `commondir` is `../..`. Its commit.
fn linked_worktree_at_shared(dir: &Path) -> git2::Oid {
    let main = git2::Repository::init(dir.join("main")).unwrap();
    {
        let mut config = main.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    write_file(&dir.join("main"), "m.txt", "m\n");
    commit_all(&main, "main");
    std::fs::remove_file(dir.join("shared.txt")).unwrap();
    main.worktree("wt", &dir.join("shared.txt"), None).unwrap();
    std::fs::write(
        dir.join("shared.txt/.git"),
        "gitdir: ../main/.git/worktrees/wt\n",
    )
    .unwrap();
    std::fs::write(dir.join("main/.git/worktrees/wt/commondir"), "../..\n").unwrap();
    // The branch is only in the common folder.
    assert!(dir.join("main/.git/refs/heads/wt").is_file());
    let head = main.head().unwrap().target().unwrap();
    head
}

/// A linked worktree of a repository in the work tree: its `gitdir:` and `commondir` (`../..`,
/// climbing inside `.git` by held folders) lead to the branch (#213).
#[test]
fn mark_resolved_reads_the_branch_of_a_linked_worktree_from_its_common_folder() {
    let (dir, repo) = make_conflicted_repo();
    let head = linked_worktree_at_shared(dir.path());
    git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert_eq!(staged_entry(&repo, "shared.txt"), (0o160000, head));
}

/// #213: a linked worktree whose `commondir` names a missing folder says its main repository
/// can't be found; one that is a link or a file is refused as unreadable, not as missing. (git
/// writes `commondir` as `../..`, so a moved main repository shows up as a missing or outside
/// git folder instead, below.)
#[test]
fn mark_resolved_says_when_a_linked_worktrees_main_repository_is_missing() {
    let commondir = |dir: &Path| dir.join("main/.git/worktrees/wt/commondir");
    let (dir, _repo) = make_conflicted_repo();
    linked_worktree_at_shared(dir.path());
    std::fs::write(commondir(dir.path()), "../../gone\n").unwrap();
    assert_eq!(
        assert_not_staged(dir.path(), NotStagedReason::MainRepositoryGone),
        "'shared.txt' is a linked worktree whose main repository can't be found: its git \
         folder's `commondir` names a folder that doesn't exist. Stage it with `git add \
         shared.txt` in a terminal once git can read it."
    );

    // The main repository moved away: its worktree's `gitdir:` names a folder that is gone.
    let (dir, _repo) = make_conflicted_repo();
    linked_worktree_at_shared(dir.path());
    std::fs::rename(dir.path().join("main"), dir.path().join("moved")).unwrap();
    assert_not_staged(dir.path(), NotStagedReason::NotARepository);

    let (dir, _repo) = make_conflicted_repo();
    linked_worktree_at_shared(dir.path());
    std::fs::write(commondir(dir.path()), "../../HEAD\n").unwrap();
    assert_not_staged(
        dir.path(),
        NotStagedReason::Unsupported("its main repository's path is a link or a file"),
    );
}

/// #213 (Unix): a `commondir` that is a link (here to the real common folder) isn't followed.
#[cfg(unix)]
#[test]
fn mark_resolved_does_not_follow_a_linked_common_folder() {
    let (dir, _repo) = make_conflicted_repo();
    linked_worktree_at_shared(dir.path());
    let git = dir.path().join("main/.git");
    std::os::unix::fs::symlink(&git, git.join("worktrees/common")).unwrap();
    std::fs::write(git.join("worktrees/wt/commondir"), "../common\n").unwrap();
    assert_not_staged(
        dir.path(),
        NotStagedReason::Unsupported("its main repository's path is a link or a file"),
    );
}

/// #213: `packed-refs` that is a folder (or, on Unix, a link) isn't read as "no packed refs",
/// so a missing loose branch isn't reported as "no commit".
#[test]
fn mark_resolved_refuses_packed_refs_that_are_not_a_regular_file() {
    let (dir, _repo) = make_conflicted_repo();
    let head = nested_repo_at_shared(dir.path());
    let branch = nested_branch(dir.path());
    let git = dir.path().join("shared.txt/.git");
    std::fs::remove_file(git.join(&branch)).unwrap();
    std::fs::create_dir(git.join("packed-refs")).unwrap();
    assert_not_staged(
        dir.path(),
        NotStagedReason::Unsupported("a `packed-refs` that isn't a regular file"),
    );

    #[cfg(unix)]
    {
        let outside = tempfile::TempDir::new().unwrap();
        std::fs::write(outside.path().join("packed"), format!("{head} {branch}\n")).unwrap();
        std::fs::remove_dir(git.join("packed-refs")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("packed"), git.join("packed-refs")).unwrap();
        assert_not_staged(
            dir.path(),
            NotStagedReason::Unsupported("a `packed-refs` that isn't a regular file"),
        );
    }
    let _ = head;
}

/// #213: a submodule of a submodule, as git lays it out: `a/b`'s `.git` is
/// `gitdir: ../../.git/modules/a/modules/b`.
#[test]
fn mark_resolved_stages_a_nested_submodule_layout() {
    let (dir, repo) = make_conflicted_repo_at("a/b");
    let head = nested_repo_at(dir.path(), "a/b");
    let modules = dir.path().join(".git/modules/a/modules");
    std::fs::create_dir_all(&modules).unwrap();
    std::fs::rename(dir.path().join("a/b/.git"), modules.join("b")).unwrap();
    std::fs::write(
        dir.path().join("a/b/.git"),
        "gitdir: ../../.git/modules/a/modules/b\n",
    )
    .unwrap();
    git_core::merge::mark_resolved(&repo, "a/b", false).unwrap();
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert_eq!(staged_entry(&repo, "a/b"), (0o160000, head));
}

/// #213, end to end with the layout git itself writes: the real `git worktree add` (isolated
/// HOME and config) makes `shared.txt` a linked worktree of the repository `main` in the work
/// tree. git writes the `gitdir:` as an absolute real path and `commondir` as `../..`. When the
/// temp folder's real path is the work tree's path as opened (Linux), the commit is staged.
/// When it isn't (macOS `/var` is a link to `/private/var`; Windows may shorten names), the path
/// can't be matched to the work tree, and the refusal says so.
#[test]
fn mark_resolved_on_a_linked_worktree_git_made() {
    let (dir, repo) = make_conflicted_repo();
    let main = git2::Repository::init(dir.path().join("main")).unwrap();
    {
        let mut config = main.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    write_file(&dir.path().join("main"), "m.txt", "m\n");
    commit_all(&main, "main");
    let head = main.head().unwrap().target().unwrap();
    std::fs::remove_file(dir.path().join("shared.txt")).unwrap();
    let target = dir.path().join("shared.txt");
    let output = test_support::git(
        &dir.path().join("main"),
        &["worktree", "add", "-b", "wt", target.to_str().unwrap()],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let gitfile = std::fs::read_to_string(dir.path().join("shared.txt/.git")).unwrap();
    let gitdir = Path::new(gitfile.trim().strip_prefix("gitdir:").unwrap().trim()).to_path_buf();
    assert!(gitdir.is_absolute(), "{gitfile}");
    assert_eq!(
        std::fs::read_to_string(gitdir.join("commondir"))
            .unwrap()
            .trim(),
        "../.."
    );
    let workdir = repo.workdir().unwrap().to_path_buf();
    let result = git_core::merge::mark_resolved(&repo, "shared.txt", false);
    if gitdir.starts_with(&workdir) {
        assert_eq!(
            result.unwrap(),
            git_core::merge::MarkResolvedOutcome::Resolved
        );
        let repo = git2::Repository::open(dir.path()).unwrap();
        assert_eq!(staged_entry(&repo, "shared.txt"), (0o160000, head));
    } else {
        let error = result.unwrap_err();
        assert!(
            matches!(
                error,
                MergeError::FolderNotStaged(_, NotStagedReason::GitFolderOutside)
            ),
            "{gitdir:?} vs {workdir:?}: {error:?}"
        );
        assert_eq!(error.to_string(), OUTSIDE_MESSAGE);
    }
}

/// #213 (Unix): a FIFO, or a link to one, at an absolute `gitdir:` or `commondir` outside the
/// work tree is refused as outside without being opened, so it can't block.
#[cfg(unix)]
#[test]
fn a_fifo_at_an_outside_git_folder_is_never_opened() {
    let outside = tempfile::TempDir::new().unwrap();
    let fifo = outside.path().join("fifo");
    mkfifo(&fifo);
    std::os::unix::fs::symlink(&fifo, outside.path().join("link")).unwrap();
    for name in ["fifo", "link"] {
        let target = outside.path().join(name);
        // As the `gitdir:`.
        let (dir, repo) = make_conflicted_repo();
        nested_repo_at_shared(dir.path());
        gitdir_file_at_shared(dir.path(), target.to_str().unwrap());
        drop(repo);
        let root = dir.path().to_path_buf();
        let finished = within_5s(move || {
            let repo = git2::Repository::open(&root).unwrap();
            git_core::merge::mark_resolved(&repo, "shared.txt", false)
                .map_err(|error| error.to_string())
        });
        if finished.is_none() {
            unblock_fifo(&fifo);
        }
        assert_eq!(
            finished,
            Some(Err(OUTSIDE_MESSAGE.to_string())),
            "gitdir {name}"
        );
        // As the `commondir`.
        let (dir, repo) = make_conflicted_repo();
        linked_worktree_at_shared(dir.path());
        std::fs::write(
            dir.path().join("main/.git/worktrees/wt/commondir"),
            format!("{}\n", target.display()),
        )
        .unwrap();
        drop(repo);
        let root = dir.path().to_path_buf();
        let finished = within_5s(move || {
            let repo = git2::Repository::open(&root).unwrap();
            git_core::merge::mark_resolved(&repo, "shared.txt", false)
                .map_err(|error| error.to_string())
        });
        if finished.is_none() {
            unblock_fifo(&fifo);
        }
        assert_eq!(
            finished,
            Some(Err(OUTSIDE_MESSAGE.to_string())),
            "commondir {name}"
        );
    }
}

/// Replaces `shared.txt`'s `.git` folder (a nested repository) with a `gitdir:` file holding
/// `target`.
fn gitdir_file_at_shared(dir: &Path, target: &str) {
    std::fs::remove_dir_all(dir.join("shared.txt/.git")).unwrap();
    std::fs::write(dir.join("shared.txt/.git"), format!("gitdir: {target}\n")).unwrap();
}

/// #213: nothing outside the work tree is opened. A relative `gitdir:` that climbs out of the
/// work tree with `..` (git 2.48's `worktree.useRelativePaths`, a main repository next to the
/// outer work tree) is refused, also when it would come back in; so is a linked worktree of a
/// repository outside (libgit2 writes its `gitdir:` absolute).
#[test]
fn mark_resolved_refuses_a_git_folder_outside_the_work_tree() {
    let elsewhere = tempfile::TempDir::new().unwrap();
    let (dir, _repo) = make_conflicted_repo();
    nested_repo_at_shared(dir.path());
    std::fs::rename(
        dir.path().join("shared.txt/.git"),
        elsewhere.path().join("nested.git"),
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("shared.txt/.git")).unwrap();
    // From `shared.txt`: `..` is the work tree root, one more `..` its parent, the temp folder
    // both are in.
    assert_eq!(dir.path().parent(), elsewhere.path().parent());
    let name = elsewhere.path().file_name().unwrap().to_str().unwrap();
    gitdir_file_at_shared(dir.path(), &format!("../../{name}/nested.git"));
    assert_eq!(
        assert_not_staged(dir.path(), NotStagedReason::GitFolderOutside),
        OUTSIDE_MESSAGE
    );

    let (dir, _repo) = make_conflicted_repo();
    nested_repo_at_shared(dir.path());
    let modules = dir.path().join(".git/modules");
    std::fs::create_dir_all(&modules).unwrap();
    std::fs::rename(
        dir.path().join("shared.txt/.git"),
        modules.join("shared.txt"),
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("shared.txt/.git")).unwrap();
    let work = dir.path().file_name().unwrap().to_str().unwrap();
    gitdir_file_at_shared(dir.path(), &format!("../../{work}/.git/modules/shared.txt"));
    assert_not_staged(dir.path(), NotStagedReason::GitFolderOutside);

    let (main_dir, main) = init_repo();
    write_file(main_dir.path(), "m.txt", "m\n");
    commit_all(&main, "main");
    let (dir, _repo) = make_conflicted_repo();
    std::fs::remove_file(dir.path().join("shared.txt")).unwrap();
    main.worktree("wt", &dir.path().join("shared.txt"), None)
        .unwrap();
    assert_not_staged(dir.path(), NotStagedReason::GitFolderOutside);
}

/// #213: a `gitdir:` with very many components is refused before holding a folder for each.
#[test]
fn mark_resolved_refuses_a_gitdir_path_that_is_too_deep() {
    let (dir, _repo) = make_conflicted_repo();
    nested_repo_at_shared(dir.path());
    gitdir_file_at_shared(dir.path(), &"x/".repeat(200));
    assert_not_staged(
        dir.path(),
        NotStagedReason::Unsupported("a git folder path that is too deep"),
    );
}

/// #213: a loose ref that exists but isn't a plain file (a link, a folder, or a link on the way
/// to it), or a ref name that isn't a plain path, isn't reported as "no commit".
#[test]
fn mark_resolved_reports_a_ref_that_is_not_a_plain_file_as_unsupported() {
    let (dir, _repo) = make_conflicted_repo();
    let head = nested_repo_at_shared(dir.path());
    let branch = nested_branch(dir.path());
    std::fs::remove_file(dir.path().join("shared.txt/.git").join(&branch)).unwrap();
    std::fs::create_dir(dir.path().join("shared.txt/.git").join(&branch)).unwrap();
    // Even with the branch in `packed-refs`: git would refuse the folder too.
    write_in_git_dir(dir.path(), "packed-refs", &format!("{head} {branch}\n"));
    assert_not_staged(
        dir.path(),
        NotStagedReason::Unsupported("a ref that isn't a regular file"),
    );

    let (dir, _repo) = make_conflicted_repo();
    nested_repo_at_shared(dir.path());
    write_in_git_dir(dir.path(), "HEAD", "ref: refs/heads/../../config\n");
    assert_not_staged(
        dir.path(),
        NotStagedReason::Unsupported("a malformed HEAD or ref"),
    );
}

/// #213: a ref name git's `check_refname_format` refuses is refused before anything is read
/// with it. `:` matters on Windows, where `name:stream` would open an NTFS alternate data
/// stream of the file `name`.
#[test]
fn mark_resolved_refuses_ref_names_git_refuses() {
    for name in [
        "refs/heads/main:stream",
        "refs/heads/a b",
        "refs/heads/a~1",
        "refs/heads/a^",
        "refs/heads/a?",
        "refs/heads/a*",
        "refs/heads/a[",
        "refs/heads/a\\b",
        "refs/heads/a\x01",
        "refs/heads/a\x7f",
        "refs/heads/a@{1}",
        "refs/heads/a..b",
        "refs/heads/.hidden",
        "refs/heads/x.lock",
        "refs/heads/x.",
        "refs/heads//x",
        "refs/heads/x/",
    ] {
        let (dir, _repo) = make_conflicted_repo();
        let head = nested_repo_at_shared(dir.path());
        write_in_git_dir(dir.path(), "HEAD", &format!("ref: {name}\n"));
        // Even where a file of that name holds a commit (here the stream's base file).
        write_in_git_dir(dir.path(), "refs/heads/main", &format!("{head}\n"));
        let repo = git2::Repository::open(dir.path()).unwrap();
        let error = git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap_err();
        assert!(
            matches!(
                error,
                MergeError::FolderNotStaged(
                    _,
                    NotStagedReason::Unsupported("a malformed HEAD or ref")
                )
            ),
            "{name:?}: {error:?}"
        );
    }
    // Names git accepts are read.
    for name in ["refs/heads/feature/x-1", "refs/heads/a.b", "refs/heads/@x"] {
        let (dir, _repo) = make_conflicted_repo();
        let head = nested_repo_at_shared(dir.path());
        write_in_git_dir(dir.path(), "HEAD", &format!("ref: {name}\n"));
        write_in_git_dir(dir.path(), name, &format!("{head}\n"));
        let repo = git2::Repository::open(dir.path()).unwrap();
        git_core::merge::mark_resolved(&repo, "shared.txt", false).unwrap();
        assert_eq!(
            staged_entry(&repo, "shared.txt"),
            (0o160000, head),
            "{name}"
        );
    }
}

/// #213 (Unix): a loose ref that is a link, or below a linked `refs/heads`, isn't followed and
/// isn't "no commit".
#[cfg(unix)]
#[test]
fn mark_resolved_does_not_follow_a_linked_ref() {
    let outside = tempfile::TempDir::new().unwrap();
    let (dir, _repo) = make_conflicted_repo();
    let head = nested_repo_at_shared(dir.path());
    let branch = nested_branch(dir.path());
    let loose = dir.path().join("shared.txt/.git").join(&branch);
    std::fs::write(outside.path().join("ref"), format!("{head}\n")).unwrap();
    std::fs::remove_file(&loose).unwrap();
    std::os::unix::fs::symlink(outside.path().join("ref"), &loose).unwrap();
    assert_not_staged(
        dir.path(),
        NotStagedReason::Unsupported("a ref that isn't a regular file"),
    );

    let (dir, _repo) = make_conflicted_repo();
    nested_repo_at_shared(dir.path());
    let branch = nested_branch(dir.path());
    let heads = dir.path().join("shared.txt/.git/refs/heads");
    let name = Path::new(&branch).file_name().unwrap();
    std::fs::create_dir_all(outside.path().join("heads")).unwrap();
    std::fs::write(outside.path().join("heads").join(name), format!("{head}\n")).unwrap();
    std::fs::remove_dir_all(&heads).unwrap();
    std::os::unix::fs::symlink(outside.path().join("heads"), &heads).unwrap();
    assert_not_staged(
        dir.path(),
        NotStagedReason::Unsupported("a ref that isn't a regular file"),
    );
}

/// #213 (Windows): a `gitdir:` naming a network share or a device is outside the work tree:
/// refused before anything is opened or looked up, so no share is contacted (and no credentials
/// are sent to its host).
#[cfg(windows)]
#[test]
fn mark_resolved_refuses_a_gitdir_on_a_network_share_or_device() {
    for target in [
        r"\\host.invalid\share\nested.git",
        r"//host.invalid/share/nested.git",
        r"\\?\UNC\host.invalid\share\nested.git",
        r"\\.\pipe\nested.git",
        r"\\?\C:\nested.git",
    ] {
        let (dir, _repo) = make_conflicted_repo();
        nested_repo_at_shared(dir.path());
        gitdir_file_at_shared(dir.path(), target);
        let started = std::time::Instant::now();
        assert_not_staged(dir.path(), NotStagedReason::GitFolderOutside);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "{target}"
        );
    }
}

/// Runs `f` on its own thread; `None` when it didn't finish within 5 seconds.
#[cfg(unix)]
fn within_5s<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_secs(5)).ok()
}

#[cfg(unix)]
fn mkfifo(path: &Path) {
    let c_path = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
    // SAFETY: a valid NUL-terminated path.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o644) }, 0);
}

/// A nested repository's `.git` that is a link isn't followed: the commit staged must come
/// from the work tree, never from a repository elsewhere (#213).
#[cfg(unix)]
#[test]
fn mark_resolved_does_not_follow_a_linked_git_folder_of_a_nested_repository() {
    let (outside_dir, outside) = init_repo();
    write_file(outside_dir.path(), "o.txt", "o\n");
    commit_all(&outside, "outside");
    let (dir, repo) = make_conflicted_repo();
    std::fs::remove_file(dir.path().join("shared.txt")).unwrap();
    std::fs::create_dir(dir.path().join("shared.txt")).unwrap();
    std::os::unix::fs::symlink(
        outside_dir.path().join(".git"),
        dir.path().join("shared.txt/.git"),
    )
    .unwrap();
    drop(repo);
    assert_not_staged(dir.path(), NotStagedReason::NotARepository);
}

/// A FIFO inside the nested repository (here its HEAD, then its branch) is never opened for
/// reading, so the worker can't hang on it (#213).
#[cfg(unix)]
#[test]
fn mark_resolved_does_not_block_on_a_fifo_in_a_nested_repository() {
    // A FIFO HEAD is as if there were no HEAD (no repository); a FIFO branch is a ref that isn't
    // a file, never "no commit".
    let cases = [
        ("HEAD", NotStagedReason::NotARepository),
        (
            "branch",
            NotStagedReason::Unsupported("a ref that isn't a regular file"),
        ),
    ];
    for (fifo_at, reason) in cases {
        let (dir, repo) = make_conflicted_repo();
        nested_repo_at_shared(dir.path());
        let git_dir = dir.path().join("shared.txt/.git");
        let fifo = match fifo_at {
            "HEAD" => git_dir.join("HEAD"),
            _ => {
                let head = std::fs::read_to_string(git_dir.join("HEAD")).unwrap();
                git_dir.join(head.trim().strip_prefix("ref: ").unwrap())
            }
        };
        std::fs::remove_file(&fifo).unwrap();
        mkfifo(&fifo);
        let root = dir.path().to_path_buf();
        drop(repo);
        let finished = within_5s(move || {
            let repo = git2::Repository::open(&root).unwrap();
            git_core::merge::mark_resolved(&repo, "shared.txt", false)
                .map_err(|error| error.to_string())
        });
        if finished.is_none() {
            // Unblock the stuck reader so the test process can exit.
            unblock_fifo(&fifo);
        }
        assert_eq!(
            finished,
            Some(Err(MergeError::FolderNotStaged(
                "shared.txt".into(),
                reason
            )
            .to_string())),
            "{fifo_at}: blocked on or staged a FIFO"
        );
        let repo = git2::Repository::open(dir.path()).unwrap();
        assert!(repo.index().unwrap().has_conflicts());
    }
}

/// A folder swapped for a FIFO after the read is never opened, so the worker can't hang (#213).
#[cfg(unix)]
#[test]
fn mark_resolved_does_not_block_on_a_fifo_swapped_in_for_a_folder() {
    let (dir, repo) = make_conflicted_repo();
    nested_repo_at_shared(dir.path());
    let folder = dir.path().join("shared.txt");
    let root = dir.path().to_path_buf();
    drop(repo);
    let swap = folder.clone();
    let finished = within_5s(move || {
        let repo = git2::Repository::open(&root).unwrap();
        git_core::merge::mark_resolved_with_pause(&repo, "shared.txt", false, &mut || {
            std::fs::remove_dir_all(&swap).unwrap();
            mkfifo(&swap);
        })
        .map_err(|error| error.to_string())
    });
    if finished.is_none() {
        unblock_fifo(&folder);
    }
    assert_eq!(
        finished,
        Some(Err(git_core::merge::MergeError::NotAFile(
            "shared.txt".into()
        )
        .to_string()))
    );
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert!(repo.index().unwrap().has_conflicts());
}

/// A folder swapped for a link to a repository outside after the read is neither staged as a
/// link nor as that repository's commit (#213).
#[cfg(unix)]
#[test]
fn mark_resolved_does_not_stage_a_link_swapped_in_for_a_folder() {
    let (outside_dir, outside) = init_repo();
    write_file(outside_dir.path(), "o.txt", "o\n");
    commit_all(&outside, "outside");
    let (dir, repo) = make_conflicted_repo();
    nested_repo_at_shared(dir.path());
    let folder = dir.path().join("shared.txt");
    let target = outside_dir.path().to_path_buf();
    let result = git_core::merge::mark_resolved_with_pause(&repo, "shared.txt", false, &mut || {
        std::fs::remove_dir_all(&folder).unwrap();
        std::os::unix::fs::symlink(&target, &folder).unwrap();
    });
    assert!(
        matches!(result, Err(MergeError::Unreadable(ref p)) if p == "shared.txt"),
        "{result:?}"
    );
    assert!(repo.index().unwrap().has_conflicts());
}

/// Opens the FIFO `path` for writing without blocking, retrying for 5 seconds, so a reader
/// stuck on it wakes up. Never blocks the test itself (a blocking open would wait forever if
/// nothing reads).
#[cfg(unix)]
fn unblock_fifo(path: &Path) {
    use std::os::unix::fs::OpenOptionsExt;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while std::time::Instant::now() < deadline {
        let writer = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path);
        if writer.is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Runs Mark resolved on `shared.txt` in `dir` on its own thread, with `swap` run once the
/// folder there is held and before anything in it is read; `None` when it didn't finish within
/// 5 seconds (`fifo` is then unblocked).
#[cfg(unix)]
fn mark_resolved_swapping_the_held_folder(
    dir: &Path,
    fifo: Option<std::path::PathBuf>,
    swap: impl FnMut() + Send + 'static,
) -> Option<Result<git_core::merge::MarkResolvedOutcome, String>> {
    let root = dir.to_path_buf();
    let mut swap = swap;
    let finished = within_5s(move || {
        let repo = git2::Repository::open(&root).unwrap();
        git_core::merge::mark_resolved_with_pauses(
            &repo,
            "shared.txt",
            false,
            &mut || {},
            &mut swap,
        )
        .map_err(|error| error.to_string())
    });
    if let (None, Some(fifo)) = (&finished, fifo) {
        unblock_fifo(&fifo);
    }
    finished
}

/// #213: once the folder is held, swapping its path for a FIFO changes nothing: the commit staged
/// is the held repository's, read through its handle, and the FIFO is never opened.
#[cfg(unix)]
#[test]
fn a_fifo_swapped_in_for_the_held_folder_is_never_opened() {
    let (dir, repo) = make_conflicted_repo();
    let head = nested_repo_at_shared(dir.path());
    drop(repo);
    let folder = dir.path().join("shared.txt");
    let moved = dir.path().join("moved");
    let (from, to) = (folder.clone(), moved.clone());
    let finished =
        mark_resolved_swapping_the_held_folder(dir.path(), Some(folder.clone()), move || {
            std::fs::rename(&from, &to).unwrap();
            mkfifo(&from);
        });
    assert_eq!(
        finished,
        Some(Ok(git_core::merge::MarkResolvedOutcome::Resolved))
    );
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert_eq!(staged_entry(&repo, "shared.txt"), (0o160000, head));
}

/// #213: once the folder is held, swapping its path for a link to a repository outside stages
/// neither the link nor that repository's commit: the held repository's commit is staged.
#[cfg(unix)]
#[test]
fn a_link_swapped_in_for_the_held_folder_is_never_followed() {
    let (outside_dir, outside) = init_repo();
    write_file(outside_dir.path(), "o.txt", "o\n");
    commit_all(&outside, "outside");
    let outside_head = outside.head().unwrap().target().unwrap();
    let (dir, repo) = make_conflicted_repo();
    let head = nested_repo_at_shared(dir.path());
    drop(repo);
    let folder = dir.path().join("shared.txt");
    let moved = dir.path().join("moved");
    let target = outside_dir.path().to_path_buf();
    let finished = mark_resolved_swapping_the_held_folder(dir.path(), None, move || {
        std::fs::rename(&folder, &moved).unwrap();
        std::os::unix::fs::symlink(&target, &folder).unwrap();
    });
    assert_eq!(
        finished,
        Some(Ok(git_core::merge::MarkResolvedOutcome::Resolved))
    );
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert_ne!(head, outside_head);
    assert_eq!(staged_entry(&repo, "shared.txt"), (0o160000, head));
}

/// #213: once the folder is held, a different repository moved in at its path isn't read.
#[cfg(unix)]
#[test]
fn another_repository_swapped_in_for_the_held_folder_is_not_read() {
    let (dir, repo) = make_conflicted_repo();
    let head = nested_repo_at_shared(dir.path());
    drop(repo);
    let (other_dir, other) = init_repo();
    write_file(other_dir.path(), "x.txt", "x\n");
    commit_all(&other, "other");
    let other_head = other.head().unwrap().target().unwrap();
    drop(other);
    let folder = dir.path().join("shared.txt");
    let moved = dir.path().join("moved");
    let other_path = other_dir.path().to_path_buf();
    let finished = mark_resolved_swapping_the_held_folder(dir.path(), None, move || {
        std::fs::rename(&folder, &moved).unwrap();
        std::fs::rename(&other_path, &folder).unwrap();
    });
    assert_eq!(
        finished,
        Some(Ok(git_core::merge::MarkResolvedOutcome::Resolved))
    );
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert_ne!(head, other_head);
    assert_eq!(staged_entry(&repo, "shared.txt"), (0o160000, head));
}

/// #213: once the folder is held, its HEAD swapped for a FIFO is never opened; the folder is
/// refused as one without a repository.
#[cfg(unix)]
#[test]
fn a_fifo_swapped_in_for_the_held_repositorys_head_is_never_opened() {
    let (dir, repo) = make_conflicted_repo();
    nested_repo_at_shared(dir.path());
    drop(repo);
    let head = dir.path().join("shared.txt/.git/HEAD");
    let fifo = head.clone();
    let finished =
        mark_resolved_swapping_the_held_folder(dir.path(), Some(head.clone()), move || {
            std::fs::remove_file(&fifo).unwrap();
            mkfifo(&fifo);
        });
    assert_eq!(
        finished,
        Some(Err(MergeError::FolderNotStaged(
            "shared.txt".into(),
            NotStagedReason::NotARepository
        )
        .to_string()))
    );
    let repo = git2::Repository::open(dir.path()).unwrap();
    assert!(repo.index().unwrap().has_conflicts());
}

#[test]
fn conflict_hunks_refuse_a_delete_modify_conflict() {
    let (_dir, repo) = make_delete_modify_conflict();

    let error = git_core::merge::conflict_hunks(&repo, "shared.txt").unwrap_err();

    assert!(
        matches!(error, git_core::merge::MergeError::NotATextConflict(_)),
        "{error:?}"
    );
}

#[test]
fn conflict_hunks_refuse_a_binary_conflict() {
    for (base, ours, theirs) in [
        ("a\0base\n", "a\0ours\n", "a\0theirs\n"),
        ("base\n", "a\0ours\n", "theirs\n"),
        ("base\n", "ours\n", "a\0theirs\n"),
        ("a\0base\n", "ours\n", "theirs\n"),
    ] {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "bin.dat", base);
        commit_all(&repo, "base commit");
        let main_branch = git_core::branch::list_branches(&repo).unwrap()[0]
            .name
            .clone();
        git_core::branch::create_branch(&repo, "feature", "HEAD").unwrap();
        write_file(dir.path(), "bin.dat", theirs);
        commit_all(&repo, "feature commit");
        git_core::branch::switch_branch(&repo, &main_branch).unwrap();
        write_file(dir.path(), "bin.dat", ours);
        commit_all(&repo, "main commit");
        git_core::merge::start_merge(&repo, "feature", git_core::merge::MergeOptions::default())
            .unwrap();

        let error = git_core::merge::conflict_hunks(&repo, "bin.dat").unwrap_err();

        assert!(
            matches!(error, git_core::merge::MergeError::BinaryConflict(ref path) if path == "bin.dat"),
            "{base:?} {ours:?} {theirs:?}: {error:?}"
        );
    }
}
