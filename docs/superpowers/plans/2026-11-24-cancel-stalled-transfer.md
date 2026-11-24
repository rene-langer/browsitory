# Cancel a stalled transfer (#1064) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this
> plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** a fetch, pull or push that stalls after connecting no longer holds the worker for good:
libgit2's server timeout ends it, and Cancel gives up on a stalled fetch or pull.

**Spec:** `docs/superpowers/specs/2026-11-24-cancel-stalled-transfer-design.md`.

**Branch:** `b62-cancel-stalled-transfer` from `origin/main`. No new dependency, no frontend
change.

## Task 1: tests (one commit, red run quoted)

The commit adds only what the tests need to compile: `TransferReporter::writing` with its default
`true`, never called; `Phase::Writing` and `SharedPhase::writing` (answers `true`, changes
nothing); `give_up` taking `cancelled_at: Option<Instant>` instead of the cancel closure, with
the old rule (the existing unit tests move to that signature, same expectations).

- [ ] git-core `tests/remote.rs`, reporter `Gate { connected, writing, calls: Vec<&str> }`:
  - `a_fetch_asks_the_reporter_before_writing_and_fetches`: `calls == ["connected", "writing"]`,
    tracking ref at the remote tip. Red: `["connected"]`.
  - `a_fetch_that_the_reporter_stops_before_writing_writes_nothing`: `Err(Cancelled)`, tracking
    ref unchanged, `FETCH_HEAD` unchanged. Red: `Ok`.
  - `a_push_asks_the_reporter_before_writing` / `a_push_that_the_reporter_stops_before_writing_
    sends_and_writes_nothing` (push_branch with set_upstream): `Err(Cancelled)`, bare tip
    unchanged, no `published` branch, no tracking ref, upstream unchanged. Red: `Ok`.
  - Characterization (green before and after): `a_fetch_follows_tags_and_writes_fetch_head`,
    `fetch_with_unspecified_prune_follows_the_remotes_prune_config_first`.
- [ ] git-core `tests/server_timeout.rs`, one test: `configure_libgit2` →
  `get_server_timeout_in_milliseconds() == 120_000` (red: 0); then set 300 ms and fetch from a
  stalling `git://` server: `Err` within 20 s, `TransferFailed` (green: libgit2's behaviour).
- [ ] repo-service `worker/tests/remote.rs`: `StalledServer` (`git://`; answers the request with
  a one-ref advertisement, signals once the client sends more, then never answers; drop closes
  its connections). `cancel_while_stalled` runs `get_status` on a thread and waits at most 15 s.
  Tests: `a_cancel_stops_a_fetch_that_stalled_after_connecting`,
  `a_cancel_stops_a_fetch_all_whose_remote_stalled_after_connecting`,
  `a_cancel_stops_a_pull_whose_fetch_stalled_after_connecting`. Red: the status read times out.
- [ ] `transfer_thread::tests`: `a_transfer_writes_only_if_the_worker_has_not_given_up`,
  `the_worker_gives_up_on_a_stalled_transfer_once_a_cancel_went_unanswered`,
  `the_worker_never_gives_up_on_a_transfer_that_writes`, and
  `a_helpers_reporter_writes_only_if_not_abandoned`.
- [ ] `Test change:` commit: `the_worker_never_gives_up_on_a_transfer_that_connected` encodes
  #547's rule that this issue changes; it becomes
  `the_worker_gives_up_on_a_connected_transfer_only_after_the_grace`.

## Task 2: implementation (one commit)

- [ ] `repo.rs`: `SERVER_TIMEOUT`, set in `configure_libgit2`.
- [ ] `remote.rs`: `fetch_with` split (`download`, `disconnect`, `writing()`/`cancel()`,
  `update_tips`, `prune` per `effective_prune`, shared with `list_remotes`); `push_refs` asks
  `writing()` after `connected()`.
- [ ] `transfer_thread.rs`: `STALLED_CANCEL_GRACE`, `cancelled_at` in `run`, `give_up`'s
  `Transferring` arm, `SharedPhase::writing`, `HelperReporter::writing`, module docs.
- [ ] `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p
  git-core -p repo-service`.

## Task 3: docs (one commit)

- [ ] `docs/ARCHITECTURE.md`: the "No transfer holds the worker past a cancel" bullet (stalls,
  server timeout, limits) and `configure_libgit2`'s settings.
- [ ] `docs/guide/branches-and-remotes.md`: the "Cancelling a fetch, pull or push" box.
- [ ] `changelog.d/1064-cancel-stalled-transfer.md`.
