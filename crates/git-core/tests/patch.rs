//! #448: a commit's file or hunk reverted or applied in the working tree, patch text, and a
//! patch file applied.
mod common;

use common::{commit_all, init_repo, read, write_file};
use git2::Repository;
use git_core::diff::{commit_diff, DiffOptions};
use git_core::patch::{
    apply_commit_change, apply_patch, commit_file_patch, commits_patch, staged_patch, ApplyTarget,
    CommitHunk, PatchError,
};

fn lines(range: std::ops::Range<u32>) -> String {
    range.map(|i| format!("line {i}\n")).collect()
}

/// `f.txt` with 30 lines, then a commit that changes line 5 and line 25 (two hunks).
fn two_hunk_commit() -> (tempfile::TempDir, Repository, String) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", &lines(0..30));
    commit_all(&repo, "base");
    let changed = lines(0..30)
        .replace("line 5\n", "line five\n")
        .replace("line 25\n", "line twenty-five\n");
    write_file(dir.path(), "f.txt", &changed);
    commit_all(&repo, "change two lines");
    let id = common::head_id(&repo).to_string();
    (dir, repo, id)
}

fn hunk(repo: &Repository, id: &str, path: &str, index: usize, context_lines: u32) -> CommitHunk {
    let opts = DiffOptions {
        ignore_whitespace: false,
        context_lines,
        algorithm: git_core::diff::DiffAlgorithm::Default,
    };
    let hunks = commit_diff(repo, id, path, &opts).unwrap();
    CommitHunk {
        old_start: hunks[index].old_start,
        new_start: hunks[index].new_start,
        options: common::hunk_ctx(context_lines),
    }
}

fn status_is_unstaged_only(repo: &Repository) -> bool {
    let entries = git_core::status::status(repo).unwrap();
    !entries.is_empty() && entries.iter().all(|e| !e.staged)
}

#[test]
fn reverting_a_commit_file_lands_as_unstaged_changes() {
    let (dir, repo, id) = two_hunk_commit();
    apply_commit_change(&repo, &id, "f.txt", None, None, true, ApplyTarget::WorkTree).unwrap();
    assert_eq!(read(dir.path(), "f.txt"), lines(0..30));
    assert!(status_is_unstaged_only(&repo));
    // Nothing was committed.
    assert_eq!(common::head_id(&repo).to_string(), id);
}

#[test]
fn reverting_one_hunk_keeps_the_other() {
    let (dir, repo, id) = two_hunk_commit();
    let second = hunk(&repo, &id, "f.txt", 1, 3);
    apply_commit_change(
        &repo,
        &id,
        "f.txt",
        None,
        Some(second),
        true,
        ApplyTarget::WorkTree,
    )
    .unwrap();
    let text = read(dir.path(), "f.txt");
    assert!(text.contains("line five\n"));
    assert!(text.contains("line 25\n"));
    assert!(!text.contains("twenty-five"));
}

#[test]
fn a_hunk_shown_with_the_whole_file_reverts_after_lines_moved_above_it() {
    let (dir, repo, id) = two_hunk_commit();
    // Lines added above both hunks since the commit, and line 0 (shown as context in the
    // whole-file view, but more than three lines from the change) edited.
    let now = read(dir.path(), "f.txt").replacen("line 0\n", "intro\nLINE ZERO\nmore\n", 1);
    write_file(dir.path(), "f.txt", &now);
    let whole = hunk(
        &repo,
        &id,
        "f.txt",
        0,
        git_core::diff::WHOLE_FILE_CONTEXT_LINES,
    );
    // The whole-file view shows one hunk; its context no longer matches line for line, but the
    // change itself still applies once trimmed and moved.
    let result = apply_commit_change(
        &repo,
        &id,
        "f.txt",
        None,
        Some(whole),
        true,
        ApplyTarget::WorkTree,
    );
    result.unwrap();
    let text = read(dir.path(), "f.txt");
    assert!(
        text.contains("line 5\n") && text.contains("line 25\n"),
        "{text}"
    );
    assert!(text.starts_with("intro\n"));
}

#[test]
fn a_file_revert_finds_hunks_that_moved() {
    let (dir, repo, id) = two_hunk_commit();
    let now = format!("new top\n{}", read(dir.path(), "f.txt")).replace("line 15\n", "");
    write_file(dir.path(), "f.txt", &now);
    apply_commit_change(&repo, &id, "f.txt", None, None, true, ApplyTarget::WorkTree).unwrap();
    let text = read(dir.path(), "f.txt");
    assert_eq!(
        text,
        format!("new top\n{}", lines(0..30).replace("line 15\n", ""))
    );
}

#[test]
fn a_conflicting_revert_leaves_the_working_tree_unchanged() {
    let (dir, repo, id) = two_hunk_commit();
    // Line 25 changed again since: the second hunk can't revert.
    let now = read(dir.path(), "f.txt").replace("line twenty-five\n", "line 25 (edited)\n");
    write_file(dir.path(), "f.txt", &now);
    let err = apply_commit_change(&repo, &id, "f.txt", None, None, true, ApplyTarget::WorkTree)
        .unwrap_err();
    assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
    // Not even the first hunk landed.
    assert_eq!(read(dir.path(), "f.txt"), now);
}

