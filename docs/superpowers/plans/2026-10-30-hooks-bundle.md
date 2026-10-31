# Hooks Bundle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close six hook follow-ups:

- **#126:** hooks and signers start in a new session.
- **#122:** a Windows hook's whole process tree dies on timeout or cancel.
- **#124:** a clean merge's commit runs `pre-merge-commit` (and `post-merge` after it).
- **#133:** Continue on a cherry-pick/revert sequence runs the commit hooks on the step it
  commits.
- **#125:** Tauri worker round trips leave the async runtime.
- **#123:** a VSCode E2E for hooks.

**Architecture:**

- In `git-core`:
  - `process.rs` gets `detach` (`setsid`) and `killpg`, plus a new
    `process/job_windows.rs` (kernel32 FFI);
  - `commit.rs` gets `CommitHookSet` and `commit_with`, and its hook helpers become
    `pub(crate)`;
  - `pick::continue_sequence` takes `HookOptions` and gains `SequenceOutcome::HookFailed` and
    `PickError::Hook`;
  - `hooks::SKIPPABLE_HOOKS` and `repo::head_info` know `pre-merge-commit`.
- In `repo-service::worker`:
  - `merge.rs` gets `CleanMerge` memory, and `post-merge` moves to after the commit;
  - `ContinueSequence` carries `no_verify`, an operation id and hook events.
- In `tauri-app`: `RepoWorker::call` / `off_runtime` replace `worker_handle`.
- `vscode-sidecar` relays hook events on Continue.
- In the frontend:
  - a `continueSequence` retry kind;
  - the `HookFailed` sequence outcome;
  - the running-hook line on `RebaseProgressPanel`.
- A VSCode E2E spec.

**Tech Stack:**

- Rust (`git2` 0.21 / libgit2 1.9, `libc` 0.2 on unix, hand-written kernel32 FFI on Windows,
  `tauri::async_runtime`);
- React + Vitest;
- `@vscode/test-electron` + CDP.

**Spec:** `docs/superpowers/specs/2026-10-30-hooks-bundle-design.md`. This plan implements the
**provisional** choice of each decision D1–D8. "Decision hooks" at the end lists what changes
for the other options.

---

## Global Constraints

- **Branch:** `feat/hooks-bundle` (already pushed with the spec and plan). One PR, opened only
  after Task Z2 passes.
- **No new dependency.** `libc` is already in `crates/git-core/Cargo.toml` under
  `[target.'cfg(unix)'.dependencies]`. Task A1 only extends its row in
  `docs/LICENSE_COMPLIANCE.md` (keep `scripts/check-license-compliance.py` green). Windows
  uses hand-written `#[link(name = "kernel32")] extern "system"` declarations, the pattern of
  `crates/git-core/src/worktree_guard/beneath_windows.rs`. **Not** `windows-sys`.
- **Tests use real temp-dir repos** (`tempfile` + `git2::Repository::init`) and real
  `#!/bin/sh` hook scripts, never a mocked `Repository`.
  - git-core integration tests: `mod common;` (it expands `test_support::hermetic_git!()`).
    `common::init_repo()` pins `core.hooksPath` to `<git dir>/hooks` and
    `commit.gpgSign=false`. Use `common::install_hook(&hooks_dir, name, body)`.
  - Worker tests: the `init_repo`/`install_hook` helpers in `worker/mod.rs`'s test module.
  - A new test binary starts with `mod common;` or `test_support::hermetic_git!();`.
- **Platform gating:**
  - Hook tests: `#[cfg(unix)]`.
  - `/proc` session tests: `#[cfg(target_os = "linux")]`.
  - Job-object tests: `#[cfg(windows)]`, printing "no sh.exe found; skipping" and returning
    when `find_sh()` is `None`.
- **Hook logging fixture** (used by B and C): a helper `log_hooks(repo, names)` that installs,
  for each name, `echo "<name> $*" >> "<git dir>/hook-log"` (the git dir is outside the work
  tree, so the log never shows as a change), and `read_log(repo) -> String`. Put it in
  `crates/git-core/tests/common/mod.rs` (`#[cfg(unix)] #[allow(dead_code)]`).
  `prepare-commit-msg` logs its two args; replace the absolute message path with `<msg>` in
  assertions.
- **No CLI git in Rust tests** unless unavoidable. The VSCode E2E uses `git` via `execFileSync`
  as the existing specs do.
- **Never** touch `~/.local/share/com.browsitory.app` or the real config dir. Probes go under
  `target/` and are deleted afterwards.
- **Copy is exact.** New strings live in one place and are asserted verbatim once:
  - "Continue without hooks";
  - `SequenceOutcome::HookFailed`'s banner `hookFailedMessage(hook)`.
- **CHANGELOG:** every push touching `crates/`, `frontend/src/` or `e2e/` needs a `CHANGELOG.md`
  change (pre-push hook). Task A1 adds a stub `### Hooks bundle` under `## [Unreleased]` (above
  `### Rebase/undo safety`), and Task Z1 writes the final text.
- **Commits:** one per task unless noted, message `<area>: <what> (#NNN)`, each ending with the
  session's `Co-Authored-By`/`Claude-Session` trailers.

## Pre-flight

- [ ] `git status` is clean on `feat/hooks-bundle`, rebased on `origin/main` if `main` moved. If
  it did, re-check the line references below.
- [ ] `cargo test -p git-core -p repo-service -p tauri-app -p vscode-sidecar` passes before any
  change. Record the counts in "Pre-flight notes".
- [ ] `cd frontend && pnpm test -- --run` passes. Record the count.
- [ ] Link the pre-push hook (CLAUDE.md "Git hooks") if this worktree doesn't have it.

Reconciled with the code at 5ce5a38d on 2026-10-30:

- `crates/git-core/src/process.rs`:
  - `spawn_retrying` l. 141/158;
  - `kill_tree` l. 168, `kill_and_reap` l. 182;
  - `own_group_without_window` l. 189;
  - `wait_bounded` l. 222;
  - `run_captured` l. 250, `run_captured_split` l. 327;
  - `spawn` (external tools; keeps `process_group(0)`) l. 720;
  - the test module from l. 817 (the `sh()` helper l. 1042).
- `crates/git-core/src/hooks.rs`:
  - `SKIPPABLE_COMMIT_HOOKS` l. 26, `SKIPPABLE_HOOKS` l. 28;
  - `run_hook` l. 202, `run_reported` l. 244.
- `crates/git-core/src/commit.rs`:
  - `gate` l. 236, `run_commit_hooks` l. 271, `run_post_commit` l. 338;
  - `message_source` l. 359, `commit` l. 387, `create_commit_reporting` l. 480.
- `crates/git-core/src/pick.rs`:
  - `SequenceOutcome` l. 74, `PickError` l. 11;
  - `continue_sequence` l. 499, `finish_paused` l. 548.
- `crates/git-core/src/repo.rs`: `head_info` l. 102.
- `crates/repo-service/src/worker/`:
  - `merge.rs` `start` l. 7 (post-merge l. 15–22);
  - `status.rs` `commit` l. 294 and `WorkerHandle::commit` l. 670;
  - `history.rs` `persist` l. 39, `continue_sequence` l. 76 and `WorkerHandle::continue_sequence`
    l. 276;
  - `mod.rs`:
    - `Command::Commit` l. 250, `Command::ContinueSequence` l. 559;
    - worker loop dispatch: Commit l. 1074, StartMerge l. 1238, AbortMerge l. 1252,
      ContinueSequence l. 1304;
    - hook tests around l. 1900–2125.
  - `hooks.rs` `run_post_hook` l. 80.
