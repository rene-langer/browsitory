# Task 1.A.01: `git-core::log`

## Goal

Add `git_core::log::log(repo, limit) -> Result<Vec<CommitInfo>, LogError>`, walking commit
history from `HEAD` in topological+time order, capped at `limit` commits. This is the data
source for Phase 1's commit-history list — a simple linear list for the current branch, no
graph rendering (that's Phase 2). No pagination this phase: `limit` is a fixed cap the caller
chooses; a longer history just doesn't show older commits yet.

## Depends on

None — new module, only touches `crates/git-core`.

## Interfaces produced

`crates/git-core/src/log.rs`:
```rust
use git2::{Repository, Sort};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LogError {
    #[error("failed to read commit log: {0}")]
    Read(#[from] git2::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub id: String,         // full 40-char hex OID
    pub short_id: String,   // first 7 hex chars of `id` — fixed length, not libgit2's
                             // variable unique-prefix short_id() (simpler, deterministic)
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,     // Unix seconds, UTC — frontend formats for display
}

pub fn log(repo: &Repository, limit: usize) -> Result<Vec<CommitInfo>, LogError> {
    // ...
}
```

`crates/git-core/src/lib.rs` gains `pub mod log;`.

## Implementation notes

- `repo.revwalk()?` then `revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?`, then
  `revwalk.push_head()`. On a repo with no commits yet, `push_head()` returns `Err` (unborn
  branch) — treat that as "empty log", not a propagated error: `return Ok(Vec::new())` in that
  case rather than `?`-propagating it.
- Iterate with `revwalk.take(limit)`; each item is `Result<Oid, git2::Error>` — propagate errors
  with `?` inside the loop (via a `for oid in revwalk.take(limit) { let oid = oid?; ... }`).
- For each `Oid`, `repo.find_commit(oid)?` gives the `Commit`. Build `short_id` as
  `oid.to_string()[..7].to_string()` (the full hex string is always 40 chars, so this never
  panics).
- `commit.author()` returns `Signature`, whose `.name()`/`.email()` return `Result<&str, Error>`
  (see `CLAUDE.md`'s git2 gotchas) — use `.ok().unwrap_or_default()` for each, don't propagate
  a signature-decoding failure as a whole-log failure.
- `commit.summary()` returns `Result<Option<&str>, Error>` — use
  `.ok().flatten().unwrap_or_default()`.
- `commit.time()` returns `Time` (not `Result`); `.seconds()` gives the `i64` Unix timestamp.

## TDD requirement

`crates/git-core/tests/log.rs` (new file, `mod common;` + the existing
`common::{init_repo, commit_all, write_file}` helpers — do not duplicate them):

- `log_returns_an_empty_vec_for_a_repository_with_no_commits`: `init_repo()` with no commits,
  assert `git_core::log::log(&repo, 10).unwrap().is_empty()`.
- `log_returns_commits_most_recent_first`: create two commits via `commit_all` with distinct
  messages ("first commit", "second commit"), assert `log(&repo, 10)` returns exactly 2 entries
  with `entries[0].summary == "second commit"` and `entries[1].summary == "first commit"`
  (`Sort::TIME` within a single linear branch puts HEAD first).
  Then assert `entries[0].short_id.len() == 7` and that it's a prefix of `entries[0].id`
  (`entries[0].id.starts_with(&entries[0].short_id)`), and that `entries[0].author_name`
  equals `"Test User"` (matching `init_repo()`'s configured identity).
- `log_respects_the_limit`: create 3 commits, assert `log(&repo, 2).unwrap().len() == 2` and
  that the two returned are the two most recent (by summary).

Write these three tests first, run `cargo test -p git-core --test log`, confirm they fail to
compile (`log` module doesn't exist yet), then implement `log.rs` per the signature above and
re-run until green.

## Acceptance criteria

- [ ] `cargo test -p git-core --test log` passes (all 3 tests).
- [ ] `cargo test --workspace` still passes (no regression in `status`/`repo` tests).
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo fmt --all -- --check` clean.
- [ ] Commit: `git add crates/git-core/src/lib.rs crates/git-core/src/log.rs crates/git-core/tests/log.rs && git commit -m "feat(git-core): add log() for commit history"`.

## Out of scope

Pagination/cursor-based loading for repos with more commits than `limit`. Multi-branch or
graph-line rendering data (parent-count/branch-topology info) — Phase 2. Filtering (by author,
path, date range).
