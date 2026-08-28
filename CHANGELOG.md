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

### Changed

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
