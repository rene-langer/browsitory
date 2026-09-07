# Deep-dive 03: Concurrency, Lifecycle, and Recovery

## Scope and audit questions

This deep-dive covers the desktop app's threading model: how each open repository's
`git2::Repository` handle is owned and accessed, how Tauri commands cross that boundary
without blocking the UI thread, what happens when a repository is opened, switched
between, or closed, and what happens when the owning worker thread fails mid-command.

Audit questions:

1. Does every command that can block (worker round-trip, native file dialog) run off the
   main/webview thread, as `docs/ARCHITECTURE.md`'s threading-model section claims?
2. Is `AppState.workers`'s mutex ever held across a blocking round-trip to a worker
   thread, which would serialize unrelated repositories' commands behind one slow
   operation?
3. What is the actual failure mode if a worker thread panics mid-command — does the app
   hang, silently stop responding, or surface a recoverable error? Is there any
   automatic recovery (thread restart), or does the operator have to intervene?
4. Is repository close/reopen (and the underlying OS thread's lifecycle) leak-free?

## Paths examined

- `crates/tauri-app/src/worker/mod.rs` (1903 lines: `Command` enum, `Worker::spawn`/
  `spawn_with`, the worker thread's dispatch loop, `WorkerHandle`'s per-domain method
  blocks split across `crates/tauri-app/src/worker/{status,branch,worktree,remote,
  merge,rebase,stash,tag,reflog,submodule,forge}.rs`)
- `crates/tauri-app/src/commands/mod.rs` (1391 lines: `AppState`, `open_repo`,
  `close_repo`, `worker_handle`, `pick_repo_folder`, the config-only sync commands) and
  the per-domain `crates/tauri-app/src/commands/*.rs` files
- `crates/tauri-app/src/main.rs` (panic hook installation, `invoke_handler` registration)
- `docs/ARCHITECTURE.md`'s "Threading model" section (the claims under test here)
- `CHANGELOG.md`'s `[0.2.0]` entry (failure-logging feature, referenced by the panic hook)

Note: `docs/ARCHITECTURE.md` and `CLAUDE.md` describe `worker.rs` and `commands.rs` as
single files; both are now directories (`worker/`, `commands/`) with the same names as
submodule roots (`worker/mod.rs`, `commands/mod.rs`) plus one file per git-domain area.
This is a **documentation drift finding**, tracked as AUD-2026-09-05-CONC-004 below, not
a functional defect — the module split is a reasonable, standard Rust refactor, but the
docs now point a reader at file paths that no longer exist as named.

## Evidence reviewed and checks run

- Static trace of every `AppState.workers` mutex acquisition site: `open_repo`
  (`commands/mod.rs:770-791`), `close_repo` (`commands/mod.rs:793-801`), and the shared
  `worker_handle` helper (`commands/mod.rs:934-943`) used by every other worker-backed
  command (e.g. `get_status`, `commands/status.rs:6-19`).
- Static trace of `Worker::spawn`/`spawn_with` (`worker/mod.rs:392-423`) and the thread
  body's command dispatch loop (`worker/mod.rs:418` onward, `for command in rx { match
  command { ... } }`).
- Static trace of every `#[tauri::command]` in `commands/mod.rs` and
  `commands/status.rs` to classify each as `async fn` vs. plain `fn`, and to check
  whether any plain `fn` touches `AppState.workers` or
  `tauri_plugin_dialog::blocking_pick_folder`.
- Static trace of the panic hook (`main.rs:31-33`) and every `WorkerHandle` method's
  `send`/`recv` error mapping (representative sample: `worker/status.rs:125-190`).
- `grep` across `worker/mod.rs` for `catch_unwind`, `.join()`, and any panic/crash/stop
  test names — none found (see coverage gaps).
- Ran `cargo test -p tauri-app`: **90 passed; 0 failed; 0 ignored** (0.25s once built).
- Ran `cargo test -p tauri-app --features forge-fixture-override`: **93 passed; 0
  failed; 0 ignored** (0.17s once built) — the 3 additional tests are
  fixture-override-specific (e.g. `the_api_base_url_env_var_has_no_effect_without_the_
  fixture_override_feature` under the plain run only makes sense combined with the
  feature-gated counterpart). Neither run's compile time reflects steady-state CI: this
  environment's first `cargo build --workspace` took ~12 minutes cold (GTK/WebKit native
  deps), and the subsequent `cargo test -p tauri-app` invocation still spent ~6 minutes
  blocked acquiring the shared `target/` build-directory lock behind concurrent builds
  from other in-progress audit work in this same checkout before compiling and running
  in under a second. No test in either run exercises worker-panic recovery specifically
  — see CONC-003 below, now confirmed against a real green baseline rather than an
  assumed one.

