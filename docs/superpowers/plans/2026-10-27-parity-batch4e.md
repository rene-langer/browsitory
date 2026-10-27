# Feature Parity Batch 4e (Git Hooks) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Commit, amend and push run the repository's client-side git hooks (`pre-commit`, `prepare-commit-msg`, `commit-msg`, `post-commit`, `pre-push`), switch/checkout/merge/pull run `post-checkout`/`post-merge`, and a failing gate hook shows its output with a "… without hooks" retry, in both the Tauri app and the VSCode extension.

**Architecture:** A new crate-private runner `git-core::process::run_captured` (combined capped output, timeout, cancel, process-group kill) backs a new `git-core::hooks` module (discovery, `run_hook`, reporter). Gate hooks are wired inside `git-core` (`commit`, `amend_head`, and `push_refs`'s `push_negotiation` callback); post-checkout/post-merge run in the `repo-service` worker after successful commands. Commit hook failures cross IPC as a `CommitOutcome::HookFailed` value; push hook failures as `TransferErrorKind::HookFailed` plus `hookOutput`. Progress reaches the frontend as `hook-progress`/`hookProgress` events (commit/amend) or `TransferPhase::RunningHook` (push), and cancel reuses `CancelRegistry`/`cancelTransfer`.

**Tech Stack:** Rust (`git2` 0.21, `thiserror`, `std::io::pipe`, `serde` in tauri-app/sidecar), React + TypeScript, Vitest + Testing Library, WebdriverIO (tauri-driver).

**Spec:** `docs/superpowers/specs/2026-10-27-parity-batch4e-design.md`

## Global Constraints

- No new Rust or npm dependency; `docs/LICENSE_COMPLIANCE.md` is untouched.
- Constants: `HOOK_TIMEOUT = 600 s`, `OUTPUT_LIMIT = 64 * 1024` bytes (tail kept), `POLL_INTERVAL = 20 ms`, `DRAIN_GRACE = 2 s`, ETXTBSY retry 5 × 50 ms.
- Hook sets: `COMMIT_HOOKS = ["pre-commit", "prepare-commit-msg", "commit-msg"]`, `SKIPPABLE_HOOKS = ["pre-commit", "commit-msg", "pre-push"]`. `no_verify` skips exactly the skippable ones.
- Hook env: cwd = worktree root (git dir when bare); `GIT_DIR=<repo.path()>`; commit hooks also `GIT_INDEX_FILE=<repo.path()>/index`, `GIT_EDITOR=:`. Message file: `<repo.path()>/COMMIT_EDITMSG`.
- Copy, exactly:
  - `The <hook> hook failed.` (error banner and `CommitError::HookFailed` display; dialog body line)
  - `The <hook> hook was cancelled.` (`HookError::Cancelled`)
  - `Could not run the <hook> hook: <io error>` (`HookError::Spawn`)
  - `Aborting commit due to empty commit message.` (`CommitError::EmptyMessage`)
  - `Browsitory stopped the hook after 10 minutes.` (appended to a timed-out hook's output; the number comes from the timeout)
  - `Browsitory could not find sh.exe (install Git for Windows) to run this hook.` (Windows `NoShell` on a gate hook)
  - `[… earlier output omitted …]` (first line of truncated output)
  - `(no output)` (dialog, empty output)
  - `Running <hook> hook…` (CommitBox status line and TransferPanel)
  - Buttons: `Commit without hooks`, `Amend without hooks`, `Push without hooks`, `Close`, `Cancel`, `Cancelling…`, `Show hook output…`; checkbox `Skip hooks`; palette `Skip hooks for the next commit` / `Run hooks for the next commit`.
  - Dialog aria-label `<hook> hook failed`; output `<pre aria-label="Hook output">`.
- Wire: Tauri event `hook-progress` / sidecar notification `hookProgress`, payload `{ repoPath, operationId, hook, phase: "Started" | "Finished" }`. Commit/amend result `{ kind: "Committed", commitId }` | `{ kind: "HookFailed", hook, output }`. Params gain `noVerify` (sidecar: defaults to `false` when absent). `TransferProgress` gains `hookOutput: string | null`; phase `"RunningHook"`; error kind `"HookFailed"`. `HeadInfo` gains `commitHooks: string[]`.
- Operation ids for commit/amend: `commit-<n>` / `amend-<n>` from the transfer id counter; cancelled through the existing `cancel_transfer`.
- Logging of ignored hook problems: `eprintln!("browsitory: …")` in `repo-service` only; `git-core` never prints.
- Tests: `git-core`/`repo-service` use real temp-dir repos with real shell-script hooks, `#[cfg(unix)]` where a hook is executed; frontend mocks `RepoClient` only.
- Every push touching `crates/`, `frontend/src/` or `e2e/` needs a `CHANGELOG.md` update (pre-push hook); the changelog lands in Task 12, so push only after Task 12.

## Review Focus

1. **A pre-commit that stages files (lint-staged, formatters) must have its changes committed.** The cached libgit2 index is re-read only after a pre-commit actually ran. Pinned in Task 3 (`pre_commit_that_stages_a_file_gets_it_committed`).
2. **A hung hook must never wedge the repo forever.** Timeout kills the process group even when a background grandchild keeps the pipe open; Cancel works while the worker is blocked. Pinned in Task 1 (`timeout_kills_a_process_whose_grandchild_holds_the_pipe`) and Task 5 (`cancel_transfer_stops_a_running_pre_commit`).
3. **pre-push gets git's exact stdin** with the live remote SHA, and a failing pre-push leaves the remote untouched. Pinned in Task 4 (`pre_push_receives_args_and_ref_lines`, `failing_pre_push_blocks_the_push`).
4. **No typed message is lost on a failed commit.** Pinned in Task 9 (`keeps the message when the commit fails`).
5. **The E2E hook must not leak into later specs.** Pinned in Task 11's `after` hook.

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/git-core/src/process.rs` (new) | `run_captured`, `TailBuffer`, `clean_output`, process-group kill, inline tests |
| `crates/git-core/src/hooks.rs` (new) | discovery, `run_hook`, `run_reported`, `failure_output`, reporter/options types, `sh_candidates`, inline tests |
| `crates/git-core/src/lib.rs` | `pub mod hooks;`, `mod process;` |
| `crates/git-core/src/commit.rs` | commit hook sequence, `CommitOutcome`, new `CommitError` variants |
| `crates/git-core/src/remote.rs` | `pre-push` in `push_refs`, `no_verify`, `RunningHook`, `HookFailed` |
| `crates/git-core/src/repo.rs` | `HeadInfo.commit_hooks` |
| `crates/git-core/tests/hooks.rs` (new), `tests/common/mod.rs` | real-repo hook tests, `install_hook` helper |
| `crates/git-core/tests/{amend,pick,merge,signing_ssh,signing_gpg,remote}.rs` | call-site updates |
| `crates/repo-service/src/worker/hooks.rs` (new) | `HookEvent`, `ChannelHookReporter`, `log_hook_result`, `run_post_hook` |
| `crates/repo-service/src/worker/{mod,status,history,remote,branch,merge,undo}.rs` | commands, handlers, post hooks, tests |
| `crates/tauri-app/src/commands/{mod,status,history,remote}.rs` | params, DTOs, `emit_hook_events`, DTO tests |
| `crates/vscode-sidecar/src/dispatch.rs`, `tests/protocol_roundtrip.rs` | params, DTOs, `hookProgress` relay, round-trips |
| `frontend/src/ipc/RepoClient.ts`, `tauriRepoClient.ts`, `vscodeRepoClient.ts` (+ tests, every full fake) | types and methods |
| `frontend/src/lib/hookFailure.ts` (new) + test | retry types and copy helpers |
| `frontend/src/state/useHookActions.ts` (new), `useAppState.ts`, `useStagingActions.ts`, `useHistoryActions.ts`, `useRemoteTransferActions.ts` (+ `useAppState.test.ts`) | state |
| `frontend/src/components/HookFailureDialog.tsx`, `.module.css`, `.test.tsx` (new) | dialog |
| `frontend/src/components/CommitBox.tsx`, `.module.css`, `.test.tsx`; `DiffPane.tsx`; `TransferPanel.tsx` (+ test) | UI |
| `frontend/src/App.tsx`, `App.test.tsx`, `lib/commands.ts` (+ test) | wiring, palette |
| `e2e/specs/git-hooks.spec.ts` (new) | E2E |
| `CHANGELOG.md`, `docs/USER_GUIDE.md`, `docs/ARCHITECTURE.md` | docs |

---

### Task 0: Pre-flight (rebase and re-check)

Batches 4b, 4c, 4d (and possibly 4f/4g) and the E2E flakiness fixes (#98–#100) merge to `main` before this batch is implemented. This plan names their pre-4b shapes; reconcile before Task 1.

- [ ] **Step 1: Rebase**

```bash
git fetch origin
git rebase origin/main
```

- [ ] **Step 2: Re-check every name this plan touches**

```bash
graft skeleton crates/git-core/src/commit.rs
graft skeleton crates/git-core/src/remote.rs
graft skeleton crates/git-core/src/merge.rs
graft skeleton crates/git-core/src/repo.rs
graft callers push_refs
graft callers amend_head
graft callers start_merge
grep -n "Commit {\|AmendCommit {\|PushCurrentBranch {\|PushBranch {\|DeleteRemoteBranch {\|ForcePushCurrentBranch {\|PushTags {\|StartMerge {" crates/repo-service/src/worker/mod.rs
grep -n "pushCurrentBranch\|pushBranch\|deleteRemoteBranch\|forcePushCurrentBranch\|pushTags\|amendCommit\|commit(" frontend/src/ipc/RepoClient.ts
grep -rn "CommitBox" frontend/src/components/DiffPane.tsx
ls e2e/support
```

Apply these rules wherever the code moved:

| If on `main` … | Then in this plan … |
|---|---|
| 4b added `push_branch(repo, remote, local_branch, remote_branch, set_upstream, …)` and `push_current_branch` wraps it | Add `no_verify: bool` to `push_branch` (before `credentials`) and pass it through the wrapper. Same for `delete_remote_branch`. Task 4's tests use whichever public function pushes the current branch. |
| 4b added `Command::PushBranch`/`DeleteRemoteBranch` and RepoClient `pushBranch`/`deleteRemoteBranch` | Give them `no_verify` exactly like `PushCurrentBranch` in Tasks 6–8, and add `PushRequest` variants `{ operation: "PushBranchTo"; remoteName; localBranch; remoteBranch; setUpstream }` and `{ operation: "DeleteRemoteBranch"; remoteName; branch }` in Task 8 (retry calls the same action with `noVerify: true`). |
| 4c changed `start_merge(repo, branch, opts: MergeOptions)` / added a squash outcome | Task 6's post-merge passes `"1"` when `opts.squash`, else `"0"`, and runs for every successful non-conflicted outcome. |
| 4g added author/trailer parameters (or an options struct) to `commit`/`amend_head` | Keep them; add `hooks: HookOptions<'_>` as the **last** parameter. Trailers are applied to the message **before** `run_commit_hooks` (Task 3). |
| 4g added CommitBox controls | Put the "Skip hooks" checkbox directly under "Amend last commit" (Task 9). |
| `e2e/support/` gained helpers for textarea input / clicking hidden buttons | Use them in Task 11 instead of the inline `browser.execute` snippets. |

- [ ] **Step 3: Baseline**

Run: `cargo test --workspace && (cd frontend && pnpm test -- --run)`
Expected: PASS. Record any pre-existing failure before starting.

---

### Task 1: `git-core::process` — captured run with timeout and cancel

**Files:**
- Create: `crates/git-core/src/process.rs`
- Modify: `crates/git-core/src/lib.rs` (add `mod process;` after `mod line_patch;`)

**Interfaces:**
- Produces (used by Task 2): `pub(crate) const POLL_INTERVAL`, `DRAIN_GRACE`; `pub(crate) struct Captured { pub success: bool, pub output: String, pub timed_out: bool }`; `pub(crate) enum RunError { Spawn(std::io::Error), Wait(std::io::Error), Cancelled }`; `pub(crate) struct TailBuffer`; `pub(crate) fn clean_output(text: &str) -> String`; `pub(crate) fn run_captured(command: Command, stdin: Option<&[u8]>, cancel: &dyn Fn() -> bool, timeout: Duration, limit: usize) -> Result<Captured, RunError>`.

- [ ] **Step 1: Write the failing tests**

`process.rs` starts with the signatures above and `todo!()` bodies (mark the module `#![allow(dead_code)]` until Task 2 uses it), plus:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn tail_buffer_keeps_the_last_bytes_and_marks_truncation() {
        let mut buffer = TailBuffer::new(4);
        buffer.push(b"abc");
        buffer.push(b"defgh");
        assert_eq!(buffer.into_text(), "[… earlier output omitted …]\nefgh");

        let mut short = TailBuffer::new(8);
        short.push(b"ok\n");
        assert_eq!(short.into_text(), "ok\n");
    }

    #[test]
    fn clean_output_strips_escapes_and_carriage_return_redraws() {
        assert_eq!(clean_output("\u{1b}[31merror\u{1b}[0m: bad\r\n"), "error: bad\n");
        assert_eq!(clean_output("\u{1b}]0;title\u{7}done"), "done");
        assert_eq!(clean_output("10%\r50%\r100%\nnext"), "100%\nnext");
    }

    #[cfg(unix)]
    fn sh(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    }

    #[cfg(unix)]
    #[test]
    fn combines_stdout_and_stderr_in_order() {
        let captured = run_captured(
            sh("echo one; echo two >&2; echo three"),
            None,
            &|| false,
            Duration::from_secs(10),
            1024,
        )
        .unwrap();
        assert!(captured.success);
        assert!(!captured.timed_out);
        assert_eq!(captured.output, "one\ntwo\nthree\n");
    }

    #[cfg(unix)]
    #[test]
    fn delivers_stdin_and_reports_failure() {
        let captured = run_captured(
            sh("cat; exit 3"),
            Some(b"from stdin\n"),
            &|| false,
            Duration::from_secs(10),
            1024,
        )
        .unwrap();
        assert!(!captured.success);
        assert_eq!(captured.output, "from stdin\n");
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_a_process_whose_grandchild_holds_the_pipe() {
        let started = Instant::now();
        let captured = run_captured(
            sh("sleep 30 & echo started; sleep 30"),
            None,
            &|| false,
            Duration::from_millis(300),
            1024,
        )
        .unwrap();
        assert!(captured.timed_out);
        assert!(!captured.success);
        assert_eq!(captured.output, "started\n");
        assert!(started.elapsed() < Duration::from_secs(5), "took {:?}", started.elapsed());
    }

    #[cfg(unix)]
    #[test]
    fn cancel_stops_the_process() {
        let started = Instant::now();
        let result = run_captured(sh("sleep 30"), None, &|| true, Duration::from_secs(60), 1024);
        assert!(matches!(result, Err(RunError::Cancelled)));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_missing_program_is_a_spawn_error() {
        let result = run_captured(
            Command::new("browsitory-no-such-program"),
            None,
            &|| false,
            Duration::from_secs(1),
            1024,
        );
        assert!(matches!(result, Err(RunError::Spawn(error)) if error.kind() == std::io::ErrorKind::NotFound));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --lib process::tests`
Expected: FAIL (panics at `todo!()`).

- [ ] **Step 3: Implement**

```rust
//! Runs an external program with combined, size-capped output, a timeout and a cancel check.
//! Used by `hooks`; see `docs/superpowers/specs/2026-10-27-parity-batch4e-design.md`.

use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(20);
/// How long to keep reading after the program exited. A background grandchild that inherited
/// the pipe would otherwise keep it open (and this call blocked) for as long as it lives.
pub(crate) const DRAIN_GRACE: Duration = Duration::from_secs(2);
const OMITTED: &str = "[… earlier output omitted …]\n";

#[derive(Debug)]
pub(crate) struct Captured {
    pub success: bool,
    pub output: String,
    pub timed_out: bool,
}

#[derive(Debug)]
pub(crate) enum RunError {
    Spawn(std::io::Error),
    Wait(std::io::Error),
    Cancelled,
}

/// Keeps the last `limit` bytes: a failing hook prints its error at the end.
pub(crate) struct TailBuffer {
    limit: usize,
    bytes: Vec<u8>,
    truncated: bool,
}

impl TailBuffer {
    pub(crate) fn new(limit: usize) -> Self {
        Self { limit, bytes: Vec::new(), truncated: false }
    }

    pub(crate) fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend_from_slice(chunk);
        // Trim in batches (at 2× the limit) so a chatty program costs amortized O(1) per byte.
        if self.bytes.len() > self.limit.saturating_mul(2) {
            self.trim();
        }
    }

    fn trim(&mut self) {
        if self.bytes.len() > self.limit {
            let excess = self.bytes.len() - self.limit;
            self.bytes.drain(..excess);
            self.truncated = true;
        }
    }

    pub(crate) fn into_text(mut self) -> String {
        self.trim();
        let text = clean_output(&String::from_utf8_lossy(&self.bytes));
        if self.truncated {
            format!("{OMITTED}{text}")
        } else {
            text
        }
    }
}

/// Removes ANSI escape sequences (CSI `ESC [ … final`, OSC `ESC ] … BEL|ESC \`, two-byte
/// `ESC x`), turns `\r\n` into `\n`, and keeps only the text after the last `\r` of each line so
/// progress redraws collapse to their final state.
pub(crate) fn clean_output(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            plain.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                for next in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&next) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(next) = chars.next() {
                    if next == '\u{7}' {
                        break;
                    }
                    if next == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    plain
        .replace("\r\n", "\n")
        .split('\n')
        .map(|line| line.rsplit('\r').next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(unix)]
fn spawn_retrying(command: &mut Command) -> std::io::Result<Child> {
    // ETXTBSY: a script written a moment ago can still be open for writing in a child forked by
    // another thread (parallel tests, or the user's editor). It clears within milliseconds.
    const ETXTBSY: i32 = 26;
    let mut attempts = 0;
    loop {
        match command.spawn() {
            Err(error) if error.raw_os_error() == Some(ETXTBSY) && attempts < 5 => {
                attempts += 1;
                std::thread::sleep(Duration::from_millis(50));
            }
            other => return other,
        }
    }
}

#[cfg(not(unix))]
fn spawn_retrying(command: &mut Command) -> std::io::Result<Child> {
    command.spawn()
}

/// Kills the program and, on Unix, every process in its group (it was spawned as the group
/// leader), so a hook's test runner or linter dies with it. Uses the POSIX `kill` utility rather
/// than a `libc` dependency. On Windows only the direct child is terminated.
fn kill_tree(child: &mut Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-s", "KILL", "--", &format!("-{}", child.id())])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

pub(crate) fn run_captured(
    mut command: Command,
    stdin: Option<&[u8]>,
    cancel: &dyn Fn() -> bool,
    timeout: Duration,
    limit: usize,
) -> Result<Captured, RunError> {
    let (mut reader, writer) = std::io::pipe().map_err(RunError::Spawn)?;
    let writer_for_stderr = writer.try_clone().map_err(RunError::Spawn)?;
    command
        .stdin(if stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(writer)
        .stderr(writer_for_stderr);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = spawn_retrying(&mut command).map_err(RunError::Spawn)?;
    // `command` still owns the pipe's write ends; until they close the reader never sees EOF.
    drop(command);

    let buffer = Arc::new(Mutex::new(TailBuffer::new(limit)));
    let (done_tx, done_rx) = mpsc::channel();
    {
        let buffer = Arc::clone(&buffer);
        std::thread::spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => buffer
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(&chunk[..read]),
                }
            }
            let _ = done_tx.send(());
        });
    }
    if let Some(input) = stdin {
        let mut child_stdin = child.stdin.take().expect("stdin is piped");
        let input = input.to_vec();
        // Written from its own thread so a program that prints a lot before reading stdin cannot
        // deadlock against us (same reasoning as `signing::run_program`). A program that exits
        // without reading closes its end; that is reported through its exit status.
        std::thread::spawn(move || {
            let _ = child_stdin.write_all(&input);
        });
    }

    let started = Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                kill_tree(&mut child);
                return Err(RunError::Wait(error));
            }
        }
        if cancel() {
            kill_tree(&mut child);
            let _ = child.wait();
            return Err(RunError::Cancelled);
        }
        if started.elapsed() >= timeout {
            timed_out = true;
            kill_tree(&mut child);
            break child.wait().map_err(RunError::Wait)?;
        }
        std::thread::sleep(POLL_INTERVAL);
    };
    let _ = done_rx.recv_timeout(DRAIN_GRACE);
    let collected = std::mem::replace(
        &mut *buffer.lock().unwrap_or_else(|error| error.into_inner()),
        TailBuffer::new(limit),
    );
    Ok(Captured {
        success: status.success() && !timed_out,
        output: collected.into_text(),
        timed_out,
    })
}
```

Note for `timeout_kills_…`: the process group kill takes the backgrounded `sleep` with it, so the pipe closes promptly; `DRAIN_GRACE` is only the fallback when a grandchild escaped the group.

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core --lib process::tests`
Expected: PASS (7 tests on Unix, 3 on Windows).

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy -p git-core --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/git-core/src/process.rs crates/git-core/src/lib.rs
git commit -m "feat(git-core): captured process runner with timeout and cancel"
```

---

### Task 2: `git-core::hooks` — discovery and `run_hook`

**Files:**
- Create: `crates/git-core/src/hooks.rs`, `crates/git-core/tests/hooks.rs`
- Modify: `crates/git-core/src/lib.rs` (`pub mod hooks;` after `pub mod history;`), `crates/git-core/src/process.rs` (drop the `#![allow(dead_code)]`), `crates/git-core/tests/common/mod.rs` (`install_hook`, and `init_repo` pins `core.hooksPath`)