#[test]
fn applying_a_commit_file_from_another_branch_cherry_picks_it_into_the_working_tree() {
    let (dir, repo, id) = two_hunk_commit();
    // Back on the base commit, with an unrelated edit at the end.
    let base = repo
        .find_commit(git2::Oid::from_str(&id).unwrap())
        .unwrap()
        .parent(0)
        .unwrap();
    repo.reset(base.as_object(), git2::ResetType::Hard, None)
        .unwrap();
    let now = format!("{}tail\n", lines(0..30));
    write_file(dir.path(), "f.txt", &now);
    let first = hunk(&repo, &id, "f.txt", 0, 3);
    apply_commit_change(
        &repo,
        &id,
        "f.txt",
        None,
        Some(first),
        false,
        ApplyTarget::WorkTree,
    )
    .unwrap();
    let text = read(dir.path(), "f.txt");
    assert!(text.contains("line five\n") && text.contains("line 25\n") && text.ends_with("tail\n"));
    assert_eq!(common::head_id(&repo).to_string(), base.id().to_string());
}

#[test]
fn reverting_an_added_file_removes_it_and_applying_a_deleted_file_removes_it() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "keep.txt", "k\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "added.txt", "a\nb\n");
    commit_all(&repo, "add");
    let id = common::head_id(&repo).to_string();
    apply_commit_change(
        &repo,
        &id,
        "added.txt",
        None,
        None,
        true,
        ApplyTarget::WorkTree,
    )
    .unwrap();
    assert!(!dir.path().join("added.txt").exists());
    // And applying it again brings it back.
    apply_commit_change(
        &repo,
        &id,
        "added.txt",
        None,
        None,
        false,
        ApplyTarget::WorkTree,
    )
    .unwrap();
    assert_eq!(read(dir.path(), "added.txt"), "a\nb\n");
}

#[test]
fn applying_with_the_index_stages_the_result() {
    let (dir, repo, id) = two_hunk_commit();
    apply_commit_change(
        &repo,
        &id,
        "f.txt",
        None,
        None,
        true,
        ApplyTarget::WorkTreeAndIndex,
    )
    .unwrap();
    assert_eq!(read(dir.path(), "f.txt"), lines(0..30));
    let entries = git_core::status::status(&repo).unwrap();
    assert!(entries.iter().all(|e| e.staged), "{entries:?}");
}

#[test]
fn a_stale_hunk_is_reported_as_not_found() {
    let (_dir, repo, id) = two_hunk_commit();
    let stale = CommitHunk {
        old_start: 999,
        new_start: 999,
        options: common::hunk_ctx(3),
    };
    let err = apply_commit_change(
        &repo,
        &id,
        "f.txt",
        None,
        Some(stale),
        true,
        ApplyTarget::WorkTree,
    )
    .unwrap_err();
    assert!(matches!(err, PatchError::HunkNotFound), "{err:?}");
}

#[test]
fn a_commit_patch_round_trips_through_apply_patch() {
    let (dir, repo, id) = two_hunk_commit();
    let patch = commits_patch(&repo, std::slice::from_ref(&id)).unwrap();
    let text = String::from_utf8(patch.clone()).unwrap();
    assert!(text.starts_with(&format!("From {id} ")), "{text}");
    assert!(text.contains("Subject: [PATCH] change two lines"), "{text}");
    // Undo the commit's change in the working tree, then apply the patch file.
    apply_commit_change(&repo, &id, "f.txt", None, None, true, ApplyTarget::WorkTree).unwrap();
    apply_patch(&repo, &patch).unwrap();
    assert!(read(dir.path(), "f.txt").contains("line twenty-five\n"));
    assert_eq!(common::head_id(&repo).to_string(), id);
}

#[test]
fn a_series_applies_in_order_or_not_at_all() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "one\n");
    commit_all(&repo, "base");
    let base = common::head_id(&repo).to_string();
    write_file(dir.path(), "f.txt", "two\n");
    commit_all(&repo, "second");
    let second = common::head_id(&repo).to_string();
    write_file(dir.path(), "f.txt", "three\n");
    write_file(dir.path(), "g.txt", "g\n");
    commit_all(&repo, "third");
    let third = common::head_id(&repo).to_string();
    let series = commits_patch(&repo, &[second.clone(), third.clone()]).unwrap();
    let text = String::from_utf8(series.clone()).unwrap();
    assert!(
        text.contains("[PATCH 1/2]") && text.contains("[PATCH 2/2]"),
        "{text}"
    );

    let base_commit = repo
        .find_commit(git2::Oid::from_str(&base).unwrap())
        .unwrap();
    repo.reset(base_commit.as_object(), git2::ResetType::Hard, None)
        .unwrap();
    apply_patch(&repo, &series).unwrap();
    assert_eq!(read(dir.path(), "f.txt"), "three\n");
    assert_eq!(read(dir.path(), "g.txt"), "g\n");

    // From the base again, but with g.txt already there: the second mail fails, and the first
    // one's change is put back.
    std::fs::remove_file(dir.path().join("g.txt")).unwrap();
    repo.reset(base_commit.as_object(), git2::ResetType::Hard, None)
        .unwrap();
    write_file(dir.path(), "g.txt", "mine\n");
    let err = apply_patch(&repo, &series).unwrap_err();
    assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
    assert_eq!(read(dir.path(), "f.txt"), "one\n");
    assert_eq!(read(dir.path(), "g.txt"), "mine\n");
}

