### Test reliability

- Fixed: a toast's Undo clicked the moment the toast appeared could be refused as stale, or
  ignored, instead of undoing its step (#769).
- Internal: the lock-contention burst test compares the waits the worker chose instead of
  elapsed time, so a slow CI runner no longer fails it (#736).
- Internal: the search test fixture with a merged side branch commits at fixed times, so the
  order of its two sibling commits no longer depends on the clock (#770).
- Internal: the VSCode E2E specs run git through one shared helper,
  `extension/e2e/src/support/git.ts`, which the Tauri E2E suite re-exports (#524).