**Interfaces:**
- Consumes: Task 1's `run_captured`, `RunError`.
- Produces (used by Tasks 3–6): everything in the spec's `hooks.rs` block — `HOOK_TIMEOUT`, `OUTPUT_LIMIT`, `COMMIT_HOOKS`, `SKIPPABLE_HOOKS`, `HookRun`, `HookOutcome`, `HookError`, `HookLookup`, `HookReporter`, `HookOptions` (+ `Default`), `HookInvocation`, `hooks_dir`, `find_hook`, `installed_hooks`, `sh_candidates`, `run_hook`, `run_reported`, `failure_output`.

- [ ] **Step 1: Test helpers**

In `tests/common/mod.rs`, inside `init_repo`'s config block, pin the hooks dir so a developer's global `core.hooksPath` never runs real hooks in the test suite (the value equals git's default):

```rust
        let hooks = repo.path().join("hooks");
        config
            .set_str("core.hooksPath", hooks.to_str().expect("utf-8 temp path"))
            .unwrap();
```

and add:

```rust
/// Writes an executable `#!/bin/sh` hook `name` into `dir` (created if missing).
#[cfg(unix)]
#[allow(dead_code)]
pub fn install_hook(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir).expect("create hooks dir");
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write hook");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod hook");
    path
}
```

- [ ] **Step 2: Write the failing tests**

Inline in `hooks.rs` (all platforms):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sh_candidates_cover_git_for_windows_layouts() {
        let cmd = sh_candidates(Path::new("C:/Program Files/Git/cmd/git.exe"));
        assert!(cmd.contains(&PathBuf::from("C:/Program Files/Git/bin/sh.exe")));
        let mingw = sh_candidates(Path::new("C:/Program Files/Git/mingw64/bin/git.exe"));
        assert!(mingw.contains(&PathBuf::from("C:/Program Files/Git/usr/bin/sh.exe")));
        let bin = sh_candidates(Path::new("C:/Git/bin/git.exe"));
        assert_eq!(bin[0], PathBuf::from("C:/Git/bin/sh.exe"));
    }

    #[test]
    fn failure_output_notes_a_timeout() {
        let outcome = HookOutcome::Ran(HookRun { success: false, output: "slow\n".into(), timed_out: true });
        assert_eq!(
            failure_output(&outcome, Duration::from_secs(600)).unwrap(),
            "slow\nBrowsitory stopped the hook after 10 minutes."
        );
        let ok = HookOutcome::Ran(HookRun { success: true, output: String::new(), timed_out: false });
        assert_eq!(failure_output(&ok, HOOK_TIMEOUT), None);
        assert_eq!(failure_output(&HookOutcome::NotInstalled, HOOK_TIMEOUT), None);
    }
}
```

`tests/hooks.rs`:

```rust
#![cfg(unix)]
mod common;

use std::time::Duration;

use git_core::hooks::{
    find_hook, hooks_dir, installed_hooks, run_hook, HookInvocation, HookLookup, HookOutcome,
    COMMIT_HOOKS,
};

fn run(repo: &git2::Repository, name: &str, args: &[&str]) -> HookOutcome {
    run_hook(
        repo,
        &HookInvocation { name, args, stdin: None, env: &[] },
        &|| false,
        Duration::from_secs(30),
    )
    .unwrap()
}

#[test]
fn hooks_dir_defaults_to_the_common_dir_hooks() {
    let (_dir, repo) = common::init_repo();
    repo.config().unwrap().remove("core.hooksPath").unwrap();
    if repo.config().unwrap().get_path("core.hooksPath").is_ok() {
        eprintln!("global core.hooksPath is set; skipping the default-path check");
        return;
    }
    assert_eq!(hooks_dir(&repo).unwrap(), repo.commondir().join("hooks"));
}

#[test]
fn relative_core_hooks_path_is_resolved_from_the_worktree_root() {
    let (dir, repo) = common::init_repo();
    repo.config().unwrap().set_str("core.hooksPath", ".githooks").unwrap();
    assert_eq!(hooks_dir(&repo).unwrap(), dir.path().join(".githooks"));
    common::install_hook(&dir.path().join(".githooks"), "pre-commit", "echo from githooks");
    assert!(matches!(run(&repo, "pre-commit", &[]), HookOutcome::Ran(run) if run.output == "from githooks\n"));
}

#[test]
fn a_missing_hook_is_not_installed_and_a_non_executable_one_is_not_run() {
    let (_dir, repo) = common::init_repo();
    assert_eq!(run(&repo, "pre-commit", &[]), HookOutcome::NotInstalled);
    let hooks = hooks_dir(&repo).unwrap();
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join("pre-commit"), "#!/bin/sh\nexit 1\n").unwrap();
    assert!(matches!(find_hook(&repo, "pre-commit").unwrap(), HookLookup::NotExecutable(_)));
    assert_eq!(run(&repo, "pre-commit", &[]), HookOutcome::NotExecutable);
    assert!(installed_hooks(&repo, &COMMIT_HOOKS).is_empty());
}

#[test]
fn a_hook_runs_in_the_worktree_root_with_git_dir_and_args() {
    let (dir, repo) = common::init_repo();
    common::install_hook(&hooks_dir(&repo).unwrap(), "post-checkout", r#"pwd; echo "$GIT_DIR"; echo "$@""#);
    let HookOutcome::Ran(result) = run(&repo, "post-checkout", &["a", "b", "1"]) else {
        panic!("hook did not run");
    };
    assert!(result.success);
    let lines: Vec<&str> = result.output.lines().collect();
    assert_eq!(std::fs::canonicalize(lines[0]).unwrap(), std::fs::canonicalize(dir.path()).unwrap());
    assert_eq!(
        std::fs::canonicalize(lines[1]).unwrap(),
        std::fs::canonicalize(repo.path()).unwrap()
    );
    assert_eq!(lines[2], "a b 1");
}

#[test]
fn a_hook_without_a_shebang_runs_through_sh() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, repo) = common::init_repo();
    let hooks = hooks_dir(&repo).unwrap();
    std::fs::create_dir_all(&hooks).unwrap();
    let path = hooks.join("pre-commit");
    std::fs::write(&path, "echo no shebang\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(run(&repo, "pre-commit", &[]), HookOutcome::Ran(run) if run.success && run.output == "no shebang\n"));
}

#[test]
fn installed_hooks_lists_executable_commit_hooks() {
    let (_dir, repo) = common::init_repo();
    let hooks = hooks_dir(&repo).unwrap();
    common::install_hook(&hooks, "commit-msg", "exit 0");
    common::install_hook(&hooks, "pre-push", "exit 0");
    assert_eq!(installed_hooks(&repo, &COMMIT_HOOKS), vec!["commit-msg".to_string()]);
}
```

And a Windows-only test file section at the end of `hooks.rs`'s inline tests:

```rust
    #[cfg(windows)]
    #[test]
    fn a_failing_hook_runs_through_the_discovered_sh() {
        let Some(_) = find_sh() else {
            eprintln!("no sh.exe found; skipping");
            return;
        };
        let dir = tempfile::TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        repo.config().unwrap().set_str("core.hooksPath", dir.path().join("h").to_str().unwrap()).unwrap();
        std::fs::create_dir_all(dir.path().join("h")).unwrap();
        std::fs::write(dir.path().join("h").join("pre-commit"), "#!/bin/sh\necho nope >&2\nexit 1\n").unwrap();
        let outcome = run_hook(
            &repo,
            &HookInvocation { name: "pre-commit", args: &[], stdin: None, env: &[] },
            &|| false,
            Duration::from_secs(30),
        )
        .unwrap();
        assert!(matches!(outcome, HookOutcome::Ran(run) if !run.success && run.output.contains("nope")));
    }
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p git-core --lib hooks::tests && cargo test -p git-core --test hooks`
Expected: compile failure (module missing), then FAIL once stubs exist.

- [ ] **Step 4: Implement `hooks.rs`**

```rust
//! Client-side git hooks: discovery (`core.hooksPath`), running them the way git does, and the
//! reporter/options the commit and push paths use. See
//! `docs/superpowers/specs/2026-10-27-parity-batch4e-design.md`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use git2::Repository;
use thiserror::Error;

use crate::process::{run_captured, RunError};

pub const HOOK_TIMEOUT: Duration = Duration::from_secs(600);
pub const OUTPUT_LIMIT: usize = 64 * 1024;
/// The hooks `git commit` runs before writing the commit; what the "Skip hooks" UI is about.
pub const COMMIT_HOOKS: [&str; 3] = ["pre-commit", "prepare-commit-msg", "commit-msg"];
/// What `--no-verify` skips.
pub const SKIPPABLE_HOOKS: [&str; 3] = ["pre-commit", "commit-msg", "pre-push"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRun {
    pub success: bool,
    pub output: String,
    pub timed_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome {
    NotInstalled,
    /// Unix: the file exists without an execute bit. Not run, like git.
    NotExecutable,
    /// Windows: no `sh.exe` next to `git.exe` or on `PATH`.
    NoShell,
    Ran(HookRun),
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HookError {
    #[error("Could not run the {hook} hook: {message}")]
    Spawn { hook: String, message: String },
    #[error("The {hook} hook was cancelled.")]
    Cancelled { hook: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookLookup {
    Missing,
    NotExecutable(PathBuf),
    Found(PathBuf),
}

/// Told about every hook that was found: `started` right before it is spawned, `finished` with
/// its result (also for `NotExecutable`/`NoShell`, which never start).
pub trait HookReporter {
    fn started(&mut self, hook: &str);
    fn finished(&mut self, hook: &str, result: &Result<HookOutcome, HookError>);
}

pub struct HookOptions<'a> {
    /// `--no-verify`: skip `SKIPPABLE_HOOKS`.
    pub no_verify: bool,
    pub cancel: Option<&'a dyn Fn() -> bool>,
    pub reporter: Option<&'a mut dyn HookReporter>,
    pub timeout: Duration,
}

impl Default for HookOptions<'_> {
    fn default() -> Self {
        Self { no_verify: false, cancel: None, reporter: None, timeout: HOOK_TIMEOUT }
    }
}

pub struct HookInvocation<'a> {
    pub name: &'a str,
    pub args: &'a [&'a str],
    pub stdin: Option<&'a [u8]>,
    pub env: &'a [(&'a str, &'a OsStr)],
}

/// Where hooks run: the worktree root, or the git dir of a bare repository.
fn run_dir(repo: &Repository) -> &Path {
    repo.workdir().unwrap_or_else(|| repo.path())
}

/// `core.hooksPath` (relative to the worktree root) or `<common dir>/hooks`, so linked
/// worktrees share the main repository's hooks.
pub fn hooks_dir(repo: &Repository) -> Result<PathBuf, git2::Error> {
    match repo.config()?.get_path("core.hooksPath") {
        Ok(path) if path.is_absolute() => Ok(path),
        Ok(path) => Ok(run_dir(repo).join(path)),
        Err(error) if error.code() == git2::ErrorCode::NotFound => {
            Ok(repo.commondir().join("hooks"))
        }
        Err(error) => Err(error),
    }
}

pub fn find_hook(repo: &Repository, name: &str) -> Result<HookLookup, git2::Error> {
    let path = hooks_dir(repo)?.join(name);
    let Ok(metadata) = std::fs::metadata(&path) else {
        return Ok(HookLookup::Missing);
    };
    if !metadata.is_file() {
        return Ok(HookLookup::Missing);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Ok(HookLookup::NotExecutable(path));
        }
    }
    Ok(HookLookup::Found(path))
}

pub fn installed_hooks(repo: &Repository, names: &[&str]) -> Vec<String> {
    names
        .iter()
        .filter(|name| matches!(find_hook(repo, name), Ok(HookLookup::Found(_))))
        .map(|name| name.to_string())
        .collect()
}

/// Where Git for Windows keeps `sh.exe` relative to a `git.exe` found on `PATH`
/// (`Git\cmd\git.exe`, `Git\bin\git.exe`, `Git\mingw64\bin\git.exe`), nearest first.
pub fn sh_candidates(git_exe: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut dir = git_exe.parent();
    for _ in 0..3 {
        let Some(current) = dir else { break };
        for relative in ["sh.exe", "bin/sh.exe", "usr/bin/sh.exe"] {
            let candidate = current.join(relative);
            if !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
        dir = current.parent();
    }
    candidates
}

#[allow(dead_code)] // used on Windows only
fn find_on_path(file: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(file))
        .find(|path| path.is_file())
}

#[cfg(windows)]
fn find_sh() -> Option<PathBuf> {
    find_on_path("git.exe")
        .and_then(|git| sh_candidates(&git).into_iter().find(|path| path.is_file()))
        .or_else(|| find_on_path("sh.exe"))
}

#[cfg(unix)]
fn hook_command(path: &Path) -> Option<Command> {
    Some(Command::new(path))
}

#[cfg(windows)]
fn hook_command(path: &Path) -> Option<Command> {
    let mut command = Command::new(find_sh()?);
    command.arg(path.to_string_lossy().replace('\\', "/"));
    Some(command)
}

fn configure(command: &mut Command, repo: &Repository, invocation: &HookInvocation<'_>) {
    command
        .args(invocation.args)
        .current_dir(run_dir(repo))
        .env("GIT_DIR", repo.path());
    for (key, value) in invocation.env {
        command.env(key, value);
    }
}

fn captured_outcome(
    hook: &str,
    result: Result<crate::process::Captured, RunError>,
) -> Result<HookOutcome, HookError> {
    match result {
        Ok(captured) => Ok(HookOutcome::Ran(HookRun {
            success: captured.success,
            output: captured.output,
            timed_out: captured.timed_out,
        })),
        Err(RunError::Cancelled) => Err(HookError::Cancelled { hook: hook.to_string() }),
        Err(RunError::Spawn(error)) | Err(RunError::Wait(error)) => Err(HookError::Spawn {
            hook: hook.to_string(),
            message: error.to_string(),
        }),
    }
}

pub fn run_hook(
    repo: &Repository,
    invocation: &HookInvocation<'_>,
    cancel: &dyn Fn() -> bool,
    timeout: Duration,
) -> Result<HookOutcome, HookError> {
    let spawn_error = |message: String| HookError::Spawn { hook: invocation.name.to_string(), message };
    let path = match find_hook(repo, invocation.name) {
        Ok(HookLookup::Found(path)) => path,
        Ok(HookLookup::Missing) => return Ok(HookOutcome::NotInstalled),
        Ok(HookLookup::NotExecutable(_)) => return Ok(HookOutcome::NotExecutable),
        Err(error) => return Err(spawn_error(error.message().to_string())),
    };
    let Some(mut command) = hook_command(&path) else {
        return Ok(HookOutcome::NoShell);
    };
    configure(&mut command, repo, invocation);
    let result = run_captured(command, invocation.stdin, cancel, timeout, OUTPUT_LIMIT);
    #[cfg(unix)]
    {
        // No shebang: exec fails with ENOEXEC and git runs the file with sh instead.
        const ENOEXEC: i32 = 8;
        if let Err(RunError::Spawn(error)) = &result {
            if error.raw_os_error() == Some(ENOEXEC) {
                let mut command = Command::new("sh");
                command.arg(&path);
                configure(&mut command, repo, invocation);
                return captured_outcome(
                    invocation.name,
                    run_captured(command, invocation.stdin, cancel, timeout, OUTPUT_LIMIT),
                );
            }
        }
    }
    captured_outcome(invocation.name, result)
}

pub fn run_reported(
    repo: &Repository,
    invocation: &HookInvocation<'_>,
    options: &mut HookOptions<'_>,
) -> Result<HookOutcome, HookError> {
    let never = || false;
    let cancel: &dyn Fn() -> bool = match options.cancel {
        Some(cancel) => cancel,
        None => &never,
    };
    if matches!(find_hook(repo, invocation.name), Ok(HookLookup::Found(_))) {
        if let Some(reporter) = options.reporter.as_deref_mut() {
            reporter.started(invocation.name);
        }
    }
    let result = run_hook(repo, invocation, cancel, options.timeout);
    if !matches!(result, Ok(HookOutcome::NotInstalled)) {
        if let Some(reporter) = options.reporter.as_deref_mut() {
            reporter.finished(invocation.name, &result);
        }
    }
    result
}

fn describe(timeout: Duration) -> String {
    let seconds = timeout.as_secs();
    if seconds >= 60 && seconds % 60 == 0 {
        format!("{} minutes", seconds / 60)
    } else {
        format!("{seconds} seconds")
    }
}

/// What a failed gate hook shows the user; `None` when the outcome lets the operation proceed.
pub fn failure_output(outcome: &HookOutcome, timeout: Duration) -> Option<String> {
    match outcome {
        HookOutcome::Ran(run) if !run.success => {
            let mut text = run.output.clone();
            if run.timed_out {
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(&format!("Browsitory stopped the hook after {}.", describe(timeout)));
            }
            Some(text)
        }
        HookOutcome::NoShell => Some(
            "Browsitory could not find sh.exe (install Git for Windows) to run this hook."
                .to_string(),
        ),
        _ => None,
    }
}
```

