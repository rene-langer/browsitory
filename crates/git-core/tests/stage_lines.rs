mod common;

use git_core::diff::{working_diff, DiffOptions};
use git_core::stage::{stage_file, StageError};

/// The displayed content of hunk `(o, n)` as the UI would send it: every line's `content`, in
/// order, from `working_diff` with the same context. Empty when no such hunk exists.
fn shown(
    repo: &git2::Repository,
    path: &str,
    staged: bool,
    o: u32,
    n: u32,
    ctx: u32,
) -> Vec<String> {
    let opts = DiffOptions {
        ignore_whitespace: false,
        context_lines: ctx,
    };
    working_diff(repo, path, staged, &opts)
        .unwrap()
        .into_iter()
        .find(|h| h.old_start == o && h.new_start == n)
        .map(|h| h.lines.into_iter().map(|l| l.content).collect())
        .unwrap_or_default()
}

/// Line ops against the hunk exactly as currently displayed (the common, non-stale case).
fn stage_lines(
    repo: &git2::Repository,
    path: &str,
    o: u32,
    n: u32,
    ctx: u32,
    idx: &[u32],
) -> Result<(), StageError> {
    let expected = shown(repo, path, false, o, n, ctx);
    git_core::stage::stage_lines(repo, path, o, n, ctx, idx, &expected)
}

fn unstage_lines(
    repo: &git2::Repository,
    path: &str,
    o: u32,
    n: u32,
    ctx: u32,
    idx: &[u32],
) -> Result<(), StageError> {
    let expected = shown(repo, path, true, o, n, ctx);
    git_core::stage::unstage_lines(repo, path, o, n, ctx, idx, &expected)
}

fn discard_lines(
    repo: &git2::Repository,
    path: &str,
    o: u32,
    n: u32,
    ctx: u32,
    idx: &[u32],
) -> Result<(), StageError> {
    let expected = shown(repo, path, false, o, n, ctx);
    git_core::stage::discard_lines(repo, path, o, n, ctx, idx, &expected)
}

fn index_content(repo: &git2::Repository, path: &str) -> Option<String> {
    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    let entry = index.get_path(std::path::Path::new(path), 0)?;
    Some(String::from_utf8(repo.find_blob(entry.id).unwrap().content().to_vec()).unwrap())
}

fn workdir_content(dir: &std::path::Path, path: &str) -> String {
    std::fs::read_to_string(dir.join(path)).unwrap()
}

/// base "a\nb\nc\n" → workdir "a\nB\nc\nd\n". Hunk lines (context 3):
/// 0 " a", 1 "-b", 2 "+B", 3 " c", 4 "+d".
fn setup_abc() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a\nb\nc\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nB\nc\nd\n");
    (dir, repo)
}

fn only_hunk(repo: &git2::Repository, staged: bool) -> (u32, u32) {
    let hunks = working_diff(repo, "f.txt", staged, &DiffOptions::default()).unwrap();
    assert_eq!(hunks.len(), 1);
    (hunks[0].old_start, hunks[0].new_start)
}

#[test]
fn stages_only_the_selected_added_line() {
    let (dir, repo) = setup_abc();
    let (o, n) = only_hunk(&repo, false);
    stage_lines(&repo, "f.txt", o, n, 3, &[4]).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc\nd\n");
    assert_eq!(workdir_content(dir.path(), "f.txt"), "a\nB\nc\nd\n");
}

#[test]
fn stages_only_the_selected_removed_line() {
    let (_dir, repo) = setup_abc();
    let (o, n) = only_hunk(&repo, false);
    stage_lines(&repo, "f.txt", o, n, 3, &[1]).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nc\n");
}

#[test]
fn stages_a_mixed_selection() {
    let (_dir, repo) = setup_abc();
    let (o, n) = only_hunk(&repo, false);
    stage_lines(&repo, "f.txt", o, n, 3, &[1, 2]).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nB\nc\n");
}