#[test]
fn a_file_patch_holds_only_that_file() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    write_file(dir.path(), "b.txt", "b\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "a.txt", "A\n");
    write_file(dir.path(), "b.txt", "B\n");
    commit_all(&repo, "both");
    let id = common::head_id(&repo).to_string();
    let text = String::from_utf8(commit_file_patch(&repo, &id, "a.txt", None).unwrap()).unwrap();
    assert!(
        text.contains("a/a.txt") && !text.contains("b.txt"),
        "{text}"
    );
    assert!(matches!(
        commit_file_patch(&repo, &id, "c.txt", None),
        Err(PatchError::NothingToApply(_))
    ));
}

#[test]
fn the_staged_patch_holds_staged_changes_only() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    assert!(matches!(
        staged_patch(&repo),
        Err(PatchError::NothingStaged)
    ));
    write_file(dir.path(), "a.txt", "staged\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();
    write_file(dir.path(), "a.txt", "unstaged\n");
    let text = String::from_utf8(staged_patch(&repo).unwrap()).unwrap();
    assert!(
        text.contains("+staged") && !text.contains("unstaged"),
        "{text}"
    );
}

#[test]
fn a_binary_file_round_trips() {
    let (dir, repo) = init_repo();
    std::fs::write(dir.path().join("bin.dat"), [0u8, 1, 2, 3, 0, 255]).unwrap();
    commit_all(&repo, "base");
    std::fs::write(dir.path().join("bin.dat"), [0u8, 9, 9, 9, 0, 255, 7]).unwrap();
    commit_all(&repo, "change binary");
    let id = common::head_id(&repo).to_string();
    apply_commit_change(
        &repo,
        &id,
        "bin.dat",
        None,
        None,
        true,
        ApplyTarget::WorkTree,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("bin.dat")).unwrap(),
        vec![0u8, 1, 2, 3, 0, 255]
    );
    let patch = commit_file_patch(&repo, &id, "bin.dat", None).unwrap();
    apply_patch(&repo, &patch).unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("bin.dat")).unwrap(),
        vec![0u8, 9, 9, 9, 0, 255, 7]
    );
}

#[test]
fn a_patch_naming_a_path_outside_or_in_git_is_refused() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    for name in ["../escape.txt", ".git/config2", "sub/.GIT/x"] {
        let patch = format!(
            "diff --git a/{name} b/{name}\nnew file mode 100644\n--- /dev/null\n+++ b/{name}\n@@ -0,0 +1 @@\n+x\n"
        );
        let err = apply_patch(&repo, patch.as_bytes()).unwrap_err();
        assert!(
            matches!(err, PatchError::InvalidPath(_) | PatchError::Unreadable(_)),
            "{name}: {err:?}"
        );
    }
    assert!(!dir.path().parent().unwrap().join("escape.txt").exists());
}

#[test]
fn garbage_is_unreadable_and_an_empty_patch_is_refused() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    let err = apply_patch(&repo, b"hello, this is not a patch\n").unwrap_err();
    assert!(
        matches!(err, PatchError::EmptyPatch | PatchError::Unreadable(_)),
        "{err:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_patch_never_writes_through_a_linked_folder() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "d/f.txt", "one\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "d/f.txt", "two\n");
    commit_all(&repo, "change");
    let id = common::head_id(&repo).to_string();
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("f.txt"), "two\n").unwrap();
    common::replace_with_link(dir.path(), "d", outside.path());
    let err = apply_commit_change(
        &repo,
        &id,
        "d/f.txt",
        None,
        None,
        true,
        ApplyTarget::WorkTree,
    )
    .unwrap_err();
    assert!(matches!(err, PatchError::BeyondSymlink(_)), "{err:?}");
    assert_eq!(
        std::fs::read_to_string(outside.path().join("f.txt")).unwrap(),
        "two\n"
    );
}

#[test]
fn a_case_pair_is_refused_where_case_folds() {
    let (dir, repo) = init_repo();
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    write_file(dir.path(), "A.txt", "a\n");
    commit_all(&repo, "base");
    let patch = "diff --git a/a.txt b/a.txt\nnew file mode 100644\n--- /dev/null\n+++ b/a.txt\n@@ -0,0 +1 @@\n+x\n";
    let err = apply_patch(&repo, patch.as_bytes()).unwrap_err();
    assert!(matches!(err, PatchError::CasePair { .. }), "{err:?}");
}

#[test]
fn the_staged_patch_of_a_repository_without_commits_holds_the_staged_files() {
    // An unborn HEAD is no error: the staged files are all new.
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "first\n");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("a.txt")).unwrap();
    index.write().unwrap();

    let text = String::from_utf8(staged_patch(&repo).unwrap()).unwrap();

    assert!(
        text.contains("new file mode") && text.contains("+first"),
        "{text}"
    );
}

