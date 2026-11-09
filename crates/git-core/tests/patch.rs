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

fn head_id(repo: &Repository) -> String {
    repo.head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string()
}

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
    let id = head_id(&repo);
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
    assert_eq!(head_id(&repo), id);
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
    assert_eq!(head_id(&repo), base.id().to_string());
}

#[test]
fn reverting_an_added_file_removes_it_and_applying_a_deleted_file_removes_it() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "keep.txt", "k\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "added.txt", "a\nb\n");
    commit_all(&repo, "add");
    let id = head_id(&repo);
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
    assert_eq!(head_id(&repo), id);
}

#[test]
fn a_series_applies_in_order_or_not_at_all() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", "one\n");
    commit_all(&repo, "base");
    let base = head_id(&repo);
    write_file(dir.path(), "f.txt", "two\n");
    commit_all(&repo, "second");
    let second = head_id(&repo);
    write_file(dir.path(), "f.txt", "three\n");
    write_file(dir.path(), "g.txt", "g\n");
    commit_all(&repo, "third");
    let third = head_id(&repo);
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
    let id = head_id(&repo);
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
    let id = head_id(&repo);
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
    let id = head_id(&repo);
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
    let id = head_id(&repo);
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
    let id = head_id(&repo);
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
    let base = head_id(&repo);
    write_file(dir.path(), "f.txt", "two\n");
    write_file(dir.path(), "h.txt", "two\n");
    commit_all(&repo, "both files");
    let first = head_id(&repo);
    write_file(dir.path(), "g.txt", "g\n");
    commit_all(&repo, "add g");
    let second = head_id(&repo);
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
    let base = head_id(&repo);
    std::fs::remove_file(dir.path().join("g.txt")).unwrap();
    commit_all(&repo, "delete g");
    let first = head_id(&repo);
    write_file(dir.path(), "g.txt", "new\n");
    commit_all(&repo, "add g again");
    let second = head_id(&repo);
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