#[test]
fn stages_selected_lines_of_an_untracked_file() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "seed.txt", "seed\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "new.txt", "x\ny\nz\n");
    let hunks = working_diff(&repo, "new.txt", false, &DiffOptions::default()).unwrap();
    stage_lines(
        &repo,
        "new.txt",
        hunks[0].old_start,
        hunks[0].new_start,
        3,
        &[1],
    )
    .unwrap();
    assert_eq!(index_content(&repo, "new.txt").unwrap(), "y\n");
}

#[test]
fn stages_an_added_last_line_without_trailing_newline() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a\nb\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nb\nc");
    let hunks = working_diff(&repo, "f.txt", false, &DiffOptions::default()).unwrap();
    let add = hunks[0]
        .lines
        .iter()
        .position(|l| l.content.starts_with('c'))
        .unwrap() as u32;
    stage_lines(
        &repo,
        "f.txt",
        hunks[0].old_start,
        hunks[0].new_start,
        3,
        &[add],
    )
    .unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc");
}

#[test]
fn staging_every_changed_line_of_a_no_eol_file_matches_the_workdir() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a\nb");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nb\nc");
    let hunks = working_diff(&repo, "f.txt", false, &DiffOptions::default()).unwrap();
    let changed: Vec<u32> = hunks[0]
        .lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.origin != git_core::diff::DiffLineOrigin::Context)
        .map(|(i, _)| i as u32)
        .collect();
    stage_lines(
        &repo,
        "f.txt",
        hunks[0].old_start,
        hunks[0].new_start,
        3,
        &changed,
    )
    .unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc");
}

#[test]
fn a_selection_that_strands_a_no_eol_line_fails_cleanly() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nb\n");
    let hunks = working_diff(&repo, "f.txt", false, &DiffOptions::default()).unwrap();
    let add_b = hunks[0]
        .lines
        .iter()
        .position(|l| l.content == "b\n")
        .unwrap() as u32;

    let result = stage_lines(
        &repo,
        "f.txt",
        hunks[0].old_start,
        hunks[0].new_start,
        3,
        &[add_b],
    );

    assert!(
        matches!(result, Err(StageError::SelectionNotApplicable)),
        "{result:?}"
    );
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a");
    assert_eq!(workdir_content(dir.path(), "f.txt"), "a\nb\n");
}

#[test]
fn stage_lines_with_zero_context_matches_the_displayed_hunk() {
    let (_dir, repo) = setup_abc();
    let zero = DiffOptions {
        ignore_whitespace: false,
        context_lines: 0,
    };
    let hunks = working_diff(&repo, "f.txt", false, &zero).unwrap();
    // With zero context: hunk 0 = "-b","+B"; hunk 1 = "+d".
    assert_eq!(hunks.len(), 2);
    stage_lines(
        &repo,
        "f.txt",
        hunks[1].old_start,
        hunks[1].new_start,
        0,
        &[0],
    )
    .unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc\nd\n");
}

#[test]
fn unstages_only_the_selected_line() {
    let (_dir, repo) = setup_abc();
    stage_file(&repo, "f.txt").unwrap();
    let (o, n) = only_hunk(&repo, true);
    // HEAD→index hunk has the same layout: 0 " a", 1 "-b", 2 "+B", 3 " c", 4 "+d".
    unstage_lines(&repo, "f.txt", o, n, 3, &[4]).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nB\nc\n");
}

#[test]
fn unstaging_a_removed_line_restores_it_in_the_index() {
    let (_dir, repo) = setup_abc();
    stage_file(&repo, "f.txt").unwrap();
    let (o, n) = only_hunk(&repo, true);
    unstage_lines(&repo, "f.txt", o, n, 3, &[1]).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nB\nc\nd\n");
}

#[test]
fn discards_only_the_selected_lines() {
    let (dir, repo) = setup_abc();
    let (o, n) = only_hunk(&repo, false);
    discard_lines(&repo, "f.txt", o, n, 3, &[1, 2]).unwrap();
    assert_eq!(
        workdir_content(dir.path(), "f.txt").replace("\r\n", "\n"),
        "a\nb\nc\nd\n"
    );
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc\n");
}

#[test]
fn stale_hunk_is_reported() {
    let (_dir, repo) = setup_abc();
    assert!(matches!(
        stage_lines(&repo, "f.txt", 99, 99, 3, &[1]),
        Err(StageError::HunkNotFound)
    ));
}