#[test]
fn the_staged_patch_reports_a_head_it_cannot_read() {
    // Only an unborn HEAD counts as "no commits yet"; a damaged one is an error, not an empty
    // tree to diff against.
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    std::fs::write(dir.path().join(".git/HEAD"), "not a reference\n").unwrap();

    let err = staged_patch(&repo).unwrap_err();

    assert!(matches!(err, PatchError::Git(_)), "{err:?}");
}

#[test]
fn a_hunk_at_the_end_of_a_file_without_a_final_newline_reverts_exactly() {
    // The trimmed hunk runs to the "No newline at end of file" marker after the last change, so
    // the file comes back without a final newline.
    let (dir, repo) = init_repo();
    let base = format!("{}last", lines(0..10));
    write_file(dir.path(), "f.txt", &base);
    commit_all(&repo, "base");
    write_file(dir.path(), "f.txt", &format!("{}LAST", lines(0..10)));
    commit_all(&repo, "change the last line");
    let id = common::head_id(&repo).to_string();
    let last = hunk(&repo, &id, "f.txt", 0, 3);

    apply_commit_change(
        &repo,
        &id,
        "f.txt",
        None,
        Some(last),
        true,
        ApplyTarget::WorkTree,
    )
    .unwrap();

    assert_eq!(read(dir.path(), "f.txt"), base);
}

#[test]
fn a_commit_message_line_starting_with_from_is_not_a_new_mail() {
    // Only `From <40 hex digits> Mon Sep 17 00:00:00 2001` starts a mail; a message line that
    // merely starts with "From " stays in its mail.
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "one\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "f.txt", "two\n");
    commit_all(
        &repo,
        "change f\n\nFrom now on f says two.\nFrom 0123456789 too.\n",
    );
    let id = common::head_id(&repo).to_string();
    let patch = commits_patch(&repo, std::slice::from_ref(&id)).unwrap();
    assert!(
        String::from_utf8_lossy(&patch).contains("\nFrom now on f says two.\n"),
        "{}",
        String::from_utf8_lossy(&patch)
    );
    apply_commit_change(&repo, &id, "f.txt", None, None, true, ApplyTarget::WorkTree).unwrap();

    apply_patch(&repo, &patch).unwrap();

    assert_eq!(read(dir.path(), "f.txt"), "two\n");
}

#[test]
fn a_patch_that_adds_a_file_over_an_untracked_one_names_it() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "new.txt", "mine\n");
    let patch = "diff --git a/new.txt b/new.txt\nnew file mode 100644\n--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+theirs\n";

    let err = apply_patch(&repo, patch.as_bytes()).unwrap_err();

    assert!(
        matches!(&err, PatchError::DoesNotApply(reason) if reason == "new.txt already exists in the working tree"),
        "{err:?}"
    );
    assert_eq!(read(dir.path(), "new.txt"), "mine\n");
}

#[test]
fn a_patch_that_renames_a_file_onto_an_untracked_one_names_it() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "old.txt", "content\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "new.txt", "mine\n");
    let patch = "diff --git a/old.txt b/new.txt\nsimilarity index 100%\nrename from old.txt\nrename to new.txt\n";

    let err = apply_patch(&repo, patch.as_bytes()).unwrap_err();

    assert!(
        matches!(&err, PatchError::DoesNotApply(reason) if reason == "new.txt already exists in the working tree"),
        "{err:?}"
    );
    assert_eq!(read(dir.path(), "new.txt"), "mine\n");
    assert_eq!(read(dir.path(), "old.txt"), "content\n");
}

#[test]
fn a_failed_series_puts_back_every_file_an_earlier_mail_changed() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "one\n");
    write_file(dir.path(), "h.txt", "one\n");
    commit_all(&repo, "base");
    let base = common::head_id(&repo).to_string();
    write_file(dir.path(), "f.txt", "two\n");
    write_file(dir.path(), "h.txt", "two\n");
    commit_all(&repo, "both files");
    let first = common::head_id(&repo).to_string();
    write_file(dir.path(), "g.txt", "g\n");
    commit_all(&repo, "add g");
    let second = common::head_id(&repo).to_string();
    let series = commits_patch(&repo, &[first, second]).unwrap();
    let base_commit = repo
        .find_commit(git2::Oid::from_str(&base).unwrap())
        .unwrap();
    std::fs::remove_file(dir.path().join("g.txt")).unwrap();
    repo.reset(base_commit.as_object(), git2::ResetType::Hard, None)
        .unwrap();
    write_file(dir.path(), "g.txt", "mine\n");

    let err = apply_patch(&repo, &series).unwrap_err();

    assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
    assert_eq!(read(dir.path(), "f.txt"), "one\n");
    assert_eq!(read(dir.path(), "h.txt"), "one\n");
    assert_eq!(read(dir.path(), "g.txt"), "mine\n");
}

