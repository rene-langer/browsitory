# Robustness Bundle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close seven follow-ups in four PRs:

- PR 1 (`test/robustness`): **#179** process-group/job tests that can fail; **#182** E2E session
  start, stale elements, merge race.
- PR 2 (`fix/worker-call-gate`): **#181** one blocking thread per repository.
- PR 3 (`feat/hook-status-parity`): **#185** reported, cancellable post-checkout/post-merge;
  **#183** hooks on clean picks, `MERGE_MSG`, `MergeChanged`, landed-commit cleanup.
- PR 4 (`fix/folder-file-swaps`): **#186** stash with a file that replaced a folder; **#187**
  unstage with conflicts below a pair root.

**Architecture:**

- `git-core`: `process.rs` (tests), `commit.rs` (`run_commit_hooks`, `run_message_hook`,
  `CommitHookSet`, `commit_with`), `pick.rs` (`step`, `run`, `apply`, `finish_paused`),
  `stash.rs`, `stage.rs`.
- `repo-service::worker`: `hooks.rs` (`HookSource`), `branch.rs`, `history.rs`, `merge.rs`,
  `remote.rs`, `status.rs`, `mod.rs` (dispatch).
- `tauri-app`: `commands/mod.rs` (`AppState`, `RepoWorker`, `HookProgressDto`), command files.
- `vscode-sidecar`: `dispatch.rs` (relays, DTO).
- `frontend`: `ipc/RepoClient.ts` types, `state/useHookActions.ts`, `components/HookRunStrip.tsx`,
  `TransferPanel.tsx`, `App.tsx`, `lib/hookFailure.ts`.
- `e2e/`: `wdio.conf.ts`, `support/dom.ts`, `support/git.ts`, specs.

**Tech Stack:** Rust (`git2` 0.21 / libgit2 1.9, `libc` 0.2, tokio via `tauri::async_runtime`),
TypeScript (React, Vitest, WebdriverIO).

**Spec:** `docs/superpowers/specs/2026-11-01-robustness-bundle-design.md`. This plan implements
the **provisional** choice of each decision D1–D13. "Decision hooks" at the end lists what changes
for the other options.

---

## Global Constraints

- **Branches:** each PR from `main`; open each only after its gate task (TZ, WZ, HZ, DZ).
- **No new dependency.** `tauri::async_runtime::Mutex` is tokio's, re-exported by tauri.
- **Tests use real temp-dir repos** (`tempfile` + `git2::Repository::init`); git-core integration
  tests start with `mod common;` / `test_support::hermetic_git!();`; git CLI twins as in
  `tests/stash.rs` l. 814. Frontend tests mock `RepoClient` only.
- **Platform gating:** marker/pty/chmod tests `#[cfg(unix)]`; chmod tests skip when
  `libc::geteuid() == 0`; Windows controls `#[cfg(windows)]` (CI `windows-latest` only).
- **Never** touch `~/.local/share/com.browsitory.app`, the real config dir or `~/.gnupg`; E2E
  uses its own `/tmp/browsitory-e2e-*` paths; probes go under `target/` and are deleted.
- **New user-visible strings** (exact): `MergeChanged`, `ConflictsBelow`, `UntrackedInTheWay`,
  `FileReplacedFolder` messages (spec §6–8); "Hook status" strip label text comes from
  `runningHookLabel`.
- **CHANGELOG:** every push touching `crates/`, `frontend/src/` or `e2e/` needs a `CHANGELOG.md`
  change (pre-push hook). Each PR's first task adds a stub section at the top of
  `## [Unreleased]`; its Z task writes the final text.
- **Commits:** one per task unless noted, `<area>: <what> (#NNN)`, ending with the session's
  `Co-Authored-By`/`Claude-Session` trailers.

## Pre-flight

- [ ] `git status` clean, rebased on `origin/main`; if `main` moved past 25040598, re-check the
  line references below.
- [ ] Baseline: `cargo test --workspace`, `cd frontend && pnpm test -- --run`. Record counts.
- [ ] Pre-push hook linked (CLAUDE.md "Git hooks").

Reconciled with the code at 25040598 on 2026-11-01:

- `crates/git-core/src/process.rs`: `DRAIN_GRACE` l. 46, `spawn_retrying` l. 154, `kill_tree`
  l. 180, `detach` l. 207, `spawn_suspended_in_job` l. 239, `wait_bounded` l. 275, `run_captured`
  l. 304, `run_captured_split` l. 386; tests: `sh` l. 1102, grandchild pipe l. 1160, split timeout
  l. 1240, session probe l. 1307–1352, `/dev/tty` l. 1356, Windows `windows_sh`/`marker_path`
  l. 1372–1384, Windows tests l. 1386–1511.
- `crates/tauri-app/src/commands/mod.rs`: `HookProgressDto` l. 833, `emit_hook_events` l. 855,
  `AppState` l. 1677, `close_repo` ~l. 1696, `RepoWorker` l. 1938, `repo_worker` l. 1961,
  `off_runtime` l. 1970, tests l. 1990–2104.