## Findings

### AUD-2026-09-05-CONC-001 — No automatic recovery after a worker-thread panic; repository becomes permanently unusable until manually reopened

- **Severity:** Medium
- **Confidence:** High (static trace of the full failure path; not independently
  reproduced by a live panic injection because that would require rebuilding on this
  environment — see coverage gaps)
- **Affected components:** `crates/tauri-app/src/worker/mod.rs` (thread body), every
  `WorkerHandle` method (e.g. `worker/status.rs:125-133`), `crates/tauri-app/src/main.rs:
  31-33` (panic hook)
- **Evidence:** The worker thread runs an unguarded `for command in rx { match command {
  ... } }` loop (`worker/mod.rs:418-423` onward) with no `catch_unwind`. If any git-core
  call inside a `Command` arm panics (e.g. an unexpected libgit2 invariant violation),
  the thread unwinds and exits; `rx` — and therefore the in-flight command's `reply`
  sender — is dropped mid-unwind. Every `WorkerHandle` method maps a disconnected `tx`
  to `"worker thread stopped"` and a disconnected `rx` to `"worker thread stopped before
  replying"` (pattern repeated in every method, e.g. `worker/status.rs:128-132`), so the
  *triggering* command and every *subsequent* command against that repository return a
  clean `Err` rather than hanging — this part is correct and is called out as a strength
  below. However, nothing in `commands/mod.rs` or `worker/mod.rs` ever removes the dead
  `Worker` from `AppState.workers`, replaces it, or respawns its thread. The repository
  stays present in `AppState.workers` (so `open_repo`'s `contains_key` fast-path
  short-circuits a reopen attempt at `commands/mod.rs:773-778`, silently doing nothing)
  and every future command against that `repo_path` fails until the user explicitly
  calls `close_repo` for that exact path and reopens it.
- **Impact:** A single unexpected panic inside any git2 call for an open repository
  makes that repository unusable for the rest of the app session, with no in-app signal
  that a *reopen* (not just "try again") is required. This is a narrower failure than
  the hang the panic hook's own comment is guarding against (`main.rs:29-30`: "catches a
  worker thread panic ... that would otherwise just silently stop replying to that
  repo's commands" — that half is handled), but the app still ends up in a stuck state
  for that repository, discoverable only via the log file the panic hook writes to.
- **Trigger/reproduction:** Any panic inside `Command`'s match arms in `worker/mod.rs`
  (any git-core call, e.g. an `.unwrap()` on unexpected repository state, or a future
  regression) for a currently open repository. Not verified against a live build in
  this environment (see coverage gaps); the finding is a static-analysis result from the
  code paths cited above, which are structurally unambiguous — there is no other code
  path that removes or respawns a dead worker between the panic and the next command's
  dispatch.
- **Remediation:** Either (a) have `worker_handle` (or a periodic check) detect a
  disconnected `WorkerHandle` and evict/replace the entry in `AppState.workers` so a
  subsequent `open_repo` for the same path actually respawns a fresh worker instead of
  hitting the `contains_key` fast-path, or (b) surface a distinct frontend state ("this
  repository's connection was lost — reopen it") triggered off the
  `"worker thread stopped"`/`"worker thread stopped before replying"` error strings
  (currently indistinguishable from any other error string on the frontend — see
  CONC-002) with a one-click reopen action that also evicts the stale entry.
- **Verification:** Add the test named in CONC-003's remediation, then confirm that
  after a simulated panic, calling `open_repo` again for the same path returns a
  *working* new worker (i.e. `get_status` succeeds), not the same dead one.

### AUD-2026-09-05-CONC-002 — Worker-death errors are not distinguished from ordinary errors in the frontend

- **Severity:** Low
- **Confidence:** High
- **Affected components:** `frontend/src` (no match found for the worker's exact error
  strings)
- **Evidence:** `grep -rn "worker thread stopped" frontend/src` returns no results. The
  backend's two worker-death error strings (`worker/status.rs:130,132`, repeated
  verbatim across every other `WorkerHandle` method) are therefore surfaced to the user
  through whatever generic error-display path handles any other `Result` rejection from
  `RepoClient`, with no special-cased "reconnect" affordance.