#[test]
fn a_series_can_delete_a_file_and_add_it_back() {
    // Each mail applies to what the one before it left: the second adds the file the first
    // deleted.
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "one\n");
    write_file(dir.path(), "g.txt", "old\n");
    commit_all(&repo, "base");
    let base = common::head_id(&repo).to_string();
    std::fs::remove_file(dir.path().join("g.txt")).unwrap();
    commit_all(&repo, "delete g");
    let first = common::head_id(&repo).to_string();
    write_file(dir.path(), "g.txt", "new\n");
    commit_all(&repo, "add g again");
    let second = common::head_id(&repo).to_string();
    let series = commits_patch(&repo, &[first, second]).unwrap();
    let base_commit = repo
        .find_commit(git2::Oid::from_str(&base).unwrap())
        .unwrap();
    repo.reset(base_commit.as_object(), git2::ResetType::Hard, None)
        .unwrap();

    apply_patch(&repo, &series).unwrap();

    assert_eq!(read(dir.path(), "g.txt"), "new\n");
}

// Linux: on the drives that fold case, the file check above already finds `a.txt`.
#[cfg(target_os = "linux")]
#[test]
fn a_new_file_meeting_an_untracked_one_where_case_folds_does_not_apply() {
    // `core.ignorecase` on a drive that doesn't fold: the check for a file already there finds
    // no `a.txt`, but libgit2's checkout takes the untracked `A.txt` for it and refuses. That
    // refusal is the patch not applying, not a repository error, and nothing is written.
    let (dir, repo) = init_repo();
    write_file(dir.path(), "x.txt", "x\n");
    commit_all(&repo, "base");
    repo.config()
        .unwrap()
        .set_bool("core.ignorecase", true)
        .unwrap();
    drop(repo);
    let repo = Repository::open(dir.path()).unwrap();
    write_file(dir.path(), "A.txt", "mine\n");
    let patch = "diff --git a/a.txt b/a.txt\nnew file mode 100644\n--- /dev/null\n+++ b/a.txt\n@@ -0,0 +1 @@\n+theirs\n";

    let err = apply_patch(&repo, patch.as_bytes()).unwrap_err();

    assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
    assert_eq!(read(dir.path(), "A.txt"), "mine\n");
    assert!(!dir.path().join("a.txt").exists());
}

/// A two-mail series over the files `base` writes: the first mail makes `first`'s change, the
/// second adds `g.txt`. The repository is reset to the base, with an untracked `g.txt` there, so the second
/// mail doesn't apply.
fn failing_series(
    base: impl Fn(&std::path::Path),
    first: impl Fn(&std::path::Path),
) -> (tempfile::TempDir, Repository, Vec<u8>) {
    let (dir, repo) = init_repo();
    base(dir.path());
    commit_all(&repo, "base");
    let base_id = common::head_id(&repo).to_string();
    first(dir.path());
    // Deletions too.
    let mut index = repo.index().unwrap();
    index.update_all(["*"].iter(), None).unwrap();
    index.write().unwrap();
    commit_all(&repo, "first");
    let first_id = common::head_id(&repo).to_string();
    write_file(dir.path(), "g.txt", "g\n");
    commit_all(&repo, "add g");
    let second_id = common::head_id(&repo).to_string();
    let series = commits_patch(&repo, &[first_id, second_id]).unwrap();
    {
        let base_commit = repo
            .find_commit(git2::Oid::from_str(&base_id).unwrap())
            .unwrap();
        repo.reset(base_commit.as_object(), git2::ResetType::Hard, None)
            .unwrap();
    }
    write_file(dir.path(), "g.txt", "mine\n");
    (dir, repo, series)
}

/// [`failing_series`] whose first mail adds `new/dir/a.txt` (no `new/` in the base).
fn series_adding_a_folder_then_failing() -> (tempfile::TempDir, Repository, Vec<u8>) {
    failing_series(
        |dir| write_file(dir, "f.txt", "one\n"),
        |dir| write_file(dir, "new/dir/a.txt", "a\n"),
    )
}

#[test]
fn a_failed_series_removes_the_folders_an_earlier_mail_created() {
    let (dir, repo, series) = series_adding_a_folder_then_failing();
    let err = apply_patch(&repo, &series).unwrap_err();
    assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
    assert!(!dir.path().join("new").exists());
    assert_eq!(read(dir.path(), "g.txt"), "mine\n");
}

#[test]
fn a_failed_series_keeps_a_folder_that_was_there_before() {
    let (dir, repo, series) = series_adding_a_folder_then_failing();
    std::fs::create_dir(dir.path().join("new")).unwrap();
    let err = apply_patch(&repo, &series).unwrap_err();
    assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
    assert!(dir.path().join("new").is_dir());
    assert!(!dir.path().join("new/dir").exists());
}

#[test]
fn a_failed_series_puts_back_a_folder_an_earlier_mail_replaced_with_a_file() {
    let (dir, repo, series) = failing_series(
        |dir| write_file(dir, "foo/bar.txt", "bar\n"),
        |dir| {
            std::fs::remove_dir_all(dir.join("foo")).unwrap();
            write_file(dir, "foo", "file\n");
        },
    );
    let err = apply_patch(&repo, &series).unwrap_err();
    assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
    assert_eq!(read(dir.path(), "foo/bar.txt"), "bar\n");
}