- `crates/tauri-app/src/commands/`:
  - `mod.rs`: `emit_hook_events` l. 840, `AppState` l. 1659, `scan_repos_in_root` note
    l. 1723, `worker_handle` l. 1917;
  - `status.rs` `commit` l. 319;
  - `history.rs` `continue_sequence` l. 105;
  - `remote.rs` `pull_current_upstream` l. 294.
- `crates/vscode-sidecar/src/dispatch.rs`: `spawn_hook_relay` l. 1564, `commit` l. 1587,
  `continue_sequence` l. 2908.
- Frontend:
  - `src/lib/hookFailure.ts`;
  - `src/state/useHistoryActions.ts` `continueSequence` l. 68;
  - `src/state/useAppState.ts` `retryWithoutHooks` l. 651;
  - `src/App.tsx`: `continueSequence` l. 433, `retryWithCurrentMessage` l. 196;
  - `src/components/DiffPane.tsx` sequence panel l. 1239;
  - `src/components/RebaseProgressPanel.tsx`;
  - `src/components/CommitBox.tsx` hook status l. 375;
  - `src/ipc/RepoClient.ts`: `SequenceOutcome` l. 184, `continueSequence` l. 722;
  - `src/ipc/tauriRepoClient.ts` l. 364, `src/ipc/vscodeRepoClient.ts` l. 444.
- Probe results (git 2.53 hook order; `setsid` vs `process_group`) are in the spec's "Current
  state". The probe sources were deleted.

## Pre-flight notes

- **Baseline** (5ce5a38d + spec/plan, 2026-10-30): `cargo test -p git-core -p repo-service -p
  tauri-app -p vscode-sidecar` 1414 passed, 0 failed, 5 ignored; frontend 1589 tests in 113
  files. `main` had not moved. The pre-push hook was already linked.
- **Toolchain:** rustc 1.97.1, git 2.53.0, Linux. The `x86_64-pc-windows-gnu` std is installed
  but there is no mingw C compiler, so `libgit2-sys` can't build for Windows. Windows code was
  type-checked with clippy through a probe crate under `target/winprobe` that compiles
  `process.rs` + `process/job_windows.rs` alone (deleted afterwards); CI is the real check.
- **Decisions:** all as recommended. The user confirmed D1-A, D3-A, D5-A, D6-A; D2-A, D4-A,
  D7-A, D8-A were taken as recommended.
- **Deviations (code wins over the plan):**
  - A2: `cancel_kills_a_hooks_background_grandchild_on_windows` cancels after 500 ms rather than
    with `|| true`: an immediate cancel can kill `sh.exe` before it has started the background
    job, which would pass without a job object. The three Windows tests ran (green) only on
    `windows-latest` CI; they were written before the implementation but could not be shown red
    there (no scratch PR).
  - B3: `CommitBox` had no merge-state prop. `hooks.merging?: boolean` was added, set by
    `DiffPane` from `headInfo.operation === "Merge"`.
  - C1: `SequenceOutcome::HookFailed` makes the Tauri and sidecar DTO `match`es non-exhaustive,
    so both DTO variants and the Tauri DTO test moved from C2 into C1 to keep every commit
    building. The `CommitError` → `PickError` mapping is a `From` impl.
  - C2: the sidecar had no dispatch-level `hookProgress` test to mirror. `continue_sequence` is
    generic over the writer (like `spawn_hook_relay`) and the new test drives a real worker into
    a buffer. `undo.rs`'s `assert_only_held!` macro builds `{ reply }` only, so
    `ContinueSequence`'s held-reply check is written out by hand.
  - C3: the shared status line is a new `HookStatus` component (`CommitBox` and
    `RebaseProgressPanel`). `App.continueSequence` stays argument-free because it is wired to
    buttons' `onClick` (the click event would otherwise be read as a truthy `noVerify`); the
    dialog's retry calls `runContinueSequence(true)` so its toast is the sequence toast.
    `retrySuccessLabel` takes every retry kind except `continueSequence` (enforced by its type).
  - D1: the saturation tests spawn from the test thread instead of inside `block_on`; the
    stand-in was a test-local inline `off_runtime`/`RepoWorker`, replaced by the real ones.
    `clone_repo` maps `off_runtime`'s error to its existing `CloneErrorDto { kind: "Io" }`.
  - Z1: the `post-merge` timing fix has its own CHANGELOG bullet.
  - E1: the Cancel flow does not assert the "The pre-commit hook was cancelled." banner. A
    successful `refresh()` clears `state.error`, and in VSCode a file-watcher refresh can fire
    right after the cancelled commit, so the banner may already be gone (it failed that way on
    CI and locally). The spec asserts the running-hook line goes away within 20 s (the hook
    sleeps 60 s) and that nothing was committed. It clicks the status line's own button, since
    other panels have "Cancel" buttons too.
  - Review fixes: a refused `StartMerge` keeps the clean-merge memory; the worker's
    `GetHeadInfo` lists only the pre-hook the merge commit will run; a `pre-commit` that
    empties the continued step skips it; `persist`'s save-failure message names a failed hook.

---

## Part A: process layer (#126, #122), git-core only

### Task A1: `setsid` for captured programs, `killpg` for the group kill (#126, D5-A)

**Files:**
- `crates/git-core/src/process.rs`;
- `crates/git-core/tests/hooks.rs`;
- `docs/LICENSE_COMPLIANCE.md` (libc row);
- `CHANGELOG.md` (stub).

- [ ] **Write the failing tests first.**
  - `process.rs` tests, `#[cfg(target_os = "linux")] fn captured_program_leads_its_own_session_without_a_terminal()`:
    - `run_captured(sh(r#"read -r s < /proc/$$/stat; set -- ${s#*) }; echo "$$ $3 $4 $5""#), None, &|| false, 10 s, 1024)`;
    - split the output into `pid pgrp sid tty_nr`;
    - assert `pid == pgrp == sid` and `tty_nr == "0"`;
    - it fails today (sid is the test runner's session).
  - Same for `run_captured_split`:
    `#[cfg(target_os = "linux")] fn split_program_leads_its_own_session_without_a_terminal()`
    (read `stdout`).
  - `#[cfg(unix)] fn a_program_opening_dev_tty_fails_at_once()`:
    - runs `sh("if (: < /dev/tty) 2>/dev/null; then echo tty; else echo none; fi")`;
    - asserts output `"none\n"` and `elapsed < 5 s`;
    - a comment notes that it only discriminates when the runner has a terminal (run it
      locally from one).
  - `tests/hooks.rs`, `#[cfg(unix)] fn a_pre_commit_reading_the_terminal_fails_instead_of_stopping()`:
    - pre-commit body `exec < /dev/tty; read answer; exit 0`;
    - `commit::commit` returns `CommitError::HookFailed { hook: "pre-commit", output }`, with
      `output` containing `/dev/tty`, within 10 s;
    - HEAD is unborn or unchanged.
  - Run `cargo test -p git-core captured_program_leads split_program_leads` and confirm the two
    Linux tests fail.
- [ ] **Implement.**
  - Rename `own_group_without_window` → `detach`. On unix:

    ```rust
    use std::os::unix::process::CommandExt;
    // SAFETY: `setsid` is async-signal-safe; nothing else runs between fork and exec.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
        });
    }
    ```

    Remove `process_group(0)` there. Combined with `setsid` it fails with `EPERM`; the probe
    is in the spec. Windows is unchanged (`CREATE_NO_WINDOW`).
  - `kill_tree` (unix): replace the `Command::new("kill")` spawn with
    `unsafe { libc::killpg(child.id() as libc::pid_t, libc::SIGKILL) };`, keeping
    `child.kill()` after it. Update the doc comment: the child leads its own session and group;
    no `kill` utility.
  - Update the module doc (l. 1–18) and remove the "Known limitation" comment.
- [ ] Existing tests must still pass:
  - `timeout_kills_a_process_whose_grandchild_holds_the_pipe`;
  - `cancel_stops_the_process`;
  - `split_times_out_and_kills_the_group`;
  - `split_cancel_stops_the_program`;
  - `a_hook_without_a_shebang_runs_through_sh` (the ENOEXEC fallback through the fork+exec
    path);
  - the signing tests (`signing_ssh`, `signing_gpg`, `signing_timeout`).
- [ ] `docs/LICENSE_COMPLIANCE.md` libc row: add "`setsid` (hooks and signing programs start in
  a new session, `process.rs`) and `killpg` (timeout/cancel kill their process group)".
  `python3 scripts/check-license-compliance.py` passes.