#[test]
fn out_of_range_index_is_reported_as_stale() {
    let (_dir, repo) = setup_abc();
    let (o, n) = only_hunk(&repo, false);
    assert!(matches!(
        stage_lines(&repo, "f.txt", o, n, 3, &[42]),
        Err(StageError::HunkNotFound)
    ));
}

#[test]
fn context_and_empty_selections_are_rejected() {
    let (_dir, repo) = setup_abc();
    let (o, n) = only_hunk(&repo, false);
    assert!(matches!(
        stage_lines(&repo, "f.txt", o, n, 3, &[0]),
        Err(StageError::ContextLineSelected)
    ));
    assert!(matches!(
        stage_lines(&repo, "f.txt", o, n, 3, &[]),
        Err(StageError::EmptySelection)
    ));
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc\n");
}

#[test]
fn stages_a_removed_line_of_a_zero_context_hunk() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a\nb\nc\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nc\n");
    let zero = DiffOptions {
        ignore_whitespace: false,
        context_lines: 0,
    };
    let hunks = working_diff(&repo, "f.txt", false, &zero).unwrap();
    stage_lines(
        &repo,
        "f.txt",
        hunks[0].old_start,
        hunks[0].new_start,
        0,
        &[0],
    )
    .unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nc\n");
}

#[test]
fn discards_an_added_line_of_a_zero_context_hunk() {
    let (dir, repo) = setup_abc();
    let zero = DiffOptions {
        ignore_whitespace: false,
        context_lines: 0,
    };
    let hunks = working_diff(&repo, "f.txt", false, &zero).unwrap();
    discard_lines(
        &repo,
        "f.txt",
        hunks[1].old_start,
        hunks[1].new_start,
        0,
        &[0],
    )
    .unwrap();
    assert_eq!(
        workdir_content(dir.path(), "f.txt").replace("\r\n", "\n"),
        "a\nB\nc\n"
    );
}

#[test]
fn unstages_a_line_from_the_middle_with_zero_context() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a\nb\nc\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nx\nb\nc\ny\n");
    stage_file(&repo, "f.txt").unwrap();
    let zero = DiffOptions {
        ignore_whitespace: false,
        context_lines: 0,
    };
    let hunks = working_diff(&repo, "f.txt", true, &zero).unwrap();
    assert_eq!(hunks.len(), 2);
    unstage_lines(
        &repo,
        "f.txt",
        hunks[1].old_start,
        hunks[1].new_start,
        0,
        &[0],
    )
    .unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nx\nb\nc\n");
}

// --- Reverse direction with a missing final newline (fix round 1) ---

#[test]
fn discarding_every_changed_line_restores_a_no_eol_base() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nb\n");
    let hunks = working_diff(&repo, "f.txt", false, &DiffOptions::default()).unwrap();
    discard_lines(
        &repo,
        "f.txt",
        hunks[0].old_start,
        hunks[0].new_start,
        3,
        &changed(&hunks[0]),
    )
    .unwrap();
    assert_eq!(workdir_content(dir.path(), "f.txt"), "a");
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a");
}

#[test]
fn discarding_only_an_added_line_after_a_no_eol_base_keeps_the_newline_fix() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nb\n");
    let hunks = working_diff(&repo, "f.txt", false, &DiffOptions::default()).unwrap();
    let add_b = hunks[0]
        .lines
        .iter()
        .position(|l| l.content == "b\n")
        .unwrap() as u32;
    discard_lines(
        &repo,
        "f.txt",
        hunks[0].old_start,
        hunks[0].new_start,
        3,
        &[add_b],
    )
    .unwrap();
    assert_eq!(workdir_content(dir.path(), "f.txt"), "a\n");
}

#[test]
fn unstaging_every_changed_line_restores_a_no_eol_head() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a\nb");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nb\nc");
    stage_file(&repo, "f.txt").unwrap();
    let hunks = working_diff(&repo, "f.txt", true, &DiffOptions::default()).unwrap();
    unstage_lines(
        &repo,
        "f.txt",
        hunks[0].old_start,
        hunks[0].new_start,
        3,
        &changed(&hunks[0]),
    )
    .unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb");
    assert_eq!(workdir_content(dir.path(), "f.txt"), "a\nb\nc");
}