/// #563: patches that change, delete or rename a symbolic link apply to the link itself, as
/// `git apply` does, never to the file it points to.
#[cfg(unix)]
mod links {
    use super::*;
    use std::path::Path;

    fn link(dir: &Path, name: &str, target: &str) {
        let at = dir.join(name);
        if at.symlink_metadata().is_ok() {
            std::fs::remove_file(&at).unwrap();
        }
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(target, at).unwrap();
    }

    fn target(dir: &Path, name: &str) -> String {
        let at = dir.join(name);
        assert!(common::is_link(dir, name), "{name} is not a link");
        std::fs::read_link(at)
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    fn commit_everything(repo: &Repository, message: &str) {
        let mut index = repo.index().unwrap();
        index.update_all(["*"].iter(), None).unwrap();
        index.write().unwrap();
        commit_all(repo, message);
    }

    fn reset_to(repo: &Repository, id: &str) {
        let commit = repo.find_commit(git2::Oid::from_str(id).unwrap()).unwrap();
        repo.reset(commit.as_object(), git2::ResetType::Hard, None)
            .unwrap();
    }

    /// `f.txt`, `h.txt` and `l -> f.txt` committed, then `change` committed; the working tree
    /// is reset to the base. Returns the change's id and its patch.
    fn link_change(change: impl Fn(&Path)) -> (tempfile::TempDir, Repository, String, Vec<u8>) {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "f.txt", "f\n");
        write_file(dir.path(), "h.txt", "h\n");
        link(dir.path(), "l", "f.txt");
        commit_all(&repo, "base");
        let base = common::head_id(&repo).to_string();
        change(dir.path());
        commit_everything(&repo, "change");
        let id = common::head_id(&repo).to_string();
        let patch = commits_patch(&repo, std::slice::from_ref(&id)).unwrap();
        reset_to(&repo, &base);
        (dir, repo, id, patch)
    }

    fn files_untouched(dir: &Path) {
        assert_eq!(read(dir, "f.txt"), "f\n");
        assert_eq!(read(dir, "h.txt"), "h\n");
    }

    #[test]
    fn a_patch_retargets_a_link() {
        let (dir, repo, _, patch) = link_change(|dir| link(dir, "l", "h.txt"));
        apply_patch(&repo, &patch).unwrap();
        assert_eq!(target(dir.path(), "l"), "h.txt");
        files_untouched(dir.path());
    }

    #[test]
    fn a_patch_deletes_a_link() {
        let (dir, repo, _, patch) = link_change(|dir| std::fs::remove_file(dir.join("l")).unwrap());
        apply_patch(&repo, &patch).unwrap();
        assert!(dir.path().join("l").symlink_metadata().is_err());
        files_untouched(dir.path());
    }

    #[test]
    fn a_patch_renames_a_link() {
        let (dir, repo, _, patch) = link_change(|dir| {
            std::fs::remove_file(dir.join("l")).unwrap();
            link(dir, "sub/m", "../f.txt");
        });
        // A folder for the new name, a new target: shown as a delete and an add.
        apply_patch(&repo, &patch).unwrap();
        assert!(dir.path().join("l").symlink_metadata().is_err());
        assert_eq!(target(dir.path(), "sub/m"), "../f.txt");
        files_untouched(dir.path());
    }

    #[test]
    fn a_patch_renames_a_link_it_keeps_pointing_the_same_way() {
        let (dir, repo, _, patch) = link_change(|dir| {
            std::fs::remove_file(dir.join("l")).unwrap();
            link(dir, "m", "f.txt");
        });
        apply_patch(&repo, &patch).unwrap();
        assert!(dir.path().join("l").symlink_metadata().is_err());
        assert_eq!(target(dir.path(), "m"), "f.txt");
        files_untouched(dir.path());
    }

    #[test]
    fn a_patch_changes_a_link_and_a_file_together() {
        let (dir, repo, _, patch) = link_change(|dir| {
            link(dir, "l", "h.txt");
            write_file(dir, "h.txt", "h2\n");
        });
        apply_patch(&repo, &patch).unwrap();
        assert_eq!(target(dir.path(), "l"), "h.txt");
        assert_eq!(read(dir.path(), "h.txt"), "h2\n");
        assert_eq!(read(dir.path(), "f.txt"), "f\n");
    }

    #[test]
    fn a_link_pointing_elsewhere_now_refuses_the_whole_patch() {
        let (dir, repo, _, patch) = link_change(|dir| {
            link(dir, "l", "h.txt");
            write_file(dir, "h.txt", "h2\n");
        });
        link(dir.path(), "l", "other.txt");
        let err = apply_patch(&repo, &patch).unwrap_err();
        assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
        assert_eq!(target(dir.path(), "l"), "other.txt");
        files_untouched(dir.path());
    }

