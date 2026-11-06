### Internal: smaller source files

- Internal: the five largest files are split by feature, with no behaviour change (#307). The
  worker's and the desktop commands' inline tests moved to `worker/tests/` and
  `commands/tests/`, the sidecar's handlers to `dispatch/<feature>.rs`, `App.tsx`'s
  `RepoWorkspace` and its state hooks to their own files, and `useAppState.test.ts` to one test
  file per area.
