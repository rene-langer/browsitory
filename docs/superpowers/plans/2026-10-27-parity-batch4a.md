# Feature Parity Batch 4a (Commit Search) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A search field above the commit graph finds commits by text, author, committer, message, SHA, path, changed lines, date range and branch, in both the Tauri app and the VSCode extension.

**Architecture:** New `git-core::search` module parses the query (`parse_query`) and runs a paged revwalk (`search_commits`) with cheap filters first and first-parent diff filters last, an offset cursor guarded by a start-point fingerprint, a per-call scan budget, and a cancel closure. `repo-service` adds `Command::SearchCommits`, cancelled through a shared generation counter (side channel, like `CancelRegistry`). Tauri and the sidecar expose `search_commits` / `cancel_search`; `RepoClient` gains `searchCommits` / `cancelSearch`. The frontend's `useCommitSearch` hook debounces, pages in rounds, and swaps `CommitGraph` for `CommitSearchResults` the same way file history does.

**Tech Stack:** Rust (`git2` 0.21, `thiserror`, `serde` in tauri-app/sidecar), React + TypeScript, Vitest + Testing Library, WebdriverIO (tauri-driver).

**Spec:** `docs/superpowers/specs/2026-10-27-parity-batch4a-design.md`

## Global Constraints

- No new Rust or npm dependency; `docs/LICENSE_COMPLIANCE.md` is untouched.
- Page limits: `PageLimits::DEFAULT = { matches: 100, scan: 20_000 }`. Frontend: `SEARCH_DEBOUNCE_MS = 250`, `SEARCH_ROUND_SIZE = 100`.
- Cursor format: `"<offset>:<fingerprint>"`, fingerprint = first 16 hex chars of `Oid::hash_object(ObjectType::Blob, <sorted start oids, lowercase hex, joined with ",">)`.
- Walk order: `Sort::TOPOLOGICAL | Sort::TIME`, same as `graph_log`.
- Text matching is case-insensitive for every key except `path:` (libgit2 pathspec, case-sensitive).
- Dates: whole days, inclusive, compared against `(committer.when().seconds() + offset_minutes*60).div_euclid(86_400)`.
- Error copy, exactly (the `InvalidQuery` texts in the spec's table, plus):
  - `The history changed since this search started. Search again.` (`StaleCursor`)
  - `Search cancelled.` (`Cancelled`)
- UI copy, exactly: input aria-label `Search commits`, placeholder `Search commits — author:, path:, contents:…`; buttons `Search syntax` (aria-label), `Close search` (aria-label); list aria-label `Search results`; status `Searching…`, `Scanned N commits…`, `No commits match this search.`, `1 match`, `N matches`, `N matches in the first M commits`; button `Load more`; palette label `Search commits`; help footer `Terms combine with AND. Case is ignored except in path:.`. Numbers via `toLocaleString("en-US")`.
- Wire: method `search_commits` params `{ repoPath, query, refs, cursor }` → `{ commits, nextCursor, scanned }`; method `cancel_search` params `{ repoPath }` → `null`.
- Search records no undo entry (`undo::plan_for` already returns `None` for unknown commands via `_ => return None`; do not add an arm).
- Tests: `git-core`/`repo-service` use real temp-dir repos; frontend mocks `RepoClient` only.
- Every push touching `crates/`, `frontend/src/` or `e2e/` needs a `CHANGELOG.md` update (pre-push hook); the changelog lands in Task 10, so push only after Task 10.

## Review Focus

1. **Conventional-commit text** (`fix: crash`) must produce the "Put the text in quotes" error, and `"fix: crash"` must search literally. Pinned in Task 1 (`unknown_key_suggests_quotes`, `quoted_text_with_a_colon_is_literal`).
2. **Fast typing.** An older in-flight search is cancelled by the newer one in the backend, and its reply never reaches the UI. Pinned in Task 3 (`a_search_superseded_before_it_runs_replies_cancelled`) and Task 6 (`drops a stale response after the query changes`).
3. **A branch moves between pages** (fetch, commit in another tool). "Load more" must not duplicate or skip commits: `StaleCursor`, and Retry restarts. Pinned in Task 2 (`resuming_after_history_moved_is_stale`) and Task 6 (`retry restarts from the first page`).
4. **A persisted graph selection names a deleted branch.** Search must still work (unlike `graph_log`). Pinned in Task 2 (`refs_selection_limits_the_walk_and_skips_missing_refs`).
5. **A commit just after midnight in another timezone.** `after:`/`before:` use the commit's own calendar day. Pinned in Task 2 (`dates_are_inclusive_days_in_the_commits_own_timezone`).

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/git-core/src/search.rs` (new) | `SearchTerm`, `SearchQuery`, `SearchError`, `PageLimits`, `SearchPage`, `parse_query`, `search_commits`, inline parser tests |
| `crates/git-core/src/lib.rs` | `pub mod search;` |
| `crates/git-core/tests/search.rs` (new) | real-repo search tests |
| `crates/repo-service/src/worker/search.rs` (new) | handler, `WorkerHandle::search_commits` / `cancel_search` |
| `crates/repo-service/src/worker/mod.rs` | `Command::SearchCommits`, `SearchGeneration`, dispatch arm, worker tests |
| `crates/tauri-app/src/commands/search.rs` (new), `commands/mod.rs`, `main.rs` | commands, `SearchPageDto` + test |
| `crates/vscode-sidecar/src/dispatch.rs`, `tests/protocol_roundtrip.rs` | methods + DTO + round-trip |
| `frontend/src/ipc/RepoClient.ts`, `tauriRepoClient.ts`, `vscodeRepoClient.ts` (+ tests, fixtures) | `SearchPage`, `searchCommits`, `cancelSearch` |
| `frontend/src/state/useCommitSearch.ts` (new) + test | debounce, rounds, generation, selection save/restore |
| `frontend/src/lib/searchSyntax.ts` (new) | help rows |
| `frontend/src/components/CommitSearchBar.tsx`, `.module.css`, `.test.tsx` (new) | field, help panel, × |
| `frontend/src/components/CommitSearchResults.tsx`, `.module.css`, `.test.tsx` (new) | results list, status line, load more |
| `frontend/src/components/CommitGraph.tsx` (+ test) | `onRequestSearch` on Ctrl/Cmd+F |
| `frontend/src/App.tsx`, `App.test.tsx`, `lib/commands.ts` (+ test), `lib/shortcuts.ts` | wiring, palette, shortcut sheet |
| `e2e/specs/commit-search.spec.ts` (new) | E2E |
| `CHANGELOG.md`, `docs/USER_GUIDE.md`, `docs/ARCHITECTURE.md` | docs |

---

### Task 1: `git-core::search` — types and `parse_query`

**Files:**
- Create: `crates/git-core/src/search.rs`
- Modify: `crates/git-core/src/lib.rs` (add `pub mod search;` after `pub mod reset;`, alphabetical: `reset`, `search`, `signing`)

**Interfaces:**
- Produces (used by Tasks 2–4): the types and `parse_query` exactly as in the spec's "Backend" block. `SearchError` derives `Debug, Error`; `SearchTerm`/`SearchQuery`/`SearchPage`/`PageLimits` derive `Debug, Clone, PartialEq, Eq` (`PageLimits` also `Copy`).
- Also produces: `fn invalid(message: impl Into<String>) -> SearchError` (private helper) and `pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64`.

- [ ] **Step 1: Write the failing parser tests**

At the bottom of `search.rs` (the module can start with just the type definitions and a `parse_query` that returns `Err(invalid("todo"))`, so the tests compile):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use SearchTerm::*;

    fn terms(input: &str) -> Vec<SearchTerm> {
        parse_query(input).unwrap().terms
    }

    fn error(input: &str) -> String {
        match parse_query(input) {
            Err(SearchError::InvalidQuery { message }) => message,
            other => panic!("expected InvalidQuery for {input:?}, got {other:?}"),
        }
    }

    #[test]
    fn bare_words_are_separate_text_terms() {
        assert_eq!(terms("  fix   Login "), vec![Text("fix".into()), Text("Login".into())]);
    }

    #[test]
    fn quotes_make_a_phrase_with_escapes() {
        assert_eq!(terms(r#""fix login" "say \"hi\" \\o/""#), vec![
            Text("fix login".into()),
            Text(r#"say "hi" \o/"#.into()),
        ]);
    }

    #[test]
    fn quoted_text_with_a_colon_is_literal() {
        assert_eq!(terms(r#""fix: crash""#), vec![Text("fix: crash".into())]);
    }

    #[test]
    fn every_key_parses_and_keys_ignore_case() {
        assert_eq!(
            terms(r#"Author:ada committer:bot message:"two words" COMMIT:AbC1 path:src/ contents:needle branch:main"#),
            vec![
                Author("ada".into()),
                Committer("bot".into()),
                Message("two words".into()),
                Commit("abc1".into()),
                Path("src/".into()),
                Contents("needle".into()),
                Branch("main".into()),
            ]
        );
    }

    #[test]
    fn dates_parse_to_days_since_the_epoch() {
        assert_eq!(terms("after:2026-01-31 before:2024-02-29 after:1969-12-31"), vec![
            After(20_484),
            Before(19_782),
            After(-1),
        ]);
    }

    #[test]
    fn unknown_key_suggests_quotes() {
        assert_eq!(
            error("feat: crash"),
            r#"Unknown search key "feat:". Put the text in quotes to search for it literally."#
        );
    }

    #[test]
    fn invalid_queries_have_specific_messages() {
        assert_eq!(error("   "), "Type something to search for.");
        assert_eq!(error("author:"), "author: needs a value.");
        assert_eq!(error("author: ada"), "author: needs a value.");
        assert_eq!(error(r#""abc"#), "Missing closing quote.");
        assert_eq!(error(r#""""#), "Empty quotes don't match anything.");
        assert_eq!(error(r#"author:"""#), "Empty quotes don't match anything.");
        assert_eq!(error("after:2026-13-01"), "after: expects a date like 2026-01-31.");
        assert_eq!(error("before:2026-02-30"), "before: expects a date like 2026-01-31.");
        assert_eq!(error("after:2026-1-5"), "after: expects a date like 2026-01-31.");
        assert_eq!(error("before:2023-02-29"), "before: expects a date like 2026-01-31.");
        assert_eq!(error("commit:xyz"), "commit: expects a hexadecimal SHA prefix.");
        assert_eq!(error("branch:a branch:b"), "Use at most one branch: term.");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --lib search::tests`
Expected: FAIL (every test hits the `"todo"` error).

- [ ] **Step 3: Implement `parse_query`**

Tokenizer over `char_indices`, following the spec's grammar:
- Skip whitespace. If the next run is `[A-Za-z]+` immediately followed by `:`, it is a key (lowercased). Known keys: `author committer message commit path contents after before branch`. Any other key is the unknown-key error. A key followed by whitespace or end of input is `"<key>: needs a value."`.
- A value starting with `"` reads until the matching unescaped `"`, unescaping `\"` and `\\` only. A missing close quote is `Missing closing quote.`. An empty result is `Empty quotes don't match anything.`. Any other value runs to the next whitespace.
- `commit:` must match `^[0-9a-fA-F]{1,40}$` and is stored lowercased. `after:`/`before:` must match `^\d{4}-\d{2}-\d{2}$` with month 1–12 and the day valid for that month (leap year: divisible by 4 and not by 100, or by 400). They are stored as `days_from_civil(..)`.
- After tokenizing: no terms → `Type something to search for.`; more than one `Branch` → `Use at most one branch: term.`.

`days_from_civil` (Howard Hinnant's algorithm; the body is fixed):

```rust
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
```

`search_commits` stays `unimplemented!()` until Task 2. Mark it `#[allow(unused_variables)]` if clippy complains.

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core --lib search::tests`
Expected: PASS (7 tests).

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy -p git-core --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/git-core/src/search.rs crates/git-core/src/lib.rs
git commit -m "feat(git-core): commit search query parser"
```

---

### Task 2: `git-core::search::search_commits`

**Files:**
- Modify: `crates/git-core/src/search.rs`
- Test: `crates/git-core/tests/search.rs` (new)

**Interfaces:**
- Consumes: Task 1 types; `crate::graph::RefDecorations` (`load`, `graph_commit`, both `pub(crate)` in `graph.rs`).
- Produces (used by Task 3): `search_commits(repo, &SearchQuery, Option<&[String]>, Option<&str>, PageLimits, &dyn Fn() -> bool) -> Result<SearchPage, SearchError>`.

- [ ] **Step 1: Write the failing tests**

`crates/git-core/tests/search.rs`:

```rust
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
    index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let author = Signature::new(author.0, author.1, &when).unwrap();
    let committer = Signature::new(committer.0, committer.1, &when).unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &author, &committer, message, &tree, &parents).unwrap()
}

fn search_with(repo: &Repository, query: &str, refs: Option<&[String]>) -> Result<SearchPage, SearchError> {
    search_commits(repo, &parse_query(query)?, refs, None, PageLimits::DEFAULT, &|| false)
}

fn summaries(repo: &Repository, query: &str) -> Vec<String> {
    search_with(repo, query, None).unwrap().commits.into_iter().map(|c| c.summary).collect()
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
    assert_eq!(summaries(&repo, &format!("commit:{}", &id[..6].to_uppercase())), ["Fix Login bug"]);
    assert!(summaries(&repo, &format!("message:{}", &id[..8])).is_empty());
    assert_eq!(summaries(&repo, "message:TOKEN"), ["Fix Login bug"]);
}

#[test]
fn author_and_committer_match_name_or_email() {
    let (dir, repo) = init_repo();
    commit_as(&repo, dir.path(), "a.txt", "a", "by ada", ("Ada Lovelace", "ada@example.com"), ("Bot", "bot@ci.test"), at(T0, 0));
    commit_as(&repo, dir.path(), "b.txt", "b", "by test", ("Test User", "test@other.test"), ("Test User", "test@other.test"), at(T0 + 60, 0));

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
    commit_as(&repo, dir.path(), "a.txt", "a", "march 9", who, who, at(T0, 0));
    // 2026-03-10T23:30Z at +01:00 is 2026-03-11 00:30 local.
    commit_as(&repo, dir.path(), "b.txt", "b", "march 11 local", who, who, at(1_773_185_400, 60));
    commit_as(&repo, dir.path(), "c.txt", "c", "march 12", who, who, at(1_773_302_400, 0));

    assert_eq!(summaries(&repo, "after:2026-03-11"), ["march 12", "march 11 local"]);
    assert_eq!(summaries(&repo, "before:2026-03-11"), ["march 11 local", "march 9"]);
    assert_eq!(summaries(&repo, "after:2026-03-11 before:2026-03-11"), ["march 11 local"]);
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
        .commit(Some("refs/heads/topic"), &sig, &sig, "topic work", &base.tree().unwrap(), &[&base])
        .unwrap();
    repo.reference("refs/remotes/origin/feature", topic_tip, true, "test").unwrap();
    let main = repo.head().unwrap().shorthand().unwrap().to_string();

    assert_eq!(summaries(&repo, "work"), ["topic work", "base work"]);
    assert_eq!(summaries(&repo, &format!("work branch:{main}")), ["base work"]);
    assert_eq!(summaries(&repo, "work branch:origin/feature"), ["topic work", "base work"]);
    match search_with(&repo, "branch:nope", None) {
        Err(SearchError::InvalidQuery { message }) => assert_eq!(message, "No branch named \"nope\"."),
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
    repo.commit(Some("refs/heads/feature"), &sig, &sig, "feature work", &base.tree().unwrap(), &[&base])
        .unwrap();

    let found = |refs: Option<Vec<String>>| -> Vec<String> {
        search_with(&repo, "work", refs.as_deref()).unwrap().commits.into_iter().map(|c| c.summary).collect()
    };
    assert!(found(Some(vec![main.clone()])).is_empty());
    assert_eq!(found(Some(vec!["feature".into()])), ["feature work"]);
    assert_eq!(found(Some(vec!["feature".into(), "deleted-branch".into()])), ["feature work"]);
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
    let side = repo.find_commit(repo.commit(None, &sig, &sig, "side", &side_tree, &[&base]).unwrap()).unwrap();
    index.remove_path(Path::new("side.txt")).unwrap();
    index.write().unwrap();
    std::fs::remove_file(dir.path().join("side.txt")).unwrap();
    // main adds main.txt
    write_file(dir.path(), "main.txt", "m");
    commit_all(&repo, "main");
    let main = repo.head().unwrap().peel_to_commit().unwrap();
    let mut merged = repo.merge_commits(&main, &side, None).unwrap();
    let tree = repo.find_tree(merged.write_tree_to(&repo).unwrap()).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "merge", &tree, &[&main, &side]).unwrap();

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
    linear_history(dir.path(), &repo, &["hit 1", "miss", "hit 2", "hit 3", "miss", "hit 4", "hit 5"]);
    let query = parse_query("hit").unwrap();
    let limits = PageLimits { matches: 2, scan: 100 };

    let mut seen = Vec::new();
    let mut cursor: Option<String> = None;
    let mut pages = 0;
    loop {
        let page = search_commits(&repo, &query, None, cursor.as_deref(), limits, &|| false).unwrap();
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
    let limits = PageLimits { matches: 10, scan: 2 };

    let first = search_commits(&repo, &query, None, None, limits, &|| false).unwrap();
    assert!(first.commits.is_empty());
    assert_eq!(first.scanned, 2);
    let second = search_commits(&repo, &query, None, first.next_cursor.as_deref(), limits, &|| false).unwrap();
    assert!(second.commits.is_empty());
    assert_eq!(second.scanned, 4);
    let third = search_commits(&repo, &query, None, second.next_cursor.as_deref(), limits, &|| false).unwrap();
    assert_eq!(third.commits.len(), 1);
    assert_eq!(third.scanned, 6);
    assert_eq!(third.next_cursor, None);
}

#[test]
fn resuming_after_history_moved_is_stale() {
    let (dir, repo) = init_repo();
    linear_history(dir.path(), &repo, &["hit 1", "hit 2"]);
    let query = parse_query("hit").unwrap();
    let limits = PageLimits { matches: 1, scan: 100 };
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test search`
Expected: FAIL (panics at `unimplemented!()`).

- [ ] **Step 3: Implement `search_commits`**

In this order:
1. **Start points**: if the query has `Branch(name)`, resolve it with `repo.find_reference` on `refs/heads/{name}`, then `refs/remotes/{name}`, then `refs/tags/{name}`, using the first that exists, peeled with `peel_to_commit`. If none exists: `invalid(format!("No branch named \"{name}\"."))`. Otherwise `refs = None` → every `refs/heads/*` (`repo.references_glob("refs/heads/*")`), and `Some(names)` → `refs/heads/{name}` for each name, skipping refs that don't exist or don't peel to a commit. Sort and dedup the oids.
2. **Fingerprint and cursor**: compute the fingerprint (Global Constraints). With a cursor, split it at the last `:`. A parse failure or a fingerprint mismatch is `StaleCursor`. No cursor means offset 0.
3. `if cancel() { return Err(Cancelled) }`. With no start points, return `SearchPage { commits: vec![], next_cursor: None, scanned: 0 }`.
4. Create the revwalk (`TOPOLOGICAL | TIME`), push every start oid, `RefDecorations::load(repo)?`, skip `offset` oids, then `.peekable()`. Loop: stop when `commits.len() >= limits.matches || scanned_here >= limits.scan`. Check `cancel()` before each `next()`. `position` counts from `offset`. When the iterator is exhausted (`peek().is_none()`), `next_cursor = None`; otherwise `Some(format!("{position}:{fingerprint}"))`. `scanned = position`.
5. **Matching** (`fn matches(repo, commit, terms) -> Result<bool, git2::Error>`): sort a copy of the terms by cost (`Commit` 0, `After`/`Before` 1, `Author`/`Committer` 2, `Text`/`Message` 3, `Path` 4, `Contents` 5; `Branch` is skipped) once per call, not per commit. Message = `String::from_utf8_lossy(commit.message_bytes()).to_lowercase()`. Names and emails use `.ok().unwrap_or_default()` (see ARCHITECTURE's git2 gotchas). `Text` also matches when the term is 4–40 hex chars and the id starts with its lowercase. The day number is `(when.seconds() + i64::from(when.offset_minutes()) * 60).div_euclid(86_400)`.
6. **First-parent diff**: build it at most once per commit, lazily, when the first `Path` or `Contents` term is reached. Use `diff_tree_to_tree(first_parent_tree.as_ref(), Some(&tree), Some(&mut opts))`, where `opts.pathspec(p)` is called for every `Path` term. Each `Path` term needs its own non-empty diff, so for two `Path` terms build one diff per term for the `Path` check. The `Contents` diff uses all `Path` patterns together. `Contents`: `diff.foreach` with a line callback that returns `false` (stop) on the first `+`/`-` line (`DiffLineType::Addition | Deletion`) whose lossy, lowercased content contains the term. `foreach` then returns `Err` with `ErrorCode::User`; treat that as "found", and treat any other `Err` as an error. Binary deltas yield no lines, so they never match.

Lowercase each term value once, in the sorted copy, not per commit.

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core --test search && cargo test -p git-core`
Expected: PASS (14 new tests, existing suite unchanged).

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy -p git-core --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/git-core
git commit -m "feat(git-core): paged, cancellable commit search"
```

---

### Task 3: `repo-service` `SearchCommits` with one in-flight search per repo

**Files:**
- Create: `crates/repo-service/src/worker/search.rs`
- Modify: `crates/repo-service/src/worker/mod.rs` (add `mod search;` to the module list, `Command::SearchCommits` after `GetFileHistory`, a `SearchGeneration` type next to `CancelRegistry`, the `search_generation` field on `Worker`/`WorkerHandle`/`spawn_with`/`handle()`, the dispatch arm, and tests after `get_commit_signature_reports_unsigned_for_a_plain_commit`)

**Interfaces:**
- Consumes: `git_core::search::{parse_query, search_commits, PageLimits, SearchPage}` (Tasks 1–2).
- Produces (used by Task 4):
  - `pub type SearchGeneration = Arc<AtomicU64>;`
  - `WorkerHandle::search_commits(&self, query: String, refs: Option<Vec<String>>, cursor: Option<String>) -> Result<git_core::search::SearchPage, String>`
  - `WorkerHandle::cancel_search(&self)`
  - `Command::SearchCommits { query: String, refs: Option<Vec<String>>, cursor: Option<String>, generation: u64, reply: Sender<Result<git_core::search::SearchPage, String>> }`

- [ ] **Step 1: Write the failing worker tests**

In `mod.rs`'s test module (add `use std::sync::atomic::Ordering;`):

```rust
    fn two_commit_repo() -> (TempDir, Repository) {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        commit_all(&repo, "alpha commit");
        write_file(dir.path(), "b.txt", "b");
        commit_all(&repo, "beta commit");
        (dir, repo)
    }

    #[test]
    fn search_commits_round_trips_through_the_worker() {
        let (dir, _repo) = two_commit_repo();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();

        let page = worker.handle().search_commits("alpha".into(), None, None).unwrap();

        assert_eq!(page.commits.len(), 1);
        assert_eq!(page.commits[0].summary, "alpha commit");
        assert_eq!(page.next_cursor, None);
        assert_eq!(page.scanned, 2);
    }

    #[test]
    fn an_invalid_search_query_replies_its_message() {
        let (dir, _repo) = two_commit_repo();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();

        assert_eq!(
            worker.handle().search_commits("author:".into(), None, None).unwrap_err(),
            "author: needs a value."
        );
    }

    #[test]
    fn a_search_superseded_before_it_runs_replies_cancelled() {
        let (dir, _repo) = two_commit_repo();
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        // Claim a generation the way `search_commits` does, then let a newer search/cancel
        // supersede it before the command is even queued.
        let stale = handle.search_generation.fetch_add(1, Ordering::SeqCst) + 1;
        handle.cancel_search();
        let (tx, rx) = mpsc::channel();
        handle
            .tx
            .send(Command::SearchCommits {
                query: "alpha".into(),
                refs: None,
                cursor: None,
                generation: stale,
                reply: tx,
            })
            .unwrap();

        assert_eq!(rx.recv().unwrap().unwrap_err(), "Search cancelled.");
        // A fresh search after the cancel runs normally.
        assert_eq!(handle.search_commits("alpha".into(), None, None).unwrap().commits.len(), 1);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p repo-service search`
Expected: FAIL to compile (`search_commits` / `search_generation` missing).

- [ ] **Step 3: Implement**

- `mod.rs`: `use std::sync::atomic::{AtomicU64, Ordering};`. Add the type with a doc comment: the generation of the newest search; a running search whose generation is no longer current stops; a side channel for the same reason as `CancelRegistry`. Add `search_generation: SearchGeneration` to `Worker` and `WorkerHandle`, create it in `spawn_with` next to `cancelled`, clone it into the thread and into `handle()`.
- Dispatch arm, after `GetFileHistory`:

```rust
                    Command::SearchCommits { query, refs, cursor, generation, reply } => {
                        search::search_commits(&repo, &search_generation, generation, query, refs, cursor, reply)
                    }
```

- `worker/search.rs`:
  - `pub(super) fn search_commits(repo: &git2::Repository, current: &SearchGeneration, generation: u64, query: String, refs: Option<Vec<String>>, cursor: Option<String>, reply: Sender<Result<SearchPage, String>>)`: parses the query, then calls `git_core::search::search_commits(repo, &parsed, refs.as_deref(), cursor.as_deref(), PageLimits::DEFAULT, &|| current.load(Ordering::SeqCst) != generation)`, mapping every error with `to_string()`.
  - `impl WorkerHandle`: `search_commits` does `let generation = self.search_generation.fetch_add(1, Ordering::SeqCst) + 1;`, sends the command, and waits for the reply, with the same "worker thread stopped" errors as `get_file_history`. `cancel_search` does `self.search_generation.fetch_add(1, Ordering::SeqCst);`. Doc-comment both: they never go through `self.tx` to cancel.

- [ ] **Step 4: Run tests**

Run: `cargo test -p repo-service`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/repo-service
git commit -m "feat(repo-service): SearchCommits worker command with generation cancel"
```

---

### Task 4: Tauri commands and sidecar methods

**Files:**
- Create: `crates/tauri-app/src/commands/search.rs`
- Modify: `crates/tauri-app/src/commands/mod.rs` (add `mod search;` and `pub use search::{cancel_search, search_commits};`; add `SearchPageDto` after `FileHistoryEntryDto`'s `impl`; add a test after `file_history_entry_dto_serializes_camel_case_and_null_old_path`), `crates/tauri-app/src/main.rs` (the `use` list and `tauri::generate_handler![…]`, next to `get_file_history`)
- Modify: `crates/vscode-sidecar/src/dispatch.rs` (method table after `"get_file_history"`, functions and DTO after `fn get_file_history`), `crates/vscode-sidecar/tests/protocol_roundtrip.rs` (new test after `commit_graph_reflects_a_commit_through_the_sidecar`)

**Interfaces:**
- Consumes: `WorkerHandle::search_commits` / `cancel_search` (Task 3).
- Produces: the wire methods in Global Constraints.

- [ ] **Step 1: Write the failing Tauri DTO test**

```rust
    #[test]
    fn search_page_dto_serializes_camel_case_and_null_cursor() {
        let commit = GraphCommit {
            id: "abc123".into(),
            short_id: "abc123".into(),
            summary: "Hit".into(),
            author_name: "Rene".into(),
            author_email: "rene@example.com".into(),
            timestamp: 1_700_000_000,
            parent_ids: vec![],
            branch_refs: vec![],
            remote_branch_refs: vec![],
        };
        let done = serde_json::to_value(SearchPageDto::from(git_core::search::SearchPage {
            commits: vec![commit.clone()],
            next_cursor: None,
            scanned: 42,
        }))
        .unwrap();
        assert_eq!(done["nextCursor"], serde_json::Value::Null);
        assert!(done.as_object().unwrap().contains_key("nextCursor"));
        assert_eq!(done["scanned"], 42);
        assert_eq!(done["commits"][0]["shortId"], "abc123");

        let more = serde_json::to_value(SearchPageDto::from(git_core::search::SearchPage {
            commits: vec![],
            next_cursor: Some("20000:0123456789abcdef".into()),
            scanned: 20_000,
        }))
        .unwrap();
        assert_eq!(more["nextCursor"], "20000:0123456789abcdef");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p tauri-app search_page_dto`
Expected: FAIL to compile (`SearchPageDto` missing).

- [ ] **Step 3: Implement the Tauri side**

- `SearchPageDto { pub commits: Vec<GraphCommitDto>, pub next_cursor: Option<String>, pub scanned: usize }`, `#[derive(Serialize)]`, `#[serde(rename_all = "camelCase")]`, plus `From<git_core::search::SearchPage>` (map the commits through `GraphCommitDto::from`).
- `commands/search.rs`:
  - `pub async fn search_commits(repo_path: String, query: String, refs: Option<Vec<String>>, cursor: Option<String>, state: State<'_, AppState>) -> Result<SearchPageDto, String>`
  - `pub async fn cancel_search(repo_path: String, state: State<'_, AppState>) -> Result<(), String>`
  - Both are `#[tauri::command]` `async fn` (ARCHITECTURE "Threading model") and go through `worker_handle(&state, &repo_path)?`.
- Register both in `main.rs`.

- [ ] **Step 4: Write the failing sidecar round-trip test**

```rust
#[test]
fn search_commits_and_cancel_search_round_trip_through_the_sidecar() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "a.txt", "a");
    commit_all(&repo, "alpha commit");
    write_file(dir.path(), "b.txt", "b");
    commit_all(&repo, "beta commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let page = sidecar.call(
        2,
        "search_commits",
        serde_json::json!({"repoPath": repo_path, "query": "beta", "refs": null, "cursor": null}),
    );
    let result = &page["result"];
    assert_eq!(result["commits"].as_array().unwrap().len(), 1);
    assert_eq!(result["commits"][0]["summary"], "beta commit");
    assert_eq!(result["nextCursor"], serde_json::Value::Null);
    assert_eq!(result["scanned"], 2);

    let invalid = sidecar.call(
        3,
        "search_commits",
        serde_json::json!({"repoPath": repo_path, "query": "feat: x", "refs": null, "cursor": null}),
    );
    assert!(invalid["error"]["message"].as_str().unwrap().starts_with("Unknown search key"));

    let cancelled = sidecar.call(4, "cancel_search", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(cancelled["result"], serde_json::Value::Null);
}
```

Check how existing tests read a JSON-RPC error (e.g. `get_status_on_an_unopened_repo_returns_a_json_rpc_error`) and use the same field path if it isn't `["error"]["message"]`.

- [ ] **Step 5: Implement the sidecar methods**

- Method table: `"search_commits" => search_commits(params, repos),` and `"cancel_search" => cancel_search(params, repos),`.
- `SearchCommitsParams { repo_path: String, query: String, refs: Option<Vec<String>>, cursor: Option<String> }` (`Deserialize`, camelCase). A private `SearchPageDto`, the same shape as Tauri's, built from `GraphCommitDto::from`.
- `cancel_search` deserializes `RepoPathParams`, calls `cancel_search()`, and returns `Ok(Value::Null)`.

- [ ] **Step 6: Run tests**

Run: `cargo test -p tauri-app search_page_dto && cargo test -p vscode-sidecar`
Expected: PASS.

- [ ] **Step 7: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/tauri-app crates/vscode-sidecar
git commit -m "feat: expose search_commits and cancel_search over Tauri and the VSCode sidecar"
```

---

### Task 5: `RepoClient.searchCommits` / `cancelSearch` in both clients

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts` (`SearchPage` after `FileHistoryEntry`; methods after `getFileHistory`), `frontend/src/ipc/tauriRepoClient.ts` (after `getFileHistory`), `frontend/src/ipc/vscodeRepoClient.ts` (after `getFileHistory`)
- Test: `frontend/src/ipc/tauriRepoClient.test.ts`, `frontend/src/ipc/vscodeRepoClient.test.ts`
- Fixtures: every full `RepoClient` literal (`App.test.tsx` `fakeClient`, `useAppState.test.ts` (7 literals), `ConflictResolutionPane.test.tsx`, `DiffPane.test.tsx`, `RebasePlanner.test.tsx`, `RepoPicker.test.tsx`, `CommitHeader.test.tsx`, wherever `tsc` reports them)

**Interfaces:**
- Produces (used by Tasks 6–8):

```ts
export interface SearchPage { commits: GraphCommit[]; nextCursor: string | null; scanned: number; }
searchCommits(repoPath: string, query: string, refs: string[] | null, cursor: string | null): Promise<SearchPage>;
cancelSearch(repoPath: string): Promise<void>;
```

- [ ] **Step 1: Write the failing client tests**

`tauriRepoClient.test.ts`, a new test:

```ts
  it("invokes search_commits without logging its errors, and cancel_search", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("Search cancelled.");
    await expect(tauriRepoClient.searchCommits("/repo", "author:ada", null, "100:abc")).rejects.toBe(
      "Search cancelled.",
    );
    expect(invoke).toHaveBeenLastCalledWith("search_commits", {
      repoPath: "/repo",
      query: "author:ada",
      refs: null,
      cursor: "100:abc",
    });
    expect(invoke).not.toHaveBeenCalledWith("plugin:log|log", expect.anything());

    vi.mocked(invoke).mockResolvedValueOnce(null);
    await tauriRepoClient.cancelSearch("/repo");
    expect(invoke).toHaveBeenLastCalledWith("cancel_search", { repoPath: "/repo" });
  });
```

Check how the file mocks `@tauri-apps/plugin-log`'s `error` (imported as `logError`). If it is a `vi.mock`, assert `expect(logError).not.toHaveBeenCalled()` instead of the `plugin:log|log` line.

`vscodeRepoClient.test.ts`, after "wires getCommitSignature":

```ts
  it("wires searchCommits and cancelSearch", async () => {
    const promise = vscodeRepoClient.searchCommits("/repo", "fix", ["main"], null);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "search_commits",
      params: { repoPath: "/repo", query: "fix", refs: ["main"], cursor: null },
    });
    const page = { commits: [], nextCursor: null, scanned: 3 };
    respond(1, page);
    await expect(promise).resolves.toEqual(page);

    const cancel = vscodeRepoClient.cancelSearch("/repo");
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "cancel_search",
      params: { repoPath: "/repo" },
    });
    respond(2, null);
    await expect(cancel).resolves.toBeNull();
  });
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/ipc`
Expected: FAIL (`searchCommits is not a function`).

- [ ] **Step 3: Implement**

- `RepoClient.ts`: the interface and the two methods, with a doc comment (`refs` has the same meaning as `getCommitGraph`'s `selectedBranches`; rejects with the query error text, or "Search cancelled." when superseded).
- `tauriRepoClient.ts`: `searchCommits: (repoPath, query, refs, cursor) => invoke<SearchPage>("search_commits", { repoPath, query, refs, cursor })`. Plain `invoke` on purpose, with a one-line comment: query errors and cancellations are expected while typing and are shown in the UI, not logged. `cancelSearch: (repoPath) => loggedInvoke("cancel_search", { repoPath })`.
- `vscodeRepoClient.ts`: `call<SearchPage>("search_commits", …)` and `call<void>("cancel_search", { repoPath })`.

- [ ] **Step 4: Fix full `RepoClient` fixtures**

Run: `cd frontend && pnpm exec tsc --noEmit -p .`
Add to every literal that errors: `searchCommits: async () => ({ commits: [], nextCursor: null, scanned: 0 }),` and `cancelSearch: async () => {},`. In `App.test.tsx`'s `fakeClient`, use `searchCommits: unused` (no test should search unless it overrides it) and `cancelSearch: async () => {}`. Repeat until `tsc` is clean.

- [ ] **Step 5: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): RepoClient.searchCommits and cancelSearch in both clients"
```

---

### Task 6: `useCommitSearch` hook

**Files:**
- Create: `frontend/src/state/useCommitSearch.ts`, `frontend/src/state/useCommitSearch.test.ts`

**Interfaces:**
- Consumes: `RepoClient.searchCommits` / `cancelSearch` (Task 5), `SelectedRow` from `./useAppState`, `describeError` from `../lib/errorMessages`.
- Produces (used by Tasks 7–8):

```ts
export const SEARCH_DEBOUNCE_MS = 250;
export const SEARCH_ROUND_SIZE = 100;

export interface CommitSearchResultsState {
  query: string;              // trimmed query these results belong to
  commits: GraphCommit[];
  nextCursor: string | null;
  scanned: number;
  loading: boolean;
  error: DescribedError | null;
}

export interface UseCommitSearchResult {
  /** Text in the field. */
  query: string;
  /** `null` while the graph shows (empty field). */
  results: CommitSearchResultsState | null;
  setQuery(text: string): void;
  loadMore(): void;
  retry(): void;
  close(): void;
}

export function useCommitSearch(
  client: RepoClient,
  repoPath: string,
  refreshGeneration: number,
  refs: string[] | null,
  selectedRow: SelectedRow,
  selectRow: (row: SelectedRow) => void,
  options?: { debounceMs?: number },
): UseCommitSearchResult;
```

- [ ] **Step 1: Write the failing tests**

```ts
import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { GraphCommit, RepoClient, SearchPage } from "../ipc/RepoClient";
import type { SelectedRow } from "./useAppState";
import { SEARCH_DEBOUNCE_MS, SEARCH_ROUND_SIZE, useCommitSearch } from "./useCommitSearch";

function commit(id: string): GraphCommit {
  return {
    id, shortId: id.slice(0, 7), summary: `commit ${id}`, authorName: "A", authorEmail: "a@example.com",
    timestamp: 1_700_000_000, parentIds: [], branchRefs: [], remoteBranchRefs: [],
  };
}
function page(ids: string[], nextCursor: string | null, scanned: number): SearchPage {
  return { commits: ids.map(commit), nextCursor, scanned };
}
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej; });
  return { promise, resolve, reject };
}

function setup(
  searchCommits: RepoClient["searchCommits"],
  { row = "uncommitted" as SelectedRow, debounceMs = 0 } = {},
) {
  const cancelSearch = vi.fn(async () => {});
  const client = { searchCommits, cancelSearch } as unknown as RepoClient;
  const selectRow = vi.fn();
  const hook = renderHook(
    ({ generation, refs, selected }: { generation: number; refs: string[] | null; selected: SelectedRow }) =>
      useCommitSearch(client, "/repo", generation, refs, selected, selectRow, { debounceMs }),
    { initialProps: { generation: 0, refs: ["main"] as string[] | null, selected: row } },
  );
  return { ...hook, selectRow, cancelSearch };
}

afterEach(() => vi.useRealTimers());

describe("useCommitSearch", () => {
  it("debounces typing and searches once with the graph's branch selection", async () => {
    vi.useFakeTimers();
    const searchCommits = vi.fn(async () => page([], null, 0));
    const { result } = setup(searchCommits, { debounceMs: SEARCH_DEBOUNCE_MS });

    act(() => result.current.setQuery("a"));
    act(() => result.current.setQuery("au"));
    await act(async () => { await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS - 1); });
    expect(searchCommits).not.toHaveBeenCalled();
    expect(result.current.query).toBe("au");
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });

    expect(searchCommits).toHaveBeenCalledTimes(1);
    expect(searchCommits).toHaveBeenCalledWith("/repo", "au", ["main"], null);
  });

  it("keeps paging until a round is full or the walk ends, reporting scanned progress", async () => {
    const second = deferred<SearchPage>();
    const searchCommits = vi.fn()
      .mockResolvedValueOnce(page(["c1"], "20000:fp", 20_000))
      .mockReturnValueOnce(second.promise);
    const { result } = setup(searchCommits);

    act(() => result.current.setQuery("x"));
    await waitFor(() => expect(result.current.results?.scanned).toBe(20_000));
    expect(result.current.results?.loading).toBe(true);
    expect(result.current.results?.commits.map((c) => c.id)).toEqual(["c1"]);
    expect(searchCommits).toHaveBeenLastCalledWith("/repo", "x", ["main"], "20000:fp");

    await act(async () => second.resolve(page(["c2"], null, 25_000)));
    expect(result.current.results).toMatchObject({ loading: false, nextCursor: null, scanned: 25_000 });
    expect(result.current.results?.commits.map((c) => c.id)).toEqual(["c1", "c2"]);
  });

  it("stops a round at SEARCH_ROUND_SIZE matches and loadMore continues from nextCursor", async () => {
    const full = Array.from({ length: SEARCH_ROUND_SIZE }, (_, i) => `c${i}`);
    const searchCommits = vi.fn()
      .mockResolvedValueOnce(page(full, "100:fp", 500))
      .mockResolvedValueOnce(page(["more"], null, 900));
    const { result } = setup(searchCommits);

    act(() => result.current.setQuery("x"));
    await waitFor(() => expect(result.current.results?.loading).toBe(false));
    expect(searchCommits).toHaveBeenCalledTimes(1);

    act(() => result.current.loadMore());
    await waitFor(() => expect(result.current.results?.nextCursor).toBeNull());
    expect(searchCommits).toHaveBeenLastCalledWith("/repo", "x", ["main"], "100:fp");
    expect(result.current.results?.commits).toHaveLength(SEARCH_ROUND_SIZE + 1);
  });

  it("selects the first match of a new query and close restores the previous row and cancels", async () => {
    const pending = deferred<SearchPage>();
    const searchCommits = vi.fn()
      .mockResolvedValueOnce(page(["c1"], "1:fp", 20_000))
      .mockReturnValueOnce(pending.promise);
    const { result, selectRow, cancelSearch } = setup(searchCommits, { row: { commitId: "before" } });

    act(() => result.current.setQuery("x"));
    await waitFor(() => expect(selectRow).toHaveBeenCalledWith({ commitId: "c1" }));

    act(() => result.current.close());
    expect(result.current.results).toBeNull();
    expect(result.current.query).toBe("");
    expect(selectRow).toHaveBeenLastCalledWith({ commitId: "before" });
    expect(cancelSearch).toHaveBeenCalledWith("/repo");

    await act(async () => pending.resolve(page(["late"], null, 30_000)));
    expect(result.current.results).toBeNull();
  });

  it("drops a stale response after the query changes", async () => {
    const first = deferred<SearchPage>();
    const searchCommits = vi.fn()
      .mockReturnValueOnce(first.promise)
      .mockResolvedValueOnce(page(["new"], null, 2));
    const { result } = setup(searchCommits);

    act(() => result.current.setQuery("old"));
    await waitFor(() => expect(searchCommits).toHaveBeenCalledTimes(1));
    act(() => result.current.setQuery("new"));
    await waitFor(() => expect(result.current.results?.loading).toBe(false));
    await act(async () => first.resolve(page(["old"], null, 2)));

    expect(result.current.results?.query).toBe("new");
    expect(result.current.results?.commits.map((c) => c.id)).toEqual(["new"]);
  });

  it("shows an error and retry restarts from the first page", async () => {
    const searchCommits = vi.fn()
      .mockResolvedValueOnce(page(Array.from({ length: SEARCH_ROUND_SIZE }, (_, i) => `c${i}`), "100:fp", 100))
      .mockRejectedValueOnce(new Error("The history changed since this search started. Search again."))
      .mockResolvedValueOnce(page(["fresh"], null, 5));
    const { result } = setup(searchCommits);

    act(() => result.current.setQuery("x"));
    await waitFor(() => expect(result.current.results?.loading).toBe(false));
    act(() => result.current.loadMore());
    await waitFor(() => expect(result.current.results?.error?.message).toMatch(/history changed/));

    act(() => result.current.retry());
    await waitFor(() => expect(result.current.results?.commits.map((c) => c.id)).toEqual(["fresh"]));
    expect(searchCommits).toHaveBeenLastCalledWith("/repo", "x", ["main"], null);
    expect(result.current.results?.error).toBeNull();
  });

  it("re-runs from the first page on refresh without changing the selection", async () => {
    const searchCommits = vi.fn(async () => page(["c1"], null, 1));
    const { result, rerender, selectRow } = setup(searchCommits);

    act(() => result.current.setQuery("x"));
    await waitFor(() => expect(result.current.results?.loading).toBe(false));
    selectRow.mockClear();
    rerender({ generation: 1, refs: ["main"], selected: { commitId: "c1" } });

    await waitFor(() => expect(searchCommits).toHaveBeenCalledTimes(2));
    expect(searchCommits).toHaveBeenLastCalledWith("/repo", "x", ["main"], null);
    expect(selectRow).not.toHaveBeenCalled();
  });

  it("clearing the field closes the search", async () => {
    const searchCommits = vi.fn(async () => page(["c1"], null, 1));
    const { result, selectRow } = setup(searchCommits, { row: { commitId: "before" } });

    act(() => result.current.setQuery("x"));
    await waitFor(() => expect(result.current.results?.loading).toBe(false));
    act(() => result.current.setQuery("   "));

    expect(result.current.results).toBeNull();
    expect(selectRow).toHaveBeenLastCalledWith({ commitId: "before" });
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/state/useCommitSearch`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement**

Model it on `useFileHistory`: refs for `generation`, `previousRow`, `selectedRowRef`, plus `refsRef` and a timer ref.
- `setQuery(text)`: set `query` and clear any pending timer. If `text.trim()` is empty, call `close()` but keep the field text (which is empty or whitespace anyway). Otherwise schedule `start(text.trim(), { selectFirst: true })` after `debounceMs`.
- `start(q, { selectFirst })`: on the first start since closed (`results === null`, tracked through a ref), `previousRow = selectedRowRef.current`. Then `gen = ++generation.current`, set the results to `{ query: q, commits: [], nextCursor: null, scanned: 0, loading: true, error: null }`, and run a round from `null`.
- Round (the loop is the part the tests pin):

```ts
let cursor = from;
let found = 0;
try {
  do {
    const page = await client.searchCommits(repoPath, query, refsRef.current, cursor);
    if (gen !== generation.current) return;
    found += page.commits.length;
    cursor = page.nextCursor;
    setResults((prev) => prev && { ...prev, commits: [...prev.commits, ...page.commits],
      nextCursor: page.nextCursor, scanned: page.scanned });
    if (selectFirstPending && page.commits.length > 0) { selectFirstPending = false; selectRow({ commitId: page.commits[0].id }); }
  } while (found < SEARCH_ROUND_SIZE && cursor !== null);
  setResults((prev) => prev && { ...prev, loading: false });
} catch (err) {
  if (gen !== generation.current) return;
  setResults((prev) => prev && { ...prev, loading: false, error: describeError(err) });
}
```

- `loadMore()`: when not loading and `nextCursor !== null`, `gen = ++generation.current`, set `loading: true, error: null`, and run a round from `nextCursor`.
- `retry()`: `start(results.query, { selectFirst: false })`, reusing the saved `previousRow`.
- `close()`: `generation.current++`, clear the timer, call `void client.cancelSearch(repoPath).catch(() => {})` when the results were loading, set the query to `""` and the results to `null`, then restore and clear `previousRow` (as in `useFileHistory.close`).
- Effect keyed on `[refreshGeneration, JSON.stringify(refs)]`: when results are open, `start(results.query, { selectFirst: false })`. Skip the initial mount.
- Unmount effect: clear the timer, and call `cancelSearch` if a search is loading.

- [ ] **Step 4: Run tests**

Run: `cd frontend && pnpm test -- --run src/state/useCommitSearch`
Expected: PASS (8 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/state/useCommitSearch.ts frontend/src/state/useCommitSearch.test.ts
git commit -m "feat(frontend): useCommitSearch hook with debounce, paging rounds and restore"
```

---

### Task 7: `CommitSearchBar` and `CommitSearchResults`

**Files:**
- Create: `frontend/src/lib/searchSyntax.ts`, `frontend/src/components/CommitSearchBar.tsx`, `CommitSearchBar.module.css`, `CommitSearchBar.test.tsx`, `CommitSearchResults.tsx`, `CommitSearchResults.module.css`, `CommitSearchResults.test.tsx`

**Interfaces:**
- Consumes: `CommitSearchResultsState` (Task 6); `ListRow`, `InlineError`; `CommitGraph.module.css` classes `list`, `branchBadge`, `remoteBranchBadge`, `commitSummary`, `author`, `date`, `loadMore` (as `FileHistoryList` uses them); `formatShortDate`.
- Produces (used by Task 8):

```ts
// lib/searchSyntax.ts
export const SEARCH_SYNTAX: { syntax: string; meaning: string }[]; // the 11 rows of the spec table, in order
export const SEARCH_SYNTAX_FOOTER = "Terms combine with AND. Case is ignored except in path:.";

export function CommitSearchBar(props: {
  query: string;
  onQueryChange: (text: string) => void;
  onClose: () => void;
  onFocusResults: () => void;
  focusRequested: boolean;
  onFocusHandled: () => void;
}): JSX.Element;

export function CommitSearchResults(props: {
  results: CommitSearchResultsState;
  selectedRow: SelectedRow;
  onSelectRow: (row: SelectedRow) => void;
  onClose: () => void;
  onLoadMore: () => void;
  onRetry: () => void;
  onRequestSearch: () => void;
  listRef?: React.Ref<HTMLUListElement>;
}): JSX.Element;

export function searchStatusText(results: CommitSearchResultsState): string; // exported from CommitSearchResults.tsx
```

- [ ] **Step 1: Write the failing tests**

`CommitSearchBar.test.tsx`:

```tsx
function renderBar(overrides: Partial<Parameters<typeof CommitSearchBar>[0]> = {}) {
  const props = {
    query: "", onQueryChange: vi.fn(), onClose: vi.fn(), onFocusResults: vi.fn(),
    focusRequested: false, onFocusHandled: vi.fn(), ...overrides,
  };
  render(<CommitSearchBar {...props} />);
  return props;
}

it("reports typing and closes on Escape", () => {
  const props = renderBar({ query: "ada" });
  const input = screen.getByRole("searchbox", { name: "Search commits" });
  fireEvent.change(input, { target: { value: "author:ada" } });
  expect(props.onQueryChange).toHaveBeenCalledWith("author:ada");
  fireEvent.keyDown(input, { key: "Escape" });
  expect(props.onClose).toHaveBeenCalled();
});

it("moves focus to the results on ArrowDown", () => {
  const props = renderBar({ query: "x" });
  fireEvent.keyDown(screen.getByRole("searchbox", { name: "Search commits" }), { key: "ArrowDown" });
  expect(props.onFocusResults).toHaveBeenCalled();
});

it("shows × only with text", () => {
  renderBar({ query: "" });
  expect(screen.queryByRole("button", { name: "Close search" })).toBeNull();
});

it("toggles the syntax help", () => {
  renderBar();
  const help = screen.getByRole("button", { name: "Search syntax" });
  expect(help).toHaveAttribute("aria-expanded", "false");
  fireEvent.click(help);
  expect(help).toHaveAttribute("aria-expanded", "true");
  expect(screen.getByRole("region", { name: "Search syntax" })).toHaveTextContent("author:NAME");
  expect(screen.getByText(SEARCH_SYNTAX_FOOTER)).toBeInTheDocument();
});

it("focuses the field when focus is requested and acknowledges it", () => {
  const props = renderBar({ focusRequested: true });
  expect(screen.getByRole("searchbox", { name: "Search commits" })).toHaveFocus();
  expect(props.onFocusHandled).toHaveBeenCalled();
});
```

`CommitSearchResults.test.tsx`:

```tsx
const base: CommitSearchResultsState = {
  query: "x", commits: [], nextCursor: null, scanned: 0, loading: false, error: null,
};

it.each([
  [{ loading: true, scanned: 0 }, "Searching…"],
  [{ loading: true, scanned: 20_000 }, "Scanned 20,000 commits…"],
  [{ loading: false, commits: [] }, "No commits match this search."],
  [{ commits: [c("a")] }, "1 match"],
  [{ commits: [c("a"), c("b")] }, "2 matches"],
  [{ commits: [c("a"), c("b")], nextCursor: "40000:fp", scanned: 40_000 }, "2 matches in the first 40,000 commits"],
])("status for %o is %s", (partial, text) => {
  expect(searchStatusText({ ...base, ...partial })).toBe(text);
});

it("renders rows, selects on click, and loads more", () => {
  const props = renderResults({ ...base, commits: [c("a"), c("b")], nextCursor: "2:fp", scanned: 2 });
  const list = screen.getByRole("listbox", { name: "Search results" });
  expect(within(list).getAllByRole("option")).toHaveLength(2);
  fireEvent.click(within(list).getByText(/commit b/));
  expect(props.onSelectRow).toHaveBeenCalledWith({ commitId: "b" });
  fireEvent.click(screen.getByRole("button", { name: "Load more" }));
  expect(props.onLoadMore).toHaveBeenCalled();
});

it("hides Load more while loading or when the walk is done", () => {
  renderResults({ ...base, commits: [c("a")], nextCursor: "1:fp", loading: true });
  expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
});

it("navigates with arrows, closes on Escape, and asks for the field on Ctrl/Cmd+F", () => {
  const props = renderResults({ ...base, commits: [c("a"), c("b")] }, { commitId: "a" });
  const list = screen.getByRole("listbox", { name: "Search results" });
  fireEvent.keyDown(list, { key: "ArrowDown" });
  expect(props.onSelectRow).toHaveBeenCalledWith({ commitId: "b" });
  fireEvent.keyDown(list, { key: "f", ctrlKey: true });
  expect(props.onRequestSearch).toHaveBeenCalled();
  fireEvent.keyDown(list, { key: "Escape" });
  expect(props.onClose).toHaveBeenCalled();
});

it("shows the error inline with Retry and Dismiss", () => {
  const props = renderResults({ ...base, error: { message: "author: needs a value." } });
  expect(screen.getByRole("alert")).toHaveTextContent("author: needs a value.");
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(props.onRetry).toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Dismiss error" }));
  expect(props.onClose).toHaveBeenCalled();
  expect(screen.queryByText("No commits match this search.")).toBeNull();
});
```

(`c(id)` builds a `GraphCommit` with summary `commit ${id}`. `renderResults(results, selectedRow = "uncommitted")` renders with `vi.fn()` callbacks and returns them.)

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/CommitSearch`
Expected: FAIL (modules missing).

- [ ] **Step 3: Implement**

- `CommitSearchBar`: a `role="search"` wrapper holding `<input type="search" aria-label="Search commits" placeholder=…>`, the "?" button (`aria-label="Search syntax"`, `aria-expanded`, `aria-controls`), × (`aria-label="Close search"`, rendered only when `query !== ""`), and the help panel (`role="region"`, `aria-label="Search syntax"`, a `<dl>` from `SEARCH_SYNTAX` with `<code>` syntax, then the footer). Esc in the field closes the help if it is open, otherwise calls `onClose`. ↓ calls `onFocusResults`. An effect on `focusRequested` focuses and selects the input, then calls `onFocusHandled()`. CSS uses existing theme tokens only, and the help panel is absolutely positioned under the bar.
- `CommitSearchResults`: the same structure as `FileHistoryList`. The status line (`role="status"`) comes from `searchStatusText`. An `InlineError` (`onRetry`, `onDismiss = onClose`) replaces the status line while `error !== null`. The list is `role="listbox"`, `aria-label="Search results"`, `tabIndex={0}`, `aria-keyshortcuts="ArrowUp ArrowDown Escape Control+F Meta+F"`, with the same key handling as `FileHistoryList` plus `(ctrlKey || metaKey) && key.toLowerCase() === "f"` → `preventDefault(); onRequestSearch()`. Rows mirror `FileHistoryList`'s minus the rename chip. **Load more** renders when `nextCursor !== null && !loading`.

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run src/components/CommitSearch && pnpm lint`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/searchSyntax.ts frontend/src/components/CommitSearchBar.* frontend/src/components/CommitSearchResults.*
git commit -m "feat(frontend): commit search field, syntax help and results list"
```

---

### Task 8: Wire search into the app (graph shortcut, palette, shortcut sheet)

**Files:**
- Modify: `frontend/src/components/CommitGraph.tsx` (`onRequestSearch?: () => void` prop; `handleKeyDown` near line 180), `frontend/src/App.tsx` (hook next to `useFileHistory` at line 92; `diffPaneCommits` at line 105; render block at lines 594–628; the palette `buildCommands` call at line 387), `frontend/src/lib/commands.ts` (new `SearchCommands` interface, 11th `buildCommands` parameter), `frontend/src/lib/shortcuts.ts` ("Commit history" group)
- Test: `frontend/src/components/CommitGraph.test.tsx`, `frontend/src/lib/commands.test.ts`, `frontend/src/App.test.tsx`

**Interfaces:**
- Consumes: Tasks 6–7.
- Produces: `export interface SearchCommands { onSearchCommits(): void }` in `commands.ts`.

- [ ] **Step 1: Write the failing tests**

`CommitGraph.test.tsx` (use the file's existing render helper):

```tsx
it("asks for the search field on Ctrl/Cmd+F while the list has focus", () => {
  const onRequestSearch = vi.fn();
  renderGraph({ onRequestSearch });
  const list = screen.getByRole("listbox", { name: "Commit history" });
  fireEvent.keyDown(list, { key: "f", ctrlKey: true });
  fireEvent.keyDown(list, { key: "F", metaKey: true });
  expect(onRequestSearch).toHaveBeenCalledTimes(2);
});
```

`commands.test.ts`:

```ts
it("offers Search commits when search commands are given", () => {
  const onSearchCommits = vi.fn();
  expect(buildCommands(makeAppState()).some((c) => c.id === "search-commits")).toBe(false);
  const cmd = buildCommands(
    makeAppState(), undefined, undefined, undefined, undefined, undefined, undefined, undefined, undefined, undefined,
    { onSearchCommits },
  ).find((c) => c.id === "search-commits");
  expect(cmd?.label).toBe("Search commits");
  cmd?.run();
  expect(onSearchCommits).toHaveBeenCalled();
});
```

`App.test.tsx` (modeled on "opens a working-tree file's history in place of the graph…"):

```tsx
it("searches from the graph with Ctrl+F, replaces the graph with results, and × restores it", async () => {
  const hit = { ...graphCommitFixture("s1"), summary: "search hit", authorName: "Searchy McAuthor" };
  const searchCommits = vi.fn(async () => ({ commits: [hit], nextCursor: null, scanned: 1 }));
  const cancelSearch = vi.fn(async () => {});
  const client = fakeClient({
    listOpenRepos: async () => ({ entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" }),
    openRepo: async () => {},
    persistOpenRepos: async () => {},
    getStatus: async () => [],
    getCommitGraph: async () => [],
    listBranches: async () => [],
    listStashes: async () => [],
    getGraphBranchSelection: async () => ["main"],
    getCommitFiles: async () => [],
    getCommitMessage: async () => "search hit",
    getCommitSignature: async () => ({ status: "unsigned", format: null, signer: null, key: null, detail: null }),
    searchCommits,
    cancelSearch,
  });

  render(<App client={client} />);
  const graph = await screen.findByRole("listbox", { name: "Commit history" });
  graph.focus();
  fireEvent.keyDown(graph, { key: "f", ctrlKey: true });
  const input = screen.getByRole("searchbox", { name: "Search commits" });
  await waitFor(() => expect(input).toHaveFocus());

  fireEvent.change(input, { target: { value: "author:searchy" } });
  const results = await screen.findByRole("listbox", { name: "Search results" }, { timeout: 2000 });
  expect(searchCommits).toHaveBeenCalledWith("/repos/a", "author:searchy", ["main"], null);
  expect(screen.queryByRole("listbox", { name: "Commit history" })).toBeNull();
  expect(await within(results).findByRole("option", { selected: true })).toHaveTextContent("search hit");

  fireEvent.click(screen.getByRole("button", { name: "Close search" }));
  expect(await screen.findByRole("listbox", { name: "Commit history" })).toBeInTheDocument();
  expect(screen.getByRole("searchbox", { name: "Search commits" })).toHaveValue("");
});
```

Use whatever `GraphCommit` literal style the file already uses (`graphCommitFixture` is a placeholder for an inline literal like the file-history test's `graphCommit`). If `fakeClient` lacks `getGraphBranchSelection`, add it to the overrides as shown.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/CommitGraph src/lib/commands src/App.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement**

- `CommitGraph.handleKeyDown`: before the existing branches (still after the `target !== currentTarget` guard), `if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === "f" && onRequestSearch) { event.preventDefault(); onRequestSearch(); return; }`. Append `Control+F Meta+F` to the list's `aria-keyshortcuts`.
- `commands.ts`: when `searchCommands` is given, push `{ id: "search-commits", label: "Search commits", keywords: ["search", "find", "filter", "author", "grep", "log"], run: searchCommands.onSearchCommits }`. It is available even while an operation is running, because search is read-only.
- `shortcuts.ts`, "Commit history" group, last row: `{ keys: "Ctrl/Cmd+F", description: "Search commits" }`.
- `App.tsx` (`RepoWorkspace`):
  - `const commitSearch = useCommitSearch(client, repoPath, appState.state.refreshGeneration, appState.state.graphBranchSelection, appState.state.selectedRow, appState.selectRow);`
  - `const [searchFocusRequested, setSearchFocusRequested] = useState(false);` `const requestSearchFocus = useCallback(() => { if (fileHistory.history !== null) fileHistory.close(); setSearchFocusRequested(true); }, [fileHistory]);` and `const searchResultsRef = useRef<HTMLUListElement>(null);`.
  - `diffPaneCommits`: `historyEntries?.map((e) => e.commit) ?? commitSearch.results?.commits ?? appState.state.commits` (add `commitSearch.results?.commits` to the memo deps).
  - Render: when `fileHistory.history !== null`, keep `FileHistoryList`. Otherwise render `<CommitSearchBar query={commitSearch.query} onQueryChange={commitSearch.setQuery} onClose={commitSearch.close} onFocusResults={() => searchResultsRef.current?.focus()} focusRequested={searchFocusRequested} onFocusHandled={() => setSearchFocusRequested(false)} />`, then `CommitSearchResults` (with `listRef={searchResultsRef}`, `onRequestSearch={requestSearchFocus}`) when `commitSearch.results !== null`, else `CommitGraph` with `onRequestSearch={requestSearchFocus}`.
  - `buildCommands(…, { onSearchCommits: requestSearchFocus })` as the 11th argument.

- [ ] **Step 4: Run tests and lint**

Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): commit search above the graph, Ctrl/Cmd+F and palette entry"
```

---

### Task 9: Tauri E2E flow

**Files:**
- Create: `e2e/specs/commit-search.spec.ts`

- [ ] **Step 1: Write the spec**

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "search-e2e.txt";
const SUMMARY = "e2e: search target";
const AUTHOR = "Searchy McAuthor <searchy@example.com>";

function git(...args: string[]) {
  execFileSync("git", args, { cwd: E2E_REPO_PATH, stdio: "inherit" });
}

describe("Browsitory commit search", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "find me\n");
    git("add", FILE);
    git("commit", "-m", SUMMARY, `--author=${AUTHOR}`);
  });

  it("finds a commit by author, opens it, and closes back to the graph", async () => {
    const input = await $(`input[aria-label="Search commits"]`);
    await input.waitForExist({ timeout: 10000 });
    // Same native-setter workaround the other specs use for controlled inputs.
    await browser.execute((el, value) => {
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      setter?.call(el, value);
      el.dispatchEvent(new Event("input", { bubbles: true }));
    }, input, "author:searchy");

    const row = await $(`ul[aria-label="Search results"] li*=${SUMMARY}`);
    await row.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), row);
    await (await $(`dd*=Searchy McAuthor`)).waitForExist({ timeout: 10000 });

    const close = await $(`button[aria-label="Close search"]`);
    await browser.execute((el) => (el as HTMLElement).click(), close);
    await (await $(`ul[aria-label="Commit history"]`)).waitForExist({ timeout: 10000 });
    await expect(input).toHaveValue("");
  });
});
```

- [ ] **Step 2: Run the Tauri E2E**

Use the exact sequence in CLAUDE.md's "E2E: Tauri" section (frontend build with `VITE_E2E_REPO_PATH`, then `cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override`, then `cd e2e && pnpm install && pnpm test`), on a headless machine under `xvfb-run`. To iterate, run only this spec: `pnpm test -- --spec ./specs/commit-search.spec.ts`.
Expected: PASS, and the other specs still pass in a full run (this spec adds one commit to the shared fixture repo, like `file-history.spec.ts`).

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/commit-search.spec.ts
git commit -m "test(e2e): commit search by author"
```