    #[test]
    fn a_link_patch_refuses_a_file_standing_where_the_link_was() {
        let (dir, repo, _, patch) = link_change(|dir| link(dir, "l", "h.txt"));
        std::fs::remove_file(dir.path().join("l")).unwrap();
        // The link's target as a plain file: still not a link.
        write_file(dir.path(), "l", "f.txt");
        let err = apply_patch(&repo, &patch).unwrap_err();
        assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
        assert_eq!(read(dir.path(), "l"), "f.txt");
    }

    #[test]
    fn a_patch_deleting_a_missing_link_does_not_apply() {
        let (dir, repo, _, patch) = link_change(|dir| std::fs::remove_file(dir.join("l")).unwrap());
        std::fs::remove_file(dir.path().join("l")).unwrap();
        let err = apply_patch(&repo, &patch).unwrap_err();
        assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
    }

    #[test]
    fn a_patch_adding_a_link_over_an_untracked_one_names_it() {
        let (dir, repo, _, patch) = link_change(|dir| link(dir, "m", "h.txt"));
        link(dir.path(), "m", "f.txt");
        let err = apply_patch(&repo, &patch).unwrap_err();
        assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
        assert_eq!(target(dir.path(), "m"), "f.txt");
    }

    #[test]
    fn reverting_and_applying_a_commits_link_change() {
        let (dir, repo, id, _) = link_change(|dir| link(dir, "l", "h.txt"));
        apply_commit_change(&repo, &id, "l", None, None, false, ApplyTarget::WorkTree).unwrap();
        assert_eq!(target(dir.path(), "l"), "h.txt");
        apply_commit_change(&repo, &id, "l", None, None, true, ApplyTarget::WorkTree).unwrap();
        assert_eq!(target(dir.path(), "l"), "f.txt");
        files_untouched(dir.path());
    }

    #[test]
    fn reverting_a_commits_link_hunk() {
        let (dir, repo, id, _) = link_change(|dir| link(dir, "l", "h.txt"));
        reset_to(&repo, &id);
        let only = hunk(&repo, &id, "l", 0, 3);
        apply_commit_change(
            &repo,
            &id,
            "l",
            None,
            Some(only),
            true,
            ApplyTarget::WorkTree,
        )
        .unwrap();
        assert_eq!(target(dir.path(), "l"), "f.txt");
    }

    #[test]
    fn reverting_a_commits_link_deletion_brings_the_link_back() {
        let (dir, repo, id, _) = link_change(|dir| std::fs::remove_file(dir.join("l")).unwrap());
        reset_to(&repo, &id);
        apply_commit_change(&repo, &id, "l", None, None, true, ApplyTarget::WorkTree).unwrap();
        assert_eq!(target(dir.path(), "l"), "f.txt");
    }

    #[test]
    fn applying_a_link_change_with_the_index_stages_it() {
        let (dir, repo, id, _) = link_change(|dir| {
            link(dir, "l", "h.txt");
            std::fs::remove_file(dir.join("f.txt")).unwrap();
        });
        apply_commit_change(
            &repo,
            &id,
            "l",
            None,
            None,
            false,
            ApplyTarget::WorkTreeAndIndex,
        )
        .unwrap();
        assert_eq!(target(dir.path(), "l"), "h.txt");
        let entries = git_core::status::status(&repo).unwrap();
        assert!(
            !entries.is_empty() && entries.iter().all(|e| e.staged && e.path == "l"),
            "{entries:?}"
        );
        let index = repo.index().unwrap();
        let entry = index.get_path(Path::new("l"), 0).unwrap();
        assert_eq!(entry.mode, 0o120000);
        assert_eq!(repo.find_blob(entry.id).unwrap().content(), b"h.txt");
        // The file the change deleted is not part of it.
        assert_eq!(read(dir.path(), "f.txt"), "f\n");
    }

    #[test]
    fn applying_a_link_deletion_with_the_index_stages_it() {
        let (dir, repo, id, _) = link_change(|dir| std::fs::remove_file(dir.join("l")).unwrap());
        apply_commit_change(
            &repo,
            &id,
            "l",
            None,
            None,
            false,
            ApplyTarget::WorkTreeAndIndex,
        )
        .unwrap();
        assert!(dir.path().join("l").symlink_metadata().is_err());
        let index = repo.index().unwrap();
        assert!(index.get_path(Path::new("l"), 0).is_none());
    }

