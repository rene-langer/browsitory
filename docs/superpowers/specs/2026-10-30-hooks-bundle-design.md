# Hooks bundle

## Goal

Close six follow-ups to the git-hooks batch (4e, #94) and the sequence batch (4g-1, #96):

| Issue | Area | Problem |
|---|---|---|
| #122 | Windows | Timeout or cancel kills only the hook's `sh.exe`. Grandchildren (a husky hook's `node`) survive, and output draining waits out the 2 s grace. |
| #123 | E2E | No VSCode E2E covers hooks: `hookProgress`, `CommitOutcome::HookFailed` and the retry over the sidecar transport are untested end to end. |
| #124 | parity | `pre-merge-commit` never runs. |
| #125 | Tauri | Worker-backed `async` commands block a Tokio runtime thread on `rx.recv()` for as long as the worker is busy, up to the 10-minute hook timeout. |
| #126 | Unix | Started from a terminal, a hook that reads `/dev/tty` is stopped by `SIGTTIN` until Cancel or the timeout. |
| #133 | parity | **Continue** on a paused cherry-pick/revert sequence runs no commit hooks. A Gerrit `commit-msg` (Change-Id) never runs on it. |

## Shipping

One PR on `feat/hooks-bundle`, with one CHANGELOG section ("Hooks bundle") under
`## [Unreleased]`. The plan is `docs/superpowers/plans/2026-10-30-hooks-bundle.md`. It changes
`git-core`, `repo-service`, `tauri-app`, `vscode-sidecar`, the frontend (one outcome variant, one
retry kind, the running-hook line on the sequence panel), and adds one VSCode E2E spec. There is
**no new dependency**: `libc` is already a direct unix dependency of `git-core` (since #149), and
the Windows job object uses hand-written kernel32 FFI, as `worktree_guard/beneath_windows.rs`
does.

## Current state (main at 5ce5a38d, checked 2026-10-30)

### The hook runner

- `git-core::hooks` (`hooks.rs`): `find_hook` (`core.hooksPath` or `<common dir>/hooks`, unix
  execute bit), `run_hook` (l. 202) and `run_reported` (l. 244, reporter `started`/`finished`).
  `HOOK_TIMEOUT` = 600 s, `OUTPUT_LIMIT` = 64 KiB. `SKIPPABLE_COMMIT_HOOKS` = `pre-commit`,
  `commit-msg` (drives the "Skip hooks" checkbox through `HeadInfo.commitHooks`).
  `SKIPPABLE_HOOKS` adds `pre-push`. On Windows hooks run through Git for Windows' `sh.exe`
  (`find_sh`). A file without a shebang is retried through `sh` on `ENOEXEC`.
- Hook environment: cwd = worktree root; `GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE` and
  `GIT_PREFIX` removed. Commit hooks get `GIT_INDEX_FILE` back, plus `GIT_EDITOR=:`. Stdin is
  `/dev/null`, except `pre-push`, which gets the ref lines.
- `git-core::process::run_captured` (`process.rs` l. 250) does the following:
  - stdout and stderr go into one `std::io::pipe`;
  - it polls every 20 ms for exit, cancel and the deadline (`wait_bounded`, l. 222);
  - `own_group_without_window` (l. 189) gives unix `process_group(0)`, and Windows
    `CREATE_NO_WINDOW`;
  - `kill_tree` (l. 168) runs the POSIX `kill -s KILL -- -<pgid>` utility, then
    `child.kill()`. On Windows only the direct child is killed;
  - it always reaps the child; after exit it waits at most `DRAIN_GRACE` = 2 s for EOF.

  `run_captured_split` (l. 327, the signing programs) shares the group and kill logic.
- Which operation runs which hook today:
  - `commit::commit` / `amend_head`: `pre-commit` → `prepare-commit-msg` → `commit-msg` →
    commit (signing reported as a `"signing"` pseudo-hook) → `post-commit`.
  - `push_refs`: `pre-push`.
  - The worker, after success only: `post-checkout` for a switch or checkout; `post-merge 0` after
    a fast-forward, after a `Merged` start (see below) and after a fast-forward pull; `post-merge
    1` after a squash.
  - Cherry-pick, revert, sequences (start and Continue), rebase, stash, undo, worktree add and
    clone run no hooks.
- Plumbing: `HookEvent` → Tauri `hook-progress` / sidecar `hookProgress`. Commit and amend
  get `commit-<n>`/`amend-<n>` ids in `CancelRegistry`, and `cancel_transfer` stops a hook.
  `CommitOutcome::HookFailed { hook, output }` crosses IPC as data. The frontend keeps
  `hookFailure { hook, output, retry, dialogOpen }` with `retry.kind` `commit | amend | push`.
  `retryLabel` gives "Commit/Amend/Push without hooks" for a hook in `SKIPPABLE_HOOKS`.

### What git does (probe, git 2.53.0)

A throwaway shell probe under `target/` (deleted afterwards) installed every hook as a logger and
ran in hermetic repositories (`GIT_CONFIG_GLOBAL=/dev/null`).

| Command | Hooks, in order |
|---|---|
| `git merge` (clean, creates the commit) | `pre-merge-commit`; `prepare-commit-msg .git/MERGE_MSG merge`; `commit-msg .git/MERGE_MSG`; `post-merge 0`. **No `pre-commit`, no `post-commit`.** |
| `git merge --no-verify` (clean) | `prepare-commit-msg … merge`; `post-merge 0` |
| `git merge` whose `pre-merge-commit` fails | Exit 1, "Not committing merge; use 'git commit' to complete the merge." `MERGE_HEAD`/`MERGE_MSG` stay and the result stays staged. The following `git commit` runs `pre-commit`, `prepare-commit-msg … merge`, `commit-msg`, `post-commit`. |
| `git merge --no-commit` (clean), then `git commit` | Merge: none. Commit: `pre-commit`, `prepare-commit-msg COMMIT_EDITMSG merge`, `commit-msg`, `post-commit`. **No `post-merge`.** |
| `git merge` with a conflict, then `git merge --continue` | Merge: none. Continue: `pre-commit`, `prepare-commit-msg … merge`, `commit-msg`, `post-commit`. |
| `git merge` fast-forward | `post-merge 0` |
| `git cherry-pick X` / `git revert --no-edit X` (clean) | `prepare-commit-msg COMMIT_EDITMSG message`; `post-commit` |
| `git cherry-pick A B` (both clean) | `prepare-commit-msg … message`, `post-commit`, once per commit |
| `git cherry-pick A..C` stopped on a conflict at B | At the stop: none. `--continue`: B gets `pre-commit`, `prepare-commit-msg … merge`, `commit-msg`, `post-commit`; then C (clean) gets `prepare-commit-msg … message`, `post-commit`. |
| Single conflicting pick, then `--continue` or `git commit` | `pre-commit`, `prepare-commit-msg … merge`, `commit-msg`, `post-commit` |

Every hook had stdin `/dev/null`. git runs hooks in its own process group and session, so from an
interactive shell a hook *can* read `/dev/tty`. That is how husky's
`exec < /dev/tty` prompts work. Started without a controlling terminal (a desktop launcher, an
IDE), `/dev/tty` fails with `ENXIO`.

### #124: how Browsitory's merge commits

`merge::start_merge` never creates a merge commit. A clean non-fast-forward merge
(`MergeOutcome::Merged`) runs `repo.merge` and leaves `MERGE_HEAD`/`MERGE_MSG` and the merged index.
The commit box is pre-filled and the user presses **Commit**. That goes through `commit::commit`,
which sees `RepositoryState::Merge` and adds the merge parents. So today a clean merge is
`git merge --no-commit` + `git commit`:

- `pre-commit`, `prepare-commit-msg merge`, `commit-msg`, `post-commit` run on Commit;
- the worker already ran `post-merge 0` when the merge was *staged* (`worker/merge.rs` l. 18),
  before any merge commit exists. git never does that.

Pull never creates a merge commit. A diverged pull offers **Merge**, which is `start_merge` on
`refs/remotes/<remote>/<branch>`, the same path. There is no "no verify" for merges except the
commit box's **Skip hooks** (`no_verify` on `commit`), which already applies, since the merge
commit is made there. Squash merges (`post-merge 1`, then a plain commit) and fast-forwards
(`post-merge 0`) already match git.

### #133: how Continue commits

`pick::continue_sequence` (l. 499) → `finish_paused` (l. 548) commits the resolved step with
`signing::create_commit`. It has the timeout only, no cancel, and no hooks. Then `run` applies
the remaining steps with `step()`, which also has no hooks. The worker:

- holds the state lock (#173, `undo::locks_state` includes `ContinueSequence`) for the whole
  command;
- persists the state through `history::persist` → `sequence.json` (#169);
- `relabel_sequence`s the undo entry on `Completed`.

`ContinueSequence` carries only a `reply`: no operation id, no events, no `no_verify`. While a
sequence is stored the gate refuses Commit and Amend, so Continue is the only way to finish a
step from Browsitory. A paused **single** cherry-pick/revert is finished with **Commit**, which
already runs the full commit hook set (with source `merge`), as git does.

The sequence panel (`RebaseProgressPanel` in `DiffPane`) replaces the commit box during a pause,
so the "Running … hook… Cancel" line (rendered by `CommitBox`) is not shown there today.

### #125: which Tauri commands block

Every worker-backed Tauri command (about 110 call sites of `worker_handle` in
`crates/tauri-app/src/commands/*.rs`) is an `async fn` that calls a `WorkerHandle` method, and
those methods block on a `std::sync::mpsc` `rx.recv()`. The worker is serial per repository, so
a call blocks for:

- its own work: a commit or amend with hooks (up to 10 min each), signing (120 s), a push with
  `pre-push`, fetch, `start_merge` and `switch_branch` (they run `post-merge`/`post-checkout`
  synchronously), and other network operations;
- **and** for everything queued before it. A `get_status` refresh queued behind a running
  hook blocks too.

Tauri's runtime is a multi-thread Tokio runtime with one worker thread per CPU core. A commit
whose hook waits, plus a few refreshes, can occupy all of them. Every other async command then
stalls, for every repository, and so does `cancel_transfer`: the **Cancel** button can't reach
the worker it is meant to stop. Only `pull_current_upstream`, `clone_repo`, `open_diff_tool` and
`open_merge_tool` use `spawn_blocking` today. `scan_repos_in_root` deliberately avoids it (a
comment records a CI stall attributed to cold blocking-pool threads for microsecond work). The
VSCode sidecar runs each request on its own thread, so it is not affected.

### #126: process groups and the controlling terminal

A probe (rustc binary under `target/`, deleted afterwards), run plain and under `script -qc`
(which gives it a pty as controlling terminal), showed:

- `process_group(0)` + `pre_exec(setsid)` → spawn fails with `EPERM`. std applies the
  `setpgid` before `pre_exec` closures, and a group leader cannot call `setsid`;
- `pre_exec(setsid)` alone → the child is pid = pgrp = sid with `tty_nr` 0, even under the pty.
  `: < /dev/tty` fails at once with "No such device or address";
- the default spawn under the pty has `tty_nr` = the pty and inherits the session.

So `setsid` replaces `process_group(0)`. The child still leads its own process group
(pgid = pid), and the group kill keeps working. The issue's premise that "`libc` needs to become
a direct dependency" is outdated: `crates/git-core/Cargo.toml` already has
`[target.'cfg(unix)'.dependencies] libc = "0.2"`. Only its `docs/LICENSE_COMPLIANCE.md` row
needs the new uses (`setsid`, `killpg`).

### #122: Windows

`kill_tree` on Windows is `child.kill()` (`TerminateProcess` on `sh.exe`). MSYS2 background jobs
and anything the hook spawned are separate Windows processes and survive. CI runs `cargo test`
and `cargo clippy` on `windows-latest`, which has Git for Windows, so `find_sh()` finds
`sh.exe`.

### #123: VSCode E2E

`extension/e2e/src/specs/*.spec.ts` run in one Extension Development Host against
`/tmp/browsitory-vscode-e2e-repo`. That is a plain `git init` repo with user name and email
set, shared by all specs in glob order. Specs drive the webview through CDP
(`support/webviewPage.ts`: `waitForText`, `waitForSelector`, `clickByText`, `fill`, …) and
check git with `execFileSync("git", …)`. The Tauri spec `e2e/specs/git-hooks.spec.ts` installs
the hook at `git rev-parse --git-path hooks` (honours `core.hooksPath`), skips on win32, and
removes the hook in `after`. CI runs the VSCode E2E on ubuntu under `xvfb-run`.

## Design

### 1. #126: hooks and signers start in a new session (D5-A)

`process::own_group_without_window` becomes `detach(command)`:

- **Unix:**

  ```rust
  unsafe { command.pre_exec(|| if libc::setsid() == -1 { Err(io::Error::last_os_error()) } else { Ok(()) }) }
  ```

  It replaces `process_group(0)`. `setsid` is async-signal-safe, which `pre_exec` requires. The
  child has no controlling terminal, so opening `/dev/tty` fails at once with `ENXIO` and the
  hook ends with its own error (for example "cannot open /dev/tty: No such device or address")
  in the hook-failure dialog. A launch from a terminal now behaves like a launch from the
  desktop. A Ctrl+C in that terminal no longer reaches the hook either, as before.
- **`kill_tree` (unix)** uses `libc::killpg(child.id() as pid_t, SIGKILL)` instead of spawning
  `kill`. The pgid is still the child's pid. That saves a process spawn per kill and a `PATH`
  lookup of `kill`. The existing safety argument stays: the child isn't reaped yet, so its pid
  can't have been reused.
- A pre-exec closure makes std use fork+exec instead of `posix_spawn`. That is a measurable but
  negligible cost per hook. `ENOEXEC` still comes back as `RunError::Spawn` with
  `raw_os_error() == ENOEXEC` (std reports exec errors through its CLOEXEC pipe), so the
  shebang-less fallback keeps working, and so does the `ETXTBSY` retry.
- Applies to `run_captured` (hooks) and `run_captured_split` (gpg/ssh-keygen signing,
  verification). `process::spawn` (editors, terminals, diff tools) keeps `process_group(0)`: those
  are long-lived GUI or terminal programs with stdin/stdout null, and are out of scope.
- The "known limitation" notes in `process.rs`, `docs/ARCHITECTURE.md` (Git hooks, signing) and
  `docs/USER_GUIDE.md` (Git hooks) are replaced by the new behaviour.

### 2. #122: a Windows job object per hook (D6-A)

New `crates/git-core/src/process/job_windows.rs` (`#[cfg(windows)] mod job_windows;` in
`process.rs`), with hand-written kernel32 FFI in the style of `beneath_windows.rs`:

```rust
#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> *mut c_void;
    fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
    fn TerminateJobObject(job: *mut c_void, exit_code: u32) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}
pub(super) struct Job(*mut c_void);          // Drop: CloseHandle
impl Job {
    pub(super) fn assign(child: &Child) -> Option<Job>;   // None: creation or assignment failed
    pub(super) fn terminate(&self);                        // TerminateJobObject(job, 1)
}
```

- `run_captured` creates the job right after the spawn and assigns the child
  (`AsRawHandle for Child`). The job travels through `wait_bounded` → `kill_and_reap` →
  `kill_tree`, which on timeout or cancel calls `job.terminate()` and then `child.kill()`.
  `Option<&Job>` on all platforms keeps one signature; on unix it is always `None`.
- Processes started by a process in a job join the job, so `sh.exe`'s children and their
  children are covered. Nested jobs work since Windows 8, so this also works when Browsitory
  itself runs inside a job (VSCode, Windows Terminal).
- **No `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`** (D6-A). A hook that exits normally keeps any
  background process it started, as on Unix, where a normal exit doesn't kill the group. Only
  timeout and cancel kill the tree. The handle is closed when `run_captured` returns. A process
  left in the job keeps running, and the job object dies with the last process.
- **Residual race:** a process the hook starts between `CreateProcess` and
  `AssignProcessToJobObject` escapes. `sh.exe` (the MSYS2 runtime) takes well over that window
  to initialise before it can start anything. Closing the window would need `CREATE_SUSPENDED`
  plus resuming the primary thread, and std doesn't expose the thread handle (it would take
  `NtResumeProcess` from ntdll). This is accepted and documented.
- Only `run_captured` (hooks). Signing (`run_captured_split`) keeps killing only the direct
  child. gpg may auto-start `gpg-agent` inside the call, and a job kill on a signing timeout
  would take the agent down with it.
- If `CreateJobObjectW` or `AssignProcessToJobObject` fails, the behaviour is today's.

### 3. #124: a clean merge's commit runs `git merge`'s hooks (D1-A, D2-A)

The commit that concludes a merge which **Browsitory started and which stopped without
conflicts** runs what `git merge` runs when it creates the commit. Any other merge commit (one
that had conflicts, or one Browsitory can't vouch for) runs what `git commit` runs, as today.

`git-core`:

- `hooks::SKIPPABLE_HOOKS` gains `"pre-merge-commit"` (`--no-verify` skips it).
- `commit.rs` gains:

  ```rust
  /// Which hooks a commit runs: `git commit`'s, or `git merge`'s when it concludes a clean merge.
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
  pub enum CommitHookSet { #[default] Commit, CleanMerge }
  pub fn commit_with(repo: &mut Repository, message: &str, set: CommitHookSet, hooks: HookOptions<'_>) -> Result<String, CommitError>;
  ```

  `commit` stays and becomes `commit_with(.., CommitHookSet::Commit, ..)`. For `CleanMerge`
  while `repo.state() == Merge`, `run_commit_hooks` runs `pre-merge-commit` in place of
  `pre-commit` (skipped by `no_verify`, same env, the index is reloaded if it ran, a failure is
  `CommitError::HookFailed`). Then come `prepare-commit-msg <COMMIT_EDITMSG> merge` and
  `commit-msg` (skipped by `no_verify`), as now. **`post-commit` is not run.** Outside a merge,
  `CleanMerge` falls back to `Commit`.
  - Deliberate deviation: the message file is still `<git dir>/COMMIT_EDITMSG`, not
    `MERGE_MSG`. Writing the user's text into `MERGE_MSG` would change the merge state that
    pre-fills the box.
- `repo::head_info`: `commit_hooks` also lists an installed `pre-merge-commit` while
  `repo.state() == Merge`, so **Skip hooks** shows for a merge that only has that hook.

`repo-service` worker:

- New `worker/merge.rs` `CleanMerge { head: Oid, merge_heads: Vec<Oid> }` with
  `capture(repo)` and `matches(&self, repo) -> bool`. It matches when the state is `Merge`, HEAD
  is the same and `MERGE_HEAD` lists the same commits. A worker-local
  `Option<CleanMerge>` is set when `StartMerge` returns `Merged`, and cleared on any other
  `StartMerge` outcome, on `AbortMerge`, and after a commit.
- `merge::start` no longer runs `post-merge` for `Merged`. Fast-forward (`0`) and squash (`1`)
  are unchanged.
- `status::commit` picks `CleanMerge` when the remembered merge `matches`, else `Commit`. After
  a successful `CleanMerge` commit it runs `post-merge 0` (`hooks::run_post_hook`, failure logged
  and ignored). A failed or cancelled commit keeps the memory, so "Commit without hooks" still
  concludes it as a clean merge (`git merge --no-verify`).
- After a restart, or for a merge started outside Browsitory or by the other window, the memory
  is empty and the commit runs `git commit`'s set. That is exactly what git does for
  `merge --no-commit` + `commit`.

Frontend: `SKIPPABLE_HOOKS` in `lib/hookFailure.ts` gains `"pre-merge-commit"`, so the dialog
offers "Commit without hooks". The **Skip hooks** checkbox `title` names the hooks
`--no-verify` would skip (adding `pre-merge-commit` while a merge is staged). No new IPC.

### 4. #133: Continue runs the commit hooks on the step it commits (D3-A, D4-A)

What runs when:

- the step **Continue commits** (the resolved, paused step) runs `git cherry-pick --continue`'s
  set: `pre-commit`, `prepare-commit-msg <COMMIT_EDITMSG> merge`, `commit-msg`, then the commit
  (signing reported and cancellable like a commit), then `post-commit`;
- steps Browsitory applies cleanly, before or after, still run **no** hooks (D3-A). The same
  goes for the sequence start, a single clean cherry-pick or revert, and rebase;
- an empty resolution (tree == HEAD's) is skipped without running hooks, as now.

`git-core::pick`:

- `continue_sequence(repo, state, hooks: HookOptions<'_>) -> SequenceRun` (signature change; the
  existing tests pass `HookOptions::default()`, and no hooks are installed there).
- `finish_paused` builds the message as now, then calls the commit path's hook runner:
  `commit::run_commit_hooks(repo, &message, "merge", hooks)` (made `pub(crate)`). It reloads the
  index if `pre-commit` ran, writes the tree and commits through
  `commit::create_commit_reporting` (made `pub(crate)`; gives the `"signing"` pseudo-hook and
  cancel), then `commit::run_post_commit`. The empty-step check runs **before** the hooks, on the
  resolved index. A `pre-commit` that changes the index is then committed as it left it.
- New outcome:

  ```rust
  SequenceOutcome::HookFailed { hook: String, output: String, progress: SequenceProgress }
  ```

  It comes back as `SequenceRun { outcome: Ok(HookFailed{..}), state: Some(state) }` with the
  state **unchanged** (`next`, `expected_head`, `stopped_on_conflict` as before). git's paused
  state (`CHERRY_PICK_HEAD`/`REVERT_HEAD`, the resolved index) is untouched, so Continue
  (with or without hooks) and Abort work as before.
  - A cancelled hook or signer is `Err(PickError::Hook(HookError::Cancelled))` (new
    `#[from]` variant), also with the state kept (`SequenceRun::stopped`).
  - A hook that can't start is a `HookFailed` (same rule as `commit::gate`).

Interplay:

- **Sequence state / #169:** `persist` saves the unchanged state, so `sequence.json` still
  describes the paused step, and a restart followed by Continue (with or without hooks) works.
  No format change.
- **Undo:** `ContinueSequence` has no undo plan. The sequence is one entry identified by the
  operation marker, which a hook failure does not change. `relabel_sequence` acts only on
  `Completed`. Files a failing `pre-commit` reformatted stay in the work tree, as with git, and
  Abort or Undo of the sequence restores them like any edit made during the pause.
- **#173 lock:** Continue already holds the state lock for the whole command. Hooks make that
  longer (as with commit today). The other window's locked commands wait `LOCK_WAIT` (10 s) and
  then run unrecorded or ask to retry. Its reads never wait.
- **Gate:** unchanged. Commit and Amend stay refused during a sequence. Continue and Abort are
  the only ways forward.
- **Signing:** the continued step's signer gets the hooks' cancel check and the `"signing"`
  progress line. Later clean steps keep the timeout only, as now.

Worker and transport:

- `Command::ContinueSequence { no_verify, operation_id, events, reply }`. The id is
  `continue-<n>` in `CancelRegistry`, reported through `ChannelHookReporter`.
  `WorkerHandle::continue_sequence(no_verify, events)`.
- Tauri `continue_sequence(repo_path, no_verify: Option<bool>, app, state)` calls
  `emit_hook_events`. Sidecar `continue_sequence` takes `noVerify` (default `false`) and uses
  `spawn_hook_relay`. `SequenceOutcomeDto` (both) gains
  `{ kind: "HookFailed", hook, output, progress }`.
- `RepoClient.continueSequence(repoPath, noVerify = false)`.

Frontend (D4-A):

- `useHistoryActions.continueSequence(noVerify = false)`: on `HookFailed` it sets
  `hookFailure { hook, output, retry: { kind: "continueSequence" }, dialogOpen: true }` and
  throws `hookFailedMessage(hook)` (as `commit` does).
- `HookRetry` gains `{ kind: "continueSequence" }`. `retryLabel` returns "Continue without
  hooks" for a hook in `SKIPPABLE_HOOKS`. `retrySuccessLabel` reuses the sequence toast.
  `retryWithoutHooks` calls `continueSequence(true)`.
- `RebaseProgressPanel` gets an optional `hooks` prop (`running`, `onCancel`) and renders the
  same "Running {hook} hook… Cancel" status line as `CommitBox`. `DiffPane` passes it for the
  sequence variant.

### 5. #125: worker round trips leave the async runtime (D7-A)

`crates/tauri-app/src/commands/mod.rs`:

```rust
/// A repository's worker as commands reach it: every round trip runs on Tokio's blocking pool,
/// so a busy worker (a hook, a signer, a transfer, or a queue behind one) never holds an async
/// runtime thread (#125).
pub(crate) struct RepoWorker(WorkerHandle);
impl RepoWorker {
    pub(crate) async fn call<T, F>(self, f: F) -> Result<T, String>
    where F: FnOnce(&WorkerHandle) -> Result<T, String> + Send + 'static, T: Send + 'static;
    /// For calls that never wait on the worker's queue (`cancel_transfer`, `cancel_search`).
    pub(crate) fn signal(&self) -> &WorkerHandle;
}
pub(crate) fn repo_worker(state: &State<AppState>, repo_path: &str) -> Result<RepoWorker, String>;
pub(crate) async fn off_runtime<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String>;
```

- `worker_handle` is removed. Every command goes through `repo_worker(..)?.call(move |w|
  w.x(..)).await`, so a new command can't block the runtime by accident: getting a
  `WorkerHandle` any other way doesn't compile.
- `call` = `off_runtime(move || f(&handle)).await?`. `off_runtime` wraps
  `tauri::async_runtime::spawn_blocking` and maps a `JoinError` to "worker task stopped".
- The existing `spawn_blocking` uses (`pull_current_upstream`, clone, diff/merge tools) move to
  `off_runtime`. `scan_repos_in_root` keeps its documented plain-async choice (no worker
  involved).
- Event forwarding (`emit_hook_events`, `emit_transfer_events`) already runs on its own
  `std::thread`, and is unchanged.
- The VSCode sidecar is unchanged (a thread per request).

### 6. #123: VSCode E2E for hooks (D8-A)

New `extension/e2e/src/specs/git-hooks.spec.ts`, skipped on win32 (as the Tauri spec):

1. **"shows a failing pre-commit's output and commits without hooks":**
   - installs `pre-commit` (`sleep 3; echo "vscode-e2e-hook: lint failed" >&2; exit 1`) at
     `git rev-parse --git-path hooks` with mode 0o755, and writes `vscode-hooks.txt`;
   - opens Browsitory and waits for the "Skip hooks" label (`HeadInfo.commitHooks` over the
     sidecar);
   - **Stage all**, fills the message, **Commit**;
   - waits for "Running pre-commit hook" (`hookProgress` notification) and for
     `dialog[aria-label="pre-commit hook failed"]` with the output in
     `pre[aria-label="Hook output"]` (`CommitOutcome::HookFailed` over the sidecar);
   - checks with git that nothing was committed;
   - clicks **Commit without hooks** and waits until `git log -1 --format=%s` is the summary.
2. **"Cancel stops a running pre-commit":**
   - a `sleep 60` hook; Commit; waits for "Running pre-commit hook", then clicks **Cancel**
     (`cancel_transfer` over the sidecar);
   - waits for "The pre-commit hook was cancelled." and checks that no commit landed, all within
     20 s.

Each test removes its hook and unstages or deletes its file in `finally`, because later specs
share the fixture repository.

## Open decisions (provisional choice in **bold**; the plan implements it)

**D1. Which hooks a merge runs (#124). User-facing.**

- **A. The commit that concludes a clean merge started in Browsitory runs `git merge`'s set:
  `pre-merge-commit` → `prepare-commit-msg merge` → `commit-msg` → `post-merge 0` (moved from
  merge start to after the commit; no `pre-commit`, no `post-commit`). A merge that had
  conflicts runs `git commit`'s set, as now.** It reuses the commit box's progress, Cancel, Skip
  hooks and failure dialog ("Commit without hooks" = `git merge --no-verify`). It also fixes
  `post-merge` running before the merge commit exists.
- B. Run `pre-merge-commit` inside **Merge** (`start_merge`), right after staging a clean
  result. A failure returns a new `MergeOutcome::HookFailed` and leaves the merge staged, as git
  does. The later commit then runs `git commit`'s set, so `pre-commit` runs too: a clean merge
  runs both hooks, and lint runs twice. It needs operation ids, progress and Cancel on
  `StartMerge`, and a "Merge without hooks" retry.
- C. Make **Merge** create the commit itself for a clean merge, like `git merge`. The merge
  dialog gains a message field and **Skip hooks**, and clean merges no longer stop in the commit
  box. This is the largest UX change.
- D. Every merge commit from the commit box runs `pre-merge-commit` instead of `pre-commit`,
  conflicted or not. There is no memory, but it is wrong for conflict resolutions: git runs
  `pre-commit` there, and that is where linting staged files matters.

Recommendation **A**: git's semantics for both paths, no new UI, smallest change.

**D2. How the worker knows a merge was clean (#124). Engineering.**

- **A. In worker memory (`Option<CleanMerge>`, checked against HEAD and `MERGE_HEAD` at
  commit).** After a restart, or when the other window started the merge, the commit runs `git
  commit`'s set. That is exactly git's `merge --no-commit` + `commit`.
- B. Persist it as `<gitdir>/browsitory/merge.json` (atomic write like `sequence.json`), so a
  restart or the other window still runs `git merge`'s set. This is about 80 more lines and
  another file to keep consistent with undo and redo.
- C. Infer it: no `#Conflicts:` block in `MERGE_MSG` (libgit2 and git both write one on
  conflict). There is no state, but a `git merge --no-commit` from a terminal would then count
  as clean.

Recommendation **A**: the fallback is git-correct, and the window it misses (restart between
Merge and Commit) is rare.

**D3. Which sequence steps run hooks (#133). User-facing.**

- **A. Only the step Continue commits (the resolved one): `pre-commit`, `prepare-commit-msg
  merge`, `commit-msg`, `post-commit`. Clean steps, the sequence start and single clean picks or
  reverts stay hook-free.** The rule for users: "finishing a paused step runs the commit hooks,
  as Commit does for a single pick; steps Browsitory applies by itself run none". It fixes the
  Gerrit case.
- B. Full git parity: in addition, every cleanly applied step (sequence start and after
  Continue, single cherry-pick and revert) runs `prepare-commit-msg <file> message` and
  `post-commit`. A failing `prepare-commit-msg` then stops a sequence on a clean step (a new kind
  of stop that leaves the step staged, which Continue commits), and the starts need progress and
  Cancel plumbing. A "prepend the ticket id" `prepare-commit-msg` would rewrite cherry-picked
  messages.
- C. A, plus `post-commit` (failure ignored) on clean steps. There is no new stop kind, but the
  parity is only partial.

Recommendation **A**. B is a reasonable follow-up issue if users ask for it.

**D4. How to skip hooks on Continue (#133). User-facing.**

- **A. Only through the failure dialog's "Continue without hooks" (`noVerify` on
  `continueSequence`).** This mirrors `git commit --no-verify && git cherry-pick --continue`.
- B. A, plus a "Skip hooks" checkbox on the sequence panel (shown when `pre-commit` or
  `commit-msg` is installed), like the commit box.
- C. Carry the commit box's **Skip hooks** state over to Continue. The box is hidden during a
  sequence, so the state would be invisible.

Recommendation **A**: the dialog appears only when needed, and B can be added later without
backend change.

**D5. What happens to hooks (and signers) that prompt on the terminal (#126). User-facing.**

- **A. `setsid` for hooks and signing programs: `/dev/tty` fails at once with `ENXIO`, so a
  prompting hook fails with its own error in the dialog (retry without hooks), the same as when
  Browsitory is started from the desktop. ssh-keygen with an encrypted key falls back to
  `SSH_ASKPASS` as on a desktop launch.**
- B. `setsid` for hooks only. A signer that prompts on `/dev/tty` still stops until the 120 s
  signing timeout or Cancel.
- C. Keep the process group, and detect a hook stopped by `SIGTTIN` (`waitid(WSTOPPED)` or
  `/proc/<pid>/stat` state `T`), then kill it with "the hook tried to read from the terminal".
  It needs Linux-specific or extra signal code, and still can't let the hook prompt.
- D. Hand the terminal to the hook (`tcsetpgrp`) so it can prompt in the terminal Browsitory was
  started from. That terminal is often hidden or gone, and job control with the parent shell
  becomes fragile. Not recommended.

Recommendation **A**.

**D6. What a Windows hook's job kills (#122). User-facing (background processes).**

- **A. Kill the job only on timeout or cancel; no `KILL_ON_JOB_CLOSE`. A background process
  that a hook leaves after exiting normally keeps running, as on Unix.**
- B. As the issue suggests: `KILL_ON_JOB_CLOSE`. Closing the handle after *every* hook kills
  whatever the hook left running (also after success), and a Browsitory crash kills running
  hooks. The issue's goal is met, but Windows then diverges from Unix.
- C. A, and also put signing programs in a job. That risks killing a `gpg-agent` that gpg
  auto-started during a signing that then times out.

Recommendation **A**.

**D7. Which Tauri commands leave the async runtime (#125). Engineering.**

- **A. All worker-backed commands, through `RepoWorker::call` (`spawn_blocking`).** Any of them
  can be queued behind a hook, so this fixes the starvation, including a Cancel that can't
  run. `worker_handle` is removed, so the rule is enforced by the compiler.
- B. Only commands that can run long themselves (commit, amend, Continue, merge, switch, the
  transfers). Reads queued behind them still block runtime threads.
- C. Async replies: `WorkerHandle` methods return a future (`tokio::sync::oneshot`) behind a
  `repo-service` feature. There is no blocking pool, but about 110 handle methods change and
  `tokio` becomes a direct dependency of `repo-service`.

Recommendation **A**. The cold-start concern in the `scan_repos_in_root` comment is about
microsecond work. Worker calls are I/O-bound, and the blocking pool keeps idle threads for 10 s.

**D8. Which VSCode E2E flows (#123). Engineering.**

- **A. Failing `pre-commit` → dialog → "Commit without hooks", plus Cancel of a running hook
  (covers `hookProgress`, `HookFailed`, retry and `cancel_transfer` over the sidecar).**
- B. Only the issue's flow (no Cancel).
- C. A, plus a Continue-with-hooks flow on a conflicting cherry-pick sequence. That is a long
  fixture in the shared repository, and the Rust and Vitest tests already cover it.

Recommendation **A**.

## Testing

Real temp-dir repos with real `#!/bin/sh` hook scripts. Integration test binaries go through
`mod common;` (it calls `test_support::hermetic_git!()`, and `common::init_repo` pins
`core.hooksPath` to the repo's own hooks dir). Hook tests are `#[cfg(unix)]`. Session tests that
read `/proc` are `#[cfg(target_os = "linux")]`. Windows tests are `#[cfg(windows)]` and skip with
a message when `find_sh()` is `None`.

- **#126** (`process.rs` unit tests):
  - Linux: `run_captured` and `run_captured_split` run a `sh` that parses `/proc/$$/stat` and
    asserts pid = pgrp = sid and `tty_nr` = 0. This holds even when the test runner has a
    terminal, and fails today (sid = the runner's session);
  - unix: a hook that does `exec < /dev/tty` fails within seconds with the shell's error in its
    output. In CI this already holds without a ctty; locally from a terminal it proves the fix;
  - the existing timeout and cancel group-kill tests must keep passing with `killpg`.
- **#122** (`process.rs` unit tests, windows): sh.exe runs
  `(sleep 3; echo alive > "$MARKER") >/dev/null 2>&1 & sleep 30` with a 500 ms timeout. After
  `run_captured` returns, wait 6 s and assert the marker does not exist. The same runs for
  cancel. A third test (D6-A) runs the same background job with `exit 0` and asserts the marker
  *does* appear. **What CI verifies:** `windows-latest` runs these with Git for Windows' sh.exe
  (on PATH via git.exe), and clippy checks the FFI. CI can't prove the spawn-to-assign race is
  closed; that stays a documented residual.
- **#124**: git-core `tests/hooks.rs` with a logging hook for every name (exact hook order and
  arguments), and worker tests in `worker/mod.rs`: `post-merge` moves, the restart fallback,
  and HookFailed followed by a retry without hooks.
- **#133**: git-core `tests/pick_sequence.rs` (`#[cfg(unix)]` hook tests: exact order, message
  rewrite, a failure keeps the state and git's paused state, `no_verify`, cancel, an empty
  resolution and clean steps run none, revert), and worker tests (events with a `continue-` id,
  `sequence.json` kept, Continue without hooks after a worker restart, cancel).
- **#125**: tauri-app unit test `off_runtime_keeps_async_runtime_threads_free`. It starts
  `available_parallelism() + 2` `off_runtime` jobs that block on a channel, then asserts that a
  plain `tauri::async_runtime::spawn(async { 7 })` completes within 5 s while they are still
  blocked, then releases them. A `#[cfg(unix)]` variant does the same through `RepoWorker::call`
  on a real `Worker` whose `pre-commit` polls for a flag file. The DTO tests cover the new `HookFailed` sequence outcome.
- **Frontend** (Vitest, mocked `RepoClient`):
  - `hookFailure.test.ts`: `pre-merge-commit` → "Commit without hooks"; `continueSequence` →
    "Continue without hooks";
  - `RebaseProgressPanel.test.tsx`: the running-hook line and Cancel;
  - `App.test.tsx`: Continue → HookFailed → dialog → "Continue without hooks" calls
    `continueSequence(repo, true)`;
  - `tauriRepoClient.test.ts` / `vscodeRepoClient.test.ts`: `noVerify` on the wire.
- **E2E:** VSCode `git-hooks.spec.ts` (§6). The Tauri E2E suite must keep passing unchanged. It
  exercises the `RepoWorker::call` refactor on every command.

## Docs

- `docs/ARCHITECTURE.md`:
  - "Git hooks": session (`setsid`) instead of background group, `killpg`, the Windows job
    object and its residual race, `pre-merge-commit` and the clean-merge rule, Continue hooks,
    `post-merge` timing;
  - signing (the `SIGTTIN` bullet is replaced);
  - "Cherry-pick sequences": `HookFailed` keeps the state;
  - the Tauri command section: `RepoWorker::call`.
- `docs/USER_GUIDE.md`, "Git hooks":
  - the hook table gains `pre-merge-commit`; `post-merge` runs after the merge commit;
  - the `/dev/tty` paragraph becomes "a hook that tries to ask on the terminal fails";
  - the Windows paragraph: Cancel and the time limit stop programs the hook started;
  - the "Continue … does not" sentence becomes the D3-A rule, with "Continue without hooks".
- `docs/LICENSE_COMPLIANCE.md`: the `libc` row lists `setsid`/`killpg` for `process.rs`.

## Risks

- **Fork+exec instead of posix_spawn** for hooks and signers (a `pre_exec` closure). That is
  slightly slower per spawn, and the closure must stay async-signal-safe (only `setsid`).
- **Prompting hooks now fail instead of hanging** (D5-A). A user who answered husky prompts in
  the terminal they launched Browsitory from loses that. This is consistent with desktop
  launches, and the failure output says what happened.
- **`RepoWorker::call` touches about 110 commands.** The change is mechanical, but the Tauri
  E2E suite (timing-sensitive, see the `scan_repos_in_root` note) is the real check. If a
  blocking-pool cold start shows up as flakiness, it shows up there.
- **D2-A memory miss.** After a restart between Merge and Commit, a clean merge runs
  `pre-commit`/`post-commit` rather than `pre-merge-commit`/`post-merge`. This is git-correct
  for that sequence, but differs from an uninterrupted merge.
- **`post-merge` moves later** for clean merges. A user who relied on it running at staging time
  (a hook that installs dependencies after Merge, before Commit) now gets it after Commit, as in
  git.
- **Continue holds the #173 lock while hooks run**, up to 10 minutes. The other window's undo
  bookkeeping waits, then degrades as documented.
- **Windows residual race** (§2), and the Windows tests depend on MSYS `sleep`/subshell
  semantics.

## Unrelated findings (not fixed here)

1. **`post-merge` ran before the merge commit existed.** For a non-fast-forward merge
   (`MergeOutcome::Merged`) the worker ran `post-merge 0` when the merge was staged. That
   is fixed here as part of D1-A, but it is a behaviour change in its own right and is listed
   in the CHANGELOG.
2. **#126's premise is outdated.** `libc` has been a direct unix dependency of `git-core` since
   #149, so no new dependency is involved.
3. **The `process.rs` docs say "Uses the POSIX `kill` utility rather than a `libc`
   dependency."** That was stale since #149. Replaced here by `killpg`.

## Out of scope

- D3-B (hooks on clean steps, single picks and reverts), D4-B (Skip hooks on the sequence
  panel), D1-C (auto-committing merges).
- Rebase hooks (`pre-rebase`, `post-rewrite`, hooks on rebase picks).
- Job objects for signing programs and for `process::spawn` (external tools).
- Making `WorkerHandle` itself async (D7-C).
- A Tauri E2E for Continue hooks or `pre-merge-commit` (Rust and Vitest cover them).
