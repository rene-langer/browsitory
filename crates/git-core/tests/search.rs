mod common;

use std::cell::Cell;
use std::path::Path;

use common::{commit_all, init_repo, write_file};
use git2::{Oid, Repository, Signature, Time};
use git_core::search::{parse_query, search_commits, PageLimits, SearchError, SearchPage};

/// Writes `file`, stages everything, and commits on HEAD with explicit author/committer/time.
#[allow(clippy::too_many_arguments)]
fn commit_as(
    repo: &Repository,
    dir: &Path,
    file: &str,
    contents: &str,
    message: &str,
    author: (&str, &str),
    committer: (&str, &str),
    when: Time,
) -> Oid {
    write_file(dir, file, contents);
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let author = Signature::new(author.0, author.1, &when).unwrap();
    let committer = Signature::new(committer.0, committer.1, &when).unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &author, &committer, message, &tree, &parents)
        .unwrap()
}

fn search_with(
    repo: &Repository,
    query: &str,
    refs: Option<&[String]>,
) -> Result<SearchPage, SearchError> {
    search_commits(
        repo,
        &parse_query(query)?,
        refs,
        None,
        PageLimits::DEFAULT,
        &|| false,
    )
}

fn summaries(repo: &Repository, query: &str) -> Vec<String> {
    search_with(repo, query, None)
        .unwrap()
        .commits
        .into_iter()
        .map(|c| c.summary)
        .collect()
}

const T0: i64 = 1_773_057_600; // 2026-03-09T12:00:00Z
fn at(seconds: i64, offset_minutes: i32) -> Time {
    Time::new(seconds, offset_minutes)
}

#[test]
fn text_matches_summary_body_and_sha_prefix_ignoring_case() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "Fix Login bug\n\nDetails about TOKEN refresh");
    write_file(dir.path(), "b.txt", "b");
    commit_all(&repo, "Unrelated");
    let id = repo.revparse_single("HEAD~1").unwrap().id().to_string();

    assert_eq!(summaries(&repo, "login"), ["Fix Login bug"]);
    assert_eq!(summaries(&repo, "token"), ["Fix Login bug"]);
    assert_eq!(summaries(&repo, "fix login"), ["Fix Login bug"]);
    assert_eq!(summaries(&repo, "\"fix login\""), ["Fix Login bug"]);
    assert!(summaries(&repo, "\"login fix\"").is_empty());
    assert_eq!(summaries(&repo, &id[..8]), ["Fix Login bug"]);
    assert_eq!(
        summaries(&repo, &format!("commit:{}", id[..6].to_uppercase())),
        ["Fix Login bug"]
    );
    assert!(summaries(&repo, &format!("message:{}", &id[..8])).is_empty());
    assert_eq!(summaries(&repo, "message:TOKEN"), ["Fix Login bug"]);
}

#[test]
fn author_and_committer_match_name_or_email() {
    let (dir, repo) = init_repo();
    commit_as(
        &repo,
        dir.path(),
        "a.txt",
        "a",
        "by ada",
        ("Ada Lovelace", "ada@example.com"),
        ("Bot", "bot@ci.test"),
        at(T0, 0),
    );
    commit_as(
        &repo,
        dir.path(),
        "b.txt",
        "b",
        "by test",
        ("Test User", "test@other.test"),
        ("Test User", "test@other.test"),
        at(T0 + 60, 0),
    );

    assert_eq!(summaries(&repo, "author:ADA"), ["by ada"]);
    assert_eq!(summaries(&repo, "author:example.com"), ["by ada"]);
    assert!(summaries(&repo, "author:bot").is_empty());
    assert_eq!(summaries(&repo, "committer:bot"), ["by ada"]);
    assert!(summaries(&repo, "committer:ada").is_empty());
    assert_eq!(summaries(&repo, "author:ada committer:ci.test"), ["by ada"]);
    assert!(summaries(&repo, "author:ada committer:test@other").is_empty());
}

#[test]
fn path_matches_directories_and_globs_against_the_first_parent() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "docs/guide.md", "g");
    commit_all(&repo, "docs");
    write_file(dir.path(), "src/lib.rs", "l");
    commit_all(&repo, "code");

    assert_eq!(summaries(&repo, "path:docs"), ["docs"]);
    assert_eq!(summaries(&repo, "path:*.md"), ["docs"]);
    assert_eq!(summaries(&repo, "path:src/"), ["code"]);
    assert!(summaries(&repo, "path:*.md path:src").is_empty());
}