    #[test]
    fn applying_with_the_index_refuses_a_link_with_unstaged_changes() {
        let (dir, repo, id, _) = link_change(|dir| link(dir, "l", "h.txt"));
        // Staged l -> other.txt, then the working tree put back: the index no longer matches.
        link(dir.path(), "l", "other.txt");
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("l")).unwrap();
        index.write().unwrap();
        link(dir.path(), "l", "f.txt");
        let err = apply_commit_change(
            &repo,
            &id,
            "l",
            None,
            None,
            false,
            ApplyTarget::WorkTreeAndIndex,
        )
        .unwrap_err();
        assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
        assert_eq!(target(dir.path(), "l"), "f.txt");
    }

    #[test]
    fn a_patch_turning_a_file_into_a_link_in_one_change_does_not_apply() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "f.txt", "f\n");
        commit_all(&repo, "base");
        let patch = "diff --git a/f.txt b/f.txt\nold mode 100644\nnew mode 120000\nindex 6a69f92..b6fc4c6\n--- a/f.txt\n+++ b/f.txt\n@@ -1 +1 @@\n-f\n+h.txt\n\\ No newline at end of file\n";
        let err = apply_patch(&repo, patch.as_bytes()).unwrap_err();
        assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
        assert_eq!(read(dir.path(), "f.txt"), "f\n");
    }

    #[test]
    fn a_link_added_with_the_index_over_a_staged_name_does_not_apply() {
        let (dir, repo, id, _) = link_change(|dir| link(dir, "m", "h.txt"));
        // `m` staged, then removed from the working tree only.
        write_file(dir.path(), "m", "mine\n");
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("m")).unwrap();
        index.write().unwrap();
        std::fs::remove_file(dir.path().join("m")).unwrap();
        let err = apply_commit_change(
            &repo,
            &id,
            "m",
            None,
            None,
            false,
            ApplyTarget::WorkTreeAndIndex,
        )
        .unwrap_err();
        assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
        assert!(dir.path().join("m").symlink_metadata().is_err());
    }

    #[test]
    fn a_link_renamed_with_the_index_onto_a_staged_name_does_not_apply() {
        let (dir, repo, id, _) = link_change(|dir| {
            std::fs::remove_file(dir.join("l")).unwrap();
            link(dir, "m", "f.txt");
        });
        write_file(dir.path(), "m", "mine\n");
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("m")).unwrap();
        index.write().unwrap();
        std::fs::remove_file(dir.path().join("m")).unwrap();
        let err = apply_commit_change(
            &repo,
            &id,
            "m",
            Some("l"),
            None,
            false,
            ApplyTarget::WorkTreeAndIndex,
        )
        .unwrap_err();
        assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
        assert_eq!(target(dir.path(), "l"), "f.txt");
        assert!(dir.path().join("m").symlink_metadata().is_err());
    }

    #[test]
    fn a_link_renamed_with_the_index_is_staged() {
        let (dir, repo, id, _) = link_change(|dir| {
            std::fs::remove_file(dir.join("l")).unwrap();
            link(dir, "m", "f.txt");
        });
        apply_commit_change(
            &repo,
            &id,
            "m",
            Some("l"),
            None,
            false,
            ApplyTarget::WorkTreeAndIndex,
        )
        .unwrap();
        assert_eq!(target(dir.path(), "m"), "f.txt");
        let index = repo.index().unwrap();
        assert!(index.get_path(Path::new("l"), 0).is_none());
        assert_eq!(index.get_path(Path::new("m"), 0).unwrap().mode, 0o120000);
    }

    #[test]
    fn a_patch_renaming_a_link_into_a_file_does_not_apply() {
        let (dir, repo, _, _) = link_change(|dir| write_file(dir, "h.txt", "h2\n"));
        let patch = "diff --git a/l b/m\nold mode 120000\nnew mode 100644\nsimilarity index 50%\nrename from l\nrename to m\nindex 6a69f92..b6fc4c6\n--- a/l\n+++ b/m\n@@ -1 +1 @@\n-f.txt\n\\ No newline at end of file\n+text\n";
        let err = apply_patch(&repo, patch.as_bytes()).unwrap_err();
        assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
        assert_eq!(target(dir.path(), "l"), "f.txt");
        assert!(dir.path().join("m").symlink_metadata().is_err());
    }

    #[test]
    fn a_link_added_with_the_index_is_staged() {
        let (dir, repo, id, _) = link_change(|dir| link(dir, "m", "h.txt"));
        apply_commit_change(
            &repo,
            &id,
            "m",
            None,
            None,
            false,
            ApplyTarget::WorkTreeAndIndex,
        )
        .unwrap();
        assert_eq!(target(dir.path(), "m"), "h.txt");
        let index = repo.index().unwrap();
        let entry = index.get_path(Path::new("m"), 0).unwrap();
        assert_eq!(entry.mode, 0o120000);
    }

    #[test]
    fn a_failed_series_puts_back_a_link_an_earlier_mail_retargeted() {
        let (dir, repo, series) = failing_series(
            |dir| {
                write_file(dir, "f.txt", "f\n");
                write_file(dir, "h.txt", "h\n");
                link(dir, "l", "f.txt");
            },
            |dir| link(dir, "l", "h.txt"),
        );
        let err = apply_patch(&repo, &series).unwrap_err();
        assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
        assert_eq!(target(dir.path(), "l"), "f.txt");
        files_untouched(dir.path());
    }

    #[test]
    fn a_failed_series_puts_back_a_link_an_earlier_mail_deleted() {
        let (dir, repo, series) = failing_series(
            |dir| {
                write_file(dir, "f.txt", "f\n");
                link(dir, "l", "f.txt");
            },
            |dir| std::fs::remove_file(dir.join("l")).unwrap(),
        );
        let err = apply_patch(&repo, &series).unwrap_err();
        assert!(matches!(err, PatchError::DoesNotApply(_)), "{err:?}");
        assert_eq!(target(dir.path(), "l"), "f.txt");
    }
}
