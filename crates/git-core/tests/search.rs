mod common;

use std::cell::Cell;
use std::path::Path;
use std::time::Duration;

use common::{commit_all, commit_tree, init_repo, write_file};
use git2::{Oid, Repository, Signature, Time};
use git_core::graph::GraphSelection;
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

/// `PageLimits::DEFAULT` without its 250 ms time budget. These tests read one page and expect
/// the whole match set in it. With the budget, a slow runner can end the page after the newest
/// commit (no match) and leave the match behind a `next_cursor` the test never follows (#182).
/// The time budget has its own tests below.
const NO_TIME_BUDGET: PageLimits = PageLimits {
    time: Duration::MAX,
    ..PageLimits::DEFAULT
};

/// The default graph: every local branch, no remote branch.
fn everything() -> GraphSelection {
    GraphSelection::default()
}

/// Exactly the local branches `names`, no remote branch.
fn local(names: &[String]) -> GraphSelection {
    GraphSelection {
        local: Some(names.to_vec()),
        remote: vec![],
    }
}

fn search_with(
    repo: &Repository,
    query: &str,
    refs: Option<&[String]>,
) -> Result<SearchPage, SearchError> {
    search_commits(
        repo,
        &parse_query(query)?,
        &GraphSelection {
            local: refs.map(<[String]>::to_vec),
            remote: vec![],
        },
        None,
        NO_TIME_BUDGET,
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
fn or_not_and_parentheses_combine_terms() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "1");
    commit_all(&repo, "fix login");
    write_file(dir.path(), "a.txt", "2");
    commit_all(&repo, "feat signup");
    write_file(dir.path(), "a.txt", "3");
    commit_all(&repo, "fix signup wip");
    write_file(dir.path(), "a.txt", "4");
    commit_all(&repo, "docs");

    assert_eq!(
        summaries(&repo, "login or feat"),
        ["feat signup", "fix login"]
    );
    assert_eq!(summaries(&repo, "fix -wip"), ["fix login"]);
    assert_eq!(summaries(&repo, "fix not wip"), ["fix login"]);
    assert_eq!(
        summaries(&repo, "(fix or feat) signup"),
        ["fix signup wip", "feat signup"]
    );
    assert_eq!(
        summaries(&repo, "fix or feat signup"),
        ["fix signup wip", "feat signup", "fix login"]
    );
    assert_eq!(summaries(&repo, "-(fix or feat)"), ["docs"]);
}

#[test]
fn contents_stays_limited_to_top_level_path_terms_under_or() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "docs/n.md", "needle\n");
    commit_all(&repo, "in docs");
    write_file(dir.path(), "src/n.rs", "needle\n");
    commit_all(&repo, "in src");
    write_file(dir.path(), "src/m.rs", "other\n");
    commit_all(&repo, "other in src");

    assert_eq!(
        summaries(&repo, "path:src (contents:needle or contents:other)"),
        ["other in src", "in src"]
    );
    // A `path:` inside `or` is a test of its own and limits no `contents:`.
    assert_eq!(
        summaries(&repo, "path:docs or contents:other"),
        ["other in src", "in docs"]
    );
    assert_eq!(summaries(&repo, "contents:needle -path:src"), ["in docs"]);
}

#[test]
fn merges_only_and_none_split_on_parent_count() {
    let (_dir, repo) = init_repo();
    merged_side_branch(&repo);

    assert_eq!(summaries(&repo, "merges:only"), ["merge work"]);
    assert_eq!(
        summaries(&repo, "work merges:none"),
        ["after work", "direct work", "side work", "base work"]
    );
    assert_eq!(summaries(&repo, "-merges:only after"), ["after work"]);
}

/// `trunk`: base → direct → merge(direct, side) → after; `side`: base → side work. Every
/// message contains "work".
fn merged_side_branch(repo: &Repository) {
    const FILE: u32 = 0o100644;
    let base = commit_tree(repo, &[("a.txt", "a", FILE)], &[], "base work");
    let direct = commit_tree(repo, &[("a.txt", "d", FILE)], &[base], "direct work");
    let side = commit_tree(repo, &[("a.txt", "s", FILE)], &[base], "side work");
    let merge = commit_tree(repo, &[("a.txt", "m", FILE)], &[direct, side], "merge work");
    let after = commit_tree(repo, &[("a.txt", "x", FILE)], &[merge], "after work");
    repo.reference("refs/heads/trunk", after, true, "test")
        .unwrap();
    repo.reference("refs/heads/side", side, true, "test")
        .unwrap();
    repo.reference("refs/heads/trunk-alias", after, true, "test")
        .unwrap();
}