(`failure_output_notes_a_timeout` expects `"slow\nBrowsitory stopped…"`: a newline is added only when the output does not already end with one.)

- [ ] **Step 5: Run tests**

Run: `cargo test -p git-core --lib hooks::tests && cargo test -p git-core --test hooks`
Expected: PASS.

- [ ] **Step 6: Lint, full git-core suite, commit**

Run: `cargo clippy -p git-core --all-targets -- -D warnings && cargo fmt --all -- --check && cargo test -p git-core`
Expected: PASS (the `core.hooksPath` pin in `init_repo` changes nothing else).

```bash
git add crates/git-core/src/hooks.rs crates/git-core/src/process.rs crates/git-core/src/lib.rs crates/git-core/tests/hooks.rs crates/git-core/tests/common/mod.rs
git commit -m "feat(git-core): hook discovery and runner"
```

---

### Task 3: Commit hooks in `commit` / `amend_head`, `HeadInfo.commit_hooks`

**Files:**
- Modify: `crates/git-core/src/commit.rs`, `crates/git-core/src/repo.rs`
- Modify (call sites, add `HookOptions::default()`): `crates/git-core/tests/{amend,pick,merge,signing_ssh,signing_gpg}.rs`, `crates/repo-service/src/worker/{status,history,mod}.rs` (temporary: `HookOptions::default()`; Task 5 replaces it), `crates/tauri-app/src/commands/mod.rs` (`HeadInfo { …, commit_hooks: vec![] }` in its DTO test)
- Test: `crates/git-core/tests/hooks.rs`

**Interfaces:**
- Consumes: Task 2.
- Produces: `CommitOutcome { Committed { commit_id }, HookFailed { hook, output } }`; `CommitError::{HookFailed { hook, output }, Hook(HookError), EmptyMessage, Io(std::io::Error)}`; `commit(repo: &mut Repository, message: &str, hooks: HookOptions<'_>) -> Result<String, CommitError>`; `amend_head(repo: &Repository, message: &str, hooks: HookOptions<'_>) -> Result<String, CommitError>`; `HeadInfo.commit_hooks: Vec<String>`.

- [ ] **Step 1: Write the failing tests** (append to `tests/hooks.rs`)

```rust
use git_core::commit::{amend_head, commit, CommitError};
use git_core::hooks::{HookError, HookOptions, HookReporter};

fn stage(dir: &std::path::Path, repo: &git2::Repository, path: &str, contents: &str) {
    common::write_file(dir, path, contents);
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new(path)).unwrap();
    index.write().unwrap();
}

fn head_summary(repo: &git2::Repository) -> String {
    repo.head().unwrap().peel_to_commit().unwrap().message().unwrap().to_string()
}

#[test]
fn failing_pre_commit_blocks_the_commit_and_returns_its_output() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::install_hook(&hooks_dir(&repo).unwrap(), "pre-commit", "echo checking; echo 'lint failed' >&2; exit 1");

    let error = commit(&mut repo, "add a", HookOptions::default()).unwrap_err();

    match error {
        CommitError::HookFailed { hook, output } => {
            assert_eq!(hook, "pre-commit");
            assert_eq!(output, "checking\nlint failed\n");
        }
        other => panic!("expected HookFailed, got {other:?}"),
    }
    assert!(repo.head().is_err(), "no commit may be created");
}

#[test]
fn pre_commit_that_stages_a_file_gets_it_committed() {
    if !common::program_available("git") {
        eprintln!("git CLI not found; skipping");
        return;
    }
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::write_file(dir.path(), "generated.txt", "made by the hook");
    common::install_hook(&hooks_dir(&repo).unwrap(), "pre-commit", "git add generated.txt");

    commit(&mut repo, "add a", HookOptions::default()).unwrap();

    let tree = repo.head().unwrap().peel_to_tree().unwrap();
    assert!(tree.get_name("generated.txt").is_some());
}

#[test]
fn commit_msg_can_rewrite_the_message() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::install_hook(&hooks_dir(&repo).unwrap(), "commit-msg", r#"printf 'rewritten\n\nIssue: 42\n' > "$1""#);

    commit(&mut repo, "original", HookOptions::default()).unwrap();

    assert_eq!(head_summary(&repo), "rewritten\n\nIssue: 42\n");
}

#[test]
fn prepare_commit_msg_gets_the_file_and_source() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::install_hook(
        &hooks_dir(&repo).unwrap(),
        "prepare-commit-msg",
        r#"case "$1" in */COMMIT_EDITMSG) ;; *) exit 1 ;; esac; echo "[$2]" >> "$1""#,
    );

    commit(&mut repo, "subject\n", HookOptions::default()).unwrap();

    assert_eq!(head_summary(&repo), "subject\n[message]\n");
}

#[test]
fn no_verify_skips_pre_commit_and_commit_msg_but_not_prepare_commit_msg() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    let hooks = hooks_dir(&repo).unwrap();
    common::install_hook(&hooks, "pre-commit", "exit 1");
    common::install_hook(&hooks, "commit-msg", "exit 1");
    common::install_hook(&hooks, "prepare-commit-msg", r#"echo prepared >> "$1""#);

    commit(&mut repo, "skip\n", HookOptions { no_verify: true, ..HookOptions::default() }).unwrap();

    assert_eq!(head_summary(&repo), "skip\nprepared\n");
}

#[test]
fn a_hook_that_empties_the_message_aborts() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::install_hook(&hooks_dir(&repo).unwrap(), "commit-msg", r#": > "$1""#);

    let error = commit(&mut repo, "gone", HookOptions::default()).unwrap_err();

    assert!(matches!(error, CommitError::EmptyMessage));
    assert_eq!(error.to_string(), "Aborting commit due to empty commit message.");
}

#[test]
fn post_commit_failure_does_not_fail_the_commit() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::install_hook(&hooks_dir(&repo).unwrap(), "post-commit", "exit 1");

    commit(&mut repo, "kept", HookOptions::default()).unwrap();

    assert_eq!(head_summary(&repo), "kept");
}

#[test]
fn amend_runs_the_commit_hooks() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    commit(&mut repo, "first", HookOptions::default()).unwrap();
    let before = repo.head().unwrap().target().unwrap();
    common::install_hook(&hooks_dir(&repo).unwrap(), "pre-commit", "echo no >&2; exit 1");

    let error = amend_head(&repo, "amended", HookOptions::default()).unwrap_err();
    assert!(matches!(error, CommitError::HookFailed { ref hook, .. } if hook == "pre-commit"));
    assert_eq!(repo.head().unwrap().target().unwrap(), before);

    amend_head(&repo, "amended", HookOptions { no_verify: true, ..HookOptions::default() }).unwrap();
    assert_eq!(head_summary(&repo), "amended");
}

#[test]
fn cancel_kills_a_running_pre_commit() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    common::install_hook(&hooks_dir(&repo).unwrap(), "pre-commit", "sleep 30");
    let started = std::time::Instant::now();

    let error = commit(&mut repo, "x", HookOptions { cancel: Some(&|| true), ..HookOptions::default() }).unwrap_err();

    assert!(matches!(error, CommitError::Hook(HookError::Cancelled { .. })));
    assert_eq!(error.to_string(), "The pre-commit hook was cancelled.");
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(repo.head().is_err());
}

#[derive(Default)]
struct Recorder(Vec<String>);
impl HookReporter for Recorder {
    fn started(&mut self, hook: &str) {
        self.0.push(format!("start {hook}"));
    }
    fn finished(&mut self, hook: &str, _result: &Result<HookOutcome, HookError>) {
        self.0.push(format!("end {hook}"));
    }
}

#[test]
fn the_reporter_sees_only_hooks_that_exist_in_order() {
    let (dir, mut repo) = common::init_repo();
    stage(dir.path(), &repo, "a.txt", "a");
    let hooks = hooks_dir(&repo).unwrap();
    common::install_hook(&hooks, "pre-commit", "exit 0");
    common::install_hook(&hooks, "post-commit", "exit 0");
    let mut recorder = Recorder::default();

    commit(&mut repo, "x", HookOptions { reporter: Some(&mut recorder), ..HookOptions::default() }).unwrap();

    assert_eq!(recorder.0, vec!["start pre-commit", "end pre-commit", "start post-commit", "end post-commit"]);
}

#[test]
fn head_info_lists_installed_commit_hooks_even_when_unborn() {
    let (_dir, repo) = common::init_repo();
    common::install_hook(&hooks_dir(&repo).unwrap(), "pre-commit", "exit 0");
    assert_eq!(git_core::repo::head_info(&repo).unwrap().commit_hooks, vec!["pre-commit".to_string()]);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test hooks`
Expected: compile errors (`commit` takes two arguments; no `commit_hooks`).

- [ ] **Step 3: Implement**

In `commit.rs`, add the outcome and variants:

```rust
use std::ffi::OsStr;

use crate::hooks::{self, HookInvocation, HookLookup, HookOptions};

/// What a commit or amend produced, as it crosses IPC: a hook failure is an expected outcome,
/// not an error, like `pick::PickOutcome::Conflicted`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitOutcome {
    Committed { commit_id: String },
    HookFailed { hook: String, output: String },
}
```

```rust
    #[error("The {hook} hook failed.")]
    HookFailed { hook: String, output: String },
    #[error("{0}")]
    Hook(#[from] hooks::HookError),
    #[error("Aborting commit due to empty commit message.")]
    EmptyMessage,
    #[error("could not write the commit message file: {0}")]
    Io(#[from] std::io::Error),
```

Shared hook sequence (private):

```rust
struct HookedMessage {
    message: String,
    /// A pre-commit ran, so the on-disk index may differ from the cached one.
    reload_index: bool,
}

fn gate(
    repo: &git2::Repository,
    invocation: &HookInvocation<'_>,
    options: &mut HookOptions<'_>,
) -> Result<bool, CommitError> {
    let outcome = hooks::run_reported(repo, invocation, options)?;
    if let Some(output) = hooks::failure_output(&outcome, options.timeout) {
        return Err(CommitError::HookFailed { hook: invocation.name.to_string(), output });
    }
    Ok(matches!(outcome, hooks::HookOutcome::Ran(_)))
}

fn installed(repo: &git2::Repository, name: &str) -> Result<bool, CommitError> {
    Ok(matches!(hooks::find_hook(repo, name)?, HookLookup::Found(_)))
}

/// `pre-commit` → `prepare-commit-msg` → `commit-msg`, as `git commit -m` runs them.
fn run_commit_hooks(
    repo: &git2::Repository,
    message: &str,
    source: &str,
    options: &mut HookOptions<'_>,
) -> Result<HookedMessage, CommitError> {
    let index_path = repo.path().join("index");
    let env: [(&str, &OsStr); 2] = [
        ("GIT_INDEX_FILE", index_path.as_os_str()),
        ("GIT_EDITOR", OsStr::new(":")),
    ];
    let mut reload_index = false;
    if !options.no_verify {
        let pre_commit = HookInvocation { name: "pre-commit", args: &[], stdin: None, env: &env };
        reload_index = gate(repo, &pre_commit, options)?;
    }
    let message_hooks = installed(repo, "prepare-commit-msg")?
        || (!options.no_verify && installed(repo, "commit-msg")?);
    if !message_hooks {
        return Ok(HookedMessage { message: message.to_string(), reload_index });
    }
    let path = repo.path().join("COMMIT_EDITMSG");
    std::fs::write(&path, message)?;
    let path_arg = path.to_string_lossy().into_owned();
    let prepare_args = [path_arg.as_str(), source];
    gate(
        repo,
        &HookInvocation { name: "prepare-commit-msg", args: &prepare_args, stdin: None, env: &env },
        options,
    )?;
    if !options.no_verify {
        let msg_args = [path_arg.as_str()];
        gate(
            repo,
            &HookInvocation { name: "commit-msg", args: &msg_args, stdin: None, env: &env },
            options,
        )?;
    }
    let edited = String::from_utf8_lossy(&std::fs::read(&path)?).into_owned();
    if edited.trim().is_empty() {
        return Err(CommitError::EmptyMessage);
    }
    Ok(HookedMessage { message: edited, reload_index })
}

/// Failure is ignored: the commit already exists. The reporter (the worker) logs it.
fn run_post_commit(repo: &git2::Repository, options: &mut HookOptions<'_>) {
    let index_path = repo.path().join("index");
    let env: [(&str, &OsStr); 2] = [
        ("GIT_INDEX_FILE", index_path.as_os_str()),
        ("GIT_EDITOR", OsStr::new(":")),
    ];
    let _ = hooks::run_reported(
        repo,
        &HookInvocation { name: "post-commit", args: &[], stdin: None, env: &env },
        options,
    );
}

/// `prepare-commit-msg`'s second argument.
fn message_source(repo: &git2::Repository, is_merging: bool) -> &'static str {
    if is_merging {
        "merge"
    } else if repo.path().join("SQUASH_MSG").exists() {
        "squash"
    } else {
        "message"
    }
}
```

`commit` (new signature; update the doc comment to mention hooks): after the `pick_author` block and before the `let oid = {` block:

```rust
    let source = message_source(repo, is_merging);
    let hooked = run_commit_hooks(repo, message, source, &mut hooks)?;
```

Inside the block, replace `let mut index = repo.index()?;` with:

```rust
        let mut index = repo.index()?;
        if hooked.reload_index {
            index.read(true)?;
        }
```

and pass `&hooked.message` instead of `message` to `create_commit`. After `cleanup_state`, before `Ok(oid.to_string())`:

```rust
    run_post_commit(repo, &mut hooks);
```

`amend_head(repo, message, mut hooks: HookOptions<'_>)`: after the `head` lookup:

```rust
    let hooked = run_commit_hooks(repo, message, "message", &mut hooks)?;
    let mut index = repo.index()?;
    if hooked.reload_index {
        index.read(true)?;
    }
    let tree_id = index.write_tree()?;
```

pass `&hooked.message` to `create_commit`, and call `run_post_commit(repo, &mut hooks);` before returning.

In `repo.rs`: add `pub commit_hooks: Vec<String>,` to `HeadInfo`, compute `let commit_hooks = crate::hooks::installed_hooks(repo, &crate::hooks::COMMIT_HOOKS);` at the top of `head_info`, and set it in both `HeadInfo { … }` literals.

