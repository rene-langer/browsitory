# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Interface refresh

- Calmer, editor-like look: a new charcoal dark palette and warm-neutral light palette with a
  single amber accent, flat borderless controls, hairline pane dividers and thin scrollbars.
- Slim title bar with editor-style tabs that sit flush on the workspace; a tab's close button
  only shows on the active or hovered tab. The multicolor strip under the header is gone.
- Sidebar section headings are small uppercase labels; secondary actions (Fetch/Pull/Push, Blame,
  History, diff view options, hunk actions, Stage all) are quiet text buttons that fill on hover.
- The selected changed file is marked by an accent edge instead of a tinted block; Commit is the
  one accent-filled button. Commit summaries use the UI font; branch labels are flat chips.
- The command palette and other overlays open near the top of the window.
- Denser branch tree: folders and branches share one even 22px row height with no gaps between
  folders, a tighter indent, and each branch's "…" menu shows on hover or focus.

## [0.3.0] - 2026-10-21

### Commit signing

- Commit signing: when `commit.gpgSign` is set, commits, amends, merges, cherry-picks, reverts and
  rebased commits are signed with gpg (`gpg.format=openpgp`) or SSH (`gpg.format=ssh`), using
  your existing gpg-agent / ssh-agent. If signing fails, no commit is made.
- The commit details pane shows the signature status of signed commits (good, untrusted, unknown
  key, expired, bad, revoked).

### Undo and redo

- **Ctrl/Cmd+Z** undoes the last Git operation in the active repository and **Ctrl/Cmd+Shift+Z**
  (or Ctrl+Y) redoes it, also from the undo/redo buttons next to Fetch/Pull/Push, the command
  palette, and the toast's Undo/Redo button. Covers staging, commits, amend, reset, branch and
  tag changes, checkout, cherry-pick, revert, merge, rebase, pull and every discard (discarded
  changes are now recoverable). Up to 50 steps, kept across restarts.
- Undo refuses, instead of overwriting, when the repository changed since the operation; the
  reflog stays available for those cases.
- Undo also refuses while a branch it would delete, move or switch to is checked out in another
  worktree, and keeps line endings right with `core.autocrlf` and other checkout filters.

### Clone and new repository

- **Clone…** in the repository picker and a "Clone repository…" palette command (desktop app):
  clone from HTTPS, SSH or a local path with progress and cancel, an optional branch, and
  submodules. An HTTPS token entered in the dialog is saved to the keychain only after a
  successful clone.
- **New repository…** creates an empty repository in a folder you pick and opens it.

### File history

- **History** button on every tracked file (working tree and commit view) and a "File history
  of current file" palette command: the commit list shows only the commits that changed the
  file, following renames, with that file's diff for the selected commit.
- Rename commits show the edited lines instead of a whole-file add.

### Line staging, diff options, syntax highlighting, discard and ignore

- Select individual lines in a diff (click, Shift+click, Ctrl/Cmd+click, drag, or ↑/↓ with
  Shift) and stage, unstage or discard just those lines. `s` and `d` act on the selection when
  there is one, otherwise on the hunk.
- Keyboard: `↑`/`↓` move a line cursor inside the active hunk and select that line,
  `Shift+↑`/`Shift+↓` extend the selection, `s` and `d` `d` act on the selection (or the hunk
  when nothing is selected), and `[` / `]` move between hunks and clear the selection.
- Line actions refuse to run if the file changed since its diff was shown (for example an edit
  inside the hunk in another editor); nothing is changed and the diff refreshes instead.
- Staging, unstaging or discarding a whole hunk now also works with zero context lines and for
  hunks of untracked, newly added or deleted files, which previously failed. Discarding a whole
  hunk of an untracked file empties the file rather than deleting it (use Discard changes… on
  the file to delete it).
- Whole-hunk staging no longer carries a file's mode change (chmod, such as the executable
  bit); stage the whole file to stage a mode change.
- Diff toolbar: Ignore whitespace, context lines (0-20) and Syntax highlighting, remembered
  across sessions. While whitespace is ignored, partial staging is turned off in the working
  tree and a hint appears in its place; file-level stage and unstage still work. Right after
  changing an option, hunk and line actions reappear once the diff has reloaded with it.
