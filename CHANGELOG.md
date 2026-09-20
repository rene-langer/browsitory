# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Security

- `resolve_conflict` now validates that the given path is an actual index conflict before
  writing to the working directory, closing a gap where any path (including one under `.git/`)
  would be written and staged unconditionally.
- Updated `rustls` to 0.23.45 to pick up the fix for RUSTSEC-2026-0285 (TLS 1.3 handshake
  messages accepted across encryption-level boundaries).

### Fixed

- Diff pane: a file's diff shows "Loading diff…" instead of a false "No differences" while it
  loads; empty diffs now read "No text differences (binary file or mode-only change)".
  Collapsed file sections no longer fetch their diff until expanded.
- Diff hunk actions are no longer tab stops. The diff is one tab stop with `[`/`]` (prev/next
  hunk), `s` (stage/unstage) and `d` (discard, press twice). The armed "Confirm Discard" is
  styled as danger and disarms on blur, Escape or after 5 seconds.
- Diff lines show old/new line-number gutters; long tokens wrap and hunk headers wrap cleanly
  at narrow widths.
- Commit dock is lighter (single border, no nested panel) and no longer lets diff content show
  beneath it.
- Sidebar section header buttons (such as the Branches "+") no longer sit under the sidebar's
  overlay scrollbar, which swallowed clicks on them once the sections overflowed; this also
  fixes the remote e2e specs that go through that button.
- Pulling a fast-forward no longer force-checks out over untracked or ignored files: the
  checkout is now safe and aborts untouched with a "would be overwritten" error, and the pull
  surfaces its dirty-worktree, no-upstream, detached-HEAD and checkout-conflict reasons instead
  of a generic "pull failed".
- `current_upstream` no longer errors on a detached HEAD (the normal state during an in-progress
  rebase); it returns "no upstream" instead. Previously this rejected the frontend's whole
  per-mutation state refresh, leaving the UI stuck showing stale pre-rebase content after a
  rebase paused on a conflict or was aborted.
- The VSCode extension's `vscode-sidecar` process now dispatches each JSON-RPC request on its
  own thread instead of one shared blocking loop, so a slow operation against one open repo (a
  large blame, a big commit graph) no longer freezes every other open repo's requests.
- `sidecarBridge.ts` no longer forwards a JSON-RPC response to the webview unless it actually
  matches a pending request.
- A worker thread that panics is now detected and evicted, so reopening the affected repository
  respawns a fresh worker instead of leaving the repository permanently stuck.
- A worker-thread failure now surfaces as "connection to this repository was lost, reopen it"
  instead of the raw internal error string.
- Remote-tracking branch rows in the Branches tree are now keyboard-focusable, and the shared
  `ContextMenu` widget implements standard menu keyboard navigation (arrow keys, Home/End,
  initial focus on open) instead of only closing on Escape.
- The commit-message textarea now has an explicit accessible name.
- The frontend's global error/rejection logging now goes through `RepoClient` instead of
  importing a Tauri-only plugin directly, so uncaught errors are actually logged from the
  VSCode webview too (previously silently dropped there).
- Pressing Enter or Space on a branch row's graph-visibility swatch or "..." button no longer
  checks the branch out; key events from controls nested in a row stay with that control.
- Branch tree rows stay on one line with truncating names (full name in the tooltip), the "+"
  button moved into the Branches header, and the graph-visibility swatch has a larger hit area
  and a hollow "hidden" state.
- Release notes no longer truncate a changelog bullet that wraps across lines, and wrapped diff
  lines get a hanging indent.
- The window no longer flashes blank while open repositories restore.
- Commit, Continue rebase and Abort rebase/merge buttons are pinned to the bottom of the diff pane
  instead of sitting below every file section, where a large change set pushed them thousands of
  pixels off screen.
- Modal dialogs (release notes, command palette, repo picker) open scrolled to the top instead of
  to whichever control received focus. The command palette also fills its dialog width, and the
  empty-state header, empty-state text and Uncommitted Changes row match the rest of the UI.

### Changed

- The transport-isolation ESLint rule now covers all of `frontend/src/**` (previously missed
  `src/lib/**`) and also bans `vscode` imports outside `vscodeRepoClient.ts`/`extension/`.

### Added

- Selecting a commit now shows a header with its full message, author, date, full SHA (copyable)
  and parents (click to jump), backed by a new `get_commit_message` IPC command wired through
  git-core, repo-service, tauri-app and the VSCode sidecar (UX-001). History rows also show
  author and date columns that hide when the pane is narrow.
- History pagination: the graph loads 300 commits at a time with a "Load more" row, and
  arrowing past the last row loads the next page instead of silently stopping (UX-002).
- Sync bar above the commit graph with Fetch, Pull and Push buttons and ahead/behind counts;
  `UpstreamInfo` gains optional `ahead`/`behind` (null until the tracking ref is fetched)
  (UX-008).
- "Checkout" is now the first item of a local branch's context menu; the user guide documents
  double-click checkout too (UX-003).
- The "Uncommitted Changes" row now has a hollow lane node and aligns with commit rows
  (VIS-004).
- Branch tree: local branches get an "Actions" button and a bold current-branch marker; inline
  new-branch/add-remote/rename forms focus on open and cancel on Escape; remote lists show
  "Loading..." and "No branches"; the Upstream block is styled, explains a disabled Pull, and
  confirms before clearing the upstream.
- Commit graph keyboard access: Enter, the Menu key or Shift+F10 open the commit menu; Home/End,
  PageUp/PageDown navigate; Shift+Arrow extends the squash range.
