# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

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