Update every call site the compiler reports: `commit(&mut repo, msg)` → `commit(&mut repo, msg, HookOptions::default())`, `amend_head(&repo, msg)` → `amend_head(&repo, msg, HookOptions::default())` (git-core tests, `repo-service` worker handlers — import `git_core::hooks::HookOptions`), and the `tauri-app` `HeadInfo` literal in its DTO test (`commit_hooks: vec![]`). The tauri/sidecar `HeadInfoDto` gain the field in Task 7.

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core && cargo build --workspace`
Expected: PASS / builds.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/git-core crates/repo-service/src/worker crates/tauri-app/src/commands/mod.rs
git commit -m "feat(git-core): run commit hooks on commit and amend"
```

---

### Task 4: `pre-push` in `push_refs`

**Files:**
- Modify: `crates/git-core/src/remote.rs`
- Modify (call sites, add `false`): `crates/git-core/tests/remote.rs`, `crates/repo-service/src/worker/remote.rs` (temporary `false`; Task 6 threads the real value), any exhaustive `match` on `TransferErrorKind`/`TransferPhase` the compiler reports in `tauri-app`/`vscode-sidecar`
- Test: `crates/git-core/tests/hooks.rs`

**Interfaces:**
- Consumes: Task 2 (`find_hook`, `run_hook`, `failure_output`, `HOOK_TIMEOUT`, `HookOutcome`, `HookError`).
- Produces: `TransferPhase::RunningHook`; `TransferErrorKind::HookFailed`; `RemoteError::HookFailed { hook: String, output: String }`, `RemoteError::Hook(HookError)`; `push_current_branch(repo, remote_name, no_verify, credentials, reporter, cancel)`, `force_push_current_branch(repo, remote_name, no_verify, credentials, reporter, cancel)`, `push_tags(repo, remote_name, names, no_verify, credentials, reporter, cancel)`.

- [ ] **Step 1: Write the failing tests** (append to `tests/hooks.rs`)

```rust
use git_core::remote::{
    push_current_branch, push_tags, CredentialProvider, RemoteError, TransferPhase,
    TransferProgress, TransferReporter,
};

#[derive(Default)]
struct Phases(Vec<TransferPhase>);
impl TransferReporter for Phases {
    fn report(&mut self, event: TransferProgress) {
        self.0.push(event.phase);
    }
}

struct NoCredentials;
impl CredentialProvider for NoCredentials {
    fn credential(&mut self, _: &str, _: Option<&str>, _: git2::CredentialType) -> Result<git2::Cred, git2::Error> {
        Err(git2::Error::from_str("no credentials for a local remote"))
    }
}

struct Pushable {
    local_dir: tempfile::TempDir,
    _remote_dir: tempfile::TempDir,
    remote_path: std::path::PathBuf,
    local: git2::Repository,
}

/// A local repo with one pushed commit on `main` and one more local commit to push.
fn pushable() -> Pushable {
    let (local_dir, local) = common::init_repo();
    local.set_head("refs/heads/main").unwrap();
    common::write_file(local_dir.path(), "a.txt", "a");
    common::commit_all(&local, "first");
    let remote_dir = tempfile::TempDir::new().unwrap();
    git2::Repository::init_bare(remote_dir.path()).unwrap();
    local.remote("origin", remote_dir.path().to_str().unwrap()).unwrap();
    local.find_remote("origin").unwrap().push(&["refs/heads/main:refs/heads/main"], None).unwrap();
    common::write_file(local_dir.path(), "b.txt", "b");
    common::commit_all(&local, "second");
    let remote_path = remote_dir.path().to_path_buf();
    Pushable { local_dir, _remote_dir: remote_dir, remote_path, local }
}

fn remote_main(path: &std::path::Path) -> git2::Oid {
    git2::Repository::open_bare(path).unwrap().refname_to_id("refs/heads/main").unwrap()
}

#[test]
fn failing_pre_push_blocks_the_push() {
    let fixture = pushable();
    let before = remote_main(&fixture.remote_path);
    common::install_hook(&hooks_dir(&fixture.local).unwrap(), "pre-push", "echo 'tests failed' >&2; exit 1");

    let error = push_current_branch(&fixture.local, "origin", false, &mut NoCredentials, &mut Phases::default(), &|| false).unwrap_err();

    match error {
        RemoteError::HookFailed { hook, output } => {
            assert_eq!(hook, "pre-push");
            assert_eq!(output, "tests failed\n");
        }
        other => panic!("expected HookFailed, got {other:?}"),
    }
    assert_eq!(remote_main(&fixture.remote_path), before);
}

#[test]
fn pre_push_receives_args_and_ref_lines() {
    let fixture = pushable();
    let old = remote_main(&fixture.remote_path);
    let new = fixture.local.head().unwrap().target().unwrap();
    let record = fixture.local_dir.path().join("pre-push.txt");
    common::install_hook(
        &hooks_dir(&fixture.local).unwrap(),
        "pre-push",
        &format!(r#"echo "$1 $2" > '{0}'; cat >> '{0}'"#, record.display()),
    );
    let mut phases = Phases::default();

    push_current_branch(&fixture.local, "origin", false, &mut NoCredentials, &mut phases, &|| false).unwrap();

    assert_eq!(
        std::fs::read_to_string(&record).unwrap(),
        format!(
            "origin {}\nrefs/heads/main {new} refs/heads/main {old}\n",
            fixture.remote_path.to_str().unwrap()
        )
    );
    assert!(phases.0.contains(&TransferPhase::RunningHook));
    assert_eq!(remote_main(&fixture.remote_path), new);
}

#[test]
fn up_to_date_refs_are_not_listed() {
    let fixture = pushable();
    push_current_branch(&fixture.local, "origin", false, &mut NoCredentials, &mut Phases::default(), &|| false).unwrap();
    let record = fixture.local_dir.path().join("pre-push.txt");
    common::install_hook(&hooks_dir(&fixture.local).unwrap(), "pre-push", &format!("cat > '{}'", record.display()));

    push_current_branch(&fixture.local, "origin", false, &mut NoCredentials, &mut Phases::default(), &|| false).unwrap();

    assert_eq!(std::fs::read_to_string(&record).unwrap(), "");
}

#[test]
fn no_verify_skips_pre_push() {
    let fixture = pushable();
    common::install_hook(&hooks_dir(&fixture.local).unwrap(), "pre-push", "exit 1");
    let mut phases = Phases::default();

    push_current_branch(&fixture.local, "origin", true, &mut NoCredentials, &mut phases, &|| false).unwrap();

    assert!(!phases.0.contains(&TransferPhase::RunningHook));
    assert_eq!(remote_main(&fixture.remote_path), fixture.local.head().unwrap().target().unwrap());
}

#[test]
fn pushing_tags_runs_pre_push() {
    let fixture = pushable();
    git_core::remote::create_tag(&fixture.local, "v1", None).unwrap();
    common::install_hook(&hooks_dir(&fixture.local).unwrap(), "pre-push", "exit 1");

    let error = push_tags(&fixture.local, "origin", &["v1".to_string()], false, &mut NoCredentials, &mut Phases::default(), &|| false).unwrap_err();

    assert!(matches!(error, RemoteError::HookFailed { .. }));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test hooks`
Expected: compile errors (arity, missing variants).

- [ ] **Step 3: Implement**

`TransferPhase`: add `RunningHook` (doc: "a pre-push hook is running; no transfer progress yet"). `TransferErrorKind`: add `HookFailed`. `RemoteError`:

```rust
    #[error("the {hook} hook failed")]
    HookFailed { hook: String, output: String },
    #[error("{0}")]
    Hook(#[from] crate::hooks::HookError),
```

`transfer_error_kind`: `Self::HookFailed { .. } => TransferErrorKind::HookFailed,`. In `push_refs`'s final `match rejection.get()` add `Some(TransferErrorKind::HookFailed) => unreachable!(),` with the comment "a hook failure aborts negotiation, never a per-ref rejection".

Thread `no_verify: bool` through the three public push functions (after `names` for `push_tags`, after `remote_name` for the others) into `push_refs` (new last parameter). Update their doc comments: "Runs the `pre-push` hook unless `no_verify`."

In `push_refs`, before `callbacks.push_negotiation`:

```rust
    let hook_failure: RefCell<Option<RemoteError>> = RefCell::new(None);
    let push_url = remote
        .pushurl()
        .or_else(|| remote.url())
        .unwrap_or_default()
        .to_string();
    let run_pre_push = !no_verify
        && matches!(
            crate::hooks::find_hook(repo, "pre-push"),
            Ok(crate::hooks::HookLookup::Found(_))
        );
```

At the end of the `push_negotiation` closure, after the lease check and before `Ok(())`:

```rust
        // git runs pre-push at exactly this point: refs advertised, nothing sent yet. A stale
        // lease was rejected above first, as git does.
        if run_pre_push {
            reporter.borrow_mut().report(TransferProgress {
                operation_id: String::new(),
                operation,
                phase: TransferPhase::RunningHook,
                current: 0,
                total: 0,
                received_bytes: 0,
                message: None,
            });
            let input = pre_push_input(updates);
            let args = [remote_name, push_url.as_str()];
            let invocation = crate::hooks::HookInvocation {
                name: "pre-push",
                args: &args,
                stdin: Some(input.as_bytes()),
                env: &[],
            };
            match crate::hooks::run_hook(repo, &invocation, cancel, crate::hooks::HOOK_TIMEOUT) {
                Ok(outcome) => {
                    if let Some(output) =
                        crate::hooks::failure_output(&outcome, crate::hooks::HOOK_TIMEOUT)
                    {
                        *hook_failure.borrow_mut() = Some(RemoteError::HookFailed {
                            hook: "pre-push".to_string(),
                            output,
                        });
                        return Err(git2::Error::from_str("pre-push hook failed"));
                    }
                }
                Err(crate::hooks::HookError::Cancelled { .. }) => {
                    cancelled_at_negotiation.set(true);
                    return Err(git2::Error::from_str("transfer cancelled"));
                }
                Err(error) => {
                    *hook_failure.borrow_mut() = Some(RemoteError::Hook(error));
                    return Err(git2::Error::from_str("pre-push hook could not run"));
                }
            }
        }
        Ok(())
```

In the `if let Err(error) = pushed` block, after the `stale_lease` and `cancelled_at_negotiation` checks:

```rust
        } else if let Some(failure) = hook_failure.borrow_mut().take() {
            Err(failure)
```

New private helper:

```rust
/// git's pre-push stdin: `<local ref> <local sha> <remote ref> <remote sha>` per ref that will
/// change. `src()` is what the remote advertised (zero when the ref is new), `dst()` the new
/// value (zero for a deletion, whose local ref is `(delete)`).
fn pre_push_input(updates: &[git2::PushUpdate<'_>]) -> String {
    let mut input = String::new();
    for update in updates {
        if update.src() == update.dst() {
            continue;
        }
        let local_ref = if update.dst().is_zero() {
            "(delete)"
        } else {
            update.src_refname().unwrap_or("")
        };
        let remote_ref = update.dst_refname().unwrap_or("");
        input.push_str(&format!(
            "{local_ref} {} {remote_ref} {}\n",
            update.dst(),
            update.src()
        ));
    }
    input
}
```

Fix every call site the compiler reports (add `false`), and any exhaustive match on `TransferPhase`/`TransferErrorKind` (the `{:?}` formatting in tauri/sidecar DTOs needs no change).

- [ ] **Step 4: Run tests**

Run: `cargo test -p git-core && cargo build --workspace`
Expected: PASS (existing `tests/remote.rs` unchanged apart from the extra `false`).

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/git-core crates/repo-service crates/tauri-app crates/vscode-sidecar
git commit -m "feat(git-core): run pre-push before sending a push"
```

---

### Task 5: `repo-service` — commit/amend hooks, events and cancel

**Files:**
- Create: `crates/repo-service/src/worker/hooks.rs`
- Modify: `crates/repo-service/src/worker/mod.rs` (`mod hooks; pub use hooks::HookEvent;`, `Command::Commit`/`AmendCommit`, dispatch, tests), `status.rs` (`commit` handler + `WorkerHandle::commit`), `history.rs` (`amend` handler + `WorkerHandle::amend_commit`), `remote.rs` (`TransferOp::cancel_check`/`clear` → `pub(super)`; `pub(super) fn next_operation_id(prefix: &str) -> String` using `NEXT_TRANSFER_ID`), `undo.rs` (`hold_reply` arm, `assert_held!` calls)
- Modify (callers): `crates/tauri-app/src/commands/{status,history}.rs`, `crates/vscode-sidecar/src/dispatch.rs` — temporarily pass `false` and a dropped `mpsc::channel().0` and map the outcome back to the old return (Task 7 does them properly).

**Interfaces:**
- Consumes: Task 3 (`CommitOutcome`, `CommitError`, `HookOptions`), Task 2 (`HookReporter`, `HookOutcome`, `HookError`, `run_hook`, `HookInvocation`, `HOOK_TIMEOUT`).
- Produces: `pub enum HookEvent { Started { operation_id: String, hook: String }, Finished { operation_id: String, hook: String } }`; `WorkerHandle::commit(&self, message: String, no_verify: bool, events: Sender<HookEvent>) -> Result<CommitOutcome, String>`; `WorkerHandle::amend_commit(&self, message: String, no_verify: bool, events: Sender<HookEvent>) -> Result<CommitOutcome, String>`; `pub(super) fn log_hook_result(hook: &str, result: &Result<HookOutcome, HookError>)`; `pub(super) fn run_post_hook(repo: &git2::Repository, name: &str, args: &[&str])`.

- [ ] **Step 1: Write the failing tests** (in `worker/mod.rs`'s `tests` module)

```rust
    #[cfg(unix)]
    fn install_hook(repo: &Repository, name: &str, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        let dir = git_core::hooks::hooks_dir(repo).expect("hooks dir");
        std::fs::create_dir_all(&dir).expect("create hooks dir");
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write hook");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_pre_commit_replies_a_hook_failed_outcome_and_records_no_undo() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        install_hook(&repo, "pre-commit", "echo nope; exit 1");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.stage_file("a.txt".into()).unwrap();
        let undo_before = handle.get_undo_state().unwrap().undo;
        let (events_tx, events_rx) = mpsc::channel();

        let outcome = handle.commit("msg".into(), false, events_tx).unwrap();

        assert_eq!(
            outcome,
            git_core::commit::CommitOutcome::HookFailed { hook: "pre-commit".into(), output: "nope\n".into() }
        );
        let events: Vec<HookEvent> = events_rx.iter().collect();
        assert!(matches!(&events[..], [
            HookEvent::Started { operation_id: a, hook: h1 },
            HookEvent::Finished { operation_id: b, hook: h2 },
        ] if a == b && a.starts_with("commit-") && h1 == "pre-commit" && h2 == "pre-commit"));
        assert_eq!(handle.get_undo_state().unwrap().undo, undo_before);
    }

    #[cfg(unix)]
    #[test]
    fn cancel_transfer_stops_a_running_pre_commit() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        install_hook(&repo, "pre-commit", "sleep 30");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.stage_file("a.txt".into()).unwrap();
        let (events_tx, events_rx) = mpsc::channel();
        let canceller = handle.clone();
        let cancel_thread = std::thread::spawn(move || {
            if let Ok(HookEvent::Started { operation_id, .. }) = events_rx.recv() {
                canceller.cancel_transfer(&operation_id);
            }
        });
        let started = std::time::Instant::now();

        let result = handle.commit("msg".into(), false, events_tx);

        cancel_thread.join().unwrap();
        assert_eq!(result.unwrap_err(), "The pre-commit hook was cancelled.");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    #[cfg(unix)]
    #[test]
    fn amend_with_no_verify_skips_a_failing_pre_commit() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        commit_all(&repo, "first");
        install_hook(&repo, "pre-commit", "exit 1");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();

        let outcome = worker.handle().amend_commit("amended".into(), true, mpsc::channel().0).unwrap();

        assert!(matches!(outcome, git_core::commit::CommitOutcome::Committed { .. }));
    }
```

Update the existing commit tests: `handle.commit("message".into())` → `handle.commit("message".into(), false, mpsc::channel().0)`; assertions on the old `String` id use `CommitOutcome::Committed { commit_id }`. In `undo.rs`'s `every_recordable_command_has_its_reply_held`:

```rust
        assert_held!(Commit { message: s(), no_verify: false, operation_id: s(), events: channel().0 });
        assert_held!(AmendCommit { message: s(), no_verify: false, operation_id: s(), events: channel().0 });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p repo-service`
Expected: compile errors.

- [ ] **Step 3: Implement `worker/hooks.rs`**

```rust
//! Worker side of git hooks: forwards hook progress to the frontend, logs hook problems the UI
//! does not show, and runs the post-checkout/post-merge hooks after successful commands.

use std::sync::mpsc::Sender;