- `e2e/wdio.conf.ts`: `beforeSession` l. 264, `waitForPort(4444)` l. 292, `afterSession` l. 302.
  `e2e/specs/merge.spec.ts` l. 112, l. 196, l. 207, l. 289; `line-staging.spec.ts` l. 47–60.
- `crates/git-core/src/commit.rs`: `run_commit_hooks` l. 272, `run_post_commit` l. 340,
  `message_source` l. 361, `CommitHookSet` l. 401, `commit_with` l. 424 (`cleanup_state` l. 511).
- `crates/git-core/src/pick.rs`: `apply` l. 230, `step` l. 296, `start` l. 418, `run` l. 483,
  `continue_sequence` l. 544, `finish_paused` l. 605.
- `crates/repo-service/src/worker/`: `hooks.rs` (`HookEvent` l. 12, `ChannelHookReporter` l. 17,
  `run_post_hook` l. 85, `run_post_hook_reported` l. 97, `run_post_checkout` l. 117);
  `branch.rs` `switch` l. 18, handle l. 70; `history.rs` `pick_many` l. 76, `continue_sequence`
  l. 92, `cherry_pick` l. 195, `revert` l. 205, `checkout` l. 224; `merge.rs` `CleanMerge` l. 17,
  `start` l. 46, handle l. 154; `remote.rs` `pull` l. 302 (post-merge l. 337); `status.rs`
  `commit` l. 301; `mod.rs` dispatch: Commit l. 1087, SwitchBranch l. 1125, StartMerge l. 1261,
  CherryPick l. 1320, CherryPickMany l. 1330, RevertMany l. 1336, CheckoutCommit l. 1371,
  PullCurrentUpstream l. 1457.
- `crates/vscode-sidecar/src/dispatch.rs`: `hook_progress_dto` l. 1549, `spawn_hook_relay` l. 1564.
- `frontend/src`: `ipc/RepoClient.ts` `HookProgress` l. 210, `TransferProgress` l. 276;
  `state/useHookActions.ts`; `components/HookStatus.tsx`, `TransferPanel.tsx` l. 33,
  `OperationStatusStrip.tsx`; `App.tsx` l. 544 (strip), l. 682 (transfer overlay), l. 1094
  (`commitHooks`); `lib/hookFailure.ts`.
- `crates/git-core/src/stash.rs`: `save_stash` l. 130, `save_path_limited_stash` l. 208,
  `FolderIsNowAFile` check l. 292, revert l. 390–437, `apply_stash` l. 508.
- `crates/git-core/src/stage.rs`: `StageError` l. 19, `unstage_file` l. 390, `pair_root` l. 427.

## Pre-flight notes

(fill in: baseline counts, whether `main` moved, decisions confirmed, T7 measurements, D2 results)

