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
  no TTY, and no console window on Windows. They block the repo's worker thread while pinentry
  waits.
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