use git_core::hooks::{HookError, HookInvocation, HookOutcome, HookReporter, HOOK_TIMEOUT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookEvent {
    Started { operation_id: String, hook: String },
    Finished { operation_id: String, hook: String },
}

pub(super) struct ChannelHookReporter {
    pub(super) events: Sender<HookEvent>,
    pub(super) operation_id: String,
}

impl HookReporter for ChannelHookReporter {
    fn started(&mut self, hook: &str) {
        let _ = self.events.send(HookEvent::Started {
            operation_id: self.operation_id.clone(),
            hook: hook.to_string(),
        });
    }

    fn finished(&mut self, hook: &str, result: &Result<HookOutcome, HookError>) {
        let _ = self.events.send(HookEvent::Finished {
            operation_id: self.operation_id.clone(),
            hook: hook.to_string(),
        });
        log_hook_result(hook, result);
    }
}

/// Logs what the UI never shows: a failed `post-*` hook, a hook skipped for lacking an execute
/// bit, and (post hooks only) a missing shell. Gate hook failures reach the UI instead.
pub(super) fn log_hook_result(hook: &str, result: &Result<HookOutcome, HookError>) {
    let post = hook.starts_with("post-");
    match result {
        Ok(HookOutcome::Ran(run)) if post && !run.success => {
            eprintln!("browsitory: the {hook} hook failed (ignored):\n{}", run.output)
        }
        Ok(HookOutcome::NotExecutable) => {
            eprintln!("browsitory: ignored the {hook} hook because it is not executable")
        }
        Ok(HookOutcome::NoShell) if post => eprintln!(
            "browsitory: skipped the {hook} hook: no sh.exe next to git.exe or on PATH"
        ),
        Err(error) if post => eprintln!("browsitory: {error}"),
        _ => {}
    }
}

/// Runs a hook whose failure must not affect the command that triggered it.
pub(super) fn run_post_hook(repo: &git2::Repository, name: &str, args: &[&str]) {
    let invocation = HookInvocation { name, args, stdin: None, env: &[] };
    let result = git_core::hooks::run_hook(repo, &invocation, &|| false, HOOK_TIMEOUT);
    log_hook_result(name, &result);
}
```

(`run_post_hook` is used by Task 6; add `#[allow(dead_code)]` until then.)

- [ ] **Step 4: Commands, handlers, handles**

`mod.rs`: `mod hooks;` and `pub use hooks::HookEvent;`. Commands:

```rust
    Commit {
        message: String,
        no_verify: bool,
        operation_id: String,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::commit::CommitOutcome, String>>,
    },
    AmendCommit {
        message: String,
        no_verify: bool,
        operation_id: String,
        events: Sender<HookEvent>,
        reply: Sender<Result<git_core::commit::CommitOutcome, String>>,
    },
```

Dispatch:

```rust
                    Command::Commit { message, no_verify, operation_id, events, reply } => {
                        status::commit(
                            &mut repo,
                            message,
                            no_verify,
                            remote::TransferOp { id: operation_id, cancelled: &cancelled },
                            events,
                            reply,
                        )
                    }
                    Command::AmendCommit { message, no_verify, operation_id, events, reply } => {
                        history::amend(
                            &repo,
                            message,
                            no_verify,
                            remote::TransferOp { id: operation_id, cancelled: &cancelled },
                            events,
                            reply,
                        )
                    }
```

`undo.rs` `hold_reply`: remove `Command::AmendCommit { reply, .. }` from the `()` group and change the commit arm to `Command::Commit { reply, .. } | Command::AmendCommit { reply, .. } => held.hold(reply),`. `plan_for` needs no change (its patterns use `..`).

`status.rs`:

```rust
/// The reply both commit and amend send: a hook failure is an outcome the UI renders, every
/// other failure stays an error message.
pub(super) fn commit_reply(
    result: Result<String, git_core::commit::CommitError>,
) -> Result<git_core::commit::CommitOutcome, String> {
    use git_core::commit::{CommitError, CommitOutcome};
    match result {
        Ok(commit_id) => Ok(CommitOutcome::Committed { commit_id }),
        Err(CommitError::HookFailed { hook, output }) => Ok(CommitOutcome::HookFailed { hook, output }),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn commit(
    repo: &mut git2::Repository,
    message: String,
    no_verify: bool,
    op: super::remote::TransferOp<'_>,
    events: Sender<super::HookEvent>,
    reply: Sender<Result<git_core::commit::CommitOutcome, String>>,
) {
    let cancel = op.cancel_check();
    let mut reporter = super::hooks::ChannelHookReporter { events, operation_id: op.id.clone() };
    let result = git_core::commit::commit(
        repo,
        &message,
        git_core::hooks::HookOptions {
            no_verify,
            cancel: Some(&cancel),
            reporter: Some(&mut reporter),
            ..Default::default()
        },
    );
    op.clear();
    let _ = reply.send(commit_reply(result));
}
```

and the handle:

```rust
    pub fn commit(
        &self,
        message: String,
        no_verify: bool,
        events: Sender<super::HookEvent>,
    ) -> Result<git_core::commit::CommitOutcome, String> {
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::Commit {
                message,
                no_verify,
                operation_id: super::remote::next_operation_id("commit"),
                events,
                reply: tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
```

`history.rs`: `amend` mirrors `commit` (calls `git_core::commit::amend_head(repo, &message, …)`, `status::commit_reply`), and `WorkerHandle::amend_commit(message, no_verify, events)` mirrors the handle above with prefix `"amend"`.

`remote.rs`: make `cancel_check` and `clear` `pub(super)`; add

```rust
/// `"<prefix>-<n>"`, unique per process; also used for commit/amend so their hooks can be
/// cancelled through `CancelRegistry` like a transfer.
pub(super) fn next_operation_id(prefix: &str) -> String {
    format!("{prefix}-{}", NEXT_TRANSFER_ID.fetch_add(1, Ordering::Relaxed))
}
```

Callers in `tauri-app` / `vscode-sidecar`: keep compiling for now, e.g. Tauri `commit`:

```rust
    match worker_handle(&state, &repo_path)?.commit(message, false, mpsc::channel().0)? {
        git_core::commit::CommitOutcome::Committed { commit_id } => Ok(commit_id),
        git_core::commit::CommitOutcome::HookFailed { hook, .. } => Err(format!("The {hook} hook failed.")),
    }
```

(Task 7 replaces these with the real DTOs.)

- [ ] **Step 5: Run tests**

Run: `cargo test -p repo-service && cargo build --workspace`
Expected: PASS.

- [ ] **Step 6: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/repo-service crates/tauri-app crates/vscode-sidecar
git commit -m "feat(repo-service): commit hook outcome, progress events and cancel"
```

---

### Task 6: `repo-service` — push `no_verify`, `hook_output`, post-checkout/post-merge

**Files:**
- Modify: `crates/repo-service/src/worker/mod.rs` (`TransferEvent::Completed.hook_output`, push commands' `no_verify`, tests), `remote.rs` (handlers, handles, `complete`, `pull`), `branch.rs` (`switch`), `history.rs` (`checkout`), `merge.rs` (`start`), `hooks.rs` (drop the `dead_code` allow)
- Modify (callers, temporary `false` / `hook_output` passthrough): `crates/tauri-app/src/commands/{mod,remote}.rs`, `crates/vscode-sidecar/src/dispatch.rs`

**Interfaces:**
- Consumes: Task 4 (`no_verify` on git-core push functions, `RemoteError::HookFailed`), Task 5 (`run_post_hook`).
- Produces: `TransferEvent::Completed { operation_id, operation, error, hook_output: Option<String> }`; `WorkerHandle::push_current_branch(remote_name, no_verify, events)`, `force_push_current_branch(remote_name, no_verify, events)`, `push_tags(remote_name, names, no_verify, events)`; `Command::{PushCurrentBranch, ForcePushCurrentBranch, PushTags}` gain `no_verify: bool`.

- [ ] **Step 1: Write the failing tests** (`worker/mod.rs` tests)

```rust
    #[cfg(unix)]
    #[test]
    fn a_failing_pre_push_completes_with_hook_failed_and_its_output() {
        let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
        let local = Repository::open(local_dir.path()).unwrap();
        write_file(local_dir.path(), "new.txt", "new");
        commit_all(&local, "to push");
        install_hook(&local, "pre-push", "echo 'tests failed'; exit 1");
        drop(local);
        let worker = Worker::spawn(local_dir.path().to_path_buf()).unwrap();
        let (event_tx, event_rx) = mpsc::channel();

        worker.handle().push_current_branch("origin".into(), false, event_tx).unwrap();
        let events: Vec<_> = event_rx.iter().collect();

        assert!(matches!(
            events.last(),
            Some(TransferEvent::Completed {
                error: Some(TransferErrorKind::HookFailed),
                hook_output: Some(output),
                ..
            }) if output == "tests failed\n"
        ));
    }

    #[cfg(unix)]
    #[test]
    fn post_checkout_gets_old_and_new_head_after_switch_and_checkout_commit() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        commit_all(&repo, "first");
        let first = repo.head().unwrap().target().unwrap();
        let main = repo.head().unwrap().shorthand().unwrap().to_string();
        repo.branch("other", &repo.find_commit(first).unwrap(), false).unwrap();
        let record = dir.path().join("post-checkout.txt");
        install_hook(&repo, "post-checkout", &format!(r#"echo "$@" >> '{}'; exit 1"#, record.display()));
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        handle.switch_branch("other".into()).unwrap();
        handle.checkout_commit(first.to_string()).unwrap();
        handle.switch_branch(main).unwrap();

        assert_eq!(
            std::fs::read_to_string(&record).unwrap(),
            format!("{first} {first} 1\n{first} {first} 1\n{first} {first} 1\n")
        );
    }

    #[cfg(unix)]
    #[test]
    fn post_merge_runs_after_a_merge() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "a");
        commit_all(&repo, "first");
        let main = repo.head().unwrap().shorthand().unwrap().to_string();
        let first = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &first, false).unwrap();
        repo.set_head("refs/heads/feature").unwrap();
        write_file(dir.path(), "b.txt", "b");
        commit_all(&repo, "feature work");
        repo.set_head(&format!("refs/heads/{main}")).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
        let record = dir.path().join("post-merge.txt");
        install_hook(&repo, "post-merge", &format!(r#"echo "$@" > '{}'"#, record.display()));
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();

        worker.handle().start_merge("feature".into()).unwrap();

        assert_eq!(std::fs::read_to_string(&record).unwrap(), "0\n");
    }
```

Update the existing push tests to pass `false` and add `hook_output: None` (or `..`) to `TransferEvent::Completed` patterns and constructions.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p repo-service`
Expected: compile errors.

- [ ] **Step 3: Implement**

`TransferEvent::Completed` gains `hook_output: Option<String>` (doc: "the failed pre-push hook's output; `None` for every other outcome"). `complete` in `worker/remote.rs`:

```rust
fn complete(
    events: &Sender<TransferEvent>,
    op: TransferOp<'_>,
    operation: TransferOperation,
    result: Result<(), git_core::remote::RemoteError>,
) {
    op.clear();
    let hook_output = match &result {
        Err(git_core::remote::RemoteError::HookFailed { output, .. }) => Some(output.clone()),
        _ => None,
    };
    let _ = events.send(TransferEvent::Completed {
        operation_id: op.id,
        operation,
        error: result.err().map(|error| error.transfer_error_kind()),
        hook_output,
    });
}
```

`pull`'s own `Completed` gets `hook_output: None`. Thread `no_verify` from the three push `Command`s through `remote::push_branch` / `force_push_branch` / `push_tags` handlers into the git-core calls, and add it to the three `WorkerHandle` methods (after `remote_name` / `names`).

Post hooks (each only after success):

```rust
// branch.rs
pub(super) fn switch(repo: &git2::Repository, name: String, reply: Sender<Result<(), String>>) {
    let before = super::hooks::head_oid(repo);
    let result = git_core::branch::switch_branch(repo, &name).map_err(|error| error.to_string());
    if result.is_ok() {
        super::hooks::run_post_checkout(repo, before);
    }
    let _ = reply.send(result);
}
```

`history::checkout` is the same around `git_core::branch::checkout_commit`. In `worker/hooks.rs`:

```rust
/// HEAD's commit, or the zero id while unborn (what git passes to post-checkout).
pub(super) fn head_oid(repo: &git2::Repository) -> git2::Oid {
    repo.head()
        .ok()
        .and_then(|head| head.target())
        .unwrap_or(git2::Oid::ZERO_SHA1)
}

/// `post-checkout <old HEAD> <new HEAD> 1` (1 = a branch/HEAD checkout, not a file checkout).
pub(super) fn run_post_checkout(repo: &git2::Repository, before: git2::Oid) {
    let old = before.to_string();
    let new = head_oid(repo).to_string();
    run_post_hook(repo, "post-checkout", &[&old, &new, "1"]);
}
```

`merge::start`:

```rust
    let result = git_core::merge::start_merge(repo, &branch_name).map_err(|error| error.to_string());
    if matches!(result, Ok(MergeOutcome::FastForwarded) | Ok(MergeOutcome::Merged)) {
        super::hooks::run_post_hook(repo, "post-merge", &["0"]);
    }
    let _ = reply.send(result);
```

`remote::pull`: after the closure computes `result`, before replying:

```rust
    if matches!(result, Ok(PullOutcome::FastForwarded { .. })) {
        super::hooks::run_post_hook(repo, "post-merge", &["0"]);
    }
```

Callers in `tauri-app`/`vscode-sidecar`: pass `false` for `no_verify` and add `hook_output` to their `Completed` destructuring (ignored until Task 7).

- [ ] **Step 4: Run tests**

Run: `cargo test -p repo-service && cargo build --workspace`
Expected: PASS.

- [ ] **Step 5: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/repo-service crates/tauri-app crates/vscode-sidecar
git commit -m "feat(repo-service): pre-push skip, hook output on push failure, post-checkout and post-merge"
```

---

### Task 7: Tauri commands and sidecar methods

**Files:**
- Modify: `crates/tauri-app/src/commands/mod.rs` (DTOs, `emit_hook_events`, `TransferProgressDto.hook_output`, `HeadInfoDto.commit_hooks`, tests), `status.rs` (`commit`), `history.rs` (`amend_commit`), `remote.rs` (push commands)
- Modify: `crates/vscode-sidecar/src/dispatch.rs`, `crates/vscode-sidecar/tests/protocol_roundtrip.rs`

**Interfaces:**
- Consumes: Tasks 5–6.
- Produces (wire, used by Task 8): commands/methods `commit` and `amend_commit` with `{ repoPath, message, noVerify }` → `{ kind: "Committed", commitId } | { kind: "HookFailed", hook, output }`; `push_current_branch` / `force_push_current_branch` `{ repoPath, remoteName, noVerify }`; `push_tags` `{ repoPath, remoteName, names, noVerify }`; event `hook-progress` / notification `hookProgress` `{ repoPath, operationId, hook, phase }`; transfer payloads gain `hookOutput`; `get_head_info` gains `commitHooks`.

- [ ] **Step 1: Write the failing DTO tests** (tauri `commands/mod.rs` tests)

```rust
    #[test]
    fn commit_outcome_dto_is_kind_tagged() {
        assert_eq!(
            serde_json::to_value(CommitOutcomeDto::from(git_core::commit::CommitOutcome::Committed {
                commit_id: "c1".into()
            }))
            .unwrap(),
            serde_json::json!({"kind": "Committed", "commitId": "c1"})
        );
        assert_eq!(
            serde_json::to_value(CommitOutcomeDto::from(git_core::commit::CommitOutcome::HookFailed {
                hook: "pre-commit".into(),
                output: "no\n".into()
            }))
            .unwrap(),
            serde_json::json!({"kind": "HookFailed", "hook": "pre-commit", "output": "no\n"})
        );
    }

    #[test]
    fn hook_progress_dto_carries_the_repo_and_phase() {
        let dto = hook_progress_dto(
            "/repo",
            repo_service::worker::HookEvent::Started { operation_id: "commit-1".into(), hook: "pre-commit".into() },
        );
        assert_eq!(
            serde_json::to_value(dto).unwrap(),
            serde_json::json!({"repoPath": "/repo", "operationId": "commit-1", "hook": "pre-commit", "phase": "Started"})
        );
    }

    #[test]
    fn a_hook_failed_transfer_carries_its_output_and_others_do_not() {
        let (_, failed) = transfer_event_payload(TransferEvent::Completed {
            operation_id: "push-1".into(),
            operation: TransferOperation::PushBranch,
            error: Some(TransferErrorKind::HookFailed),
            hook_output: Some("tests failed\n".into()),
        });
        let value = serde_json::to_value(failed).unwrap();
        assert_eq!(value["errorKind"], "HookFailed");
        assert_eq!(value["hookOutput"], "tests failed\n");
        assert_eq!(value["message"], serde_json::Value::Null);
        let (_, started) = transfer_event_payload(TransferEvent::Started {
            operation_id: "push-1".into(),
            operation: TransferOperation::PushBranch,
        });
        assert_eq!(serde_json::to_value(started).unwrap()["hookOutput"], serde_json::Value::Null);
    }
```

Extend the existing `HeadInfoDto` test to set `commit_hooks: vec!["pre-commit".into()]` and assert `"commitHooks": ["pre-commit"]`. Add `RunningHook` to whatever existing test enumerates phases, if any.

Sidecar (`tests/protocol_roundtrip.rs`): change the existing commit assertion to

```rust
    assert_eq!(committed["result"]["kind"], "Committed");
    let commit_id = committed["result"]["commitId"].as_str().expect("commit id").to_string();
```

and add (Unix):

```rust
#[cfg(unix)]
#[test]
fn a_failing_pre_commit_streams_hook_progress_and_returns_hook_failed() {
    use std::os::unix::fs::PermissionsExt;
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, repo) = init_repo();
    write_file(dir.path(), "new.txt", "hello");
    let hooks = repo.path().join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    std::fs::write(hooks.join("pre-commit"), "#!/bin/sh\necho blocked\nexit 1\n").unwrap();
    std::fs::set_permissions(hooks.join("pre-commit"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));
    sidecar.call(2, "stage_file", serde_json::json!({"repoPath": repo_path, "path": "new.txt"}));

    let (response, notifications) = sidecar.call_and_collect_notifications(
        3,
        "commit",
        serde_json::json!({"repoPath": repo_path, "message": "m", "noVerify": false}),
        false,
    );

    assert_eq!(
        response["result"],
        serde_json::json!({"kind": "HookFailed", "hook": "pre-commit", "output": "blocked\n"})
    );
    assert!(notifications.iter().any(|n| n["method"] == "hookProgress"
        && n["params"]["phase"] == "Started"
        && n["params"]["hook"] == "pre-commit"
        && n["params"]["repoPath"] == repo_path.as_str()));
}
```

(If the `Finished` notification arrives after the response it is simply left unread; the sidecar is dropped at the end of the test. `init_repo` in the sidecar tests does not pin `core.hooksPath`; if a global one is set, the test writes to the wrong place — guard with `git2::Config::open_default().and_then(|c| c.get_path("core.hooksPath")).is_ok()` → skip with `eprintln!`.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tauri-app && cargo test -p vscode-sidecar`
Expected: compile errors / FAIL.