- [ ] `CHANGELOG.md`: add `### Hooks bundle` under `## [Unreleased]` with a stub bullet.
- [ ] `cargo test -p git-core && cargo clippy -p git-core --all-targets -- -D warnings`.
- [ ] Commit `process: start hooks and signers in a new session (#126)`.

### Task A2: Windows job object for hooks (#122, D6-A)

**Files:**
- new `crates/git-core/src/process/job_windows.rs`;
- `crates/git-core/src/process.rs`.

- [ ] **Write the failing tests first** (`process.rs` tests, all `#[cfg(windows)]`, all skip
  without `find_sh()`). Shared helper `windows_sh(script) -> Command`: `find_sh()` with
  `["-c", script]`, plus env `MARKER=<temp file>`.
  - `fn timeout_kills_a_hooks_background_grandchild_on_windows()`:
    - script `(sleep 3; echo alive > "$MARKER") >/dev/null 2>&1 & sleep 30`, timeout 500 ms;
    - assert `timed_out`;
    - `std::thread::sleep(6 s)`;
    - assert the marker file does **not** exist.
  - `fn cancel_kills_a_hooks_background_grandchild_on_windows()`: the same with `&|| true` and
    a 60 s timeout; assert `RunError::Cancelled` and no marker.
  - `fn a_normal_exit_leaves_a_hooks_background_process_running_on_windows()` (D6-A):
    - script `(sleep 2; echo alive > "$MARKER") >/dev/null 2>&1 & exit 0`;
    - assert success;
    - poll up to 10 s for the marker to **appear**.
  - These run only on `windows-latest` CI. Before pushing, confirm the first two fail there
    on a scratch push or a draft workflow run, or say in the PR that they were written
    test-first but only CI can run them.
- [ ] **Implement `process/job_windows.rs`.**
  - `use std::ffi::c_void; use std::os::windows::io::AsRawHandle; use std::process::Child;`
  - The `#[link(name = "kernel32")] extern "system"` block: `CreateJobObjectW`,
    `AssignProcessToJobObject`, `TerminateJobObject`, `CloseHandle` (signatures in spec §2).
  - `pub(super) struct Job(*mut c_void);`
    - `fn assign(child: &Child) -> Option<Job>`: create with null attributes and name. Null →
      `None`. Assign fails → `CloseHandle` and `None`.
    - `fn terminate(&self)`: `TerminateJobObject(self.0, 1)`, result ignored.
    - `impl Drop` → `CloseHandle`.
  - Module doc:
    - why there is no `KILL_ON_JOB_CLOSE` (D6-A);
    - the residual spawn-to-assign race;
    - why signing is not covered.
