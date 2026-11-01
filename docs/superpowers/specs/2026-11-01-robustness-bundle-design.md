# Robustness bundle

## Goal

Close seven follow-ups from the hooks (#176), UX polish (#184) and lock (#191) batches:

| Issue | Area | Problem |
|---|---|---|
| #179 | process tests | Unix tests would pass if `killpg` missed the group; the `/dev/tty` test can't tell the cases apart on CI; the Windows job tests were never seen red. |
| #181 | tauri-app | Every queued worker call parks a Tokio blocking-pool thread (cap 512); a long hook plus repeated refreshes can starve every repository. |
| #182 | E2E | Tauri WebDriver session start hangs; line-staging stale element; `merge.spec.ts:215` race. (The lock flakes were fixed in #191.) |
| #183 | hooks | Clean pick steps run no `prepare-commit-msg`/`post-commit`; a clean merge's message hooks get `COMMIT_EDITMSG`, not `MERGE_MSG`; a merge changed during `pre-merge-commit` is committed with stale parents; a `cleanup_state` error after the commit landed is reported as a failure. |
| #185 | hooks UX | `post-checkout` and `post-merge` after a fast-forward, squash or pull run with no status and no Cancel. |
| #186 | stash | A path-limited stash of an untracked file that replaced a tracked folder is unverified (and probably refused with a misleading message); Stash… without untracked deletes untracked files in the way. |
| #187 | staging | Unstaging a folder/file pair with conflict entries below it fails with "invalid entry mode" after rewriting the index. |

## Shipping

Four PRs (D13-A), one CHANGELOG section each under `## [Unreleased]`:

- **PR 1** `test/robustness`: #179, #182. `### Test robustness`. `git-core/src/process.rs` tests,
  `process/job_windows.rs` tests, `e2e/` (harness, support helpers, specs). No production change.
- **PR 2** `fix/worker-call-gate`: #181. `### Worker calls`. `tauri-app/src/commands/mod.rs`.
- **PR 3** `feat/hook-status-parity`: #185, #183. `### Hooks: status and parity`. `git-core`
  (`commit.rs`, `pick.rs`), `repo-service` (`worker/{hooks,branch,history,merge,remote,status,mod}.rs`),
  `tauri-app` and `vscode-sidecar` (DTO field, event channels), `frontend` (`HookRunStrip`, routing,
  `TransferPanel` label, pick hook failures), `e2e/specs/git-hooks.spec.ts`.
- **PR 4** `fix/folder-file-swaps`: #186, #187. `### Folder/file swaps`. `git-core` (`stash.rs`,
  `stage.rs`).

No new dependency. New user-visible strings are listed per section.

## Current state (main at 25040598, checked 2026-11-01)

### #179
- `process.rs`: `DRAIN_GRACE` = 2 s (l. 46); `kill_tree` (l. 180) does `killpg(child pid)` and
  `child.kill()`; `detach` (l. 207) `setsid` in `pre_exec`; `wait_bounded` (l. 275);
  `run_captured` (l. 304), `run_captured_split` (l. 386, no job by design).
- `timeout_kills_a_process_whose_grandchild_holds_the_pipe` (l. 1160): output `"started\n"` either
  way; a wrong group only costs the 2 s grace. `split_times_out_and_kills_the_group` (l. 1240)
  allows 10 s. `a_program_opening_dev_tty_fails_at_once` (l. 1356) only discriminates with a
  controlling terminal; `captured_program_leads_its_own_session_without_a_terminal` (l. 1322,
  Linux, `/proc/$$/stat` tty_nr) already proves no controlling terminal on Linux.
- Windows: `timeout_kills_a_hooks_background_grandchild_on_windows`,
  `cancel_kills_…`, `a_normal_exit_leaves_…`, `a_hook_is_in_its_job_before_it_runs` (l. 1386+).
  Nothing shows them failing without a job.

### #181
- `tauri-app/src/commands/mod.rs`: `AppState { workers }` (l. 1677); `RepoWorker(WorkerHandle)`
  (l. 1938); `call` (l. 1942) = `off_runtime(move || f(&handle))`; `repo_worker` (l. 1961);
  `off_runtime` (l. 1970) = `spawn_blocking`. Tests l. 2012 and l. 2039.