#[test]
fn unstaging_only_the_last_added_line_after_a_no_eol_head() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a\nb");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nb\nc");
    stage_file(&repo, "f.txt").unwrap();
    let hunks = working_diff(&repo, "f.txt", true, &DiffOptions::default()).unwrap();
    let add_c = hunks[0]
        .lines
        .iter()
        .position(|l| l.content == "c")
        .unwrap() as u32;
    unstage_lines(
        &repo,
        "f.txt",
        hunks[0].old_start,
        hunks[0].new_start,
        3,
        &[add_c],
    )
    .unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\n");
}

fn changed(hunk: &git_core::diff::DiffHunk) -> Vec<u32> {
    hunk.lines
        .iter()
        .enumerate()
        .filter(|(_, l)| l.origin != git_core::diff::DiffLineOrigin::Context)
        .map(|(i, _)| i as u32)
        .collect()
}

// --- Whole-hunk operations at zero context (fix round 1) ---

fn setup_pure_deletion() -> (tempfile::TempDir, git2::Repository) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a\nb\nc\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nc\n");
    (dir, repo)
}

const ZERO: DiffOptions = DiffOptions {
    ignore_whitespace: false,
    context_lines: 0,
};

#[test]
fn stage_hunk_stages_a_zero_context_pure_deletion() {
    let (dir, repo) = setup_pure_deletion();
    let h = &working_diff(&repo, "f.txt", false, &ZERO).unwrap()[0];
    git_core::stage::stage_hunk(&repo, "f.txt", h.old_start, h.new_start, 0).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nc\n");
    assert_eq!(workdir_content(dir.path(), "f.txt"), "a\nc\n");
}

#[test]
fn unstage_hunk_unstages_a_zero_context_pure_deletion() {
    let (_dir, repo) = setup_pure_deletion();
    stage_file(&repo, "f.txt").unwrap();
    let h = &working_diff(&repo, "f.txt", true, &ZERO).unwrap()[0];
    git_core::stage::unstage_hunk(&repo, "f.txt", h.old_start, h.new_start, 0).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc\n");
}

#[test]
fn discard_hunk_discards_a_zero_context_pure_deletion() {
    let (dir, repo) = setup_pure_deletion();
    let h = &working_diff(&repo, "f.txt", false, &ZERO).unwrap()[0];
    git_core::stage::discard_hunk(&repo, "f.txt", h.old_start, h.new_start, 0).unwrap();
    assert_eq!(
        workdir_content(dir.path(), "f.txt").replace("\r\n", "\n"),
        "a\nb\nc\n"
    );
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc\n");
}

// --- Whole-hunk behaviour on added/deleted files (characterization; must not change) ---

#[test]
fn stage_hunk_on_a_deleted_file_stages_the_deletion() {
    let (dir, repo) = setup_abc();
    std::fs::remove_file(dir.path().join("f.txt")).unwrap();
    let h = &working_diff(&repo, "f.txt", false, &DiffOptions::default()).unwrap()[0];
    git_core::stage::stage_hunk(&repo, "f.txt", h.old_start, h.new_start, 3).unwrap();
    assert_eq!(index_content(&repo, "f.txt"), None);
}

#[test]
fn stage_hunk_on_an_untracked_file_adds_it() {
    let (dir, repo) = setup_abc();
    common::write_file(dir.path(), "new.txt", "x\ny\n");
    let h = &working_diff(&repo, "new.txt", false, &DiffOptions::default()).unwrap()[0];
    git_core::stage::stage_hunk(&repo, "new.txt", h.old_start, h.new_start, 3).unwrap();
    assert_eq!(index_content(&repo, "new.txt").unwrap(), "x\ny\n");
}

#[test]
fn unstage_hunk_on_a_newly_added_file_removes_it_from_the_index() {
    let (dir, repo) = setup_abc();
    common::write_file(dir.path(), "new.txt", "x\ny\n");
    stage_file(&repo, "new.txt").unwrap();
    let h = &working_diff(&repo, "new.txt", true, &DiffOptions::default()).unwrap()[0];
    git_core::stage::unstage_hunk(&repo, "new.txt", h.old_start, h.new_start, 3).unwrap();
    assert_eq!(index_content(&repo, "new.txt"), None);
    assert_eq!(workdir_content(dir.path(), "new.txt"), "x\ny\n");
}

