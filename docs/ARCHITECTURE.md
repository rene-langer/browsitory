# Architecture

## Crate/package layout

```
browsitory/
├── crates/
│   ├── git-core/        # git2-based service layer, UI-agnostic, unit-tested headlessly
│   ├── config/          # repo registry + preferences: recent-repos list, backed by TOML
│   ├── repo-service/    # transport-agnostic worker threads, credentials, forge/PR API access
│   ├── tauri-app/        # thin Tauri command adapter over repo-service
│   └── vscode-sidecar/  # JSON-RPC-over-stdio adapter over repo-service, for the VSCode extension
├── extension/             # VSCode extension host, webview security, and sidecar lifecycle
└── frontend/              # React + TypeScript + Vite, shared by Tauri and the VSCode webview
```

`vscode-sidecar` is `tauri-app`'s sibling for the VSCode extension target: a standalone binary
speaking line-delimited JSON-RPC 2.0 over stdio instead of Tauri's IPC. It wires every
`RepoClient` method except the VSCode-native ones (`pickRepoFolder`, `getAppVersion`,
`getLastSeenVersion`, `setLastSeenVersion`, `openExternalUrl`, `logFrontendError`, and the
external-tool launchers `openInEditor`, `revealInFileManager`, `openTerminal`, `openDiffTool`,
`openMergeTool`), which the `extension/` host
implements directly against VSCode APIs instead of round-tripping through the sidecar — see
`docs/superpowers/specs/2026-08-30-vscode-extension-design.md`'s "VSCode-native integrations"
section. Transfer progress (`subscribeTransferProgress`) rides the same JSON-RPC connection as
everything else, as server-initiated notifications (`crates/vscode-sidecar/src/dispatch.rs`'s
`spawn_progress_relay`) rather than request/response calls. Target-specific VSIX packages each
bundle the `extension/` host, the `frontend/dist-vscode` webview bundle, and exactly one matching
sidecar binary.

### VSCode host and sidecar lifecycle

One `SidecarBridge` belongs to each open Browsitory webview panel. The host handles the
VSCode-native methods above itself; all repository/config/credential requests keep their
original JSON-RPC id and are forwarded one-per-line to one lazily spawned sidecar. Valid sidecar
responses remove their ids from the pending set before being relayed unchanged.

The bridge state machine is `idle` → `running` → `reconnecting` or `failed`. Process `exit` or
`error` and stdin write failure detach the old process, reject every unresolved id with JSON-RPC
code `-32001`, and emit one id-less `transportStatus` notification carrying the same diagnostic.
Requests are never retained for replay, because repository methods include mutations whose
completion is unknowable after transport loss. A later repository request makes one fresh spawn
attempt; a synchronous spawn failure moves to `failed`, rejects that request, and waits for a
later user retry rather than scheduling background restarts. Panel closure and extension
deactivation dispose the webview listener and bridge, kill a live child, and drain pending ids.

## Why Tauri + a web frontend, not egui again