- [ ] **Step 3: Implement Tauri**

`commands/mod.rs`:

```rust
#[derive(Serialize)]
#[serde(tag = "kind")]
pub enum CommitOutcomeDto {
    Committed {
        #[serde(rename = "commitId")]
        commit_id: String,
    },
    HookFailed {
        hook: String,
        output: String,
    },
}

impl From<git_core::commit::CommitOutcome> for CommitOutcomeDto {
    fn from(outcome: git_core::commit::CommitOutcome) -> Self {
        match outcome {
            git_core::commit::CommitOutcome::Committed { commit_id } => Self::Committed { commit_id },
            git_core::commit::CommitOutcome::HookFailed { hook, output } => Self::HookFailed { hook, output },
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookProgressDto {
    pub repo_path: String,
    pub operation_id: String,
    pub hook: String,
    pub phase: &'static str,
}

fn hook_progress_dto(repo_path: &str, event: repo_service::worker::HookEvent) -> HookProgressDto {
    use repo_service::worker::HookEvent;
    let (operation_id, hook, phase) = match event {
        HookEvent::Started { operation_id, hook } => (operation_id, hook, "Started"),
        HookEvent::Finished { operation_id, hook } => (operation_id, hook, "Finished"),
    };
    HookProgressDto { repo_path: repo_path.to_string(), operation_id, hook, phase }
}

/// Like `emit_transfer_events`: drains on its own thread until the worker drops its sender.
fn emit_hook_events(
    app: AppHandle,
    repo_path: String,
    events: mpsc::Receiver<repo_service::worker::HookEvent>,
) {
    thread::spawn(move || {
        for event in events {
            let _ = app.emit("hook-progress", hook_progress_dto(&repo_path, event));
        }
    });
}
```

`TransferProgressDto` gains `pub hook_output: Option<String>` (doc: "local pre-push hook output; unlike `message` this comes from the user's own script, not the remote"); `None` in `From<TransferProgress>` and the `Started` arm; the `Completed` arm sets it from the event. `HeadInfoDto` gains `pub commit_hooks: Vec<String>` (filled from `info.commit_hooks`).

`status.rs`:

```rust
#[tauri::command]
pub async fn commit(
    repo_path: String,
    message: String,
    no_verify: bool,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CommitOutcomeDto, String> {
    let (event_tx, event_rx) = mpsc::channel();
    emit_hook_events(app, repo_path.clone(), event_rx);
    Ok(CommitOutcomeDto::from(
        worker_handle(&state, &repo_path)?.commit(message, no_verify, event_tx)?,
    ))
}
```

`history.rs` `amend_commit` is the same shape calling `amend_commit`. `remote.rs`: `push_current_branch`, `force_push_current_branch`, `push_tags` take `no_verify: bool` and pass it on. No new command names, so `main.rs`'s handler list is unchanged.

- [ ] **Step 4: Implement the sidecar**

