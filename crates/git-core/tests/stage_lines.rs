mod common;

use git_core::diff::{working_diff, DiffOptions};
use git_core::stage::{discard_lines, stage_file, stage_lines, unstage_lines, StageError};

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