- `WorkerHandle` methods (`repo-service/src/worker/*.rs`, ~99) send a `Command` and block on a
  std `mpsc` reply. The worker thread is serial (`for command in rx`, `mod.rs` l. 770).
- Tauri's runtime is tokio with the default 512 blocking threads. `tauri::async_runtime`
  re-exports `tokio::sync::Mutex` (tauri 2.11.5 `async_runtime.rs` l. 13–17).
- Every `call` closure only makes round trips on its own handle (checked); the merge tool waits outside `call`
  (`tools.rs` l. 104–117). `cancel_transfer`/`cancel_search` use `signal()`.
- Frontend: `refresh()` (`useAppState.ts` l. 393) sends ~16 reads at once; window focus sends one
  `getUndoState` (`App.tsx` l. 304–309).

### #182
- Lock flakes: fixed in #191 (`only_the_first_…` uses `CONTENDED_WAIT`/`LOCK_WAIT`).
- Session start (CI job 124293605140, `git-hooks.spec.ts`): the app started, `POST /session`
  aborted after 120 s (wdio default `connectionRetryTimeout`), "1 retries". Every session start on
  CI takes ~30 s (e.g. 20:34:29 → 20:34:59), about 15 minutes per E2E job.
  `e2e/wdio.conf.ts`: `beforeSession` (l. 264) spawns tauri-driver (not detached) and waits for
  port 4444 only (l. 292); `afterSession` (l. 302) SIGTERMs tauri-driver, which SIGKILLs
  WebKitWebDriver (tauri-driver 2.0.6 `server.rs` l. 172–193) — WebKitWebDriver then can't close
  an app whose session never completed. Nothing waits for 4445 or checks for leftovers.
- Stale elements (job 124265886301): `line-staging.spec.ts:59`
  `$("span=line-untracked.tmp (New)").waitForExist({ reverse: true })` failed in `findElement`
  ("Stale element found when trying to create the node handle"). The same reverse-wait pattern:
  `rebase.spec.ts` l. 121/202/336/382, `merge.spec.ts` l. 289, `keyboard-context-menus.spec.ts`
  l. 68/86/95, `git-hooks.spec.ts` l. 85. `cherry-pick-sequence.spec.ts` was fixed in #176 by
  polling in the page.
