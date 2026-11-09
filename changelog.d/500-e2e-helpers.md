### Internal: shared E2E helpers

- Changed: the Tauri E2E specs run the git CLI through one helper, `e2e/support/git.ts`
  (`git`, `gitIn`, `gitWith`), instead of 13 local helpers and about 170 direct calls (#500).
- Changed: `wdio.conf.ts` sets a 10 s `waitforTimeout` (`UI_TIMEOUT` in
  `e2e/support/timeouts.ts`); waits no longer repeat it, and longer ones use named constants
  (#500).
- Changed: `waitForAppReady` and the retrying `openPalette` live in `e2e/support/app.ts`, and
  both E2E suites create their fixture repository with one shared `initFixtureRepo` (#500).