#[test]
fn contents_finds_added_and_removed_lines_but_not_context() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "keep\nold line\n");
    commit_all(&repo, "base");
    write_file(dir.path(), "a.txt", "keep\nnew line\n");
    commit_all(&repo, "edit");
    write_file(dir.path(), "b.txt", "unrelated KEEP\n");
    commit_all(&repo, "other");

    assert_eq!(summaries(&repo, "contents:\"old line\""), ["edit", "base"]);
    assert_eq!(summaries(&repo, "contents:new"), ["edit"]);
    assert_eq!(summaries(&repo, "contents:keep"), ["other", "base"]);
}

#[test]
fn contents_is_limited_to_path_terms() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "docs/n.md", "needle\n");
    commit_all(&repo, "in docs");
    write_file(dir.path(), "src/n.rs", "needle\n");
    commit_all(&repo, "in src");

    assert_eq!(summaries(&repo, "contents:needle path:src"), ["in src"]);
    assert_eq!(summaries(&repo, "contents:needle"), ["in src", "in docs"]);
}

#[test]
fn dates_are_inclusive_days_in_the_commits_own_timezone() {
    let (dir, repo) = init_repo();
    let who = ("Test User", "test@example.com");
    commit_as(
        &repo,
        dir.path(),
        "a.txt",
        "a",
        "march 9",
        who,
        who,
        at(T0, 0),
    );
    // 2026-03-10T23:30Z at +01:00 is 2026-03-11 00:30 local.
    commit_as(
        &repo,
        dir.path(),
        "b.txt",
        "b",
        "march 11 local",
        who,
        who,
        at(1_773_185_400, 60),
    );
    commit_as(
        &repo,
        dir.path(),
        "c.txt",
        "c",
        "march 12",
        who,
        who,
        at(1_773_302_400, 0),
    );

    assert_eq!(
        summaries(&repo, "after:2026-03-11"),
        ["march 12", "march 11 local"]
    );
    assert_eq!(
        summaries(&repo, "before:2026-03-11"),
        ["march 11 local", "march 9"]
    );
    assert_eq!(
        summaries(&repo, "after:2026-03-11 before:2026-03-11"),
        ["march 11 local"]
    );
    assert_eq!(summaries(&repo, "before:2026-03-10"), ["march 9"]);
}

#[test]
fn branch_term_searches_only_what_that_ref_reaches() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "base work");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("topic", &base, false).unwrap();
    let sig = repo.signature().unwrap();
    let topic_tip = repo
        .commit(
            Some("refs/heads/topic"),
            &sig,
            &sig,
            "topic work",
            &base.tree().unwrap(),
            &[&base],
        )
        .unwrap();
    repo.reference("refs/remotes/origin/feature", topic_tip, true, "test")
        .unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_string();

    assert_eq!(summaries(&repo, "work"), ["topic work", "base work"]);
    assert_eq!(
        summaries(&repo, &format!("work branch:{main}")),
        ["base work"]
    );
    assert_eq!(
        summaries(&repo, "work branch:origin/feature"),
        ["topic work", "base work"]
    );
    match search_with(&repo, "branch:nope", None) {
        Err(SearchError::InvalidQuery { message }) => {
            assert_eq!(message, "No branch named \"nope\".")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn refs_selection_limits_the_walk_and_skips_missing_refs() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_string();
    repo.branch("feature", &base, false).unwrap();
    let sig = repo.signature().unwrap();
    repo.commit(
        Some("refs/heads/feature"),
        &sig,
        &sig,
        "feature work",
        &base.tree().unwrap(),
        &[&base],
    )
    .unwrap();

    let found = |refs: Option<Vec<String>>| -> Vec<String> {
        search_with(&repo, "work", refs.as_deref())
            .unwrap()
            .commits
            .into_iter()
            .map(|c| c.summary)
            .collect()
    };
    assert!(found(Some(vec![main.clone()])).is_empty());
    assert_eq!(found(Some(vec!["feature".into()])), ["feature work"]);
    assert_eq!(
        found(Some(vec!["feature".into(), "deleted-branch".into()])),
        ["feature work"]
    );
    assert_eq!(found(None), ["feature work"]);
}

#[test]
fn merges_are_judged_against_their_first_parent() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "base.txt", "b");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    let sig = repo.signature().unwrap();
    // side branch adds side.txt
    write_file(dir.path(), "side.txt", "s");
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("side.txt")).unwrap();
    let side_tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let side = repo
        .find_commit(
            repo.commit(None, &sig, &sig, "side", &side_tree, &[&base])
                .unwrap(),
        )
        .unwrap();
    index.remove_path(Path::new("side.txt")).unwrap();
    index.write().unwrap();
    std::fs::remove_file(dir.path().join("side.txt")).unwrap();
    // main adds main.txt
    write_file(dir.path(), "main.txt", "m");
    commit_all(&repo, "main");
    let main = repo.head().unwrap().peel_to_commit().unwrap();
    let mut merged = repo.merge_commits(&main, &side, None).unwrap();
    let tree = repo
        .find_tree(merged.write_tree_to(&repo).unwrap())
        .unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "merge", &tree, &[&main, &side])
        .unwrap();

    assert_eq!(summaries(&repo, "path:side.txt"), ["merge", "side"]);
    assert_eq!(summaries(&repo, "path:main.txt"), ["main"]);
}

