### Internal: e2e specs wait for the staged file before committing

- Fixed: `first-flow.spec.ts` clicked Commit right after the stage click, so when the stage had
  not finished the commit never happened and the spec timed out waiting for it in history. It
  now waits for the file's Unstage control (the row is under Staged) and clicks Commit with
  `clickInPage`, which waits for the button to be enabled. `commit-graph.spec.ts` (both tests),
  `blame-viewer.spec.ts` and `commit-signing.spec.ts` had the same race and get the same fix
  (#697).
