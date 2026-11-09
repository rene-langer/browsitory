//! Line-range history (#447). Each scenario checks explicit summaries and, when the git CLI is
//! available, that `git log -L` lists the same commits.

mod common;

use std::time::Duration;

use common::{commit_tree, init_repo};
use git2::{Oid, Repository};
use git_core::graph::GraphSelection;
use git_core::history::HistoryError;
use git_core::line_history::{line_history, LineHistory, LineRange};
use git_core::search::{parse_query, search_commits, PageLimits, SearchError};

const REGULAR: u32 = 0o100644;

fn set_head(repo: &Repository, tip: Oid) {
    repo.reference("refs/heads/main", tip, true, "test")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
}

fn lines(start: usize, end: usize) -> LineRange {
    LineRange { start, end }
}

fn summaries(history: &LineHistory) -> Vec<&str> {
    history
        .entries
        .iter()
        .map(|e| e.commit.summary.as_str())
        .collect()
}

/// `count` numbered lines, `prefix1` to `prefix<count>`, each ending in a newline.
fn numbered(prefix: &str, count: usize) -> String {
    (1..=count).map(|n| format!("{prefix}{n}\n")).collect()
}

/// Replaces line `n` (1-based) of `text` with `with`.
fn replace_line(text: &str, n: usize, with: &str) -> String {
    text.lines()
        .enumerate()
        .map(|(i, l)| format!("{}\n", if i + 1 == n { with } else { l }))
        .collect()
}

/// When the git CLI is available, asserts that `git log -L` for the same range lists exactly
/// `history`'s commits, in the same order.
fn assert_matches_git(repo: &Repository, history: &LineHistory, path: &str, range: LineRange) {
    assert_matches_git_in(repo, history, path, range, true);
}