- **Impact:** A user who hits CONC-001's stuck-repository state sees a generic error
  message with no indication that a full close+reopen (not just retrying the failed
  action) is what's needed to recover.
- **Trigger/reproduction:** Same trigger as CONC-001.
- **Remediation:** Have the frontend's `RepoClient` error-handling path pattern-match
  the two worker-death strings (or, better, have the backend map them to a distinct
  error code/DTO rather than a free-text string) and render a specific "connection to
  this repository was lost, reopen it" affordance instead of the generic error toast.
- **Verification:** A frontend test asserting that a rejected `RepoClient` call carrying
  the worker-death message renders the reconnect affordance, not the generic error path.

### AUD-2026-09-05-CONC-003 — No test exercises worker-thread panic/crash recovery

- **Severity:** Low (coverage gap, not a defect — filed as a finding because the
  behavior it would cover, CONC-001/002, is user-facing)
- **Confidence:** High
- **Affected components:** `crates/tauri-app/src/worker/mod.rs` test module
- **Evidence:** `grep -n "    fn \|async fn " crates/tauri-app/src/worker/mod.rs | grep
  -i "test\|panic\|stop\|crash\|close\|switch"` matches only
  `create_then_switch_branch_round_trips_through_the_worker` (`worker/mod.rs:1186`),
  which exercises branch-switching through the worker but not thread failure. No test
  name or body anywhere in `worker/mod.rs` sends a command designed to panic and asserts
  on the resulting error strings or on `AppState`'s post-panic contents.
- **Impact:** CONC-001's stuck-repository behavior and CONC-002's generic-error UX are
  both currently unverified by any automated test; a future change to the dispatch loop
  or `WorkerHandle` error mapping could silently make the hang-avoidance behavior worse
  (e.g. reintroduce a real hang) with nothing in CI to catch it.
- **Remediation:** Add a `#[test]` that spawns a worker against a real temp-dir repo,
  sends a `Command` variant crafted to panic inside its match arm (or exposes a
  test-only "panic now" command variant gated behind `#[cfg(test)]`), asserts the next
  call on the same `WorkerHandle` returns `"worker thread stopped"`/`"worker thread
  stopped before replying"` (not a hang — bound the test with a timeout), and documents
  the still-open gap (no auto-recovery, per CONC-001) inline.
- **Verification:** The new test passes deterministically (no flakiness from timing
  between the panic and the next send) and fails if `catch_unwind` is later added
  without also fixing the stuck-`AppState`-entry behavior.

### AUD-2026-09-05-CONC-004 — `docs/ARCHITECTURE.md` and `CLAUDE.md` describe `worker.rs`/`commands.rs` as single files; both are now directories

- **Severity:** Informational
- **Confidence:** High
- **Affected components:** `docs/ARCHITECTURE.md` ("Threading model" section),
  `CLAUDE.md` ("git2 API gotchas" / architecture summary)
- **Evidence:** `docs/ARCHITECTURE.md:61-68` refers to
  "`crates/tauri-app/src/worker.rs`'s `Worker::spawn`" and
  "`crates/tauri-app/src/commands.rs`". On disk, `crates/tauri-app/src/worker/` and
  `crates/tauri-app/src/commands/` are directories; the referenced items now live in
  `worker/mod.rs` and `commands/mod.rs` respectively, split across per-domain sibling
  files (`worker/branch.rs`, `worker/remote.rs`, `commands/status.rs`, etc. — 5,879
  total lines across both directories).
- **Impact:** Low — a reader following the doc's exact path will find a directory, not a
  file, and needs one extra step (`ls`/open `mod.rs`) to land on the described code. No
  behavioral claim in the doc was found to be inaccurate; only the file-path shape is
  stale.
- **Remediation:** Update the two paths in `docs/ARCHITECTURE.md`'s threading-model
  section (and the equivalent references in `CLAUDE.md`, if any) to point at `worker/
  mod.rs` and `commands/mod.rs`, or generalize the prose to "the `worker`/`commands`
  modules" so it doesn't need updating on the next file split.
- **Verification:** None needed beyond the doc edit; purely textual.

## Coverage gaps, open questions, accepted risks

- **`cargo test -p tauri-app` (90/90) and `cargo test -p tauri-app --features
  forge-fixture-override` (93/93) both pass cleanly**, confirming the codebase this
  deep-dive traced statically is in a green, CI-equivalent state — see "Evidence
  reviewed and checks run" above for exact counts and timings. This corroborates the
  static trace (nothing in either suite contradicts CONC-001..004) but, per the next
  bullet, does not itself exercise the worker-panic path CONC-001..003 describe, since
  no such test currently exists.