- 2026-11-01: the user accepted every recommended option (D1-A … D13-A).
- Part T (PR 1): T1 and T2 landed in one commit (the Unix control sits next to the marker
  tests). The Unix control doesn't kill the survivor: it exits by itself right after writing the
  marker, and killing a pid read from a file after it may have exited risks hitting a reused pid.
  Red checks run locally: with `killpg` sending signal 0 instead of `SIGKILL`, all six marker /
  time-bound tests fail; with `setsid` removed from `detach`, only the new pty `/dev/tty` test
  fails (the old one still passes). T4 and T5 landed in one commit (both touch
  `merge.spec.ts`). T5 also converts `refs-and-remotes.spec.ts`'s reverse wait (a row that
  unmounts, missing from the spec's list). `reapLeftovers` matches WebKitWebDriver only with
  `--port=4445` (tauri-driver's native port), so an unrelated WebKitWebDriver is left alone.

---

## Part T: tests that can fail (PR 1, `test/robustness`)

### Task T1: Unix marker tests for the process-group kill (#179)

**Files:** `crates/git-core/src/process.rs` (tests); `CHANGELOG.md` (stub `### Test robustness`).

- [ ] Add `#[cfg(unix)] fn unix_marker() -> (TempDir, PathBuf)` and `fn sh_with_marker(script,
  marker) -> Command` (sets `MARKER`).
- [ ] Tests (`GRANDCHILD = r#"(sleep 1; echo alive > "$MARKER") >/dev/null 2>&1 & sleep 30"#`):
  - `timeout_kills_a_hooks_background_grandchild_on_unix`: `run_captured(.., 300 ms)`, assert
    `timed_out`, sleep 2.5 s, assert no marker;
  - `cancel_kills_a_hooks_background_grandchild_on_unix`: cancel after 300 ms;
  - `split_timeout_kills_a_background_grandchild` and `split_cancel_kills_…` via
    `run_captured_split`.
- [ ] Tighten: `timeout_kills_a_process_whose_grandchild_holds_the_pipe` and
  `split_times_out_and_kills_the_group` assert `started.elapsed() < timeout + DRAIN_GRACE / 2`.
- [ ] Run `cargo test -p git-core process` 3×; all green (they pass on correct code; T2 proves they
  can fail).
- [ ] Commit `process tests: a background grandchild must not survive timeout or cancel (#179)`.

### Task T2: negative controls (#179, D2-A)

**Files:** `process.rs` (tests).

- [ ] `#[cfg(unix)] fn without_its_own_group_a_background_grandchild_survives()`: `sh_with_marker`
  with `(echo $$ > "$MARKER.pid"; sleep 1; echo alive > "$MARKER") …& sleep 30` **without**
  `detach`; `spawn_retrying`; `wait_bounded(&mut child, &|| false, 300 ms, None)`; wait up to 5 s
  for the marker; assert it exists; `libc::kill(pid_from_file, SIGKILL)`.
- [ ] `#[cfg(windows)] fn without_a_job_a_background_grandchild_survives_on_windows()`:
  `windows_sh(GRANDCHILD_WIN)`, `detach`, stdio null, `spawn_retrying`, `wait_bounded(.., None)`,
  wait up to 10 s, assert the marker exists. Doc comment: the control for the three job tests.
- [ ] Commit `process tests: controls showing the marker tests can fail (#179)`.

### Task T3: `/dev/tty` under a real terminal (#179, D1-A)

**Files:** `process.rs` (tests).

- [ ] `#[cfg(unix)] #[test] #[ignore] fn tty_probe_inner()`: returns at once unless
  `BROWSITORY_TTY_PROBE=1`; asserts `std::fs::File::open("/dev/tty").is_ok()` (we really have a
  terminal), then runs the existing probe through `run_captured`, asserts `"none\n"` within 5 s.
- [ ] `#[cfg(unix)] #[test] fn a_program_opening_dev_tty_fails_at_once_under_a_terminal()`:
  `posix_openpt(O_RDWR | O_NOCTTY)`, `grantpt`, `unlockpt`, `ptsname` (copy the name), open the
  slave; `Command::new(current_exe()).args(["--exact", "process::tests::tty_probe_inner",
  "--ignored", "--nocapture"]).env("BROWSITORY_TTY_PROBE", "1")` with stdio from the slave fd and
  `pre_exec(|| { setsid(); ioctl(0, TIOCSCTTY, 0) })`; drain the master on a thread; assert
  success within 30 s. Keep the old test, with its comment updated to point at this one.
- [ ] Run on Linux locally; macOS on CI.
- [ ] Commit `process tests: check /dev/tty with a real controlling terminal (#179)`.

### Task T4: merge spec waits for the merge commit (#182)

**Files:** `e2e/support/git.ts` (new); `e2e/specs/merge.spec.ts`.

- [ ] `waitForMergeCommit(repoPath, timeout = 10000)`: `browser.waitUntil` on `git rev-list
  --parents -n 1 HEAD` having 3 fields and no `.git/MERGE_HEAD`, `timeoutMsg` naming both.
- [ ] Replace l. 112–113 and l. 196–197 with `await waitForMergeCommit(E2E_REPO_PATH)`; keep the
  parents assertion after it.
- [ ] Typecheck (`cd e2e && pnpm exec tsc --noEmit`), run the spec locally under `xvfb-run -a`.
- [ ] Commit `e2e: wait for the app's merge commit, not the branch row (#182)`.

### Task T5: in-page waits and clicks (#182)

**Files:** `e2e/support/dom.ts` (new); `line-staging.spec.ts`, `rebase.spec.ts`,
`merge.spec.ts`, `keyboard-context-menus.spec.ts`, `git-hooks.spec.ts`.

- [ ] `waitUntilGoneInPage(selector, text?, { contains?, timeout? })` and
  `clickInPage(selector, text?, { contains?, timeout? })` (spec §2), with a header comment citing
  the WebKitWebDriver "create the node handle" stale error.
- [ ] Convert: `line-staging.spec.ts` l. 50–59 (menu clicks and the reverse wait); every
  `waitForExist({ reverse: true })` listed in the spec; `merge.spec.ts` l. 271–274's `isExisting`
  poll. Leave `waitForDisplayed({ reverse })` on always-mounted inputs.
- [ ] Run those specs locally 3× under `xvfb-run -a`.
- [ ] Commit `e2e: poll and click in the page where rows unmount (#182)`.

### Task T6: harness process group and diagnostics (#182, D4-A)

**Files:** `e2e/wdio.conf.ts`.

- [ ] `spawn(driverPath, [], { detached: true, … })`; `closeTauriDriver()` kills `-pid` with
  SIGTERM, awaits `exit` up to 5 s, then SIGKILL to `-pid`; `afterSession` awaits it.
- [ ] `beforeSession`: `pgrep -u $(id -u) -f "<tauriAppBinary>|WebKitWebDriver"`; log and kill any
  leftovers; after spawning, `waitForPort(4444)` and `waitForPort(4445)`; record
  `sessionRequestedAt`.
- [ ] `before` hook: log `[e2e] session start: ${Date.now() - sessionRequestedAt} ms`.
- [ ] Full suite locally under `xvfb-run -a`; confirm no leftover processes (`pgrep`) afterwards.
- [ ] Commit `e2e: run tauri-driver in its own process group and reap it (#182)`.

### Task T7: the 30 s session start (#182)

**Files:** `e2e/wdio.conf.ts`; maybe `.github/workflows/ci.yml` (e2e step).

- [ ] Locally: run `first-flow.spec.ts` under `xvfb-run -a` with each candidate env
  (`NO_AT_BRIDGE=1`, `GTK_USE_PORTAL=0`, `WEBKIT_DISABLE_COMPOSITING_MODE=1`) and under
  `dbus-run-session`; record the session-start times in "Pre-flight notes".
- [ ] Push the branch once with the best candidate; compare the CI `[e2e] session start` lines.
- [ ] Keep a change only if CI starts drop below ~5 s; otherwise keep T6 alone and record the
  numbers for a follow-up.
- [ ] Commit `e2e: <setting> cuts WebDriver session start (#182)` (or no commit).

### Task TZ: changelog, docs, gate (PR 1)

- [ ] `CHANGELOG.md` → `### Test robustness`:
  - `Fixed (tests): the Unix tests for stopping a hook now check that a background process the hook started can't finish its work after a timeout or Cancel; control tests (also for the Windows job object) show the checks fail without the process group or job (#179).`
  - `Fixed (tests): the /dev/tty check now runs under a real terminal, so it also proves something on CI (#179).`
  - `Fixed (tests): flaky desktop E2E tests: a merge test that could start the next test before the app's merge commit landed, stale-element failures when a row disappears during a lookup, and WebDriver sessions that hung after an earlier app instance was left running (#182).`
- [ ] `docs/ARCHITECTURE.md` "Testing strategy": in-page helpers rule; tauri-driver process group.
- [ ] Gate: `cargo build --workspace`; `cargo test -p git-core` 3×; `cargo clippy --workspace
  --all-targets -- -D warnings`; `cargo fmt --all -- --check`; Tauri E2E and VSCode E2E in
  CLAUDE.md order (`xvfb-run -a`); `python3 scripts/check-changelog.py --base origin/main`.
- [ ] CI: three consecutive green `e2e` runs without rerun (record run ids); `windows-latest`
  shows the T2 control green.
- [ ] PR: D1, D2, D4 answers; "Closes #179, closes #182" (lock items fixed by #191).

---

## Part W: worker call gate (PR 2, `fix/worker-call-gate`)

### Task W1: failing test (#181)

**Files:** `crates/tauri-app/src/commands/mod.rs` (tests); `CHANGELOG.md` (stub `### Worker calls`).

- [ ] `#[cfg(unix)] fn queued_calls_for_a_busy_repo_do_not_delay_another_repo()`: two repos (the
  existing hook fixture for A, a plain repo B); start the A commit (hook waits on a release file);
  spawn 600 `RepoWorker(..).call(|w| w.get_status())` on A; then `call` on B with a 5 s
  `recv_timeout` via a plain thread + `block_on`; assert B answered; release; join all.
  Construct `RepoWorker` through a test helper that builds the gate (W2).
- [ ] Confirm it fails (B times out) before W2.

### Task W2: per-repository gate (#181, D3-A)

**Files:** `commands/mod.rs`.

- [ ] `AppState { workers, gates: Mutex<HashMap<String, Arc<tauri::async_runtime::Mutex<()>>>> }`
  (`Default` derive stays).
- [ ] `pub(crate) struct RepoWorker { handle: WorkerHandle, gate: Arc<..> }`; `repo_worker`
  clones/creates the gate; `close_repo` removes it.
- [ ] `call`: `let _turn = self.gate.lock().await;` then `off_runtime(move || f(&handle)).await?`.
  `signal()` unchanged. Doc comment: serial worker, one thread per repository, FIFO.
- [ ] Update the two existing tests to the new constructor; add
  `calls_for_one_repo_keep_their_order` (10 calls appending to a shared Vec through the gate).
- [ ] `cargo test -p tauri-app` and clippy.
- [ ] Commit `tauri-app: one blocking thread per repository for queued worker calls (#181)`.

### Task WZ: changelog, docs, gate (PR 2)

- [ ] `CHANGELOG.md` → `### Worker calls`:
  - `Fixed (desktop app): while a hook ran for a long time, repeated refreshes could use up the app's background threads, and actions in every other open repository then hung until the hook ended. Actions waiting for one repository now share one background thread (#181).`
- [ ] `docs/ARCHITECTURE.md` "Threading model" l. 176–186: the gate.
- [ ] Gate as in TZ (Rust + both E2E suites). PR: D3 answer, "Closes #181".

---

## Part H: hook status and parity (PR 3, `feat/hook-status-parity`)

### Task H1: `HookSource` through both transports (#185)

**Files:** `worker/hooks.rs`, every `ChannelHookReporter` construction (`status.rs`, `history.rs`),
`tauri-app/commands/mod.rs`, `vscode-sidecar/src/dispatch.rs`, `frontend/src/ipc/RepoClient.ts`,
`state/useHookActions.ts`, `App.tsx`; `CHANGELOG.md` (stub `### Hooks: status and parity`).

- [ ] **Failing tests:** Tauri `hook_progress_dto_serializes_its_source` (`"source":"Checkout"`);
  sidecar relay test asserts `params.source == "Commit"` for Continue; Vitest
  `useHookActions` keeps `source`; `CommitBox` gets no running hook for `source: "Checkout"`.
- [ ] **Implement:** `pub enum HookSource { Commit, Checkout, Merge, Pick }`; field `source` on
  both `HookEvent` variants and on `ChannelHookReporter`; DTOs gain `source`; `HookProgress.source:
  "Commit" | "Checkout" | "Merge" | "Pick"`; `hookRun` stores it; `App.tsx` passes
  `hookRun?.source === "Commit" ? hookRun : null` to `CommitBox` and `RebaseProgressPanel`.
- [ ] `cargo test -p repo-service -p tauri-app -p vscode-sidecar`; `pnpm test -- --run`; lint.
- [ ] Commit `hooks: tag hook progress with its source (#185)`.

### Task H2: post-checkout reported and cancellable (#185)

**Files:** `worker/branch.rs`, `worker/history.rs`, `worker/mod.rs`, `worker/hooks.rs`; Tauri
`commands/branch.rs`, `commands/history.rs`; sidecar `switch_branch`/`checkout_commit`; all
`switch_branch`/`checkout_commit` call sites (tests).

- [ ] **Failing tests** (worker, real repos with a `post-checkout` hook):
  - `switch_reports_post_checkout_on_its_operation`: events `Started{source: Checkout, hook:
    "post-checkout"}` then `Finished`, same `operation_id` starting `switch-`;
  - `cancel_stops_post_checkout_and_keeps_the_switch`: hook `sleep 30`; on `Started`,
    `cancel_transfer(id)`; reply `Ok` within 10 s; HEAD on the new branch;
  - the same two for `checkout_commit`;
  - `post_checkout_gets_old_and_new_head_…` (l. 2090) stays green.
- [ ] **Implement:** `Command::{SwitchBranch, CheckoutCommit}` gain `operation_id`, `events`;
  handles gain `events: Sender<HookEvent>`; `run_post_checkout(repo, before, op, events)` uses
  `run_post_hook_reported` with `HookOptions { cancel, reporter }`, then `op.clear()`. Tauri
  commands `emit_hook_events`; sidecar `spawn_hook_relay`.
- [ ] Commit `hooks: report post-checkout and let Cancel stop it (#185)`.

### Task H3: post-merge after fast-forward and squash, and after pull (#185)

**Files:** `worker/merge.rs`, `worker/remote.rs`, `worker/mod.rs`; Tauri `commands/merge.rs`;
sidecar `start_merge`.

- [ ] **Failing tests:** fast-forward and squash `start_merge` report `post-merge` with
  `source: Merge` and are cancellable (outcome unchanged); pull fast-forward emits
  `TransferEvent::Progress { phase: RunningHook }` before `Completed`, and `cancel_transfer` on the
  pull's id during a `sleep 30` post-merge completes the pull with `error: None` within 10 s.
- [ ] **Implement:** `Command::StartMerge` gains `operation_id`, `events`; `merge::start` runs
  `run_post_hook_reported`; `remote::pull` sends `RunningHook` and runs the hook with
  `op.cancel_check()`.
- [ ] Commit `hooks: report post-merge after fast-forward, squash and pull (#185)`.

### Task H4: frontend strip and transfer label (#185, D8-A)

**Files:** `frontend/src/components/HookRunStrip.tsx` (+ `.module.css`, test), `App.tsx`,
`TransferPanel.tsx` (+ test).

- [ ] **Failing tests:** `HookRunStrip` renders "Running post-checkout hook…" with Cancel for
  `source: "Checkout"`, nothing for `"Commit"` or `null`; Cancel calls `onCancel` once and latches;
  `TransferPanel` shows "Running post-merge hook…" for `{ operation: "Pull", phase: "RunningHook" }`
  and "Running pre-push hook…" for a push.
- [ ] **Implement:** `HookRunStrip({ running, onCancel })` (role=status, aria-label "Hook status",
  `HookStatus` inside); render under `OperationStatusStrip`, Cancel enabled while `pending`;
  `TransferPanel` label by `progress.operation`.
- [ ] Commit `ui: show post-checkout and post-merge hooks with Cancel (#185)`.

### Task H5: merge commit checks the merge after its hooks (#183)

**Files:** `crates/git-core/src/commit.rs`; `worker/merge.rs`, `worker/mod.rs`;
`crates/git-core/tests/hooks.rs`.

- [ ] **Failing tests:**
  - `a_merge_restarted_during_pre_merge_commit_is_not_committed`: clean merge of `side`;
    `pre-merge-commit` runs `git merge --abort && git merge --no-ff --no-commit side2`;
    `commit_with(.., CleanMerge { head, merge_heads: [side] })` → `CommitError::MergeChanged`;
    HEAD unchanged; `MERGE_HEAD` = `side2`.
  - `commit_with_runs_pre_commit_when_the_expected_merge_is_not_the_current_one`: expected heads
    `[x]`, repository merging `[y]` → `pre-commit` ran, `merge_hooks == false`.
- [ ] **Implement:** `CommitHookSet::CleanMerge { head, merge_heads }`; `CommitError::MergeChanged`
  (spec §6 text); re-read after `run_commit_hooks`; `CleanMerge::hook_set()` in the worker.
- [ ] Commit `commit: refuse a clean merge that changed while its hooks ran (#183)`.

### Task H6: `MERGE_MSG` for a clean merge's message hooks (#183, D6-A)

**Files:** `commit.rs`; `tests/hooks.rs`.

- [ ] **Failing test:** a `prepare-commit-msg` logging `basename "$1" "$2"` and appending a line;
  clean-merge commit → log `MERGE_MSG merge`, the commit has the appended line; parity twin: real
  `git merge --no-ff side` with the same hook logs the same.
- [ ] **Implement:** `enum MessageFile { CommitEditmsg, MergeMsg }` parameter on
  `run_commit_hooks`; `commit_with` passes `MergeMsg` when `merge_hooks`.
- [ ] Commit `commit: a clean merge's message hooks get MERGE_MSG, as with git merge (#183)`.

### Task H7: a landed commit stays a success (#183, D7-A)

**Files:** `commit.rs`, `pick.rs` (`finish_paused`); `tests/hooks.rs`; worker test.

- [ ] **Failing tests** (`#[cfg(unix)]`, skip as root):
  - `a_cleanup_failure_after_a_merge_commit_still_reports_it`: clean merge; `chmod 0555 .git`;
    `commit_with(CleanMerge…)` is `Ok` with `merge_hooks`; HEAD has 2 parents; restore mode.
  - worker: same setup with a `post-merge` writing a marker in the worktree → reply `Committed`,
    marker exists.
  - Continue: paused step, `chmod 0555 .git` before Continue → outcome not an error, step committed.
- [ ] **Implement:** after the commit, `if let Err(e) = repo.cleanup_state() { eprintln!(...) }`
  (same for `clear_squash_message`); `finish_paused` likewise.
- [ ] Commit `commit: a commit that landed counts even if git's state can't be cleared (#183)`.

### Task H8: hooks on clean pick steps in git-core (#183, D5-A, D9-A)

**Files:** `commit.rs` (`run_message_hook`), `pick.rs`; `tests/pick.rs`, `tests/pick_sequence.rs`.

- [ ] **Failing tests** (logging hooks for all four commit hooks):
  - `clean_sequence_steps_run_prepare_commit_msg_and_post_commit`: `cherry_pick_many` of 3 clean
    commits with hooks → log `prepare-commit-msg COMMIT_EDITMSG message` + `post-commit`, 3×; no
    `pre-commit`/`commit-msg`;
  - **parity:** the same hooks in a twin repo, `git cherry-pick A B C` → identical log;
  - `prepare_commit_msg_can_rewrite_a_picked_message`;
  - `a_failing_prepare_commit_msg_pauses_the_step_staged`: outcome `HookFailed`, state
    `CherryPick`, `CHERRY_PICK_HEAD` = step, state `stopped_on_conflict`; then `continue_sequence`
    commits it (full hooks) and finishes;
  - `continue_runs_the_hooks_for_later_clean_steps`;
  - single `cherry_pick`/`revert` → hooks ran; failure → `PickOutcome::HookFailed`, pick in
    progress;
  - cancel: during `post-commit` of step 1 of 3 → step 1 committed, run stops with
    `PickError::Hook(Cancelled)`, `next == 1`.
- [ ] **Implement** per spec §5 (`step(.., hooks)`, `StepResult::HookFailed`, `run(.., hooks)`,
  `apply(.., hooks)`, public signatures gain `hooks: HookOptions`). Update the doc comments
  (no more "D3-A").
- [ ] Commit `picks: run prepare-commit-msg and post-commit on clean steps, as git (#183)`.

### Task H9: picks through the worker and transports (#183)

**Files:** `worker/history.rs`, `worker/mod.rs`; Tauri `commands/history.rs`; sidecar handlers;
`frontend` (`state/useHistoryActions.ts`, `lib/hookFailure.ts`, `RepoClient.ts` `PickOutcome`).

- [ ] **Failing tests:** worker `cherry_pick_many_reports_hooks_with_source_pick` and cancel;
  Tauri `PickOutcomeDto` serializes `HookFailed`; sidecar relay for `cherry_pick_many`; Vitest:
  `cherryPick` resolving `HookFailed` opens the hook dialog with only Close and keeps the strip.
- [ ] **Implement:** commands gain `operation_id`/`events` (mint `"pick"`); `HookRetry` gains
  `{ kind: "pick" }` (`retryLabel` → `null`).
- [ ] Commit `picks: report their hooks and show a failing prepare-commit-msg (#183)`.

### Task H10: E2E (#185)

**Files:** `e2e/specs/git-hooks.spec.ts`.

- [ ] New `it`: install `post-checkout` `sleep 30`; switch branch from the sidebar; the
  "Hook status" strip shows "Running post-checkout hook…"; `clickInPage("button", "Cancel")`;
  `waitUntilGoneInPage('[aria-label="Hook status"]')`; `git rev-parse --abbrev-ref HEAD` is the
  new branch. Remove the hook in `after`.
- [ ] Commit `e2e: post-checkout status and Cancel (#185)`.

### Task HZ: docs, changelog, gate (PR 3)

- [ ] `CHANGELOG.md` → `### Hooks: status and parity`:
  - `While post-checkout runs after you switch branches or check out a commit, or post-merge after a fast-forward or squash merge, "Running post-checkout hook…" shows at the top of the repository with Cancel. Cancelling stops the hook and keeps the switch or merge (#185).`
  - `A pull that fast-forwards shows its post-merge hook in the transfer panel, with Cancel (#185).`
  - `Cherry-picks and reverts that apply without conflicts now run prepare-commit-msg and post-commit for each commit they create, as git does, including the steps a Continue applies after the paused one. If prepare-commit-msg fails, that commit stays staged and paused, so you can fix the hook and continue or commit it (#183).`
  - `When you commit a merge that had no conflicts, prepare-commit-msg and commit-msg get .git/MERGE_MSG, as with git merge (#183).`
  - `Fixed: if a merge was aborted and restarted in a terminal while pre-merge-commit ran, the commit used the old merge's parents; it now stops with "The merge changed while its hooks ran." (#183).`
  - `Fixed: a commit that was created but whose merge or cherry-pick state couldn't be cleared was reported as failed, skipped post-merge, and a retry could create a second merge commit; it now counts as done (#183).`
- [ ] `docs/ARCHITECTURE.md`: "Cherry-pick sequences" l. 360–373; "Git hooks" l. 485–500.
  `docs/USER_GUIDE.md`: hooks table l. 206–212 (add "cherry-pick or revert": `prepare-commit-msg`,
  `post-commit`), status paragraph l. 237–242 (drop "show no status line").
- [ ] Gate: full CLAUDE.md order (Rust, frontend build/lint/test, extension compile/test, both
  E2E suites); `check-changelog.py`. Manual smoke on a `/tmp` scratch repo: slow `post-checkout`
  in the desktop app and the extension, Cancel in both.
- [ ] PR: D5–D9 answers; "Closes #183, closes #185".

---

## Part D: folder/file swaps (PR 4, `fix/folder-file-swaps`)

### Task D1: unstage refuses conflicts below a pair root (#187, D12-A)

**Files:** `crates/git-core/src/stage.rs`; `crates/git-core/tests/stage_commit.rs`;
`CHANGELOG.md` (stub `### Folder/file swaps`).

- [ ] **Failing tests:**
  - `unstaging_a_file_with_only_conflict_entries_below_it_is_refused`: HEAD has file `d`; index:
    remove `d`, `conflict_add(Some(d/f@1), Some(d/f@2), Some(d/f@3))`, write; read
    `.git/index` bytes; `unstage_file("d")` → `ConflictsBelow("d")`; bytes unchanged.
  - `unstaging_a_pair_with_a_staged_file_and_conflicts_below_keeps_the_staged_file`: plus `d/g` at
    stage 0; unstage `d` and `d/g` → refused; `d/g` still staged.
  - The #184 pair tests stay green.
- [ ] **Implement:** `StageError::ConflictsBelow(String)` (spec §8 text); check before changes;
  no `index.write()` in the pair-root branch; `index.read(true)` when `reset_default` fails.
- [ ] Commit `unstage: refuse while conflicts sit below a folder/file swap (#187)`.

### Task D2: characterize stash behaviour (#186)

**Files:** `crates/git-core/tests/stash.rs`.

- [ ] Tests on `folder_replaced_by_file()` (l. 1101), each with a git CLI twin:
  - `path_limited_stash_of_a_file_that_replaced_a_folder` (paths `["d"]`, untracked on);
  - `stash_all_with_untracked_of_a_file_that_replaced_a_folder` then `pop_stash`;
  - `stash_all_without_untracked_deletes_a_file_in_the_way` (expected to show the deletion).
- [ ] Record each actual result in "Pre-flight notes" (refused? stash trees? pop result?), then
  turn the assertions into the target behaviour for D3/D4. Commit the target tests red only on this
  branch.

### Task D3: path-limited stash of the file in the way (#186, D11-A)

**Files:** `stash.rs`.

- [ ] Skip `FolderIsNowAFile` when `parent` is in `untracked`. Assert after save: worktree tree
  lacks `d/f`, untracked tree has `d`, worktree has `d/f` = `f\n`, no `d` file.
- [ ] If D2's pop failed: `StashError::FileReplacedFolder` (spec §7) for apply/pop, detected
  before libgit2 writes; test that the stash is kept and the worktree untouched.
- [ ] Commit `stash: stash a file that replaced a folder together with the folder's deletion (#186)`.

### Task D4: refuse deleting untracked files in the way (#186, D10-A)

**Files:** `stash.rs`.

- [ ] Failing test from D2 now expects `UntrackedInTheWay { untracked: "d", tracked: "d/f" }`,
  nothing stashed, `d` intact; the mirror case (HEAD file `a`, untracked `a/x`).
- [ ] **Implement** the pre-check (spec §7) for `include_untracked == false` on both paths.
- [ ] Commit `stash: refuse to delete untracked files in the way without Include untracked (#186)`.

### Task DZ: docs, changelog, gate (PR 4)

- [ ] `CHANGELOG.md` → `### Folder/file swaps`:
  - `Fixed: unstaging a file whose folder a merge left conflicts in failed with "invalid entry mode" and could unstage other files below it; it now says "Can't unstage 'd': the merge left conflicts in it. Resolve them first." and changes nothing (#187).`
  - `Stashing a file that replaced a tracked folder now also stashes the folder's deletion and puts the folder back, as git stash does (#186).` (plus, if D3's second step was needed: `Applying such a stash says to use git stash apply from a terminal, and keeps the stash.`)
  - `Stash… with Include untracked files off refuses when an untracked file is in the way of a stashed folder or file, instead of deleting it (#186).`
- [ ] `docs/USER_GUIDE.md` "Stashes" l. 445–455 and the unstaging note.
- [ ] Gate: Rust, frontend sanity, both E2E suites, `check-changelog.py`. PR: D10–D12 answers,
  D2 findings; "Closes #186, closes #187".

---

## Decision hooks

| Decision | If the user picks… | Change |
|---|---|---|
| D1-B | rely on the session probe | Drop T3; rename the `/dev/tty` test `…_without_a_terminal` and document why it can't discriminate on CI. |
| D1-C | `script` | T3 runs `script -qec "<test binary> …" /dev/null` on Linux only; macOS keeps the old test. |
| D2-B | red CI run | Drop T2; push a commit on PR 1 disabling `detach`/the job in a scratch test, record the red run, revert. |
| D3-B | async replies | Separate PR: `Reply<T>` (std or tokio oneshot) in every `Command`; `WorkerHandle` gains `async` variants; W1's test unchanged. |
| D3-C | frontend coalescing | W2 replaced by `refresh()` coalescing in `useAppState.ts` (in-flight promise + one trailing rerun, `refreshGeneration` per run); W1 becomes a Vitest test; #181 stays partly open. |
| D4-B | spec retries | T6 also sets `specFileRetries: 1`, `specFileRetriesDeferred: true`. |
| D5-B | after Continue only | H8: hooks passed only from `continue_sequence` to `run`; drop single-pick and start hooks, H9's single-pick parts and `source: Pick`. |
| D5-C | won't fix | Drop H8/H9; ARCHITECTURE keeps D3-A. |
| D6-B | keep COMMIT_EDITMSG | Drop H6; document in USER_GUIDE. |
| D7-B | warning field | H7 adds `warning: Option<String>` to `CommitOutcome::Committed`, DTOs and an error-banner message. |
| D8-B | modal overlay | H4 renders the strip's content in `Overlay` + `Panel` like `TransferPanel`. |
| D8-C | commit box only | Drop H1's routing and H4's strip; `CommitBox` shows every source. |
| D9-B | continue after cancel | H8: a cancelled `post-commit` doesn't stop `run`. |
| D10-B | confirm | D4 adds `StashPlan` dry-run IPC and a confirm dialog (frontend + both transports). |
| D10-C | document | Drop D4. |
| D11-B | refuse | D3 keeps the refusal with "Can't stash 'd': it replaced the folder 'd' that is still tracked. Use git stash from a terminal." |
| D11-C | own apply | Separate PR: ordered apply (tracked via libgit2 merge + checkout, then untracked through `worktree_guard`). |
| D12-B | git parity | D1: replace entries at/below the root with HEAD's (conflicts dropped) in one in-memory edit, one write; tests expect success. |
| D12-C | refuse only | D1 without the in-memory change. |
| D13-B | three PRs | Parts H and D on one branch, one CHANGELOG section. |
| D13-C | one PR | All parts on `feat/robustness-bundle`, sections as above. |
