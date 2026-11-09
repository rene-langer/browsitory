//! #469: the diff algorithm. Hunk and line actions find their hunk by re-diffing, so they must
//! use the displayed diff's algorithm: these files diff into different hunks under Myers
//! (`Default`) and `Patience`.
mod common;

use common::{commit_all, init_repo, read, write_file};
use git2::Repository;
use git_core::diff::{
    commit_diff, working_diff, DiffAlgorithm, DiffHunk, DiffOptions, HunkOptions,
};
use git_core::patch::{apply_commit_change, ApplyTarget, CommitHunk};
use git_core::stage::{stage_file, StageError};

const MIDDLE: [&str; 8] = ["m0", "m1", "m2", "m3", "m4", "m5", "m6", "m7"];
const OLD_TOP: [&str; 6] = ["}", "u39", "{", "y", "u37", "y"];
const NEW_TOP: [&str; 6] = ["}", "u39", "y", "{", "y", "u37"];
const OLD_BOTTOM: [&str; 8] = ["}", "u70", "}", "{", "{", "{", "y", "x"];
const NEW_BOTTOM: [&str; 8] = ["x", "}", "u70", "}", "{", "{", "{", "y"];

fn text(parts: &[&[&str]]) -> String {
    parts
        .iter()
        .flat_map(|p| p.iter())
        .map(|l| format!("{l}\n"))
        .collect()
}

fn old_text() -> String {
    text(&[&OLD_TOP, &MIDDLE, &OLD_BOTTOM])
}

fn new_text() -> String {
    text(&[&NEW_TOP, &MIDDLE, &NEW_BOTTOM])
}

const PATIENCE: HunkOptions = HunkOptions {
    context_lines: 3,
    algorithm: DiffAlgorithm::Patience,
};

const MYERS: HunkOptions = HunkOptions {
    context_lines: 3,
    algorithm: DiffAlgorithm::Default,
};

/// `f.txt` committed as `old_text()`, changed to `new_text()` in the working tree.
fn modified_repo() -> (tempfile::TempDir, Repository) {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", &old_text());
    commit_all(&repo, "base");
    write_file(dir.path(), "f.txt", &new_text());
    (dir, repo)
}

fn hunks(repo: &Repository, staged: bool, opts: HunkOptions) -> Vec<DiffHunk> {
    working_diff(repo, "f.txt", staged, &opts.diff_options()).unwrap()
}

fn starts(hunks: &[DiffHunk]) -> Vec<(u32, u32)> {
    hunks.iter().map(|h| (h.old_start, h.new_start)).collect()
}

fn shown(hunk: &DiffHunk) -> Vec<String> {
    hunk.lines.iter().map(|l| l.content.clone()).collect()
}

fn index_text(repo: &Repository) -> String {
    let index = repo.index().unwrap();
    let entry = index.get_path(std::path::Path::new("f.txt"), 0).unwrap();
    String::from_utf8(repo.find_blob(entry.id).unwrap().content().to_vec()).unwrap()
}

#[test]
fn the_algorithms_split_the_example_into_different_hunks() {
    let (_dir, repo) = modified_repo();
    let myers = hunks(&repo, false, MYERS);
    let patience = hunks(&repo, false, PATIENCE);
    let minimal = hunks(
        &repo,
        false,
        HunkOptions {
            algorithm: DiffAlgorithm::Minimal,
            ..MYERS
        },
    );
    assert_eq!(starts(&myers), vec![(1, 1), (12, 12), (19, 20)]);
    assert_eq!(starts(&patience), vec![(1, 1), (12, 12)]);
    // Myers' second hunk only inserts `x`; patience's removes and re-adds the block around it.
    assert_eq!(myers[1].new_lines, 7);
    assert_eq!(patience[1].new_lines, 11);
    assert_eq!(starts(&minimal), starts(&myers));
}

#[test]
fn stage_hunk_stages_the_patience_hunk() {
    let (_dir, repo) = modified_repo();
    git_core::stage::stage_hunk(&repo, "f.txt", 12, 12, PATIENCE).unwrap();
    // The whole bottom block, as patience showed it; the top is untouched.
    assert_eq!(index_text(&repo), text(&[&OLD_TOP, &MIDDLE, &NEW_BOTTOM]));
}

