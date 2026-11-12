//! File and line history through the worker's cache (#860): paging continues one walk instead
//! of restarting it, a refresh with HEAD unchanged walks nothing, and a time budget bounds every
//! call. Each page is checked against the uncached walk.

mod common;

use std::time::Duration;

use common::{commit_tree, init_repo};
use git2::{Oid, Repository};
use git_core::graph::WalkCache;
use git_core::history::{file_history, file_history_cached, FileHistoryEntry, FileHistoryPage};
use git_core::line_history::{line_history, line_history_cached, LineHistory, LineRange};

const REGULAR: u32 = 0o100644;
/// Long enough that no call in these tests runs out of it.
const NO_BUDGET: Duration = Duration::from_secs(3600);

fn set_head(repo: &Repository, tip: Oid) {
    repo.reference("refs/heads/main", tip, true, "test")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
}

fn ids(entries: &[FileHistoryEntry]) -> Vec<Oid> {
    entries
        .iter()
        .map(|e| Oid::from_str(&e.commit.id).unwrap())
        .collect()
}

/// 30 commits on main: `f.txt` changes in every third, `g.txt` in every one. A side branch from
/// the 10th commit sets `f.txt` to "side" and is merged into the 21st, which keeps main's
/// version. `f.txt` therefore has 11 changes: 10 on main and the side commit.
fn history_repo() -> (tempfile::TempDir, Repository, Oid) {
    let (dir, repo) = init_repo();
    let mut tip: Option<Oid> = None;
    let mut tenth = None;
    for i in 0..30 {
        let f = (i / 3).to_string();
        let g = i.to_string();
        let files = [
            ("f.txt", f.as_str(), REGULAR),
            ("g.txt", g.as_str(), REGULAR),
        ];
        let mut parents: Vec<Oid> = tip.into_iter().collect();
        if i == 20 {
            let base = tenth.expect("made at i = 10");
            let side = commit_tree(
                &repo,
                &[("f.txt", "side", REGULAR), ("g.txt", "10", REGULAR)],
                &[base],
                "side",
            );
            parents.push(side);
        }
        let commit = commit_tree(&repo, &files, &parents, &format!("c{i}"));
        if i == 10 {
            tenth = Some(commit);
        }
        tip = Some(commit);
    }
    let tip = tip.unwrap();
    set_head(&repo, tip);
    (dir, repo, tip)
}

/// Asks for `limit` entries, then `limit + page`, and so on, the way "Load more" does, until the
/// history is complete. Returns every page.
fn page_through(
    repo: &Repository,
    cache: &mut WalkCache,
    path: &str,
    page: usize,
) -> Vec<FileHistoryPage> {
    let mut pages: Vec<FileHistoryPage> = Vec::new();
    let mut limit = page;
    loop {
        let result = file_history_cached(repo, cache, path, limit, NO_BUDGET).unwrap();
        let complete = result.complete;
        pages.push(result);
        if complete {
            return pages;
        }
        assert!(pages.len() < 100, "the history never completed");
        limit += page;
    }
}

#[test]
fn paging_through_the_cache_equals_the_uncached_history_and_walks_once() {
    let (_dir, repo, _) = history_repo();
    let all = file_history(&repo, "f.txt", usize::MAX).unwrap();
    assert_eq!(all.len(), 11);
    let mut full = WalkCache::default();
    let whole = file_history_cached(&repo, &mut full, "f.txt", usize::MAX, NO_BUDGET).unwrap();
    assert_eq!(whole.entries, all);
    assert!(whole.complete);
    let examined_by_one_walk = full.stats().history_commits;
    assert!(examined_by_one_walk > 0);

    for page in [1, 2, 3, 7] {
        let mut cache = WalkCache::default();
        let pages = page_through(&repo, &mut cache, "f.txt", page);
        for (n, result) in pages.iter().enumerate() {
            let limit = page * (n + 1);
            assert_eq!(
                result.entries,
                file_history(&repo, "f.txt", limit).unwrap(),
                "page size {page}, limit {limit}"
            );
        }
        assert_eq!(pages.last().unwrap().entries, all, "page size {page}");
        let stats = cache.stats();
        assert_eq!(stats.history_walks, 1, "page size {page}");
        assert_eq!(
            stats.history_commits, examined_by_one_walk,
            "page size {page}: every commit is examined once"
        );
    }
}