- [ ] **Wire it into `process.rs`.**
  - `#[cfg(windows)] mod job_windows;`.
  - `kill_tree(child, job: Option<&Job>)`, `kill_and_reap(child, job)` and
    `wait_bounded(child, cancel, timeout, job)`, with a type alias so unix compiles:
    `#[cfg(windows)] type Job = job_windows::Job; #[cfg(not(windows))] enum Job {}`.
  - `run_captured`: after the spawn,
    `#[cfg(windows)] let job = job_windows::Job::assign(&child);` (unix: `None::<Job>`), and
    pass `job.as_ref()`.
  - `run_captured_split` passes `None` (spec §2: signing keeps today's behaviour).
  - In `kill_tree`, windows: `if let Some(job) = job { job.terminate(); }` before
    `child.kill()`.
- [ ] Linux: `cargo test -p git-core`, `cargo clippy -p git-core --all-targets -- -D warnings`.
  If a Windows target is installed, also `cargo clippy -p git-core --target
  x86_64-pc-windows-gnu`; otherwise rely on CI.
- [ ] Commit `process: kill a Windows hook's process tree through a job object (#122)`.

---

## Part B: `pre-merge-commit` (#124, D1-A, D2-A)

### Task B1: `CommitHookSet::CleanMerge` in git-core

**Files:**
- `crates/git-core/src/hooks.rs`;
- `crates/git-core/src/commit.rs`;
- `crates/git-core/src/repo.rs`;
- `crates/git-core/tests/common/mod.rs` (`log_hooks`, `read_log`);
- `crates/git-core/tests/hooks.rs`.

- [ ] **Write the failing tests first** in `tests/hooks.rs`.
  - Shared fixture: `clean_merge_fixture() -> (TempDir, Repository)`, where `main` and
    `feature` touch different files, and `start_merge("feature", { fast_forward: Never })` has
    returned `Merged`.
  - `fn a_clean_merge_commit_runs_git_merges_hooks()`:
    - log all of `pre-commit, pre-merge-commit, prepare-commit-msg, commit-msg, post-commit`;
    - `commit_with(.., CommitHookSet::CleanMerge, HookOptions::default())`;
    - the log equals `"pre-merge-commit \nprepare-commit-msg <msg> merge\ncommit-msg <msg>\n"`
      (no pre-commit, no post-commit);
    - HEAD has 2 parents; `repo.state()` is Clean.
  - `fn a_failing_pre_merge_commit_keeps_the_merge_staged()`:
    - `pre-merge-commit` is `echo blocked; exit 1`;
    - `Err(CommitError::HookFailed { hook: "pre-merge-commit", output: "blocked\n" })`;
    - `repo.state() == Merge`; HEAD unchanged; `MERGE_HEAD` present.
  - `fn no_verify_skips_pre_merge_commit_and_commit_msg()`:
    - both hooks fail, `prepare-commit-msg` logs;
    - `no_verify: true` → Ok, and the log is only `prepare-commit-msg <msg> merge`.
  - `fn a_pre_merge_commit_that_stages_a_file_gets_it_committed()`: mirror the existing
    `pre_commit_that_stages_a_file_gets_it_committed` with `git add` in `pre-merge-commit`.
  - `fn a_merge_commit_after_conflicts_runs_the_commit_hooks()`:
    - conflicting fixture; resolve and stage;
    - `commit` (hook set `Commit`);
    - log `"pre-commit \nprepare-commit-msg <msg> merge\ncommit-msg <msg>\npost-commit \n"`.
  - `fn the_clean_merge_hook_set_outside_a_merge_runs_the_commit_hooks()`:
    - a plain staged change, `commit_with(CleanMerge)`;
    - log `pre-commit`, `prepare-commit-msg <msg> message`, `commit-msg`, `post-commit`.
  - `fn head_info_lists_pre_merge_commit_only_while_merging()`:
    - an executable `pre-merge-commit` only;
    - `head_info(..).commit_hooks` is `[]` when clean and `["pre-merge-commit"]` after
      `start_merge` → `Merged`.
  - `hooks.rs` unit test `fn no_verify_skips_pre_merge_commit()`:
    `SKIPPABLE_HOOKS.contains(&"pre-merge-commit")`.
- [ ] **Implement.**
  - `hooks.rs`: `SKIPPABLE_HOOKS: [&str; 4] = ["pre-commit", "pre-merge-commit", "commit-msg",
    "pre-push"]`. Update the doc comments of both consts.
  - `commit.rs`:
    - add `CommitHookSet` (spec §3);
    - `run_commit_hooks(repo, message, pre_hook: &str, source, options)`, where `pre_hook` is
      `"pre-commit"` or `"pre-merge-commit"` (`amend_head` passes `"pre-commit"`);
    - `commit_with(repo, message, set, hooks)` holds today's `commit` body. It computes
      `let merge_hooks = set == CommitHookSet::CleanMerge && is_merging;`, passes the pre-hook
      name, and skips `run_post_commit` when `merge_hooks`;
    - `pub fn commit(repo, message, hooks)` = `commit_with(repo, message, CommitHookSet::Commit,
      hooks)`;
    - doc comment: the clean-merge set mirrors `git merge`; the worker runs `post-merge`.
  - `repo.rs::head_info`: build `names` from `SKIPPABLE_COMMIT_HOOKS`, and push
    `"pre-merge-commit"` when `repo.state() == RepositoryState::Merge`.
- [ ] `cargo test -p git-core --test hooks` and `cargo test -p git-core`.
- [ ] Commit `commit: run git merge's hooks when concluding a clean merge (#124)`.

### Task B2: worker remembers a clean merge; `post-merge` after its commit

**Files:**
- `crates/repo-service/src/worker/merge.rs`;
- `crates/repo-service/src/worker/status.rs`;
- `crates/repo-service/src/worker/mod.rs` (loop state and tests).

- [ ] **Write the failing tests first** (`worker/mod.rs` tests, `#[cfg(unix)]`):
  - `fn a_staged_merge_runs_no_post_merge_until_it_is_committed()`:
    - `start_merge(feature, Never)` → `Merged`;
    - the `post-merge` record file does not exist;
    - `commit(..)` → `Committed`;
    - the record is `"0\n"`.
  - `fn a_clean_merge_commit_runs_pre_merge_commit_not_pre_commit()`:
    - `pre-commit` and `pre-merge-commit` both log to one file; after Merge and then Commit it
      is `"pre-merge-commit\n"`;
    - `HookEvent::Started` names `pre-merge-commit`.
  - `fn a_failing_pre_merge_commit_replies_hook_failed_and_commit_without_hooks_lands_the_merge()`:
    - first commit → `CommitOutcome::HookFailed { hook: "pre-merge-commit", .. }`;
    - `get_head_info().operation == Some(Merge)`;
    - commit with `no_verify: true` → `Committed`, two parents, `post-merge` record `"0\n"`.
  - `fn a_merge_commit_after_conflicts_runs_post_commit_not_post_merge()`: conflict, resolve,
    commit; the `post-commit` record exists, the `post-merge` record does not.
  - `fn a_clean_merge_committed_after_a_worker_restart_runs_the_commit_hooks()` (D2-A fallback):
    - Merge with worker 1; drop it; spawn worker 2;
    - Commit → the log is `"pre-commit\n"`, and there is no `post-merge` record.
  - Update `post_merge_runs_after_a_merge_and_a_squash` → rename to
    `post_merge_runs_after_a_fast_forward_and_a_squash` (body unchanged; it never covered
    `Merged`).
- [ ] **Implement.**
  - `worker/merge.rs`:
    - `#[derive(Debug, Clone, PartialEq, Eq)] pub(super) struct CleanMerge { head: git2::Oid,
      merge_heads: Vec<git2::Oid> }`;
    - `fn capture(repo: &mut Repository) -> Option<Self>` (`mergehead_foreach`; `None` unless
      `state == Merge`);
    - `fn matches(&self, repo: &mut Repository) -> bool`.
  - `merge::start`: drop the `Merged` arm from the post-merge match. Return the outcome so the
    loop can capture.
  - Loop state `let mut clean_merge: Option<merge::CleanMerge> = None;` in `mod.rs`:
    - `StartMerge`: `Ok(Merged)` → `CleanMerge::capture(&mut repo)`; any other result → `None`;
    - `AbortMerge` → `None`.
    - `Commit`: `set = if clean_merge.as_ref().is_some_and(|m| m.matches(&mut repo)) {
      CleanMerge } else { Commit }`, passed to `status::commit`. On `Ok(Committed)` with
      `CleanMerge` → `hooks::run_post_hook(&repo, "post-merge", &["0"])`. After any successful
      commit → `clean_merge = None`.
  - `status::commit` gains `set: git_core::commit::CommitHookSet` and calls
    `git_core::commit::commit_with`.
  - Doc comment on the loop field (spec §3, D2-A fallback).
- [ ] `cargo test -p repo-service`.
- [ ] Commit `worker: run post-merge after a clean merge's commit, not when it is staged (#124)`.

### Task B3: frontend knows `pre-merge-commit`

**Files:**
- `frontend/src/lib/hookFailure.ts` and `hookFailure.test.ts`;
- `frontend/src/components/CommitBox.tsx` and `CommitBox.test.tsx`.

- [ ] **Tests first.**
  - `hookFailure.test.ts`: `it("offers Commit without hooks for a failed pre-merge-commit")`.
  - `CommitBox.test.tsx`:
    `it("names pre-merge-commit in the Skip hooks tooltip while a merge is staged")`. The
    title is "Skip the pre-merge-commit and commit-msg hooks for the next commit
    (--no-verify)" when `headInfo.operation === "Merge"`, else today's text.
- [ ] **Implement.**
  - `SKIPPABLE_HOOKS` gains `"pre-merge-commit"`; the comment still mirrors Rust.
  - CommitBox title: `merging ? "pre-merge-commit" : "pre-commit"`, from the prop the box
    already has for the merge state. If it has none, pass `operation` through `DiffPane`.
- [ ] `cd frontend && pnpm lint && pnpm test -- --run`.
- [ ] Commit `frontend: pre-merge-commit is skippable (#124)`.

---

## Part C: commit hooks on sequence Continue (#133, D3-A, D4-A)

### Task C1: `continue_sequence` runs the commit hooks on the resolved step

**Files:**
- `crates/git-core/src/commit.rs` (`pub(crate)` helpers);
- `crates/git-core/src/pick.rs`;
- `crates/git-core/tests/pick_sequence.rs`.

Callers of `continue_sequence` to update: `crates/git-core/tests/pick_sequence.rs` and
`crates/repo-service/src/worker/history.rs`, both with `HookOptions::default()` for now.

- [ ] **Write the failing tests first** (`tests/pick_sequence.rs`, `#[cfg(unix)]` for the hook
  ones). Use the existing `side_branch`/`paused` helpers: a three-commit sequence whose
  middle commit conflicts.
  - `fn continue_runs_the_commit_hooks_once_on_the_resolved_step()`:
    - log all five commit hooks;
    - pause on step 2, resolve, stage;
    - `continue_sequence(&repo, state, HookOptions::default())` → `Completed { applied: 3 }`;
    - the log is exactly `"pre-commit \nprepare-commit-msg <msg> merge\ncommit-msg <msg>\npost-commit \n"`
      (the clean steps 1 and 3 ran none).
  - `fn a_commit_msg_hook_rewrites_the_continued_steps_message()`:
    - `commit-msg` appends `Change-Id: I123` to `$1`;
    - after Continue, the step-2 commit message ends with `Change-Id: I123`, and steps 1 and 3
      don't have it.
  - `fn a_failing_commit_msg_keeps_the_step_paused_and_the_state_unchanged()`:
    - `commit-msg` is `echo no change id; exit 1`;
    - outcome `Ok(SequenceOutcome::HookFailed { hook: "commit-msg", output: "no change id\n",
      progress })` with `progress.current_step == 2`;
    - `run.state == Some(state_before)`;
    - `repo.state() == CherryPick`; HEAD == `expected_head`; the index has no conflicts.
    - Then `continue_sequence(.., HookOptions { no_verify: true, ..Default::default() })` →
      `Completed`.
  - `fn continue_with_no_verify_skips_pre_commit_and_commit_msg()`: both fail; the log is only
    `prepare-commit-msg` and `post-commit`.
  - `fn a_pre_commit_that_stages_a_file_is_part_of_the_continued_step()`: `pre-commit` writes
    and `git add`s `extra.txt`; the step-2 commit's tree contains it.
  - `fn cancelling_a_continue_hook_keeps_the_sequence_paused()`:
    - `pre-commit` is `sleep 30`; `cancel` is `|| true`;
    - `Err(PickError::Hook(HookError::Cancelled { .. }))`, `state` is `Some` and unchanged,
      `repo.state() == CherryPick`, elapsed < 20 s.
  - `fn an_empty_resolution_is_skipped_without_running_hooks()`: resolve to HEAD's side (the
    existing `whole_file_ours_empties_the_step_and_it_is_skipped` setup); `pre-commit` fails;
    → `Completed` with the step in `skipped`, and the log is empty.
  - `fn a_revert_sequence_continue_runs_the_commit_hooks()`: mirror the first test with
    `revert_many`.
  - Mechanically update every existing `continue_sequence(&repo, state)` call to pass
    `HookOptions::default()`.