#[test]
fn the_same_hunk_identity_under_myers_is_a_different_hunk() {
    // Why the algorithm travels with every hunk action: `(12, 12)` exists under both, but means
    // other lines.
    let (_dir, repo) = modified_repo();
    git_core::stage::stage_hunk(&repo, "f.txt", 12, 12, MYERS).unwrap();
    assert_ne!(index_text(&repo), text(&[&OLD_TOP, &MIDDLE, &NEW_BOTTOM]));
}

#[test]
fn stage_lines_stages_the_selected_patience_line() {
    let (_dir, repo) = modified_repo();
    let hunk = &hunks(&repo, false, PATIENCE)[1];
    // Three context lines, then `-}`: remove the bottom block's first line only.
    assert_eq!(hunk.lines[3].content, "}\n");
    git_core::stage::stage_lines(&repo, "f.txt", 12, 12, PATIENCE, &[3], &shown(hunk)).unwrap();
    let mut expected: Vec<&str> = OLD_BOTTOM.to_vec();
    expected.remove(0);
    assert_eq!(index_text(&repo), text(&[&OLD_TOP, &MIDDLE, &expected]));
}

#[test]
fn a_line_action_under_another_algorithm_finds_no_matching_hunk() {
    let (_dir, repo) = modified_repo();
    let hunk = &hunks(&repo, false, PATIENCE)[1];
    let result = git_core::stage::stage_lines(&repo, "f.txt", 12, 12, MYERS, &[3], &shown(hunk));
    assert!(
        matches!(result, Err(StageError::HunkNotFound)),
        "{result:?}"
    );
    assert_eq!(index_text(&repo), old_text());
}

#[test]
fn unstage_lines_unstages_the_selected_patience_line() {
    let (_dir, repo) = modified_repo();
    stage_file(&repo, "f.txt").unwrap();
    let hunk = &hunks(&repo, true, PATIENCE)[1];
    assert_eq!(hunk.lines[3].content, "}\n");
    git_core::stage::unstage_lines(&repo, "f.txt", 12, 12, PATIENCE, &[3], &shown(hunk)).unwrap();
    // The removed `}` is back in the index, before the `x` patience kept.
    let mut expected: Vec<&str> = vec!["}"];
    expected.extend(NEW_BOTTOM);
    assert_eq!(index_text(&repo), text(&[&NEW_TOP, &MIDDLE, &expected]));
}

#[test]
fn discard_lines_discards_the_selected_patience_line() {
    let (dir, repo) = modified_repo();
    let hunk = &hunks(&repo, false, PATIENCE)[1];
    // `x` (kept as context) at 10, then the re-added block: drop its first line, `+}`.
    assert_eq!(hunk.lines[10].content, "x\n");
    assert_eq!(hunk.lines[11].content, "}\n");
    git_core::stage::discard_lines(&repo, "f.txt", 12, 12, PATIENCE, &[11], &shown(hunk)).unwrap();
    let mut expected: Vec<&str> = NEW_BOTTOM.to_vec();
    expected.remove(1);
    assert_eq!(
        read(dir.path(), "f.txt"),
        text(&[&NEW_TOP, &MIDDLE, &expected])
    );
}

#[test]
fn reverting_a_commit_hunk_uses_its_algorithm() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "f.txt", &old_text());
    commit_all(&repo, "base");
    write_file(dir.path(), "f.txt", &new_text());
    commit_all(&repo, "change");
    let id = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id()
        .to_string();
    let shown = commit_diff(&repo, &id, "f.txt", &PATIENCE.diff_options()).unwrap();
    assert_eq!(starts(&shown), vec![(1, 1), (12, 12)]);
    let hunk = CommitHunk {
        old_start: 12,
        new_start: 12,
        options: PATIENCE,
    };
    apply_commit_change(
        &repo,
        &id,
        "f.txt",
        None,
        Some(hunk),
        true,
        ApplyTarget::WorkTree,
    )
    .unwrap();
    // Patience's bottom hunk reverted whole; the top keeps the commit's change.
    assert_eq!(
        read(dir.path(), "f.txt"),
        text(&[&NEW_TOP, &MIDDLE, &OLD_BOTTOM])
    );
}

#[test]
fn diff_options_default_to_myers() {
    assert_eq!(DiffOptions::default().algorithm, DiffAlgorithm::Default);
}