/// A repository whose `origin` (a second local repository, fetched) holds
/// [`merged_side_branch`]; the only local branch, `main`, is at `side work`.
fn fetched_side_branch() -> (tempfile::TempDir, tempfile::TempDir, Repository) {
    let (upstream_dir, upstream) = init_repo();
    merged_side_branch(&upstream);
    let (dir, repo) = init_repo();
    repo.remote("origin", upstream_dir.path().to_str().unwrap())
        .unwrap()
        .fetch(&[] as &[&str], None, None)
        .unwrap();
    let side = repo.refname_to_id("refs/remotes/origin/side").unwrap();
    repo.reference("refs/heads/main", side, true, "test")
        .unwrap();
    repo.set_head("refs/heads/main").unwrap();
    (upstream_dir, dir, repo)
}

fn with_remote(local: Option<&[&str]>, remote: &[&str]) -> GraphSelection {
    GraphSelection {
        local: local.map(|names| names.iter().map(|n| n.to_string()).collect()),
        remote: remote.iter().map(|n| n.to_string()).collect(),
    }
}

#[test]
fn search_walks_the_selected_remote_branches_like_the_graph() {
    let (_upstream, _dir, repo) = fetched_side_branch();
    let query = parse_query("work").unwrap();

    for (selection, expected) in [
        // Default: no remote branch is walked.
        (with_remote(None, &[]), 2),
        // Every local branch plus a remote one: every ancestor.
        (with_remote(None, &["origin/trunk"]), 5),
        // One remote branch alone: its first-parent line.
        (with_remote(Some(&[]), &["origin/trunk"]), 4),
        // A local and a remote branch: every ancestor; a missing remote branch is skipped.
        (
            with_remote(Some(&["main"]), &["origin/trunk", "origin/gone"]),
            5,
        ),
    ] {
        let found: Vec<String> =
            search_commits(&repo, &query, &selection, None, NO_TIME_BUDGET, &|| false)
                .unwrap()
                .commits
                .into_iter()
                .map(|c| c.id)
                .collect();
        let graph: Vec<String> = git_core::graph::graph_log(&repo, 50, &selection)
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(found, graph, "{selection:?}");
        assert_eq!(found.len(), expected, "{selection:?}");
    }
}

#[test]
fn a_cursor_does_not_resume_after_the_remote_selection_changed() {
    let (_upstream, _dir, repo) = fetched_side_branch();
    let query = parse_query("work").unwrap();
    let limits = PageLimits {
        matches: 1,
        scan: 100,
        time: Duration::MAX,
    };
    let without = with_remote(None, &[]);
    let with = with_remote(None, &["origin/trunk"]);

    let first = search_commits(&repo, &query, &without, None, limits, &|| false).unwrap();
    let cursor = first.next_cursor.expect("more to scan");

    assert!(matches!(
        search_commits(&repo, &query, &with, Some(&cursor), limits, &|| false),
        Err(SearchError::StaleCursor)
    ));
    assert!(search_commits(&repo, &query, &without, Some(&cursor), limits, &|| false).is_ok());
}

#[test]
fn one_selected_branch_searches_its_first_parent_line_like_the_graph() {
    let (_dir, repo) = init_repo();
    merged_side_branch(&repo);
    let found = |query: &str, refs: &[&str]| -> Vec<String> {
        let refs: Vec<String> = refs.iter().map(|r| r.to_string()).collect();
        search_with(&repo, query, Some(&refs))
            .unwrap()
            .commits
            .into_iter()
            .map(|c| c.summary)
            .collect()
    };

    assert_eq!(
        found("work", &["trunk"]),
        ["after work", "merge work", "direct work", "base work"]
    );
    // Several branches, or an explicit `branch:` term, still walk every ancestor.
    assert_eq!(found("work", &["trunk", "side"]).len(), 5);
    assert_eq!(found("work branch:trunk", &["trunk"]).len(), 5);
    assert_eq!(summaries(&repo, "work").len(), 5);
}