#[test]
fn unstage_hunk_on_a_staged_deletion_restores_the_index_entry() {
    let (dir, repo) = setup_abc();
    std::fs::remove_file(dir.path().join("f.txt")).unwrap();
    stage_file(&repo, "f.txt").unwrap();
    let h = &working_diff(&repo, "f.txt", true, &DiffOptions::default()).unwrap()[0];
    git_core::stage::unstage_hunk(&repo, "f.txt", h.old_start, h.new_start, 3).unwrap();
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc\n");
}

#[test]
fn discard_hunk_on_a_deleted_file_restores_it() {
    let (dir, repo) = setup_abc();
    std::fs::remove_file(dir.path().join("f.txt")).unwrap();
    let h = &working_diff(&repo, "f.txt", false, &DiffOptions::default()).unwrap()[0];
    git_core::stage::discard_hunk(&repo, "f.txt", h.old_start, h.new_start, 3).unwrap();
    assert_eq!(
        workdir_content(dir.path(), "f.txt").replace("\r\n", "\n"),
        "a\nb\nc\n"
    );
}

#[test]
fn discard_hunk_on_an_untracked_file_empties_it_but_never_deletes_it() {
    let (dir, repo) = setup_abc();
    common::write_file(dir.path(), "new.txt", "x\ny\n");
    let h = &working_diff(&repo, "new.txt", false, &DiffOptions::default()).unwrap()[0];
    git_core::stage::discard_hunk(&repo, "new.txt", h.old_start, h.new_start, 3).unwrap();
    assert_eq!(workdir_content(dir.path(), "new.txt"), "");
}

// --- Final-review fixes: symlinked parent on workdir discard, error mapping ---

#[cfg(unix)]
fn setup_symlinked_parent() -> (tempfile::TempDir, git2::Repository, tempfile::TempDir) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "d/f.txt", "a\nb\nc\n");
    common::commit_all(&repo, "base");
    let outside = tempfile::TempDir::new().unwrap();
    std::fs::write(outside.path().join("f.txt"), "a\nB\nc\n").unwrap();
    std::fs::remove_dir_all(dir.path().join("d")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("d")).unwrap();
    (dir, repo, outside)
}

#[cfg(unix)]
#[test]
fn discard_hunk_does_not_write_through_a_symlinked_parent() {
    let (_dir, repo, outside) = setup_symlinked_parent();
    let hunks = working_diff(&repo, "d/f.txt", false, &DiffOptions::default()).unwrap();
    let h = &hunks[0];
    let result = git_core::stage::discard_hunk(&repo, "d/f.txt", h.old_start, h.new_start, 3);
    assert!(matches!(result, Err(StageError::InvalidPath)), "{result:?}");
    assert_eq!(workdir_content(outside.path(), "f.txt"), "a\nB\nc\n");
}

#[cfg(unix)]
#[test]
fn discard_lines_does_not_write_through_a_symlinked_parent() {
    let (_dir, repo, outside) = setup_symlinked_parent();
    let hunks = working_diff(&repo, "d/f.txt", false, &DiffOptions::default()).unwrap();
    let h = &hunks[0];
    let first_changed = changed(h)[0];
    let result = discard_lines(
        &repo,
        "d/f.txt",
        h.old_start,
        h.new_start,
        3,
        &[first_changed],
    );
    assert!(matches!(result, Err(StageError::InvalidPath)), "{result:?}");
    assert_eq!(workdir_content(outside.path(), "f.txt"), "a\nB\nc\n");
}

#[test]
fn an_index_lock_failure_is_not_reported_as_an_inapplicable_selection() {
    let (dir, repo) = setup_abc();
    let (o, n) = only_hunk(&repo, false);
    std::fs::write(dir.path().join(".git/index.lock"), "").unwrap();
    let result = git_core::stage::stage_hunk(&repo, "f.txt", o, n, 3);
    match result {
        Err(StageError::Index(e)) => assert_eq!(e.code(), git2::ErrorCode::Locked, "{e}"),
        other => panic!("{other:?}"),
    }
}