#[test]
fn a_refresh_with_head_unchanged_walks_nothing() {
    let (_dir, repo, _) = history_repo();
    let mut cache = WalkCache::default();
    let first = file_history_cached(&repo, &mut cache, "f.txt", 4, NO_BUDGET).unwrap();
    let examined = cache.stats().history_commits;

    let again = file_history_cached(&repo, &mut cache, "f.txt", 4, NO_BUDGET).unwrap();
    let fewer = file_history_cached(&repo, &mut cache, "f.txt", 2, NO_BUDGET).unwrap();

    assert_eq!(again, first);
    assert_eq!(fewer.entries, first.entries[..2].to_vec());
    assert!(!fewer.complete);
    assert_eq!(cache.stats().history_commits, examined);
    assert_eq!(cache.stats().history_walks, 1);
}

#[test]
fn a_new_tag_shows_on_a_refresh_without_a_new_walk() {
    let (_dir, repo, _) = history_repo();
    let mut cache = WalkCache::default();
    let first = file_history_cached(&repo, &mut cache, "f.txt", 3, NO_BUDGET).unwrap();
    let examined = cache.stats().history_commits;
    let tagged = repo
        .find_object(Oid::from_str(&first.entries[1].commit.id).unwrap(), None)
        .unwrap();
    repo.tag_lightweight("v1", &tagged, false).unwrap();

    let again = file_history_cached(&repo, &mut cache, "f.txt", 3, NO_BUDGET).unwrap();

    assert_eq!(again.entries[1].commit.tags, vec!["v1".to_string()]);
    assert_eq!(again.entries, file_history(&repo, "f.txt", 3).unwrap());
    assert_eq!(cache.stats().history_commits, examined);
}

#[test]
fn a_moved_head_walks_again_from_the_new_head() {
    let (_dir, repo, tip) = history_repo();
    let mut cache = WalkCache::default();
    file_history_cached(&repo, &mut cache, "f.txt", 3, NO_BUDGET).unwrap();
    let newer = commit_tree(
        &repo,
        &[("f.txt", "new", REGULAR), ("g.txt", "29", REGULAR)],
        &[tip],
        "newer",
    );
    set_head(&repo, newer);

    let moved = file_history_cached(&repo, &mut cache, "f.txt", 3, NO_BUDGET).unwrap();

    assert_eq!(moved.entries, file_history(&repo, "f.txt", 3).unwrap());
    assert_eq!(moved.entries[0].commit.summary, "newer");
    assert_eq!(cache.stats().history_walks, 2);
}

#[test]
fn a_zero_budget_still_examines_one_commit_per_call() {
    let (_dir, repo, _) = history_repo();
    let all = file_history(&repo, "f.txt", usize::MAX).unwrap();
    let mut cache = WalkCache::default();

    let first = file_history_cached(&repo, &mut cache, "f.txt", 300, Duration::ZERO).unwrap();
    assert_eq!(cache.stats().history_commits, 1);
    assert!(!first.complete);
    assert!(first.entries.len() <= 1);

    let mut calls = 1;
    let mut last = first;
    while !last.complete {
        let before = cache.stats().history_commits;
        last = file_history_cached(&repo, &mut cache, "f.txt", 300, Duration::ZERO).unwrap();
        calls += 1;
        let examined = cache.stats().history_commits - before;
        assert!(examined <= 1, "call {calls} examined {examined} commits");
        assert!(calls < 1_000, "the history never completed");
    }
    assert_eq!(last.entries, all);
    assert!(calls > all.len());
    assert_eq!(cache.stats().history_walks, 1);
}