- Diffs are syntax-highlighted for common languages, unless a file's diff has more than 2,000
  lines in total across its hunks.
- File "⋯" menu (also right-click or Shift+F10 on a file header): Discard changes…, and for
  untracked files Ignore file / Ignore all *.ext / Ignore folder, which append to `.gitignore`.
  Discarding (a file, a hunk or lines; tracked or untracked) refuses paths outside the
  repository, including through symlinked folders. Ignoring refuses a `.gitignore` that is a
  symlink, never overwrites one that can't be read, and rejects patterns with leading or
  trailing spaces.
- "Discard all…" next to "Stage all" discards the unstaged changes of exactly the files its
  confirmation lists; untracked files are deleted and conflicted files are skipped. Folders
  such as untracked nested repositories are never deleted, and files that couldn't be discarded
  are named in an error once the rest are done.
- Palette commands for each diff option and for discarding.

### Cherry-pick, revert, reset, amend and force push

- Commit context menu: Cherry-pick, Revert, Checkout (detached) and "Reset ⟨branch⟩ to here…"
  (soft, mixed or hard; hard asks first). Cherry-pick and revert conflicts use the merge
  conflict flow, with their own status-strip label and abort button. A cherry-pick finished
  after resolving conflicts keeps the original author, and aborting one leaves unrelated
  unstaged edits alone (including when a conflicted file name contains glob characters).
- "Amend last commit" toggle in the commit box, plus "Edit commit message…" on the latest
  commit; warns when that commit is already pushed.
- Force push with lease: offered when a push is rejected as non-fast-forward, and from the
  palette; refused if the remote branch moved since the last fetch.
- Detached HEAD is shown in the sync bar, with push and pull disabled.
- Palette commands for all of the above.

### Multi-select commits for interactive rebase

- Ctrl/Cmd+click commits in the graph to select several, including non-adjacent ones (Shift
  selects a range). A toolbar and the context menu offer Interactive rebase…, Squash, Fixup and
  Drop, each opening the rebase planner with the selection pre-marked. Squash and Fixup require
  adjacent commits; Drop and Interactive rebase… accept any selection.

### UX audit: feedback, states and content

- A persistent status strip now shows while a merge or rebase is in progress (step, conflict
  count, Abort), and Commit / Continue rebase show a visible reason when disabled.
- Success toasts (polite live region) after commit, checkout, branch delete, stash, fetch, push
  and pull. Error banners float instead of shifting the layout (FB-004). A failed diff, blame or
  commit-file fetch in the diff pane keeps the raw error text, adds a plain-language hint for known
  error kinds and offers Retry; a failed repository open also offers Retry, and a lost backend
  connection shows a fixed hint. Errors from actions (the top-level banner for a failed commit,
  checkout, push and so on) still show the message only, with no hint and no Retry.
- New success, warning and info color tokens for light and dark themes (VIS-002), applied to status
  strip, toasts, and conflicted file tinting.
- The inline New branch form shows its base, validates the name inline, and can check the new
  branch out. Pull request source/target branches default sensibly and suggest known branches.
- The repository picker leads with folder names, explains workspaces, and hints at the command
  palette. The palette hint is platform-aware and release notes use a "What's new" icon.