#[test]
fn an_unborn_repository_returns_an_empty_final_page() {
    let (_dir, repo) = init_repo();
    let page = search_with(&repo, "anything", None).unwrap();
    assert!(page.commits.is_empty());
    assert_eq!(page.next_cursor, None);
    assert_eq!(page.scanned, 0);
}

fn linear_history(dir: &Path, repo: &Repository, messages: &[&str]) {
    for (i, message) in messages.iter().enumerate() {
        write_file(dir, &format!("f{i}.txt"), message);
        commit_all(repo, message);
    }
}

#[test]
fn paging_resumes_without_duplicates_or_gaps() {
    let (dir, repo) = init_repo();
    linear_history(
        dir.path(),
        &repo,
        &["hit 1", "miss", "hit 2", "hit 3", "miss", "hit 4", "hit 5"],
    );
    let query = parse_query("hit").unwrap();
    let limits = PageLimits {
        matches: 2,
        scan: 100,
    };

    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let page =
            search_commits(&repo, &query, None, cursor.as_deref(), limits, &|| false).unwrap();
        pages += 1;
        seen.extend(page.commits.into_iter().map(|c| c.summary));
        cursor = page.next_cursor;
        if cursor.is_none() {
            assert_eq!(page.scanned, 7);
            break;
        }
    }
    assert_eq!(seen, ["hit 5", "hit 4", "hit 3", "hit 2", "hit 1"]);
    assert_eq!(pages, 3);
}

#[test]
fn a_page_stops_at_the_scan_budget() {
    let (dir, repo) = init_repo();
    linear_history(dir.path(), &repo, &["needle", "a", "b", "c", "d", "e"]);
    let query = parse_query("needle").unwrap();
    let limits = PageLimits {
        matches: 10,
        scan: 2,
    };

    let first = search_commits(&repo, &query, None, None, limits, &|| false).unwrap();
    assert!(first.commits.is_empty());
    assert_eq!(first.scanned, 2);
    let second = search_commits(
        &repo,
        &query,
        None,
        first.next_cursor.as_deref(),
        limits,
        &|| false,
    )
    .unwrap();
    assert!(second.commits.is_empty());
    assert_eq!(second.scanned, 4);
    let third = search_commits(
        &repo,
        &query,
        None,
        second.next_cursor.as_deref(),
        limits,
        &|| false,
    )
    .unwrap();
    assert_eq!(third.commits.len(), 1);
    assert_eq!(third.scanned, 6);
    assert_eq!(third.next_cursor, None);
}

#[test]
fn resuming_after_history_moved_is_stale() {
    let (dir, repo) = init_repo();
    linear_history(dir.path(), &repo, &["hit 1", "hit 2"]);
    let query = parse_query("hit").unwrap();
    let limits = PageLimits {
        matches: 1,
        scan: 100,
    };
    let first = search_commits(&repo, &query, None, None, limits, &|| false).unwrap();
    let cursor = first.next_cursor.expect("more to scan");
    linear_history(dir.path(), &repo, &["hit 3"]);

    assert!(matches!(
        search_commits(&repo, &query, None, Some(&cursor), limits, &|| false),
        Err(SearchError::StaleCursor)
    ));
    assert!(matches!(
        search_commits(&repo, &query, None, Some("garbage"), limits, &|| false),
        Err(SearchError::StaleCursor)
    ));
}

#[test]
fn cancel_stops_before_and_during_the_walk() {
    let (dir, repo) = init_repo();
    linear_history(dir.path(), &repo, &["a", "b", "c", "d", "e"]);
    let query = parse_query("zzz").unwrap();

    assert!(matches!(
        search_commits(&repo, &query, None, None, PageLimits::DEFAULT, &|| true),
        Err(SearchError::Cancelled)
    ));

    let polls = Cell::new(0);
    let cancel = || {
        polls.set(polls.get() + 1);
        polls.get() >= 3
    };
    assert!(matches!(
        search_commits(&repo, &query, None, None, PageLimits::DEFAULT, &cancel),
        Err(SearchError::Cancelled)
    ));
    assert_eq!(polls.get(), 3);
}