- Command palette groups commands by kind, uses the UI font size and shows key hints; the header
  shows a Ctrl/Cmd+K hint; the empty-state repository picker is a styled row list.
- BranchTree's edit-remote, credentials, set-upstream and diverged-pull dialogs now share a
  `FormDialog` primitive that focuses the first field.
- A desktop VSCode extension host now loads the shared React frontend in a restricted webview,
  routes folder/external-URL/version operations through native VSCode APIs, and forwards the
  remaining `RepoClient` surface to `vscode-sidecar` over JSON-RPC. Sidecar exit, process error,
  and stdin failure reject pending actions with an in-app diagnostic; a later action lazily
  starts one fresh process without replaying mutations.
- Target-specific VSIX packages now bundle the compiled extension, package-local webview assets,
  and exactly one matching `vscode-sidecar` binary for Linux x64, macOS x64/arm64, or Windows x64.
  Main-branch CI retains all four packages as artifacts, and tag releases attach them to the draft
  GitHub Release.
- A new `extension/e2e/` layer gives the VSCode extension its own black-box E2E coverage:
  `@vscode/test-electron` drives the unpacked extension inside a real Extension Development
  Host, and a hand-rolled raw Chrome DevTools Protocol client (no Playwright or other
  browser-automation library) attaches to the webview's nested content frame for DOM assertions.
  One flow so far — open repo → stage a file → commit → see it in history — mirroring `e2e/`'s
  existing pattern. Wired into CI as a new `e2e-vscode` job that gates `build-vsix`.
- The branch tree (local and each remote) now nests branches into folders split on each `/` in
  their name, matching a filesystem-style tree view. Checkout is now double-click instead of
  single-click (single click just selects/highlights a row); the graph-visibility control is now
  a per-branch colored swatch button instead of a checkbox.

### Changed

- Extracted the git worker/credential/forge service layer out of `tauri-app` into a new
  internal `repo-service` crate, in preparation for a future VSCode extension — no
  user-visible behavior change.
- `crates/vscode-sidecar` now wires the full `RepoClient` method surface (every method except
  the five VSCode-native ones — repo folder picking, app/last-seen version, and opening an
  external URL — which the extension host wires directly against VSCode APIs), including a
  transfer-progress JSON-RPC notification mechanism for fetch/push/pull.

### Fixed

- The commit graph's lane lines could render broken: a lane-shifting connector could be cut by
  a later-painted, unrelated pass-through line crossing it, a pass-through lane's straight
  continuation could be dropped entirely when another commit's connector happened to land on
  the same lane, and even once both were fixed, a straight pass-through line split into two
  segments at the row's midpoint could still show a hairline rendering seam. `CommitLaneGraphic`
  now paints connectors last and draws each non-own pass-through lane as a single unbroken line.

## [0.2.0] - 2026-08-29

### Added

- Failure logging: the backend now writes a rotated log file to the OS log
  directory (`tauri-plugin-log`), with a panic hook covering worker-thread
  crashes. The frontend routes every failed IPC call and any uncaught
  error/rejection into the same file via `@tauri-apps/plugin-log`, so a bug
  report doesn't require a live dev session to diagnose.
- A gear popover in the sidebar lets each panel (Stashes, Worktrees,
  Submodules, Reflog, Tags, Pull Requests) be shown or hidden, persisted
  globally (one `localStorage` setting shared across every repo and open
  tab) — clutter from features a project doesn't use can be tucked
  away.
- Each branch's context menu in the Branches tree gained "Isolate branch",
  a one-click way to filter the commit graph down to just that branch; the
  tree's "+" menu gained "Show all branches" to clear the filter.

### Changed

- The diff view now renders every changed file's diff expanded by default,
  Sublime Merge-style, instead of a file list where each row had to be
  clicked to reveal its diff. Each file's header carries an inline
  Stage/Unstage control and a Blame button, plus a per-file collapse
  chevron; a "Collapse all"/"Expand all" toggle gives an overview of a
  large changeset.
- HTTPS credential-store failures now retain a safe diagnostic in the
  application log while keeping the on-screen message generic.
- Stashes moved out of the Branches sidebar section into their own
  "Stashes" accordion section, with a confirmation before dropping one.
- Every remaining sidebar section (Worktrees, Submodules, Reflog, Remotes,
  Tags, Pull Requests) reskinned to the Phase 5 design system: consistent
  row styling, per-row icons, and empty states.
- The separate branch-switcher dropdown and Remotes accordion were
  replaced by a single, always-expanded Branches tree: local branches and
  one folder per remote, with every mutating action (create/rename/delete
  a branch, merge, add/edit/remove a remote, set upstream, manage
  credentials, push/fetch) moved to right-click context menus and the
  command palette.

### Fixed

- `config.toml` writes are now atomic (write-temp-then-rename) instead of
  an in-place `fs::write`, which could interleave with a concurrent save
  and leave corrupt trailing bytes from the prior write.
- The workspaces E2E spec's Edit/Delete steps now allow up to 45s for the
  picker to reload its workspace list after a full app restart, matching
  the CI-runner contention already documented for this suite's repo-scan
  wait — the previous 10s budget was flaking on `main`.
- `abort_rebase_restores_the_original_branch_and_tip_exactly` no longer
  asserts against the pre-rebase commit's OID, which only matched by
  coincidence when two independently-generated committer timestamps
  landed in the same second; it was flaking on the `windows-latest` CI
  runner. Also: the `rust` CI job's macOS/Windows legs now run on every
  pull request instead of only on pushes to `main`, so a platform-specific
  regression like this one is caught before merge.