- Sentence case for UI labels (FB-007): picker, workspace and rebase buttons, the branch "+" menu
  ("New branch…", "Add remote…"), hunk actions, conflict resolution ("Accept ours", "Keep their
  version", …) and "Back to diff", documented in `docs/CONTENT_GUIDELINES.md` together with the
  Title Case labels that remain.
- Added test coverage for the `?` keyboard shortcut guard and sidebar auto-collapse behavior in `App`.

### Security

- `resolve_conflict` now validates that the given path is an actual index conflict before
  writing to the working directory, closing a gap where any path (including one under `.git/`)
  would be written and staged unconditionally.
- Updated `rustls` to 0.23.45 to pick up the fix for RUSTSEC-2026-0285 (TLS 1.3 handshake
  messages accepted across encryption-level boundaries).

### Fixed

- Staged files no longer offer a discard action (the hunk and line discard buttons and `d` in a staged file's diff); it could previously revert an
  unrelated hunk of unstaged changes.
- Diff pane: a file's diff shows "Loading diff…" instead of a false "No differences" while it
  loads; empty diffs now read "No text differences (binary file or mode-only change)".
  Collapsed file sections no longer fetch their diff until expanded, and sections far from the
  viewport not until scrolled near (PERF-001, IntersectionObserver). Open diffs refetch on every
  refresh (an action, the palette's Refresh, a stash apply or pull), and a hunk stage/unstage/
  discard refetches both the staged and unstaged diff of that file once the action has finished.
  Only the active repository tab's workspace is mounted, to avoid paying every open tab's DOM and
  IPC cost; a half-typed commit message is kept per repository across tab switches, and a tab
  switched away from mid-action no longer stays unclosable.
- Diff hunk actions are no longer tab stops. The diff is one tab stop with `[`/`]` (prev/next
  hunk), `s` (stage/unstage) and `d` (discard, press twice). The armed "Confirm discard" is
  styled as danger and disarms on blur, Escape or after 5 seconds.
- Diff lines show old/new line-number gutters; long tokens wrap and hunk headers wrap cleanly
  at narrow widths. Paired Remove/Add lines now highlight only the changed words (computed once
  per loaded diff; line pairs too long to compare cheaply, such as minified code, are marked
  wholly changed instead), and a Split view / Unified view toggle switches the diff between the
  single-column and a side-by-side layout (UX-007).
- Commit dock is lighter (single border, no nested panel) and no longer lets diff content show
  beneath it.
- Accessibility pass: confirm and form dialogs return focus to the invoking control when they
  close; workspace deletion uses the shared modal `ConfirmDialog`; repo tabs use roving tabindex
  with Left/Right/Home/End navigation and link to their workspace panel. Each tab's close button
  is still inside the tablist but is now `aria-hidden` and out of the Tab order (mouse-only), so
  the tablist exposes only tabs; from the keyboard, Delete closes the focused tab, the palette
  has a "Close tab" command, and Ctrl/Cmd+W closes the active tab where the host doesn't take
  the key first (Tauri's default macOS menu binds Cmd+W to Close Window) (A11Y-003); swatches,
  tab close buttons, sidebar toolbar buttons and split dividers have at least 24x24 pointer
  targets (`--size-target-min`); context menus no longer close on mouse leave and stay inside the
  viewport; reduced-motion preference disables smooth scrolling and transitions; sidebar toolbar
  icons have tooltips. Added a shared `Field` primitive (label, hint, error, `aria-invalid`,
  `aria-describedby`, `required`).
- Narrow windows: the Tauri window has a minimum size (800x500), the diff pane keeps a minimum
  width instead of collapsing to zero, and the sidebar auto-collapses below 900px (RESP-001).
- The header "+" (open repository) button is pinned outside the scrolling tab strip (RESP-002).
- Transfer progress shows MB/GB instead of ever-larger KB values, and the transfer panel now has
  a working Cancel button (PERF-002). Cancelling aborts the in-flight `git2` transfer itself: a
  shared registry of cancelled operation IDs is polled from inside the fetch progress callbacks,
  so a cancel lands even while the worker thread is blocked in network I/O. A push can only be
  cancelled before its data is sent (at `push_negotiation`); a cancel pressed later lets the push
  finish and report its real outcome, since the remote may already have applied it. Surfaces
  as a new `TransferErrorKind::Cancelled`, a `cancel_transfer` command on both the Tauri and
  VSCode transports, and a "Fetch/Pull/Push cancelled." notice rather than a failure message.

- Keyboard shortcut sheet, opened with `?` or the "Show keyboard shortcuts" palette command
  (UX-006).
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
- Remote-tracking branch tips now show inline in the commit graph as a dashed-border badge next to
  the existing solid local-branch badge, with their own per-remote-branch graph-visibility toggle
  in the branch tree.

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
  instead of a file list where each row had to be
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