In `dispatch.rs`: its own `CommitOutcomeDto` and `HookProgressDto` (copies, by convention); `TransferProgressDto.hook_output` (filled exactly like Tauri, `None` for `message`); `HeadInfoDto.commit_hooks`. Params:

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitMessageParams {
    repo_path: String,
    message: String,
    #[serde(default)]
    no_verify: bool,
}
```

used by `commit` and `amend_commit` (replace `CommitParams`; keep `MessageParams` if anything else uses it). `PushParams { repo_path, remote_name, #[serde(default)] no_verify }` for `push_current_branch`/`force_push_current_branch`; `PushTagsParams` gains `#[serde(default)] no_verify: bool`. `"commit" => commit(params, repos, stdout)`, `"amend_commit" => amend_commit(params, repos, stdout)`. Relay:

```rust
/// Writes each hook event as a `hookProgress` notification; same framing as
/// `spawn_progress_relay`.
fn spawn_hook_relay<W: std::io::Write + Send + 'static>(
    repo_path: String,
    event_rx: mpsc::Receiver<HookEvent>,
    out: Arc<Mutex<W>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for event in event_rx {
            let notification = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "hookProgress",
                "params": hook_progress_dto(&repo_path, event),
            });
            let Ok(line) = serde_json::to_string(&notification) else { continue };
            let mut out = out.lock().unwrap_or_else(|error| error.into_inner());
            if writeln!(out, "{line}").and_then(|()| out.flush()).is_err() {
                return;
            }
        }
    })
}

fn commit(params: Value, repos: &Repos, stdout: &Arc<Mutex<std::io::Stdout>>) -> Result<Value, String> {
    let params: CommitMessageParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_hook_relay(params.repo_path.clone(), event_rx, Arc::clone(stdout));
    let outcome = worker_handle(repos, &params.repo_path)?.commit(params.message, params.no_verify, event_tx)?;
    serde_json::to_value(CommitOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}
```

`amend_commit` mirrors it.

- [ ] **Step 5: Run tests**

Run: `cargo test -p tauri-app && cargo test -p vscode-sidecar && cargo test --workspace`
Expected: PASS.

- [ ] **Step 6: Lint and commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings && cargo clippy --workspace --all-targets --features tauri-app/forge-fixture-override -- -D warnings && cargo fmt --all -- --check`

```bash
git add crates/tauri-app crates/vscode-sidecar
git commit -m "feat: hook outcomes, progress events and noVerify over Tauri and the sidecar"
```

---

### Task 8: `RepoClient` and both clients

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts`, `tauriRepoClient.ts`, `vscodeRepoClient.ts`, `tauriRepoClient.test.ts`, `vscodeRepoClient.test.ts`
- Modify: every full `RepoClient` fake (find them with `cd frontend && pnpm exec tsc --noEmit`; today: `App.test.tsx`, `state/useAppState.test.ts`, `components/{DiffPane,RepoPicker,RebasePlanner,ConflictResolutionPane,CloneDialog}.test.tsx`) — add `subscribeHookProgress: () => () => {}`, `commitHooks: []` to `HeadInfo` literals, `hookOutput: null` to `TransferProgress` literals, and make `commit`/`amendCommit` fakes resolve `{ kind: "Committed", commitId: "c" }`.

**Interfaces:**
- Produces (used by Tasks 9–10):

```ts
export type CommitOutcome =
  | { kind: "Committed"; commitId: string }
  | { kind: "HookFailed"; hook: string; output: string };

export interface HookProgress {
  repoPath: string;
  operationId: string;
  hook: string;
  phase: "Started" | "Finished";
}

// TransferErrorKind gains "HookFailed"; TransferProgress.phase gains "RunningHook";
// TransferProgress gains `hookOutput: string | null`; HeadInfo gains `commitHooks: string[]`.
commit(repoPath: string, message: string, noVerify: boolean): Promise<CommitOutcome>;
amendCommit(repoPath: string, message: string, noVerify: boolean): Promise<CommitOutcome>;
pushCurrentBranch(repoPath: string, remoteName: string, noVerify: boolean): Promise<string>;
forcePushCurrentBranch(repoPath: string, remoteName: string, noVerify: boolean): Promise<string>;
pushTags(repoPath: string, remoteName: string, names: string[], noVerify: boolean): Promise<string>;
subscribeHookProgress(listener: (event: HookProgress) => void): () => void;
```

- [ ] **Step 1: Write the failing client tests**

`tauriRepoClient.test.ts`:

```ts
  it("commits with noVerify and returns the outcome", async () => {
    vi.mocked(invoke).mockResolvedValue({ kind: "HookFailed", hook: "pre-commit", output: "no\n" });

    await expect(tauriRepoClient.commit(TEST_REPO_PATH, "msg", true)).resolves.toEqual({
      kind: "HookFailed",
      hook: "pre-commit",
      output: "no\n",
    });
    expect(invoke).toHaveBeenCalledWith("commit", { repoPath: TEST_REPO_PATH, message: "msg", noVerify: true });
  });

  it("passes noVerify to pushes", async () => {
    vi.mocked(listen).mockResolvedValue(() => {});
    vi.mocked(invoke).mockResolvedValue("push-1");
    await tauriRepoClient.pushTags(TEST_REPO_PATH, "origin", ["v1"], true);
    expect(invoke).toHaveBeenCalledWith("push_tags", { repoPath: TEST_REPO_PATH, remoteName: "origin", names: ["v1"], noVerify: true });
  });

  it("forwards hook-progress events and unregisters", async () => {
    const unlisten = vi.fn();
    let hookListener: ((event: { payload: unknown }) => void) | undefined;
    vi.mocked(listen).mockImplementation(async (event, listener) => {
      if (event === "hook-progress") hookListener = listener as typeof hookListener;
      return unlisten;
    });
    const received = vi.fn();

    const unsubscribe = tauriRepoClient.subscribeHookProgress(received);
    await vi.waitFor(() => expect(hookListener).toBeDefined());
    const payload = { repoPath: "/r", operationId: "commit-1", hook: "pre-commit", phase: "Started" };
    hookListener?.({ payload });
    expect(received).toHaveBeenCalledWith(payload);

    unsubscribe();
    expect(unlisten).toHaveBeenCalled();
  });
```

`vscodeRepoClient.test.ts` (using the file's existing `notify`/`respond`/posted-message helpers):

```ts
  it("delivers hookProgress notifications to subscribed listeners", () => {
    const received: unknown[] = [];
    const unsubscribe = vscodeRepoClient.subscribeHookProgress((event) => received.push(event));
    const event = { repoPath: "/r", operationId: "commit-1", hook: "pre-commit", phase: "Started" };

    notify("hookProgress", event);
    expect(received).toEqual([event]);

    unsubscribe();
    notify("hookProgress", event);
    expect(received).toHaveLength(1);
  });

  it("sends noVerify with commit", async () => {
    const promise = vscodeRepoClient.commit("/repo", "m", true);
    expect(lastPosted()).toMatchObject({ method: "commit", params: { repoPath: "/repo", message: "m", noVerify: true } });
    respond(1, { kind: "Committed", commitId: "c1" });
    await expect(promise).resolves.toEqual({ kind: "Committed", commitId: "c1" });
  });
```

(Use the file's actual helper names for "last posted message" and request ids; the existing `pull_current_upstream` test shows the pattern.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run src/ipc`
Expected: FAIL (type errors / missing methods).

- [ ] **Step 3: Implement**

`RepoClient.ts`: the types above (doc comment on `commit`: "A failing gate hook resolves `HookFailed`; only real failures reject."), and update `cancelTransfer`'s doc comment: "…the in-flight transfer **or hook run** `operationId` (commit/amend ids arrive on `subscribeHookProgress`)…".

`tauriRepoClient.ts`:

```ts
let hookListenersReady: Promise<void> = Promise.resolve();
```

```ts
  commit: async (repoPath: string, message: string, noVerify: boolean) => {
    await hookListenersReady;
    return loggedInvoke<CommitOutcome>("commit", { repoPath, message, noVerify });
  },
  amendCommit: async (repoPath: string, message: string, noVerify: boolean) => {
    await hookListenersReady;
    return loggedInvoke<CommitOutcome>("amend_commit", { repoPath, message, noVerify });
  },
  subscribeHookProgress: (listener: (event: HookProgress) => void) => {
    let disposed = false;
    let stop: (() => void) | null = null;
    const registration = listen<HookProgress>("hook-progress", ({ payload }) => listener(payload));
    hookListenersReady = registration.then((unlisten) => {
      if (disposed) unlisten();
      else stop = unlisten;
    });
    void hookListenersReady.catch(() => {});
    return () => {
      if (disposed) return;
      disposed = true;
      stop?.();
    };
  },
```

Push methods add `noVerify` to their args objects. `vscodeRepoClient.ts`: `const hookProgressListeners = new Set<(event: HookProgress) => void>();`, a `hookProgress` branch in `handleMessage` next to `transferProgress`, clear it where `transferProgressListeners.clear()` runs, `subscribeHookProgress` like `subscribeTransferProgress`, and `noVerify` in the five methods' params.

Fix the fakes listed under **Files**.

- [ ] **Step 4: Run tests**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm test -- --run && pnpm lint`
Expected: PASS (state/UI still call the old shapes; `tsc` points at every call site, which Task 9 rewrites — if `tsc` fails only in `state/`/`App.tsx`, pass `false` / ignore the outcome there temporarily so this task stays green).

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): RepoClient commit outcomes, noVerify and hook progress"
```

---

### Task 9: State — hook failure, retry, skip hooks, running hook

**Files:**
- Create: `frontend/src/lib/hookFailure.ts`, `frontend/src/lib/hookFailure.test.ts`, `frontend/src/state/useHookActions.ts`
- Modify: `frontend/src/state/useAppState.ts`, `useStagingActions.ts`, `useHistoryActions.ts`, `useRemoteTransferActions.ts`, `useAppState.test.ts`

**Interfaces:**
- Consumes: Task 8.
- Produces (used by Task 10):

```ts
// lib/hookFailure.ts
export type PushRequest =
  | { operation: "PushBranch"; remoteName: string }
  | { operation: "ForcePushBranch"; remoteName: string }
  | { operation: "PushTags"; remoteName: string; names: string[] };
export type HookRetry =
  | { kind: "commit"; message: string }
  | { kind: "amend"; message: string }
  | { kind: "push"; push: PushRequest };
export interface HookFailure { hook: string; output: string; retry: HookRetry; dialogOpen: boolean }
export const SKIPPABLE_HOOKS: ReadonlySet<string>;
export function hookFailedMessage(hook: string): string;      // "The <hook> hook failed."
export function runningHookLabel(hook: string): string;       // "Running <hook> hook…"
export function retryLabel(failure: HookFailure): string | null; // null when not skippable
export function retrySuccessLabel(retry: HookRetry): string;  // toast after a successful retry

// AppState additions
skipHooks: boolean;
hookRun: { operationId: string; hook: string } | null;
hookFailure: HookFailure | null;
clearCommitMessage: boolean;

// UseAppStateResult changes/additions
commit(message: string, noVerify: boolean): Promise<boolean>;
amendCommit(message: string, noVerify: boolean): Promise<boolean>;
pushCurrentBranch(remoteName: string, noVerify?: boolean): Promise<void>;
forcePushCurrentBranch(remoteName: string, noVerify?: boolean): Promise<void>;
pushTags(remoteName: string, names: string[], noVerify?: boolean): Promise<void>;
setSkipHooks(on: boolean): void;
cancelHookRun(): Promise<void>;
openHookFailure(): void;
closeHookFailure(): void;
clearCommitMessageHandled(): void;
retryWithoutHooks(retry: HookRetry): Promise<void>;
```

- [ ] **Step 1: `lib/hookFailure.ts` with its test**

```ts
export type PushRequest =
  | { operation: "PushBranch"; remoteName: string }
  | { operation: "ForcePushBranch"; remoteName: string }
  | { operation: "PushTags"; remoteName: string; names: string[] };

export type HookRetry =
  | { kind: "commit"; message: string }
  | { kind: "amend"; message: string }
  | { kind: "push"; push: PushRequest };

export interface HookFailure {
  hook: string;
  output: string;
  retry: HookRetry;
  dialogOpen: boolean;
}

/** What `--no-verify` skips; mirrors `git_core::hooks::SKIPPABLE_HOOKS`. */
export const SKIPPABLE_HOOKS: ReadonlySet<string> = new Set(["pre-commit", "commit-msg", "pre-push"]);

export function hookFailedMessage(hook: string): string {
  return `The ${hook} hook failed.`;
}

export function runningHookLabel(hook: string): string {
  return `Running ${hook} hook…`;
}

export function retryLabel(failure: HookFailure): string | null {
  if (!SKIPPABLE_HOOKS.has(failure.hook)) return null;
  if (failure.retry.kind === "commit") return "Commit without hooks";
  if (failure.retry.kind === "amend") return "Amend without hooks";
  return "Push without hooks";
}

export function retrySuccessLabel(retry: HookRetry): string {
  if (retry.kind === "commit") return "Committed";
  if (retry.kind === "amend") return "Amended commit";
  if (retry.push.operation === "ForcePushBranch") return "Force pushed";
  if (retry.push.operation === "PushTags") return "Tags pushed";
  return "Push complete";
}
```

```ts
import { describe, expect, it } from "vitest";
import { hookFailedMessage, retryLabel, retrySuccessLabel, runningHookLabel } from "./hookFailure";

describe("hookFailure", () => {
  it("builds the copy", () => {
    expect(hookFailedMessage("pre-commit")).toBe("The pre-commit hook failed.");
    expect(runningHookLabel("pre-push")).toBe("Running pre-push hook…");
  });

  it("offers a retry only for hooks --no-verify skips", () => {
    const base = { output: "", dialogOpen: true };
    expect(retryLabel({ ...base, hook: "pre-commit", retry: { kind: "commit", message: "m" } })).toBe("Commit without hooks");
    expect(retryLabel({ ...base, hook: "commit-msg", retry: { kind: "amend", message: "m" } })).toBe("Amend without hooks");
    expect(retryLabel({ ...base, hook: "pre-push", retry: { kind: "push", push: { operation: "PushBranch", remoteName: "o" } } })).toBe("Push without hooks");
    expect(retryLabel({ ...base, hook: "prepare-commit-msg", retry: { kind: "commit", message: "m" } })).toBeNull();
  });

  it("names the success toast after the original action", () => {
    expect(retrySuccessLabel({ kind: "push", push: { operation: "ForcePushBranch", remoteName: "o" } })).toBe("Force pushed");
    expect(retrySuccessLabel({ kind: "push", push: { operation: "PushTags", remoteName: "o", names: [] } })).toBe("Tags pushed");
  });
});
```

Run: `cd frontend && pnpm test -- --run src/lib/hookFailure.test.ts` → PASS. Commit:

```bash
git add frontend/src/lib/hookFailure.ts frontend/src/lib/hookFailure.test.ts
git commit -m "feat(frontend): hook failure retry types and copy"
```

- [ ] **Step 2: Write the failing state tests** (`useAppState.test.ts`, using its existing fake-client builder; shown with a local `makeClient(overrides)` — use whatever the file already provides)

```ts
describe("git hooks", () => {
  it("stores a commit hook failure with its retry and reports the error", async () => {
    const commit = vi.fn(async () => ({ kind: "HookFailed" as const, hook: "pre-commit", output: "lint\n" }));
    const { result } = renderHook(() => useAppState(makeClient({ commit }), TEST_REPO_PATH));

    let ok = true;
    await act(async () => {
      ok = await result.current.commit("msg", false);
    });

    expect(ok).toBe(false);
    expect(commit).toHaveBeenCalledWith(TEST_REPO_PATH, "msg", false);
    expect(result.current.state.error).toBe("The pre-commit hook failed.");
    expect(result.current.state.hookFailure).toEqual({
      hook: "pre-commit",
      output: "lint\n",
      retry: { kind: "commit", message: "msg" },
      dialogOpen: true,
    });
  });

  it("retries a commit without hooks and asks the CommitBox to clear", async () => {
    const commit = vi
      .fn()
      .mockResolvedValueOnce({ kind: "HookFailed", hook: "pre-commit", output: "" })
      .mockResolvedValueOnce({ kind: "Committed", commitId: "c1" });
    const { result } = renderHook(() => useAppState(makeClient({ commit }), TEST_REPO_PATH));
    await act(async () => void (await result.current.commit("msg", false)));

    await act(async () => {
      await result.current.retryWithoutHooks({ kind: "commit", message: "msg" });
    });

    expect(commit).toHaveBeenLastCalledWith(TEST_REPO_PATH, "msg", true);
    expect(result.current.state.hookFailure).toBeNull();
    expect(result.current.state.error).toBeNull();
    expect(result.current.state.clearCommitMessage).toBe(true);
    act(() => result.current.clearCommitMessageHandled());
    expect(result.current.state.clearCommitMessage).toBe(false);
  });

  it("skip hooks applies to the next commit only", async () => {
    const commit = vi.fn(async () => ({ kind: "Committed" as const, commitId: "c" }));
    const { result } = renderHook(() => useAppState(makeClient({ commit }), TEST_REPO_PATH));
    act(() => result.current.setSkipHooks(true));

    await act(async () => void (await result.current.commit("m", result.current.state.skipHooks)));

    expect(commit).toHaveBeenCalledWith(TEST_REPO_PATH, "m", true);
    expect(result.current.state.skipHooks).toBe(false);
  });

  it("tracks the running hook for this repo only and clears it when the commit settles", async () => {
    let emit: ((event: HookProgress) => void) | undefined;
    let finish: ((outcome: CommitOutcome) => void) | undefined;
    const client = makeClient({
      subscribeHookProgress: (listener: (event: HookProgress) => void) => {
        emit = listener;
        return () => {};
      },
      commit: () => new Promise<CommitOutcome>((resolve) => (finish = resolve)),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let pending: Promise<boolean> | undefined;
    act(() => {
      pending = result.current.commit("m", false);
    });
    act(() => emit?.({ repoPath: "/other", operationId: "commit-9", hook: "pre-commit", phase: "Started" }));
    expect(result.current.state.hookRun).toBeNull();
    act(() => emit?.({ repoPath: TEST_REPO_PATH, operationId: "commit-1", hook: "pre-commit", phase: "Started" }));
    expect(result.current.state.hookRun).toEqual({ operationId: "commit-1", hook: "pre-commit" });

    await act(async () => {
      finish?.({ kind: "Committed", commitId: "c" });
      await pending;
    });
    expect(result.current.state.hookRun).toBeNull();
  });

  it("cancels the running hook through cancelTransfer", async () => {
    const cancelTransfer = vi.fn(async () => {});
    let emit: ((event: HookProgress) => void) | undefined;
    const client = makeClient({
      cancelTransfer,
      subscribeHookProgress: (listener: (event: HookProgress) => void) => {
        emit = listener;
        return () => {};
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    act(() => emit?.({ repoPath: TEST_REPO_PATH, operationId: "commit-1", hook: "pre-commit", phase: "Started" }));

    await act(async () => result.current.cancelHookRun());

    expect(cancelTransfer).toHaveBeenCalledWith(TEST_REPO_PATH, "commit-1");
  });

  it("turns a HookFailed push into a retryable hook failure", async () => {
    let listener: ((progress: TransferProgress) => void) | undefined;
    const pushCurrentBranch = vi.fn(async () => "push-1");
    const client = makeClient({
      pushCurrentBranch,
      subscribeTransferProgress: (next: (progress: TransferProgress) => void) => {
        listener = next;
        return () => {};
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(async () => result.current.pushCurrentBranch("origin"));
    expect(pushCurrentBranch).toHaveBeenCalledWith(TEST_REPO_PATH, "origin", false);

    act(() =>
      listener?.({
        operationId: "push-1",
        operation: "PushBranch",
        phase: "Failed",
        errorKind: "HookFailed",
        current: 0,
        total: 0,
        receivedBytes: 0,
        message: null,
        hookOutput: "tests failed\n",
      }),
    );

    expect(result.current.state.error).toBe("The pre-push hook failed.");
    expect(result.current.state.hookFailure).toEqual({
      hook: "pre-push",
      output: "tests failed\n",
      retry: { kind: "push", push: { operation: "PushBranch", remoteName: "origin" } },
      dialogOpen: true,
    });

    await act(async () => result.current.retryWithoutHooks(result.current.state.hookFailure!.retry));
    expect(pushCurrentBranch).toHaveBeenLastCalledWith(TEST_REPO_PATH, "origin", true);
  });

  it("dismissing the error forgets the hook failure; closing the dialog keeps it", async () => {
    const commit = vi.fn(async () => ({ kind: "HookFailed" as const, hook: "pre-commit", output: "" }));
    const { result } = renderHook(() => useAppState(makeClient({ commit }), TEST_REPO_PATH));
    await act(async () => void (await result.current.commit("m", false)));

    act(() => result.current.closeHookFailure());
    expect(result.current.state.hookFailure?.dialogOpen).toBe(false);
    act(() => result.current.openHookFailure());
    expect(result.current.state.hookFailure?.dialogOpen).toBe(true);
    act(() => result.current.dismissError());
    expect(result.current.state.hookFailure).toBeNull();
  });
});
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run src/state/useAppState.test.ts -t "git hooks"`
Expected: FAIL.

- [ ] **Step 4: Implement**

`useAppState.ts`: the four `AppState` fields (initial `skipHooks: false`, `hookRun: null`, `hookFailure: null`, `clearCommitMessage: false`), `headInfo` initial value gains `commitHooks: []`, `dismissError` also sets `hookFailure: null`. Pass `runMutationWithOutcome` into `useStagingActions` and `useHistoryActions` (new parameter after `runMutation`).

`useStagingActions.commit`:

```ts
  const commit = useCallback(
    async (message: string, noVerify: boolean) => {
      const ok = await runMutationWithOutcome(async () => {
        const outcome = await client.commit(repoPath, message, noVerify);
        if (outcome.kind === "HookFailed") {
          setState((prev) => ({
            ...prev,
            hookFailure: { hook: outcome.hook, output: outcome.output, retry: { kind: "commit", message }, dialogOpen: true },
          }));
          throw new Error(hookFailedMessage(outcome.hook));
        }
        setState((prev) => ({ ...prev, skipHooks: false, hookFailure: null }));
      });
      setState((prev) => ({ ...prev, hookRun: null }));
      return ok;
    },
    [client, runMutationWithOutcome, repoPath, setState],
  );
```

`credentialFailureMessage(new Error("The pre-commit hook failed."))` returns the message unchanged, so `state.error` is the hook message. `useHistoryActions.amendCommit(message, noVerify)` is the same with `client.amendCommit`, `retry: { kind: "amend", message }`, and on success also `amendMode: false` (as today).

`useRemoteTransferActions`: `const lastPushRequest = useRef<PushRequest | null>(null);`; `pushCurrentBranch(remoteName, noVerify = false)` sets `lastPushRequest.current = { operation: "PushBranch", remoteName }` and calls `client.pushCurrentBranch(repoPath, remoteName, noVerify)`; same for `forcePushCurrentBranch` (`"ForcePushBranch"`) and `pushTags` (`"PushTags"`, with `names`). In `transferFailureMessage`, first line after the cancel branch: `if (progress.errorKind === "HookFailed") return hookFailedMessage("pre-push");`. In the `Failed` branch's `setState`:

```ts
          hookFailure:
            progress.errorKind === "HookFailed" && lastPushRequest.current !== null
              ? {
                  hook: "pre-push",
                  output: progress.hookOutput ?? "",
                  retry: { kind: "push", push: lastPushRequest.current },
                  dialogOpen: true,
                }
              : null,
```

The `startTransfer` placeholder `TransferProgress` literal gains `hookOutput: null`.

`useHookActions.ts`:

```ts
import { useCallback, useEffect, useRef } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";

export interface HookActions {
  setSkipHooks(on: boolean): void;
  cancelHookRun(): Promise<void>;
  openHookFailure(): void;
  closeHookFailure(): void;
  clearCommitMessageHandled(): void;
}

// Owns the hook-progress subscription (commit/amend; pushes report through transfer progress)
// and the small pieces of hook UI state that are not tied to one action.
export function useHookActions(
  client: RepoClient,
  repoPath: string,
  hookRun: AppState["hookRun"],
  setState: (updater: (prev: AppState) => AppState) => void,
): HookActions {
  const hookRunRef = useRef(hookRun);
  useEffect(() => {
    hookRunRef.current = hookRun;
  }, [hookRun]);

  useEffect(
    () =>
      client.subscribeHookProgress((event) => {
        if (event.repoPath !== repoPath) return;
        if (event.phase === "Started") {
          setState((prev) => ({ ...prev, hookRun: { operationId: event.operationId, hook: event.hook } }));
        } else {
          setState((prev) =>
            prev.hookRun?.operationId === event.operationId ? { ...prev, hookRun: null } : prev,
          );
        }
      }),
    [client, repoPath, setState],
  );

  const setSkipHooks = useCallback((on: boolean) => setState((prev) => ({ ...prev, skipHooks: on })), [setState]);
  const cancelHookRun = useCallback(async () => {
    const run = hookRunRef.current;
    if (run !== null) await client.cancelTransfer(repoPath, run.operationId);
  }, [client, repoPath]);
  const setDialogOpen = useCallback(
    (open: boolean) =>
      setState((prev) =>
        prev.hookFailure === null ? prev : { ...prev, hookFailure: { ...prev.hookFailure, dialogOpen: open } },
      ),
    [setState],
  );
  const openHookFailure = useCallback(() => setDialogOpen(true), [setDialogOpen]);
  const closeHookFailure = useCallback(() => setDialogOpen(false), [setDialogOpen]);
  const clearCommitMessageHandled = useCallback(
    () => setState((prev) => ({ ...prev, clearCommitMessage: false })),
    [setState],
  );
  return { setSkipHooks, cancelHookRun, openHookFailure, closeHookFailure, clearCommitMessageHandled };
}
```

In `useAppState`, call `useHookActions(client, repoPath, state.hookRun, setState)` and add:

```ts
  const retryWithoutHooks = useCallback(
    async (retry: HookRetry) => {
      setState((prev) => ({ ...prev, hookFailure: null, error: null }));
      if (retry.kind === "commit") {
        if (await commit(retry.message, true)) setState((prev) => ({ ...prev, clearCommitMessage: true }));
        return;
      }
      if (retry.kind === "amend") {
        await amendCommit(retry.message, true);
        return;
      }
      const push = retry.push;
      if (push.operation === "PushBranch") await pushCurrentBranch(push.remoteName, true);
      else if (push.operation === "ForcePushBranch") await forcePushCurrentBranch(push.remoteName, true);
      else await pushTags(push.remoteName, push.names, true);
    },
    [commit, amendCommit, pushCurrentBranch, forcePushCurrentBranch, pushTags, setState],
  );
```

Export everything listed under **Produces** from `UseAppStateResult`.

- [ ] **Step 5: Run tests**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm test -- --run src/state && pnpm lint`
Expected: PASS (App/DiffPane call sites are fixed in Task 10; if `tsc` flags them now, pass `appState.state.skipHooks` there already).

- [ ] **Step 6: Commit**

```bash
git add frontend/src/state frontend/src/lib
git commit -m "feat(frontend): hook failure state, retry without hooks and skip hooks"
```

---

### Task 10: UI — dialog, CommitBox, TransferPanel, App, palette

**Files:**
- Create: `frontend/src/components/HookFailureDialog.tsx`, `HookFailureDialog.module.css`, `HookFailureDialog.test.tsx`
- Modify: `frontend/src/components/CommitBox.tsx`, `CommitBox.module.css`, `CommitBox.test.tsx`, `DiffPane.tsx`, `TransferPanel.tsx`, `TransferPanel.test.tsx`, `frontend/src/App.tsx`, `App.test.tsx`, `frontend/src/lib/commands.ts`, `commands.test.ts`

**Interfaces:**
- Consumes: Task 9.
- Produces: `HookFailureDialog({ failure: HookFailure, onRetry: () => void, onClose: () => void })`; `CommitBox` props `onCommit: (message: string) => Promise<boolean>`, `hooks?: { installed: boolean; skip: boolean; onToggleSkip(on: boolean): void; running: { operationId: string; hook: string } | null; onCancel(): void }`, `clearMessage?: boolean`, `onClearMessageHandled?: () => void`; `amend.onAmend: (message: string) => Promise<boolean> | void`.

- [ ] **Step 1: Write the failing component tests**

`HookFailureDialog.test.tsx`:

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { HookFailureDialog } from "./HookFailureDialog";

const failure = { hook: "pre-commit", output: "lint failed\n", retry: { kind: "commit" as const, message: "m" }, dialogOpen: true };

describe("HookFailureDialog", () => {
  it("shows the hook, its output and the retry", () => {
    const onRetry = vi.fn();
    render(<HookFailureDialog failure={failure} onRetry={onRetry} onClose={() => {}} />);

    expect(screen.getByRole("dialog", { name: "pre-commit hook failed" })).toBeInTheDocument();
    expect(screen.getByText("The pre-commit hook failed.")).toBeInTheDocument();
    expect(screen.getByLabelText("Hook output")).toHaveTextContent("lint failed");
    fireEvent.click(screen.getByRole("button", { name: "Commit without hooks" }));
    expect(onRetry).toHaveBeenCalled();
  });

  it("says when there was no output and hides the retry for prepare-commit-msg", () => {
    render(<HookFailureDialog failure={{ ...failure, hook: "prepare-commit-msg", output: "" }} onRetry={() => {}} onClose={() => {}} />);

    expect(screen.getByLabelText("Hook output")).toHaveTextContent("(no output)");
    expect(screen.queryByRole("button", { name: /without hooks/ })).toBeNull();
    expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
  });
});
```

`CommitBox.test.tsx` additions:

```tsx
  it("keeps the message when the commit fails and clears it when it succeeds", async () => {
    const onCommit = vi.fn().mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    render(<CommitBox onCommit={onCommit} disabled={false} onAbortMerge={() => {}} />);
    const box = screen.getByLabelText("Commit message");
    fireEvent.change(box, { target: { value: "feat: x" } });

    fireEvent.click(screen.getByRole("button", { name: "Commit" }));
    await waitFor(() => expect(onCommit).toHaveBeenCalledTimes(1));
    expect(box).toHaveValue("feat: x");

    fireEvent.click(screen.getByRole("button", { name: "Commit" }));
    await waitFor(() => expect(box).toHaveValue(""));
  });

  it("shows Skip hooks only when hooks are installed", () => {
    const onToggleSkip = vi.fn();
    const hooks = { installed: true, skip: false, onToggleSkip, running: null, onCancel: () => {} };
    const { rerender } = render(<CommitBox onCommit={async () => true} disabled={false} onAbortMerge={() => {}} hooks={hooks} />);
    fireEvent.click(screen.getByLabelText("Skip hooks"));
    expect(onToggleSkip).toHaveBeenCalledWith(true);

    rerender(<CommitBox onCommit={async () => true} disabled={false} onAbortMerge={() => {}} hooks={{ ...hooks, installed: false }} />);
    expect(screen.queryByLabelText("Skip hooks")).toBeNull();
  });

  it("shows the running hook with a cancel button that latches", () => {
    const onCancel = vi.fn();
    render(
      <CommitBox
        onCommit={async () => true}
        disabled={false}
        onAbortMerge={() => {}}
        hooks={{ installed: true, skip: false, onToggleSkip: () => {}, running: { operationId: "commit-1", hook: "pre-commit" }, onCancel }}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("Running pre-commit hook…");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Cancelling…" })).toBeDisabled();
  });

  it("clears the message on a one-shot request", () => {
    const onClearMessageHandled = vi.fn();
    const { rerender } = render(<CommitBox onCommit={async () => true} disabled={false} onAbortMerge={() => {}} />);
    fireEvent.change(screen.getByLabelText("Commit message"), { target: { value: "kept" } });
    rerender(<CommitBox onCommit={async () => true} disabled={false} onAbortMerge={() => {}} clearMessage onClearMessageHandled={onClearMessageHandled} />);
    expect(screen.getByLabelText("Commit message")).toHaveValue("");
    expect(onClearMessageHandled).toHaveBeenCalled();
  });
```

Existing CommitBox tests that pass `onCommit={vi.fn()}` and expect the box to clear must use `vi.fn(async () => true)` and `await waitFor(...)`.

`TransferPanel.test.tsx`:

```tsx
  it("names the running pre-push hook instead of object counts", () => {
    render(<TransferPanel progress={{ ...progress, phase: "RunningHook" }} onCancel={() => {}} />);
    expect(screen.getByText("Running pre-push hook…")).toBeInTheDocument();
    expect(screen.queryByText(/objects/)).toBeNull();
  });
```

`commands.test.ts` (with the file's existing `appState` fixture builder):

```ts
  it("offers to toggle skip hooks only when commit hooks are installed", () => {
    const setSkipHooks = vi.fn();
    const selectRow = vi.fn();
    const withHooks = makeAppState({ headInfo: { ...headInfo, commitHooks: ["pre-commit"] }, skipHooks: false }, { setSkipHooks, selectRow });
    const command = buildCommands(withHooks).find((c) => c.id === "toggle-skip-hooks");
    expect(command?.label).toBe("Skip hooks for the next commit");
    command?.run();
    expect(selectRow).toHaveBeenCalledWith("uncommitted");
    expect(setSkipHooks).toHaveBeenCalledWith(true);

    const on = makeAppState({ headInfo: { ...headInfo, commitHooks: ["pre-commit"] }, skipHooks: true }, { setSkipHooks, selectRow });
    expect(buildCommands(on).find((c) => c.id === "toggle-skip-hooks")?.label).toBe("Run hooks for the next commit");

    const none = makeAppState({ headInfo: { ...headInfo, commitHooks: [] } }, {});
    expect(buildCommands(none).some((c) => c.id === "toggle-skip-hooks")).toBe(false);
  });
```

`App.test.tsx` (with its existing client/render helpers): a commit whose fake returns `HookFailed` opens `dialog[aria-label="pre-commit hook failed"]`; "Close" hides it and the error layer shows a "Show hook output…" button that reopens it; "Commit without hooks" calls `commit(repo, msg, true)` and closes the dialog.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run src/components/HookFailureDialog.test.tsx src/components/CommitBox.test.tsx src/components/TransferPanel.test.tsx src/lib/commands.test.ts src/App.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement**

`HookFailureDialog.tsx`:

```tsx
import type { HookFailure } from "../lib/hookFailure";
import { hookFailedMessage, retryLabel } from "../lib/hookFailure";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import styles from "./HookFailureDialog.module.css";

/** A failed gate hook's output, with a retry that skips the hooks `--no-verify` skips. */
export function HookFailureDialog({
  failure,
  onRetry,
  onClose,
}: {
  failure: HookFailure;
  onRetry: () => void;
  onClose: () => void;
}) {
  const label = retryLabel(failure);
  return (
    <ConfirmDialog
      ariaLabel={`${failure.hook} hook failed`}
      message={
        <>
          <p>{hookFailedMessage(failure.hook)}</p>
          <pre className={styles.output} aria-label="Hook output" tabIndex={0}>
            {failure.output === "" ? "(no output)" : failure.output}
          </pre>
        </>
      }
      // A hook --no-verify cannot skip (prepare-commit-msg) gets no retry: it would fail again.
      confirmLabel={label ?? ""}
      hideConfirm={label === null}
      cancelLabel="Close"
      onConfirm={onRetry}
      onCancel={onClose}
    />
  );
}
```

`ConfirmDialog` gains an optional `hideConfirm?: boolean` prop (default `false`) that omits the confirm button; add a `ConfirmDialog.test.tsx` case ("renders only the cancel action when hideConfirm is set"). Add `frontend/src/components/primitives/ConfirmDialog.tsx` and its test to this task's commit.

`HookFailureDialog.module.css`:

```css
.output {
  max-height: 50vh;
  max-width: min(80ch, 80vw);
  overflow: auto;
  margin: 0;
  padding: 8px;
  font-family: var(--font-mono, ui-monospace, monospace);
  font-size: 12px;
  white-space: pre;
  background: var(--surface-sunken, transparent);
  border: 1px solid var(--border, currentColor);
  border-radius: 4px;
}
```

(Use the token names `frontend/src/styles` actually defines; `grep -n "mono\|--border" frontend/src/styles/*.css`.)

`CommitBox.tsx`:
- Props as in **Produces**.
- `commitIfReady` (non-amend branch):

```tsx
    const submitted = message;
    void onCommit(submitted).then((ok) => {
      if (!ok) return;
      // Only clear what was committed: text typed while the commit (and its hooks) ran stays.
      setMessage((current) => (current === submitted ? "" : current));
      setLastSeeded("");
    });
```

- One-shot clear:

```tsx
  useEffect(() => {
    if (!clearMessage) return;
    setMessage("");
    setLastSeeded("");
    onClearMessageHandled?.();
  }, [clearMessage, onClearMessageHandled]);
```

- Under the amend toggle:

```tsx
      {hooks?.installed === true && (
        <label className={styles.amendToggle} title="Skip the pre-commit and commit-msg hooks for the next commit (--no-verify)">
          <input type="checkbox" checked={hooks.skip} onChange={(event) => hooks.onToggleSkip(event.target.checked)} />
          Skip hooks
        </label>
      )}
      {hooks?.running != null && (
        <p role="status" className={styles.hookStatus}>
          {runningHookLabel(hooks.running.hook)}{" "}
          <button
            type="button"
            disabled={cancellingId === hooks.running.operationId}
            onClick={() => {
              setCancellingId(hooks.running!.operationId);
              hooks.onCancel();
            }}
          >
            {cancellingId === hooks.running.operationId ? "Cancelling…" : "Cancel"}
          </button>
        </p>
      )}
```

with `const [cancellingId, setCancellingId] = useState<string | null>(null);` (keyed by operation id, like `TransferPanel`). Add `.hookStatus` to `CommitBox.module.css` (same font size/color as `.reason`).

`DiffPane.tsx`: pass-through props `onCommit: (message: string) => Promise<boolean>`, `onAmendCommit?: (message: string) => Promise<boolean>`, `skipHooks?: boolean`, `onSetSkipHooks?: (on: boolean) => void`, `hookRun?: { operationId: string; hook: string } | null`, `onCancelHookRun?: () => void`, `clearCommitMessage?: boolean`, `onCommitMessageCleared?: () => void`; on `<CommitBox>`:

```tsx
            hooks={{
              installed: headInfo.commitHooks.length > 0,
              skip: skipHooks,
              onToggleSkip: onSetSkipHooks,
              running: hookRun,
              onCancel: onCancelHookRun,
            }}
            clearMessage={clearCommitMessage}
            onClearMessageHandled={onCommitMessageCleared}
```

(`NO_HEAD_INFO` gains `commitHooks: []`; defaults `skipHooks = false`, `hookRun = null`, no-op callbacks.)

`TransferPanel.tsx`: when `progress.phase === "RunningHook"`, render `<p>{runningHookLabel("pre-push")}</p>` instead of the phase/progress track/objects/bytes lines; Cancel stays.

`App.tsx`:
- `onCommit={(message) => announce("Committed", appState.commit)(message, appState.state.skipHooks)}`, `onAmendCommit={(message) => announce("Amended commit", appState.amendCommit)(message, appState.state.skipHooks)}`, plus the new DiffPane props from `appState`.
- Error layer, after the force-push offer button:

```tsx
          {appState.state.hookFailure !== null && !appState.state.hookFailure.dialogOpen && (
            <button type="button" onClick={appState.openHookFailure}>
              Show hook output…
            </button>
          )}
```

- Next to the force-push `ConfirmDialog`:

```tsx
      {active && appState.state.hookFailure?.dialogOpen === true && (
        <HookFailureDialog
          failure={appState.state.hookFailure}
          onRetry={() => {
            const retry = appState.state.hookFailure!.retry;
            void announce(retrySuccessLabel(retry), appState.retryWithoutHooks)(retry);
          }}
          onClose={appState.closeHookFailure}
        />
      )}
```

`lib/commands.ts`, next to `amend-last-commit` but outside the `repositoryOperationDisabled` guard (it changes no repository state):

```ts
  if (state.headInfo.commitHooks.length > 0) {
    commands.push({
      id: "toggle-skip-hooks",
      label: state.skipHooks ? "Run hooks for the next commit" : "Skip hooks for the next commit",
      keywords: ["hooks", "skip", "no-verify", "bypass", "pre-commit", "commit-msg"],
      run: () => {
        appState.selectRow("uncommitted");
        appState.setSkipHooks(!state.skipHooks);
      },
    });
  }
```

- [ ] **Step 4: Run tests**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm test -- --run && pnpm lint && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): hook failure dialog, skip hooks, running hook status"
```

---

### Task 11: Tauri E2E flow

**Files:**
- Create: `e2e/specs/git-hooks.spec.ts`

- [ ] **Step 1: Write the spec**

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "hooks-e2e.txt";
const SUMMARY = "e2e: committed without hooks";
const HOOK_OUTPUT = "e2e-hook: lint failed";

function git(...args: string[]): string {
  return execFileSync("git", args, { cwd: E2E_REPO_PATH }).toString().trim();
}

describe("Browsitory git hooks", () => {
  let hookPath = "";

  before(function () {
    if (process.platform === "win32") this.skip();
    const hooksDir = path.resolve(E2E_REPO_PATH, git("rev-parse", "--git-path", "hooks"));
    fs.mkdirSync(hooksDir, { recursive: true });
    hookPath = path.join(hooksDir, "pre-commit");
    fs.writeFileSync(hookPath, `#!/bin/sh\necho "${HOOK_OUTPUT}" >&2\nexit 1\n`, { mode: 0o755 });
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "hooked\n");
  });

  after(() => {
    // Later specs share this fixture repo: never leave the hook (or a stray staged file) behind.
    if (hookPath) fs.rmSync(hookPath, { force: true });
    if (git("log", "-1", "--format=%s") !== SUMMARY) {
      try {
        git("reset", "-q", "--", FILE);
      } catch {
        // not staged
      }
      fs.rmSync(path.join(E2E_REPO_PATH, FILE), { force: true });
    }
  });

  it("shows a failing pre-commit's output and commits without hooks", async () => {
    const commitMessageInput = await $("textarea[placeholder='Commit message']");
    await commitMessageInput.waitForExist({ timeout: 10000 });
    const stageButton = await $(`button[aria-label="Stage ${FILE}"]`);
    await stageButton.waitForExist({ timeout: 10000 });
    // Same opacity-0 workaround as first-flow.spec.ts.
    await browser.execute((el) => (el as HTMLElement).scrollIntoView({ block: "center" }), stageButton);
    await browser.execute((el) => (el as HTMLElement).click(), stageButton);
    await browser.execute(
      (el, value) => {
        const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")?.set;
        setter?.call(el, value);
        el.dispatchEvent(new Event("input", { bubbles: true }));
      },
      commitMessageInput,
      SUMMARY,
    );
    await browser.execute((el) => (el as HTMLElement).click(), await $("button=Commit"));

    const dialog = await $(`dialog[aria-label="pre-commit hook failed"]`);
    await dialog.waitForExist({ timeout: 10000 });
    const output = await $(`pre[aria-label="Hook output"]`);
    await browser.waitUntil(async () => (await output.getText()).includes(HOOK_OUTPUT), {
      timeout: 10000,
      timeoutMsg: "hook output not shown",
    });
    if (git("log", "-1", "--format=%s") === SUMMARY) throw new Error("commit landed despite the failing hook");

    await browser.execute((el) => (el as HTMLElement).click(), await $("button=Commit without hooks"));

    await browser.waitUntil(() => git("log", "-1", "--format=%s") === SUMMARY, {
      timeout: 10000,
      timeoutMsg: "commit without hooks did not land",
    });
    await dialog.waitForExist({ timeout: 10000, reverse: true });
    await browser.waitUntil(async () => (await commitMessageInput.getValue()) === "", {
      timeout: 10000,
      timeoutMsg: "commit message was not cleared after the retried commit",
    });
  });
});
```

- [ ] **Step 2: Run the Tauri E2E**

Use the exact sequence in CLAUDE.md's "E2E: Tauri" section (frontend build with `VITE_E2E_REPO_PATH`, then `cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override`, then `cd e2e && pnpm install && pnpm test`), under `xvfb-run` on a headless machine. To iterate: `pnpm test -- --spec ./specs/git-hooks.spec.ts`.
Expected: PASS, and a full run still passes (the spec adds one commit to the shared fixture and removes its hook).

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/git-hooks.spec.ts
git commit -m "test(e2e): failing pre-commit and commit without hooks"
```

