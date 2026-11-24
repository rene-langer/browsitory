### Internal

- Changed: the changelog check (pre-push hook and CI) also requires an entry for changes to
  `extension/`, the VS Code extension that ships in every VSIX, its E2E tests included (#1057).
- Changed: CI's Tauri E2E job installs a pinned tauri-driver that Renovate updates, instead of
  the newest release on every run (#1058).
- Added: an extension test fails when its copies of `HostRequest` and `DiffSource` drift from the
  frontend's `RepoClient.ts` (#1059).
- Changed: the frontend keeps one copy each of the byte-size formatter used by the image diff and
  transfer progress, `isUntracked` and the syntax-highlighting error logger, and drops the unused
  `HISTORY_EDIT_KINDS` (#1060).
- Changed: the release workflow's `verify` job restores main's Rust cache instead of compiling
  from scratch (#1061).