A prior pass (see git history on this branch before 2026-08-11) used `egui` for a
single-language, no-webview native UI. It worked, but `egui`'s immediate-mode canvas can't be
embedded in a VSCode webview later — a stated product requirement ("frontend shall be either
the standalone app or a vscode integration"). Tauri packages a plain web frontend as a
standalone app today, and that same frontend becomes the VSCode webview later by adding one
more `RepoClient` implementation — no UI rewrite.

## Why git2 (libgit2 bindings), not gitoxide

`git2` is mature and complete: native blame, native interactive rebase primitives, native
merge conflict handling, native stash/cherry-pick, native remote transports. The pure-Rust
alternative (`gitoxide`/`gix`) is a closer license fit (no GPL exception needed at all), but
its write-side operations (merge, rebase) are less mature as of this writing. `git2` is the
pragmatic choice; the libgit2 license deviation is documented, not silently accepted — see
`CLAUDE.md`'s License policy section.

## The `RepoClient` IPC boundary

`frontend/src/ipc/RepoClient.ts` defines the interface every UI component depends on — see that
file directly for the current method list rather than a copy here, which would just go stale.

`frontend/src/ipc/tauriRepoClient.ts` implements it over `@tauri-apps/api`'s `invoke()`. This
is the *only* file allowed to import `@tauri-apps/api` — every other frontend file receives a
`RepoClient` as a prop/context value, so it can't accidentally couple to Tauri.
`frontend/src/ipc/vscodeRepoClient.ts` implements the same interface over webview `postMessage`;
`frontend/src/ipc/transportStatus.ts` is a transport-neutral lifecycle seam that lets `App`
reuse its global `InlineError` path without importing VSCode APIs. Tauri registers no lifecycle
status listener and retains its existing updater/error behavior.

The rule is enforced mechanically, not just by review: `frontend/eslint.config.js` has a
`no-restricted-imports` override banning `@tauri-apps/*` imports from
`frontend/src/components/**` and `frontend/src/state/**`, so a violation fails `pnpm lint`
(and CI).

## git2 API gotchas

- `StatusEntry::path()`, `Signature::name()`/`email()`, and `Reference::shorthand()` return
  `Result<&str, Error>` — never `Option`, not even nested. Handle with `let Ok(x) = ... else {
  continue };` in a loop, or `.ok().unwrap_or_default()` otherwise (no `.flatten()` — there's no
  `Option` layer to flatten; it won't compile). See `crates/git-core/src/status.rs`,
  `crates/git-core/src/log.rs`.
- `Commit::summary()` is the one in this family that's actually `Result<Option<&str>, Error>` —
  `Ok(None)` means no summary, not an error. This is the one that wants
  `.ok().flatten().unwrap_or_default()`. See `crates/git-core/src/log.rs`.
- Both shapes coexist in this crate — check the actual signature rather than assuming from a
  similar-sounding accessor; a plain-`Result` accessor rejects `.flatten()` and fails to compile
  with an unhelpful error at the call site.
- `StringArray::iter()` (from `Repository::remotes()`) yields `Result<Option<&str>, Error>` per
  slot — needs `.iter().flatten().flatten()`, not a single `.flatten()`.
- libgit2 1.9.6's path-limited stash (`stash_save_ext` with a pathspec) is unusable: after
  stashing the paths it resets the *whole* index and worktree to HEAD (forced checkout, plus
  removing untracked files), discarding every change outside the paths, and its untracked commit
  holds all untracked files. git2 0.21's `StashSaveOptions` also has no message setter.
  `stash::save_stash` therefore builds a path-limited stash itself and pushes it on `refs/stash`
  before reverting only the stashed paths; stashing everything still uses `stash_save2`.
- **Path-limited stash shape (deliberate, differs from git).** The worktree commit is HEAD's tree
  plus the selected files' worktree contents, the index commit is HEAD's tree plus the selected
  files' index entries, and the untracked commit holds only the selected untracked files. git
  stores the whole index in the index commit; then "Restore staged changes" (and `git stash
  apply --index`) would re-stage, or refuse over, files the stash never took.
- **Symlink write-through.** A forced checkout of a path that is now a symlink (a tracked file
  replaced by a link, a typechange) writes the blob *through* the link, overwriting its target,
  which may be another file with unsaved edits or a file outside the repository. Likewise any
  path below a symlinked directory. `save_path_limited_stash` refuses a path with a symlinked
  ancestor ("'d/f' is beyond a symbolic link.") before writing anything, and removes a selected
  symlink before its checkout (the link is in the stash; Pop restores it). The revert removes
  untracked files *before* checking out tracked paths (a checkout turning a directory back into
  HEAD's symlink would make `l/f` resolve into the link's target; that case is now refused up
  front, see below), and every removal re-checks for a symlinked ancestor at that moment and
  skips rather than follows.
- A selected tracked path that is now a real directory but a symlink, file or submodule in HEAD
  (or the index, with keep-index) makes `save_stash` refuse before writing anything
  (`StashError::DirectoryReplacedNonDirectory`, like git's "could not write file 'l' with mode
  120000: directory not empty"): the forced checkout would replace the directory and silently
  delete what the stash does not hold, such as ignored files or untracked files left out. The
  untracked-first revert order stays as a second safety net.
- Known libgit2 stash-apply quirks, not handled: an apply can leave a `<path>~Updated upstream`
  file next to a conflicted path, and a failed apply over a directory that replaced a symlink can
  remove HEAD's symlink from the worktree (it is still in HEAD and recoverable with a checkout,
  and the stash is kept).
- libgit2's stash apply fails with "odb: cannot read object: null OID cannot exist" for a stashed
  file whose directory is a symlink in HEAD; `apply_stash` maps it to
  `StashError::SymlinkReplacedByDirectory` (nothing is written, the stash is kept).
- `Repository::stash_pop` drops the stash even when the apply left conflicts; Browsitory applies
  and drops itself. `stash_apply` refuses whenever the index differs from HEAD
  (`ErrorCode::Uncommitted`); `stash::apply_stash` maps that and the other refusals to
  user-facing `StashError` variants.
- `cleanup_state` does not remove `SQUASH_MSG`; `merge::clear_squash_message` does, after a
  commit, an abort, a reset or an undo.
- `git_merge` ignores the merge analysis: called when a fast-forward is possible it still writes
  `MERGE_HEAD`, which is what `FastForward::Never` relies on.

## Threading model

`git2::Repository` **is** `Send` (libgit2 handles can be moved between threads, and `git2`
carries an `unsafe impl Send` to say so) but it is **not** `Sync`: a `&Repository` must never
be used from two threads at once. So the handle can be *given* to exactly one thread, but not
*shared*. That rules out the obvious Tauri shape — managed state requires `Send + Sync`, so a
bare `Repository` can't be `State` at all, and `State<Mutex<Repository>>` (which does satisfy
`Sync`) would funnel every concurrent command invocation through a single lock held for the
duration of blocking git work.

The `!Sync` constraint is what message-passing answers. Each opened repository gets one
dedicated OS thread (`crates/repo-service/src/worker/mod.rs`'s `Worker::spawn`) that opens its
own `Repository` handle and owns it exclusively for the thread's lifetime — the handle is moved
in once and never shared by reference. Tauri commands (`crates/tauri-app/src/commands/mod.rs`)
send a `Command` enum value over a `std::sync::mpsc` channel to that thread and receive the
result over a per-call reply channel; only owned, `Send` command/reply values cross the boundary.
Commands clone the channel `Sender` out of the state mutex and drop the guard before blocking
on a reply, so one slow repository operation can't serialize unrelated commands. One worker
thread per open repo also means multiple repos never contend on a shared handle.

**Every Tauri command that blocks must be an `async fn`.** A plain `#[tauri::command] fn` is
dispatched by `tauri-macros`' `ExecutionContext::Blocking` path, which runs the function body
*inline on the main/UI thread*; an `async fn` command takes the `ExecutionContext::Async` path
(`InvokeResolver::respond_async_serialized` → `async_runtime::spawn`) and runs off the main
thread. So anything that parks the calling thread — `reply_rx.recv()` on a worker round-trip,
`tauri_plugin_dialog`'s `blocking_pick_folder()` — freezes the whole window if the command is
sync. In `blocking_pick_folder()`'s case it doesn't merely freeze, it *deadlocks*: the dialog
is posted back to the main thread via `run_on_main_thread` and then waited on, so when the main
thread is the caller the dialog can never be dispatched and the app hangs forever. Every
command in `commands/mod.rs` that touches the worker or the dialog plugin is therefore an
`async fn`. Note that an `async fn` command taking a borrowed argument (e.g.
`state: State<'_, AppState>`) must return a `Result`, which is why they all do.

## Undo stack

- **Snapshots** (`git-core::undo`): HEAD, `refs/heads/*` + `refs/tags/*`, the index as a tree,
  and, for commands that touch the working tree, a "worktree tree" (HEAD tree with every
  modified/deleted/untracked file overlaid, raw bytes, no filters). Undo restores `before` over
  `after`, redo the reverse, touching only components that differ and only after verifying each
  still equals the expected side (strict: any drift refuses).
- **Recording** (`repo-service::worker::undo`): the worker snapshots before/after each
  recordable command; failed/no-op commands record nothing; operations that stop on conflicts
  stay pending until completed (one entry) or aborted (none).
- **Pending entry**: the paused operation's label and pre-operation snapshot are stored in
  `undo.json` as `pending` (optional, still version 1: older builds ignore it and drop it on
  their next save), pinned by the undo anchor like the stacks. It carries the marker of the
  operation it belongs to (`worker::gate::OperationProbe::marker`): `rebase <branch ref>
  <original tip> <onto>`, `sequence <kind> <original HEAD> <first commit> <count>`, `merge
  <HEAD> <MERGE_HEAD…> @<mtime>`, `cherry-pick <HEAD> <CHERRY_PICK_HEAD> @<mtime>`, `revert
  <HEAD> <REVERT_HEAD> @<mtime>`, or `git <state> <HEAD>`, each constant for the operation's
  whole life (the stored rebase and sequence come first because their `CHERRY_PICK_HEAD` changes
  per step). `@<mtime>` is the state file's modification time in nanoseconds (omitted when it
  can't be read): git writes `MERGE_HEAD`/`CHERRY_PICK_HEAD`/`REVERT_HEAD` once per operation and
  nothing rewrites them during the pause, so a merge or pick aborted outside the app and started
  again with the same heads gets a new marker, and both processes stat the same file (#172). On
  file systems with 1–2 s mtime resolution an abort and identical restart within one tick still
  collide; a pending entry from a build before this marker format is dropped once. Before every command
  the recorder re-reads `undo.json` and drops a pending entry whose marker isn't the current one
  (`None` included), so an operation finished after an app restart, a closed tab, a crashed
  worker or in the other app (Tauri vs. VSCode) is still one entry, and an operation aborted or
  replaced outside the app never records under the old label.
- **State lock** (`worker/state_lock.rs`, #172): every write to `undo.json` happens under an
  advisory lock on `<gitdir>/browsitory/state.lock` (std's `File::try_lock`: `flock` on
  Linux/macOS, `LockFileEx` on Windows), after a forced reload (`UndoStore::reload`, not the
  mtime check, which misses a second write in the same tick), and re-checks its decision there,
  so two processes (the Tauri app and the VSCode sidecar) or two workers never lose an entry.
  The worker holds the lock from before `settle` until `finish` for every command
  `undo::locks_state` lists (everything `plan_for` records, `RebaseContinue`, `AbortRebase`,
  `ContinueSequence`, `AbortSequence`, `AbortMerge`, Undo, Redo), so no other holder ever sees
  "the operation ended, but its entry isn't recorded yet" and drops it as stale. Other commands
  (reads, fetch, staging, resolving) never wait: `settle`/`finish` take the lock only when about
  to write, and skip the write while it's held (a later command repeats it). The guard is
  dropped before the held replies are released. A locked command that can't get the lock within
  `LOCK_WAIT` (10 s; a long holder such as a pull, a commit hook or a pinentry dialog in the
  other app) logs once and runs unrecorded, and later locked commands try only once until the
  lock is taken again, so the worker's queue waits once, not once per click; Undo/Redo instead reply "Another Browsitory window is changing this repository's
  undo history. Try again in a moment." The lock is an OS lock on a handle, so a process that
  exits or is killed, or a worker thread that panics, releases it; the file is never deleted
  (that would let the next opener lock a new inode). Where locks aren't supported (some network
  file systems) the worker logs once and runs unlocked, as before. Each linked worktree has its
  own lock, like its own `undo.json`. After a failed save (e.g. `undo.json` written by a newer
  version) the in-memory stacks stay authoritative for the session and aren't reloaded. On NFS,
  Linux emulates `flock` with per-process `fcntl` locks, so only separate processes exclude
  each other there. `undo.json`, `sequence.json` and `rebase.json` are written
  by `atomic_file::write_atomically` (temp file `<name>.<pid>.<n>.tmp`, unique per write in a
  process, `sync_all`, rename).
- **Storage**: `<gitdir>/browsitory/undo.json` per worktree (50 entries); objects pinned by
  `refs/browsitory/undo/main` (main worktree) or `refs/browsitory/undo/wt/<name>` (linked
  worktree), an anchor commit visible in `git log --all`, plus
  `refs/browsitory/undo-keep/main/<n>` / `refs/browsitory/undo-keep/wt/<name>/<n>` refs for
  non-commit objects such as tag objects.
- **Known cost**: commands that touch the working tree hash every dirty/untracked file into the
  object database, so very large untracked files make those commands slower and grow `.git`
  until gc after the entry ages out.

## Commit signing

- `git-core::signing::create_commit` is the only way `git-core` creates a user-facing commit
  (commit, amend, cherry-pick, revert, rebase step and squash collapse).
  - It writes the object with `repo.commit(None, …)`, or when `commit.gpgSign` is set, with
    `commit_create_buffer` → signer → `commit_signed`.
  - It then moves HEAD with `reference_matching` against the expected current target, writing
    libgit2's own reflog message (`commit: …`, `commit (initial): …`, `commit (merge): …`,
    `commit (amend): …`).
  - The signer runs before any ref moves, so on failure HEAD and refs never move. Config errors
    (unsupported `gpg.format`, missing SSH key) are checked first and touch nothing, including for
    cherry-pick/revert, which validate before staging. A signer-program failure during
    cherry-pick/revert leaves the pick in progress (`CHERRY_PICK_HEAD`/`REVERT_HEAD`, staged
    result) to commit again or abort, as git does. A rebase retry after a failed squash collapse
    re-baselines the drift check first, so Continue can be retried. The undo anchor commit
    deliberately bypasses signing.
- Signers are external programs run exactly as git runs them: `gpg --status-fd=2 -bsau <key>`
  and `ssh-keygen -Y sign -n git -f <key>` (`-U` for a literal key), with the buffer on stdin,
  no TTY, and no console window on Windows. They run through `process::run_captured_split`
  (stdout and stderr kept apart, the program in its own process group so a kill takes its
  children too) and block the repo's worker thread while pinentry waits, at most
  `SIGN_TIMEOUT` (120 s) → `SignError::TimedOut`. A signature over 1 MiB is an error, never
  truncated. Commit and amend pass the operation's cancel check (`SignOptions`) and report a
  `"signing"` pseudo-hook to the hook reporter, so the commit box shows "Waiting for the signing
  program…" with the hooks' Cancel (`SignError::Cancelled`); cherry-pick, revert and rebase
  commits get the timeout only. Verification is bounded by `VERIFY_TIMEOUT` (30 s), after which
  the signature shows the error status.
  - Running in a process group means a signer launched from a terminal that prompts on
    `/dev/tty` (ssh-keygen with an encrypted key and no agent) is stopped by `SIGTTIN` until the
    timeout or Cancel; pinentry (gpg-agent) is a separate process and unaffected.
- `verify_commit` picks the format from the signature armor, not from config:
  - gpg: `--status-fd` lines map to `SignatureStatus`.
  - ssh: `find-principals` + `verify` against `gpg.ssh.allowedSignersFile`, falling back to
    `check-novalidate` (→ `GoodUntrusted`).
  - Unsigned commits never spawn a process; there is no cache.
- Tests use real `gpg`/`ssh-keygen` with throwaway keys and skip when they are missing.
  `tests/signing_gpg.rs` is its own binary because it sets `GNUPGHOME` once per process.

## Commit search

- `git-core::search` parses the query once (`parse_query`) and pages a `TOPOLOGICAL | TIME`
  revwalk (`search_commits`): cheap filters first, first-parent diff filters (`path:`,
  `contents:`) last. A page ends at 100 matches, after 20,000 scanned commits, or after a
  250 ms time budget (`PageLimits::time`, checked after each commit so a page always makes
  progress) — so a commit/stage queued on the single in-order repo worker never waits behind
  thousands of `path:`/`contents:` tree diffs; a time-boxed page just returns `next_cursor`
  and the frontend's paging round continues.
- The cursor is `<offset>:<fingerprint of the start oids>`; resuming re-walks and skips
  `offset` (polling cancel every 1,024 skipped commits), and any moved start point is
  `StaleCursor` instead of a duplicate or a gap.
- A refresh after any mutation (or a graph branch selection change) deliberately restarts an
  open search from page 1 rather than resuming its cursor.
- One in-flight search per repo: `WorkerHandle::search_commits` bumps a shared
  `SearchGeneration` (`Arc<AtomicU64>`) before queueing, and the running walk polls it, so a
  newer search or `cancel_search` stops an older one without going through the command queue
  (same reasoning as `CancelRegistry`).

## Cherry-pick sequences

- `git-core::pick` is a pure state machine: `cherry_pick_many`/`revert_many`/`continue_sequence`
  take and return a `SequenceState` (`SequenceRun { outcome, state }`, `state: Some` = "keep
  this", `None` = "no sequence any more"); `git-core` has no persistence and no serde.
- `repo-service`'s `SequenceStore` (`worker/sequence_store.rs`) persists it as
  `<gitdir>/browsitory/sequence.json` (per worktree, versioned, temp file + `sync_all` + rename,
  like `undo.json`), written only when a sequence stops and deleted when it completes or is
  aborted. On load it must parse, have a known version (2; a version 1 file loads as an error
  stop with an unknown original ref), and name commits that exist in the repository; anything
  else is logged once and treated as absent, never fatal.
- Whether the stored state still matches the repository is checked when it is used, so a stale
  sequence stays visible and abortable: Continue requires git's paused pick (`CHERRY_PICK_HEAD` /
  `REVERT_HEAD`) to be the sequence's current commit, and HEAD to be where the stop left it;
  otherwise `OperationInProgress` / `HeadMoved`. With git state clean and HEAD unchanged (an
  error stop, or `git cherry-pick --abort` from a terminal) Continue retries the step. Only a
  conflict stop (`stoppedOnConflict`) accepts HEAD one commit further as the step finished by
  hand, and only if git's pick state is gone and the commit is recognisably the step (for a
  cherry-pick `pick::commit_matches_pick`: same author name/email/time, the step's message, and
  every path the step changes cleanly changed too; "This reverts commit <id>" for a revert). The index
  is re-read before use, so a terminal `git add` is seen. Abort aborts its own paused step (or
  the step's conflicts if `CHERRY_PICK_HEAD` was deleted), rewinds only when HEAD still names
  the ref the sequence started on (`originalRef`) and is still at the stop point and git has
  nothing else in progress, and otherwise just forgets the sequence (`rewound: false`).
- A stored sequence counts as "in progress" for undo (one pending entry, dropped on abort),
  blocks picks, merge, rebase, commit/amend, reset, checkout, branch switch, pull, stash
  apply/pop and reflog restore in the worker (`gate::gate_during_operation`), and, like a
  rebase, protects its branch (`gate::GateScope`, #171): deleting or renaming the branch named
  by `originalRef`, or adding a worktree on it, and stash save are refused while it is stored
  (a renamed branch would make Abort answer `rewound: false`; a stash would carry the paused
  step's index away). A detached or unknown `originalRef` protects no branch. It is reported
  as `HeadInfo.sequence`. The undo entry is labelled with the number of commits applied. `AbortMerge` aborts the
  whole sequence when one is stored, so a stale frontend can't strand the file.
- Empty steps are skipped (git stops for `--skip`). Steps, including the one Continue commits,
  run no hooks, matching Browsitory's single cherry-pick/revert. Real git runs
  `prepare-commit-msg`/`post-commit` on a clean pick and the full commit hooks on
  `--continue`.

## Interactive rebase

- `git-core::rebase` is Browsitory's own state machine; it does not use libgit2's `git2::Rebase`
  or write git's `.git/rebase-merge`. `start_rebase` first refuses tracked local changes
  (`RebaseError::LocalChanges`, #170: any staged change, or an unstaged change to a tracked file;
  untracked and ignored files and submodules don't count, as with `git rebase`), because its
  rollback after a failed first step and Abort force-check-out the whole branch tree; from a clean
  tracked tree they can only discard what the rebase itself wrote (or edits made during a pause,
  as `git rebase --abort` does). It then checks out `onto` and detaches HEAD; each
  step is `repo.cherrypick` on the detached HEAD, committed by `land_current_step` (which then
  clears `CHERRY_PICK_HEAD`); squash/fixup groups collapse when the group ends; `finish` moves
  the branch and reattaches HEAD. The branch ref is untouched until `finish`.
- To git a paused Browsitory rebase is a cherry-pick on a detached HEAD: an edit or conflict
  stop is `CherryPick` with `CHERRY_PICK_HEAD` = the step (plus `MERGE_MSG` and, for a conflict,
  the conflicted index); an error stop (a step refused, e.g. a local edit in the way) is `Clean`.
  `git status` suggests `git cherry-pick --continue/--abort`; `git rebase --continue` says no
  rebase is in progress.
- `RebaseState` is plain public data (`plan`, `cursor`, `original_branch_ref`, `original_tip`,
  `onto`, `group_start_parent`, `expected_head`, `stop: Conflict | Edit | Error`). `Conflict`/
  `Edit` mean the step at `cursor` is applied to the index; `Error` means it isn't.
  `cursor == plan.len()` after a failed `finish`.
- **Persistence**: `repo-service`'s `RebaseStore` (`worker/rebase_store.rs`) writes
  `<gitdir>/browsitory/rebase.json` (per worktree, `version` 1, camelCase, temp file +
  `sync_all` + rename) when a start or Continue pauses or fails, deletes it on `Done` and after a
  successful Abort, and is read on every use (no cache), so a restarted or respawned worker and
  the other app see the same rebase. On load the plan must be non-empty with `cursor <=
  plan.len()` and not on a `Drop`, the branch a valid `refs/heads/` name, every id a commit in
  the repository and a `Reword` message non-empty; anything else is logged once and treated as
  absent. Versioning follows `sequence.json`: an additive change bumps the version and keeps
  reading the older one. When saving fails the worker keeps the state in memory for the session
  (`rebase_fallback`, preferred over the file until the next successful save or clear) and
  reports the failure with the outcome.
- **Health** (`rebase_health`, reported as `HeadInfo.rebase.health`; the first failing check
  decides): the branch is gone (`BranchMissing`); it isn't at `original_tip` (`BranchMoved`);
  git has an operation other than this rebase's own pick paused (`OtherOperation`); HEAD isn't
  detached at `expected_head` and isn't the step committed by hand (`HeadMoved`). Otherwise
  `Paused`. A diverged rebase stays stored and visible ("interrupted"); only Abort is offered.
- **Continue** (`rebase_continue`):

  | Situation | Action |
  |---|---|
  | `cursor == plan.len()` | HEAD at `expected_head`, git clean: retry `finish` |
  | own paused pick, HEAD at `expected_head` | commit the index as the step, go on |
  | clean, HEAD at `expected_head` | re-apply the step (error stop, or `--abort` outside) |
  | clean, conflict/edit stop, HEAD = the step committed by hand | adopt it (a `Reword` gets its planned message; a group still collapses), go on |
  | another pick/merge/revert paused | `OperationInProgress` |
  | anything else | `HeadMoved` |

  "Committed by hand" is `pick::commit_matches_pick`: one parent, `expected_head`; the picked
  commit's author name/email/time; its message (or a `Reword`'s planned message; comment lines
  and trailing whitespace ignored); and every path that applying the step onto `expected_head`
  changes cleanly is changed by the commit too (conflicted paths may be resolved any way, and an
  edit stop may change more). Author and time alone would accept another plan commit by the same
  author. Refusals leave the state untouched; a later error
  marks the stop `Error` unless the step is still applied and paused. `finish` moves the branch
  with `reference_matching` against `original_tip` and refuses `BranchMoved`/`BranchMissing`
  instead of overwriting a branch that moved during the pause.
- **Abort** (`abort_rebase` → `RebaseAbort { restored }`): with HEAD at `expected_head` (or at
  the step committed by hand, which health reports as `Paused`), the
  branch still at `original_tip` and nothing else paused, it force-checks-out the branch, then
  reattaches HEAD (checkout first, so a refused checkout leaves HEAD where the rebase can still
  abort) — `restored: true`. Otherwise (diverged) HEAD and refs stay where they are —
  `restored: false`: with HEAD still at `expected_head` the paused step is undone like `git
  cherry-pick --abort`; with HEAD moved only git's pick state is forgotten, so work staged on
  the other branch survives. The worker forgets the state only when Abort succeeded.
- **Writes in the worker**: a refused Continue (state unchanged) writes nothing, and neither does
  a failed Continue that finds HEAD moved by another writer (its commit lost HEAD's
  compare-and-swap), so one app never puts back a state the other app moved on from. A file that can't be deleted
  when the rebase ends is remembered as ended for the session (`gate::RebaseMemory::forgotten`),
  so it doesn't keep gating the repository.
- A stored rebase blocks the same commands as a sequence (`gate::gate_during_operation`, also
  after an error stop where git sees nothing in progress; both also refuse creating a branch,
  which checks it out), plus deleting or renaming the rebased branch or adding a worktree on it
  (it isn't checked out, so git would allow it) and stash save, counts as in progress for undo, and
  is reported as `HeadInfo.rebase { branch, onto, currentStep, totalSteps, health }`, which the
  frontend reads on every refresh (there is no separate progress call).

## Git hooks

- `git-core::hooks` finds a hook in `core.hooksPath` (relative to the worktree root) or
  `<common dir>/hooks` and runs it through `git-core::process::run_captured`, a generic
  captured-run helper, with cwd = worktree root and `GIT_DIR`/`GIT_WORK_TREE`/`GIT_INDEX_FILE`/
  `GIT_PREFIX` removed from the environment (git sets none of them for hooks; a pinned `GIT_DIR`
  breaks hooks that `cd` into a subdirectory; commit hooks get `GIT_INDEX_FILE` back): stdout and stderr share one `std::io::pipe` (true `2>&1` order), the last
  64 KiB are kept (ANSI escapes and `\r` redraws stripped), the child is polled every 20 ms for
  exit, cancel and a 10-minute deadline, and it is always reaped. On Unix it leads its own
  process group and timeout/cancel run `kill -s KILL -- -<pgid>`, so its children die too; after
  exit the pipe gets a 2 s drain grace in case an escaped grandchild holds it open (the hook's
  result is not held up longer than that). Because the hook leads a *background* process group,
  a hook that reads the controlling terminal (app started from a shell, hook opens `/dev/tty`)
  gets `SIGTTIN` and stays stopped until Cancel or the timeout; starting it in a new session
  (`setsid`) would make that fail fast but needs `pre_exec` + `libc`, which this batch avoids.
  Windows runs
  hooks through Git for Windows' `sh.exe` and only kills that process.
- Gate hooks live in `git-core`: `commit`/`amend_head` (`pre-commit` → reload the index only if
  it ran → `prepare-commit-msg` → `commit-msg` via `<git dir>/COMMIT_EDITMSG` → commit →
  `post-commit`) and `push_refs`'s `push_negotiation` callback (`pre-push` with git's stdin,
  after the ref advertisement and the lease check, before any pack data). Every push
  (`push_branch` and its `push_current_branch` wrapper, force push, tags,
  `delete_remote_branch`) funnels through `push_refs`, so all of them run `pre-push`.
  `post-checkout`/`post-merge` run in the `repo-service` worker after successful commands only,
  so undo restores, rebase and other git-core internals never trigger hooks.
- A hook failure crosses IPC as data: `CommitOutcome::HookFailed` for commit/amend, and
  `TransferErrorKind::HookFailed` + `hookOutput` for pushes. Hook output is the only process
  output forwarded over IPC; remote sideband text still is not.
- Progress: `HookReporter` → `HookEvent` → `hook-progress` / `hookProgress` for commit/amend;
  `TransferPhase::RunningHook` for pushes. Commit/amend get `commit-<n>`/`amend-<n>` ids in
  `CancelRegistry`, so `cancelTransfer` stops a running hook. Hooks block the repo worker like
  signing does.
- Commit trailers (`Co-authored-by` from the commit box) are appended by the worker with
  `git_core::commit::append_trailers` *before* `commit`/`amend_head`, so `prepare-commit-msg` and
  `commit-msg` see them, as with `git commit --trailer`.

## Worktree writes and symlinks

libgit2's *forced* checkouts (and plain `std::fs` writes) follow a symlink that replaced a
tracked file, or a tracked folder on the way to one, so they could write or delete outside the
repository (#114). `git-core::worktree_guard` owns every rule about this:

- Every forced checkout calls `worktree_guard::prepare_forced_checkout` immediately before
  libgit2 runs: it `scan`s the checkout's source (target tree or index) **and** the current HEAD
  tree and index (a forced checkout also deletes what those track and the target lacks), limited
  to the checkout's literal paths, and `clear_hazards` removes each link found (only the link,
  never its target, never recursively; a Windows directory link with `remove_dir`). A link that
  no tracked path goes through is the user's and is left alone; a source entry that is itself a
  symlink is not a hazard. Call sites: `reset_to(Hard)`, `discard_file`, undo/redo
  `checkout_from_index`, `abort_merge` (merge, cherry-pick, revert, sequences),
  `resolve_whole_file`, `restore_original_branch` (abort rebase and its rollback).
- "Stash all" cannot clear links in between libgit2's record and reset, so with a hazard it goes
  through the path-limited stash (batch 4c) with every changed path: a file link is stashed and
  then replaced, a folder link is refused (`BeyondSymlink`).
- All of Browsitory's own raw work-tree I/O goes through `worktree_guard` (#149):
  `write_beneath` / `remove_beneath` (conflict resolution, abort merge's removal of added
  files, discarding an untracked file), `read_beneath` + `write_beneath` for **Ignore**'s
  `.gitignore` append, `clear_hazards`, `open_beneath` (image preview), and for undo/redo and the
  path-limited stash `read_beneath` (snapshots and checks; never below a symlinked folder, never
  a FIFO), `replace_beneath` (restore writes; a symlinked folder on the way is removed and
  recreated, never written through) and `remove_and_prune_beneath` (deletions and the folders
  they empty). `undo.rs` keeps only its mode logic.
- Those functions walk the path one component at a time and hold every folder on the way, so a
  folder swapped for a link at any moment can't redirect them. Unix
  (`worktree_guard/beneath_unix.rs`): `openat` with `O_NOFOLLOW | O_DIRECTORY` from the previous
  folder's descriptor, and the leaf acted on through its parent's descriptor (`unlinkat`,
  `openat(O_NOFOLLOW | O_NONBLOCK)` + `fstat`, `symlinkat`, `readlinkat`, `fchmod`). A write
  never truncates a FIFO or device; an existing file keeps its mode. Windows
  (`worktree_guard/beneath_windows.rs`): each component is opened by full path *as itself*
  (`FILE_FLAG_OPEN_REPARSE_POINT`) and held without `FILE_SHARE_DELETE`, which pins it against
  rename and delete until the act is done; the handle's attributes and reparse tag
  (`GetFileInformationByHandleEx`) decide. A name surrogate (symbolic link, junction, WSL link)
  is refused, or removed itself where the caller clears links, and never contacted (a junction
  to a UNC share included). Another reparse point (OneDrive/Cloud Files placeholder) is walked
  through as a folder, or as a file read and written through a second, normal handle (so the
  file system filter sees it) accepted only with the same file ID. Because an *empty* held
  folder can still be turned into a junction in place, and the flag only guards a path's last
  component, every component and leaf opened by path must sit in the held parent
  (`GetFinalPathNameByHandleW` of both); leaves are deleted through their own handle
  (`FileDispositionInfo`), new files are created with `create_new` and deleted again through
  their handle if misplaced. Residual: creating a missing folder (`CreateDirectoryW`) goes by
  path, so in that instant an empty folder could appear in a junction's target (nothing is
  written into it). Hand-declared `kernel32` imports, no new dependency. A `Step` seam in both
  walkers makes the races deterministic in tests.
- A folder link that HEAD itself tracks is not a hazard for a forced checkout: libgit2 replaces
  a tracked link with the target's folder (a typechange) by unlinking it, never following it,
  even when the user retargeted it; removing it first made `reset --hard` fail ("null OID").
- Safe (non-forced) checkouts refuse to *modify* a file below a link but still *create* new
  ones through it, also through a link nothing tracks. So every safe operation first calls
  `refuse_linked_folders` with what it brings in, refusing with `LinkedFolder` ("'d' is a
  symbolic link where the repository has a folder. Remove or rename it first.", one error type
  every operation's error enum wraps) while a symlink stands where HEAD, the index or the
  incoming side has a folder. Tree checkouts (switch, checkout commit, merge fast-forward, pull
  fast-forward, rebase start, sequence rewind) pass the target tree (`Incoming::Checkout`); a
  folder link HEAD tracks is skipped there (libgit2 replaces it, or reports a conflict when it
  was changed). Merge-like operations pass their merged result (`Incoming::Merge`): merge and
  squash `merge_commits`, cherry-pick/revert steps and rebase steps `cherrypick_commit` /
  `revert_commit`, stash apply/pop the stash's trees. Nothing is skipped there: libgit2 merges
  `d/new.txt` without a conflict and writes it through a folder link HEAD tracks. The guard
  examines each folder once across all sources and never stats files (100k-file repo: ~0.24 s).
  A leaf typechange is refused by libgit2 itself. `tests/symlink_write_through.rs` pins every
  operation × {file link, folder link, a new file below a tracked or an untracked folder link, a
  committed folder link}, refusals included, so a future switch to a forced checkout is caught.
- Submodule update refuses when the submodule path or a folder above it is a symlink (libgit2
  would clone through it); nested updates check each nested path the same way.
- `stage_file`, `stage_hunk` and `stage_lines` (and the discard variants) refuse a path below a
  symlinked folder like git ("'d/f' is beyond a symbolic link."): the index→workdir diff would
  read through the link. Staging the link itself replaces the folder's index entries (`git add
  -A`), after which such a path has nothing left to stage and `stage_file` returns `Ok` — so
  "Stage all", which stages each unstaged path in status order (`d` before `d/f`), stages the
  typechange.
- The forced-checkout path filter follows `core.ignorecase` like libgit2's pathspecs.
- Case-insensitive file systems (#156): a tree holding a link (or submodule) `D` and a folder
  `d/…` names one file twice on APFS/NTFS. libgit2 1.9 writes links after all other files in a
  checkout and, under `core.ignorecase`, removes what is in the way, so it silently drops `d/x`
  rather than write through `D` (spec `2026-10-28-security-followups-design.md` §1.1–1.3 has the
  macOS/Windows probe). `worktree_guard::find_case_collision` refuses such a source anyway: a
  pure pass over the index entries that folds names (each character upper- then lowercased,
  without the HFS+ ignorable code points; NFC/NFD is not folded) and compares folded
  link/gitlink paths with folded strict-prefix folders (and a link with a gitlink), both orders
  and exact duplicates, over all of an operation's incoming sources together; a file next to a
  folder is fine. It applies when `folds_case` (`core.ignorecase`, or always on macOS/Windows
  builds). Only collisions an operation *brings in* are refused: `known_collisions` (every
  collision of HEAD's tree, plus `also_known`'s) are exempt with the identical `(link, folder)`
  spelling (#167, decision D9-A), so with HEAD holding `D` + `d/x` unrelated merges,
  cherry-picks, reverts, stash applies, pulls, rebases and a switch to a branch with the same pair
  work, while a second collision or a differently spelled one is still refused. Rebase Abort and
  its start rollback (`restore_original_branch`) and a sequence's rewind pass the tree they
  return to as `also_known` (`*_known` variants, #161): it is a tree the user already had. The
  exemption is safe because the on-disk scan runs first and doesn't depend on it: on a
  case-insensitive file system `lstat("d")` answers with the link `D`, and the `tracked_as_link`
  skip compares exact case, so a write below `d/` through an on-disk `D` is refused
  (`LinkedFolder`) or the link cleared first (forced checkouts). `tests/case_insensitive_links.rs`
  has rows for operations on a HEAD that holds the pair.
  `refuse_linked_folders` checks each incoming source after its link scan, `prepare_forced_checkout` its target
  (path-limited), undo its target work tree before its raw writes, `worktree::create_worktree`
  the branch's tree before anything is created, clone the fresh HEAD tree (cloned with no
  checkout, then checked, then checked out forced with the same progress and cancel; a
  collision fails the clone and cleans up), and submodule update the target commit's tree (an
  initialized submodule before libgit2 updates it; a fresh one cloned without checkout, then
  checked, then checked out). `tests/case_insensitive_links.rs` re-runs every operation on a
  case-insensitive file system in CI (self-skipping elsewhere) and asserts nothing outside the
  repository changes; its raw libgit2 rows are a canary for a libgit2 upgrade.
- Residual risk: a concurrent local process can swap a link in between `clear_hazards` (or a
  refusal check: `scan`, `refuse_linked_folders`, `has_symlinked_ancestor` in stage/stash,
  `refuse_linked_path`) and libgit2's own write; libgit2 has no `openat`-style checkout. The
  same threat class as #138 (a local writer racing the app). Browsitory's own raw reads, writes
  and deletes in the work tree all go through the walker (above); what is left outside it is
  stat-only checks (the refusal checks listed here, undo's `symlink_metadata` / `read_dir`
  checks) and the external diff/merge tool, which writes `MERGED` by path itself, as git's
  `mergetool` does.
- Residual risk, hardlinks: a tracked file that is a hardlink to a file outside the repository
  is written in place by libgit2's checkouts and by `fs::write` (the shared inode changes, so
  the outside name sees the new content). A hardlink is indistinguishable from a regular file by
  path; it needs the user (or a local process) to have created it on the same filesystem, and
  git behaves the same way.

## Image preview

- `git_core::diff::file_sides(repo, &DiffSource, path, cap)` returns both sides of a diff as
  bytes for the same `DiffSource` 4f's diff tool uses (index ↔ work tree, HEAD ↔ index, first
  parent ↔ commit, `from` ↔ `to` or the work tree), so it covers the working tree, commits,
  stashes and compares. Each side is capped at `MAX_PREVIEW_BYTES` (20 MB), judged from the
  object header (`odb.read_header`) or file metadata before anything is read, and the work tree
  read is bounded too. A work tree side never follows a symlink (the file itself, or any folder
  on the way, even one swapped in during the read: `worktree_guard::open_beneath`), and paths
  must be relative without `.`/`..`. A staged rename's HEAD side is read from
  `DiffSource::Staged { old_path }` (`StatusEntry.old_path`).
- Bytes cross IPC as base64 in JSON (`repo_service::base64`, a hand-rolled RFC 4648 encoder, no
  dependency) with the image type `git_core::diff::sniff_image_mime` read from the magic bytes
  (PNG, JPEG, GIF, WebP, SVG root element). The frontend only picks *candidates* by extension;
  it renders only a sniffed type from a fixed allowlist, as an `<img src="data:…">`, never inline
  SVG, `<object>` or `<iframe>`, so SVG script never runs. The VSCode webview CSP allows
  `img-src data:` for this and nothing else.

## External tools

- Each host launches tools its own way. The desktop app spawns programs from Rust
  (`commands/tools.rs`). The VSCode extension host answers the five launch methods with VSCode
  APIs (`vscode.open`, `vscode.diff`, `_open.mergeEditor`, `createTerminal`, `revealFileInOS`),
  and they never reach the sidecar.
- `git-core::process` (shared with hooks) splits editor/terminal command lines itself (POSIX or
  `CommandLineToArgvW` rules, no expansion) and never uses a shell for them; `{file}`, `{line}`
  and `{dir}` are replaced inside each word after splitting. Programs are resolved to absolute
  paths through absolute `PATH` entries only, so nothing in the work tree or the current
  directory can shadow them, and every path handed to a program is absolute (it can't be read
  as an option).
- Paths from IPC go through `launch::resolve_in_workdir` (no empty, absolute, `.`/`..`, root
  or prefix components; no `:` on Windows), and tool side paths are refused when a directory on
  the way is a symlink out of the work tree.
- With no editor configured, `launch::prepare_editor` refuses files the OS would run rather than
  open (by extension per OS, execute bit, `#!`/ELF/PE/Mach-O magic, and the same checks on a
  symlink's target) before calling `tauri_plugin_opener::open_path`.
- `git-core::tools` resolves `diff.tool`/`merge.tool` from **trusted config levels only**
  (system, XDG, global): a repository's own `.git/config` (and its includes) can neither pick a
  tool nor change its `cmd`, `path` or `trustExitCode`, and is reported as ignored. This is
  deliberately stricter than hooks (`git-core::hooks`), which keep git's behaviour and honour
  a repository-level `core.hooksPath`: running a repository's hooks is part of what git does on
  every commit, while a click on "Open in diff tool" must never run something the repository
  chose. A
  `difftool.<name>.cmd`/`mergetool.<name>.cmd` runs as `sh -c` with `LOCAL`/`REMOTE`/`MERGED`/
  `BASE` as environment variables, exactly as git runs it, so no path is ever spliced into
  shell text. On Windows, `sh` is Git for Windows' `sh.exe`; without it, the cmd is split and
  expanded directly. Presets are argv templates run without a shell.
- The repo worker only prepares a tool launch: it resolves config and writes the
  `LOCAL`/`REMOTE`/`BASE` files into a fresh `browsitory-tool-*` directory in the temp dir
  (random name, 0700, files created `O_EXCL` with sanitized names). The Tauri command spawns the
  tool and, for the merge tool, waits on `spawn_blocking`, so a tool window that stays open for
  hours never blocks the repo's command queue. Stale directories are swept after 24 hours.
- VSCode reads historical file contents through the `browsitory-file:` content provider, which
  calls the sidecar's internal `read_file_version`. No temp files are needed there. Before
  opening VSCode's merge editor the host calls the internal `check_merge_target`, the same
  guards `prepare_merge_tool` applies (text conflict, no symlink/submodule, work tree file
  inside the tree).
- Side files and `read_file_version` return raw blob bytes: git2 0.21 exposes neither
  `git_blob_filter` nor libgit2's filter lists, so CRLF/`ident`/filter-driver conversion is not
  applied (documented for users in USER_GUIDE).
- Runnable-file refusal checks one extension list on every OS (Wine runs Windows installers and
  shortcuts from `xdg-open`/`open`) plus content sniffing (`#!`, ELF, PE, Mach-O, OLE except
  old Office documents, `[Desktop Entry]`).
- Work tree paths come from `WorkerHandle::workdir()` (captured at spawn), never from the raw
  repo path, which may be a subdirectory (`Repository::discover`).

## Fetch all

- `git-core::remote::fetch_all` fetches every remote in `repo.remotes()` order under one
  operation id (`TransferOperation::FetchAll`), building each remote's credential provider
  through a `credentials_for` callback. A failing remote is recorded, not fatal; a cancel stops
  the loop.
- The worker turns each failure into `TransferEvent::RemoteFailed` (wire: phase `RemoteFailed`
  plus `remoteName`) before `Completed`, which fails only when no remote succeeded.
- `prune: Option<bool>`: `None` is `FetchPrune::Unspecified`, so libgit2 applies
  `remote.<name>.prune` then `fetch.prune` itself. The UI's Prune toggle is a per-repo
  `localStorage` preference; unset, every fetch sends `null`.
- Push and force push of the current branch go where `git-core::remote::push_target` resolves
  from `push.default` (unset = `simple`): `upstream`/`tracking` push to the upstream's branch,
  `current` to the local name, `simple`/`matching` only when the names match, `nothing` never.
  An undecided target is `PushTarget::NeedsChoice`: the backend refuses before connecting and the
  UI opens "Push to…" instead (`UpstreamInfo.push_branch` = null). The force-push lease reads the
  same target's tracking ref, and the UI only offers "Force push (with lease)" after a rejected
  push whose target equals that resolution.

## Error handling

`git-core` functions return typed errors (`thiserror` enums per module: `RepoError`,
`StatusError`, ...). `Worker`/Tauri commands map these to `Result<T, String>` crossing the IPC
boundary (Tauri serializes `Err` as a rejected JS promise). `RepoClient` methods return
`Promise<T>` that reject with that message — no error is swallowed at the boundary.
The VSCode bridge additionally synthesizes `-32001` JSON-RPC errors for requests interrupted by
sidecar loss. Its separate `transportStatus` notification rejects any still-pending webview
promises and presents the diagnostic globally; it does not extend the transport-neutral
`RepoClient` interface.

## Testing strategy

- `git-core`/`config`: `cargo test`, real temp-dir repos/files, no mocks.
- `repo-service`: inline unit tests next to the code they cover (worker, credential store, and
  forge/PR API logic), including `crates/repo-service/src/worker/mod.rs`'s tests, which spawn a
  real worker thread against a real temp-dir repo — no mocks. It also owns the DTO wire-format
  contract: `crates/repo-service/src/lib.rs`'s `wire_format_tests` module pins the `StatusKind`
  and `DiffLineOrigin` `Debug` output against the matching unions in
  `frontend/src/ipc/RepoClient.ts`, a contract no other test covers.
- `tauri-app`: now a thin Tauri command adapter over `repo-service`, so it doesn't need
  delegation tests of its own. It keeps `crates/tauri-app/src/commands/mod.rs`'s inline tests
  for the DTO serialization it's still responsible for (e.g. camelCase field names on structs
  like `WorkspaceDto`/`OpenRepoEntryDto`).
- Hermetic git config: every Rust test binary (`git-core`, `repo-service`, `vscode-sidecar`,
  `tauri-app`) invokes `test_support::hermetic_git!()` (dev-only `crates/test-support`), which
  before `main` points libgit2's global/XDG/system search paths at an empty directory and sets
  `HOME`/`XDG_CONFIG_HOME`/`GIT_CONFIG_NOSYSTEM`/`GIT_CONFIG_GLOBAL` for spawned `git`, hooks
  and the sidecar binary, so the developer's or CI runner's own git config never leaks in. A
  test that needs a global config builds its own `git2::Config` or passes `HOME` to a child —
  never moves the process-wide search path. New test binaries must include the macro (a
  `tests/common` module counts).
- `frontend`: Vitest + Testing Library, mocking `RepoClient` (a real interface seam).
- `extension`: Vitest with fake child-process and VSCode boundaries for JSON-RPC framing,
  native-method routing, process-loss recovery, webview security, and deterministic disposal.
- E2E: `tauri-driver` + WebdriverIO (`e2e/`) driving
  the built `tauri-app` binary as a black box — `cargo build --workspace --features
  tauri-app/custom-protocol` (the `custom-protocol` feature makes the binary load its embedded
  `frontend/dist` instead of the Vite dev server; plain `cargo build` stays in dev-server mode
  regardless of debug/release, so it can't be driven standalone). One flow per major feature
  area, added where a flow spans backend+frontend in a way unit tests can't catch; `App.tsx`
  reads a `VITE_E2E_REPO_PATH` build-time env var to auto-open a fixture repo, since
  `RepoPicker`'s native folder dialog can't be driven through WebDriver. (Not Playwright:
  Playwright drives browser engines it manages itself and can't attach to a native
  Tauri/webkit2gtk window; `tauri-driver` is Tauri's own WebDriver bridge, the
  actually-supported E2E path.) Both `e2e/package.json` and `frontend/package.json` pin
  `packageManager: "pnpm@9.15.9"` — a contributor running a different pnpm major without
  Corepack honoring this pin can break `e2e/`'s frozen-lockfile install.
- E2E for the VSCode extension (`extension/e2e/`, a standalone pnpm package, sibling to but not
  a workspace member of `extension/`): `@vscode/test-electron` launches a real Extension
  Development Host against the unpacked dev-mode extension and drives `vscode.*` commands
  directly (`browsitory.open`, staging, commit) via Mocha specs under `extension/e2e/src/specs/`,
  mirroring `e2e/`'s one-flow-per-feature-area black-box pattern (currently: open repo → stage a
  file → commit → see it in history). The webview panel itself isn't reachable through
  `vscode.*` APIs, so `extension/e2e/src/support/webviewPage.ts` attaches over a hand-rolled raw
  Chrome DevTools Protocol connection (Node's built-in `WebSocket`/`fetch`, no browser-automation
  library — an initial `playwright-core`-based attempt was fully removed after proving unable to
  reliably locate VSCode's nested webview content frame, which sits behind an outer sandbox
  wrapper frame with no DOM of its own; see `docs/tasks/phase-6/d-02-vscode-e2e-harness.md` for
  the two bugs that shape found and fixed). `extension/e2e/src/runTests.ts` is the outer harness:
  it sets `GSETTINGS_BACKEND=memory`/`GIO_USE_PROXY_RESOLVER=dummy` and best-effort `pkill`s any
  leftover `dconf watch` process before launch, then picks a dynamically free CDP port, to
  mitigate a GTK/GLib proxy-resolver helper that can otherwise squat the debug port. CI runs it
  under `xvfb-run` as its own `e2e-vscode` job (`.github/workflows/ci.yml`), parallel to `e2e`.

### Credential release acceptance

Run this manual check only with disposable repositories, accounts, tokens, and SSH keys. Never
put a token in a remote URL, Git config, issue, screenshot, or terminal transcript.

1. Create a disposable HTTPS remote and a local repository, then add the remote in Browsitory.
   Open its credentials, select **HTTPS token**, enter the disposable username and test token,
   and save. Fetch twice; both fetches must succeed without requesting credentials again.
2. Forget the HTTPS credential and fetch once more. The app must say **Save an HTTPS token for
   this remote before retrying.** It must not show a libgit2 callback error or the remote URL.
3. Create a separate disposable SSH remote, load its disposable private key into the local SSH
   agent, select **SSH agent** in that remote's credentials, then fetch and push. SSH-agent mode
   must show no token field and must not access the HTTPS credential store.
4. Inspect the local `.git/config`, the app's visible error, and transfer-progress UI after each
   step. Config may contain only `browsitory.remote.<name>.auth-mode` and, for HTTPS, the
   non-secret username. Confirm the test token is absent from config, progress, errors, and all
   visible UI before treating the release as accepted.

## Current scope

Full Git GUI feature set: repo view (log/diff/stage/commit), branch management, stash, merge
conflict resolution, interactive rebase, blame, multi-branch commit graph, push/pull/fetch with
progress, multi-remote and tag push, OS-keychain credentials, worktrees, submodules, reflog, and
GitHub/Bitbucket PR integration — all reachable identically from the Tauri desktop app and the
VSCode extension via `RepoClient`. Design system (tokens, layout primitives, iconography) is
rolled out across the core commit-review views; extending it to the rest is in progress.

For what shipped when, see `CHANGELOG.md`. For in-flight and planned work, see
`docs/superpowers/plans/` and `docs/tasks/`.