- [ ] **Implement.**
  - `commit.rs`: make `run_commit_hooks`, `HookedMessage`, `run_post_commit` and
    `create_commit_reporting` `pub(crate)`.
  - `pick.rs`:
    - `PickError::Hook(#[from] crate::hooks::HookError)` with `#[error("{0}")]`.
    - `SequenceOutcome::HookFailed { hook: String, output: String, progress: SequenceProgress }`,
      with a doc comment: the step is still paused and the state unchanged.
    - `continue_sequence(repo, state, mut hooks: HookOptions<'_>)`: thread `&mut hooks` into
      `finish_paused`.
    - `finish_paused`:
      - after the existing empty check (unchanged position: before any hook);
      - `let hooked = match commit::run_commit_hooks(repo, &message, "pre-commit", "merge",
        &mut hooks)`, where `Err(CommitError::HookFailed { hook, output })` →
        `return Ok(Paused::HookFailed { hook, output })` (a new private enum, so `next` is
        not advanced);
      - other `CommitError`s (from the hooks or from `create_commit_reporting`) map as
        follows: `Hook(e)` → `PickError::Hook(e)`, `Sign(e)` → `PickError::Sign(e)`,
        `Write(e)` → `PickError::Git(e)`, anything else (`Io`, `EmptyMessage`) → a new
        `PickError::Commit(crate::commit::CommitError)` (`#[error("{0}")]`). All of them come
        back through `SequenceRun::stopped` with the state unchanged.
      - Then: reload the index if `hooked.reload_index`, `write_tree`, and commit with
        `commit::create_commit_reporting(repo, &mut hooks, HeadUpdate { expected:
        Some(head.id()), reflog_operation: "commit" }, &author, &committer, &hooked.message,
        &tree, &[&head])`; then `cleanup_state`, `commit::run_post_commit(repo, &mut hooks)`,
        and `state.next += 1`.
    - In `continue_sequence`, map `Paused::HookFailed` to `SequenceRun { outcome:
      Ok(SequenceOutcome::HookFailed { hook, output, progress: state.progress() }), state:
      Some(state) }`.
    - Update the doc comments of `cherry_pick_many` ("steps run no commit hooks") and
      `continue_sequence` to the D3-A rule.
  - `worker/history.rs::continue_sequence`: pass `HookOptions::default()` (Task C2 wires the
    real options).
- [ ] `cargo test -p git-core --test pick_sequence`, then `cargo test -p git-core -p
  repo-service`.
- [ ] Commit `pick: run the commit hooks on the step Continue commits (#133)`.

### Task C2: worker, Tauri and sidecar plumbing for Continue hooks

**Files:**
- `crates/repo-service/src/worker/{mod.rs,history.rs,undo.rs}`;
- `crates/tauri-app/src/commands/{history.rs,mod.rs}`;
- `crates/vscode-sidecar/src/dispatch.rs`.