// --- Final-review fix: line ops refuse a hunk whose content changed since it was shown ---

/// base "a\nb\nc\n" → workdir "a\nX\nb\nY\nc\n"; displayed hunk " a +X  b +Y  c".
fn setup_stale_view() -> (tempfile::TempDir, git2::Repository, (u32, u32), Vec<String>) {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "f.txt", "a\nb\nc\n");
    common::commit_all(&repo, "base");
    common::write_file(dir.path(), "f.txt", "a\nX\nb\nY\nc\n");
    let h = working_diff(&repo, "f.txt", false, &DiffOptions::default()).unwrap()[0].clone();
    let displayed: Vec<String> = h.lines.iter().map(|l| l.content.clone()).collect();
    assert_eq!(displayed, ["a\n", "X\n", "b\n", "Y\n", "c\n"]);
    (dir, repo, (h.old_start, h.new_start), displayed)
}

#[test]
fn discard_lines_refuses_a_hunk_edited_since_it_was_shown() {
    let (dir, repo, (o, n), displayed) = setup_stale_view();
    // The user adds a line above Y in an editor; the hunk keeps its (old_start, new_start).
    common::write_file(dir.path(), "f.txt", "a\nX\nb\nMINE\nY\nc\n");

    let result = git_core::stage::discard_lines(&repo, "f.txt", o, n, 3, &[3], &displayed);

    assert!(
        matches!(result, Err(StageError::HunkNotFound)),
        "{result:?}"
    );
    assert_eq!(
        workdir_content(dir.path(), "f.txt"),
        "a\nX\nb\nMINE\nY\nc\n"
    );
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc\n");
}

#[test]
fn stage_and_unstage_lines_refuse_a_stale_hunk() {
    let (dir, repo, (o, n), displayed) = setup_stale_view();
    common::write_file(dir.path(), "f.txt", "a\nX\nb\nMINE\nY\nc\n");
    let result = git_core::stage::stage_lines(&repo, "f.txt", o, n, 3, &[3], &displayed);
    assert!(
        matches!(result, Err(StageError::HunkNotFound)),
        "{result:?}"
    );
    assert_eq!(index_content(&repo, "f.txt").unwrap(), "a\nb\nc\n");

    stage_file(&repo, "f.txt").unwrap();
    let result = git_core::stage::unstage_lines(&repo, "f.txt", o, n, 3, &[3], &displayed);
    assert!(
        matches!(result, Err(StageError::HunkNotFound)),
        "{result:?}"
    );
    assert_eq!(
        index_content(&repo, "f.txt").unwrap(),
        "a\nX\nb\nMINE\nY\nc\n"
    );
}

#[test]
fn discard_lines_with_matching_content_still_works() {
    let (dir, repo, (o, n), displayed) = setup_stale_view();
    git_core::stage::discard_lines(&repo, "f.txt", o, n, 3, &[3], &displayed).unwrap();
    assert_eq!(workdir_content(dir.path(), "f.txt"), "a\nX\nb\nc\n");
}

#[test]
fn discard_hunk_of_d_never_uses_a_hunk_of_d_slash_inner() {
    let (dir, repo) = common::init_repo();
    common::write_file(dir.path(), "d", "file d\n");
    common::commit_all(&repo, "a");
    std::fs::remove_file(dir.path().join("d")).unwrap();
    common::write_file(dir.path(), "d/inner.txt", "inner\n");
    // The hunk of `d/inner.txt` as `d`'s diff showed it before #116 was fixed (an add at 0,1).
    let inner = &working_diff(&repo, "d/inner.txt", false, &DiffOptions::default()).unwrap()[0];
    assert_eq!((inner.old_start, inner.new_start), (0, 1));

    let result = git_core::stage::discard_hunk(&repo, "d", inner.old_start, inner.new_start, 3);

    assert!(
        matches!(result, Err(StageError::HunkNotFound)),
        "{result:?}"
    );
    assert_eq!(workdir_content(dir.path(), "d/inner.txt"), "inner\n");
}