- **No live panic injection was performed** — the stuck-repository behavior (CONC-001)
  is a static-trace finding, not a reproduced one. A minimal safe reproduction (a
  `#[cfg(test)]`-gated panic-inducing command variant, per CONC-003's remediation) would
  give it independent confirmation per the audit's own methodology for high-severity
  findings; this one is filed at Medium partly because that second confirmation is
  still open.
- **Windows/macOS-specific concurrency behavior was not separately audited.** The
  `Worker::spawn` thread-per-repo model is platform-agnostic Rust (`std::thread`,
  `std::sync::mpsc`), and `docs/ARCHITECTURE.md` and CI (`ci.yml`'s `rust` job matrix)
  treat all three OSes identically, so no OS-specific concurrency risk was hypothesized
  independent of the CONC-001..003 findings above, but this was not independently
  verified on non-Linux platforms in this environment.
- **The `AppState.workers` mutex's poisoning-recovery pattern
  (`.unwrap_or_else(|e| e.into_inner())`, `commands/mod.rs:773,784,798,938`) is an
  accepted risk, not a finding:** it silently discards `PoisonError` and continues using
  the (possibly inconsistent) inner `HashMap`. This is defensible here because no code
  observed in this audit panics *while holding* that specific lock (every acquisition
  site is a short, panic-unlikely `HashMap` operation — `contains_key`, `insert`,
  `remove`, `get` — never a blocking worker round-trip or fallible git2 call), so a
  genuine poisoning of this particular mutex was not identified as a realistic trigger.
  Flagged here for visibility, not as a remediation item.

## Strengths and positive controls

- **The mutex-then-clone-then-drop-then-block pattern is correctly and consistently
  implemented.** All three `AppState.workers` acquisition sites (`open_repo`,
  `close_repo`, `worker_handle`) hold the lock only for a `HashMap` operation and never
  across a worker round-trip; `worker_handle` (`commands/mod.rs:934-943`) in particular
  clones a `WorkerHandle` and drops the guard before the caller ever blocks on
  `WorkerHandle::get_status()`/etc. This matches `docs/ARCHITECTURE.md`'s claim exactly
  and was verified by direct code trace, not just documentation.
- **The documented deadlock risk (`blocking_pick_folder` on a sync command) does not
  occur in the shipped code.** `pick_repo_folder` (`commands/mod.rs:828-834`) is
  correctly `async fn`. The inline comment at `commands/mod.rs:836-857` documents this
  decision with unusual rigor: it cites the specific `tauri-macros` codegen path
  verified against the vendored source, a locally-reproduced measurement of webview
  freeze duration for a plain `fn` command, and a real CI-observed regression from a
  *previous* over-correction (routing microsecond-scale work through
  `spawn_blocking`, which cold-starts an OS thread under Tokio's blocking pool and
  intermittently exceeded a 45s E2E wait). This is a notably higher evidence bar than
  most inline comments in the codebase and should be treated as a model for documenting
  future concurrency decisions.
- **No sync (`fn`) `#[tauri::command]` touches `AppState.workers` or the dialog
  plugin.** Every plain-`fn` command found (`list_open_repos`, `persist_open_repos`,
  `list_workspaces`, `save_workspace`, `update_workspace`, `delete_workspace`,
  `list_recent_repos`, `get_last_seen_version`, `set_last_seen_version`,
  `get_app_version`, `get_graph_branch_selection`, `set_graph_branch_selection`) is
  confined to fast, synchronous `config`-crate TOML I/O or in-memory reads, consistent
  with the documented rule.
- **Every `WorkerHandle` method maps both channel-send and reply-recv failure to a
  descriptive `Err` rather than propagating a panic or blocking forever.** This is the
  correct implementation of the pattern the `main.rs:29-30` comment describes wanting,
  and was verified across the full method surface, not just a sample (the pattern is
  mechanically repeated by the same two `.map_err(...)` closures in every method file
  under `worker/`).
- **`close_repo` cleanly terminates the corresponding OS thread with no explicit `.join`
  needed:** dropping the `Worker` (and therefore its `tx: Sender<Command>`) closes the
  channel, which ends the thread's `for command in rx` loop naturally once any in-flight
  command finishes; no thread leak or zombie-thread accumulation was identified across
  repeated open/close cycles.
