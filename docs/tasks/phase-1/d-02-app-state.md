# Task 1.D.02: `useAppState` hook

## Goal

Give `frontend/src/state/` its first real content (Phase 0 only had the ESLint constraint, no
code): a `useAppState` hook owning `repoPath`, `selectedRow` (the unified history-list
selection), `status`, and `log`. Every mutation (`stageFile`/`unstageFile`/`commit`) refetches
`status`+`log` afterward, so no component keeps its own copy of shared state. Per-selection diff
content is *not* centralized here — that stays local to the component that renders it (Task
1.E.04), same self-contained-fetch pattern `StatusView` already uses.

## Depends on

1.D.01 (`RepoClient` interface — this hook is generic over any `RepoClient`, and its test uses a
fake one, so it doesn't need the real `tauriRepoClient` to exist, but it does need the finished
interface shape to type against).

## Interfaces produced

`frontend/src/state/useAppState.ts`:
```tsx
import { useCallback, useState } from "react";
import type { CommitInfo, RepoClient, StatusEntry } from "../ipc/RepoClient";

export type SelectedRow = "uncommitted" | { commitId: string };

export interface AppState {
  repoPath: string | null;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  log: CommitInfo[];
  error: string | null;
}

export interface UseAppStateResult {
  state: AppState;
  openRepo(path: string): Promise<void>;
  selectRow(row: SelectedRow): void;
  stageFile(path: string): Promise<void>;
  unstageFile(path: string): Promise<void>;
  commit(message: string): Promise<void>;
  refresh(): Promise<void>;
}

export function useAppState(client: RepoClient): UseAppStateResult {
  // ...
}
```
Later frontend tasks (1.E.01, 1.E.03, 1.E.04, 1.F.01) all consume `useAppState`'s return value —
`state.status`/`state.log`/`state.selectedRow`/`state.error` for rendering, and
`openRepo`/`selectRow`/`stageFile`/`unstageFile`/`commit` as the actions they wire to clicks/
keystrokes.

## Implementation notes

`LOG_LIMIT = 300` (a module-level constant) is the fixed cap passed to `client.getLog`, matching
the design's "no pagination this phase" decision.

Every mutating action (`openRepo`, `stageFile`, `unstageFile`, `commit`) follows the same
try/refresh/catch shape — factor it into one private helper so the four call sites don't repeat
it:
```tsx
export function useAppState(client: RepoClient): UseAppStateResult {
  const [state, setState] = useState<AppState>({
    repoPath: null,
    selectedRow: "uncommitted",
    status: [],
    log: [],
    error: null,
  });

  const refresh = useCallback(async () => {
    try {
      const [status, log] = await Promise.all([
        client.getStatus(),
        client.getLog(LOG_LIMIT),
      ]);
      setState((prev) => ({ ...prev, status, log, error: null }));
    } catch (err) {
      setState((prev) => ({ ...prev, error: String(err) }));
    }
  }, [client]);

  const runMutation = useCallback(
    async (mutate: () => Promise<void>) => {
      try {
        await mutate();
        await refresh();
      } catch (err) {
        setState((prev) => ({ ...prev, error: String(err) }));
      }
    },
    [refresh],
  );

  const openRepo = useCallback(
    (path: string) =>
      runMutation(async () => {
        await client.openRepo(path);
        setState((prev) => ({ ...prev, repoPath: path, selectedRow: "uncommitted" }));
      }),
    [client, runMutation],
  );

  const selectRow = useCallback((row: SelectedRow) => {
    setState((prev) => ({ ...prev, selectedRow: row }));
  }, []);

  const stageFile = useCallback(
    (path: string) => runMutation(() => client.stageFile(path)),
    [client, runMutation],
  );
  const unstageFile = useCallback(
    (path: string) => runMutation(() => client.unstageFile(path)),
    [client, runMutation],
  );
  const commit = useCallback(
    (message: string) => runMutation(() => client.commit(message)),
    [client, runMutation],
  );

  return { state, openRepo, selectRow, stageFile, unstageFile, commit, refresh };
}
```
Note `openRepo`'s inner callback sets `repoPath`/`selectedRow` *before* `runMutation`'s own
`refresh()` call runs (it's part of the `mutate` closure passed in, which resolves before
`runMutation` calls `refresh()`) — so by the time `status`/`log` populate, `repoPath` is already
set. `selectRow` is the one action that's synchronous and never refetches — clicking a different
history row doesn't need new `status`/`log`, only a new `selectedRow` for `DiffPane` (Task
1.E.04) to react to.

## TDD requirement

`frontend/src/state/useAppState.test.ts` (new file), using `renderHook`/`act` from
`@testing-library/react` (already a dependency) and a fake `RepoClient` (same
object-literal-implementing-the-interface pattern as `StatusView.test.tsx`'s `fakeClient`, but
implementing all 11 `RepoClient` methods this time — build one fake per test with the specific
return values/rejections that test needs):

- `openRepo populates status and log and sets repoPath`: fake client whose `openRepo` resolves,
  `getStatus` resolves to one `StatusEntry`, `getLog` resolves to one `CommitInfo`. Call
  `await act(() => result.current.openRepo("/repo"))`. Assert
  `result.current.state.repoPath === "/repo"`, `state.status.length === 1`,
  `state.log.length === 1`, `state.selectedRow === "uncommitted"`.
- `selectRow updates selectedRow without refetching`: fake client tracking a call counter on
  `getStatus`. After an initial `openRepo` (counter now 1), call
  `act(() => result.current.selectRow({ commitId: "abc123" }))`. Assert
  `result.current.state.selectedRow` deep-equals `{ commitId: "abc123" }` and the `getStatus`
  call counter is still 1 (unchanged).
- `stageFile calls client.stageFile then refreshes status`: fake client's `getStatus` returns a
  different array on its second call than its first (e.g. track a call counter, return
  `[entryA]` on call 1, `[]` on call 2 — simulating the file becoming fully staged and dropping
  out of an unstaged-only view, or simplest: just assert the array reference/content after
  `stageFile` matches whatever the *second* `getStatus` call returned). Call
  `await act(() => result.current.openRepo("/repo"))` then
  `await act(() => result.current.stageFile("a.txt"))`. Assert the fake's `stageFile` was called
  with `"a.txt"`, and `result.current.state.status` reflects the post-stage `getStatus` result.
- `errors surface in state.error without throwing`: fake client whose `openRepo` rejects with
  `new Error("no such directory")`. Call `await act(() => result.current.openRepo("/bad"))` —
  this must not throw out of `act` (the hook catches internally). Assert
  `result.current.state.error === "Error: no such directory"`.

Write these four tests first (against a `useAppState` that doesn't exist yet), run
`pnpm test -- --run` inside `frontend/`, confirm they fail (module not found), then implement
`useAppState.ts` per the interfaces/notes above and re-run until green.

## Acceptance criteria

- [ ] `pnpm test -- --run` passes (4 new tests + existing `StatusView` tests unaffected).
- [ ] `pnpm build` succeeds (TypeScript compiles clean).
- [ ] `pnpm lint` clean.
- [ ] Commit: `git add frontend/src/state && git commit -m "feat(frontend): add useAppState hook for repo/selection/status/log state"`.

## Out of scope

Persisting `selectedRow`/`repoPath` across app restarts (every launch starts at `RepoPicker`,
Task 1.E.01 — the *recent-repos list* persists via Task 1.B.01/1.C.02, but which repo/row was
selected does not). Undo/redo. Optimistic UI updates before a mutation's refetch completes (every
action shows its effect only after the round-trip; acceptable latency for whole-file
stage/unstage/commit on local repos).
