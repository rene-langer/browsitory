# Architecture

## Crate/package layout

```
browsitory/
├── crates/
│   ├── git-core/        # git2-based service layer, UI-agnostic, unit-tested headlessly
│   ├── config/          # repo registry + preferences: recent-repos list, backed by TOML
│   ├── repo-service/    # transport-agnostic worker threads, credentials, forge/PR API access,
│   │                    #   and the wire DTOs both transports send (`wire`)
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
everything else, as server-initiated notifications (`crates/vscode-sidecar/src/dispatch/remote.rs`'s
`spawn_progress_relay`) rather than request/response calls. Target-specific VSIX packages each
bundle the `extension/` host, the `frontend/dist-vscode` webview bundle, and exactly one matching
sidecar binary.

Dependency direction: `tauri-app` → `repo-service` ← `vscode-sidecar`, `repo-service` →
`git-core`, `config`, and `config` → `git-core` (for the limits both enforce, such as
`MAX_CONTEXT_LINES`). Neither transport depends on the other.

### Wire DTOs

The JSON shapes both transports send and receive are defined once, in `repo_service::wire`
(`crates/repo-service/src/wire.rs`): about 70 DTOs, their `From` impls to and from the
`git-core`, `config` and `repo-service` types, the transfer and hook event payloads
(`transfer_progress_dto`, `hook_progress_dto`), and the error envelope every failed request
answers (`ErrorDto`, see "Error handling"). The Tauri commands and the sidecar handlers
both map through these, so a mapping can't drift between the two. Only transport-specific types
stay in the transport crates:

- `tauri-app`: the clone and init types (`clone.rs`: `CloneRequestDto`, `HttpsInputDto`,
  `CloneOutcomeDto`, `CloneErrorDto`; the VSCode extension has no clone), `MergeToolOutcomeDto`
  (VSCode uses its own merge editor), the Tauri event names, and `list_open_repos`'s
  `(entries, activePath)` tuple.
- `vscode-sidecar`: `OpenReposDto` (`list_open_repos`'s object), `FileVersionDto` and
  `read_file_version`'s `{ text }` (extension host only), and every method's params struct.

A wire field that holds one of a fixed set of names carries the enum itself, never a string:
the `git-core` enums (`StatusKind`, `DiffLineOrigin`, `TransferOperation`, `TransferErrorKind`,
`OperationKind`, `TerminalRebaseKind`, `SequenceKind`, `RebaseDivergence`) and
`repo-service`'s `HookSource` derive `Serialize` themselves and go on the wire as their variant
names. Where the payload adds names of its own, `wire.rs` has a small enum
(`TransferProgressPhaseDto`: `TransferPhase` plus `Starting`, `RemoteFailed`, `Completed`,
`Failed`; `HookProgressPhaseDto`).

Each transport keeps its own serialization path: Tauri sends `serde_json::to_string` of the
value (fields in declaration order); the sidecar goes through `serde_json::Value` (fields
sorted). The wire snapshots in "Testing strategy" pin both.

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

**Host requests and notifications (#677, #678, #679, #759).** The extension host asks the
webview to open repositories, show a file's or lines' history, select a commit (from the Timeline
view, through `useGoTo` like Go to), or run a global shortcut with an id-less
`hostRequest` notification (`extension/src/hostRequests.ts`; `RepoClient.subscribeHostRequests`
on the frontend). It holds them until the webview sends `hostReady`, which `useHostRequests`
causes once the restored tabs are open, so a request's tab isn't replaced by the restore. The
webview also sends `webviewFocus`, which sets the `browsitory.webviewFocused` context key. Both
are handled in `extension.ts` and never reach the bridge or the sidecar. VSCode hands every key
pressed in a webview to its own keybindings, even when the page prevents the default, so the
global shortcuts VSCode binds itself (`vscodeKeybinding` in `frontend/src/lib/keybindings.ts`)
are contributed as hidden `browsitory.shortcut.<id>` commands with keybindings for
`activeWebviewPanelId == 'browsitory' && browsitory.webviewFocused`. The webview leaves those
keys alone in VSCode (`data-host="vscode"`) and runs the shortcut when the request arrives.
`frontend/src/lib/vscodeKeybindings.ts` derives the contributions from `GLOBAL_SHORTCUTS`, and
its test keeps `extension/package.json` equal to them. A `WebviewPanelSerializer` brings the
panel back after a reload.

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
`AGENTS.md`'s License policy section. Because the tests and the "git2 API gotchas" below
characterize libgit2 behaviour, Renovate holds
`git2`/`libgit2-sys` updates for Dependency Dashboard approval (`renovate.json`; see
`AGENTS.md`'s Dependency updates section) — re-check those notes when approving one.

## The `RepoClient` IPC boundary

`frontend/src/ipc/RepoClient.ts` defines the interface every UI component depends on — see that
file directly for the current method list rather than a copy here, which would just go stale.

`frontend/src/ipc/tauriRepoClient.ts` implements it over `@tauri-apps/api`'s `invoke()`. This
is the *only* file allowed to import `@tauri-apps/api` — every other frontend file receives a
`RepoClient` as a prop/context value, so it can't accidentally couple to Tauri.
`frontend/src/ipc/vscodeRepoClient.ts` implements the same interface over webview `postMessage`.
Both build most of it from `createRepoClient(call)` in `frontend/src/ipc/createRepoClient.ts`
(#504), which holds every method the transports send alike (backend name and params) once; a
transport passes its call function and adds only the members that really differ, listed in
`TransportSpecific` there (clone and init, `listOpenRepos`'s reply shape, the event
subscriptions, `logFrontendError`). What a transport does around a call (logging, error mapping,
the desktop's wait for its event listeners before a command that emits transfer or hook events)
lives in its call function, not in a per-method override. A new operation is one line in
`createRepoClient.ts`.
`frontend/src/ipc/transportStatus.ts` is a transport-neutral lifecycle seam that lets `App`
reuse its global `InlineError` path without importing VSCode APIs. Tauri registers no lifecycle
status listener and retains its existing updater/error behavior.

The rule is enforced mechanically, not just by review: `frontend/eslint.config.js` has a
`no-restricted-imports` override banning `@tauri-apps/*` imports from
`frontend/src/components/**` and `frontend/src/state/**`, so a violation fails `pnpm lint`
(and CI).

## Frontend state

Each open repository tab is one `RepoWorkspace` (`frontend/src/RepoWorkspace.tsx`). Its
repository data and the actions on it come from `useAppState` (`frontend/src/state/useAppState.ts`,
built from one action slice per domain); the views read the actions they need from
`WorkspaceActionsContext` (`frontend/src/state/useWorkspaceActions.ts`, #491) and get their data
as props. The workspace renders its dialogs in `WorkspaceDialogs`, outside the sidebar: a
collapsed sidebar stays mounted but `hidden`, and a modal opened inside it would be invisible yet
leave the page inert.

### Kept tabs and changes from outside the app

`App` keeps the workspaces of the last `MOUNTED_TABS` (4) shown tabs mounted
(`frontend/src/state/useMountedTabs.ts`, #642); an inactive one is hidden with `display: none`.
Everything in a workspace that listens globally (shortcuts, palette and drop requests, the focus
checks below, automatic fetch) is gated by its `active` prop, so a hidden workspace never reacts.
A hidden element has no width, so `useNarrowLayout` and `SplitView` ignore a zero width instead of
collapsing the sidebar, and `useScrollMemory` puts the lists' scroll positions back when the tab is
shown again. An older tab unmounts (PERF-001's reason: one visible view shouldn't cost N tabs' DOM
and IPC) and reads the repository again when it mounts. The tab strip activates manually: arrow
keys only move the focus.

Changes made outside the app (#650) are found by comparing a change stamp:
`git_core::change_stamp` hashes what the app shows (the work tree as `status` reads it,
with each changed file's size and modification time; the index entries without their stat data;
HEAD, references, the stash reflog, the configuration, the files of an operation in progress and
each linked worktree's HEAD). Ignored files and the rest of the git folder (objects, reflogs,
`FETCH_HEAD`) don't count. `get_change_stamp` is an ordinary worker read on both transports.
The stamp has two parts (`ChangeStamp`, `{ refs, workTree }`, #685): `refs` covers HEAD, the
references, the stash list, the configuration, the operation files and the linked worktrees;
`workTree` the index and the work tree. `refresh()` keeps the history reads (the graph and its
expanded merges, branches with ahead/behind, tags, the current and every remote's upstreams) when
their key, the `refs` part plus the graph selection, "Collapse merges" and the limit, equals the
one they were read with, so staging, unstaging, discarding or editing a file re-reads only the
status, the stashes, the worktree, submodule and remote lists, HEAD and undo state. The Refresh
command (`refresh({ full: true })`), a failed refresh and a missing stamp read everything. Refreshes
are coalesced per tab (#222): one runs and at most one waits; a call while one runs shares the
waiting one, which starts after the running one ends, so it still sees the caller's change.
`useAppState.refresh()` reads the stamp before its other reads (a change landing during them then
differs), and `checkForChanges()` refreshes only when the current stamp differs from that one, so
the app's own action, whose refresh already read the new stamp, causes no second refresh.
`useExternalChanges` calls it while the tab is shown, the app visible and nothing of the tab
running: every 3 s, stretched to ten times the last check's duration (at most 60 s), and at once
on window focus, on becoming visible again (the VSCode panel's visibility included) and when the
tab is shown. At most one check runs at a time, and none while a refresh runs.

A file system watcher says when to check (#756). `repo_service::watch::RepoWatchers` keeps one
`RepoWatcher` (the `notify` crate, a licence exception, see `docs/LICENSE_COMPLIANCE.md`) per path
the frontend watches; `useExternalChanges` calls `watchRepo` when its workspace mounts and
`unwatchRepo` when it unmounts (at most the 4 kept tabs are watched), and `closeRepo` stops it
too. The watcher follows what the stamp reads: the work tree minus what git ignores, and the git
folder's (and, for a linked worktree, the common folder's) HEAD, index, `refs/`, `packed-refs`,
configuration, stash reflog, `info/exclude` and the files of an operation in progress; objects,
other reflogs and `*.lock` files are left out (`watch/paths.rs`), and so are mere reads (inotify
reports opening a file: the app's own stamp check would otherwise report itself). Events are
debounced (300 ms quiet, at most 2 s) into one `Changed`, sent as the `repo-watch` Tauri event or
the `repoWatch` sidecar notification (`RepoWatchDto { repoPath, kind }`), which the extension host
relays to the webview like the other notifications. A report for the shown tab checks at once; one
that arrives during a check queues one more check; one for a hidden tab is dropped (showing it
checks anyway). While watched, the poll above slows to every 30 s as a fallback (some network file
systems report nothing).

On Linux, inotify needs a watch per folder and a user's watches are shared with every program
(VS Code's own watcher included), so the watcher walks the tree itself and adds one watch per
folder git doesn't ignore, plus the git folders that hold the files above (`.git`, `refs/**`,
`logs`, `logs/refs`, `worktrees/*`, ...), never `objects/`. New folders get watches as they appear,
moved or deleted ones lose them, and a changed `.gitignore` or `info/exclude` walks the tree again.
More than 20 000 work tree folders (`MAX_FOLDERS`) is not watched. macOS (FSEvents) and Windows
(ReadDirectoryChangesW) use one recursive watch per root. When a watcher can't start (`watchRepo`
answers `false`) or stops (the watch limit was reached: `Stopped`), the reason goes to stderr (the
desktop log, the extension's output channel) and the frontend polls every 3 s again.

### Who owns a dialog's open state

Every dialog has exactly one owner, chosen by who opens and closes it (#493):

- **`AppState.dialog`**, when more than one place opens it (a menu and the command palette) or a
  `useAppState` action opens or closes it itself (a push that needs a target opens "Push to…", a
  stash that landed closes the stash dialog). It is one `WorkspaceDialog | null` union
  (`frontend/src/state/workspaceDialog.ts`): push, force push, delete remote branch, create tag,
  delete remote tag, a remote's edit / credentials / remove, merge, reword and stash. There is one
  `openDialog(dialog)` and one `closeDialog(kind?)`. Only one is open at a time: opening one
  replaces the open one, which is what lets a single field hold them all. An action that finishes
  closes only its own kind (`withoutDialog`), never a dialog opened meanwhile. Being in
  `AppState`, the dialog survives a `refresh()` (the same object, so identity-keyed state such as
  `RemoteDialogs`' snapshot keeps working).
- **`RepoWorkspace` (or one of its hooks)**, when the palette and a view open it but nothing in
  `useAppState` does: the discard confirmations (`discardRequest`), the cherry-pick/revert parent
  picker and reset dialog (`commitDialog`, which carries the element to return focus to), the
  terminal-abort confirmation, the palette and the shortcut sheet.
- **The component itself**, when only that component opens it: `BranchTree`'s force delete, Set
  upstream dialog, clear-upstream confirmation and inline rename, for example.

A component whose dialog is owned higher up is always controlled: it takes the state and its
setter (or just an open callback) as required props and keeps no local copy. There is no "local
state unless a callback is passed" mode, so a test exercises the same state flow production uses;
tests that render such a component alone hold the state in a small wrapper
(`frontend/src/test/BranchTreeWithDialogs.tsx` for `BranchTree` and its remote dialogs, the
`DiffPane` wrapper in `DiffPane.test.tsx` for the discard confirmations).

Two kinds of UI state stay out of `dialog` on purpose. The sidebar's inline New branch and Add
remote forms (`createBranchDraft`, `addRemoteDraftOpen`) are not modal: they can be open together
and next to a dialog, so each keeps its own `AppState` field (the palette opens them too). And a
prompt that an operation's state raises (the hook failure dialog, a diverged pull's choice)
stays with that state. A running fetch, pull or push is no dialog at all: the toolbar shows it
(`RepoActivity`, from `transfer` and `transferSubject`), and the window stays usable (#659); a
tab that unmounts mid-transfer hands it to the next mount of its workspace
(`useRemoteTransferActions`). The rebase planner (`rebaseOnto`/`rebasePreset`) predates
the rule and still has its own fields.

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
  untracked-first revert order stays as a second safety net. **Exception (#207, deliberate,
  differs from git):** a folder that replaced a regular file, with untracked files included,
  is stashed when it holds nothing but untracked files and links (`stashable_folder`: no
  ignored entry, no `.git`, nothing tracked): the folder's files join the untracked commit, the
  file's deletion goes into the worktree commit, and the revert removes the files (pruning the
  folder) before it checks the file out. git's `stash push -u -- d` records only `d/x` (its
  worktree tree is HEAD's, the deletion is lost), puts `d` back, and its own pop then fails
  ("cannot create directory at 'd': File exists"); with an ignored file in `d/` it saves the
  stash and stops half-way ("could not write file 'd'… directory not empty") with the index
  changed. Browsitory's stash pops through `apply_in_order` (below). A folder that replaced a
  symlink or submodule stays refused.
- Known libgit2 stash-apply quirk, not handled: an apply can leave a `<path>~Updated upstream`
  file next to a conflicted path of the tracked changes. For the untracked files it can't happen:
  stashes whose untracked files meet a tracked file or folder take Browsitory's own apply
  (`apply_in_order`, below).
- Entries in the way (#186, #208, #211). libgit2's whole stash resets with
  `GIT_CHECKOUT_FORCE` (plus `REMOVE_UNTRACKED` with untracked files, stash.c
  `reset_index_and_workdir`). Its checkout removes a non-folder where the restored tree has a
  folder (`checkout_action_with_wd_blocker`) and a folder where it has a file
  (`checkout_action_with_wd_dir`, removed with everything in it, a nested `.git` included),
  ignored or not; `wd_item_is_removable`, which protects nested repositories, only applies to
  worktree-only items. git's `reset --hard` deletes the same entries. Before a whole stash,
  `stash::entry_in_the_way` finds the first entry the reset would delete because it sits where
  the restored tree has a folder, where it has a file (folders only), or below a path it has as a
  file, and `save_stash` refuses: an untracked file with `UntrackedInTheWay`, only without
  "Include untracked files" (with them it is in the stash); an ignored file or folder with
  `IgnoredInTheWay` and an untracked nested repository (a folder with a `.git`) with
  `RepositoryInTheWay`, in both modes, since neither is ever stashed. Ignored folders are
  checked whole (`recurse_ignored_dirs(false)`), so unrelated ones such as `target/` cost one
  status entry. The restored tree is HEAD's; with keep-index the index's too: libgit2 1.9.6
  resets once, to the index commit (stash.c l.751), but `git_checkout_tree` keeps HEAD as the
  checkout baseline, so an entry in the way of a path HEAD has and the index deletes is still
  removed (git's keep-index stash deletes it too). The path-limited stash checks only its own
  tracked paths (`UntrackedInTheWay`, `FolderIsNowAFile`, `DirectoryReplacedNonDirectory`).
  Under `core.ignorecase` paths compare case-insensitively (`docs` over `Docs/f`). An untracked
  symlink is skipped there: the hazard route refuses it as `BeyondSymlink`. A file removed from
  the index and edited (`git rm --cached d`, status `INDEX_DELETED | WT_NEW`) is not refused.
- A file removed from the index but not ignored (`git rm -r --cached d`, then edited). libgit2
  1.9.6 builds the stash's worktree tree from an index-to-worktree diff without
  `RECURSE_UNTRACKED_DIRS` (stash.c l.391). For `d/f` it reports `d/` untracked and `d/f`
  deleted, and `stash_delta_merge` (l.360) only joins the two for the same path, so the edit
  was lost: Pop deleted `d/f`, and with keep-index the reset deleted it at once. With untracked
  files `build_untracked_tree` (l.292) does recurse, so the edit landed in the untracked commit
  (a top-level `f` in both trees), and the apply merged it against HEAD's version: conflict
  markers and `WouldOverwrite`. `save_stash` therefore sends a whole stash with any such
  non-folder through the path-limited stash. The check (`edit_removed_from_index`) costs a
  HEAD-to-index diff plus one no-follow stat and one ignore check per staged deletion, with no
  worktree scan. The path-limited stash records the file once, as a tracked change: the edit in
  the worktree tree, the index commit without it, never in the untracked commit. Then it puts
  HEAD's version back in the worktree and HEAD's entry back in the index (with keep-index the
  index has no entry, so the file is deleted, as with git). Pop brings the edit back; with
  "Restore staged changes" the removal from the index too (`git stash pop --index`). Deliberate
  difference: git's `stash -u` records the edit in both trees and its own pop then fails;
  Browsitory's stash pops in both. Known limitation: the path-limited stash's own refusals
  apply on this route. For example, a file removed from the index plus an unrelated tracked
  file `t` that is now a folder gets `DirectoryReplacedNonDirectory` for `t`, which libgit2's
  whole stash would have handled. A folder that replaced a regular file is no longer one of them
  when it holds only untracked files (#207, above).
- The path-limited stash's revert (`git_checkout_tree` with the reverted paths) updates the
  index only for the files it writes. A path removed from the index whose worktree file already
  matches HEAD (an unedited `git rm --cached f`, or the unedited `d/g` of `git rm -r --cached
  d`) stayed removed, so Pop refused with `StagedChanges` (#248). After the checkout it now puts
  HEAD's entry back for every reverted path HEAD has as a file or link. HEAD may have a folder
  or submodule at a path whose index entry was a staged file; that path is skipped. It does
  this entry by entry, not with `reset_default`, which takes pathspecs (`[ab]` would also
  unstage `a` and `b`). A replaced entry keeps its assume-unchanged and skip-worktree flags;
  intent-to-add is dropped.
- `save_stash` refuses any stash while the index has conflicts (`ConflictsBeforeStash`),
  before choosing a route. libgit2's whole stash and git refuse too ("cannot create a tree from
  a not fully merged index"). The path-limited stash (Stash file…, and Stash all on the hazard
  or #247 routes) would otherwise record the file with its markers and put HEAD's entry back in
  the index. The merge would stay in progress, so the next commit would drop the other side.
  The index is re-read first (`read(false)`), so a conflict from a terminal merge counts.
- Repositories and ignored files in an untracked folder. With untracked files, libgit2's reset
  (`REMOVE_UNTRACKED`) removes an untracked folder whole (`checkout_action_wd_only`);
  `wd_item_is_removable` only spares a folder that itself holds a `.git`, so `vendor/lib/.git`
  below the untracked `vendor/` was deleted with its history, and so were ignored files such as
  `vendor/x.o`. git's `stash -u` keeps both (`git clean -d` skips nested repositories and
  ignored files). `save_stash` sends such a stash (Include untracked on, an untracked repository
  or an ignored entry below a folder that the index or HEAD has no path in) through the
  path-limited stash, as it does for symlink hazards (`kept_entry_in_untracked_folder`): it
  stashes the same files git does and deletes only those. An untracked folder holding only
  ignored entries doesn't count: libgit2's iterator reports it as ignored and leaves it. Known
  limitation: on that route the path-limited stash's own refusals apply (a folder that replaced
  a tracked file is stashed only when it holds just untracked files, #207). Before, the stash
  succeeded and deleted the nested repository or ignored files.
- An ignored file that is also `INDEX_DELETED` (`git rm --cached .env`, then ignored, then
  edited) is not in libgit2's stash (its worktree commit skips ignored files; git records it),
  and the reset writes HEAD's version over it (with keep-index it deletes it); Pop then deletes
  it. For a folder ignored as a whole (`git rm -r --cached build`, `build/` ignored) status
  (`recurse_ignored_dirs(false)`) flags only the folder entry `IGNORED`; its files show up as
  plain `INDEX_DELETED`. `entry_in_the_way` therefore refuses with
  `StashError::IgnoredOverwritten` for any `INDEX_DELETED` entry without `WT_NEW` whose worktree
  path is a non-folder (one no-follow stat per staged deletion) and a restored file, in both
  modes. It names the ignored folder when the file is inside one. Stash file… refuses its
  selected staged deletions by the same rule, on the same whole-worktree status
  (`WorktreeScan::ignored_overwritten`, #259); the path-limited status can't tell, since with
  untracked files left out it never reports `WT_NEW`. Stashing such a file would put HEAD's
  entry back in the index, so a pop without "Restore staged changes" would leave it tracked
  again with its local edit unstaged; `git stash push -- .env` refuses too ("did not match any
  file(s) known to git").
- A gitlink in the index that HEAD doesn't have (`git add lib` on a nested repository, or `git
  submodule add` before the commit) makes libgit2 1.9.6's stash fail with "cannot create blob
  from '…/lib': it is a directory" while the folder is there; without the folder the stash
  succeeds but leaves the gitlink staged, so Pop refuses. `save_stash` checks the index first
  (`refuse_staged_repository`) and refuses a whole stash, or Stash file… on that path, with
  `StashError::NewSubmodule` (a `submodule.*.path` in `.gitmodules` names it),
  `EmbeddedRepository` (the folder holds a `.git`) or `NewGitlink`; nothing is written. git
  stashes the gitlink and keeps the folder (#244). A gitlink HEAD already has, changed or not,
  stashes fine: the stash records the outer gitlink only and never moves the nested
  repository's HEAD or touches its files (git doesn't recurse into it either).
- Stash file… on a staged rename's new path also stashes the old path's deletion
  (`with_rename_partners`, renames detected HEAD to index as in `status::status`; #152). If the
  worktree has something at the old path again, picking only the new path refuses
  (`RenameSourceRecreated`) instead of stashing and overwriting it; picking the old path
  stashes both. Stash all on the path-limited route selects every change, so it never refuses
  this: the new file at the old path is stashed like any edit to a file removed from the index
  and comes back on Pop. Submodule renames aren't paired (the new gitlink is refused first), nor
  are paths that aren't UTF-8 (skipped everywhere in the path-limited stash).
- Stash file… refuses a case-only rename (`git mv README readme`; `CaseOnlyRename`,
  `refuse_case_only_rename`) where names fold. libgit2's ignore-case index (`core.ignorecase`)
  finds one name as the other, so the stash's index tree held both names and the revert changed
  nothing. "Case-only" means what libgit2 compares as one entry (`icase_index_key`):
  `strcasecmp`/`git__strcasecmp` and a `git__tolower` hash (index.c, index_map.c), ASCII letters
  only, byte by byte; on macOS and Windows also what the drive takes for one file (`stash_fold`,
  #389). Seen on CI with a rename made by `mv` (`git mv` refuses where the drive folds) and
  staged by libgit2: APFS folds `Straße`/`Strasse`, `Café`/`CAFÉ` and NFC/NFD spellings
  (`worktree_guard::fold_name`); NTFS only `Café`/`CAFÉ` (letter-by-letter simple upper case,
  `ntfs_fold`), not `ß`/`ss` or NFC/NFD. libgit2 kept the two names apart, so Stash all refused
  with a misleading `IgnoredOverwritten("Straße")` (the deleted old name found the file) and
  Stash file… with that or `RenameSourceRecreated`; now they get `CaseOnlyRename` like `README`
  → `readme`. NFC/NFD only matters with `core.precomposeunicode` off: on (git's and libgit2's
  default on macOS) both read the NFD name as NFC and see no rename. On Linux with
  `core.ignorecase` the drive and libgit2 keep all of these apart and they stash normally
  (`a_unicode_rename_the_drive_keeps_apart_stashes_normally`, which also covers `ß` and NFC/NFD
  on Windows). `git add -A` after such an `mv` keeps the old name (its lstat finds the file) and
  adds the new one, so the index holds two names of one file. Stash all handles that (libgit2's
  reset removes the new name before writing the old one; Pop restores both entries), but Stash
  file… on either name deleted the shared file until Pop, or wrote a stash that changed nothing
  and Pop refused, so it refuses with `CasePair` (below). The check runs before the
  ignored-file check, since the staged deletion `Old` can find a file at `Old` (the new `old` on a drive that folds names, or an ignored
  `Old` beside it) and would look like an ignored file removed from the index. "Where names
  fold" is `worktree_guard::try_folds_case`, as for every stash check (#337, #369): macOS, Windows, or
  `core.ignorecase`. The stash never probes the drive (`dir_folds_case`), so on a Linux
  casefold folder or a vfat mount without `core.ignorecase` the check doesn't run. Stash all
  refuses them too (#370; `refuse_staged_case_only_rename`, before `entry_in_the_way` and any
  write): the index has a path HEAD doesn't, and HEAD a path differing from it only in case
  (`stash_fold`) that the index doesn't. Through libgit2's stash, Linux with `core.ignorecase`
  wrote a stash whose index tree held both names and left the rename in place, and on macOS and Windows
  the deleted `Old` found the file `old` (`IgnoredOverwritten("Old")`, seen on CI). `git stash`
  stashes the rename and puts `Old` back on all three (on macOS its worktree tree holds both
  names); libgit2's stash can't, and the path-limited route refuses as
  Stash file… does, so refusing is the consistent fix.
- A case pair (`A` and `a`, committed on a case-sensitive drive, then `core.ignorecase` on; on
  macOS and Windows such a pair can't even be checked out) is no rename, but libgit2 1.9 can't
  stash around it either (#392). Its forced checkout of HEAD (stash.c
  `reset_index_and_workdir`, `GIT_CHECKOUT_FORCE`) keeps only one of the two in its ignore-case
  index, even with nothing to check out
  (`libgit2_forced_checkout_drops_one_of_a_case_pair_under_ignorecase` pins this): the other
  became a staged deletion. Whether its file was deleted too depended on the order the
  directory lists the two names in: the workdir iterator sorts with plain `strcasecmp` and no
  tie-break (`filesystem_iterator_entry_cmp_icase`, iterator.c), the tree iterator breaks ties
  case-sensitively, so on a tmpfs (newest first) or an ext4 htree (hash order) the walk paired
  the tree's `A` with the file `a` for some names and not others. Locally (tmpfs and ext4, all
  26 letter pairs, both write orders) the index lost one entry every time and the file only for
  some names, which is how the ubuntu-latest runner could delete `a` where a local run kept the
  file (the runner's report is #392). The path-limited
  stash's checkout of a pair member did the same (one entry dropped, sometimes the file
  deleted, Pop then refused with `StagedChanges`). git keeps both. `save_stash` therefore
  refuses with `StashError::CasePair` where names fold (`try_folds_case`), before anything is
  written: Stash all when HEAD or the index holds two paths `icase_index_key` takes for one
  (checked after the rename check, so a case-only rename keeps its `CaseOnlyRename`), Stash
  file… when it picks one of them (`refuse_case_only_rename` tells a pair, two names in HEAD or
  in the index alone, from a rename, one name in each; there by `stash_fold`, so on macOS and
  Windows also two names only the drive folds, see above). Stash file… on other paths keeps the
  pair, since its checkout is limited to the stashed paths. Only whole paths count: `D/f` and
  `d/g` are two entries to libgit2 and stash normally.
- libgit2's stash apply checks the untracked tree out *before* the tracked changes (git does it
  the other way round), and merges it against the **index** (`merge_index_and_tree(NULL,
  repo_index, untracked_tree)`, stash.c l.1117), which still has every tracked path the stash
  deletes. So two stash shapes can't be applied through it: an untracked file `d` with the
  deletion of the folder `d/` (#186; libgit2 reported success and left `d~Stashed changes`, a
  pop dropped the stash) and the mirror, an untracked `d/x` with the deletion of the file `d`
  (#206; checked out as `d~Updated upstream` with success). `apply_stash` detects them before
  anything is written (`untracked_file_over_folder`: an untracked file's path is a real folder
  and the stash recorded that swap, its base has the folder and its worktree tree doesn't;
  `untracked_below_tracked_file`: an untracked file has a proper prefix that is a stage-0 index
  path) and applies them itself in git's order (`apply_in_order`, #229). It redoes libgit2's
  merges with HEAD's tree for the index (both require them to match, else `StagedChanges`): the
  worktree tree onto HEAD against the stash's base, and the index to leave behind, the index tree
  with "Restore staged changes" (a conflict is `IndexConflict`) or else HEAD plus the stash's new
  files (`stage_new_files`). Then one safe checkout writes the merge and the untracked files
  together, labels "Updated upstream"/"Stashed changes"; libgit2's checkout removes before it
  writes, so the folder (or file) is gone before the file (or folder) appears. The index
  becomes the merge when it has conflicts, else the unstashed index, else stays; unchanged
  entries keep their stat cache. A checkout stopping part-way is rolled back (#321). Only these
  shapes take the route; every other stash keeps libgit2's apply (owner decision on #229).
  Refused before writing: an untracked file at or above a path the merge keeps
  (`FileReplacedFolder(path)`, HEAD changed a file of the folder since;
  `UntrackedBelowTrackedFile`, a file `d` committed after stashing or changed by HEAD since,
  reported also when the index differs from HEAD), a real folder at an untracked file's path
  that holds anything HEAD hasn't as a file (`UntrackedOverFolder`: an untracked, ignored or
  nested-repository entry; libgit2's safe checkout doesn't refuse that and fails with "Is a
  directory"), a file/folder collision in the merge (`FileFolderConflict`) and a case pair
  brought in. A local edit in the way is `WouldOverwrite`. Where names fold
  (`try_folds_case`), paths compare folded (`fold_name`, whole components), so the mirror shape
  over the file `D` applies too (#234). Deliberate difference: `git stash pop` fails on the
  mirror shape (it restores untracked files first); Browsitory applies it. A folder that appeared
  after stashing gets `UntrackedOverFolder` ("move the folder first"), as before.
- libgit2's stash apply fails with "odb: cannot read object: null OID cannot exist" for a stashed
  file whose directory is a symlink in HEAD; `apply_stash` maps it to
  `StashError::SymlinkReplacedByDirectory` (the stash is kept). A stashed untracked file below
  such a symlink is refused before that as `UntrackedBelowTrackedFile`.
- `Repository::stash_pop` drops the stash even when the apply left conflicts; Browsitory applies
  and drops itself. `stash_apply` refuses whenever the index differs from HEAD
  (`ErrorCode::Uncommitted`); `stash::apply_stash` maps that and the other refusals to
  user-facing `StashError` variants.
- `cleanup_state` (`git_repository_state_cleanup`) deletes every `state_files` entry:
  `MERGE_HEAD`, `MERGE_MODE`, `MERGE_MSG`, `REVERT_HEAD`, `CHERRY_PICK_HEAD`, `BISECT_LOG`,
  `rebase-merge/`, `rebase-apply/` and `sequencer/`. The last four belong to a `git bisect`,
  `git rebase` or `git cherry-pick A B C` running in a terminal, so `git-core/src` never calls
  it (#249; the test `no_cleanup_state_in_git_core_src` in `tests/bisect.rs` keeps it that way).
  An operation's own files are removed by `commit::clear_operation_files` (`MERGE_HEAD`,
  `MERGE_MODE`, `MERGE_MSG`, `AUTO_MERGE`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`; missing files are
  fine, every file is tried, the first error is returned). libgit2's mixed and hard `git_reset`
  also end with `cleanup_state`, so `reset::reset_to` does those two itself (forced checkout for
  hard, move HEAD with git's "reset: moving to <id>" reflog message, index from the target
  tree, `clear_operation_files`); a soft reset still uses `git_reset`, which cleans nothing.
- `cleanup_state` does not remove `SQUASH_MSG` and neither does `clear_operation_files`;
  `merge::clear_squash_message` does, after a commit, an abort, a reset or an undo.
- During a terminal `git rebase` or `git am` libgit2 reports `RebaseMerge`/`RebaseInteractive`/
  `Rebase`/`ApplyMailbox`/`ApplyMailboxOrRebase`, whatever else is paused (it checks the rebase
  folders first), so `HeadInfo.operation` is `None` even while git's `MERGE_MSG` for a
  conflicted step exists. `repo::terminal_rebase` reads git's folders itself (see "Terminal
  rebases").
- `repo.state()` checks the rebase folders, then `MERGE_HEAD`, `REVERT_HEAD` (+
  `sequencer/todo` → `RevertSequence`), `CHERRY_PICK_HEAD` (+ todo → `CherryPickSequence`), and
  only then `BISECT_LOG` → `Bisect`. So a bisect is hidden by any other paused operation, and a
  terminal sequence paused between steps (`sequencer/todo` without a pick head) is `Clean`
  (`pick::terminal_sequence` reads the sequencer files instead, see "Cherry-pick sequences").
  `repo::no_operation` (`Clean | Bisect`) is the "nothing in progress" check everywhere: git
  allows picks, reverts, rebases, reset, amend and commits during a bisect (#250). A merge or
  pick paused during a bisect still reads as that operation. `repo::head_info` reports the
  bisect separately (`HeadInfo.bisect`, from `BISECT_LOG` and `BISECT_START` in `repo.path()`,
  so per worktree).
- `git_merge` ignores the merge analysis: called when a fast-forward is possible it still writes
  `MERGE_HEAD`, which is what `FastForward::Never` relies on.
- **libgit2 misses a file/folder conflict when a name sorts between (#410).** libgit2 1.9 finds
  a file/folder conflict only when the file and the folder's entries are neighbours in sorted
  order. `c` < `c.txt` < `c/g` (`.` sorts before `/`): merging a side that deletes `c.txt` and
  adds `c/g` into a HEAD that added the file `c` gives a merged index with both `c` and `c/g` at
  stage 0 and no conflict (without `c.txt`, `ab` against `ab/g`, it records the conflict `ab`
  and checks the file out as `ab~HEAD`). git reports "CONFLICT (file/directory)", merges `c/g`
  and leaves HEAD's file as the conflict `c~HEAD`. With libgit2 a cherry-pick, revert or rebase
  step committed a tree without the file `c` while the work tree kept `c` and never got `c/g`;
  a merge (squash, a diverged pull's merge) and a stash apply stopped part-way with "failed to
  make directory 'c': directory exists". Every operation that writes a libgit2 merge checks its
  in-memory merge (`merge_commits`, `cherrypick_commit`, `revert_commit`; for a stash
  `merge_trees` redoing `git_stash_apply`'s merges against HEAD's tree) before anything is
  written. `file_folder::find_file_folder_conflict` finds an entry `p` while another entry lives
  below `p/`, where `p` is a stage-0 entry or a conflict entry without a stage 2 (see the file
  coming in over a folder, below); libgit2's own conflict entry `p` with HEAD's side (stage 2)
  next to `p/…` is left alone.
  Each operation then produces git's outcome itself (#416), without libgit2's
  `merge`/`cherrypick`/`revert`/`stash_apply`, whose checkout of such an index fails.
  `file_folder::check_out_as_conflicts` rewrites the merged index (`record_as_conflicts`: the
  stage-0 `c` becomes `c~<our label>` at stage 2, "added by us", or `c~<their label>` at stage 3
  when the file comes from the other side; a `/` in a label becomes `_` and a taken name gets
  `_0`..., as git does), makes it the repository's index and checks it out safely (`c/g` is
  written, HEAD's file moves to `c~HEAD`), rolled back by `partial_checkout` when it stops
  part-way (#321/#329). The conflict is a stage-2-only (or stage-3-only) entry, "added by
  us/them" like git's own. git's labels: `HEAD` for ours; the branch for a merge,
  `<short id> (<subject>)` for a cherry-pick or rebase step, `parent of <short id> (<subject>)`
  for a revert; `Updated upstream`/`Stashed changes` for a stash.
  - **A file coming in over HEAD's folder (#607)**: here libgit2 does find the conflict, but
    records the incoming file only as a conflict entry at `c` (stage 3, "added by them"; stages 1
    and 3, "deleted by us", when HEAD deleted a file the other side changed) next to the folder's
    entries at stage 0. With a name sorting between them (`c.txt`), its checkout then writes the
    file over the folder and fails on Linux ("could not open 'c' for writing: Is a directory",
    while on macOS libgit2's own checkout got through in CI); without
    one it writes the file as `c~<its own label>` but leaves the conflict at `c`. git moves the
    conflict stages to `c~<their label>`. `record_as_conflicts` does the same: the entry keeps
    its stages and moves to `c~<their label>`, checked out as above.
    `libgit2_still_writes_a_file_coming_in_over_a_folder_onto_the_folder` pins the libgit2 side.
  - **Merge, squash, cherry-pick and revert**: the caller writes the operation's state files as
    libgit2 would (`MERGE_HEAD`/`MERGE_MODE`/`MERGE_MSG`, or `CHERRY_PICK_HEAD`/`REVERT_HEAD`/
    `MERGE_MSG`; a squash only `SQUASH_MSG`), so Abort (`reset --merge` semantics, via the index
    diff) works as for any other conflict. A pick's checkout allows conflicts (skips a file with
    local changes in the way) as libgit2's pick does; a merge's refuses, as libgit2's merge does.
  - **Rebase steps**: the rebase is Browsitory's own (`advance` picks each step with libgit2's
    `cherrypick`, no `rebase-merge/`), so `rebase::stop_at_file_folder_conflict` writes
    `CHERRY_PICK_HEAD` and `MERGE_MSG` like the pick and stops the step at a conflict
    (`RebaseStop::Conflict`): Continue lands the resolved index, Abort goes back to the branch.
  - **Stash apply/pop**: `stash::file_folder_merge` redoes libgit2's merge of the stash's
    worktree tree against HEAD's; `apply_with_file_folder_conflict` checks it out with the
    stash's untracked files (written and rolled back with it, then dropped from the index; an
    untracked file at a path the merge holds is still refused with `FileFolderConflict`). As
    git, a conflicted apply leaves the merged changes staged, and a pop keeps the stash. A
    local change or an untracked file in the way refuses (`WouldOverwrite`), as libgit2's apply
    does. With "Restore staged changes" (`--index`) a collision in the index tree's merge is
    refused as `IndexConflict`, as `git stash apply --index` refuses it ("Conflicts in index.
    Try without --index."); one only in the worktree tree's merge gives the conflict without
    the index, as git's "Index was not unstashed."
  The test
  `libgit2_still_merges_a_file_and_a_folder_without_a_conflict` (`tests/file_folder_conflict.rs`)
  pins the libgit2 behaviour: when a libgit2 update fixes it, that test fails and the guard can
  give way to libgit2's conflict.

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
thread per open repo also means multiple repos never contend on a shared handle. The thread's
state and loop are `worker/worker_loop.rs`'s `WorkerLoop`: its `step` takes the state lock
(`acquire`), does the undo bookkeeping and operation gate (`before`), runs the command
(`dispatch`, the `match` over `Command` in `worker_loop/dispatch.rs`), then finishes the undo
entry and releases the lock (`after`) (#486).

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

**No worker round trip runs on an async runtime thread (#125).** An `async fn` alone only moves
the wait onto one of Tokio's few runtime threads (one per core): a commit whose hook waits, plus
the refreshes queued behind it on the same serial worker, could occupy all of them and stall
every other command, for every repository, including the `cancel_transfer` meant to stop the
hook. So commands reach a worker only through `repo_worker(&state, &path)?.call(move |w| …)`
(`RepoWorker` in `commands/mod.rs`), which runs the round trip on Tokio's blocking pool
(`off_runtime` → `spawn_blocking`); there is no other way to get a `WorkerHandle`, so a new
command can't block the runtime by accident. `cancel_transfer`/`cancel_search` use
`RepoWorker::signal()`, which writes into state shared with the worker and never waits on its
queue. Clone and the diff/merge tool waits use `off_runtime` too. `scan_repos_in_root` stays
plain async: it involves no worker and is microseconds of work (see its comment). The VSCode
sidecar runs each request on its own thread and is unaffected.

**One blocking thread per repository (#181).** The blocking pool is capped (512 threads), and
each queued round trip used to hold one of its threads until the serial worker replied: a long
hook plus repeated refreshes on one repository could fill the pool, and every other
repository's commands then queued behind it until the hook ended. So `call` first awaits the
repository's gate (`AppState`'s per-repository `tauri::async_runtime::Mutex<()>`, keyed by
the registry's repository key, created by `repo_worker` on first use and dropped by
`close_repo` with the worker) and only then takes a blocking thread.
The guard moves into the blocking closure, so a dropped command future can't release the gate
while its round trip still runs. The worker runs one command at a time, so waiting at the gate
instead of in the worker's queue loses little; waiting calls are parked tasks, not threads, and
Tokio's mutex is FIFO, so calls keep the order in which they reach the gate. Each `call` closure
only makes round trips on its own handle and never awaits another call, so nothing can deadlock
on the gate. What must not wait behind the worker goes through the ungated `signal()`:
`cancel_transfer` and `cancel_search`; the search generation, which `search_commits` claims
before it waits for the gate (`WorkerHandle::claim_search_generation`), so a cancel or a newer
search also cancels a search still waiting there; and the stored work tree path
(`tools::workdir_of`) behind Open in editor, Reveal and Open terminal.

**One worker per repository, whatever the spelling (#223).** Commands name a repository by the
path string the frontend sent, and one repository has many spellings: a trailing slash, a
subfolder, `..`, a symbolic link, another letter case on Windows and macOS. A worktree's Open
button, for example, opens the main worktree under `list_worktrees`' path, which can differ
from the tab's. Keyed by that raw string, each spelling got its own worker (and gate), so two
commands against one repository could run at the same time. `WorkerRegistry`
(`crates/repo-service/src/worker/registry.rs`), shared by `AppState` and the sidecar's `Repos`,
keys workers by `repo_key`: the discovered work tree root (the git folder for a bare
repository), canonicalized, with trailing separators trimmed if that fails. A linked worktree
has its own root and so its own worker. `open` always recomputes the key and records the
spelling as an alias; `get` only reads the aliases (no file system access per command), so a
spelling that was never opened is "repo not open". `close` forgets one alias and drops the
worker only when no alias still names its key; the Tauri app then drops the gate too, under
the registry's lock (`close_with`/`get_with`; lock order registry → gates). `open_with` reports
the other keys it leaves without a worker the same way (a spelling that now names another
repository, a failed respawn), so no gate outlives its worker. `open` keeps the former
`ensure_worker`'s rules: a dead worker is evicted and respawned, and the spawn runs unlocked.
The worker reopens its repository from its spawn path after worktree commands, so that path
must name the same repository for the worker's lifetime (`spawn_path`). The first opener's
spelling won't do (a subfolder may be gone by then), and neither will the work tree root: with
`core.worktree` (a dotfiles repository whose work tree is the home folder) the root has no
`.git`, so discovering from it finds nothing or an enclosing repository. So the worker is
spawned from the git folder (`.git`, `.git/worktrees/<name>`, `.git/modules/<name>`, or a bare
repository itself), then the root, whichever discovers the same key; the spelling is the last
resort.
Two tabs of one repository share its worker, so they share one undo history, and a new search
in one cancels a search still running in the other (one search generation per worker).
Everything else keeps the caller's spelling: hook and transfer events, recent repositories, open
tabs and per-repository settings (also a tab's custom name, `config`'s `repo_aliases`, #625), so
two spellings still make two tabs. A repository renamed
while open keeps its alias until it is closed. The registry is per process: two VSCode windows
run two sidecars, so one repository still gets one worker per window (only the undo lock works
across processes).

### Forge HTTP client

The pull-request adapters talk to GitHub and Bitbucket through `ReqwestForgeApi`
(`crates/repo-service/src/pull_requests.rs`), a blocking `reqwest` client. It is built lazily
on the worker thread at its first request, because building it inside Tauri's tokio runtime
panics. TLS is rustls with the ring crypto provider. reqwest is built with
`rustls-no-provider`, so the client installs ring as rustls's process-wide default once, before
its first build; if something else installed a provider first (Tauri's updater installs ring
too), that one stays. Server certificates are checked against the operating system's
certificate store (`rustls-platform-verifier`), not a bundled root list, so a corporate root CA
the OS trusts works too. The E2E forge fixture server is plain HTTP on `127.0.0.1` and needs no
certificate.

## Undo stack

- **Snapshots** (`git-core::undo`): HEAD, `refs/heads/*` + `refs/tags/*`, the index as a tree,
  and, for commands that touch the working tree, a "worktree tree" (HEAD tree with every
  modified/deleted/untracked file overlaid, raw bytes, no filters). Undo restores `before` over
  `after`, redo the reverse, touching only components that differ and only after verifying each
  still equals the expected side (strict: any drift refuses).
- **Recording** (`repo-service::worker::undo`): the worker snapshots before/after each
  recordable command; failed/no-op commands record nothing; operations that stop on conflicts
  stay pending until completed (one entry) or aborted (none).
- **Command policy** (`repo-service::worker::policy`, #485): one function, `policy::of`, states
  for every worker `Command` the undo entry it records (label and snapshot scope), whether it
  takes the state lock, is refused while that lock stays busy (and with which text), has its
  reply held until `finish`, is refused while a rebase or sequence is stored (`policy::Gate`:
  always, only on the protected branch, or only a stash save), and aborts a paused operation.
  It names every variant (no catch-all), so a new command doesn't compile until it has a
  policy, and it builds each policy from a few constructors (`recorded`, `locked`,
  `continuing`, `aborting`, `free`) so the pairings can't drift: whatever is recorded or ends a
  paused operation holds its reply, and whatever holds its reply takes the lock. The worker
  loop, `undo` and `gate` only read it; the typed reply they answer or hold is reached through
  `policy::reply_of`, which also names every variant. `worker/policy_tests.rs` pins every
  command's policy in one table.
- **Pending entry**: the paused operation's label and pre-operation snapshot are stored in
  `undo.json` as `pending` (optional, still version 1: older builds ignore it and drop it on
  their next save), pinned by the undo anchor like the stacks. It carries the marker of the
  operation it belongs to (`worker::gate::OperationProbe::marker`): `rebase <branch ref>
  <original tip> <onto>`, `sequence <kind> <original HEAD> <first commit> <count>`, `merge
  <HEAD> <MERGE_HEAD…> @<mtime>`, `cherry-pick <HEAD> <CHERRY_PICK_HEAD> @<mtime>`, `revert
  <HEAD> <REVERT_HEAD> @<mtime>`, `terminal <Rebase|Am> <branch> <onto> <orig-head>@<mtime>` for
  a terminal `git rebase` or `git am` (#264), else `git <state> <HEAD>`, each constant for the
  operation's whole life (the stored rebase and sequence come first because their
  `CHERRY_PICK_HEAD` changes per step). A terminal rebase moves HEAD at every step, so its marker
  is built from its own files instead: branch and onto as `repo::terminal_rebase` reads them,
  `orig-head`, and the mtime of a file git writes once when the rebase starts and never during
  it (`rebase-merge/orig-head`, `rebase-apply/last`; `git am` writes no `orig-head`, `onto` or
  `head-name`, so its marker is the kind and that mtime). A pending entry so survives the rebase's
  steps, and is still dropped when the rebase ends outside the app or is aborted and started
  again (a new mtime). Nothing in the app ends a terminal rebase (every command that could is
  refused), so such an entry is always dropped in the end, never recorded. A terminal `git bisect` alone (libgit2's `Bisect`) has no marker, as `Clean`
  (#250): commands are recorded and Undo/Redo run during it (`undo::restore` checks
  `repo::no_operation`), and its marker no longer changes with every `git bisect good/bad`.
  `@<mtime>` is the state file's modification time in nanoseconds (omitted when it
  can't be read): git writes `MERGE_HEAD`/`CHERRY_PICK_HEAD`/`REVERT_HEAD` once per operation and
  nothing rewrites them during the pause, so a merge or pick aborted outside the app and started
  again with the same heads gets a new marker, and both processes stat the same file (#172). On
  file systems with 1–2 s mtime resolution an abort and identical restart within one tick still
  collide. (No released build stored a pending entry: 0.3.0 kept it in memory, so there is no
  entry in an older marker format to migrate.) Before every command the recorder re-reads
  `undo.json` and drops a pending entry whose marker isn't the current one (`None` included), so
  an operation finished after an app restart, a closed tab, a crashed worker or in the other app
  (Tauri vs. VSCode) is still one entry, and an operation aborted or replaced outside the app
  never records under the old label. When that drop can't take the lock (the other app holds
  it), the recorder remembers the stale entry, and the `finish` after the command drops it if it
  is still pending instead of recording it as finished by that command (#175).
- **State lock** (`worker/state_lock.rs`, #172): every write to `undo.json` happens under an
  advisory lock on `<gitdir>/browsitory/state.lock` (std's `File::try_lock`: `flock` on
  Linux/macOS, `LockFileEx` on Windows), after a forced reload (`UndoStore::reload`, not the
  mtime check, which misses a second write in the same tick), and re-checks its decision there,
  so two processes (the Tauri app and the VSCode sidecar) or two workers never lose an entry.
  The worker holds the lock from before `settle` until `finish` for every command
  whose policy has `locks_state` (everything recorded, `RebaseContinue`, `AbortRebase`,
  `ContinueSequence`, `AbortSequence`, `AbortMerge`, Undo, Redo), so no other holder ever sees
  "the operation ended, but its entry isn't recorded yet" and drops it as stale. Stash save and
  worktree add (#174) take it too, though they record nothing: the gate refuses them while a
  rebase or sequence is stored, and under the lock that check and the change can't interleave
  with the other app starting one. Other commands (reads, fetch, staging, resolving) never wait:
  `settle`/`finish` take the lock only when about to write, and skip the write while it's held
  (a later command repeats it). The guard is
  dropped before the held replies are released. A locked command that can't get the lock within
  `LOCK_WAIT` (10 s; a long holder such as a pull, a commit hook or a pinentry dialog in the
  other app) logs once and runs unrecorded, and the next locked command waits only
  `CONTENDED_WAIT` (1 s), so the worker's queue waits long once, not once per click, while a
  brief hold (the other app staging a file) is still waited out (#175). While the lock keeps
  being found busy, later locked commands that would run unrecorded try once instead: each
  `Busy` after the first opens a window of single tries that lasts until `CONTENDED_WAIT` after
  the last unrecorded command finished (`state_lock::Contention`), so a burst of queued commands
  costs about one `CONTENDED_WAIT`, not one each, however long each takes. Commands that would
  refuse (policy `busy`: Undo, Redo, discards, Reset hard) still wait
  `CONTENDED_WAIT` inside the window. After a quiet `CONTENDED_WAIT` the next command waits
  again, and taking the lock resets it (#197). The commands that destroy work-tree content with
  no other way back (discard hunk, lines, file or all unstaged, and a hard reset) are refused
  instead with "Another Browsitory window is changing this repository's undo history. Try again
  in a moment, so this change can be undone." (`undo::reply_busy`); Undo/Redo instead reply
  "Another Browsitory window is changing this repository's undo history. Try again in a
  moment." Both the wait and the reply read the command's policy (`policy::BusyReply`, #225,
  #485). The lock is an OS lock on a handle, so a process that exits or is
  killed, or a worker thread that panics, releases it; the file is never deleted (that would
  let the next opener lock a new inode). The guard unlocks explicitly on drop (#175): a child
  that any thread forks (a hook, a signer, git) holds a copy of every descriptor until it
  execs, and a `flock` lock is released only by an explicit unlock or the last close, so closing
  our descriptor alone could leave the lock held for that instant. (Open-file-description locks
  would not help: they are inherited across `fork` the same way.) The lock is held across a
  commit's signing and a pull's transfer by design: the other app waits (or, after
  `LOCK_WAIT`, runs unrecorded) rather than interleave with them. Where locks aren't supported
  (some network file systems) the worker logs once and runs unlocked, as before. Each linked
  worktree has its own lock, like its own `undo.json`. While `undo.json` was written by a newer
  version (never overwritten) the in-memory stacks are the session's history and aren't
  reloaded; after any other failed save the next locked write reloads the file first, so a
  stale copy never overwrites the other window's entries. On NFS, Linux emulates `flock` with
  per-process `fcntl` locks, so only separate processes exclude each other there. `undo.json`,
  `sequence.json` and `rebase.json` are written by `atomic_file::write_atomically` (temp file
  `<name>.<pid>.<n>.tmp`, unique per write in a process, `sync_all`, rename).
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
  (stdout and stderr kept apart, the program in a new session and process group so a kill
  takes its children too) and block the repo's worker thread while pinentry waits, at most
  `SIGN_TIMEOUT` (120 s) → `SignError::TimedOut`. A signature over 1 MiB is an error, never
  truncated. Commit and amend pass the operation's cancel check (`SignOptions`) and report a
  `"signing"` pseudo-hook to the hook reporter, so the commit box shows "Waiting for the signing
  program…" with the hooks' Cancel (`SignError::Cancelled`). Cherry-pick and revert steps do the
  same through their hook options, and so do rebase steps (#151): `rebase::start_rebase_with`
  and `rebase_continue_with` take `HookOptions` for its `cancel` and `reporter` only (a rebase
  runs no hooks), and the worker reports the signer under `HookSource::Pick` with a `rebase-<n>`
  operation id, so `HookRunStrip` offers Cancel. A cancelled step is an error like a timed-out
  one: `start_rebase` rolls back, Continue keeps the step paused to try again. Verification is bounded by `VERIFY_TIMEOUT` (30 s), after which
  the signature shows the error status.
  - Signers start in a new session (`setsid`, #126), so they have no controlling terminal even
    when Browsitory was started from one: a signer that would prompt on `/dev/tty` (ssh-keygen
    with an encrypted key and no agent) fails at once (or falls back to `SSH_ASKPASS`), as on a
    desktop launch; pinentry (gpg-agent) is a separate process and unaffected. Signers are not
    put in a Windows job object (a job kill on a timeout would also end a `gpg-agent` that gpg
    auto-started).
- `verify_commit` picks the format from the signature armor, not from config:
  - gpg: `--status-fd` lines map to `SignatureStatus`.
  - ssh: `find-principals` + `verify` against `gpg.ssh.allowedSignersFile`, falling back to
    `check-novalidate` (→ `GoodUntrusted`).
  - Unsigned commits never spawn a process; there is no cache.
- Tests use real `gpg`/`ssh-keygen` with throwaway keys and skip when they are missing.
  `tests/signing_gpg.rs` is its own binary because it sets `GNUPGHOME` once per process.

## Commit search

- `git-core::search` parses the query once (`parse_query`) into a boolean expression
  (`SearchExpr`: AND of adjacent terms, `or`, `not`/`-`, parentheses, #450) and pages a
  `TOPOLOGICAL | TIME` revwalk (`search_commits`): within every AND/OR, cheap filters first,
  first-parent diff filters (`path:`, `contents:`) last. Parsing lives only in the backend; an
  invalid query is `ErrorKind::InvalidSearchQuery`, which the search field shows. `branch:`
  sets the start points, so it must be a single top-level AND term; `line:` is a set computed
  once per page and tested per commit, so it composes under `not`/`or`; `contents:` stays
  scoped to the top-level AND `path:` terms. A page ends at 100 matches, after 20,000 scanned commits, or after a
  250 ms time budget (`PageLimits::time`, checked after each commit so a page always makes
  progress) — so a commit/stage queued on the single in-order repo worker never waits behind
  thousands of `path:`/`contents:` tree diffs; a time-boxed page just returns `next_cursor`
  and the frontend's paging round continues.
- The graph and a search without a `branch:` term walk the same start points,
  `graph::GraphSelection::start_oids`: the selected local branches (`local: None`, nothing
  saved, is every local branch; `Some([])` none) plus the selected remote branches (`remote`,
  `<remote>/<branch>` names pushed as `refs/remotes/…`; empty, also for a never-saved remote
  selection, walks none, so the default graph has no remote-only commits, #385). Missing names
  are skipped (#97). A detached HEAD is always a start point too, whatever the selection (#467):
  its commits may be on no branch (a commit after checking out a commit or tag, a submodule's
  checkout, a terminal rebase), and it is what the user is on. An attached HEAD adds nothing (its
  branch is walked when selected). The search cursor's fingerprint covers it, as it covers every
  start oid. The frontend marks the row with a HEAD badge from `HeadInfo` (`headId` +
  `detachedAt`); `GraphCommit` carries no HEAD flag. File history already walks from HEAD.
  The selection is one `selection` parameter (`GraphSelectionDto`, `{ local, remote }`, #498) of
  `get_commit_graph`, `locate_commit` and `search_commits` on both transports; `remote` has no
  default, so a caller can't drop it by leaving it out. The saved-selection setters name their
  list `local` / `remote` the same way.
- Exactly one branch in the graph selection, local and remote together (what "Isolate branch"
  sets), makes both `graph::graph_log` and `search_commits` (without a `branch:` term) walk
  first parents only (`GraphSelection::walks_first_parent`, #380): the branch's own commits and
  merges, not the merged side branches. `local: None` never counts as one. Merges keep every
  `parent_ids` entry; the frontend's `assignLanes` gives a merge parent that isn't in the list
  no lane (`hiddenParentIds`, drawn as a short stub) instead of a lane running to the bottom.
  No selection or several branches walk every ancestor. "Isolate branch" saves both selections
  (`isolateGraphBranch`): on a remote branch, local `[]` and remote `[name]`; on a local branch,
  local `[name]`, and remote `[]` only when a non-empty remote selection was saved (a
  never-saved one walks nothing and keeps every remote badge).
- "Collapse merges" (#617) is the optional `firstParent` of `GraphSelectionDto` (absent is
  `false`, sent only when on), saved per repository in `config.toml`'s `graph_collapse_merges`
  path list (`get/set_graph_collapse_merges`). With it, `graph_log` and `locate_commit` walk
  first parents from every start point; `search_commits` ignores it (it still narrows only for
  one isolated branch, `isolates_one_branch`), so a search finds commits inside collapsed merges.
  In any first-parent walk each merge row carries `mergedCount`, the size of `first..other`
  (every other parent of an octopus merge together), counted with one hide/push revwalk per
  merge row of the page; `get_merged_commits(mergeId, limit)` lists that range
  `TOPOLOGICAL | TIME` (a worker read). `CommitLocation.merge` (`{ mergeId, mergePosition,
  offset }`) names the merge row that holds a target the walk doesn't reach: along a tip's
  first-parent line the commits reaching the target come first, so a binary search with
  `graph_descendant_of` finds the last one, the merge. The frontend keeps the walk's rows
  (`graphRows`) and the expanded merges (`expandedMerges`, reloaded by `refresh()` while the
  merge is still a merge row) apart and lists `withExpandedMerges(rows, expansions)` as
  `commits`, so selection, keys, menus, history entries and the lines-changed badges see expanded
  commits as ordinary rows and never a collapsed one. `assignLanes(commits, merged)` gives a
  spliced commit whose first parent isn't listed a stub instead of a lane to the bottom. "Go to"
  loads the graph through `mergePosition`, then expands the merge to at least `offset + 1`.
- Going back to the default clears a saved selection (`clear_graph_branch_selection`,
  `clear_graph_remote_branch_selection`, #402) instead of saving today's branch names, which
  would hide branches created later and, with one local branch, walk first parents only. The
  setters take a list, never `null`, so only these calls clear. "Show all branches"
  (`showAllGraphBranches`) clears both in one mutation. A dot click that turns every local
  branch on, and deleting a branch when every remaining one would be selected, clear the local
  one only (`coversEveryBranch` in `frontend/src/lib/graphSelection.ts`).
- The cursor is `<offset>:<fingerprint of the start oids and first-parent mode>` (the two
  things that decide the walk, so a changed remote selection changes it too); resuming
  re-walks and skips `offset` (polling cancel every 1,024 skipped commits), and any moved start
  point is `StaleCursor` instead of a duplicate or a gap.
- The graph pages the same way (#685): `get_commit_graph(limit, selection, cursor)` answers a
  `GraphPage` (`commits`, `nextCursor`, `stale`) from `graph::graph_page`, with the same cursor
  format (`graph::walk_fingerprint`). A cursor of another walk answers `stale: true` with no
  commits rather than an error, and the frontend reads the history again from the start.
  `useAppState.loadMoreHistory` appends the next page in the tab's read queue (after a running
  refresh, never alongside one); a refresh that reads the history again reads as many rows as
  are loaded (`graphLimit`). `CommitGraph` renders only the rows in view
  (`components/useVirtualRows.ts`: one row height, measured from the first row; padding on the
  list for the others; `aria-setsize`/`aria-posinset` on the rows) and asks for the next page
  when the rows in view come within `LOAD_AHEAD_ROWS` of the last one. Without layout (a hidden
  tab, jsdom) every row renders.
- Branch colours (#673): `useCommitGraphLayout` computes the lanes once per commit list in
  `RepoWorkspace`, for `CommitGraph` and for the sidebar: a branch's swatch, and the stripe on
  its badges, is the colour of the lane its tip's row sits on (`laneColors.branchLaneColors`); a
  branch whose tip isn't a loaded row has a neutral swatch. Lane colours follow the layout, so
  they can change when history changes.
- A refresh after any mutation (or a graph branch selection change) deliberately restarts an
  open search from page 1 rather than resuming its cursor.
- An open search highlights its matches in the graph by default (#687): `CommitGraph`'s
  `searchMatches` dims the other rows, `useSearchMatches` counts them ("3 of 100+") and steps
  through them in walk order (the graph's), reading the search's next round past the last loaded
  match, and Go to (`goToRevision(id, label, { keepSearch: true })`) shows each one: loading
  history, expanding a collapsed merge or explaining why the graph has no row. The search
  bar's **List** (`searchViewPreference.ts`, per machine) shows `CommitSearchResults`, the
  matches alone, instead.
- One in-flight search per repo: `WorkerHandle::search_commits` bumps a shared
  `SearchGeneration` (`Arc<AtomicU64>`) before queueing, and the running walk polls it, so a
  newer search or `cancel_search` stops an older one without going through the command queue
  (same reasoning as `CancelRegistry`).

## Cherry-pick sequences

- `git-core::pick` is a pure state machine: `cherry_pick_many`/`revert_many`/`continue_sequence`
  take and return a `SequenceState` (`SequenceRun { outcome, state }`, `state: Some` = "keep
  this", `None` = "no sequence any more"); `git-core` never stores this state itself:
  `repo-service` does, using the file formats in `rebase_file.rs`/`sequence_file.rs`. Those
  formats and `Serialize` on the few enums the wire DTOs carry (see "Wire DTOs") are
  `git-core`'s only serde.
- git's own terminal sequences (`git cherry-pick A B C`, `git revert A B`) keep their remaining
  list in `.git/sequencer/todo`. Wherever Browsitory finishes or clears a pick after a commit
  (the Commit button's `commit_with` and its leftover handling, a single `cherry_pick`/`revert`
  and its empty or failed step, Continue's step and its skips, a squash `start_merge`), it uses
  `commit::clear_commit_state`, git's post-commit cleanup, not libgit2's `cleanup_state` (which
  deletes every `state_files` entry: the merge and pick files, `BISECT_LOG`, `rebase-*` and
  `sequencer/`). It removes `MERGE_HEAD`, `MERGE_MODE`, `MERGE_MSG`, `AUTO_MERGE`,
  `CHERRY_PICK_HEAD` and `REVERT_HEAD` (as `git commit` does), and removes `sequencer/` only when a pick was finished and its todo has at most
  one line left (git's `have_finished_the_last_pick`), so `git cherry-pick --continue` carries on
  after a step committed in Browsitory (#241, D2-A). A missing or unreadable todo keeps the
  folder. It never touches `BISECT_LOG`, `rebase-*` or `SQUASH_MSG`, so, as with
  `git commit`, a commit no longer ends a `git bisect` started in a terminal.
  `merge::abort_merge` uses the same `clear_commit_state`, like git's `remove_branch_state`
  after `git reset --merge` (#249): it keeps `BISECT_LOG` and `rebase-*`, and keeps
  `sequencer/` for `git cherry-pick --continue` unless the aborted step was the list's last
  pick. It re-reads the index first (the worker's long-lived handle may predate a terminal
  merge). During a terminal rebase or `git am` it always refuses (`MergeError::TerminalRebase`,
  changing nothing): the conflicts and `MERGE_MSG` are the rebase's step, and so are the
  `MERGE_HEAD` git writes for a `--rebase-merges` merge step and the `CHERRY_PICK_HEAD` of an
  empty pick (`rebase -i`). Aborting those would make `git rebase --continue` drop the merged
  side branch. A merge started by hand during the rebase looks the same, so it is ended with
  `git merge --abort` in the terminal (#263). `pick::abort_sequence`'s leftover case and the rebase use `clear_operation_files`,
  which never touches `sequencer/`. A state file Abort can't remove is
  `MergeError::StateFiles` ("Couldn't remove git's merge state: …").
- **Terminal sequences** (#258, #253). `pick::terminal_sequence` reads git's files itself,
  since libgit2 calls a sequence between steps `Clean`: `Some` while `sequencer/todo` exists
  (per worktree). `kind` comes from the todo's first command (`pick`/`p`, `revert`/`r`, as
  `git status`), else `REVERT_HEAD`; `paused_on_conflict` = a pick head exists; `started_at` =
  `sequencer/head`; `rewinds` = HEAD equals `sequencer/abort-safety` and differs from
  `started_at` (git's `rollback_is_safe`; a manual `git commit` doesn't update `abort-safety`,
  so a step committed by hand turns rewinding off); `commits_to_remove` counts
  `started_at..HEAD` (capped at 1000). `head_info` reports it as `HeadInfo.terminal_sequence`.
  - Refusals (D5-A): while one is paused between steps, `check_ready` (every single and
    several-commit cherry-pick/revert), `continue_sequence` (before any step) and
    `rebase::start_rebase` refuse with `PickError::TerminalSequence` /
    `RebaseError::TerminalSequence` ("A cherry-pick started in a terminal isn't finished. …").
    git allows a single pick and a rebase there; refusing them keeps every paused pick in the
    repository the terminal's own step, so Abort can't mistake one for it. Merge, commit,
    amend, reset, checkout, stash and undo stay allowed and keep the list (#249).
  - Abort (D7-A): `pick::abort_terminal_sequence(repo, confirmed)` is `git cherry-pick
    --abort`. Like git it checks everything before writing anything, so a refusal leaves the
    paused step, its conflicts, the resolution in the work tree and the list byte-identical:
    another operation paused there (a merge) is `OperationInProgress`; damaged sequencer files
    (`TerminalSequence::damage`, in git's order: `head` missing or not a full id,
    `abort-safety` present but unparsable, the start commit missing when it would rewind) are
    `SequencerDamaged` (git refuses these too; the message points at `git cherry-pick --quit`;
    a missing `abort-safety` is "not safe", as git: no rewind, list removed); `confirmed`
    (`ConfirmedAbort`: HEAD, start, `rewinds`, `commits_to_remove` and `paused_on_conflict` as
    the UI showed them, so a step that paused after a between-steps confirmation isn't
    discarded without the dialog's warning) must still match, else `TerminalSequenceChanged`;
    the step's reset is planned by `merge::plan_operation_reset` (below); and a rewind is
    checked by `check_rewind`: the linked-folder guard, then every path that differs between
    HEAD and the start must be unchanged in the index and work tree (`status_file` empty; an
    untracked or ignored file in the way counts), except the step's own touched paths, which
    its reset puts back first; else `RewindBlocked(path)`. A path that is a folder in the work
    tree and in HEAD (a step replaced file `d` with `d/x`; `status_file` errors on a folder)
    blocks only if a status scan of the folder finds a changed, untracked or ignored entry. Only then: the step is reset keeping `sequencer/`
    (`merge::abort_step_keeping_list`), `rewind_to(started_at)` (shared with `abort_sequence`:
    SAFE `checkout_tree`, move the branch, or for a detached HEAD `reference("HEAD", …)` so the
    reflog says `reset: moving to <id>` as git's `reset --merge`), and `sequencer/` is removed.
    `commits_to_remove` counts up to `COMMITS_TO_REMOVE_CAP` + 1 (the UI says "more than
    1000"). Aborting only the step and keeping the list is not offered: `git cherry-pick
    --continue` would then take the aborted step as done and silently skip it.
  - `merge::plan_operation_reset` (used by `abort_merge` too) refuses, before writing, a touched
    path that is not a conflict and whose work-tree file differs from its staged version
    (a later edit, a type change such as a symlink, or an untracked file where the operation
    staged a deletion): `MergeError::WouldDiscard`, git's "Entry '…' not uptodate. Cannot
    merge." A conflicted path's work-tree file is the step's resolution and is discarded, as
    with git.
  - Routing: the worker's `AbortSequence` aborts the stored sequence if there is one, else the
    terminal one. `AbortMerge` without a stored sequence refuses a terminal sequence's step or
    list ("This cherry-pick was started in a terminal. Try Abort again to see what it will
    undo.", nothing changed); a merge paused during it is aborted as usual. git-core's
    `merge::abort_merge` itself keeps `git reset --merge`'s behaviour. The frontend sends every
    Abort of a terminal sequence (strip, commit box, palette) to `abortSequence` with
    `confirmed` (HEAD id and the fields shown), after a confirmation when it `rewinds` and
    removes commits; the worker refuses a terminal abort without one. Not undoable; the commits
    stay in the reflog.
- `repo-service`'s `SequenceStore` (`worker/sequence_store.rs`, a `StateFile` over the
  `sequence_file` codec, like `RebaseStore`) persists it as
  `<gitdir>/browsitory/sequence.json` (per worktree, versioned, temp file + `sync_all` + rename,
  like `undo.json`), written only when a sequence stops and deleted when it completes or is
  aborted. On load it must parse, have a known version (2; a version 1 file loads as an error
  stop with an unknown original ref), and name commits that exist in the repository; anything
  else is logged once and treated as absent, never fatal. The format and these checks are
  `git_core::sequence_file` (`encode`/`decode`), so the other worktrees load the file exactly
  as its own worktree does (#293, `repo::HoldRule` below).
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
  nothing else in progress, and otherwise just forgets the sequence (`rewound: false`). A
  renamed branch counts as that ref, as git's `cherry-pick --abort` rewinds it (#293): HEAD is
  at the stop point, `originalRef` no longer exists, and HEAD's branch's reflog records the
  renames from `originalRef` ("Branch: renamed <old> to <new>", written by `git branch -m` and
  libgit2, maybe in steps) at `originalHead` or a commit after it, so written during the
  sequence (a step's `post-commit` hook may rename too). An older rename from that name, or a
  branch switched to after `originalRef` was deleted, has no such entries and isn't rewound; nor
  is a rename whose reflog is gone. This reads reflog messages, which anyone can write (`git
  update-ref -m`), so it is a heuristic: a forged rename entry at such a commit would count.
- A stored sequence counts as "in progress" for undo (one pending entry, dropped on abort),
  blocks picks, merge, rebase, commit/amend, reset, checkout, branch switch, pull, stash
  apply/pop and reflog restore in the worker (`gate::gate_during_operation`, reading each
  command's `policy::Gate`), and, like a
  rebase, protects its branch (`gate::GateScope`, #171): deleting or renaming the branch named
  by `originalRef`, or adding a worktree on it, and stash save are refused while it is stored
  (a renamed branch would make Abort answer `rewound: false`; a stash would carry the paused
  step's index away). A detached or unknown `originalRef` protects no branch. It is reported
  as `HeadInfo.sequence`. The undo entry is labelled with the number of commits applied. `AbortMerge` aborts the
  whole sequence when one is stored, so a stale frontend can't strand the file.
- Empty steps are skipped (git stops for `--skip`), without hooks; so is a step that the
  continued step's `pre-commit` put back to HEAD's tree. The step Continue commits
  (the resolved, paused one) runs `git cherry-pick --continue`'s commit hooks through the commit
  path's helpers (`commit::run_commit_hooks` with `pre-commit` and source `merge`, then
  `create_commit_reporting` with the signing pseudo-hook and cancel, then `post-commit`), #133.
  A failing hook is `SequenceOutcome::HookFailed { hook, output, progress }` with the state
  returned unchanged and git's paused state (`CHERRY_PICK_HEAD`/`REVERT_HEAD`, the resolved
  index) untouched, so `sequence.json` still describes the paused step and Continue (with
  `no_verify` from "Continue without hooks") or Abort work as before; a cancelled hook is
  `PickError::Hook`, also with the state kept. The worker holds the #173 state lock across the
  hooks, as for any Continue.
- Steps applied cleanly (a single `cherry_pick`/`revert`, every step of `cherry_pick_many`/
  `revert_many`, and the steps Continue applies after the paused one) run what git's sequencer
  runs for a commit it creates (#183, D5-A; checked against `git cherry-pick A B C` in a twin
  repo): `commit::run_message_hook` (`prepare-commit-msg <COMMIT_EDITMSG> message`, the file
  written only when the hook is installed, no `pre-commit`/`commit-msg`), then
  `create_commit_reporting`, then `post-commit` while `CHERRY_PICK_HEAD` and `MERGE_MSG` are still
  there, as in git's `try_to_commit` (the parity tests log what each hook sees; a revert has
  `MERGE_MSG` only in git, and libgit2's `REVERT_HEAD` is not compared), and only then the state
  is cleared. git 2.53 passes no `GIT_REFLOG_ACTION` to these hooks, so Browsitory sets none.
  A failing (or message-emptying)
  `prepare-commit-msg` is `StepResult::HookFailed`: nothing is committed, git's pick state and the
  staged step stay, a single pick returns `PickOutcome::HookFailed` and a sequence stops as
  `SequenceOutcome::HookFailed` with `stopped_on_conflict` set, so Continue commits it through
  `finish_paused` (the full commit hooks) or the user commits it by hand. A cancel during
  `prepare-commit-msg` is `PickError::Hook` with the step paused likewise. D9-A: `run` checks the
  cancel flag before each step, so a Cancel during a step's `post-commit` (or between hooks)
  keeps that commit and stops the sequence after it as `SequenceOutcome::Stopped { kept,
  progress }`, not an error (state kept with `expected_head` at the kept commit); the UI shows
  "Stopped after "<summary>"; its commit was kept. Continue to resume." and Continue resumes
  with the next step. The hooks report on the pick's own operation
  (`pick-<n>`, `HookSource::Pick`) and the later steps of a Continue on its `continue-<n>`
  (`HookSource::Commit`, so they show on the sequence panel).
- A clean step and `finish_paused` treat a `cleanup_state` failure after the commit landed like
  `commit_with` does (`commit::cleanup_after_commit`: logged, `post-commit` still runs, the step
  counts and `next`/`expected_head` move past it, D7-A, #216). The next step then usually fails
  to write its own pick state and the sequence stops there; Continue first clears a leftover pick
  state that names the previous step while HEAD is still `expected_head`
  (`is_leftover_of_committed_step`; never after a conflict stop, whose pick state is the paused
  step), then goes on. Abort of a stored sequence (`abort_sequence`) clears such a leftover the
  same way, then rewinds to the start only while HEAD is still `expected_head`, which `run` sets
  to each step's base before the step and to the kept commit when it stops after one, so a step
  whose commit landed but whose state stayed behind is rewound with the rest. `start` drops
  repeated commit ids (keeping the first), so no two steps share an id and a pick state always
  names exactly one step.

## Interactive rebase

- `git-core::rebase` is Browsitory's own state machine; it does not use libgit2's `git2::Rebase`
  or write git's `.git/rebase-merge`. `start_rebase` first refuses any operation in progress,
  naming it (#265): a terminal rebase or `git am` (`RebaseError::TerminalRebase`, #263), a
  paused merge, cherry-pick or revert (`RebaseError::OperationPaused`: "A merge is in progress.
  Commit or abort it before starting a rebase."; a merge resolved to HEAD's side used to get
  through), a cherry-pick or revert started in a terminal and paused between steps
  (`RebaseError::TerminalSequence`, #258); a terminal bisect is none (#250). Then the plan, then
  tracked local changes
  (`RebaseError::LocalChanges`, #170: any staged change, or an unstaged change to a tracked file,
  including a skip-worktree file whose content (after git's filters) differs from the index,
  which libgit2's status hides; untracked and ignored files, submodules and a sparse checkout's
  missing skip-worktree files don't count, as with `git rebase`), because its
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
- **Terminal rebases** (#263). `repo::terminal_rebase` reports a `git rebase` or `git am` run
  in a terminal and not finished, per worktree: `rebase-merge/` is always a rebase,
  `rebase-apply/` is `git am` when it holds `applying` and a rebase otherwise (git's
  `wt_status_check_rebase`). It reads the branch (`head-name`, short name, empty when detached),
  `onto`, and the step (`msgnum`/`end`, or `next`/`last`); `git am` has no branch or onto.
  `head_info` reports it as `HeadInfo.terminal_rebase`. Browsitory's own rebase never writes
  these folders, so it is never taken for one.
  - Refusals: `repo::check_no_terminal_rebase` returns `TerminalRebaseInProgress` ("A rebase
    started in a terminal isn't finished. Continue it there with git rebase --continue, or abort
    it with git rebase --abort."; "A git am …" for `git am`), carried transparently by
    `PickError`, `RebaseError`, `ResetError`, `CommitError` and `MergeError`. It is checked by
    every pick and revert (`check_ready`), `continue_sequence`, `abort_terminal_sequence`,
    `start_rebase`, `rebase_continue`, `reset_to`, `amend_head`, Abort merge (see
    `abort_merge`) and `start_merge` while git's `MERGE_HEAD` exists (a `--rebase-merges` merge
    step). These all refused before as "an operation in progress" without naming it,
    except Abort merge, which reset the rebase's conflicted step and removed its `MERGE_MSG`
    (since #249 it no longer deleted `rebase-merge/`, so `git rebase --abort` still worked).
    Commit, staging and conflict resolution stay allowed, as `git commit` and `git add` are.
  - Commit behaves like `git commit` at the rebase's step (`commit::merging_now`,
    `picking_now`): with git's `MERGE_HEAD` (a merge step) it makes the merge commit with
    `MERGE_HEAD` as second parent and clears the merge's files, so `git rebase --continue` keeps
    the side branch; with `CHERRY_PICK_HEAD` (an empty pick) it keeps the picked commit's author
    and clears the pick's files. The resulting state files and rebased history are twinned with
    the git CLI. git writes that `MERGE_HEAD` without a final newline, which libgit2's
    `mergehead_foreach` rejects, so `commit::merge_heads` reads the file itself.
  - The rebased branch (`head-name` in any worktree's `rebase-merge/` or `rebase-apply/`) can't
    be deleted, renamed or checked out in a new worktree (`repo::BranchBeingRebased`, "topic is
    being rebased by a rebase started in a terminal. …"), from this or any other worktree, nor
    switched to from another worktree (`check_branch_not_rebased_elsewhere`), as git refuses.
    Undo refuses to move it, or to put HEAD on it from another worktree (`repo::HoldRule`
    below). HEAD is detached during the rebase, so libgit2's own checked-out checks don't see
    it.
  - Reflog restore (`reflog::restore_reflog_entry`, #287) refuses to move a branch that is
    being rebased in any worktree (`BranchBeingRebased`) or that a worktree holds
    (`repo::BranchHeld`, "branch 'topic' is checked out in another worktree at '<folder>'"). The
    first is `check_branch_not_rebased`; the second is `repo::branch_held` (#291), which Undo,
    Switch, branch delete and rename, and worktree add use too. It reads every worktree's git
    folder, like git's `get_worktrees` (a linked worktree whose `gitdir` file is gone isn't
    listed, as in git): the branch its `HEAD` is on, a terminal rebase's `head-name` and
    `rebase-merge/update-refs`, the branch a bisect started from (`BISECT_START`, while
    `BISECT_LOG` exists), and, for other worktrees whose HEAD is detached, Browsitory's own
    paused rebase (`rebase_file::decode`, the same check the owning worktree's `RebaseStore`
    loads with; this worktree's own is the worker's gate's). For other worktrees whose HEAD is
    on that branch, it also reads Browsitory's own paused cherry-pick or revert sequence
    (`sequence_file::decode`, the owning `SequenceStore`'s check; its `originalRef` branch,
    `BranchUse::Sequence`, "branch 'topic' has a paused cherry-pick in another worktree at
    '<folder>'", #293). A sequence runs on its branch, so HEAD stays attached during the pause.
    Once HEAD left that branch, the sequence no longer touches it: Abort doesn't rewind it, and
    Continue, which checks only the commit HEAD is on, goes on on HEAD's new branch. So it then
    holds nothing (the owning window's gate still protects `originalRef`). Which of these count
    follows the git command the action stands for (`repo::HoldRule`): Restore and branch delete
    follow `branch_checked_out` (`branch -f`/`-D`: all of them, in every worktree, this one too
    except its HEAD branch, whether or not HEAD is detached); Switch and worktree add follow
    `find_shared_symref` (HEAD, and a rebase's or bisect's branch only while that worktree's
    HEAD is detached; Switch skips this worktree, worktree add doesn't; update-refs don't
    count); rename follows `reject_rebase_or_bisect_branch` (rebase and bisect in worktrees
    whose HEAD is detached, this one too). A stored sequence counts under every rule. Undo uses
    the first rule for the refs it moves or deletes and Switch's for the branch it puts HEAD on,
    and returns the refusal as `UndoOutcome::BranchHeld` (the entry stays). Each case is twinned
    with the git CLI. A worktree whose folder is gone, or no longer has its `.git` file, still
    counts (locked on an unmounted drive, or not pruned yet); the refusal then says to run `git
    worktree prune`, or that the worktree is locked. Unlike git, rename, Switch and worktree add
    stay refused while a terminal rebase in another worktree (for rename and worktree add, any
    worktree) rebases the branch, whether or not HEAD is detached there
    (`check_branch_not_rebased`, #263). git counts a terminal `git cherry-pick`/`git revert`
    sequence in another worktree only through that worktree's HEAD, so `branch -f`/`-D`,
    `switch` and `worktree add` refuse its branch and `branch -m` renames it (HEAD moves along,
    and `cherry-pick --abort` still rewinds it: git records commits, not the name); Browsitory
    follows git for a terminal sequence. For its own sequence, rename is refused too, unlike
    git, as the owning window's gate refuses it: the stored sequence keeps the old name, which
    that gate protects, and Abort finds a renamed branch only through its reflog
    (`pick::abort_sequence`, below). The branch checked out in this worktree is still moved
    (ref only; the work tree and index stay), though `git branch -f` refuses that one: moving
    HEAD's branch back is what Restore is for. Moving this worktree's HEAD (detached, or its
    branch by `HEAD` or by name) has Reset's guards: refused during a terminal rebase
    (`TerminalRebaseInProgress`) or a paused merge, cherry-pick, revert or rebase
    (`ReflogError::OperationInProgress`, Reset's wording). The move is a compare-and-swap
    (`reference_matching` on the target read just before), so a ref moved meanwhile is left
    alone (`ReflogError::ReferenceMoved`).
  - No in-app Abort: `git rebase --abort` also restores an autostash, `--update-refs` branches
    and the apply backend's state, which Browsitory can't do exactly; the strip says to continue
    or abort in the terminal.
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
  (`gate::RebaseMemory::fallback`, preferred over the file until the next successful save or clear) and
  reports the failure with the outcome.
- **Health** (`rebase_health`, reported as `HeadInfo.rebase.health`; the first failing check
  decides): the branch is gone (`BranchMissing`); it isn't at `original_tip` (`BranchMoved`);
  git has an operation other than this rebase's own pick paused (`OtherOperation`); HEAD isn't
  detached at `expected_head` and isn't the step committed by hand (`HeadMoved`). Otherwise
  `Paused`. A diverged rebase stays stored and visible ("interrupted"); only Abort is offered.
  A `git bisect` started in a terminal is not another operation: every state check of the
  rebase (`resume_kind`, `rebase_health`, `is_step_committed_by_hand`, `abort_rebase`) uses
  `repo::no_operation`, so a rebase run during a bisect pauses, continues and aborts as without
  one. Landing a step, `finish` and Abort clear only the pick's own files
  (`commit::clear_operation_files`), so the bisect, and a terminal rebase's or cherry-pick's
  state, survive (#249, #250).
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
  exit, cancel and a 10-minute deadline, and it is always reaped. On Unix it starts a new
  session (`pre_exec` → `setsid`, #126; not combined with `process_group(0)`, which std applies
  first and which makes `setsid` fail with `EPERM`): it leads its own process group, and
  timeout/cancel `killpg(pid, SIGKILL)` that group (the pid can't have been reused, the child
  isn't reaped yet), so its children die too. It also has no controlling terminal, so a hook
  that opens `/dev/tty` fails at once with `ENXIO` instead of being stopped by `SIGTTIN` when the
  app was started from a shell. After exit the pipe gets a 2 s drain grace in case an escaped
  grandchild holds it open (the hook's result is not held up longer than that). Windows runs
  hooks through Git for Windows' `sh.exe`, started with `CREATE_SUSPENDED`, assigned to an
  unnamed job object and only then resumed (`process/job_windows.rs`, hand-written kernel32
  FFI, #122, #180), so nothing the hook starts directly or through its descendants can escape
  the job. Out of reach: processes a broker starts on the hook's behalf (COM/DCOM activation,
  Task Scheduler, WMI, a service), which belong to the broker's tree, and the fallback below
  when the job can't be created or assigned. std closes the primary thread's handle, so
  `resume` finds the process's threads with a Toolhelp32 snapshot (retried a few times;
  `OpenThread` + `ResumeThread`, skipping a thread that fails); a hook of which no thread could
  be resumed is killed at once instead of waiting out the time limit. Timeout/cancel `TerminateJobObject` and then kill the child, so
  programs the hook started die too. There is no `KILL_ON_JOB_CLOSE` (D6-A): a hook that exits
  normally keeps its background processes, as on Unix. If creating or assigning the job fails,
  the hook still runs and only the child is killed.
- Gate hooks live in `git-core`: `commit`/`amend_head` (`pre-commit` → reload the index only if
  it ran → `prepare-commit-msg` → `commit-msg` via `<git dir>/COMMIT_EDITMSG` → commit →
  `post-commit`), `commit_with(.., CommitHookSet::CleanMerge { head, merge_heads }, ..)` (#124:
  while exactly that merge is staged, `pre-merge-commit` in place of `pre-commit` and no
  `post-commit`, as `git merge` runs them; the message hooks get `<git dir>/MERGE_MSG`
  (`MessageFile::MergeMsg`, #183, D6-A), as with `git merge`), the picks and `continue_sequence`
  (see "Cherry-pick sequences") and `push_refs`'s `push_negotiation` callback (`pre-push` with git's stdin,
  after the ref advertisement and the lease check, before any pack data). Every push
  (`push_branch` and its `push_current_branch` wrapper, force push, tags,
  `delete_remote_branch`, `delete_remote_tag`) funnels through `push_refs`, so all of them run
  `pre-push`.
  `post-checkout`/`post-merge` run in the `repo-service` worker after successful commands only,
  so undo restores, rebase and other git-core internals never trigger hooks. Each runs on its
  command's own operation, before the cancel id is released and before the reply
  (`worker::hooks::run_post_hook_reported` / `run_post_hook_on`, #178, #185): a clean merge's
  commit runs `post-merge 0` on `commit-<n>`; a branch switch and a commit checkout run
  `post-checkout` on `switch-<n>` / `checkout-<n>`; a fast-forward or squash `StartMerge` runs
  `post-merge 0|1` on `merge-<n>`. Each reports `Started`/`Finished`, `cancelTransfer(id)` stops
  it, and its result (cancel included) never changes the command's outcome; `no_verify` doesn't
  skip it. A pull that fast-forwarded sends `TransferEvent::Progress { phase: RunningHook }` on
  its own transfer operation (only when `post-merge` is installed) and runs `post-merge 0` with
  the pull's cancel check, before `Completed`; the toolbar's transfer progress (`RepoActivity`) labels `RunningHook` by
  operation (`Pull` → `post-merge`, else `pre-push`).
- `HookEvent::{Started, Finished}` carry `source: HookSource` (`Commit` for commit, amend and
  Continue; `Checkout`, `Merge`, `Pick`), sent as `source` in both transports' `hook-progress` /
  `hookProgress` payloads (#185). The frontend keeps it in `state.hookRun`: a `Commit` hook shows
  in the commit box and sequence panel (`HookStatus`), any other in the non-modal `HookRunStrip`
  ("Hook status", role `status`) under `OperationStatusStrip`, with Cancel (D8-A). `RepoClient`
  method signatures don't change; the transports create the event channel per command.
- `commit_with` re-reads the merge after the pre hook and again after the message hooks (#183):
  if the state is no longer `Merge`, or HEAD or `MERGE_HEAD` changed (a merge aborted and
  restarted in a terminal during `pre-merge-commit`), it returns `CommitError::MergeChanged` and
  commits nothing; the first check comes before the message file is written, so a restarted
  merge's own `MERGE_MSG` is kept. Once the commit landed, `commit::clear_commit_state` (see
  "Cherry-pick sequences") and `clear_squash_message` failures are logged (`commit::cleanup_after_commit`) and the result is
  still `Ok` (D7-A): post hooks run, the worker forgets the clean merge, the frontend clears the
  message, and the leftover merge/pick state stays visible in the operation strip, where Abort
  clears it harmlessly (worktree and index equal the new HEAD). A later commit that finds such a
  leftover merge (every `MERGE_HEAD` already in HEAD's history) clears it first and commits as an
  ordinary single-parent commit, so no second merge commit is made. Otherwise the parents are
  reduced as `git commit` does (`reduce_heads`, #227): HEAD and the `MERGE_HEAD` commits without
  repeats and without any commit another of them has in its history (so a mixed octopus
  `MERGE_HEAD` keeps its other parents, and a HEAD the merged commit contains is dropped too),
  unless `MERGE_MODE` says `no-ff`. libgit2 always writes `no-ff` there; `start_merge` rewrites it
  as `git merge` does (`no-ff` only for `FastForward::Never`, else empty); if it still can't be cleared,
  `CommitError::LeftoverMergeState` refuses the commit. A leftover cherry-pick is handled the
  same way (#226, D3-A): when HEAD is the pick's own commit (`pick::commit_carries_pick` with
  `Coverage::Exact`: one parent, the picked commit's author with its time, and exactly the
  pick's result on every path it changes cleanly onto HEAD's parent, at least one; the message
  isn't compared, since `prepare-commit-msg` may have rewritten it), the commit clears the
  leftover first and is authored by the user, not by the picked commit's author. Both
  leftovers are cleared with `clear_commit_state`, so a terminal sequence's `.git/sequencer`
  and the rest of its todo stay, as after `git commit` (#241). If it can't be cleared, `CommitError::LeftoverPickState` refuses the
  commit. A paused pick's HEAD is the commit the pick applies to; it matches only if it has the
  picked author to the second and the pick's exact content on every cleanly changed path. The
  usual way that happens is a duplicate pick: `git cherry-pick X` run again in a terminal once
  HEAD is X's pick stops as empty and leaves `CHERRY_PICK_HEAD`. That is deliberately treated
  as a leftover too: clearing it is the outcome of `git cherry-pick --skip`, and a sequence's
  todo stays, so `git cherry-pick --continue` goes on with the next step (tested with the git
  CLI). Not recognised, so the commit takes the picked author
  as before: a pick with no clean change, a merge commit picked with a mainline (the parent used
  isn't recorded), and any error while checking (it never blocks a commit). A leftover revert
  is handled the same way (#232): HEAD is the revert's own commit when it has one parent, a
  message that still says "This reverts commit <full id of REVERT_HEAD>", and exactly the
  revert's result on every path the revert changes cleanly onto HEAD's parent, at least one
  (`pick::commit_carries_revert`). The author can't tell (a revert is authored by the user), so
  the message line stands in for it; a commit that undoes the same change by hand is not taken
  for the revert. The commit then clears the leftover, so `prepare-commit-msg` gets `message`,
  not `merge`; if it can't, `CommitError::LeftoverRevertState`. Not recognised: a revert of a
  merge commit, and a message hook that removed the line.
- Stale vs. live operation state (#240): git itself never calls `MERGE_HEAD`,
  `CHERRY_PICK_HEAD` or `REVERT_HEAD` stale. While one exists, `git status` shows the operation
  in progress, `git commit` pre-fills `MERGE_MSG` (hook source `merge`) and finishes it, and
  `git commit --amend` refuses for a merge or cherry-pick (it amends with a `REVERT_HEAD` and
  removes it). Browsitory calls the state a leftover only when HEAD already carries the
  operation's change by the three checks above (`commit::leftover_operation`): a history walk
  for a merge, heuristics for a pick (author, time and content) and a revert (message line and
  content). The index must have no conflict stages: a paused operation with conflicts is never a
  leftover, even when HEAD was moved onto `MERGE_HEAD` in a terminal, so nothing is cleared
  before the commit or amend could succeed. Any other state is an operation in progress and
  treated as git treats it. A leftover is never removed on its own: it
  is reported as `HeadInfo.operation_leftover` (the strip says "… already applied" and that its
  changes are already in the last commit, and keeps Abort; disabled actions give the same
  reason; the commit box isn't pre-filled with `MERGE_MSG`, and amend is offered), and it is
  cleared by Abort, by the next commit (above) or by `amend_head`, which clears it with
  `clear_commit_state` before amending (the same `Leftover…State` errors if it can't). An amend
  during an operation in progress is still `OperationInProgress`, also for a revert, where git
  would amend. `head_info` lists no `pre-merge-commit` for a leftover merge: the next commit is
  an ordinary one. Cost: `head_info` runs the check on every refresh, but only while git shows
  a merge, cherry-pick or revert. The pick check compares author and time, and the revert check
  the message line, before it does an in-memory pick or revert onto HEAD's parent.
- A pull drops its cancel flag (`TransferOp::clear`) before `post-merge`, so a Cancel pressed
  during the fetch that came too late to stop the pull doesn't kill the hook at once; only a
  Cancel pressed while `post-merge` runs stops it.
- Clean merges (#124, D1-A/D2-A): Browsitory never creates the merge commit in `start_merge`; a
  clean non-fast-forward merge (`MergeOutcome::Merged`) is staged and committed from the commit
  box. The worker remembers such a merge in memory (`worker/merge.rs` `CleanMerge`: HEAD and the
  `MERGE_HEAD` commits) and, while the repository still shows exactly that merge, commits it with
  `CleanMerge::hook_set()` (the same HEAD and heads, which `commit_with` checks again before and
  after the hooks) and runs `post-merge 0` after the commit (not when the merge is
  staged), and only when `commit_with` reports `Committed::merge_hooks` (the merge was still in
  progress at commit time, not aborted by another window or a terminal in between). The memory is cleared by any other `StartMerge` result, by `AbortMerge` and by a
  successful commit; a failed or cancelled commit keeps it, so "Commit without hooks" is
  `git merge --no-verify`. After a restart, or for a merge started by the other window or
  outside Browsitory, there is none and the commit runs `git commit`'s hooks, which is what git
  does for `merge --no-commit` + `commit`. A merge that had conflicts always runs the commit
  hooks, as in git. The same fallback applies when Undo takes back a clean merge's commit (the
  merge is staged again, but the memory was cleared by the commit). The worker's
  `GetHeadInfo` lists only the pre-hook the commit will run in `commitHooks` (`pre-merge-commit`
  while the remembered merge matches, else `pre-commit`), so **Skip hooks** and its tooltip
  name the right hook.
- A hook failure crosses IPC as data: `CommitOutcome::HookFailed` for commit/amend, and
  `TransferErrorKind::HookFailed` + `hookOutput` for pushes. Hook output is the only process
  output forwarded over IPC; remote sideband text still is not.
- Progress: `HookReporter` → `HookEvent` → `hook-progress` / `hookProgress` for commit, amend
  and sequence Continue; `TransferPhase::RunningHook` for pushes. Commit/amend/Continue get
  `commit-<n>`/`amend-<n>`/`continue-<n>` ids in `CancelRegistry`, so `cancelTransfer` stops a
  running hook. Hooks block the repo worker like
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
- With `core.ignorecase` on, `prepare_forced_checkout` first refuses (`worktree_guard::CasePair`,
  writing nothing) when the checkout's target, HEAD or the index holds a case pair (`A` and `a`,
  compared as libgit2's ignore-case index does: whole paths, ASCII case only) that the checkout
  selects: any pair for a whole-tree checkout, a pair one of the literal paths selects otherwise
  (#403). libgit2 1.9 keeps only one of such a pair in its index, and depending on directory
  order deletes the other's file, even when both are named as paths
  (`tests/case_pair_checkout.rs` pins this; the stash's own form of it is `StashError::CasePair`,
  #392). Seen: `reset_to(Hard)` deleted `d/f` of `D/f`/`d/f`, rebase abort dropped it from the
  index too, and `discard_file("x/B")` left `x/b` twice in the index. A path-limited checkout of
  other paths keeps the pair and goes ahead. `start_rebase` runs the same check (onto, HEAD and
  index) before it writes anything. Undo/redo routes around it: a pair member is never one of
  `checkout_from_index`'s paths but a raw write.
- Merge-like steps refuse a case pair they bring in (#522): with `core.ignorecase` on,
  `worktree_guard::refuse_case_pair_brought_in` (`CasePairBroughtIn`, writing nothing) refuses
  a result that holds a pair HEAD's tree doesn't. libgit2's in-memory merge holds both names,
  but its ignore-case index takes `a` for `A`'s entry: the commit made from it kept one name
  (`a` without `A`, or `A` not recorded) and left the other's file untracked. It runs next to
  `find_file_folder_conflict` on each in-memory result: a rebase step (`advance`; the step
  stops as an error, so Continue retries and Abort works, or `start_rebase` rolls back), a
  cherry-pick or revert step (`pick::step`, single or in a sequence), a merge or squash
  (`merge_commits`), a stash apply's `merge_trees` (with `--index` both), and the tree a
  fast-forward merge or pull checks out (libgit2's safe checkout kept one name in the index and
  didn't write the other's file). Only the result is compared with HEAD, not their union: a
  case-only rename (`A` in HEAD, only `a` in the result) holds no pair, and libgit2 commits it
  correctly. A pair HEAD already holds is refused by a merge or squash up front (#512,
  `merge::refuse_case_pair_held`, `CasePair`): libgit2's merge checkout fails on it with a raw
  "1 conflict prevents checkout" even when the merge touches neither name. A fast-forward, a
  cherry-pick, a revert, a switch and a checkout keep both names and go ahead
  (`tests/case_pair_held.rs`). A clone refuses one the same way (#512): with `core.ignorecase`
  on for the new repository (a global setting on a case-sensitive file system),
  `clone::checkout_clone` runs `refuse_case_pair` on the fetched tree before its forced
  checkout, and the clone is removed (`CloneError::CasePair`).
- Cost of the case-pair check (#512, `worktree_guard`'s ignored `case_pair_cost`, release build,
  100,000 files): a whole-tree check walks the target, HEAD's tree (once when it is the target)
  and the index, about 0.19 s. A path-limited one (discarding a file) collects only the names
  its paths select (`selected_case_pairs`: the index filtered, tree walks that skip every folder
  neither on the way to nor below a selected path), about 0.01 s instead of 0.18 s.
- Switches and checkouts refuse a case pair they bring in the same way (#532): `switch_branch`,
  `checkout_commit` (a commit, tag or remote branch) and `create_branch` (before the branch is
  created) run `refuse_case_pair_brought_in` on the target tree against HEAD's tree, after the
  linked-folder check. libgit2's safe checkout kept one name in the index and didn't write the
  other's file, so the next commit recorded a deletion. `create_worktree` runs it with no base
  (the new worktree's index starts empty), so any pair in the tree is refused. Refused rather
  than written around (as undo and aborts do): on the case-insensitive drives
  `core.ignorecase` exists for, both files can't exist at once anyway. Reflog restore moves
  only a ref and checks nothing out.
- A submodule update refuses one too (#534): `check_submodule_tree` runs
  `refuse_case_pair_brought_in` in the submodule, on the target commit's tree against the
  submodule's HEAD tree, with the submodule's own `core.ignorecase` (a global one applies to a
  fresh clone). A fresh clone, or one whose checkout was refused (empty work tree, HEAD already
  at the target), has no base: any pair is refused, and the clone stays with an empty work tree.
- Aborts never refuse a case pair (#511): a pick outside Browsitory (or one before #522) can
  bring one in that no up-front check sees (libgit2's merge then records one name in the index
  and writes the other's file, so the pair is split between HEAD, the index and the disk).
  `restore_original_branch`
  (rebase abort) and `abort_merge`'s reset collect `worktree_guard::case_pairs_for_abort` (target,
  HEAD, index, and the names on disk below the folders they name), leave every name of a pair
  out of the forced checkout (a path-limited checkout of the other paths) and put them back with
  `restore_case_pairs`: the target's blob written raw, or the file removed. A name only on disk
  is removed only when its content is a blob the repository holds, else it is the user's and
  stays. The index is then written from the target tree (rebase) or HEAD (`abort_merge`).
  `plan_operation_reset` compares a pair name's file with its own index entry instead of
  libgit2's diff, which under the fold reports the other name's file as a change
  (`WouldDiscard`). A name the index lost to the other spelling is compared with HEAD's blob
  and the blobs of its other spellings, so an edit to it is refused rather than overwritten
  (#543). A sequence abort's rewind (`pick::rewind_to`, a safe checkout of the
  original commit) failed after such a step had landed ("1 conflict prevents checkout": HEAD
  holds the other name, and the original name's file is on disk); with pairs it checks out the
  other paths safely, writes the pair with `restore_case_pairs` and the index from the original
  tree (as `git reset --merge`), and refuses (`RewindBlocked`) first when a pair name's file
  holds neither HEAD's nor the original's content (#522).
- "Stash all" cannot clear links in between libgit2's record and reset, so with a hazard it goes
  through the path-limited stash (batch 4c) with every changed path: a file link is stashed and
  then replaced, a folder link is refused (`BeyondSymlink`).
- All of Browsitory's own raw work-tree I/O goes through `worktree_guard` (#149):
  `write_beneath` / `remove_beneath` (conflict resolution, abort merge's removal of added
  files, discarding an untracked file), `read_beneath` + `write_beneath` for **Ignore**'s
  `.gitignore` append, `clear_hazards`, `open_beneath` (image preview), and for undo/redo and the
  path-limited stash `read_beneath` (snapshots and checks; never below a symlinked folder, never
  a FIFO), `replace_beneath` (restore writes, each file replaced whole; a symlinked folder on
  the way is removed and recreated, never written through) and `remove_and_prune_beneath`
  (deletions and the folders they empty). `undo.rs` keeps only its mode logic.
- Those functions walk the path one component at a time and hold every folder on the way, so a
  folder swapped for a link at any moment can't redirect them. Unix
  (`worktree_guard/beneath_unix.rs`): `openat` with `O_NOFOLLOW | O_DIRECTORY` from the previous
  folder's descriptor, and the leaf acted on through its parent's descriptor (`unlinkat`,
  `openat(O_NOFOLLOW | O_NONBLOCK)` + `fstat`, `symlinkat`, `readlinkat`, `fchmod`). A write
  never truncates a FIFO or device; an existing file keeps its mode. Windows
  (`worktree_guard/beneath_windows.rs`): the work tree root is opened by path; every component
  below it and every leaf is opened (or created) by its single name relative to the held folder
  above it (`NtCreateFile` with `RootDirectory`, `nt_open`, #163, #193), *as itself*
  (`FILE_OPEN_REPARSE_POINT`), and folders are held without `FILE_SHARE_DELETE`, which pins
  them against rename and delete until the act is done; the handle's attributes and reparse tag
  (`GetFileInformationByHandleEx`) decide. A name surrogate (symbolic link, junction, WSL link)
  is refused, or removed itself where the caller clears links, and never contacted (a junction
  to a UNC share included); a link's target is read from its own handle
  (`FSCTL_GET_REPARSE_POINT`, as `read_link` would give it). Another reparse point
  (OneDrive/Cloud Files placeholder) is walked through as a folder, or as a file read and
  written through a second, normal handle (so the file system filter sees it), opened by its
  file ID (`FILE_OPEN_BY_FILE_ID`, so a link swapped in under the name isn't followed; by name
  only where the file system has no opens by ID: FAT/exFAT, which have no links, and SMB
  shares) and accepted only with the same file ID. Leaves are opened for reading with no more
  access than attributes, so a file whose ACL denies writing still reads; acts that may delete
  ask for `DELETE` and, where the ACL allows it, `FILE_WRITE_ATTRIBUTES`. A write classifies
  the leaf for reading and probes it for writing, but its rename replaces the old file, which
  needs `DELETE` on it (or `FILE_DELETE_CHILD` on the folder): a file whose ACL lets the user
  write but not delete it is no longer written (#239, see below). A reopen by ID must also still sit in the held parent (it may
  have been renamed out meanwhile). An *empty* held folder can still be turned into a junction
  in place; on a local NTFS/ReFS volume a relative open below it then fails with
  `ERROR_CANT_RESOLVE_FILENAME`, reported as "a folder of the working tree changed", so
  nothing is opened, created or contacted in the junction's target. On an SMB share the
  redirector resolves a relative name by path on the server, so every component and leaf must
  also sit in the held parent (`GetFinalPathNameByHandleW` of both), and a new file is deleted
  again through its handle if misplaced. A file with hard links has one final path, which may
  be any of its names (and is, after a reopen by ID): one in another folder then looked moved,
  so writing or reading such a tracked file failed with "a folder of the working tree changed"
  (#260). When the final path is elsewhere and the file has more than one name
  (`GetFileInformationByHandle`), its names are listed (`NtQueryInformationFile`,
  `FileHardLinkInformation`, parsed by `reparse::hard_links`), and one of them must be the leaf
  (ignoring case) in the held parent, the folder compared by its file index on the same volume
  (`name_in_parent`); a write then also takes the name's spelling from that entry. That still
  proves the handle is the file named so in the held folder; a folder never has hard links, so
  folders keep the final-path check alone, and where the names can't be listed (FAT/exFAT,
  which have no hard links, many SMB shares) the leaf is refused as before. A write's rename is resolved by path on the server
  too, so afterwards the file's final path is compared with the work tree root's: only when it
  positively lies outside the work tree is the file deleted through its handle and the write
  reported as "a folder of the working tree changed". By then the name it landed on outside
  has already been replaced, and that deletion removes it: the outside file is lost, as with
  any write the server redirected. When a path can't be read, or the file is inside the work
  tree (a sync tool may have moved it on at once), it is kept and the write succeeds, so the
  user's content is never deleted on a doubt (`settle_after_rename`). There the race is only
  narrowed. Leaves are deleted
  through their own handle (`FileDispositionInfoEx` with POSIX semantics, so the name is gone
  once our handle closes even while a scanner or indexer keeps the file open, #168, and with
  `IGNORE_READONLY_ATTRIBUTE`, so a read-only file is deleted as by std's `remove_file` and
  git, #194). Windows 10 1607–1803 don't know that flag (`ERROR_INVALID_PARAMETER`), so there
  POSIX semantics alone are tried, which refuse a read-only file with `ERROR_ACCESS_DENIED`.
  On access denied, and where the file system has no POSIX deletes (FAT/exFAT, some shares),
  it falls back to `FileDispositionInfo`, as std's `remove_dir_all` does; when that is denied
  on a read-only file, the attribute is cleared through the same handle (`FileBasicInfo`,
  settable attributes only) and the delete tried again, and the attribute is restored if it
  still fails (not possible when the ACL denies `FILE_WRITE_ATTRIBUTES`). Tests turn a held
  folder into a junction in place (`FSCTL_SET_REPARSE_POINT`) during a write, a read and a
  folder creation (#164, #193), and use a work tree through the admin share
  `\\localhost\C$\…` (SMB). Hand-declared `kernel32` and `ntdll` imports, no new dependency. A
  `Step` seam in both walkers makes the races deterministic in tests.
- `write_beneath` replaces the file whole and never truncates it first (#239). It writes a new
  temporary file `.{name}.browsitory-{pid}-{n}.tmp` (at most 64 bytes or UTF-16 units of the
  name) in the held parent, flushes it (`sync_all`) and renames it over the name in that same
  held folder; on any failure the temporary file is removed and the old file stays as it was.
  Unix: `fstatat` without following decides first: a folder, FIFO, socket or device is an
  error and is never opened; a regular file must open `O_WRONLY | O_NOFOLLOW | O_NONBLOCK`
  (no create, no truncate; the #209 read-only check runs on `EACCES`), and its mode
  (`& 0o7777`) is copied to the temporary file with `fchmod`. The temporary file is created
  `O_CREAT | O_EXCL | O_NOFOLLOW` (another name on `EEXIST`), then `renameat(parent, tmp,
  parent, name)` and a best-effort `fsync` of the parent. On a case-insensitive volume the
  name is the existing entry's own spelling (a `readdir` of the held parent: the exact name,
  else the single entry equal ignoring case, `fold_name`). Windows: the leaf is classified as
  before; an existing file must reopen for reading and writing by its file ID (read-only →
  #209), its settable attributes (`FileBasicInfo`) are copied to the temporary file, and its
  on-disk name comes from `GetFinalPathNameByHandleW` when that equals the leaf ignoring case
  (else the leaf as given: a short 8.3 name, or another name of a hardlinked file in the same
  folder), or for a hardlinked file whose final path is in another folder from its name entry
  in the held parent (#260, see above). The
  temporary file is created with
  `nt_open(FILE_CREATE)` in the held parent (checked with `in_parent`) and renamed through its
  own handle with `NtSetInformationFile(FileRenameInformationEx, REPLACE_IF_EXISTS |
  POSIX_SEMANTICS)` whose `RootDirectory` is the held parent, so the new name is resolved
  against that handle, never a path; POSIX semantics replace a file a scanner or the indexer
  keeps open with delete sharing. Where the class or flag is refused (`ERROR_INVALID_PARAMETER`,
  `ERROR_NOT_SUPPORTED`, `ERROR_INVALID_FUNCTION`: older Windows, FAT, some shares) the
  classic `FileRenameInformation` with `ReplaceIfExists` follows. A file another program holds
  open without sharing delete (some editors) can't be replaced: a sharing violation, or access
  denied while an open of it for deleting is refused as a sharing violation, becomes
  `worktree_guard::FileInUse` (`ResourceBusy`, tested with `is_in_use`), after three tries
  100 ms apart (a backup or sync tool that held it for a moment), as is a sharing violation
  of the probe for writing. Nothing is changed, and the temporary file is deleted through its
  handle. Conflict resolution and **Ignore** report it as `MergeError::FileInUse` /
  `StageError::FileInUse`: "'f.txt' is open in another program. Close it, then try again."
  What a replace doesn't keep: ACLs, extended attributes, alternate data streams, Windows
  encryption (EFS) and compression of the file itself (the new file takes the folder's), the
  owner (Unix: a group-writable file owned by someone else becomes the user's, as with git's
  checkout), and hardlinks (the other names keep the old content). Writing now also needs
  permission to create files in the folder, and on Windows permission to delete the old file
  (`DELETE` on it or `FILE_DELETE_CHILD` on the folder); without it the system's access
  denied is reported. A crash between creating and renaming the temporary file leaves
  it behind as an untracked file.
- libgit2's own checkouts hit the same held file on Windows and report it only as an OS error
  (#268, #303). On windows-latest it reads, for a file the checkout writes over and for one it
  deletes alike: "could not remove 'C:/Users/runneradmin/AppData/Local/Temp/.tmpJhLclX/z.txt':
  The process cannot access the file because it is being used by another process." The
  reason is in the system's language, and libgit2 words other failures differently (`failed
  remove - '<p>' is locked`, `could not find '<p>' to remove`, ...). So `worktree_guard` reads
  no wording: every `'` after an opening `'<work tree>/` (with or without a `\\?\` prefix) ends
  a candidate path below the work tree (no `..`, `.`, empty part or `:`), and the longest
  candidate the probe finds held is the file. The probe goes through the walker: it opens the file for
  deleting, sharing everything, and closes it at once; a sharing violation means another
  program holds it. Every other platform, and anything else, keeps libgit2's error. Checkouts
  of a path list (`held_open_after_checkout`: undo/redo from the index, discard, taking a
  whole side of a conflict) report the walker's `FileInUse` message; they also check each
  listed path when the message quotes none. Whole-tree checkouts (`held_open`, a `map_err`
  for any error type with `From<HeldOpen>`) report `HeldOpen { path, during }`, whose message
  says the files before it may already be written and how to finish: "'f.txt' is open in
  another program. Some files were already switched. Close it, then try again to finish." It
  covers branch switch and commit checkout, hard reset, merge fast-forward, rebase abort
  ("… put back. Close it, then abort again to finish."), clone ("Close it, then try again.":
  the clone's folder is removed, kind `FileInUse`) and submodule update. Each of these
  finishes when tried again: a forced checkout rewrites everything, and a safe one against a
  tree accepts a file already at its target.
- Operations whose retry would refuse or skip the files a stopped checkout already wrote roll
  them back instead (#321, `partial_checkout`), on every platform, when a checkout stops
  before it touched a file (Windows held file, read-only file or folder) or while it wrote one
  (a full disk, an I/O error; #329). libgit2 writes the index on disk only once a checkout has
  finished; until then only its in-memory copy holds the entries of the files written.
  `Written::record` registers two libgit2 callbacks on the checkout:
  - progress: each path the checkout wrote or removed, reported once done, so never the file
    it stopped at;
  - notify (`UPDATED`, called while libgit2 plans the checkout, before it writes anything):
    each path it means to write, with its `symlink_metadata` (size, mtime, kind) and how many
    of its folders exist. libgit2 opens a file with `O_TRUNC`, so a write that fails part-way
    leaves it truncated; a planned path that was never reported done but no longer matches
    its metadata is the file it stopped while writing, and is rolled back with the rest. A
    planned path it never reached (on Windows the held file itself) is left alone.

  The same two callbacks give a planned file with other hard links a name of its own before
  libgit2 writes anything (#420, see "Hardlinks" below); `Written::record` installs them
  through `hardlink::install`, so the two uses share one notify and one progress callback.
  Such a file is copied in the progress callback's "0 of n" baseline, after the notify calls
  recorded its metadata, so its planned entry is taken again right after the copy
  (`Written::restamp`): the copy's new mtime is not a write of the checkout's, and a file the
  checkout never reached is still left alone.

  Callers that check out a merge result with conflicts (cherry-pick, revert, rebase steps) also
  call `Written::expect_conflicts` with that index before the checkout: it lists, beside each
  conflict path, the entries named `<name>~*` and `<name>.lock`. libgit2 writes conflicts
  last; it writes a side of a file/folder or rename conflict as `<name>~<label>`
  (`f.txt~HEAD`, never over an existing entry) and reports only the conflict's path, and when
  writing a conflict's merged content fails it leaves its temporary `<name>.lock`, which would
  make the retry fail. Such an entry that is new after the stop is removed too. (A Windows
  held file or a read-only folder stops a checkout earlier: libgit2 removes every conflicted
  file before writing anything.)
  - Cherry-pick and revert (each step of a sequence too), rebase start (onto's checkout,
    before HEAD is detached) and each rebase step (#329), pull fast-forward, and stash apply
    and pop (both of the stash's checkouts, untracked files first): `roll_back` re-reads the
    index from disk and puts each written path back as it has it. A path the index has is
    checked out from it (forced, just those paths, after `prepare_forced_checkout` removed
    links in the way, so filters apply and nothing is written through a link); a path it lacks
    (a file the checkout created, a conflict's side file) is removed with
    `remove_and_prune_beneath_keeping`, unless the index has files below it. Pruning stops at
    the folders that were there before the checkout, so an empty folder the user had stays
    (#329). Every path is tried even after one fails (tracked files first, then created ones,
    then tracked ones again if a created one was in the way); the first error is reported.
    These are safe checkouts from that index (each refuses a staged change first), which write
    over a tracked file only when it matches the index, so no uncommitted change to a tracked
    file is lost, and unrelated edits stay. An ignored file in the way of a file the checkout
    creates is written over by libgit2 (git also treats ignored files as expendable), so the
    rollback removes it rather than bringing it back, by design; an ignored file at a path the
    checkout hadn't reached yet stays as it was. The message is #302's plain one,
    `HeldDuring::RolledBack`: "'f.txt' is open in another program. Close it, then try
    again." Other errors (read-only, full disk) keep libgit2's message. A pop keeps the stash.
  - A rebase step (`advance`, at start or on Continue) whose pick's checkout stops is rolled
    back like a cherry-pick, and its `CHERRY_PICK_HEAD`/`MERGE_MSG` removed: HEAD stays at the
    last step that landed (`expected_head`), the stop is `RebaseStop::Error`, and Continue
    applies the step again. During `start_rebase` the rollback to the branch
    (`restore_original_branch`) then starts from that clean tree; a held (or read-only) file
    the step stopped at is the same there as on the branch (the checkout of `onto`, or an
    earlier step, would have stopped at it first), so that checkout doesn't write it again.
  - Merge abort (also the cherry-pick and revert aborts, and a rebase abort that undoes its
    paused step) runs its forced checkout of HEAD's touched paths *before* it resets the
    index, so a stop leaves the index with what is still to abort. `roll_forward` then
    finishes the paths reached (the file it stopped while writing, and the files the abort
    removed itself, too): their work-tree files
    are checked out from HEAD again without touching the index (libgit2's forced checkout of a
    tree drops every conflict of the index, also outside its path list), then each one's
    index entry is set to HEAD's, conflicts included. `MERGE_HEAD` and the other state files
    stay, and aborting again finishes: `HeldDuring::Abort`, "… Some files were already put
    back. Close it, then abort again to finish.", or the plain message when nothing was put
    back yet. A file the operation added that can't be removed (Windows: held open) doesn't
    stop the rest of the abort; it is named the same way and stays for the next abort.
  - Rebase abort back to the branch (`restore_original_branch`, a forced checkout of the whole
    tree) needs no rollback: aborting again rewrites everything. `Written` only decides the
    message: "… Some files were already put back. …" (`HeldDuring::RebaseAbort`) once the
    checkout wrote something, the plain one before.
  - When a rollback itself fails, libgit2's error is reported unchanged.
- `replace_beneath` (undo/redo restores) goes through the same temporary file + rename (#251):
  both walkers share `swap_in` with `write_beneath` (Unix `swap_in_file` / `swap_in`: create,
  fill, `sync_all`, `renameat`, `unlinkat` of the temporary entry on any error, best-effort
  `fsync` of the folder; Windows `swap_in`: `create_temp`, fill, `sync_all`, the rename with the
  in-use retry and `on_wait`, `delete_by_handle` of the temporary file on error,
  `settle_after_rename`). A failed restore leaves the old file or link as it was. Unlike a
  write, a restore keeps nothing of the old file (D1-A): the mode comes from the restored entry
  (Unix: the umask's, exec bits set or cleared), Windows attributes are reset, and a read-only
  file is replaced (Windows: `FILE_RENAME_FLAG_IGNORE_READONLY_ATTRIBUTE` on the POSIX rename;
  where that flag is refused (1709–1803) the POSIX rename without it, and where POSIX renames
  are refused the classic one; such a rename that is denied clears the target's read-only
  attribute and tries once more, putting it back if that fails). Like the classic delete's
  (#194), that clear acts on the file, so a hardlinked file's other names lose the attribute
  too, and keep it lost once the rename replaced this name. It keeps the spelling of the name on
  disk. Unix: a link is made under a temporary name with `symlinkat` and renamed over the
  target; a FIFO, socket or device is replaced by the rename without being opened; an empty
  folder is removed first (`AT_REMOVEDIR`), a full one is an error. Windows: a link or an empty
  folder is deleted through its handle only once the new file is complete, just before the
  rename; the file is not probed for writing, so a file held without sharing delete (read-only
  or not) fails the rename and, after the retry, is `UndoError::FileInUse` ("'f.txt' is open in
  another program. Close it, then try again."). Atomic per file only: a restore of several
  files that fails part-way keeps the files already restored. Trying again finishes it:
  `WorktreeChange::verify` counts a path already at its target (content, mode and kind; absent
  when the target deletes it; or a folder when the target deletes the file there but has files
  below it, which `apply` then leaves) as unchanged, not as "changed since the operation", and
  `restore` accepts an index already equal to the target's (a stop in `checkout_from_index`,
  after the index was written). Every other path is still checked against the snapshot, so an
  edit made since the stop is still refused.
- `write_beneath` refuses an existing read-only file and leaves it as it is (#209): nothing is
  written and the read-only state is kept. Unix: an `EACCES` open of a regular file the user
  owns without owner write permission (`fstatat`, `st_uid == geteuid()`, no `S_IWUSR`). Windows:
  an access-denied reopen for writing of a file whose classifying handle has
  `FILE_ATTRIBUTE_READONLY`. Either way the error is `worktree_guard::read_only_error()`
  (`PermissionDenied` wrapping `ReadOnlyFile`, tested with `is_read_only`), which conflict
  resolution and **Ignore** report as `MergeError::ReadOnlyFile` / `StageError::ReadOnlyFile`
  ("'f.txt' is read-only. Make it writable, then try again."). `resolve_whole_file` writes
  through a forced libgit2 checkout, which would replace a read-only file, so it first asks
  `is_read_only_beneath` (the same rule through the walker, without opening the file: Unix
  `fstatat` in the held parent, so a 0o000 file counts; Windows the attributes of a no-access
  handle, so an online-only OneDrive file isn't downloaded) and refuses before anything
  is written. Any other denied write (an ACL, another owner) keeps the system's error. Deletes
  still clear the read-only attribute (#194), so the side that deleted the file still deletes
  it, and `replace_beneath` replaces a read-only file (undo/redo restores, like git) through
  its rename, see above.
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
  `d/new.txt` without a conflict and writes it through a folder link HEAD tracks. Merge,
  cherry-pick, revert and rebase steps pass `Incoming::MergeAsConflict` instead (#155): a folder
  link HEAD tracks, that the result keeps unchanged and that still points where HEAD's does on
  disk, is skipped, because files below it are a file/folder collision these operations write as
  git's directory/file conflict ("directory in the way of d from HEAD; moving it to d~HEAD"):
  `file_folder::check_out_as_conflicts` moves the link to `d~HEAD` (stage 2) and its safe
  checkout unlinks `d` before it creates the folder, or, when the link and the folder's entries
  sort next to each other, libgit2 records the conflict itself (`d` at stage 2, the link written
  as `d~HEAD`). A retargeted link is a local change that the picks' checkout (allowing
  conflicts) would keep and write through, so it is still refused. The guard
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
  pure pass over the index entries that folds names (NFD-normalized, as APFS/HFS+ compare
  names, so `é` as one or two code points is one name; without the HFS+ ignorable code points;
  each character lower-, upper- then lowercased, so `ẞ`, `ß` and `ss` meet; NFD again, so the
  fold is a fixed point for every scalar value, #166; NTFS doesn't normalize, so there it only
  refuses more) and compares folded
  link/gitlink paths with folded strict-prefix folders (and a link with a gitlink), both orders
  and exact duplicates, over all of an operation's incoming sources together; a file next to a
  folder is fine. It applies when `try_folds_case` (`core.ignorecase`, or always on
  macOS/Windows builds). A config it can't read, or a `core.ignorecase` that isn't a boolean,
  stops the operation with `CaseConfigUnreadable` ("Can't read core.ignorecase from the Git
  config: …"), not "case doesn't fold" (#225). `prepare_forced_checkout` reads the value
  strictly on every OS, before it removes any link, for every forced checkout, cleanups
  included (Abort, Reset hard, discards, undo): libgit2's checkout counts an invalid value as
  false, but its attribute and ignore parsing refuses it, so going on could remove the links
  and then stop half-way with HEAD unchanged. The user fixes the setting instead. Stash save,
  apply and pop read it strictly on every OS too, first, before any check or write (#369):
  stash save's forced reset removes the links it stashed without `prepare_forced_checkout`,
  and its path-limited route writes the stash commit and `refs/stash` before the reset, so a
  later refusal would leave a stash written and the worktree half reset. Drop only edits the
  stash list and doesn't read it. Worktree add also applies it when the destination folds case (#162): a worktree
  shares the main repository's config, whose `core.ignorecase` libgit2 probed where the main
  repository lives, so `worktree_guard::dir_folds_case` creates a hidden
  `.browsitory-case-probe-*` temp file in the nearest existing folder of the destination, looks
  it up with its ASCII case flipped and removes it (a failed probe leaves the config to
  decide). Only collisions an operation *brings in* are refused: `known_collisions` (every
  collision of HEAD's tree, plus `also_known`'s) are exempt with the identical `(link, folder)`
  spelling and an identical link entry, the same blob id and mode in every incoming source
  (#167; D9-A tightened toward D9-B: a retargeted link is a new link), so with HEAD holding `D` + `d/x` unrelated merges,
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
- **Worktree add creates missing parents and rolls back (#200).** libgit2's `git_worktree_add`
  creates only the last folder of the destination, so `create_worktree` first creates the
  missing parent folders, top-down, as `git worktree add` does. A folder it can't create (a file
  in the way, no permission) is `WorktreeError::CreateFolder`, naming that folder. When a later
  step fails, the call removes what it created: the parent folders (deepest first, only while
  empty), the destination (only while empty), `<commondir>/worktrees/<name>` when it didn't exist
  before (libgit2 creates it before the destination and leaves it behind on failure, which
  blocked a retry with the same name), and the branch when this call created it.
- **A worktree add that fails after `gitdir` is rolled back too (#245).** libgit2 writes `.git`,
  `commondir` and `gitdir`, then HEAD, then checks out, and cleans up nothing when a later step
  fails (a full disk, a name the file system refuses such as `aux` or `a.` on Windows, a path
  that's too long). The entry is ours when this call created it and its `gitdir` is missing or
  names `<destination>/.git` (canonicalized); an entry naming another folder is left alone. For
  ours, in this order: when the destination's `.git` file points at our entry (libgit2 writes it
  only into the folder it just created), `roll_back_destination` removes each file, link and
  submodule path of the branch's tree through `remove_and_prune_beneath` (no link followed,
  emptied folders pruned), then the tree's folders deepest first while empty (a file the
  checkout never wrote stops the pruning of its folder), then `.git`, then the destination if it
  is empty; then the entry (`remove_dir_all`), which frees the branch; then the new branch; then
  the parent folders. Tree paths go to the walker as raw bytes on Unix, so a name that isn't
  UTF-8 (possible on Linux) is removed like any other (#262); on Windows a tree name is always
  UTF-8, and one that isn't is skipped. A raw name that isn't exactly one plain component
  (empty, `.`, `..`, holding `/` or NUL; git never writes these) is skipped too. Every libgit2 error of the add is
  `WorktreeError::AddFailed` ("Couldn't create the worktree: {reason}.", `reason` = libgit2's
  message); when anything is left in the destination (which didn't exist when the call
  started), it is kept and the error is `AddFailedFolderLeft`, naming the folder. git does it
  differently: it deletes the whole destination and keeps the `-b` branch. There is no forced remove for a
  half-made worktree left by an older version; the user deletes the folder, prunes and deletes
  the branch. Residual risk: another process that creates the same `worktrees/<name>` between the
  existence check and libgit2's mkdir, or an empty destination folder after it, can have that
  folder removed by the rollback; a process that writes a file at one of the tree's paths in the
  new folder during the add loses that file.
- Residual risk: a concurrent local process can swap a link in between `clear_hazards` (or a
  refusal check: `scan`, `refuse_linked_folders`, `has_symlinked_ancestor` in stage/stash,
  `refuse_linked_path`) and libgit2's own write; libgit2 has no `openat`-style checkout. The
  same threat class as #138 (a local writer racing the app). Browsitory's own raw reads, writes
  and deletes in the work tree all go through the walker (above); what is left outside it is
  stat-only checks (the refusal checks listed here, undo's `symlink_metadata` / `read_dir`
  checks) and the external diff/merge tool, which writes `MERGED` by path itself, as git's
  `mergetool` does.
- Hardlinks: a tracked file that is a hardlink to a file outside the repository (or in another
  folder of it) gets the new content under its work-tree name only, and the other name keeps
  its content, as with git, whose checkout and `git apply` unlink the name and create a new
  file. Browsitory's own writes (`write_beneath`, `replace_beneath`: Save resolution,
  **Ignore**, Undo/Redo of a work-tree-only change) replace the name (#239), on Windows too
  (#260). libgit2 writes a file it checks out in place (`O_TRUNC`, the shared file changes), so
  before it writes, `hardlink` gives each file it is about to write that has more than one
  name a name of its own (#420): `worktree_guard::separate_hard_link` reads it and writes it
  back through `write_beneath`'s path (a new file renamed over the name, content and mode or
  Windows attributes kept), so a folder swapped for a symlink or junction is still refused
  and nothing is written through it. The link count is an `lstat`'s `st_nlink` (Windows:
  `nNumberOfLinks` of a handle opened on the name itself, attributes only, not following a
  reparse point). Where:
  - every checkout Browsitory configures (`hardlink::on_checkout`, or `Written::record`, see
    "partial checkout" above): Discard file, branch switch and detached checkout, hard reset,
    merges (fast-forward, merge, squash, taking a side), cherry-pick, revert and rebase steps,
    rebase start, pull fast-forward, stash apply and pop and the path-limited stash's reset,
    aborts, Undo/Redo of a path the index holds, the rollbacks' own checkouts, submodule
    checkout and update. The files come from the notify callback (`UPDATED`, sent while
    libgit2 plans the checkout for each file it will write, forced overwrites included) and
    are separated when the progress callback reports the "0 of n" baseline (no path), which
    libgit2 sends once the checkout is planned and before it removes or writes anything; an
    operation that runs several checkouts with one builder (stash apply) gets a baseline per
    checkout. A checkout refused while planning (a local change in the way) separates nothing,
    as git changes nothing then. Only planned files are looked at, one `lstat` each; no extra
    walk of the tree.
  - Discard hunk and Discard lines (`stage::apply_selection`, `hardlink::before_apply`):
    libgit2's apply checks out its result with options of its own, so the patch's work-tree
    paths are separated before it, once a `check` apply shows the patch applies (a stale
    selection changes nothing, links included).
  - Stash of everything (`stash_save2`): its reset takes no checkout options, so the tracked
    changed paths (one status, the same `changed_paths` the path-limited route uses) are
    separated before it (`hardlink::before_writing`).
  - Conflicted files written with markers go through libgit2's lock file, renamed over the
    name, which replaces the name already.
  Best effort: a file the copy can't write (read-only, held open on Windows, beyond a link) is
  left to libgit2's write as before. A fresh clone and a new worktree write into folders with
  nothing of the user's and take no hook. The copy is held in memory (libgit2 holds the blob it
  writes there too) and isn't atomic with libgit2's write: a process writing the file in
  between loses that write, as it would to libgit2's. Reads (Undo's snapshot, Mark resolved,
  the image preview, staging, which libgit2 does by path) read the file through either name. A
  hardlink is indistinguishable from a regular file by path; it needs the user (or a local
  process) to have created it on the same filesystem.
- Folder/link pairs in the index (#157): `stage_file` of `d/f` below a symlinked `d` removes
  `d/f`'s stage-0 entry (stages its deletion; git sees no file there) and reads nothing through
  the link; conflict entries there are still refused (`BeyondSymlink`), as are hunk and line
  staging. In the other direction (a real folder where HEAD or the index has a file or link,
  not a submodule) `stage_file` of the folder's name stages that file's deletion, so Stage all
  works in any order. `unstage_file` resets the pair as a unit: `pair_root` finds the outermost
  proper ancestor that is a non-folder in the index (a staged link or file that replaced the
  folder) or in HEAD (a folder that replaced a committed link or file), or else the path itself
  when it is a non-folder on one side and a folder on the other. The pair's index entries are
  removed first (libgit2 can't reset a root whose entries sit across the swap), then
  `reset_default` with the root restores HEAD's. Staged renames into the unit unstage with it;
  a rename source inside another staged pair is left out (its deletion belongs to that pair).
- A file in a folder's place (#154): `worktree_guard::file_in_place_of_folder` finds a proper
  ancestor that is now a regular file (links are left to the symlink checks).
  `discard_file` of a tracked path then refuses with `StageError::FolderIsNowAFile` (libgit2
  can't recreate the folder, and git would delete the file the user didn't pick);
  `discard_all_unstaged` handles untracked paths first, so confirming both the file and the
  folder's files succeeds. Discarding a tracked file that is now a folder refuses with
  `StageError::FolderHoldsUnrestorable` when the folder holds something the undo snapshot
  (`undo::build_worktree_tree`) leaves out: an ignored file or folder, or a nested repository.
  `discard_all_unstaged` checks this before discarding anything and leaves that folder's
  untracked files alone too. The path-limited stash refuses with
  `StashError::FolderIsNowAFile` before anything is recorded, unless the file is stashed too
  (the user picked `d`, or everything): then the revert removes it before checking `d/f` out,
  as `git stash push -u -- d` does (#186). Without untracked files it refuses with
  `UntrackedInTheWay` instead (see "git2 API gotchas"). This covers only the "file replaced a
  folder" direction: a selected path whose file a folder replaced (`d/x` where HEAD has the file
  `d`) is still refused with `DirectoryReplacedNonDirectory`, which loses nothing, although
  `git stash push -u -- d` accepts it.
- Conflicts below a pair (#187): when the index has conflict entries (stage 1–3) at or below the
  pair root, `unstage_file` refuses with `StageError::ConflictsBelow` before changing anything:
  libgit2's `reset_default` fails on them ("invalid entry mode"), and dropping them would
  discard a side of the merge. The pair's removals stay in memory (`reset_default` uses the same
  repository index and writes it once); a failed reset re-reads the index from disk.

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
  tool and, for the merge tool, waits on `off_runtime` (the blocking pool), so a tool window that stays open for
  hours never blocks the repo's command queue. Stale directories are swept after 24 hours.
- VSCode reads historical file contents through the `browsitory-file:` content provider, which
  calls the sidecar's internal `read_file_version`. No temp files are needed there. Before
  opening VSCode's merge editor the host calls the internal `check_merge_target`, the same
  guards `prepare_merge_tool` applies (text conflict, no symlink/submodule, work tree file
  inside the tree).
- Side files and `read_file_version` return raw blob bytes: git2 0.21 exposes neither
  `git_blob_filter` nor libgit2's filter lists, so CRLF/`ident`/filter-driver conversion is not
  applied (documented for users in USER_GUIDE).
- A commit's file tree (#446, `git-core::tree`): `get_commit_tree` lists one folder per call
  (the frontend loads folders as they open) or, with a filter, walks the whole tree for matching
  paths (at most `MAX_TREE_MATCHES`). `get_file_at_commit` sends text, or a sniffed image's
  bytes, up to `MAX_VIEW_BYTES` (judged from the object header), otherwise only the size.
  "Save as…" writes the raw blob (like `git show`) to a path the host's own save dialog picked:
  the Tauri command shows the dialog and then calls the worker; in VSCode the native
  `save_file_version` shows VSCode's dialog and calls the host-only sidecar method
  `write_file_version`, so the webview never names a destination. `restore_file_version` is a
  forced checkout of one path from the commit's tree with the index left alone, behind the same
  symlink and folder guards as a discard, and records a `Full` undo entry refused while the
  state lock stays busy (a discard).
- Runnable-file refusal checks one extension list on every OS (Wine runs Windows installers and
  shortcuts from `xdg-open`/`open`) plus content sniffing (`#!`, ELF, PE, Mach-O, OLE except
  old Office documents, and freedesktop launchers: shared-mime-info's x-desktop magics,
  `[Desktop Entry]` starting within the first 33 bytes, or `[Desktop Action`,
  `[KDE Desktop Entry]`, `# Config File` / `# KDE Config File` at offset 0; or a strict key file
  as GLib parses one (after comments, the first line a group header, then only group headers
  and `key=value` lines) with a `[Desktop Entry]`, `[KDE Desktop Entry]` or `[Desktop Action …]`
  group. Any other line ends the scan as "not a launcher", so a README quoting a launcher in a
  code block stays a document; a strict key file still undecided after 64 KiB counts as
  runnable; #165). The launcher sniff runs on Linux only (macOS and Windows have no
  `.desktop` launchers) and not for a name whose raw extension is in `DOCUMENT_EXTENSIONS`
  (`txt`, `md`, `json`, … each with a shared-mime-info glob already in 1.15, checked against
  `/usr/share/mime/globs2` on Linux CI): GLib never types a named file as a launcher from its
  contents and Qt sniffs only when no glob matches, so the glob wins before any magic. A link
  skips it only when both its name and its target's name are documents (Qt types a link by its
  name and its target's contents; #201). `.desktop` and `.kdelnk`
  are runnable by name. The head is read through the opened handle (`O_NONBLOCK | O_NOFOLLOW` on
  Unix, the type checked on the handle after a `symlink_metadata` pre-check, so a device node is
  never opened), so a FIFO swapped in after a check can't block. Mark resolved refuses a FIFO,
  socket or device in the conflicted path, then reads the file once through
  `worktree_guard::read_beneath` (no link followed, a FIFO never opened), checks those bytes for
  markers and stages exactly them: a file's blob through `blob_writer` with the path as hint (the
  same clean filters as `add_path`, also for a link kept as a file when `core.symlinks` is off),
  a link's target as is, then `Index::remove_path` (which moves the conflict stages to REUC, as
  `add_path` does) and `Index::add`. The entry carries the stat data (`FileStat`) of the
  descriptor the bytes were read from, taken before reading, so status trusts it like any entry
  `git add` wrote. Its device is `st_dev`, as the git CLI records it (libgit2 records
  `st_rdev`). libgit2's status never compares the device; git compares the device number only
  if it was compiled to use it and `core.checkStat` isn't `minimal` (its `core.checkStat`
  docs), and then `st_dev` matches. A link's entry carries the link's own stat data (`lstat`,
  taken before its target is read); on Windows that is zeroed, so status reads the link again
  on each refresh there (#225). With `core.filemode`/`core.symlinks` off the mode
  comes from stage 0, else 2, 3, 1, as libgit2's `index_existing_and_best`. libgit2's
  `add_path` would `lstat` and open the path a second time, following a link and blocking on a
  FIFO swapped in meanwhile (#199).
  A file or link it can't open is `MergeError::Unreadable`. A path that read as absent is
  judged again with `worktree_guard::probe_beneath`, through held folders, never by path:
  missing (the deletion is staged), beyond a link, a FIFO, or a folder, which it holds open
  (`HeldFolder`, with every folder above it up to the work tree root). A held folder is never
  reopened by path (#213): reads below it walk from its handle like `read_beneath`, each capped
  (4 KiB for `.git`, HEAD, `commondir` and a loose ref, 64 MiB for `packed-refs`), and
  `HeldFolder::resolve` follows a `gitdir:` or `commondir` one component at a time from held
  handles: a name is opened in the folder held before it (never a link), and `..` is the held
  folder above (physical, never applied to the path's text).

  The rule is simple: nothing outside the work tree is opened or looked up. Inside it,
  everything goes through held handles. A `gitdir:` or `commondir` is used only when it stays in
  the work tree:
  - a relative path whose `..` never climbs above the work tree root (a submodule's
    `../.git/modules/x`, a linked worktree's `commondir` of `../..` inside `.git`);
  - an absolute path below the work tree root exactly as it was opened, compared by component
    (so `/a/bc` is not inside `/a/b`); the rest is resolved from the held root the same way.

  Everything else is `NotStagedReason::GitFolderOutside`. Its message says the git folder is
  outside the work tree or at a path Browsitory can't match to it, and to stage the folder with
  `git add <path>` in a terminal. That covers:
  - an absolute path elsewhere, including Windows network shares and devices;
  - another spelling of the work tree (a symlinked or case-folded one, `\\?\`, an 8.3 name).
    git writes a linked worktree's `gitdir:` as an absolute real path, so a work tree opened
    through a link (macOS `/var` for `/private/var`) or under a short name lands here;
  - a relative path that climbs out of the work tree, even to come back in (git 2.48's
    `worktree.useRelativePaths` for a main repository outside);
  - a linked worktree of a repository outside (libgit2 and older git write absolute paths).

  A path with more than 64 names (normal components; `/`, a prefix, `.` and `..` don't count)
  is refused unopened as well.

  A nested repository is staged as a gitlink (`Index::add`, mode 160000) to the commit its
  HEAD names: `.git` (a folder, or a `gitdir:` file), HEAD, then a loose ref or `packed-refs`
  (in `commondir` for a linked worktree), symbolic refs followed as deep as git does
  (`SYMREF_MAXDEPTH`). A ref name is used only if git's `check_refname_format` would accept
  it, so `:` never reaches a Windows path (an NTFS alternate data stream). Otherwise it is
  `MergeError::FolderNotStaged` with a reason:
  - no repository: a plain folder, `.git` a link, or its `gitdir:` missing, a link or a file;
  - no commit: the branch is absent both as a loose ref and from `packed-refs`;
  - main repository missing: a linked worktree whose `commondir` names a folder that doesn't
    exist. git writes `commondir` as `../..` inside the git folder, so this only happens when
    it was edited; a moved main repository shows up as a missing `gitdir:` (no repository) or
    one outside the work tree instead;
  - git folder outside the work tree, as above;
  - a format it can't read: SHA-256 ids, reftable, a HEAD on a per-worktree ref, a ref name
    git refuses, a malformed or oversized file, a ref that exists but isn't a regular file (a
    link, folder or FIFO, or a link on the way to it), a `packed-refs` that isn't a regular
    file, a `commondir` that is a link or a file, or a path that is too deep.

  The messages for no repository, a git folder outside, a missing main repository and an
  unreadable format all end by saying to stage the folder with `git add <path>` in a terminal.

  `git add` stages a deletion when a folder on the way is a file, at any depth; so does Mark
  resolved (git twin tests). Discard file, hunk or lines below such a file names it
  (`StageError::FolderIsNowAFile`) and writes nothing; staging lines there only changes the
  index.
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
- **Fetch automatically (#440).** The app-wide interval is `[fetch] auto_fetch_minutes` in
  `config.toml` (0 = off, the default; `get_fetch_settings`/`set_fetch_settings`); a
  repository's override is a `localStorage` preference like Prune (`lib/autoFetch.ts`). The
  frontend schedules it (`state/useAutoFetch.ts`, active tab only) and calls
  `backgroundFetch`, which is `fetch_all` reported as `TransferOperation::BackgroundFetch`
  (`RemoteFailed` carries the operation too), so the tab's transfer session
  (`lib/transferSession.ts`) never shows or adopts it. `WorkerHandle::background_fetch` starts
  nothing and answers `null` while the worker's `CancelRegistry` knows any operation
  (`allocate_if_idle`: a fetch, pull, push, commit, merge, … queued or running), so it never
  overlaps another fetch or a mutation from any tab or spelling. The frontend waits while its own action runs or the host is hidden
  (`ipc/hostVisibility.ts`: the page's `visibilitychange`, plus the VSCode panel's
  `onDidChangeViewState`, posted by the extension host as a `panelVisibility` notification), and
  cancels a running background fetch when the user starts an action. Fetches never prompt for
  credentials (the provider only reads the OS store and the SSH agent), but reading the OS store
  can make the OS prompt (a locked keyring; macOS for an unsigned build). So a background fetch
  uses `RemoteCredentialProvider::without_prompt`: a token held in the process-wide
  `CredentialSession` (from a read or save this session), else the store only once something the
  user did read or wrote it successfully, else the remote fails as `MissingCredential` without
  touching the store. A remote failing with
  `MissingCredential`, `CredentialStoreFailure` or `SshAgentFailure` is named once per session
  in a toast. Like every fetch it records no undo entry.
- **No transfer holds the worker past a cancel while it connects (#462, #547).** A fetch or
  push blocked connecting calls no libgit2 callback, so the `CancelRegistry` side channel can't
  reach it, and libgit2's connect/read timeouts (`git2::opts`) are process-wide, so they are
  left alone. Fetch, Fetch all, the background fetch, Pull's fetch and every push-shaped command
  (push, Push to…, force push, Push tags, delete remote branch/tag) therefore run their git-core
  call on a helper thread with its own `Repository` and credential provider while the worker
  polls it (`worker/transfer_thread.rs`). git-core connects first (`connect_auth`; the fetch or
  push then reuses the connection) and asks `TransferReporter::connected` before downloading,
  sending, running `pre-push` or writing anything. While the helper is still connecting, the
  worker gives up on it on a cancel (the transfer ends `Cancelled` at once) or, for the
  background fetch only, after `BACKGROUND_CONNECT_LIMIT` (30 s): under one lock it marks the
  transfer abandoned, so `connected` answers `false` and the helper, when the OS finally lets
  it go, writes nothing. Once connected the worker waits as for any transfer; the callbacks see
  a cancel. Otherwise the worker always waits for the helper, so a helper's writes never overlap
  another command; Pull's fast-forward and `post-merge` run on the worker after it. Helpers
  report progress through the worker (stamped with the operation's ID), never through the
  caller's event channel, so an abandoned one keeps no listener open. Manual Fetch all and the
  background fetch share one loop (`worker/fetch_all.rs`). A remote whose connect failed at the
  network level (`RemoteError::Unreachable`: the host didn't resolve or accept the connection)
  or ran out of time is skipped by later background fetches for twice the interval, doubling up
  to an hour (`Memory`; `backgroundFetch` passes the interval); a remote whose abandoned helper
  still waits is skipped too, so at most one per remote lingers. A skipped remote is a quiet
  `RemoteFailed` (`TransferFailed`). A fetch that connects, manual or background, ends the
  backoff.
- Push and force push of the current branch go where `git-core::remote::push_target` resolves
  from `push.default` (unset = `simple`): `upstream`/`tracking` push to the upstream's branch,
  `current` to the local name, `simple`/`matching` only when the names match, `nothing` never.
  An undecided target is `PushTarget::NeedsChoice`: the backend refuses before connecting and the
  UI opens "Push to…" instead (`UpstreamInfo.push_branch` = null). The force-push lease reads the
  same target's tracking ref, and the UI only offers "Force push (with lease)" after a rejected
  push whose target equals that resolution.

## Desktop app folders and the bundle identifier

The bundle identifier in `crates/tauri-app/tauri.conf.json` is `com.browsitory.desktop` (up to
0.3.x it was `com.browsitory.app`, #433). It names the per-user folders Tauri and the webviews
use: the log folder (`<local data>/<id>/logs`, `~/Library/Logs/<id>` on macOS) and the webview's
storage, which holds the frontend's `localStorage` (`<local data>/<id>` on Linux and Windows,
`~/Library/WebKit/<id>` on macOS). On macOS it is also the bundle ID, the app's identity for
Gatekeeper, TCC folder permissions and the keychain. It does not name the `config` crate's
`config.toml` (recent repositories, workspaces, settings; `directories::ProjectDirs`
`("com", "browsitory", "Browsitory")`) or the keychain service names in
`repo-service/src/credentials.rs` (`com.browsitory.git`, `com.browsitory.forge`, shared with the
VSCode extension). The Windows installers don't derive their upgrade identity from it either: the
MSI `UpgradeCode` comes from the product name, and the NSIS registry keys from the product name
and the manufacturer (the identifier's second segment, `browsitory`, for both identifiers).

`crates/tauri-app/src/identifier_migration.rs` moves the old folders on the first start after the
change. It is the first plugin registered, because plugins are set up in registration order and
the log plugin opens its file in its setup; the webview is created later. In each folder Tauri
resolves identifier folders under (data, local data, config, cache, and on macOS
`~/Library/{Logs,WebKit,HTTPStorages}`) it renames `com.browsitory.app` to the new identifier,
only when the new folder doesn't exist yet. A rename is atomic and deletes nothing; a failure is
logged once the log plugin is up, and the app starts with fresh folders. Don't change the
identifier again without extending this migration.

## Error handling

`git-core` functions return typed errors (`thiserror` enums per module: `RepoError`,
`StatusError`, ...). The worker answers every request with the shared envelope
`repo_service::wire::ErrorDto` (`{ kind, message }`, #496), and decides its kind where the error
is made (#527): the reply channels carry `Result<T, ErrorDto>`, and a handler turns a typed error
into one with `ErrorDto::from_error`, which reads the kind from the error's type along its
`source()` chain (`RemoteError::DirtyWorktree`, `MergeError::NotATextConflict`,
`SearchError::InvalidQuery`, a credential store failure, an OS permission error), or builds it
with a constructor (`ErrorDto::worker_stopped`, `ErrorDto::pull_failed`). Every `WorkerHandle`
method goes through `WorkerHandle::request`, so a stopped worker always answers `WorkerStopped`.
Text (`ErrorDto::from(String)`) is always `Other`: nothing reads the kind from a message.
Tauri commands return `Result<T, ErrorDto>` (Tauri sends `Err` as the rejected promise's
value), and the sidecar puts it in the JSON-RPC error's `data`, next to the same `message`.
`ErrorKind` lists only the failures the frontend tells apart: a new one needs a kind there and a
typed producer, not a text check.

`PermissionDenied` comes from `git_core::os_error::is_permission_denied`: an `io::Error` of that
kind, or a libgit2 `Os` error. libgit2 keeps no error number, so the latter is recognised by the
system's own text for `EACCES`/`EPERM` (`ERROR_ACCESS_DENIED` on Windows) at the end of its
message, as `io::Error` renders it.

Errors Tauri raises before a command runs (an argument its generated wrapper can't deserialise,
an unknown command, an ACL refusal) stay bare strings: they go through Tauri's own
`InvokeResolver`, whose responder an app can't wrap. The frontend reads them as `Other`
(`toRepoError`).

`RepoClient` methods reject with `RepoError` (`frontend/src/ipc/RepoClient.ts`): the message,
whose `String()` is the message alone, and the `kind`. Frontend code switches on the kind
(`errorKind(err)`) and never on the message text. `RepoErrorKind` adds `TransportLost`, which
only the VSCode client sets: the extension host's bridge synthesizes `-32001` JSON-RPC errors
for requests interrupted by sidecar loss, and its separate `transportStatus` notification
rejects any still-pending webview promises and presents the diagnostic globally; it does not
extend the transport-neutral `RepoClient` interface. No error is swallowed at the boundary.

In the frontend, a failed action's message lives in `AppState.error` (the banner) until it is
dismissed or the next change starts (#177). Every `useMutationRunner` variant clears it in the
same update that sets `pending`, and transfers and pulls clear it when they start. Reads never
clear another action's error: `refresh()`, selecting a reflog reference and listing pull
requests each clear only the message their own earlier failure set (`state/ownError.ts`).
Without that rule, a mutation's trailing `refresh()` that overlapped a later, failing command
(Stage all, then Commit and Cancel on the serial worker) wiped the later command's error.
`dismissError` also clears the force-push offer and the hook failure; starting a mutation clears
only `error`.

## Testing strategy

- `git-core`/`config`: `cargo test`, real temp-dir repos/files, no mocks.
- `repo-service`: inline unit tests next to the code they cover (worker, credential store, and
  forge/PR API logic), including the worker's tests (`crates/repo-service/src/worker/tests/`, one
  file per area), which spawn a real worker thread against a real temp-dir repo — no mocks. The wire format of its DTOs is
  pinned by the wire snapshots and the generated DTO types below.
- `tauri-app`: now a thin Tauri command adapter over `repo-service`, so it doesn't need
  delegation tests of its own. Its tests (`crates/tauri-app/src/commands/tests/`) still
  check the serialization of DTOs as the desktop app sends them (e.g. camelCase field names on
  `WorkspaceDto`/`OpenRepoEntryDto`), although those DTOs now live in `repo_service::wire`.
- Hermetic git config: every Rust test binary (`git-core`, `repo-service`, `vscode-sidecar`,
  `tauri-app`) invokes `test_support::hermetic_git!()` (dev-only `crates/test-support`), which
  before `main` points libgit2's global/XDG/system search paths at an empty directory and sets
  `HOME`/`XDG_CONFIG_HOME`/`GIT_CONFIG_NOSYSTEM`/`GIT_CONFIG_GLOBAL` for spawned `git`, hooks
  and the sidecar binary, so the developer's or CI runner's own git config never leaks in. A
  test that needs a global config builds its own `git2::Config` or passes `HOME` to a child —
  never moves the process-wide search path. New test binaries must include the macro (a
  `tests/common` module counts).
  The guard also removes every inherited `GIT_*` variable (a hook's or `git rebase -x`'s
  `GIT_DIR`/`GIT_INDEX_FILE`/`GIT_WORK_TREE`, `GIT_CONFIG_PARAMETERS`, `GIT_AUTHOR_*`, …) and
  sets an empty `GNUPGHOME` and `LC_ALL=C` (#282).
- git CLI twins: a test that runs the `git` CLI starts it with `test_support::git_command(dir)`
  or `test_support::git(dir, args)`, not `Command::new("git")` (#282). Only a bare
  `git --version` availability check may skip the helper. The helper applies the same
  isolation to the command itself (it removes inherited `GIT_*` variables and sets the hermetic
  `HOME`, `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_NOSYSTEM=1`, `GNUPGHOME` and `LC_ALL=C`), so it holds
  even where a test changed the environment.
- Lock-wait races: a test that changes the repository while a worker waits for the state lock
  uses `Worker::spawn_with_pause_on_lock_wait` (test-only) and `PauseControl`, not a fixed
  sleep: the worker stops when it first finds the lock busy, and its wait starts after `go`
  (#274).
- `frontend`: Vitest + Testing Library, mocking `RepoClient` (a real interface seam). Tests
  build the fake with `frontend/src/test/fakeRepoClient.ts` (#499): a `Proxy` that answers the
  test's overrides and fails every other call with `not used in this test: <method>`, or in
  `"empty"` mode answers an empty value from its `EMPTY_RESULTS` table. A new `RepoClient`
  method needs no edit in the tests. `useAppState` tests use `transferClient` in
  `frontend/src/state/useAppStateTestClient.ts`, which adds an empty repository tracking
  `origin/main` on top.
- Transport parity (#306): `frontend/src/ipc/transportParity.test.ts` calls every method of
  both `RepoClient` implementations with a mocked transport and records the backend name and
  params each sends; the params must match for every method both send (#504). It checks those names against four generated lists in `frontend/src/ipc/generated/`:
  the Tauri commands (`tauri-commands.json`), the sidecar methods (`sidecar-methods.json`),
  the methods the extension host answers itself (`extension-native-methods.json`) and the
  sidecar methods only the extension host may call (`extension-host-sidecar-methods.json`).
  Each list comes from one declaration in code (the `commands!` macro in
  `crates/tauri-app/src/main.rs`, the `methods!` macro in `crates/vscode-sidecar/src/dispatch.rs`,
  `NATIVE_METHODS` and `HOST_SIDECAR_METHODS` in `extension/src/sidecarBridge.ts`), and that
  code's own test suite fails when the checked-in file is out of date (rerun the failing test
  with `UPDATE_TRANSPORT_METHODS=1` to rewrite it; never in CI). Intended differences between the transports are explicit, commented allow-lists in the
  parity test. A new operation must reach both transports, or be added to an allow-list with a
  reason.
- DTO types (#306): the wire DTOs derive ts-rs's `TS` in test builds only. The shared ones in
  `repo_service::wire` use `#[cfg_attr(any(test, feature = "ts"), derive(ts_rs::TS))]`: that
  crate's own tests derive it, and its `ts` feature, enabled only by the transports'
  dev-dependencies, lets their tests use the shared types too. ts-rs is never a normal
  dependency of the app or the sidecar (`cargo tree -e normal` shows it absent). The
  transport-specific DTOs use `#[cfg_attr(test, ...)]`. Three tests write the generated files
  and fail when a checked-in one is out of date (`UPDATE_TRANSPORT_METHODS=1` rewrites it):
  `wire_bindings_are_current` (`crates/repo-service/src/wire.rs`) writes
  `frontend/src/ipc/generated/wire-dto.ts`, and `dto_bindings_are_current`
  (`crates/tauri-app/src/commands/mod.rs`, `crates/vscode-sidecar/src/dispatch/tests.rs`) writes the
  transport-specific `tauri-dto.ts` and `sidecar-dto.ts`. They also fail when a listed DTO uses
  a type that is neither listed nor imported (`sidecar-dto.ts` imports `OpenRepoEntry` from
  `wire-dto.ts`). `frontend/src/ipc/dtoContract.ts` then requires each
  generated type to equal its hand-written type in `RepoClient.ts`, so `pnpm build` fails when a
  Rust field is renamed, retyped or made optional. The few intended differences are written out
  there. `dtoContract.test.ts` checks that every generated type is compared, and that no type
  name is generated in two files.
  - Limits and defaults (#497): `wire_bindings_are_current` also writes
    `frontend/src/ipc/generated/wire-constants.ts`: `MAX_CONTEXT_LINES`,
    `WHOLE_FILE_CONTEXT_LINES`, `COMMIT_MESSAGE_COLUMNS`, `CODE_FONT_SIZES`, `TAB_SIZES`, the
    `Default` of `config`'s `DiffSettings`, `CommitMessageSettings`, `FetchSettings`,
    `AppearanceSettings` (#615) and `UpdateSettings` (#630) serialized as their DTOs, and the
    commit search keys (`git_core::search::KEYS`). `RepoClient.ts` re-exports them; the frontend
    declares none of these values itself. `searchSyntax.test.ts` checks the search help's keys
    against `SEARCH_KEYS`.
  - Fields with a fixed set of names hold the `Serialize` enum itself (see "Wire DTOs"), so
    ts-rs generates their string-literal union. The `git-core`/`repo-service` enums derive
    `TS` behind the same `ts` feature (enabled only by dev-dependencies).
  - ts-rs reads the serde attributes, but it can't tell a `skip_serializing_if` field (sent
    only sometimes) from one that is always sent: mark it `ts(optional)`. On input, serde reads
    any missing `Option` field as `None`; the generated types describe what the frontend sends.
  - Not covered: answers that are plain strings, lists of strings or `null` (such as
    `get_commit_message` or `list_recent_repos`); the JSON-RPC notification envelopes (their
    `params` are the checked `TransferProgress`/`HookProgress`); and `read_file_version`'s
    `{ text }`, which only the extension host reads. The desktop app's `list_open_repos` returns
    a tuple, which `tauriRepoClient.ts` reshapes, so only its entries are compared.
- Wire snapshots (#306): `wire_snapshots_are_current` in
  `crates/tauri-app/src/commands/wire_snapshots.rs` and
  `crates/vscode-sidecar/src/dispatch/wire_snapshots.rs` builds every output DTO from the same
  domain values (`crates/test-support/wire_fixtures.rs`, included by both with `#[path]`),
  through the same `From` impls and helpers the commands and handlers call, and
  compares the exact bytes with `crates/*/tests/wire/dto-snapshots.txt`, one
  `<dto>/<case> <json>` line per case. The fixtures cover every enum variant and both `None`
  and `Some` of every optional field: a fieldless enum's list and a data-carrying enum's
  `assert_every_variant!` patterns use an exhaustive `match`, so a new variant fails to compile,
  and a pattern without a case fails the test. The desktop file also pins the Tauri event
  names.
  - Encoding: Tauri sends `serde_json::to_string` of the value (command errors go through a
    `Value` first); the sidecar always goes through `serde_json::Value`, which sorts keys. The
    snapshot tests model this in a `wire()` function. For the sidecar it is checked end to
    end: `the_progress_relay_writes_the_snapshot_bytes` and
    `the_hook_relay_writes_the_snapshot_bytes` run the real relays into a buffer, and
    `crates/vscode-sidecar/tests/wire_dispatch.rs` compares the real binary's response lines
    with snapshot lines (`get_status`) and pins `read_file_version`'s `{ text }`. The Tauri side
    is not checked end to end (that would need a running webview).
  - `crates/vscode-sidecar/tests/wire_dispatch.rs` also pins the response line of every
    sidecar method called with no `params`, `{}` and `"x"`
    (`tests/wire/dispatch-responses.txt`): the missing-field and wrong-type error of every
    params struct, the JSON-RPC error encoding, and the answers of the methods that take no
    params, with `BROWSITORY_CONFIG_DIR` set to an empty temp dir.
  - Sidecar params: each arm of the `methods!` table names its handler's params struct, as in
    `"get_status" => get_status(RepoPathParams, repos)`. The macro reads the request's `params`
    into it with `serde_json::from_value` and answers a failure with the error's text, so
    handlers take the typed struct. That text names the struct (`expected struct
    RepoPathParams`), so renaming a params struct changes the wire. An arm with no params type,
    as in `list_recent_repos()`, ignores the request's `params`. `tests/params_errors.rs` pins
    more of this through the binary: wrong field types, a later or nested missing field, an
    unknown enum value, extra fields and positional arrays (accepted), and methods without
    params accepting anything. The params structs are not shared with the Tauri commands and
    have no generated TypeScript.
  - `shared_dtos_match_across_transports` (sidecar) requires every case of a shared DTO in both
    transports (transport-only DTOs are listed with reasons) and pins the cases whose bytes
    differ in `cross-transport-differences.txt`. Both transports build the shared DTOs with the
    same `repo_service::wire` code, so these differences come only from the serialization path
    (`key-order`).
  - Not covered: input-only DTOs, JSON-RPC envelopes other than those above, and the Tauri
    encoding path itself. Regenerate with `UPDATE_TRANSPORT_METHODS=1`, desktop app first.
- `extension`: Vitest with fake child-process and VSCode boundaries for JSON-RPC framing,
  native-method routing, process-loss recovery, webview security, and deterministic disposal.
- E2E: `tauri-driver` + WebdriverIO (`e2e/`) driving
  the built `tauri-app` binary as a black box — `cargo build --workspace --features
  tauri-app/custom-protocol` (the `custom-protocol` feature makes the binary load its embedded
  `frontend/dist` instead of the Vite dev server; plain `cargo build` stays in dev-server mode
  regardless of debug/release, so it can't be driven standalone). One flow per major feature
  area, added where a flow spans backend+frontend in a way unit tests can't catch; `App`
  (`frontend/src/state/useE2eAutoOpen.ts`) reads a `VITE_E2E_REPO_PATH` build-time env var to auto-open a fixture repo, since
  `RepoPicker`'s native folder dialog can't be driven through WebDriver. The same variable
  turns off the release notes dialog (`useReleaseNotes`) and the automatic update check
  (`UpdateBanner`), so E2E runs and the guide screenshots never contact the update endpoints or
  download a release bundle (#576); release builds don't set it. Outside E2E builds the automatic
  check runs only once `[updates] check_automatically` (`get_update_settings`, default on, #630)
  has been read as on; a failed read leaves it off. "Check for updates" (palette and the
  Preferences "Updates" section, both shown only when `main.tsx` passes the `updateBanner`, so
  never in VSCode) runs the check the banner registers through `state/updateCheck.ts`, whatever
  the setting. (Not Playwright:
  Playwright drives browser engines it manages itself and can't attach to a native
  Tauri/webkit2gtk window; `tauri-driver` is Tauri's own WebDriver bridge, the
  actually-supported E2E path.) All four pnpm projects (`frontend/`, `extension/`, `e2e/`,
  `extension/e2e/`) pin the same `packageManager: "pnpm@12.18.0"` (#361), and the workflows'
  `pnpm/action-setup` steps name only the major (`version: 12`); pnpm switches to the pinned
  version itself, and each lockfile's first YAML document (the "env lockfile") records it with
  its integrity. A CI-vs-local pnpm major mismatch once broke `e2e/`'s frozen-lockfile
  install, so bump all four pins, the env lockfiles and the workflow majors together. pnpm 12
  runs no dependency build script unless the project's `pnpm-workspace.yaml` allows it
  (`allowBuilds`; an unlisted one fails the install): `extension/` allows `@vscode/vsce-sign`,
  `e2e/` allows `esbuild` and denies `edgedriver`/`geckodriver` (their postinstall only
  downloads a driver when `*_AUTO_INSTALL` is set); `frontend/` and `extension/e2e/` need none.
  pnpm's `minimumReleaseAge` default (1 day) applies to installs too: a lockfile entry
  published less than a day ago fails `--frozen-lockfile`.
  - Elements that unmount: waiting for a row, menu or dialog to disappear, or clicking one that
    may re-render, goes through `e2e/support/dom.ts` (`waitUntilGoneInPage`, `clickInPage`),
    which query and act inside one `browser.execute` (`waitUntilInPage` for the opposite wait).
    WebdriverIO's own `waitForExist({ reverse: true })` looks the element up and inspects it in
    separate round trips; once React unmounted it, WebKitWebDriver failed with a stale-element
    error on every retry until the wait timed out (#182), so retrying doesn't help.
    `waitForDisplayed({ reverse: true })` on an element that stays mounted is fine. After a
    commit made through the UI, wait on the repository (`e2e/support/git.ts`), not on a row
    whose text may already match something else.
  - Shared spec helpers (#500): specs run the git CLI only through `e2e/support/git.ts`
    (`git(cwd, ...args)` returns trimmed output and throws with git's stderr; `gitIn(repo)`
    binds a repository; `gitWith` keeps output untrimmed or discards it). The VSCode specs
    use the same runner (#524): it lives in `extension/e2e/src/support/git.ts` (Node built-ins
    only, for the same `rootDir` reason as `fixtureRepo.ts` below), and `e2e/support/git.ts`
    re-exports it next to its WebDriver waits. The child inherits
    the runner's environment, so the screenshot run's isolated HOME and git config reach it.
    `wdio.conf.ts` sets `waitforTimeout` to `UI_TIMEOUT` (`e2e/support/timeouts.ts`, 10 s), so
    a wait passes `timeout` only when it needs longer, as a named constant (`SLOW_UI_TIMEOUT`,
    `RESTART_TIMEOUT`, or a spec's own). `e2e/support/app.ts` holds `waitForAppReady` and the
    retrying `openPalette`. The fixture repository set-up (`initFixtureRepo`, the "Test User"
    identity) is one file, `extension/e2e/src/support/fixtureRepo.ts`, which both suites
    import: there because `extension/e2e/` compiles with `rootDir: "src"` and can't import
    from outside it, while `e2e/` runs TypeScript through tsx and can. Keep it to Node
    built-ins.
  - Harness hardening (#182): `wdio.conf.ts` starts `tauri-driver` in its own process group,
    which also holds WebKitWebDriver and the app, and records the group id in
    `target/e2e-tauri-driver.pgid`. `afterSession` SIGTERMs the group, waits up to 5 s and
    SIGKILLs it. `beforeSession` reaps (and logs) only what an aborted earlier session of this
    worktree left: that recorded group, this worktree's app binary running under WebDriver
    automation without its WebKitWebDriver, or a WebKitWebDriver on port 4445 without its
    tauri-driver (Linux, via `/proc`). A driver port still held by anything else fails the
    session with "port … is in use by another process" rather than being killed. It then waits
    for ports 4444 and 4445, and every session logs `[e2e] session start: N ms`. CI runs the
    suite under `dbus-run-session`: without a session bus each session start waited ~31 s.
  - Process tests (`git-core/src/process.rs`): each check that a timeout or Cancel kills a
    hook's background process has a negative control showing the check fails without the
    process group (Unix) or job object (Windows), and the `/dev/tty` check runs the test binary
    again under a pty it creates, so it discriminates without a terminal on CI (#179).
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
- Mutation testing (`ci.yml`'s `mutants` job, pull requests and dispatched runs only,
  informational): cargo-mutants mutates the lines a PR changes under `crates/git-core/src/` and `crates/repo-service/src/` and
  runs that package's tests against each mutant. A *missed* mutant is a code change no test
  noticed. It measures whether new tests check the behaviour or merely run the code — the risk
  when the same agent writes a test and the code it tests. It is not a required check and its
  steps never fail it. It posts the counts and the missed and timed-out mutants as one sticky
  PR comment (marked `<!-- mutants-report -->`, updated on re-runs; not for fork PRs or
  dispatched runs), repeats them in the run summary, and uploads the full `mutants.out` as an
  artifact. Agents triage that comment (see `AGENTS.md`'s "Testing conventions"). To
  reproduce locally: `git diff origin/main...HEAD > pr.diff && cargo mutants --in-diff pr.diff
  --package git-core --package repo-service`. Frontend and extension code are not covered.
- Removed assertions (`ci.yml`'s `removed-assertions` job, pull requests and dispatched runs
  only, informational, #310): `scripts/report-removed-assertions.py` reads the PR's diff and
  lists every assertion line it removes or rewrites in a test file (`assert*!` in `crates/`,
  `expect(`/`assert` in `*.test.*` and `*.spec.*`, asserts in `scripts/test_*.py`; only a
  multi-line assertion's first line). A changed assertion shows as removed and added. Like the
  mutants report it posts one sticky comment (`<!-- removed-assertions -->`, created only when
  something is found) and the run summary, and never fails. It covers what mutation testing
  can't see: a test loosened in the same PR as the code. Agents answer it in the PR (see
  `AGENTS.md`'s "Task workflow").
- Skipping unaffected jobs (`ci.yml`'s `changes` job): a pull request runs only the build and
  test work its changes can affect. The `changes` job diffs the PR's merge commit against its
  base and decides with two allowlists of paths known not to matter, so any other path runs
  everything:
  - No Rust: `frontend/` (except `frontend/src/ipc/generated/`, which Rust tests compare their
    DTOs against), `extension/`, `e2e/`, `CHANGELOG.md` and everything docs-only. The three
    `rust` jobs then skip their steps.
  - Docs only: `docs/` (except a file named in `frontend/`, `extension/`, `e2e/`, `crates/` or
    `scripts/`, such as `docs/guide/search.md`, which a frontend test imports), `changelog.d/`,
    `README.md`, `CONTRIBUTING.md`, `AGENTS.md`, `CLAUDE.md`, `PRIVACY.md` and `mkdocs.yml`.
    `rust`, `frontend`, `extension`, `e2e`, `e2e-vscode` and `vsix-smoke` then skip their steps;
    `audit`, `licenses` and `changelog` always run in full. `CHANGELOG.md` and
    `THIRD_PARTY_NOTICES.md` are not docs: the frontend and the packages read them.

  Those jobs are required checks, so they never skip themselves (a skipped matrix job reports
  one unexpanded check, and `rust (ubuntu-latest)` would never report): they run, skip every
  step and finish green in seconds. They skip only on an explicit "false" from `changes`, so if
  that job fails, everything runs. Pushes to main, the weekly schedule and dispatched runs
  always run everything. A new push to a pull request cancels that pull request's older run;
  other runs are never cancelled. A new test or build step that reads a file outside `crates/`
  (Rust) or under `docs/` (any job) must take that path off the matching allowlist.

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

For what shipped when, see `CHANGELOG.md`. For in-flight and planned work, see the GitHub
issues and the newest plans in `docs/superpowers/plans/`.