/// Like `assert_matches_git`; `ordered: false` ignores the order (commits made in the same second
/// on two branches have no order of their own).
fn assert_matches_git_in(
    repo: &Repository,
    history: &LineHistory,
    path: &str,
    range: LineRange,
    ordered: bool,
) {
    if !common::program_available("git") {
        eprintln!("git CLI not found; skipping the git log -L comparison");
        return;
    }
    let output = test_support::git_command(repo.workdir().unwrap())
        .args([
            "log",
            "--no-patch",
            "--format=%H",
            &format!("-L{},{}:{path}", range.start, range.end),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut git: Vec<String> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    let mut ours: Vec<String> = history
        .entries
        .iter()
        .map(|e| e.commit.id.clone())
        .collect();
    if !ordered {
        ours.sort();
        git.sort();
    }
    assert_eq!(ours, git, "line history differs from git log -L");
}

/// a.txt with ten lines, then: c2 edits line 2, c3 edits line 8, c4 inserts three lines at the
/// top, c5 edits line 12 (old line 9).
fn linear() -> (tempfile::TempDir, Repository) {
    let (dir, repo) = init_repo();
    let v1 = numbered("l", 10);
    let v2 = replace_line(&v1, 2, "two");
    let v3 = replace_line(&v2, 8, "eight");
    let v4 = format!("{}{v3}", numbered("top", 3));
    let v5 = replace_line(&v4, 12, "nine");
    let c1 = commit_tree(&repo, &[("a.txt", &v1, REGULAR)], &[], "c1");
    let c2 = commit_tree(&repo, &[("a.txt", &v2, REGULAR)], &[c1], "c2");
    let c3 = commit_tree(&repo, &[("a.txt", &v3, REGULAR)], &[c2], "c3");
    let c4 = commit_tree(&repo, &[("a.txt", &v4, REGULAR)], &[c3], "c4");
    let c5 = commit_tree(&repo, &[("a.txt", &v5, REGULAR)], &[c4], "c5");
    set_head(&repo, c5);
    (dir, repo)
}

#[test]
fn lists_only_the_commits_that_changed_the_range_through_moved_lines() {
    let (_dir, repo) = linear();

    // Lines 10..12 at HEAD are l7, eight, nine: old lines 7..9.
    let history = line_history(&repo, "HEAD", "a.txt", lines(10, 12), 100).unwrap();

    assert_eq!(summaries(&history), vec!["c5", "c3", "c1"]);
    assert!(!history.truncated);
    assert_eq!(history.entries[0].lines, lines(10, 12));
    // c3 and c1 knew the range as lines 7..9, before c4 inserted three lines above it.
    assert_eq!(history.entries[1].lines, lines(7, 9));
    assert_eq!(history.entries[2].lines, lines(7, 9));
    assert_eq!(
        history.entries[0].commit.branch_refs,
        vec!["main".to_string()]
    );
    assert_matches_git(&repo, &history, "a.txt", lines(10, 12));
}

#[test]
fn lines_added_by_a_commit_end_their_history_there() {
    let (_dir, repo) = linear();

    let history = line_history(&repo, "HEAD", "a.txt", lines(1, 3), 100).unwrap();

    assert_eq!(summaries(&history), vec!["c4"]);
    assert_matches_git(&repo, &history, "a.txt", lines(1, 3));
}

#[test]
fn a_range_that_includes_an_insertion_follows_its_old_lines() {
    let (_dir, repo) = linear();

    // top3 (added by c4) and l1, two.
    let history = line_history(&repo, "HEAD", "a.txt", lines(3, 5), 100).unwrap();

    assert_eq!(summaries(&history), vec!["c4", "c2", "c1"]);
    assert_eq!(history.entries[1].lines, lines(1, 2));
    assert_matches_git(&repo, &history, "a.txt", lines(3, 5));
}

#[test]
fn starts_at_an_older_commit_with_that_commits_line_numbers() {
    let (_dir, repo) = linear();
    let c3 = repo
        .head()
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .parent(0)
        .unwrap()
        .parent(0)
        .unwrap();
    assert_eq!(c3.summary().unwrap(), Some("c3"));

    let history = line_history(&repo, &c3.id().to_string(), "a.txt", lines(2, 2), 100).unwrap();

    assert_eq!(summaries(&history), vec!["c2", "c1"]);
}

#[test]
fn stops_at_the_limit() {
    let (_dir, repo) = linear();

    let history = line_history(&repo, "HEAD", "a.txt", lines(10, 12), 2).unwrap();

    assert_eq!(summaries(&history), vec!["c5", "c3"]);
    assert!(line_history(&repo, "HEAD", "a.txt", lines(10, 12), 0)
        .unwrap()
        .entries
        .is_empty());
}

#[test]
fn follows_the_range_across_a_rename() {
    let (_dir, repo) = init_repo();
    let v1 = numbered("l", 10);
    let v2 = replace_line(&v1, 5, "five");
    let v3 = replace_line(&v2, 1, "one");
    let c1 = commit_tree(&repo, &[("old.txt", &v1, REGULAR)], &[], "add");
    let c2 = commit_tree(&repo, &[("old.txt", &v2, REGULAR)], &[c1], "edit five");
    let c3 = commit_tree(&repo, &[("new.txt", &v2, REGULAR)], &[c2], "rename");
    let c4 = commit_tree(&repo, &[("new.txt", &v3, REGULAR)], &[c3], "edit one");
    set_head(&repo, c4);

    let history = line_history(&repo, "HEAD", "new.txt", lines(4, 6), 100).unwrap();

    assert_eq!(summaries(&history), vec!["edit five", "add"]);
    assert_eq!(history.entries[0].path, "old.txt");
    assert_matches_git(&repo, &history, "new.txt", lines(4, 6));
}

#[test]
fn a_merge_that_took_one_sides_lines_follows_that_side_only() {
    let (_dir, repo) = init_repo();
    let base = numbered("l", 10);
    let ours = replace_line(&base, 2, "ours two");
    let theirs = replace_line(&base, 8, "theirs eight");
    let merged = replace_line(&ours, 8, "theirs eight");
    let c1 = commit_tree(&repo, &[("a.txt", &base, REGULAR)], &[], "base");
    let main = commit_tree(&repo, &[("a.txt", &ours, REGULAR)], &[c1], "main edit");
    let side = commit_tree(&repo, &[("a.txt", &theirs, REGULAR)], &[c1], "side edit");
    let merge = commit_tree(
        &repo,
        &[("a.txt", &merged, REGULAR)],
        &[main, side],
        "merge",
    );
    set_head(&repo, merge);

    let eight = line_history(&repo, "HEAD", "a.txt", lines(7, 9), 100).unwrap();
    assert_eq!(summaries(&eight), vec!["side edit", "base"]);
    assert_matches_git(&repo, &eight, "a.txt", lines(7, 9));

    let two = line_history(&repo, "HEAD", "a.txt", lines(1, 3), 100).unwrap();
    assert_eq!(summaries(&two), vec!["main edit", "base"]);
    assert_matches_git(&repo, &two, "a.txt", lines(1, 3));
}

#[test]
fn a_merge_that_changed_the_lines_against_both_sides_is_listed() {
    let (_dir, repo) = init_repo();
    let base = numbered("l", 10);
    let ours = replace_line(&base, 5, "ours");
    let theirs = replace_line(&base, 5, "theirs");
    let resolved = replace_line(&base, 5, "resolved");
    let c1 = commit_tree(&repo, &[("a.txt", &base, REGULAR)], &[], "base");
    let main = commit_tree(&repo, &[("a.txt", &ours, REGULAR)], &[c1], "main edit");
    let side = commit_tree(&repo, &[("a.txt", &theirs, REGULAR)], &[c1], "side edit");
    let merge = commit_tree(
        &repo,
        &[("a.txt", &resolved, REGULAR)],
        &[main, side],
        "merge",
    );
    set_head(&repo, merge);

    let history = line_history(&repo, "HEAD", "a.txt", lines(5, 5), 100).unwrap();

    let mut listed = summaries(&history);
    listed[1..3].sort_unstable();
    assert_eq!(listed, vec!["merge", "main edit", "side edit", "base"]);
    assert_eq!(history.entries.last().unwrap().commit.id, c1.to_string());
    assert_matches_git_in(&repo, &history, "a.txt", lines(5, 5), false);
}

#[test]
fn rejects_ranges_outside_the_file_and_files_without_lines() {
    let (_dir, repo) = init_repo();
    let c1 = commit_tree(
        &repo,
        &[
            ("a.txt", "one\ntwo\nthree", REGULAR),
            ("dir/b.txt", "b\n", REGULAR),
            ("bin", "\0\u{1}\u{2}binary", REGULAR),
        ],
        &[],
        "c1",
    );
    set_head(&repo, c1);

    // A last line without a newline still counts.
    assert_eq!(
        summaries(&line_history(&repo, "HEAD", "a.txt", lines(3, 3), 10).unwrap()),
        vec!["c1"]
    );
    assert!(matches!(
        line_history(&repo, "HEAD", "a.txt", lines(2, 4), 10),
        Err(HistoryError::InvalidRange {
            start: 2,
            end: 4,
            count: 3
        })
    ));
    assert!(matches!(
        line_history(&repo, "HEAD", "a.txt", lines(0, 1), 10),
        Err(HistoryError::InvalidRange { .. })
    ));
    assert!(matches!(
        line_history(&repo, "HEAD", "a.txt", lines(3, 2), 10),
        Err(HistoryError::InvalidRange { .. })
    ));
    assert!(matches!(
        line_history(&repo, "HEAD", "dir", lines(1, 1), 10),
        Err(HistoryError::NotAFile { .. })
    ));
    assert!(matches!(
        line_history(&repo, "HEAD", "missing.txt", lines(1, 1), 10),
        Err(HistoryError::NotAFile { .. })
    ));
    assert!(matches!(
        line_history(&repo, "HEAD", "bin", lines(1, 1), 10),
        Err(HistoryError::NotText { .. })
    ));
    assert!(matches!(
        line_history(&repo, "HEAD", "../a.txt", lines(1, 1), 10),
        Err(HistoryError::InvalidPath)
    ));
    assert_eq!(
        HistoryError::InvalidRange {
            start: 2,
            end: 4,
            count: 3
        }
        .to_string(),
        "Lines 2-4 aren't in the file, which has 3 lines."
    );
}

#[test]
fn an_unborn_head_has_no_line_history() {
    let (_dir, repo) = init_repo();

    let history = line_history(&repo, "HEAD", "a.txt", lines(1, 1), 10).unwrap();

    assert!(history.entries.is_empty());
}

const NO_TIME_BUDGET: PageLimits = PageLimits {
    time: Duration::MAX,
    ..PageLimits::DEFAULT
};

fn search(repo: &Repository, query: &str) -> Result<Vec<String>, SearchError> {
    let query = parse_query(query)?;
    let page = search_commits(
        repo,
        &query,
        &GraphSelection::default(),
        None,
        NO_TIME_BUDGET,
        &|| false,
    )?;
    Ok(page.commits.into_iter().map(|c| c.summary).collect())
}

#[test]
fn the_line_search_key_matches_the_line_history_and_combines_with_other_terms() {
    let (_dir, repo) = linear();

    assert_eq!(
        search(&repo, "line:a.txt:10-12").unwrap(),
        vec!["c5", "c3", "c1"]
    );
    assert_eq!(search(&repo, "line:a.txt:10-12 c3").unwrap(), vec!["c3"]);
    assert_eq!(search(&repo, "line:a.txt:1").unwrap(), vec!["c4"]);
}

#[test]
fn the_line_search_key_keeps_its_set_under_not_and_or() {
    let (_dir, repo) = linear();

    assert_eq!(
        search(&repo, "-line:a.txt:10-12").unwrap(),
        vec!["c4", "c2"]
    );
    assert_eq!(
        search(&repo, "line:a.txt:1 or line:a.txt:10-12").unwrap(),
        vec!["c5", "c4", "c3", "c1"]
    );
    assert_eq!(
        search(&repo, "not (line:a.txt:10-12 or c4)").unwrap(),
        vec!["c2"]
    );
}

#[test]
fn the_line_search_key_reads_lines_at_the_branch_terms_tip() {
    let (_dir, repo) = linear();
    let c3 = repo.revparse_single("HEAD~2").unwrap().id();
    repo.reference("refs/heads/old", c3, true, "test").unwrap();

    // Line 2 at `old` (c3) is "two"; at HEAD it is "top2", added by c4.
    assert_eq!(
        search(&repo, "line:a.txt:2 branch:old").unwrap(),
        vec!["c2", "c1"]
    );
}

#[test]
fn the_line_search_key_reports_a_bad_range_as_an_invalid_query() {
    let (_dir, repo) = linear();

    match search(&repo, "line:a.txt:13-40") {
        Err(SearchError::InvalidQuery { message }) => assert_eq!(
            message,
            "line: Lines 13-40 aren't in the file, which has 13 lines."
        ),
        other => panic!("expected InvalidQuery, got {other:?}"),
    }
    assert!(matches!(
        search(&repo, "line:nope.txt:1"),
        Err(SearchError::InvalidQuery { .. })
    ));
}

#[test]
fn the_line_search_key_stops_when_cancelled() {
    let (_dir, repo) = linear();
    let query = parse_query("line:a.txt:10-12").unwrap();
    let calls = std::cell::Cell::new(0);

    // Let the search's own first poll through, then cancel inside the line walk.
    let result = search_commits(
        &repo,
        &query,
        &GraphSelection::default(),
        None,
        NO_TIME_BUDGET,
        &|| {
            calls.set(calls.get() + 1);
            calls.get() > 1
        },
    );

    assert!(matches!(result, Err(SearchError::Cancelled)));
}