#[test]
fn complete_is_set_once_the_history_ends() {
    let (_dir, repo) = init_repo();
    let v0 = commit_tree(&repo, &[("other.txt", "x", REGULAR)], &[], "v0");
    let v1 = commit_tree(
        &repo,
        &[("other.txt", "x", REGULAR), ("f.txt", "1", REGULAR)],
        &[v0],
        "v1",
    );
    let v2 = commit_tree(
        &repo,
        &[("other.txt", "x", REGULAR), ("f.txt", "2", REGULAR)],
        &[v1],
        "v2",
    );
    set_head(&repo, v2);
    let mut cache = WalkCache::default();

    let partial = file_history_cached(&repo, &mut cache, "f.txt", 1, NO_BUDGET).unwrap();
    let exact = file_history_cached(&repo, &mut cache, "f.txt", 2, NO_BUDGET).unwrap();
    let beyond = file_history_cached(&repo, &mut cache, "f.txt", 300, NO_BUDGET).unwrap();

    assert_eq!(ids(&partial.entries), vec![v2]);
    assert!(!partial.complete);
    assert_eq!(ids(&exact.entries), vec![v2, v1]);
    assert!(exact.complete, "v1 added the file: nothing older to list");
    assert_eq!(beyond, exact);
}

#[test]
fn an_unborn_head_or_a_missing_file_is_complete_and_empty() {
    let (_dir, repo) = init_repo();
    let mut cache = WalkCache::default();
    let unborn = file_history_cached(&repo, &mut cache, "f.txt", 300, NO_BUDGET).unwrap();
    assert!(unborn.entries.is_empty());
    assert!(unborn.complete);

    let c1 = commit_tree(&repo, &[("f.txt", "1", REGULAR)], &[], "c1");
    set_head(&repo, c1);
    let missing = file_history_cached(&repo, &mut cache, "missing.txt", 300, NO_BUDGET).unwrap();
    assert!(missing.entries.is_empty());
    assert!(missing.complete);
}

#[test]
fn the_two_most_recent_files_are_kept() {
    let (_dir, repo, _) = history_repo();
    let mut cache = WalkCache::default();
    let f = file_history_cached(&repo, &mut cache, "f.txt", 300, NO_BUDGET).unwrap();
    let g = file_history_cached(&repo, &mut cache, "g.txt", 300, NO_BUDGET).unwrap();
    let examined = cache.stats().history_commits;

    assert_eq!(
        file_history_cached(&repo, &mut cache, "f.txt", 300, NO_BUDGET).unwrap(),
        f
    );
    assert_eq!(
        file_history_cached(&repo, &mut cache, "g.txt", 300, NO_BUDGET).unwrap(),
        g
    );
    assert_eq!(cache.stats().history_walks, 2);
    assert_eq!(cache.stats().history_commits, examined);
}

// ---- Line history ----

fn lines(start: usize, end: usize) -> LineRange {
    LineRange { start, end }
}

/// 10 numbered lines in `l.txt`. Commit `i` (of 30) rewrites line `i % 10 + 1` in every third
/// commit, and `g.txt` in every commit.
fn line_repo() -> (tempfile::TempDir, Repository, Oid) {
    let (dir, repo) = init_repo();
    let mut text: Vec<String> = (1..=10).map(|n| format!("line {n}")).collect();
    let mut tip: Option<Oid> = None;
    for i in 0..30 {
        if i % 3 == 0 {
            text[i % 10] = format!("line {} edited in c{i}", i % 10 + 1);
        }
        let content: String = text.iter().map(|l| format!("{l}\n")).collect();
        let g = i.to_string();
        let parents: Vec<Oid> = tip.into_iter().collect();
        tip = Some(commit_tree(
            &repo,
            &[("l.txt", &content, REGULAR), ("g.txt", &g, REGULAR)],
            &parents,
            &format!("c{i}"),
        ));
    }
    let tip = tip.unwrap();
    set_head(&repo, tip);
    (dir, repo, tip)
}