- `merge.spec.ts`: after Commit, l. 112 waits on `li*=e2e-merge-feature` and l. 196 on
  `li*=e2e-merge-adddelete-feature`, both matched by the existing sidebar branch row, so the next
  step (l. 115 `rev-list`; l. 207 the next test's own `git commit`) can run before the app's merge
  commit lands (run 41239613330: "nothing to commit").
- `extension/e2e` uses in-page helpers (`webviewPage.ts`); no flake reported.

### #183
- `pick.rs`: `step` (l. 296) commits a clean step with `signing::create_commit`, no hooks, then
  `cleanup_state()?`; `run` (l. 483); `continue_sequence` (l. 544) runs hooks only in
  `finish_paused` (l. 605: `run_commit_hooks(.., "pre-commit", "merge", ..)`,
  `create_commit_reporting`, `cleanup_state()?`, `run_post_commit`). Single `cherry_pick`/`revert`
  (`apply`, l. 230) run none.
- git (sequencer `try_to_commit`): each picked commit runs `prepare-commit-msg <COMMIT_EDITMSG>
  message` and `post-commit`; no `pre-commit`/`commit-msg`. A failing `prepare-commit-msg` leaves
  the pick staged with `CHERRY_PICK_HEAD`.
- `commit.rs`: `run_commit_hooks` (l. 272) always writes `<gitdir>/COMMIT_EDITMSG`; `commit_with`
  (l. 424) reads `MERGE_HEAD` before the hooks, commits with those parents, then
  `cleanup_state()?` (l. 511) — an error after the commit landed. git merge's own commit passes
  `.git/MERGE_MSG` to `prepare-commit-msg` and `commit-msg`.
- Worker: `CleanMerge { head, merge_heads }` (`worker/merge.rs` l. 17) compared in
  `Command::Commit` (`mod.rs` l. 1095) under the state lock. So "another window aborts and starts a
  different merge" is already covered between commands; the gap left is a change *during*
  `pre-merge-commit` (a terminal), and the check-to-use split between the worker and `commit_with`.
- `status::commit` (worker, l. 301): `committed = result.is_ok()`; on `Err` the merge is kept
  remembered, post-merge is skipped, the frontend keeps the message; a retry with `MERGE_HEAD`
  still present would create a second merge commit.

### #185
- `worker/hooks.rs`: `run_post_hook` (l. 85, unreported, not cancellable), `run_post_checkout`
  (l. 117). Callers: `branch.rs` l. 22 (switch), `history.rs` l. 233 (checkout commit), `merge.rs`
  l. 57–58 (fast-forward `0`, squash `1`), `remote.rs` l. 337 (pull fast-forward).
- Reported path (#184): `run_post_hook_reported` + `ChannelHookReporter { events, operation_id }`,
  cancel through `TransferOp::cancel_check` and `cancel_transfer(operation_id)`. Events:
  Tauri `hook-progress` (`commands/mod.rs` l. 830–860), sidecar `hookProgress`
  (`dispatch.rs` l. 1539–1580). Frontend: `useHookActions` sets `state.hookRun` on any `Started`;
  `CommitBox` (l. 375) and `RebaseProgressPanel` render `HookStatus`. `OperationStatusStrip`
  (App.tsx l. 544) sits at the top of the repository panel. `TransferPanel` hard-codes
  `runningHookLabel("pre-push")` for `RunningHook`.

### #186
- `stash.rs`: `save_stash` (l. 130) routes paths to `save_path_limited_stash` (l. 208), whose
  status pathspec is literal (`disable_pathspec_match`); libgit2's iterator pathlist makes `d`
  match `d/f` (`iterator.c` `iterator_pathlist_next_is`). So for HEAD `d/f` + untracked file `d`,
  `tracked = [d/f]`, `untracked = [d]`, and the `FolderIsNowAFile` check (l. 292, #184) refuses
  with "Can't stash 'd/f': 'd' is now a file…" although the revert removes `d` (step 1, l. 398)
  before checking `d/f` out. (Unverified by running; Task D2 pins it.)
- `apply_stash` (l. 508) uses libgit2 `git_stash_apply`, which checks out the untracked tree
  before the modified tree (`stash.c` ~l. 1113 vs ~l. 1149); git restores untracked last. Popping
  {untracked `d`, deleted `d/f`} may therefore conflict. Stash all with untracked
  (`stash_save2`) already produces such stashes.
- Without "Include untracked files", `stash_save2`'s forced reset deletes an untracked file in
  the way of a tracked path (git's `reset --hard` does too).

### #187
- `stage.rs` `unstage_file` (l. 390): `pair_root` (l. 427) treats conflict entries as "entries
  below" (`index_has_entries_below`); the pair-root branch removes stage-0 entries
  (`remove_dir(.., 0)`) and **writes** the index, then `reset_default` fails ("invalid entry
  mode") on the remaining conflict entries. With `d/g` staged and `d/f` conflicted, `d/g` is lost.

## Design

### 1. #179: tests that can fail (D1-A, D2-A)
- Unix marker tests (`#[cfg(unix)]`): timeout and cancel for `run_captured` and
  `run_captured_split`, script `(sleep 1; echo alive > "$MARKER") >/dev/null 2>&1 & sleep 30`,
  timeout 300 ms / cancel at 300 ms, then sleep 2.5 s and assert no marker. Background jobs of a
  non-interactive `sh` stay in its process group, so `killpg` must reach them.
- The two pipe tests also assert `elapsed < timeout + DRAIN_GRACE / 2`.
- Negative controls in the same test module (private functions, no production seam):
  `without_its_own_group_a_background_grandchild_survives` (Unix: `sh` spawned *without*
  `detach`, `wait_bounded(.., None)`; `killpg` finds no group; marker appears) and
  `without_a_job_a_background_grandchild_survives_on_windows` (`detach` + `spawn_retrying` +
  `wait_bounded(.., None)`; marker appears). Each kills the survivor at the end (`kill -9 -- -pgid`
  is not available for the Unix control; it writes its pid to a file which the test kills).
- `/dev/tty` (D1-A): `a_program_opening_dev_tty_fails_at_once_under_a_terminal` re-runs the test
  binary (`std::env::current_exe()`, `--exact <inner> --ignored --nocapture`,
  `BROWSITORY_TTY_PROBE=1`) as a child that `pre_exec`s `setsid()` and
  `ioctl(slave, TIOCSCTTY, 0)` on a pty slave opened from `posix_openpt` / `grantpt` / `unlockpt` /
  `ptsname` (all in `libc`, already a dependency), with stdin/stdout/stderr on the slave. The inner
  `#[ignore]` test runs the existing probe and asserts `"none\n"` within 5 s. The outer test asserts
  the child's exit status and reads the pty master to completion.

### 2. #182: E2E (D4-A)
- `e2e/support/dom.ts`:
  - `waitUntilGoneInPage(selector: string, text?: string, timeout = 10000)`:
    `browser.waitUntil(() => browser.execute(…querySelectorAll(selector)… none whose trimmed
    textContent equals/contains text…))`.
  - `clickInPage(selector: string, text?: string, timeout = 10000)`: polls an execute that finds
    and clicks the element in one call and returns whether it did.
- `e2e/support/git.ts`: `waitForMergeCommit(repo, timeout)`: `rev-list --parents -n 1 HEAD` has
  3 fields and `.git/MERGE_HEAD` is gone.
- Specs: `merge.spec.ts` l. 112 and l. 196 use `waitForMergeCommit`; every `waitForExist({ reverse:
  true })` on an element that unmounts (list above) and the line-staging menu clicks use the
  in-page helpers. `waitForDisplayed({ reverse })` on palette inputs stays (they stay mounted).
- Harness (`wdio.conf.ts`):
  - spawn tauri-driver with `detached: true` (its own process group: tauri-driver, WebKitWebDriver
    and the app);
  - `afterSession`: `process.kill(-pid, "SIGTERM")`, wait for exit up to 5 s, then `SIGKILL` the
    group;
  - `beforeSession`: log and kill any leftover `WebKitWebDriver` and `target/debug/tauri-app`
    started by this user (`pgrep -f <exact binary path>`), wait for ports 4444 **and** 4445 to
    accept;
  - log each session's start duration (`before` hook time minus `beforeSession` end) as
    `[e2e] session start: N ms`.
- Environment experiment for the 30 s start (Task T7): locally under `xvfb-run` and once on CI,
  measure with each of `NO_AT_BRIDGE=1`, `GTK_USE_PORTAL=0`, `dbus-run-session -- pnpm test` (CI
  step), `WEBKIT_DISABLE_COMPOSITING_MODE=1`; keep only a setting that brings the start under
  5 s, documented in the conf. No `specFileRetries`.

### 3. #181: one blocking thread per repository (D3-A)
- `AppState` gains `gates: Mutex<HashMap<String, Arc<tauri::async_runtime::Mutex<()>>>>`.
  `repo_worker` returns `RepoWorker { handle, gate }` (gate created on first use, removed by
  `close_repo`).
- `call`: `Arc::clone(&self.gate).lock_owned().await`, the guard moved into the `off_runtime`
  closure (a dropped future can't free the gate while the round trip runs). Tokio's mutex is
  FIFO, so calls for one repository keep the order in which they reach the gate; they wait as
  tasks, not threads. Each `call` closure only makes round trips on its own handle and never
  awaits another call, so the gate can't deadlock.
- `signal()` takes no gate. Outside the gate: `cancel_transfer`/`cancel_search`; the search
  generation (`WorkerHandle::claim_search_generation` before `call`, then
  `search_commits_claimed`), so a cancel or newer search cancels a search waiting at the gate;
  and `workdir()` (a stored path) for Open in editor, Reveal and Open terminal. The doc comment
  and ARCHITECTURE "Threading model" explain why one thread per repository is enough (the
  worker is serial).

### 4. #185: post hooks reported and cancellable (D8-A)
- `worker/hooks.rs`: `HookSource { Commit, Checkout, Merge, Pick }`; `HookEvent::{Started,
  Finished}` gain `source`; `ChannelHookReporter` gains `source`. Existing commit/amend/Continue
  reporters use `Commit`.
- Switch (`Command::SwitchBranch`), checkout commit (`Command::CheckoutCommit`) and
  `Command::StartMerge` gain `operation_id` + `events`; handle methods gain `events:
  Sender<HookEvent>` and mint `next_operation_id("switch" | "checkout" | "merge")`. After success,
  `run_post_hook_reported(.., HookOptions { cancel, reporter })`, then `op.clear()`, then the
  reply. A cancel only stops the hook.
- Pull: before `post-merge`, `TransferEvent::Progress { phase: RunningHook, .. }` on the pull's
  operation; the hook runs with `op.cancel_check()`; the outcome is unchanged.
- DTOs: Tauri `HookProgressDto` and sidecar `hookProgress` gain `source` (`"Commit" | "Checkout" |
  "Merge" | "Pick"`). Tauri `switch_branch`, `checkout_commit`, `start_merge` create the event
  channel (`emit_hook_events`); the sidecar's handlers `spawn_hook_relay`.
- Frontend: `HookProgress.source`; `hookRun` keeps it. `CommitBox`/`RebaseProgressPanel` get
  `hookRun` only when `source === "Commit"`. New `HookRunStrip` (role=status, aria-label "Hook
  status") under `OperationStatusStrip` for other sources: `runningHookLabel(hook)` + Cancel
  (reuses `HookStatus`). Cancel stays enabled while `pending`. `TransferPanel` labels
  `RunningHook` by operation: `Pull` → `post-merge`, otherwise `pre-push`.

### 5. #183: clean pick steps (D5-A, D9-A)
- `commit.rs`: `pub(crate) fn run_message_hook(repo, message, source, options) ->
  Result<HookedMessage, CommitError>`: only `prepare-commit-msg <COMMIT_EDITMSG> <source>`
  (written only when installed), the empty-message check, no `pre-commit`/`commit-msg`.
- `pick.rs`: `step(.., hooks: Option<&mut HookOptions>)`. With hooks, a clean non-empty step runs
  `run_message_hook(.., "message", ..)`, commits through `create_commit_reporting` (signing
  reported and cancellable), `cleanup_state`, then `run_post_commit`. `StepResult::HookFailed {
  hook, output }` on a failing or empty-message `prepare-commit-msg`: git's pick state stays
  (`CHERRY_PICK_HEAD`/`REVERT_HEAD`, `MERGE_MSG`, staged result).
  - `cherry_pick`/`revert` gain `hooks`; `PickOutcome::HookFailed { hook, output }`.
  - `cherry_pick_many`/`revert_many` gain `hooks`; `run(repo, state, hooks)`. A `HookFailed` step
    stops as `SequenceOutcome::HookFailed { .. }` with `stopped_on_conflict = true`, so Continue
    commits it through `finish_paused` (full commit hooks). `continue_sequence` passes its hooks to
    `run` for the later steps.
  - Cancel (D9-A): during `prepare-commit-msg` → like a failure but `PickError::Hook(Cancelled)`;
    after a step's `post-commit`, if cancelled → `next += 1`, stop with `PickError::Hook(Cancelled)`
    (state kept; Continue resumes with the next step).
- Worker: `CherryPick`, `RevertCommit`, `CherryPickMany`, `RevertMany` gain `operation_id` +
  `events` (`source: Pick`; mint `"pick"`); Tauri commands and sidecar relay as in §4.
- Frontend: `PickOutcome` `HookFailed` → `hookFailure` with retry kind `"pick"` (no retry button:
  `prepare-commit-msg` can't be skipped); the operation strip shows "Cherry-picking"/"Reverting"
  and the commit box finishes it. `cherryPickMany`/`revertMany` already handle `HookFailed` via
  the Continue path (retry kind `continueSequence`).

### 6. #183: merge commit residuals (D6-A, D7-A)
- `CommitHookSet::CleanMerge { head: Oid, merge_heads: Vec<Oid> }` (no longer `Copy`). The worker
  builds it from `CleanMerge`. `commit_with`: `merge_hooks` only when the repository shows exactly
  that merge before the hooks; after `run_commit_hooks`, re-read `MERGE_HEAD` (and HEAD) and if
  either changed → `CommitError::MergeChanged` ("The merge changed while its hooks ran. Nothing was
  committed; check the merge and commit again."), nothing committed.
- `run_commit_hooks(.., message_file: MessageFile)`: `MessageFile::MergeMsg` (`.git/MERGE_MSG`)
  when `merge_hooks`, else `CommitEditmsg`.
- After the commit landed, `cleanup_state` and `clear_squash_message` failures are logged
  (`eprintln!("browsitory: the commit was created, but …")`) and the result is still
  `Ok(Committed)`; post hooks run. Same in `finish_paused`. The leftover state stays visible in the
  operation strip, whose Abort clears it harmlessly (worktree equals the new HEAD).

### 7. #186: stash (D10-A, D11-A)
- Characterization first (Task D2). Expected (git):
  `git stash push -u -- d` records `d/f`'s deletion and untracked `d`, leaves `d/f` back and `d`
  gone; `git stash pop` reproduces the file `d`.
- `save_path_limited_stash`: the `FolderIsNowAFile` refusal is skipped when the blocking `parent`
  is in `untracked` (it is removed in step 1 before `d/f` is checked out).
- If the D2 pop test shows libgit2 can't apply such a stash, `apply_stash` maps that failure (or
  detects up front: an untracked-tree file whose path is a folder in the stash's base tree that the
  worktree tree removes) to `StashError::FileReplacedFolder` ("This stash can't be applied here
  because a file replaced a folder; use git stash apply from a terminal."), nothing written. The
  stash is kept.
- `save_stash` with `include_untracked = false` (both paths): refuse with
  `StashError::UntrackedInTheWay { untracked, tracked }` ("Stashing would delete the untracked file
  'd', which is in the way of 'd/f'. Turn on Include untracked files, or move 'd' first.") when an
  untracked file sits where HEAD (or with `keep_index`, the index) has a folder, or an untracked
  file lies below a path HEAD has as a file. Only after Task D2 shows the deletion.

### 8. #187: unstage (D12-A)
- `unstage_file`: after `pair_root`, if `root.is_some()` and the index has an entry with stage > 0
  at `unit` or below `unit/` → `StageError::ConflictsBelow(unit)` ("Can't unstage '{0}': the merge
  left conflicts in it. Resolve them first."), before any change.
- The pair-root removals stay in memory (no `index.write()`); `repo.reset_default` uses the same
  repository index and writes once. On error, `index.read(true)` discards the removals.

## Open decisions (provisional choice in **bold**; the plan implements it)

**D1. `/dev/tty` test (#179). Engineering.** **A. Re-run under a pty session.** B. Rely on the
Linux session probe; document. C. `script` utility (Linux/macOS differ). Recommendation **A**.

**D2. Proving the tests can fail (#179). Engineering.** **A. Permanent negative controls.**
B. One-off red CI run. C. Both. Recommendation **A**: the proof stays in the suite.

**D3. #181. Engineering.** **A. Per-repository async gate.** B. Async replies in repo-service
(~99 methods + sidecar). C. Frontend refresh coalescing only. Recommendation **A** (C can follow).

**D4. Session start (#182). Engineering.** **A. Hygiene + diagnostics + measured env change, no
retries.** B. A + `specFileRetries: 1`. C. Retries only. Recommendation **A**.

**D5. Which clean picks run hooks (#183). User-facing.** **A. Every clean pick (single, sequence
start, after Continue): `prepare-commit-msg … message` and `post-commit`.** B. Only steps after
Continue. C. Won't fix; document. Recommendation **A**: B behaves differently within one sequence.

**D6. Message file of a clean merge's commit (#183). User-facing (hook-visible).** **A.
`.git/MERGE_MSG`.** B. Keep `COMMIT_EDITMSG`, document. Recommendation **A**.

**D7. `cleanup_state` failing after the commit landed (#183). User-facing.** **A. Commit counts,
error logged, post hooks run.** B. A + warning in `CommitOutcome` (DTO change). C. Unchanged.
Recommendation **A**.

**D8. Where non-commit hook status shows (#185). User-facing.** **A. "Hook status" strip under the
operation strip; pull in the transfer panel.** B. Modal overlay. C. Commit box only.
Recommendation **A**.

**D9. Cancel during a clean step's hooks (#183). User-facing.** **A. Stop the hook, keep the
commit, stop the sequence after that step.** B. Stop the hook only, continue. Recommendation **A**.

**D10. Stash… without untracked, untracked file in the way (#186). User-facing.** **A. Refuse.**
B. Confirm with a list (dry-run API). C. Document only. Recommendation **A**.

**D11. Path-limited stash of a file that replaced a folder (#186). User-facing.** **A. Allow (as
git); a clear apply/pop error if libgit2 can't apply it.** B. Refuse such saves clearly. C. Own
ordered apply. Recommendation **A**.

**D12. Unstaging with conflicts below a pair root (#187). User-facing.** **A. Refuse + in-memory
removals.** B. git parity (drop conflict stages). C. Refuse only. Recommendation **A**: B silently
discards the conflict sides.

**D13. PRs. Process.** **A. Four.** B. Three (merge PR 3 and 4). C. One. Recommendation **A**.

## Testing

Real temp-dir repositories, never a mocked `Repository`; frontend tests mock `RepoClient`.

- **#179** (`process.rs` inline): Unix marker timeout/cancel ×2 (captured, split), elapsed bounds,
  negative controls (Unix, Windows), pty `/dev/tty` test (Unix).
- **#182**: the E2E suite itself, three full CI runs green without reruns; `[e2e] session start`
  timings recorded in the PR.
- **#181** (`tauri-app` inline, Unix): 600 queued calls on a hook-blocked repository don't delay
  another repository (red today); gate preserves order.
- **#185** (`repo-service` worker tests): `Started`/`Finished` with `source` on switch, checkout,
  fast-forward and squash merge; cancel stops each and keeps the result; pull reports
  `RunningHook` and is cancellable. Tauri DTO serialization; sidecar relay tests. Vitest:
  `HookRunStrip`, routing by source, `TransferPanel` label. E2E: `git-hooks.spec.ts` slow
  `post-checkout` → strip → Cancel.
- **#183** (`git-core` `tests/pick.rs`, `tests/pick_sequence.rs`, `tests/hooks.rs`): hook logs for
  single/sequence/after-Continue picks and reverts; no `pre-commit`/`commit-msg`; a message
  rewritten by `prepare-commit-msg` lands; failure pauses the step; cancel; a **parity test**
  against `git cherry-pick A B C` in a twin repo (hermetic git); `MERGE_MSG` path; `MergeChanged`
  via a `pre-merge-commit` that restarts the merge; `cleanup_state` failure (`chmod 0555 .git`,
  skipped as root). Worker tests for events and the reply.
- **#186/#187** (`git-core` `tests/stash.rs`, `tests/stage_commit.rs`): characterization against
  git CLI twins; refusals leave `.git/index` bytes and the stash list unchanged.

## Docs

- `docs/ARCHITECTURE.md`: "Threading model" (l. 170–186: the gate); "Cherry-pick sequences"
  (l. 360–373: D3-B replaces D3-A); "Git hooks" (l. 485–500: post hooks reported with `source`,
  `MERGE_MSG`, `MergeChanged`, landed-commit cleanup); "Testing strategy" (E2E in-page helpers,
  harness process group).
- `docs/USER_GUIDE.md`: hooks table (l. 206–212: picks), status paragraph (l. 237–242),
  "Stashes" (l. 445–455), staging/unstaging note for conflicts.
- `CHANGELOG.md`: one section per PR (see the plan).

## Risks

- **D5-A** changes behaviour for every cherry-pick and revert in repositories with
  `prepare-commit-msg`/`post-commit` hooks (slower picks; a failing hook now pauses a pick).
- **#185** changes `HookEvent`, both transports' hook DTOs and five worker commands' signatures
  (≈40 test call sites).
- **#182** env experiment is CI-only evidence; a change that helps locally may not on CI.
- **#186** depends on libgit2's pop behaviour, verified only by Task D2.
- **D7-A** leaves a merge/pick state visible after a landed commit in a rare I/O failure.

## Unrelated findings (not fixed here)

1. `pick::step` also returns an error when `cleanup_state` fails after a clean step's commit; the
   sequence then reports `HeadMoved` on Continue (Abort works).
2. git runs `post-checkout` after `git clone`, `git worktree add` and the start of `git rebase`;
   Browsitory doesn't.
3. The VSCode sidecar starts one OS thread per request with no cap; a long hook plus refreshes
   piles up threads there too (no cross-repository starvation).
4. Every Tauri WebDriver session takes ~30 s to start on CI (about 15 min per run), addressed only
   if T7 finds the cause.

## Out of scope

- D3-B (async reply channels), D4-B (spec retries), D11-C (own stash apply), D12-B.
- `post-rewrite` and rebase hooks; the hook items in "Unrelated findings".
