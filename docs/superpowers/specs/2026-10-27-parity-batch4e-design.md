# Feature parity, batch 4e: Git hooks

## Goal

Run the repository's client-side git hooks the way `git commit` / `git push` / `git switch` /
`git merge` do: `pre-commit`, `prepare-commit-msg`, `commit-msg`, `post-commit`, `pre-push`,
`post-checkout` and `post-merge`. A failing gate hook blocks the operation and shows its output,
with a one-click retry that skips the skippable hooks (`--no-verify`). Both frontends (Tauri app,
VSCode extension) through `RepoClient`. Refines section 4e of
`docs/superpowers/specs/2026-10-27-parity-batch4-roadmap-design.md` (issue #94).

## Current state

- libgit2 runs no hooks. Nothing in `git-core` spawns a hook today; batch 1 listed hooks out of
  scope.
- `git-core::signing::run_program` is the only process runner: `std::process::Command` with piped
  stdin/stdout/stderr, stdin written from a separate thread, `wait_with_output`, and
  `CREATE_NO_WINDOW` on Windows. **Known gap: no timeout and no cancel** — a signer that never
  exits blocks the repo worker forever. stdout and stderr are captured separately, not combined.
- `git-core::commit::commit(repo: &mut Repository, message: &str) -> Result<String, CommitError>`
  and `amend_head(repo: &Repository, message: &str) -> Result<String, CommitError>` both end in
  `signing::create_commit`. `commit` reads `MERGE_HEAD` (`mergehead_foreach` needs `&mut`) and
  `CHERRY_PICK_HEAD` before building the tree from `repo.index()`; it never re-reads the index
  from disk.
- `CommitError` = `Write(git2::Error)`, `NothingToAmend`, `OperationInProgress`, `Sign(SignError)`.
- `git-core::remote::push_refs` (private) is the single funnel for `push_current_branch`,
  `force_push_current_branch` and `push_tags`. Its `push_negotiation` callback runs **after** the
  remote's ref advertisement and **before** any pack data is sent; it is already where cancel and
  the force-with-lease check happen, and its `PushUpdate`s carry the live remote SHA (`src()`)
  and the new SHA (`dst()`).
- `RemoteError` has no hook variant. `TransferErrorKind` = `NonFastForward`, `RejectedRemoteRef`,
  `StaleLease`, `MissingCredential`, `CredentialStoreFailure`, `SshAgentFailure`,
  `TransferFailed`, `Cancelled`. `TransferPhase` = `Receiving`, `Updating`.
- `repo-service`: pushes are asynchronous — the reply is the operation id, the outcome arrives as
  `TransferEvent::Completed { operation_id, operation, error: Option<TransferErrorKind> }` on a
  per-command `Sender<TransferEvent>`. Cancel goes through `CancelRegistry` (side channel) via
  `TransferOp { id, cancelled }` (`worker/remote.rs`). `Commit` replies `Result<String, String>`,
  `AmendCommit` replies `Result<(), String>`; neither has an event channel. Both are recorded for
  undo (`plan_for`, `hold_reply`).
- Tauri: `TransferProgressDto` deliberately sends `message: None` — remote sideband text is never
  forwarded over IPC. The sidecar mirrors this. Structured errors only exist for clone
  (`CloneErrorDto`, Tauri only); the sidecar's JSON-RPC errors are message strings. Outcome types
  (`PickOutcome`, `MergeOutcome`, `UndoOutcome`) are the established way to return a
  non-exceptional failure through both transports.
- Frontend: `CommitBox` has an "Amend last commit" checkbox and no overflow menu.
  `commitIfReady` calls `onCommit(message)` and **clears the message immediately, before the
  commit result is known** (see "Pre-existing bug"). `ConfirmDialog` takes a `ReactNode` body.
  A failed branch push shows its message in the error layer plus an offer button
  (`forcePushOffer`). `TransferPanel` prints `progress.phase` verbatim.
- `HeadInfo { head_id, detached_at, head_pushed, operation }` is refreshed on every refresh and is
  what `CommitBox` reads for amend availability and the "Already pushed" warning.
- The Tauri E2E suite shares one fixture repo (`/tmp/browsitory-e2e-repo`); each spec prepares it
  with git CLI calls in `before` and must leave it usable for the specs after it.

## Decisions

- **No new dependency.** The process runner is hand-rolled on `std` (`std::io::pipe` is stable
  since Rust 1.87; the toolchain is `stable`). `docs/LICENSE_COMPLIANCE.md` does not change.
- **A new shared runner, `git-core/src/process.rs`.** `run_captured(command, stdin, cancel,
  timeout, limit)` combines stdout and stderr through one `std::io::pipe` (true `2>&1` ordering),
  keeps the **last** 64 KiB (failures print at the end) with a leading
  `[… earlier output omitted …]` line, strips ANSI escape sequences and carriage-return
  progress redraws, polls `try_wait`/cancel/deadline every 20 ms, and waits at most 2 s for the
  pipe to drain after the process exits (a backgrounded grandchild can hold it open forever).
  `signing.rs` keeps its own runner in this batch; moving it onto `run_captured` to close its
  timeout gap is a follow-up, not part of 4e.
- **Kill the whole hook process tree on Unix.** The hook is spawned as a process-group leader
  (`CommandExt::process_group(0)`), and timeout/cancel run `kill -s KILL -- -<pgid>` (the POSIX
  `kill` utility, avoiding a `libc` dependency) before `Child::kill`. On Windows only `sh.exe`
  itself is killed (`TerminateProcess`); grandchildren may survive. Documented.
- **Spawn retries on `ETXTBSY`** (errno 26 on Linux and macOS) up to 5 × 50 ms: a hook written a
  moment ago can still be open for writing in a child forked by another thread (parallel tests).
  A hook without a shebang (`ENOEXEC`, errno 8) is re-run as `sh <hook>`, as git does.
- **Hook discovery.** `hooks_dir(repo)` = `core.hooksPath` (via `Config::get_path`, so `~/` is
  expanded; a relative path is joined to the worktree root, or to the git dir for a bare repo)
  else `repo.commondir()/hooks` — so linked worktrees share the main repository's hooks, as in
  git. On Unix a hook must be a regular file with an execute bit; one without is
  `NotExecutable` and is not run (git prints a hint; we log). On Windows any regular file is a
  hook and runs through `sh.exe`.
- **Windows `sh` discovery.** Find `git.exe` on `PATH`; for its directory, the parent and the
  grandparent, try `sh.exe`, `bin\sh.exe`, `usr\bin\sh.exe` (covers `Git\cmd\git.exe`,
  `Git\bin\git.exe` and `Git\mingw64\bin\git.exe`); else `sh.exe` on `PATH`. The candidate list is
  a pure function (`sh_candidates`) tested on every platform. The hook path is passed to `sh` with
  forward slashes.
- **No `sh` on Windows fails gate hooks loudly, post hooks are skipped with a log line.** The
  roadmap said "skip with a logged notice" for all hooks; silently bypassing a `pre-commit` or
  `pre-push` that a team relies on (secret scanning, lint) is worse than one extra click, and a
  log line in a GUI app is invisible. A gate hook with no shell is reported as a hook failure
  whose output reads `Browsitory could not find sh.exe (install Git for Windows) to run this
  hook.`, so the dialog offers "Commit/Push without hooks". *(Open question 2 — recommendation
  used here.)*
- **Hook environment, like git.** Working directory = worktree root (git dir for a bare repo).
  `GIT_DIR` is **not** set, as in git (≥ 2.x): the hook finds the repository from its working
  directory, so `cd sub && git add x` stages `sub/x`. Inherited `GIT_DIR`, `GIT_WORK_TREE`,
  `GIT_INDEX_FILE` and `GIT_PREFIX` are removed. Commit hooks then get
  `GIT_INDEX_FILE=<git dir>/index` and `GIT_EDITOR=:`. The rest of the environment is inherited
  unchanged; the macOS "GUI apps don't see your shell PATH" caveat is documented exactly as it is
  for signing, with no login-shell PATH resolution.
- **Timeout: 10 minutes** (`HOOK_TIMEOUT`). A timed-out gate hook is a failure whose output ends
  with `Browsitory stopped the hook after 10 minutes.`. `HookOptions::timeout` exists so tests can
  shorten it.
- **Commit hook sequence** (`commit` and `amend_head`): `pre-commit` → if a `pre-commit` actually
  ran, `index.read(true)` so files it staged are committed (lint-staged, formatters) →
  `prepare-commit-msg <file> <source>` → `commit-msg <file>` → write the commit →
  `post-commit` (failure ignored, reported to the caller's `HookReporter`, logged by the worker).
  `<file>` is `<git dir>/COMMIT_EDITMSG` (the path git uses; some tools assume it), written only
  when `prepare-commit-msg` or `commit-msg` will run, and re-read afterwards so hook edits apply.
  `<source>` is `merge` while finishing a merge (`MERGE_HEAD` present) and `message` otherwise
  (Browsitory always supplies the message, like `git commit -m`). A message that is empty or
  whitespace after the hooks aborts with `CommitError::EmptyMessage`
  ("Aborting commit due to empty commit message.", git's text). Without message hooks the message
  is used exactly as before.
- **`no_verify` skips exactly what `--no-verify` skips:** `pre-commit`, `commit-msg` and
  `pre-push`. `prepare-commit-msg` and `post-*` still run. A failure of a hook `no_verify` cannot
  skip (`prepare-commit-msg`) shows the dialog **without** a retry button.
- **`pre-push` runs inside `push_negotiation`, not before connecting.** This deviates from the
  roadmap's "runs before connecting, remote SHA from the local tracking ref": `push_negotiation`
  already fires after the ref advertisement and before any pack data is sent, which is exactly
  when git runs `pre-push`, and its `PushUpdate`s carry the real remote SHA. So stdin matches git:
  one `<local ref> <local sha> <remote ref> <remote sha>` line per update whose old and new SHA
  differ; a deletion is `(delete) 0000… <remote ref> <remote sha>`. Arguments: the remote name and
  its push URL (`pushurl`, else `url`). Order inside the callback: cancel check → lease check →
  `pre-push` (git also rejects a stale lease before the hook). Cancel during the hook kills it and
  reports `RemoteError::Cancelled`. Because the funnel is `push_refs`, every push runs `pre-push`:
  branch, force, tags — and, after 4b, `push_branch` and `delete_remote_branch` (git runs
  `pre-push` for `git push origin :branch` too).
- **`post-checkout` and `post-merge` run in `repo-service`, not in `git-core`.** The worker runs
  them after a successful `SwitchBranch`, `CheckoutCommit` (`post-checkout <old HEAD> <new HEAD>
  1`, old HEAD = zeros when unborn), `StartMerge` with `FastForwarded`/`Merged`, and a
  fast-forwarded `PullCurrentUpstream` (`post-merge 0`). `git-core::branch`/`merge` stay
  side-effect free, so undo/redo restores, rebase internals and tests never trigger hooks. Their
  failures are logged with `eprintln!("browsitory: …")` (the existing worker logging convention),
  never shown. They report no progress and cannot be cancelled; they are bounded by the timeout.
- **Errors in `git-core`, outcomes across IPC.** `git-core` returns
  `CommitError::HookFailed { hook, output }` and `RemoteError::HookFailed { hook, output }` (as the
  roadmap says). The worker turns the commit one into
  `Ok(CommitOutcome::HookFailed { hook, output })` so it crosses both transports as data, like
  `PickOutcome::Conflicted`; other commit errors stay `Err(String)`. For pushes,
  `TransferErrorKind::HookFailed` plus a new `hook_output: Option<String>` on
  `TransferEvent::Completed` (`hookOutput` on the wire, `null` for every other event).
- **Hook output is allowed over IPC, remote text still is not.** Unlike sideband messages, hook
  output comes from a script the user installed in their own repository; showing it is the point
  of the feature. `message` stays `null` everywhere.
- **Progress label from the worker.** `git-core` reports `HookReporter::started(hook)` just
  before spawning a hook that will run, and `finished(hook, &result)` for every hook that was
  found (run, not executable, or no shell). Commit/amend: the worker forwards these as
  `HookEvent::{Started, Finished} { operation_id, hook }` on a per-command channel; Tauri emits
  `hook-progress`, the sidecar sends a `hookProgress` notification, both with
  `{ repoPath, operationId, hook, phase }`. Push: `push_refs` reports
  `TransferPhase::RunningHook` through the existing `TransferReporter` (only when a `pre-push` is
  installed and not skipped), so the transfer overlay shows "Running pre-push hook…" with its
  existing Cancel button. Post-checkout/post-merge report nothing.
- **Commit hooks are cancellable through the existing registry.** `WorkerHandle::commit` /
  `amend_commit` allocate `commit-<n>` / `amend-<n>` operation ids from the same counter as
  transfers; `HookEvent::Started` carries it; the frontend cancels with the existing
  `cancelTransfer(repoPath, operationId)`. No new IPC method. A cancelled commit is
  `Err("The pre-commit hook was cancelled.")` (shown in the error banner, like "Push cancelled.").
- **"Skip hooks" is a checkbox under "Amend last commit", shown only when a commit hook is
  installed**, for the next commit or amend only (reset after a successful one). There is no
  CommitBox overflow menu to put it in, and one control does not justify adding one. `HeadInfo`
  gains `commit_hooks: Vec<String>` (installed `pre-commit`, `prepare-commit-msg`, `commit-msg`),
  refreshed with the rest of `HeadInfo`, also for an unborn HEAD. Palette: "Skip hooks for the
  next commit" / "Run hooks for the next commit". *(Open question 1 — recommendation used here.)*
- **Failure UI reuses `ConfirmDialog`.** `HookFailureDialog`: aria-label `<hook> hook failed`, the
  line `The <hook> hook failed.`, the output in a scrollable monospace `<pre aria-label="Hook
  output" tabindex="0">` (`(no output)` when empty), confirm button "Commit without hooks" /
  "Amend without hooks" / "Push without hooks" (absent for a non-skippable hook), cancel button
  "Close" (autofocused, as `ConfirmDialog` does for every dialog). It opens automatically; the
  error banner keeps `The <hook> hook failed.` and a "Show hook output…" button (the
  `forcePushOffer` pattern) to reopen it. Dismissing the banner forgets the failure.
- **Retry replays the exact request with `noVerify: true`.** The failure keeps a serializable
  `HookRetry` (`commit` + message, `amend` + message, or `push` + `PushBranch`/`ForcePushBranch`/
  `PushTags` + remote (+ tag names)). A force push retried from the dialog does not ask for
  confirmation again (it was confirmed once). A commit retried from the dialog clears the
  CommitBox through a one-shot `clearCommitMessage` request (the `focusMessage` pattern).
- **Pre-existing bug fixed here:** `CommitBox` keeps the message until `onCommit` resolves `true`,
  and then clears it only if the user has not typed since. Hook failures make the old
  clear-before-result behavior lose the message on every failed commit.
- **Undo is unchanged.** Commit/amend stay recorded (`RefsAndIndex`), pushes stay unrecorded. A
  `pre-commit` that re-stages files and then fails can leave an index change that is recorded
  under the commit's label; accepted (rare, and undo restores the pre-hook index).
- **Rebase, cherry-pick, revert, undo/redo, worktree creation and clone run no hooks.** Finishing
  a paused cherry-pick/revert/merge through `commit` does run commit hooks (that is `git commit`).
- **Hooks block the repo worker** while they run, as signing does. Other commands queue; cancel
  reaches the hook through the side channel.

## Backend

### `git-core/src/process.rs` (new, crate-private)

```rust
pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(20);
pub(crate) const DRAIN_GRACE: Duration = Duration::from_secs(2);

pub(crate) struct Captured { pub success: bool, pub output: String, pub timed_out: bool }

pub(crate) enum RunError { Spawn(std::io::Error), Wait(std::io::Error), Cancelled }

pub(crate) struct TailBuffer { /* last `limit` bytes + truncated flag */ }

/// ANSI CSI/OSC/two-byte escapes removed; "\r\n" → "\n"; within a line, only the text after
/// the last "\r" is kept (progress redraws).
pub(crate) fn clean_output(text: &str) -> String;

pub(crate) fn run_captured(
    command: Command,
    stdin: Option<&[u8]>,
    cancel: &dyn Fn() -> bool,
    timeout: Duration,
    limit: usize,
) -> Result<Captured, RunError>;
```

### `git-core/src/hooks.rs` (new)

```rust
pub const HOOK_TIMEOUT: Duration = Duration::from_secs(600);
pub const OUTPUT_LIMIT: usize = 64 * 1024;
pub const COMMIT_HOOKS: [&str; 3] = ["pre-commit", "prepare-commit-msg", "commit-msg"];
pub const SKIPPABLE_HOOKS: [&str; 3] = ["pre-commit", "commit-msg", "pre-push"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookRun { pub success: bool, pub output: String, pub timed_out: bool }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookOutcome { NotInstalled, NotExecutable, NoShell, Ran(HookRun) }

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HookError {
    #[error("Could not run the {hook} hook: {message}")]
    Spawn { hook: String, message: String },
    #[error("The {hook} hook was cancelled.")]
    Cancelled { hook: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookLookup { Missing, NotExecutable(PathBuf), Found(PathBuf) }

pub trait HookReporter {
    fn started(&mut self, hook: &str);
    fn finished(&mut self, hook: &str, result: &Result<HookOutcome, HookError>);
}

pub struct HookOptions<'a> {
    pub no_verify: bool,
    pub cancel: Option<&'a dyn Fn() -> bool>,
    pub reporter: Option<&'a mut dyn HookReporter>,
    pub timeout: Duration,          // Default: HOOK_TIMEOUT
}

pub struct HookInvocation<'a> {
    pub name: &'a str,
    pub args: &'a [&'a str],
    pub stdin: Option<&'a [u8]>,
    pub env: &'a [(&'a str, &'a OsStr)],
}

pub fn hooks_dir(repo: &Repository) -> Result<PathBuf, git2::Error>;
pub fn find_hook(repo: &Repository, name: &str) -> Result<HookLookup, git2::Error>;
pub fn installed_hooks(repo: &Repository, names: &[&str]) -> Vec<String>;
pub fn sh_candidates(git_exe: &Path) -> Vec<PathBuf>;
pub fn run_hook(repo: &Repository, invocation: &HookInvocation<'_>,
                cancel: &dyn Fn() -> bool, timeout: Duration) -> Result<HookOutcome, HookError>;
/// `run_hook` plus reporter calls, using `options.cancel`/`timeout`.
pub fn run_reported(repo: &Repository, invocation: &HookInvocation<'_>,
                    options: &mut HookOptions<'_>) -> Result<HookOutcome, HookError>;
/// Text shown for a failed gate hook (output + timeout / no-shell note).
pub fn failure_output(outcome: &HookOutcome, timeout: Duration) -> Option<String>;
```

`failure_output` returns `Some` for `Ran { success: false }` and, on Windows, `NoShell`; `None`
otherwise. Gate callers treat `Some(text)` as `HookFailed { hook, output: text }`.

### `commit.rs`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitOutcome {
    Committed { commit_id: String },
    HookFailed { hook: String, output: String },
}

pub enum CommitError {
    // existing variants …
    #[error("The {hook} hook failed.")]
    HookFailed { hook: String, output: String },
    #[error("{0}")]
    Hook(#[from] crate::hooks::HookError),
    #[error("Aborting commit due to empty commit message.")]
    EmptyMessage,
    #[error("could not write the commit message file: {0}")]
    Io(#[from] std::io::Error),
}

pub fn commit(repo: &mut Repository, message: &str, hooks: HookOptions<'_>) -> Result<String, CommitError>;
pub fn amend_head(repo: &Repository, message: &str, hooks: HookOptions<'_>) -> Result<String, CommitError>;
```

### `remote.rs`

- `TransferPhase::RunningHook`; `TransferErrorKind::HookFailed`.
- `RemoteError::HookFailed { hook, output }` ("the {hook} hook failed") and
  `RemoteError::Hook(HookError)`; `transfer_error_kind` maps `HookFailed` → `HookFailed`.
- `push_current_branch`, `force_push_current_branch`, `push_tags` gain `no_verify: bool` (after
  the remote/name arguments, before `credentials`); `push_refs` gains `no_verify: bool`.

### `repo.rs`

`HeadInfo` gains `commit_hooks: Vec<String>` = `installed_hooks(repo, &COMMIT_HOOKS)`, on both
the unborn and the normal path.

## `repo-service`, Tauri, sidecar, `RepoClient`

- `worker/hooks.rs` (new): `pub enum HookEvent { Started { operation_id, hook }, Finished {
  operation_id, hook } }` (re-exported from `worker`), `ChannelHookReporter`,
  `log_hook_result(hook, &result)`, `run_post_hook(repo, name, args)`.
- `Command::Commit` / `Command::AmendCommit` gain `no_verify: bool`, `operation_id: String`,
  `events: Sender<HookEvent>`; both reply `Result<CommitOutcome, String>`.
  `WorkerHandle::commit(message, no_verify, events)` and `amend_commit(message, no_verify,
  events)`. `hold_reply` moves `AmendCommit` next to `Commit`.
- Push commands and `WorkerHandle::push_current_branch` / `force_push_current_branch` /
  `push_tags` gain `no_verify: bool`. `TransferEvent::Completed` gains `hook_output:
  Option<String>`.
- Post hooks after switch / checkout commit / merge / fast-forward pull, as in Decisions.
- Tauri: `commit(repo_path, message, no_verify, app)` / `amend_commit(…)` →
  `CommitOutcomeDto` (`#[serde(tag = "kind")]`: `{kind: "Committed", commitId}` /
  `{kind: "HookFailed", hook, output}`), `emit_hook_events(app, repo_path, rx)` emitting
  `hook-progress` with `HookProgressDto { repoPath, operationId, hook, phase }`. Push commands
  take `no_verify`. `TransferProgressDto.hook_output`. `HeadInfoDto.commit_hooks`.
- Sidecar: same params (`noVerify` defaults to `false` when absent), its own `CommitOutcomeDto`
  and `HookProgressDto`, a `hookProgress` notification relay, `hookOutput`, `commitHooks`.
- `RepoClient`:

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

commit(repoPath: string, message: string, noVerify: boolean): Promise<CommitOutcome>;
amendCommit(repoPath: string, message: string, noVerify: boolean): Promise<CommitOutcome>;
pushCurrentBranch(repoPath: string, remoteName: string, noVerify: boolean): Promise<string>;
forcePushCurrentBranch(repoPath: string, remoteName: string, noVerify: boolean): Promise<string>;
pushTags(repoPath: string, remoteName: string, names: string[], noVerify: boolean): Promise<string>;
subscribeHookProgress(listener: (event: HookProgress) => void): () => void;
// HeadInfo.commitHooks: string[]; TransferErrorKind += "HookFailed";
// TransferProgress.phase += "RunningHook"; TransferProgress.hookOutput: string | null.
```

## Frontend

- `lib/hookFailure.ts`: `HookRetry`, `PushRequest`, `HookFailure`, `SKIPPABLE_HOOKS`,
  `hookFailedMessage(hook)` → `The <hook> hook failed.`, `retryLabel(retry)`,
  `runningHookLabel(hook)` → `Running <hook> hook…`, `retrySuccessLabel(retry)`.
- `AppState`: `skipHooks: boolean`, `hookRun: { operationId; hook } | null`,
  `hookFailure: HookFailure | null`, `clearCommitMessage: boolean`.
- `state/useHookActions.ts` (new): subscribes to `subscribeHookProgress` (filtered by `repoPath`),
  `setSkipHooks`, `cancelHookRun`, `openHookFailure`, `closeHookFailure`,
  `clearCommitMessageHandled`.
- `useStagingActions.commit(message, noVerify): Promise<boolean>`,
  `useHistoryActions.amendCommit(message, noVerify): Promise<boolean>`: a `HookFailed` outcome
  stores `hookFailure` and fails the mutation with `The <hook> hook failed.`; success resets
  `skipHooks`. `hookRun` is cleared when the mutation settles.
- `useRemoteTransferActions`: push actions take an optional `noVerify` (default `false`), record
  the last `PushRequest`, and a `Failed` event with `HookFailed` stores `hookFailure`.
- `useAppState.retryWithoutHooks(retry)`.
- `components/HookFailureDialog.tsx` (+ CSS): as in Decisions.
- `CommitBox`: `onCommit` returns `Promise<boolean>`; `hooks` prop (installed, skip, toggle,
  running `{ operationId, hook }`, cancel) renders the "Skip hooks" checkbox and a
  `role="status"` line `Running pre-commit hook…` with Cancel/`Cancelling…`; `clearMessage` +
  `onClearMessageHandled` one-shot.
- `TransferPanel`: phase `RunningHook` shows `Running pre-push hook…` and hides the object/byte
  lines.
- `App.tsx`: wires the above; error layer "Show hook output…" button; dialog gated on `active`
  like every other modal.
- Palette: `toggle-skip-hooks` (only when `headInfo.commitHooks` is non-empty), keywords hooks,
  skip, no-verify, bypass, pre-commit, commit-msg.

## Testing

- `git-core` inline (`process.rs`): `TailBuffer` keeps the tail and marks truncation;
  `clean_output` strips CSI/OSC and CR redraws. Unix-only: combined stdout/stderr order; stdin is
  delivered; non-zero exit is `success: false`; timeout kills a hook whose background grandchild
  holds the pipe and returns within a few seconds; cancel returns `Cancelled`.
- `git-core` inline (`hooks.rs`): `sh_candidates` for `cmd\git.exe`, `bin\git.exe` and
  `mingw64\bin\git.exe` layouts (all platforms).
- `git-core` `tests/hooks.rs` (`#![cfg(unix)]`, real temp repos, shell-script hooks):
  `hooks_dir` default and relative `core.hooksPath`; non-executable hook not run; hook runs in
  the worktree root without `GIT_DIR`; a pre-commit that `cd`s into a subdirectory stages there; failing pre-commit blocks the commit (HEAD unchanged) and
  returns combined output; pre-commit that stages a file gets it committed (skipped without a
  `git` CLI); commit-msg rewriting the message; prepare-commit-msg sees `message` / `merge`;
  `no_verify` skips pre-commit and commit-msg but runs prepare-commit-msg; message emptied by a
  hook → `EmptyMessage`; post-commit failure does not fail the commit; amend runs hooks; cancel
  kills a sleeping pre-commit; reporter sees `started`/`finished` in order and nothing when no
  hook is installed; `head_info().commit_hooks`; pre-push blocks a push to a bare remote (remote
  unchanged) with output; pre-push stdin/args; up-to-date refs are not listed; `no_verify` skips
  pre-push; `RunningHook` is reported; tags push runs pre-push.
- `git-core` Windows-only: a failing pre-commit runs through the discovered `sh.exe` (skipped
  when none is found).
- `repo-service` (`#[cfg(unix)]` where hooks run): commit replies `HookFailed` outcome and records
  no undo entry; `HookEvent`s arrive Started→Finished with the commit's operation id; cancel via
  `cancel_transfer` stops a sleeping pre-commit; amend `no_verify`; failing pre-push →
  `Completed { error: Some(HookFailed), hook_output: Some(..) }`; post-checkout receives
  `<old> <new> 1` after switch and checkout commit; failing post-checkout still switches;
  post-merge after a merge; `hold_reply` still holds both commit replies.
- `tauri-app`: `CommitOutcomeDto`, `HookProgressDto`, `TransferProgressDto.hookOutput`,
  `HeadInfoDto.commitHooks` serialization.
- `vscode-sidecar`: `commit` returns `{kind: "Committed", commitId}`; a failing pre-commit
  returns `HookFailed` and streams `hookProgress` notifications (Unix).
- Frontend (mock `RepoClient`): both clients' wiring (`noVerify`, `hook-progress` /
  `hookProgress`); `useAppState`: hook failure stores retry and sets the error, retry commits with
  `noVerify` and requests a message clear, skip-hooks resets after success, push hook failure
  from a `Failed` event, `hookRun` from events filtered by repo; `CommitBox`: keeps the message on
  failure, clears on success, checkbox visibility, running line with cancel; `HookFailureDialog`;
  `TransferPanel` `RunningHook`; App: dialog opens, "Show hook output…" reopens it; palette
  entry.
- E2E Tauri (`e2e/specs/git-hooks.spec.ts`): install a failing `pre-commit` into the fixture repo
  (`git rev-parse --git-path hooks`), stage a file, commit, see the dialog with the hook's output,
  click "Commit without hooks", see the commit land; `after` removes the hook (and un-stages and
  deletes the file if the commit never landed) so later specs are unaffected. Skipped on Windows.

## Docs

- `docs/USER_GUIDE.md`: a "Hooks" subsection after "Signing commits": which hooks run and when,
  `core.hooksPath`, Skip hooks / retry, the 10-minute limit and Cancel, Windows needs Git for
  Windows' `sh`, the macOS PATH caveat, what does not run hooks.
- `CHANGELOG.md` under Unreleased: "Git hooks".
- `docs/ARCHITECTURE.md`: "Git hooks" section after "Commit search".
- No `docs/LICENSE_COMPLIANCE.md` change.

## Overlaps with other batches

All of 4b–4d (and 4f/4g, if they land first) merge before this batch is implemented; the plan's
pre-flight re-checks every name below.

- **4b (refs and remotes).** `push_current_branch` becomes a wrapper over
  `push_branch(repo, remote, local_branch, remote_branch, set_upstream, …)`, and
  `delete_remote_branch` pushes `:refs/heads/<b>` through the same funnel. `no_verify` goes on
  `push_branch`, `delete_remote_branch`, `force_push_current_branch` and `push_tags`; the wrapper
  passes it through. `pre-push` is wired once in `push_refs`, so all of them run it (deletes
  produce a `(delete)` stdin line). The frontend "Push to…" dialog and the remote-branch delete
  get `PushRequest` variants (`PushBranchTo`, `DeleteRemoteBranch`) so the retry replays them.
  `fetch_all` runs no hooks.
- **4c (merge and stash options).** `start_merge(repo, branch, opts: MergeOptions)`: `post-merge`
  runs after `FastForwarded`/`Merged`, and after a squash merge with argument `1`
  (`opts.squash`). A squash leaves the user to commit; that commit runs commit hooks with
  `prepare-commit-msg` source `squash` when `SQUASH_MSG` exists. No hooks for stash operations.
- **4g (conflicts, picks and polish).** `amend_head` gains `author: Option<SignatureInput>` and
  `commit` gains trailers. Trailers must be appended **before** `COMMIT_EDITMSG` is written so
  `commit-msg` (commitlint and similar) sees the final message. If 4g introduced an options
  struct for commit/amend, put `no_verify` in a separate `HookOptions` argument as specified here
  rather than merging the two. 4g's CommitBox controls (counter, co-author, author field) and the
  "Skip hooks" checkbox share the area under the textarea. Sequenced cherry-pick/revert run no
  hooks.
- **4f (external tools)** spawns detached processes without capture; if it adds a process helper,
  do not merge it with `process::run_captured` (different lifetimes and no capture).
- **E2E flakiness fixes (#98–#100)** may add shared helpers in `e2e/support/` (setting textarea
  values, clicking hidden buttons, waiting for refresh). The new spec uses them if they exist.

## Out of scope

Server-side hooks; `reference-transaction`; `pre-merge-commit` (merges Browsitory creates
without conflicts do not run it — a known difference from `git merge`); `pre-rebase`,
`post-rewrite`, `pre-auto-gc`, `post-index-change`, `push-to-checkout`, `sendemail-validate`,
`fsmonitor-watchman`; hooks during rebase, cherry-pick, revert, undo/redo, worktree creation and
clone; hook managers' config files (husky, pre-commit, lefthook — their generated hook scripts
run like any other); killing a hook's grandchildren on Windows (job objects); a configurable
timeout; progress or cancel for post-checkout/post-merge; moving `signing.rs` onto the new runner;
a VSCode E2E flow for hooks.