- [ ] **Write the failing tests first.**
  - `worker/mod.rs`, `#[cfg(unix)] fn continue_sequence_reports_hook_progress_and_a_failure_keeps_the_stored_sequence()`:
    - pause a sequence through the worker, resolve and stage; `commit-msg` fails;
    - `handle.continue_sequence(false, events_tx)` → `HookFailed { hook: "commit-msg", .. }`;
    - the events are exactly `Started`/`Finished` for `commit-msg` (the only installed hook),
      both with the same `continue-` id;
    - `get_head_info().sequence` is `Some` with `current_step == 2`;
    - the undo stack is unchanged.
  - `#[cfg(unix)] fn cancel_transfer_stops_a_continue_hook()`: mirror
    `cancel_transfer_stops_a_running_pre_commit` with `continue_sequence`. The error is "The
    pre-commit hook was cancelled.", and the sequence is still stored.
  - `#[cfg(unix)] fn continue_without_hooks_after_a_restart_finishes_the_sequence()` (#169):
    - HookFailed with worker 1; drop it; spawn worker 2;
    - `continue_sequence(true, ..)` → `Completed`; `sequence.json` is gone.
  - `tauri-app` DTO test
    `fn sequence_outcome_hook_failed_serializes_with_hook_output_and_progress()`: the JSON is
    `{"kind":"HookFailed","hook":"commit-msg","output":"x","progress":{…}}` in the existing DTO
    test style (`commands/mod.rs` tests).
  - `vscode-sidecar` dispatch test
    `fn continue_sequence_accepts_no_verify_and_relays_hook_progress()`, in the style of the
    existing `commit` hookProgress dispatch test (search `hookProgress` in `dispatch.rs`
    tests). If none exists, assert the `HookFailed` JSON and that `noVerify: true` lets
    Continue complete.
- [ ] **Implement.**
  - `Command::ContinueSequence { no_verify: bool, operation_id: String, events:
    Sender<HookEvent>, reply }`.
  - `WorkerHandle::continue_sequence(&self, no_verify, events) -> Result<SequenceOutcome,
    String>`, with id `next_operation_id("continue")`.
  - The loop arm builds `remote::TransferOp { id, cancelled }`, a `ChannelHookReporter`, and
    `HookOptions { no_verify, cancel: Some(&cancel), reporter: Some(&mut reporter),
    ..Default::default() }` for `history::continue_sequence`, then `op.clear()`.
  - Fix the pattern matches in `undo.rs` (`hold_reply`, `locks_state`) and `gate.rs`
    (destructuring `{ reply }` → `{ reply, .. }`).
  - Tauri `continue_sequence(repo_path, no_verify: Option<bool>, app: AppHandle, state)`:
    channel, `emit_hook_events(app, repo_path.clone(), event_rx)`, and
    `.continue_sequence(no_verify.unwrap_or(false), event_tx)`. `SequenceOutcomeDto` gains
    `HookFailed { hook, output, progress }`.
  - Sidecar: `ContinueSequenceParams { repo_path, #[serde(default)] no_verify }` and
    `spawn_hook_relay`; DTO variant as in Tauri.
- [ ] `cargo test -p repo-service -p tauri-app -p vscode-sidecar`.
- [ ] Commit `worker: Continue reports and cancels hooks, and can skip them (#133)`.

### Task C3: frontend: HookFailed on Continue, "Continue without hooks", running-hook line

**Files:**
- `frontend/src/ipc/{RepoClient.ts,tauriRepoClient.ts,vscodeRepoClient.ts}` and their tests;
- `frontend/src/lib/hookFailure.ts` and its test;
- `frontend/src/state/{useHistoryActions.ts,useAppState.ts}`;
- `frontend/src/App.tsx` and `App.test.tsx`;
- `frontend/src/components/{RebaseProgressPanel.tsx,DiffPane.tsx}`;
- `RebaseProgressPanel.test.tsx`.

- [ ] **Tests first.**
  - `tauriRepoClient.test.ts`: `continueSequence("/r", true)` invokes `continue_sequence` with
    `{ repoPath: "/r", noVerify: true }`, and a default call sends `noVerify: false`. Update
    l. 450.
  - `vscodeRepoClient.test.ts` l. 146: the table row expects `{ repoPath: "/r", noVerify:
    false }`. Add a row for `true`.
  - `hookFailure.test.ts`:
    - `it("offers Continue without hooks after a failed hook on Continue")`, for
      `retry: { kind: "continueSequence" }` and hook `commit-msg`;
    - `it("offers no retry for a failed prepare-commit-msg on Continue")` → `null`.
  - `RebaseProgressPanel.test.tsx`:
    `it("shows the running hook with a Cancel button that calls onCancel")`. Props
    `hooks={{ running: { operationId: "continue-1", hook: "pre-commit" }, onCancel }}`; the
    text is "Running pre-commit hook…"; clicking Cancel calls `onCancel` once.
  - `App.test.tsx`:
    `it("a failing hook on Continue opens the hook dialog and Continue without hooks finishes the sequence")`:
    - the mocked `continueSequence` resolves `HookFailed` first, then `Completed`;
    - the dialog `pre-commit hook failed` shows the output;
    - clicking "Continue without hooks" calls `continueSequence(repo, true)`;
    - the dialog closes and the sequence toast shows.
- [ ] **Implement.**
  - `RepoClient.ts`:
    - `SequenceOutcome` gains `| { kind: "HookFailed"; hook: string; output: string; progress:
      SequenceProgress }`;
    - `continueSequence(repoPath: string, noVerify?: boolean)`, documenting that hook progress
      arrives on `subscribeHookProgress`.
  - Both clients send `noVerify: noVerify ?? false`.
  - `hookFailure.ts`:
    - `HookRetry` gains `| { kind: "continueSequence" }`;
    - `retryLabel` returns `"Continue without hooks"` for it;
    - the retry path shows the same toast as a normal Continue (`sequenceToast`). If
      `retrySuccessLabel` must handle every kind, route the `continueSequence` kind through
      App's continue handler instead of a fixed label.
  - `useHistoryActions.continueSequence(noVerify = false)`:
    - inside the mutation, `HookFailed` → set `hookFailure { hook, output, retry: { kind:
      "continueSequence" }, dialogOpen: true }` and `throw new Error(hookFailedMessage(hook))`;
    - success → `hookFailure: null`;
    - always → `hookRun: null` (as `commit`).
  - `useAppState.retryWithoutHooks`: the `continueSequence` kind → `continueSequence(true)`.
  - `App.tsx`: `retryWithCurrentMessage` passes `continueSequence` retries straight through, and
    the `continueSequence` toast handler ignores `HookFailed` (no toast; the banner and dialog
    show it).
  - `RebaseProgressPanel`: an optional `hooks?: { running: { operationId: string; hook: string }
    | null; onCancel(): void }` renders the same `<p role="status">` line as `CommitBox`
    (`runningHookLabel`, Cancel and "Cancelling…"). If the markup is shared, extract a
    `HookStatus` component from `CommitBox` and use it in both.
  - `DiffPane` passes `commitHooks.running` / `commitHooks.onCancel` to the sequence variant of
    `RebaseProgressPanel` (not the rebase variant).
- [ ] `cd frontend && pnpm lint && pnpm test -- --run && pnpm build`. Then
  `cd ../extension && pnpm test -- --run` (the extension shares the RepoClient types).
- [ ] Commit `frontend: hook failures on Continue offer Continue without hooks (#133)`.

---

## Part D: Tauri commands leave the async runtime (#125, D7-A)

### Task D1: `off_runtime` and `RepoWorker::call`

**Files:**
- `crates/tauri-app/src/commands/mod.rs` (helpers and tests);
- every `crates/tauri-app/src/commands/*.rs` that calls `worker_handle` (about 110 call
  sites).

- [ ] **Write the failing tests first** (`commands/mod.rs` tests).
  - `fn off_runtime_keeps_async_runtime_threads_free()`:
    - `let n = std::thread::available_parallelism().map_or(4, |n| n.get()) + 2;`
    - in `tauri::async_runtime::block_on`, spawn `n` tasks
      `off_runtime(move || release_rx.lock().unwrap().recv())`, sharing one
      `Arc<Mutex<Receiver>>`, or one channel per task;
    - then `let probe = tauri::async_runtime::spawn(async { 7 });`. Await it from a helper
      `std::thread` that calls `block_on(probe)` and sends the result on an `mpsc` channel.
      Assert `recv_timeout(5 s) == Ok(7)` (no `tokio` dependency needed);
    - finally release all blockers and await them.
    - Write it first against a stand-in `async fn off_runtime_inline<T>(f) -> T { f() }` to see
      it fail (the probe times out), then replace it with the real helper.
  - `#[cfg(unix)] fn repo_worker_call_does_not_hold_a_runtime_thread_while_a_hook_runs()`:
    - a real `Worker` on a temp repo whose `pre-commit` polls for `<git dir>/release`;
    - spawn `n` `RepoWorker(handle.clone()).call(|w| w.get_status())` jobs plus one `call` that
      commits;
    - assert a plain async probe completes within 5 s;
    - create the release file; all jobs finish.
- [ ] **Implement** (spec §5).
  - `off_runtime`:
    `tauri::async_runtime::spawn_blocking(f).await.map_err(|_| "worker task stopped".to_string())`.
  - `RepoWorker`, `repo_worker(state, repo_path)` (the old `worker_handle` lookup), `call`, and
    `signal`.
  - Delete `worker_handle`. Convert every command mechanically:
    `worker_handle(&state, &repo_path)?.x(args)` →
    `repo_worker(&state, &repo_path)?.call(move |w| w.x(args)).await`.
    - Owned arguments move into the closure. Convert DTOs (`Into`) before the closure, so
      that nothing borrowed crosses it.
    - Event channels: create them before `call`, and move the `Sender` in.
  - `cancel_transfer`, `cancel_search` → `repo_worker(..)?.signal().cancel_…(..)` (no round
    trip).
  - `pull_current_upstream`, `clone_repo`, `open_diff_tool`, `open_merge_tool`: their
    `spawn_blocking` → `off_runtime`. Keep `scan_repos_in_root` as is.
  - Update the module doc next to `worker_handle`'s old place: why every round trip leaves the
    runtime (#125), and why `scan_repos_in_root` doesn't need to.
- [ ] `cargo test -p tauri-app && cargo test -p tauri-app --features forge-fixture-override &&
  cargo clippy -p tauri-app --all-targets -- -D warnings`.
- [ ] `grep -rn "worker_handle" crates/tauri-app/src` → no matches.
- [ ] Commit `tauri-app: run worker round trips off the async runtime (#125)`.

---

## Part E: VSCode E2E (#123, D8-A)

### Task E1: `extension/e2e/src/specs/git-hooks.spec.ts`

**Files:** new `extension/e2e/src/specs/git-hooks.spec.ts`.

- [ ] Write the spec (spec §6), modelled on `commit-signing.spec.ts`:
  - `describe("Browsitory VSCode extension git hooks")`.
  - `before`: `if (process.platform === "win32") this.skip();`. Use `cwd` = workspace folder;
    `hooksDir = path.resolve(cwd, git("rev-parse", "--git-path", "hooks"))`.
  - Helper `installHook(body)` writes `pre-commit` with `#!/bin/sh\n${body}\n` and mode 0o755.
  - `it("shows a failing pre-commit's output and commits without hooks")`, timeout 120 s:
    - hook `sleep 3; echo "vscode-e2e-hook: lint failed" >&2; exit 1`;
    - write `vscode-hooks.txt`; `closeAllEditors`; `browsitory.open`; `connectToWebview`;
    - `waitForText("label", "Skip hooks")`;
    - `waitForText("span", "vscode-hooks.txt (New)")`;
    - `clickByText("button", "Stage all")`;
    - `fill('textarea[aria-label="Commit message"]', SUMMARY)`;
    - `clickByText("button", "Commit")`;
    - `waitForText("p", "Running pre-commit hook")`;
    - `waitForSelector('dialog[aria-label="pre-commit hook failed"]')`;
    - `waitForText("pre", "vscode-e2e-hook: lint failed")`;
    - assert `git log -1 --format=%s` !== SUMMARY;
    - `clickByText("button", "Commit without hooks")`;
    - poll until `git log -1 --format=%s` === SUMMARY.
  - `it("Cancel stops a running pre-commit")`:
    - hook `sleep 60`; write `vscode-hooks-cancel.txt`; stage; fill; Commit;
    - `waitForText("p", "Running pre-commit hook")`;
    - `clickByText("button", "Cancel")`;
    - `waitForText("*", "The pre-commit hook was cancelled.", 20000)` (or the banner's tag, as
      rendered by `InlineError`; check the markup);
    - assert no commit with that summary.
  - `finally` in each test:
    - `webview.close()`; remove the hook;
    - if the commit didn't land, `git reset -q -- <file>` (try/catch) and delete the file;
    - `closeAllEditors`.
- [ ] Run the VSCode E2E locally in CLAUDE.md order (Task Z2 block). Add `xvfb-run -a` when
  headless.
- [ ] `cd extension/e2e && pnpm exec tsc --noEmit -p .` (the CI typecheck step).
- [ ] Commit `extension e2e: hook failure, retry and cancel over the sidecar (#123)`.

---

## Part Z: docs, changelog, gate

### Task Z1: Docs and changelog

- [ ] `CHANGELOG.md` → `## [Unreleased]` → `### Hooks bundle`, replacing the stub:
  - `Merging a branch now runs the pre-merge-commit hook (skipped by Skip hooks and "Commit without hooks") when you commit a merge that had no conflicts, and post-merge runs after that commit instead of when the merge is staged. A merge that had conflicts runs the commit hooks, as in git (#124).`
  - `Continue on a paused multi-commit cherry-pick or revert runs pre-commit, prepare-commit-msg, commit-msg and post-commit for the step it commits, so a Gerrit Change-Id hook works. A failing hook keeps the step paused and offers "Continue without hooks" (#133).`
  - `Fixed: started from a terminal, a hook (or signing program) that reads the terminal no longer hangs until Cancel or the time limit; it fails at once, as when Browsitory is started from the desktop (#126).`
  - `Fixed (Windows): Cancel and the time limit now also stop the programs a hook started, not just its shell (#122).`
  - `Fixed (desktop app): a long-running hook, signing or transfer could stall other actions, including Cancel, across all open repositories (#125).`
  - `Tests: the VSCode extension's E2E covers a failing hook, "Commit without hooks" and Cancel (#123).`
- [ ] `docs/ARCHITECTURE.md`:
  - "Git hooks" (l. ~418–455):
    - session instead of background group; `killpg`;
    - the Windows job object (D6-A, residual race, signing excluded);
    - the clean-merge rule and `post-merge` timing (D1-A, D2-A);
    - Continue hooks (D3-A), with the `continue-<n>` ids.
  - Signing bullet (l. ~269): replace the `SIGTTIN` sub-bullet.
  - "Cherry-pick sequences": `SequenceOutcome::HookFailed` keeps the state; the lock is held
    across hooks.
  - Tauri command layer: `RepoWorker::call` / `off_runtime` (#125).
- [ ] `docs/USER_GUIDE.md` "Git hooks" (l. ~195–247):
  - add a `pre-merge-commit` row to the table ("commit a merge that had no conflicts");
  - `post-merge`: "after a merge commit, a fast-forward or a squash, or a pull with a
    fast-forward";
  - replace the `/dev/tty` paragraph: "Hooks cannot ask you anything: … A hook that tries to
    read from the terminal fails.";
  - Windows: "Cancel and the time limit stop the hook and the programs it started.";
  - replace the last paragraph with the D3-A rule and "Continue without hooks".
- [ ] Commit `Docs and changelog for the hooks bundle (#122, #123, #124, #125, #126, #133)`.

### Task Z2: Full gate (CLAUDE.md order), then PR

Run everything from the repo root unless a `cd` is shown. Every step must be green before the
next.

- [ ] `cargo build --workspace`
- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy -p tauri-app --features forge-fixture-override --all-targets -- -D warnings && cargo test -p tauri-app --features forge-fixture-override`
- [ ] `cd frontend && pnpm install && pnpm build && pnpm lint && pnpm test -- --run && cd ..`
- [ ] `cd extension && pnpm install && pnpm test -- --run && pnpm run compile && cd ..`
- [ ] **Tauri E2E** (exact CLAUDE.md order; put `xvfb-run -a` in front of `pnpm test` when
  headless):
  ```bash
  cargo install tauri-driver --locked   # once, if missing
  cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
  cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
  cd e2e && pnpm install && pnpm test && cd ..
  ```
  The whole suite must pass: it is the regression check for the `RepoWorker::call` refactor.
  `git-hooks.spec.ts`, `cherry-pick-sequence.spec.ts`, `merge.spec.ts` and
  `workspaces.spec.ts` (the `scan_repos_in_root` timing case) need particular attention.
- [ ] **VSCode E2E** (exact CLAUDE.md order):
  ```bash
  cargo build --workspace
  cd frontend && pnpm run generate:release-notes && VITE_E2E_REPO_PATH=/tmp/browsitory-vscode-e2e-repo pnpm exec vite build --config vite.vscode.config.ts && cd ..
  cd extension && pnpm install && pnpm run compile && cd ..
  cd extension/e2e && pnpm install --ignore-workspace && pnpm test && cd ../..
  ```
  `git-hooks.spec.ts` must pass, and the other specs must still pass after it (shared fixture
  repo).
- [ ] `python3 scripts/check-changelog.py --base origin/main` and `python3
  scripts/check-license-compliance.py` pass, and the `### Hooks bundle` entry is present.
- [ ] Manual smoke (`cargo tauri dev` from `crates/tauri-app`, which is itself a terminal
  launch; on a scratch repo, never the real data dir):
  1. A `pre-commit` with `exec < /dev/tty; read x`: Commit fails within a second, the dialog
     shows "/dev/tty: No such device or address", and "Commit without hooks" lands it.
  2. Merge a diverged branch with a logging `pre-merge-commit` and `post-merge`: nothing is
     logged at Merge; at Commit, `pre-merge-commit` runs, then `post-merge 0`.
  3. A three-commit cherry-pick with a conflict in the middle and a failing `commit-msg`:
     Continue shows the dialog, the panel still says step 2, and "Continue without hooks"
     finishes it.
  4. While a `sleep 120` `pre-commit` runs, click around (refresh, switch repository tab):
     the UI stays responsive, and Cancel stops the hook at once.
- [ ] Push. Open the PR with:
  - D1–D8 and their answers;
  - the Windows CI result for the A2 tests;
  - "Closes #122, closes #123, closes #124, closes #125, closes #126, closes #133";
  - the spec's unrelated findings.

---

## Decision hooks

| Decision | If the user picks… | Change |
|---|---|---|
| D1-B | `pre-merge-commit` inside Merge | Drop B1's `CommitHookSet` and B2's memory. `merge::start_merge` takes `HookOptions` and runs `pre-merge-commit` after a clean `repo.merge` (index reload). A failure is a new `MergeOutcome::HookFailed { hook, output }` with the merge left staged. `StartMerge` gains `no_verify`/`operation_id`/`events`, the DTOs and clients a `noVerify`, and `HookRetry` a `{ kind: "merge"; branchName; options }` with "Merge without hooks" (which keeps the staged merge rather than redoing it). `post-merge` stays at `Merged` (or is dropped, see finding 1). Tests: `a_failing_pre_merge_commit_leaves_the_merge_staged` (git-core), a worker HookFailed test, App.test. |
| D1-C | auto-commit clean merges | A larger task: `start_merge` creates the commit with the dialog's message through `commit_with(CleanMerge)`. MergeDialog gains a message textarea (pre-filled with `merge_message`) and **Skip hooks**, and the outcome `Merged` becomes `Committed { commitId }`. The undo label and the merge E2E change. Not recommended in this batch. |
| D1-D | `pre-merge-commit` for every merge commit | B1: `commit` uses the merge pre-hook whenever `is_merging`. B2: no memory; `post-merge` after every merge commit. Drop the after-conflicts and restart tests, and add `a_merge_commit_after_conflicts_runs_pre_merge_commit`. |
| D2-B | persisted memory | B2: new `worker/merge_store.rs` (`<gitdir>/browsitory/merge.json`, `write_atomically`, version 1, `{ head, mergeHeads }`), saved on `Merged` and cleared on commit/abort/mismatch. Undo/redo leave it alone (it is validated on use). The restart test flips to `a_clean_merge_committed_after_a_worker_restart_still_runs_pre_merge_commit`. |
| D2-C | infer from `MERGE_MSG` | B2: `CleanMerge` becomes `!merge_msg.contains("\n#Conflicts:") && !merge_msg.contains("\n# Conflicts:")`; no loop state. Add a test for a terminal-started `--no-commit` merge (documents the deviation). |
| D3-B | full parity | C1 also: `step()` takes `&mut HookOptions` and, for a clean result, runs `prepare-commit-msg <file> message` (through `COMMIT_EDITMSG`) before the commit and `post-commit` after. A failing `prepare-commit-msg` leaves the step staged (`CHERRY_PICK_HEAD`, no conflicts), stops with `HookFailed` and `stopped_on_conflict: true` (so Continue commits it). `cherry_pick`/`revert`/`cherry_pick_many`/`revert_many` take `HookOptions`. `CherryPick`/`RevertCommit`/`CherryPickMany`/`RevertMany` get ids and events, plus frontend progress on the graph actions. Many more tests; a separate PR is recommended. |
| D3-C | post-commit on clean steps | C1: `step()` calls `commit::run_post_commit` after a clean commit (reporter only, failure ignored). Test `clean_steps_run_post_commit_only`. |
| D4-B | Skip hooks on the panel | C3: `RebaseProgressPanel` gets `skipHooks`/`onToggleSkip` for the sequence variant, shown when `headInfo.commitHooks` is non-empty. `continueSequence(appState.state.skipHooks)`, and reset `skipHooks` on success. Vitest for the checkbox. |
| D4-C | reuse the commit box's Skip hooks | C3: `continueSequence` reads `state.skipHooks`, plus a palette entry "Skip hooks for the next commit" that covers Continue. |
| D5-B | hooks only | A1: `run_captured` uses `detach_session`; `run_captured_split` keeps `process_group(0)` (two helpers). Drop `split_program_leads_its_own_session_without_a_terminal`, and keep the signing `SIGTTIN` note in ARCHITECTURE. |
| D5-C | detect `SIGTTIN` | A1 instead: keep `process_group(0)`. `wait_bounded` polls `/proc/<pid>/stat` state `T` on Linux (`waitid(P_PID, WSTOPPED\|WNOHANG\|WNOWAIT)` elsewhere) and kills the group with output note "Browsitory stopped the hook: it tried to read from the terminal." New tests under `script -qc` (Linux). |
| D6-B | `KILL_ON_JOB_CLOSE` | A2: `SetInformationJobObject(job, 9 /* JobObjectExtendedLimitInformation */, &info)` with a hand-written `JOBOBJECT_EXTENDED_LIMIT_INFORMATION` (`BASIC_LIMIT_INFORMATION` {i64, i64, u32 `LimitFlags` = 0x2000, usize, usize, u32, usize, u32, u32} + `IO_COUNTERS` (6×u64) + 4×usize; `#[repr(C)]`, with a size assert test for x86_64). Flip `a_normal_exit_leaves_…` to `a_normal_exit_kills_a_hooks_background_process_on_windows`. |
| D6-C | signing in a job too | A2: `run_captured_split` also assigns a job. Add a Windows signing-timeout test with `gpg.program` = an sh script that backgrounds a marker writer. |
| D7-B | long commands only | D1: keep `worker_handle` for reads, and add `RepoWorker::call` only for commit, amend, Continue, merge, switch, checkout, push/fetch/pull, rebase, stash apply/pop and undo/redo. Drop the compile-time guarantee, and keep the saturation test. |
| D7-C | async replies | Replace Part D with a `repo-service` feature `async-replies` (`tokio` with only `sync`, MIT; add a LICENSE row): each `WorkerHandle` method gains an `_async` twin using `tokio::sync::oneshot`. Tauri awaits the twins; the sidecar is unchanged. |
| D8-B | issue's flow only | E1: drop the Cancel `it`. |
| D8-C | plus Continue | E1: a third `it` that builds a conflicting two-commit pick on a side branch through `git`, pauses, resolves, installs a failing `commit-msg`, clicks "Continue cherry-pick", then "Continue without hooks". Clean up with `git cherry-pick --abort`/branch delete in `finally`. |