#[test]
fn a_cursor_from_a_first_parent_walk_does_not_resume_a_full_walk() {
    let (_dir, repo) = init_repo();
    merged_side_branch(&repo);
    let query = parse_query("work").unwrap();
    let limits = PageLimits {
        matches: 1,
        scan: 100,
        time: Duration::MAX,
    };
    let one = ["trunk".to_string()];
    // Same start commit, but two selected branches: a full walk.
    let two = ["trunk".to_string(), "trunk-alias".to_string()];

    let first = search_commits(&repo, &query, &local(&one), None, limits, &|| false).unwrap();
    let cursor = first.next_cursor.expect("more to scan");

    assert!(matches!(
        search_commits(&repo, &query, &local(&two), Some(&cursor), limits, &|| {
            false
        }),
        Err(SearchError::StaleCursor)
    ));
    assert!(
        search_commits(&repo, &query, &local(&one), Some(&cursor), limits, &|| {
            false
        })
        .is_ok()
    );
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

#[test]
fn search_finds_commits_on_a_detached_head_that_no_branch_contains() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "base");
    let base = repo.head().unwrap().target().unwrap();
    repo.set_head_detached(base).unwrap();
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "detached needle");

    assert_eq!(summaries(&repo, "needle"), ["detached needle"]);
    let page = search_with(&repo, "needle", Some(&[])).unwrap();
    assert_eq!(page.commits.len(), 1);
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
        time: Duration::MAX,
    };

    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let page = search_commits(
            &repo,
            &query,
            &everything(),
            cursor.as_deref(),
            limits,
            &|| false,
        )
        .unwrap();
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
        time: Duration::MAX,
    };

    let first = search_commits(&repo, &query, &everything(), None, limits, &|| false).unwrap();
    assert!(first.commits.is_empty());
    assert_eq!(first.scanned, 2);
    let second = search_commits(
        &repo,
        &query,
        &everything(),
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
        &everything(),
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
        time: Duration::MAX,
    };
    let first = search_commits(&repo, &query, &everything(), None, limits, &|| false).unwrap();
    let cursor = first.next_cursor.expect("more to scan");
    linear_history(dir.path(), &repo, &["hit 3"]);

    assert!(matches!(
        search_commits(&repo, &query, &everything(), Some(&cursor), limits, &|| {
            false
        }),
        Err(SearchError::StaleCursor)
    ));
    assert!(matches!(
        search_commits(
            &repo,
            &query,
            &everything(),
            Some("garbage"),
            limits,
            &|| false
        ),
        Err(SearchError::StaleCursor)
    ));
}

#[test]
fn cancel_stops_before_and_during_the_walk() {
    let (dir, repo) = init_repo();
    linear_history(dir.path(), &repo, &["a", "b", "c", "d", "e"]);
    let query = parse_query("zzz").unwrap();

    assert!(matches!(
        search_commits(&repo, &query, &everything(), None, NO_TIME_BUDGET, &|| true),
        Err(SearchError::Cancelled)
    ));

    let polls = Cell::new(0);
    let cancel = || {
        polls.set(polls.get() + 1);
        polls.get() >= 3
    };
    // No time budget: a page that ran out of time would end before the third poll.
    assert!(matches!(
        search_commits(&repo, &query, &everything(), None, NO_TIME_BUDGET, &cancel),
        Err(SearchError::Cancelled)
    ));
    assert_eq!(polls.get(), 3);
}

#[test]
fn a_zero_time_budget_still_scans_one_commit_per_page_and_pages_to_the_full_match_set() {
    let (dir, repo) = init_repo();
    linear_history(
        dir.path(),
        &repo,
        &["hit 1", "miss", "hit 2", "hit 3", "miss", "hit 4", "hit 5"],
    );
    let query = parse_query("hit").unwrap();
    let limits = PageLimits {
        matches: 100,
        scan: 100,
        time: Duration::ZERO,
    };

    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    let mut previous_scanned = 0;
    let mut pages = 0;
    loop {
        let page = search_commits(
            &repo,
            &query,
            &everything(),
            cursor.as_deref(),
            limits,
            &|| false,
        )
        .unwrap();
        pages += 1;
        assert!(page.scanned > previous_scanned, "every page makes progress");
        previous_scanned = page.scanned;
        seen.extend(page.commits.into_iter().map(|c| c.summary));
        cursor = page.next_cursor;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(previous_scanned, 7);
    assert_eq!(pages, 7);
    assert_eq!(seen, summaries(&repo, "hit"));
    assert_eq!(seen, ["hit 5", "hit 4", "hit 3", "hit 2", "hit 1"]);
}

#[test]
fn cancel_is_polled_while_skipping_to_a_cursor() {
    let (dir, repo) = init_repo();
    linear_history(dir.path(), &repo, &["a", "b", "c", "d", "e", "f"]);
    let query = parse_query("zzz").unwrap();
    let limits = PageLimits {
        matches: 10,
        scan: 5,
        time: Duration::MAX,
    };
    let first = search_commits(&repo, &query, &everything(), None, limits, &|| false).unwrap();
    let cursor = first.next_cursor.expect("one commit left");

    // Polls: once before the walk, then while skipping the 5 already-scanned commits, then
    // once for the last commit. Without polling during the skip it would be 2.
    let polls = Cell::new(0);
    let counting = || {
        polls.set(polls.get() + 1);
        false
    };
    let page = search_commits(
        &repo,
        &query,
        &everything(),
        Some(&cursor),
        limits,
        &counting,
    )
    .unwrap();
    assert_eq!(page.scanned, 6);
    assert!(polls.get() > 2, "polled {} times", polls.get());

    // Cancelled on the first poll inside the skip.
    let polls = Cell::new(0);
    let cancel = || {
        polls.set(polls.get() + 1);
        polls.get() >= 2
    };
    assert!(matches!(
        search_commits(&repo, &query, &everything(), Some(&cursor), limits, &cancel),
        Err(SearchError::Cancelled)
    ));
}