fn line_page(
    cache: &mut WalkCache,
    repo: &Repository,
    revision: &str,
    limit: usize,
) -> LineHistory {
    line_history_cached(
        repo,
        cache,
        revision,
        "l.txt",
        lines(1, 5),
        limit,
        NO_BUDGET,
    )
    .unwrap()
}

#[test]
fn line_history_pages_equal_the_uncached_history_and_walk_once() {
    let (_dir, repo, _) = line_repo();
    let all = line_history(&repo, "HEAD", "l.txt", lines(1, 5), usize::MAX).unwrap();
    assert!(all.entries.len() >= 5, "{}", all.entries.len());
    let mut full = WalkCache::default();
    let whole = line_page(&mut full, &repo, "HEAD", usize::MAX);
    assert_eq!(whole.entries, all.entries);
    assert!(whole.complete);
    assert!(!whole.truncated);
    let examined_by_one_walk = full.stats().history_commits;

    for page in [1, 2, 4] {
        let mut cache = WalkCache::default();
        let mut limit = page;
        loop {
            let result = line_page(&mut cache, &repo, "HEAD", limit);
            let expected = line_history(&repo, "HEAD", "l.txt", lines(1, 5), limit).unwrap();
            assert_eq!(
                result.entries, expected.entries,
                "page {page}, limit {limit}"
            );
            if result.complete {
                assert_eq!(result.entries, all.entries);
                break;
            }
            assert!(limit < 1_000, "the history never completed");
            limit += page;
        }
        assert_eq!(cache.stats().history_walks, 1, "page size {page}");
        assert_eq!(
            cache.stats().history_commits,
            examined_by_one_walk,
            "page size {page}"
        );
    }
}

#[test]
fn a_line_history_refresh_with_head_unchanged_walks_nothing() {
    let (_dir, repo, _) = line_repo();
    let mut cache = WalkCache::default();
    let first = line_page(&mut cache, &repo, "HEAD", 3);
    let examined = cache.stats().history_commits;

    let again = line_page(&mut cache, &repo, "HEAD", 3);

    assert_eq!(again, first);
    assert_eq!(cache.stats().history_commits, examined);
}

#[test]
fn a_line_history_at_a_fixed_commit_survives_a_moved_head() {
    let (_dir, repo, tip) = line_repo();
    let revision = tip.to_string();
    let mut cache = WalkCache::default();
    let first = line_page(&mut cache, &repo, &revision, 3);
    let newer = commit_tree(&repo, &[("l.txt", "other\n", REGULAR)], &[tip], "newer");
    set_head(&repo, newer);

    let again = line_page(&mut cache, &repo, &revision, 3);

    assert_eq!(again, first);
    assert_eq!(cache.stats().history_walks, 1);
}

#[test]
fn a_line_history_with_a_zero_budget_still_makes_progress() {
    let (_dir, repo, _) = line_repo();
    let all = line_history(&repo, "HEAD", "l.txt", lines(1, 5), usize::MAX).unwrap();
    let mut cache = WalkCache::default();
    let mut calls = 0;
    let last = loop {
        let before = cache.stats().history_commits;
        let result = line_history_cached(
            &repo,
            &mut cache,
            "HEAD",
            "l.txt",
            lines(1, 5),
            300,
            Duration::ZERO,
        )
        .unwrap();
        calls += 1;
        let examined = cache.stats().history_commits - before;
        assert!(examined <= 1, "call {calls} examined {examined} commits");
        assert!(calls < 1_000, "the history never completed");
        if result.complete {
            break result;
        }
    };
    assert_eq!(last.entries, all.entries);
    assert!(calls > all.entries.len());
}

#[test]
fn line_history_errors_still_come_from_the_first_call() {
    let (_dir, repo, _) = line_repo();
    let mut cache = WalkCache::default();
    let result = line_history_cached(
        &repo,
        &mut cache,
        "HEAD",
        "l.txt",
        lines(5, 11),
        300,
        NO_BUDGET,
    );
    assert!(matches!(
        result,
        Err(git_core::history::HistoryError::InvalidRange { count: 10, .. })
    ));
}