---

### Task 10: Docs

**Files:**
- Modify: `CHANGELOG.md` (under `## [Unreleased]`, after the "Interface refresh" subsection), `docs/USER_GUIDE.md` (new section after "## File history"), `docs/ARCHITECTURE.md` (new "## Commit search" after "## Commit signing")

- [ ] **Step 1: CHANGELOG**

```markdown
### Commit search

- A search field above the commit graph (**Ctrl/Cmd+F** on the commit list, or "Search commits"
  in the palette) finds commits by message or SHA, `author:`, `committer:`, `message:`,
  `commit:`, `path:`, `contents:` (added or removed lines), `after:`/`before:` dates and
  `branch:`. Terms combine with AND; quote text with spaces or a colon.
- Results replace the graph, load 100 at a time with **Load more**, and show how many commits
  were scanned; Esc or × returns to the graph and your previous selection.
```

- [ ] **Step 2: USER_GUIDE**

A `## Searching commits` section: how to open it (field above the graph, Ctrl/Cmd+F on the commit list, palette), a two-column table generated by hand from `SEARCH_SYNTAX` (same rows as the spec's table), then these notes:
- Terms combine with AND, and case is ignored except in `path:`.
- Quote text with spaces or a colon (`"fix: crash"`).
- Dates are whole days in the commit's own timezone, inclusive.
- `path:` and `contents:` look at what a commit changed compared with its first parent, so a merge counts as the changes it brought in.
- Without `branch:`, the search covers the branches the graph shows.
- Results load 100 at a time, and "Scanned N commits…" shows progress on large histories.
- Selecting a result shows its diff as usual.
- Esc or × goes back to the graph and the previous selection.
- File history opened from a result returns to the results when closed.

Also add "Ctrl/Cmd+F" to the "Command palette" section's last sentence if that section lists shortcuts (it does not today; leave it if not).

- [ ] **Step 3: ARCHITECTURE**

```markdown
## Commit search

- `git-core::search` parses the query once (`parse_query`) and pages a `TOPOLOGICAL | TIME`
  revwalk (`search_commits`): cheap filters first, first-parent diff filters (`path:`,
  `contents:`) last. A page ends at 100 matches or after 20,000 scanned commits.
- The cursor is `<offset>:<fingerprint of the start oids>`; resuming re-walks and skips
  `offset`, and any moved start point is `StaleCursor` instead of a duplicate or a gap.
- One in-flight search per repo: `WorkerHandle::search_commits` bumps a shared
  `SearchGeneration` (`Arc<AtomicU64>`) before queueing, and the running walk polls it, so a
  newer search or `cancel_search` stops an older one without going through the command queue
  (same reasoning as `CancelRegistry`).
```

- [ ] **Step 4: Full verification and commit**

Run:
```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd frontend && pnpm lint && pnpm test -- --run && pnpm build && cd ..
cd extension && pnpm test -- --run && pnpm run compile && cd ..
```
Expected: all PASS.

```bash
git add CHANGELOG.md docs/USER_GUIDE.md docs/ARCHITECTURE.md
git commit -m "docs: commit search in user guide, changelog and architecture"
```