---

### Task 12: Docs

**Files:**
- Modify: `CHANGELOG.md` (end of `## [Unreleased]`), `docs/USER_GUIDE.md` (new `### Hooks` after `### Signing commits`), `docs/ARCHITECTURE.md` (new `## Git hooks` after `## Commit search`)

- [ ] **Step 1: CHANGELOG**

```markdown
### Git hooks

- Commits and amends run your repository's `pre-commit`, `prepare-commit-msg`, `commit-msg` and
  `post-commit` hooks; pushes run `pre-push`; switching branches, checking out a commit, merging
  and pulling run `post-checkout` / `post-merge`. `core.hooksPath` is honored.
- A failing hook stops the commit or push and shows its output, with **Commit without hooks** /
  **Push without hooks** to retry (`--no-verify`). A **Skip hooks** checkbox (or the palette)
  skips them for the next commit.
- While a hook runs, "Running pre-commit hook…" shows with a Cancel button; hooks are stopped
  after 10 minutes. On Windows hooks run with Git for Windows' `sh`.
- A commit that fails no longer clears the message you typed.
```

- [ ] **Step 2: USER_GUIDE**

A `### Hooks` subsection with:
- Which hooks run: table of hook → when (commit/amend: pre-commit, prepare-commit-msg, commit-msg, post-commit; push of a branch, force push, tags (and, after 4b, publish/delete remote branch): pre-push; switch branch / check out commit: post-checkout; merge, fast-forward pull: post-merge).
- Hooks come from `core.hooksPath` or `.git/hooks`; they must be executable (Unix). Hook managers (husky, pre-commit, lefthook) work through the scripts they install.
- A failing `pre-commit`, `commit-msg` or `pre-push` blocks the operation; the dialog shows its output; **Commit/Amend/Push without hooks** retries with `--no-verify`. A failing `prepare-commit-msg` cannot be skipped. Failures of `post-*` hooks are ignored.
- **Skip hooks** under the commit message (shown only when a commit hook is installed) or "Skip hooks for the next commit" in the palette.
- Hooks get the same environment git gives them; like signing, apps launched from the macOS Dock may not see your shell's `PATH` — use absolute paths in hooks or extend `PATH` inside them.
- Windows: hooks run through `sh.exe` from Git for Windows; without it, a gate hook fails with a message saying so (use the retry to skip it), post hooks are skipped.
- Running hooks show "Running … hook…" with Cancel; hooks are stopped after 10 minutes.
- Rebase, cherry-pick, revert, undo/redo do not run hooks. `pre-push` runs after Browsitory contacts the remote and sees the real remote commit, as in git.

- [ ] **Step 3: ARCHITECTURE**

```markdown
## Git hooks

- `git-core::hooks` finds a hook in `core.hooksPath` (relative to the worktree root) or
  `<common dir>/hooks` and runs it through `process::run_captured`: stdout and stderr share one
  `std::io::pipe`, the last 64 KiB are kept (ANSI and `\r` redraws stripped), the process is
  polled every 20 ms for exit, cancel and a 10-minute deadline, and on Unix it leads its own
  process group so timeout/cancel kill its children too. Windows runs hooks through the `sh.exe`
  next to `git.exe`.
- Gate hooks live in `git-core`: `commit`/`amend_head` (`pre-commit` → reload index →
  `prepare-commit-msg` → `commit-msg` via `COMMIT_EDITMSG` → commit → `post-commit`) and
  `push_refs`'s `push_negotiation` callback (`pre-push` with git's stdin, after the ref
  advertisement and the lease check, before any pack data). `post-checkout`/`post-merge` run in
  the `repo-service` worker after successful commands only, so undo restores and git-core
  internals never trigger hooks.
- A hook failure crosses IPC as data: `CommitOutcome::HookFailed` for commit/amend, and
  `TransferErrorKind::HookFailed` + `hookOutput` for pushes. Hook output is the only process
  output forwarded over IPC; remote sideband text still is not.
- Progress: `HookReporter` → `HookEvent` → `hook-progress` / `hookProgress` for commit/amend;
  `TransferPhase::RunningHook` for pushes. Commit/amend get `commit-<n>`/`amend-<n>` ids in
  `CancelRegistry`, so `cancelTransfer` stops a running hook. Hooks block the repo worker like
  signing does.
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
git commit -m "docs: git hooks in user guide, changelog and architecture"
```

---

## Self-Review Notes

- Spec coverage: runner (T1), discovery/Windows sh (T2), commit sequence + `HeadInfo.commit_hooks` (T3), pre-push (T4), worker outcome/events/cancel (T5), push skip + post hooks (T6), wire (T7), clients (T8), state (T9), UI + palette (T10), E2E (T11), docs (T12). Overlaps handled in T0.
- Names used across tasks: `HookOptions`, `HookReporter`, `HookEvent`, `CommitOutcome`, `commit_reply`, `next_operation_id`, `run_post_hook`, `run_post_checkout`, `head_oid`, `hookFailedMessage`, `retryLabel`, `retrySuccessLabel`, `runningHookLabel`, `retryWithoutHooks`, `clearCommitMessage`/`clearCommitMessageHandled`.
