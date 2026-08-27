# Optimistic UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an optimistic-update path for every frontend mutation whose result the frontend can
predict exactly (list-shaped CRUD on branches/tags/remotes/worktrees/one stash op, plus
file-level stage/unstage), so those actions feel instant instead of showing a visible
fetch-then-refresh round-trip.

**Architecture:** One new `runOptimisticMutation` family (three variants, matching the existing
`runMutation`/`runMutationWithMessage`/`runMutationWithOutcome` shapes) added to
`frontend/src/state/useMutationRunner.ts`. Each variant snapshots the whole `AppState` object
inside its first `setState` call, applies a caller-supplied `optimisticUpdate: (prev) => AppState`
transform immediately, awaits the real backend call, and on failure restores the exact snapshot
(no per-mutation inverse function needed) before setting the error. ~17 call sites across 5
already-split domain hooks (`useStagingActions`, `useBranchActions`, `useWorktreeActions`,
`useStashActions`, `useRemoteTransferActions`) swap their existing `runMutation*` call for the
matching `runOptimisticMutation*` variant plus a transform describing the predicted result.

**Tech Stack:** React 18 + TypeScript, Vitest + `@testing-library/react`'s `renderHook`/`act`
(existing `frontend/src/state/*.test.ts` convention — mock `RepoClient`, never
`@tauri-apps/api`).

**Spec:** `docs/superpowers/specs/2026-08-26-optimistic-ui-design.md`

## Global Constraints

- No new npm dependencies.
- Zero change to any exported hook's function signatures/return types (`AppState`,
  `UseAppStateResult`, and every domain hook's own exported interface stay exactly as they are —
  this is an internal swap under `useMutationRunner`, not a public-API change).
- `pending` stays a single global flag on `AppState` — do not narrow it to per-item state.
- Every migrated mutation needs both a passing optimistic-application test (asserts the state
  change is visible *before* the backend promise resolves) and a rollback-on-failure test
  (asserts state is back to its pre-call value after a rejection) before its task is done.
- Out of scope for this plan (stays fetch-then-refresh, do not touch): `stageHunk`,
  `unstageHunk`, `discardHunk`, `commit`, `applyStash`, `saveStash`, `switchBranch`,
  `initSubmodule`, `updateSubmodule`, `fetchRemote`, `pushCurrentBranch`, `pushTags`,
  `pullCurrentUpstream`, `mergeBranch` and the rest of merge/rebase, and all forge/PR actions.
  See the spec's "Explicitly out of scope" section for why each is excluded.

---

### Task 1: `runOptimisticMutation` family in `useMutationRunner.ts`

**Files:**
- Modify: `frontend/src/state/useMutationRunner.ts`
- Create: `frontend/src/state/useMutationRunner.test.ts`

**Interfaces:**
- Produces: `OptimisticUpdate = (prev: AppState) => AppState`; `RunOptimisticMutation = (optimisticUpdate: OptimisticUpdate, mutate: () => Promise<void>) => Promise<void>`; `RunOptimisticMutationWithMessage = (optimisticUpdate: OptimisticUpdate, mutate: () => Promise<void>) => Promise<string | null>`; `RunOptimisticMutationWithOutcome = (optimisticUpdate: OptimisticUpdate, mutate: () => Promise<void>) => Promise<boolean>`; all three added to the `MutationRunner` interface and returned from `useMutationRunner`.

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/state/useMutationRunner.test.ts`:

```ts
import { act, renderHook } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import type { AppState } from "./useAppState";
import { useMutationRunner } from "./useMutationRunner";

const BASE_STATE: AppState = {
  repoPath: "/repo",
  selectedRow: "uncommitted",
  status: [],
  commits: [],
  graphBranchSelection: null,
  branches: [],
  worktrees: [],
  submodules: [],
  reflogRefs: [],
  selectedReflogReference: null,
  reflog: [],
  remotes: [],
  tags: [],
  upstream: null,
  remoteUpstreams: {},
  forgeRepositories: [],
  pullRequests: {},
  createBranchDraft: null,
  stashes: [],
  mergeMessage: null,
  rebaseProgress: null,
  rebaseOnto: null,
  squashPreset: null,
  pendingPull: null,
  pullOutcome: null,
  transfer: null,
  error: null,
  pending: false,
};

function setupRunner() {
  return renderHook(() => {
    const [state, setState] = useState<AppState>(BASE_STATE);
    const refresh = async () => {};
    const runner = useMutationRunner(refresh, setState);
    return { state, ...runner };
  });
}

describe("useMutationRunner's optimistic variants", () => {
  it("runOptimisticMutation applies the update before the mutation resolves, keeps it after success", async () => {
    const { result } = setupRunner();
    let resolveMutate: (() => void) | null = null;

    let mutationPromise!: Promise<void>;
    act(() => {
      mutationPromise = result.current.runOptimisticMutation(
        (prev) => ({ ...prev, branches: [{ name: "feature", isCurrent: false }] }),
        () => new Promise<void>((resolve) => { resolveMutate = resolve; }),
      );
    });

    expect(result.current.state.branches).toEqual([{ name: "feature", isCurrent: false }]);
    expect(result.current.state.pending).toBe(true);

    await act(async () => {
      resolveMutate?.();
      await mutationPromise;
    });

    expect(result.current.state.branches).toEqual([{ name: "feature", isCurrent: false }]);
    expect(result.current.state.pending).toBe(false);
  });

  it("runOptimisticMutation rolls back to the pre-call snapshot on failure", async () => {
    const { result } = setupRunner();

    await act(() =>
      result.current.runOptimisticMutation(
        (prev) => ({ ...prev, branches: [{ name: "feature", isCurrent: false }] }),
        async () => {
          throw new Error("boom");
        },
      ),
    );

    expect(result.current.state.branches).toEqual([]);
    expect(result.current.state.error).toBe("boom");
    expect(result.current.state.pending).toBe(false);
  });

  it("runOptimisticMutationWithMessage resolves null on success, restores state and resolves the message on failure", async () => {
    const { result } = setupRunner();

    const successResult = await act(() =>
      result.current.runOptimisticMutationWithMessage(
        (prev) => ({ ...prev, tags: [{ name: "v1", targetId: "", annotated: false, message: null, taggerName: null, timestamp: null }] }),
        async () => {},
      ),
    );
    expect(successResult).toBeNull();
    expect(result.current.state.tags).toHaveLength(1);

    const failureResult = await act(() =>
      result.current.runOptimisticMutationWithMessage(
        (prev) => ({ ...prev, tags: [...prev.tags, { name: "v2", targetId: "", annotated: false, message: null, taggerName: null, timestamp: null }] }),
        async () => {
          throw new Error("tag exists");
        },
      ),
    );
    expect(failureResult).toBe("tag exists");
    expect(result.current.state.tags).toHaveLength(1);
  });

  it("runOptimisticMutationWithOutcome resolves true on success, restores state and resolves false on failure", async () => {
    const { result } = setupRunner();

    const succeeded = await act(() =>
      result.current.runOptimisticMutationWithOutcome(
        (prev) => ({ ...prev, remotes: [{ name: "upstream", fetchUrl: "u", pushUrl: null, authMode: null, authUsername: null }] }),
        async () => {},
      ),
    );
    expect(succeeded).toBe(true);
    expect(result.current.state.remotes).toHaveLength(1);

    const failed = await act(() =>
      result.current.runOptimisticMutationWithOutcome(
        (prev) => ({ ...prev, remotes: prev.remotes.map((r) => ({ ...r, name: "renamed" })) }),
        async () => {
          throw new Error("rename failed");
        },
      ),
    );
    expect(failed).toBe(false);
    expect(result.current.state.remotes[0].name).toBe("upstream");
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx pnpm test -- --run useMutationRunner`
Expected: FAIL — `runOptimisticMutation`/`runOptimisticMutationWithMessage`/`runOptimisticMutationWithOutcome` don't exist on the returned object (TypeScript error / `undefined is not a function`).

- [ ] **Step 3: Implement the three variants**

In `frontend/src/state/useMutationRunner.ts`, add the new types near the top (after the existing three type aliases) and the three functions inside `useMutationRunner`, and add them to the returned object and the `MutationRunner` interface:

```ts
export type OptimisticUpdate = (prev: AppState) => AppState;
export type RunOptimisticMutation = (optimisticUpdate: OptimisticUpdate, mutate: () => Promise<void>) => Promise<void>;
export type RunOptimisticMutationWithMessage = (optimisticUpdate: OptimisticUpdate, mutate: () => Promise<void>) => Promise<string | null>;
export type RunOptimisticMutationWithOutcome = (optimisticUpdate: OptimisticUpdate, mutate: () => Promise<void>) => Promise<boolean>;
```

Add to the `MutationRunner` interface:

```ts
export interface MutationRunner {
  runMutation: RunMutation;
  runMutationWithOutcome: RunMutationWithOutcome;
  runMutationWithMessage: RunMutationWithMessage;
  runOptimisticMutation: RunOptimisticMutation;
  runOptimisticMutationWithMessage: RunOptimisticMutationWithMessage;
  runOptimisticMutationWithOutcome: RunOptimisticMutationWithOutcome;
}
```

Inside `useMutationRunner`, after the existing `runMutationWithMessage` definition:

```ts
  // Optimistic counterparts of the three variants above: apply `optimisticUpdate` to `AppState`
  // immediately (before awaiting `mutate`), so the UI reflects the predicted result with no
  // visible round-trip. On failure, restore the *exact* pre-call snapshot rather than computing
  // a per-mutation inverse — cheap (one object) and trivially correct: failure always means
  // "exactly what it looked like before this call touched anything." The `pending`-driven global
  // lock (still set here) already rules out two overlapping optimistic mutations racing each
  // other's snapshot/restore.
  const runOptimisticMutation = useCallback<RunOptimisticMutation>(
    async (optimisticUpdate, mutate) => {
      let snapshot: AppState | null = null;
      setState((prev) => {
        snapshot = prev;
        return { ...optimisticUpdate(prev), pending: true };
      });
      try {
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
      } catch (err) {
        setState(() => ({ ...(snapshot as AppState), error: credentialFailureMessage(err), pending: false }));
      }
    },
    [refresh, setState],
  );

  const runOptimisticMutationWithMessage = useCallback<RunOptimisticMutationWithMessage>(
    async (optimisticUpdate, mutate) => {
      let snapshot: AppState | null = null;
      setState((prev) => {
        snapshot = prev;
        return { ...optimisticUpdate(prev), pending: true };
      });
      try {
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
        return null;
      } catch (err) {
        const message = credentialFailureMessage(err);
        setState(() => ({ ...(snapshot as AppState), error: message, pending: false }));
        return message;
      }
    },
    [refresh, setState],
  );

  const runOptimisticMutationWithOutcome = useCallback<RunOptimisticMutationWithOutcome>(
    async (optimisticUpdate, mutate) => {
      let snapshot: AppState | null = null;
      setState((prev) => {
        snapshot = prev;
        return { ...optimisticUpdate(prev), pending: true };
      });
      try {
        await mutate();
        await refresh();
        setState((prev) => ({ ...prev, pending: false }));
        return true;
      } catch (err) {
        setState(() => ({ ...(snapshot as AppState), error: credentialFailureMessage(err), pending: false }));
        return false;
      }
    },
    [refresh, setState],
  );
```

Update the final `return` statement to include the three new functions.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx pnpm test -- --run useMutationRunner`
Expected: PASS, all 4 tests.

- [ ] **Step 5: Typecheck and commit**

Run: `cd frontend && npx pnpm build`
Expected: no TypeScript errors (this file's exports aren't consumed anywhere yet, so nothing else should break).

```bash
git add frontend/src/state/useMutationRunner.ts frontend/src/state/useMutationRunner.test.ts
git commit -m "feat(frontend): add runOptimisticMutation family to useMutationRunner"
```

---

### Task 2: Wire the optimistic runners through `useAppState.ts`

**Files:**
- Modify: `frontend/src/state/useAppState.ts:267` (the `useMutationRunner` destructuring) and the 5 domain-hook call sites below it (lines 269-336 in the current file — exact line numbers will have shifted slightly if Task 1 changed nothing here, but the call sites are unambiguous by hook name).

**Interfaces:**
- Consumes: `runOptimisticMutation`, `runOptimisticMutationWithMessage`, `runOptimisticMutationWithOutcome` from Task 1's `useMutationRunner` return value.
- Produces: nothing new externally — this task only threads the three functions into each domain hook's parameter list (added as new trailing parameters) so Tasks 3-9 can consume them. No domain hook's behavior changes in this task; they'll ignore the new parameters until migrated.

This task also touches `useBranchActions`, `useWorktreeActions`, `useStashActions`,
`useRemoteTransferActions` (their function signatures, adding the new parameters), but each
hook's *body* is untouched until its own task — the new parameters are simply unused until then.
Prefix each newly-added-but-not-yet-used parameter with `_` (this project's existing convention
for intentionally-unused parameters — check `frontend/eslint.config.js`'s `no-unused-vars`
`argsIgnorePattern`) so the file stays lint-clean in the meantime. Each of Tasks 3-9 renames its
hook's `_`-prefixed parameter back to its plain name as part of first using it.

- [ ] **Step 1: Update `useAppState.ts`'s `useMutationRunner` destructuring**

In `frontend/src/state/useAppState.ts`, change:

```ts
  const { runMutation, runMutationWithOutcome, runMutationWithMessage } = useMutationRunner(refresh, setState);
```

to:

```ts
  const {
    runMutation,
    runMutationWithOutcome,
    runMutationWithMessage,
    runOptimisticMutation,
    runOptimisticMutationWithMessage,
    runOptimisticMutationWithOutcome,
  } = useMutationRunner(refresh, setState);
```

- [ ] **Step 2: Typecheck**

Run: `cd frontend && npx pnpm build`
Expected: PASS — these are now unused local variables, which is a TypeScript `noUnusedLocals` error if that's enabled. Check the error; if it fires, that's expected and resolved by the end of this step's task set. **Do not silence it with an eslint-disable or `_` prefix here** — instead, immediately proceed to Step 3, which consumes `runOptimisticMutation` in `useStagingActions`'s call site (folded into this task since it's a one-line change), removing the unused-variable window entirely. The other 4 domain hooks' call sites are updated the same way, one line each, all in this same task, so nothing is left half-wired.

- [ ] **Step 3: Thread the new runners into every domain hook's call site**

Change each call site in `useAppState.ts` to pass the new runner(s) that hook's own task will need. These are additive trailing arguments — do not reorder existing ones.

`useStagingActions` call site — add `runOptimisticMutation`:
```ts
  const {
    selectRow,
    stageFile,
    unstageFile,
    stageAllFiles,
    unstageAllFiles,
    stageHunk,
    unstageHunk,
    discardHunk,
    commit,
  } = useStagingActions(client, repoPath, runMutation, runOptimisticMutation, setState);
```

`useBranchActions` call site — add `runOptimisticMutation`:
```ts
  const {
    createBranch,
    switchBranch,
    deleteBranch,
    renameBranch,
    openCreateBranchDraft,
    closeCreateBranchDraft,
    setGraphBranchSelection,
  } = useBranchActions(client, repoPath, runMutation, runMutationWithMessage, runOptimisticMutation, runOptimisticMutationWithMessage, setState);
```

`useWorktreeActions` call site — add `runOptimisticMutation`:
```ts
  const { createWorktree, removeWorktree, pruneWorktrees } = useWorktreeActions(
    client,
    repoPath,
    runMutation,
    runMutationWithMessage,
    runOptimisticMutation,
    runOptimisticMutationWithMessage,
  );
```

`useStashActions` call site — add `runOptimisticMutation` (keep passing `state` for now; Task 6
removes it once `dropStash`'s optimistic transform reads `prev` from its own callback instead —
see Step 5 below):
```ts
  const { saveStash, applyStash, dropStash } = useStashActions(client, repoPath, runMutation, runOptimisticMutation, state, setState);
```

`useRemoteTransferActions` call site — add all three optimistic variants:
```ts
  const {
    addRemote,
    renameRemote,
    updateRemoteUrls,
    removeRemote,
    saveHttpsCredential,
    forgetHttpsCredential,
    setRemoteAuthMode,
    setCurrentUpstream,
    clearCurrentUpstream,
    listRemoteBranches,
    fetchRemote,
    createTag,
    deleteTag,
    pushCurrentBranch,
    pushTags,
    pullCurrentUpstream,
    clearPendingPull,
  } = useRemoteTransferActions(
    client,
    repoPath,
    refresh,
    runMutation,
    runMutationWithMessage,
    runMutationWithOutcome,
    runOptimisticMutation,
    runOptimisticMutationWithMessage,
    runOptimisticMutationWithOutcome,
    setState,
  );
```

- [ ] **Step 4: Update each domain hook's function signature to accept (and, for now, ignore) the new parameters**

This is mechanical — each hook's exported function signature gains the new parameter(s) with the
matching type imports. Since the parameters aren't used in the hook body yet, TypeScript's
`noUnusedParameters` (if enabled — check `frontend/tsconfig.app.json`) would flag them; prefix
each with `_` for now (e.g. `_runOptimisticMutation: RunOptimisticMutation`) — Tasks 3-9 each
rename their hook's `_runOptimisticMutation*` parameter back to its unprefixed name as the very
first step of actually using it, so this prefix never survives past its own task's completion.

`useStagingActions.ts`:
```ts
import type { RunMutation, RunOptimisticMutation } from "./useMutationRunner";
// ...
export function useStagingActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  _runOptimisticMutation: RunOptimisticMutation,
  setState: (updater: (prev: AppState) => AppState) => void,
): StagingActions {
```

`useBranchActions.ts`:
```ts
import type { RunMutation, RunMutationWithMessage, RunOptimisticMutation, RunOptimisticMutationWithMessage } from "./useMutationRunner";
// ...
export function useBranchActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runMutationWithMessage: RunMutationWithMessage,
  _runOptimisticMutation: RunOptimisticMutation,
  _runOptimisticMutationWithMessage: RunOptimisticMutationWithMessage,
  setState: (updater: (prev: AppState) => AppState) => void,
): BranchActions {
```

`useWorktreeActions.ts`:
```ts
import type { RunMutation, RunMutationWithMessage, RunOptimisticMutation, RunOptimisticMutationWithMessage } from "./useMutationRunner";
// ...
export function useWorktreeActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runMutationWithMessage: RunMutationWithMessage,
  _runOptimisticMutation: RunOptimisticMutation,
  _runOptimisticMutationWithMessage: RunOptimisticMutationWithMessage,
): WorktreeActions {
```

`useStashActions.ts` — add `_runOptimisticMutation`, keep `state: AppState` as-is for now (Task 6 removes it — see Step 5 below):
```ts
import type { RunMutation, RunOptimisticMutation } from "./useMutationRunner";
// ...
export function useStashActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  _runOptimisticMutation: RunOptimisticMutation,
  state: AppState,
  setState: (updater: (prev: AppState) => AppState) => void,
): StashActions {
```

`useRemoteTransferActions.ts`:
```ts
import type {
  RunMutation,
  RunMutationWithMessage,
  RunMutationWithOutcome,
  RunOptimisticMutation,
  RunOptimisticMutationWithMessage,
  RunOptimisticMutationWithOutcome,
} from "./useMutationRunner";
// ...
export function useRemoteTransferActions(
  client: RepoClient,
  repoPath: string,
  refresh: () => Promise<void>,
  runMutation: RunMutation,
  runMutationWithMessage: RunMutationWithMessage,
  runMutationWithOutcome: RunMutationWithOutcome,
  _runOptimisticMutation: RunOptimisticMutation,
  _runOptimisticMutationWithMessage: RunOptimisticMutationWithMessage,
  _runOptimisticMutationWithOutcome: RunOptimisticMutationWithOutcome,
  setState: (updater: (prev: AppState) => AppState) => void,
): RemoteTransferActions {
```

- [ ] **Step 5: Confirm `useStashActions`'s `state` parameter is untouched**

`dropStash`'s current body still reads `state.stashes[index]`/`state.selectedRow` from its
`state` parameter, unchanged by this task (Steps 3-4 above only add `_runOptimisticMutation`
alongside it). Task 6 is the one that rewrites `dropStash` to read `prev` inside its
`optimisticUpdate` callback instead and removes `state` entirely — don't do that removal here.

- [ ] **Step 6: Typecheck and run the full test suite**

Run: `cd frontend && npx pnpm build && npx pnpm test -- --run`
Expected: PASS — build clean (no unused-parameter/unused-variable errors, since every new
parameter is either consumed or `_`-prefixed), all existing tests still pass (no behavior
changed yet in this task, only plumbing).

- [ ] **Step 7: Lint and commit**

Run: `cd frontend && npx pnpm lint`
Expected: clean.

```bash
git add frontend/src/state/useAppState.ts frontend/src/state/useStagingActions.ts frontend/src/state/useBranchActions.ts frontend/src/state/useWorktreeActions.ts frontend/src/state/useStashActions.ts frontend/src/state/useRemoteTransferActions.ts
git commit -m "refactor(frontend): thread the optimistic mutation runners through useAppState's domain hooks"
```

---

### Task 3: Migrate `useStagingActions` (`stageFile`, `unstageFile`, `stageAllFiles`, `unstageAllFiles`)

**Files:**
- Modify: `frontend/src/state/useStagingActions.ts`
- Test: `frontend/src/state/useAppState.test.ts` (existing file — this is the hook's current integration-test surface; add tests here rather than creating a new per-hook test file, matching the pattern the rest of this file already uses for `stageFile`, e.g. the existing test at line ~959, "stageFile calls client.stageFile then refreshes status")

**Interfaces:**
- Consumes: `runOptimisticMutation` (now the hook's real parameter name, no longer `_`-prefixed) from Task 2.
- Produces: no signature change to `StagingActions`.

A single-file path can have **two** `StatusEntry` rows simultaneously (one `staged: true`, one
`staged: false` — a partially-staged file, see `crates/git-core/src/status.rs`'s multiple
`entries.push` calls). `stageFile`/`unstageFile` are *whole-file* operations, so the correct
optimistic transform removes any existing entry/entries for that path and replaces them with
exactly one entry at the new `staged` value, preserving `kind`.

- [ ] **Step 1: Write the failing tests**

Add to `frontend/src/state/useAppState.test.ts` (near the existing `stageFile` test, e.g. right
after the "stageFile calls client.stageFile then refreshes status" test around line 1016):

```ts
  it("stageFile shows the file as staged before the backend call resolves, keeps it after success", async () => {
    const entryA: StatusEntry = { path: "a.txt", staged: false, kind: "Modified" };
    let resolveStageFile: (() => void) | null = null;
    const client = transferClient({
      getStatus: async () => [entryA],
      stageFile: async () => new Promise<void>((resolve) => { resolveStageFile = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let stagePromise!: Promise<void>;
    act(() => {
      stagePromise = result.current.stageFile("a.txt");
    });

    expect(result.current.state.status).toEqual([{ path: "a.txt", staged: true, kind: "Modified" }]);

    await act(async () => {
      resolveStageFile?.();
      await stagePromise;
    });

    expect(result.current.state.status).toEqual([{ path: "a.txt", staged: true, kind: "Modified" }]);
  });

  it("stageFile rolls back the optimistic staged flag if the backend call fails", async () => {
    const entryA: StatusEntry = { path: "a.txt", staged: false, kind: "Modified" };
    const client = transferClient({
      getStatus: async () => [entryA],
      stageFile: async () => {
        throw new Error("stage failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.stageFile("a.txt"));

    expect(result.current.state.status).toEqual([entryA]);
    expect(result.current.state.error).toBe("stage failed");
  });

  it("stageFile merges a partially-staged file's two status rows into one staged row", async () => {
    const unstagedPart: StatusEntry = { path: "a.txt", staged: false, kind: "Modified" };
    const stagedPart: StatusEntry = { path: "a.txt", staged: true, kind: "Modified" };
    const client = transferClient({
      getStatus: async () => [unstagedPart, stagedPart],
      stageFile: async () => new Promise<void>(() => {}),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    act(() => {
      void result.current.stageFile("a.txt");
    });

    expect(result.current.state.status).toEqual([{ path: "a.txt", staged: true, kind: "Modified" }]);
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx pnpm test -- --run useAppState -t "stageFile shows the file as staged"`
Expected: FAIL — the optimistic state isn't applied yet (current `stageFile` still awaits the
backend call before touching `state.status` at all).

- [ ] **Step 3: Implement the optimistic transform and migrate all four mutations**

In `frontend/src/state/useStagingActions.ts`, add the import and rename the parameter:

```ts
import type { RepoClient, StatusEntry, StatusKind } from "../ipc/RepoClient";
import type { AppState, SelectedRow } from "./useAppState";
import type { RunMutation, RunOptimisticMutation } from "./useMutationRunner";

// A path can have two StatusEntry rows simultaneously (staged + unstaged, for a partially-staged
// file — see crates/git-core/src/status.rs). stageFile/unstageFile/stageAllFiles/unstageAllFiles
// are whole-file operations, so the predicted result replaces every existing row for each target
// path with exactly one row at the new `staged` value, preserving `kind`.
function withFilesStaged(status: StatusEntry[], paths: string[], staged: boolean): StatusEntry[] {
  const targets = new Set(paths);
  const kindByPath = new Map<string, StatusKind>();
  for (const entry of status) {
    if (targets.has(entry.path)) kindByPath.set(entry.path, entry.kind);
  }
  const untouched = status.filter((entry) => !targets.has(entry.path));
  const updated = paths
    .filter((path) => kindByPath.has(path))
    .map((path) => ({ path, staged, kind: kindByPath.get(path) as StatusKind }));
  return [...untouched, ...updated];
}

export function useStagingActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runOptimisticMutation: RunOptimisticMutation,
  setState: (updater: (prev: AppState) => AppState) => void,
): StagingActions {
```

Replace the four bodies:

```ts
  const stageFile = useCallback(
    (path: string) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, status: withFilesStaged(prev.status, [path], true) }),
        () => client.stageFile(repoPath, path),
      ),
    [client, runOptimisticMutation, repoPath],
  );
  const unstageFile = useCallback(
    (path: string) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, status: withFilesStaged(prev.status, [path], false) }),
        () => client.unstageFile(repoPath, path),
      ),
    [client, runOptimisticMutation, repoPath],
  );
  const stageAllFiles = useCallback(
    (paths: string[]) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, status: withFilesStaged(prev.status, paths, true) }),
        async () => {
          for (const path of paths) {
            await client.stageFile(repoPath, path);
          }
        },
      ),
    [client, runOptimisticMutation, repoPath],
  );
  const unstageAllFiles = useCallback(
    (paths: string[]) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, status: withFilesStaged(prev.status, paths, false) }),
        async () => {
          for (const path of paths) {
            await client.unstageFile(repoPath, path);
          }
        },
      ),
    [client, runOptimisticMutation, repoPath],
  );
```

`stageHunk`/`unstageHunk`/`discardHunk`/`commit`/`selectRow` are unchanged — leave them exactly
as they are (out of scope, see Global Constraints).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx pnpm test -- --run useAppState`
Expected: PASS, including the 3 new tests and every pre-existing test in this file (in
particular, the existing "stageFile calls client.stageFile then refreshes status" and "Stage all
makes a single bulk call with every unstaged path" tests must still pass unmodified — this
confirms the migration didn't change observable behavior beyond adding the optimistic step).

- [ ] **Step 5: Lint, typecheck, and commit**

Run: `cd frontend && npx pnpm build && npx pnpm lint`
Expected: clean.

```bash
git add frontend/src/state/useStagingActions.ts frontend/src/state/useAppState.test.ts
git commit -m "feat(frontend): make file-level stage/unstage optimistic"
```

---

### Task 4: Migrate `useBranchActions` (`createBranch`, `deleteBranch`, `renameBranch`)

**Files:**
- Modify: `frontend/src/state/useBranchActions.ts`
- Test: `frontend/src/state/useAppState.test.ts`

**Interfaces:**
- Consumes: `runOptimisticMutation`, `runOptimisticMutationWithMessage` (now real parameter names) from Task 2.
- Produces: no signature change to `BranchActions`.

`switchBranch`/`openCreateBranchDraft`/`closeCreateBranchDraft`/`setGraphBranchSelection` are
unchanged (out of scope / not mutation-shaped).

- [ ] **Step 1: Write the failing tests**

Add to `frontend/src/state/useAppState.test.ts`:

```ts
  it("deleteBranch removes the branch from state before the backend call resolves", async () => {
    const branchA: BranchInfo = { name: "feature", isCurrent: false };
    let resolveDelete: (() => void) | null = null;
    const client = transferClient({
      listBranches: async () => [branchA],
      deleteBranch: async () => new Promise<void>((resolve) => { resolveDelete = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    expect(result.current.state.branches).toEqual([branchA]);

    let deletePromise!: Promise<void>;
    act(() => {
      deletePromise = result.current.deleteBranch("feature", false);
    });

    expect(result.current.state.branches).toEqual([]);

    await act(async () => {
      resolveDelete?.();
      await deletePromise;
    });

    expect(result.current.state.branches).toEqual([]);
  });

  it("deleteBranch restores the branch to state if the backend call fails", async () => {
    const branchA: BranchInfo = { name: "feature", isCurrent: false };
    const client = transferClient({
      listBranches: async () => [branchA],
      deleteBranch: async () => {
        throw new Error("branch has unmerged changes");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.deleteBranch("feature", false));

    expect(result.current.state.branches).toEqual([branchA]);
    expect(result.current.state.error).toBe("branch has unmerged changes");
  });

  it("createBranch adds a not-yet-current branch to state before the backend call resolves", async () => {
    let resolveCreate: (() => void) | null = null;
    const client = transferClient({
      listBranches: async () => [],
      createBranch: async () => new Promise<void>((resolve) => { resolveCreate = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let createPromise!: Promise<string | null>;
    act(() => {
      createPromise = result.current.createBranch("feature", "main");
    });

    expect(result.current.state.branches).toEqual([{ name: "feature", isCurrent: false }]);

    await act(async () => {
      resolveCreate?.();
      await createPromise;
    });

    expect(result.current.state.branches).toEqual([{ name: "feature", isCurrent: false }]);
  });

  it("createBranch resolves the failure message and removes the optimistic entry on failure", async () => {
    const client = transferClient({
      listBranches: async () => [],
      createBranch: async () => {
        throw new Error("branch already exists");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    const failure = await act(() => result.current.createBranch("feature", "main"));

    expect(failure).toBe("branch already exists");
    expect(result.current.state.branches).toEqual([]);
  });

  it("renameBranch renames the branch in state before the backend call resolves", async () => {
    const branchA: BranchInfo = { name: "old-name", isCurrent: false };
    let resolveRename: (() => void) | null = null;
    const client = transferClient({
      listBranches: async () => [branchA],
      renameBranch: async () => new Promise<void>((resolve) => { resolveRename = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let renamePromise!: Promise<void>;
    act(() => {
      renamePromise = result.current.renameBranch("old-name", "new-name");
    });

    expect(result.current.state.branches).toEqual([{ name: "new-name", isCurrent: false }]);

    await act(async () => {
      resolveRename?.();
      await renamePromise;
    });
  });
```

Add `BranchInfo` to this file's existing `import type { ... } from "../ipc/RepoClient";` list if
it isn't already imported (check first — `RemoteInfo`, `StashEntry`, etc. are already imported
per the file's current header; add `BranchInfo` alongside them).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx pnpm test -- --run useAppState -t "deleteBranch removes the branch"`
Expected: FAIL.

- [ ] **Step 3: Implement**

In `frontend/src/state/useBranchActions.ts`:

```ts
import { useCallback } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type { RunMutation, RunMutationWithMessage, RunOptimisticMutation, RunOptimisticMutationWithMessage } from "./useMutationRunner";

export interface BranchActions {
  createBranch(name: string, startPoint: string): Promise<string | null>;
  switchBranch(name: string): Promise<void>;
  deleteBranch(name: string, force: boolean): Promise<void>;
  renameBranch(oldName: string, newName: string): Promise<void>;
  openCreateBranchDraft(startPoint: string): void;
  closeCreateBranchDraft(): void;
  setGraphBranchSelection(selectedBranches: string[]): Promise<void>;
}

export function useBranchActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runMutationWithMessage: RunMutationWithMessage,
  runOptimisticMutation: RunOptimisticMutation,
  runOptimisticMutationWithMessage: RunOptimisticMutationWithMessage,
  setState: (updater: (prev: AppState) => AppState) => void,
): BranchActions {
  const createBranch = useCallback(
    (name: string, startPoint: string) =>
      runOptimisticMutationWithMessage(
        (prev) => ({ ...prev, branches: [...prev.branches, { name, isCurrent: false }] }),
        async () => {
          await client.createBranch(repoPath, name, startPoint);
          setState((prev) => ({ ...prev, createBranchDraft: null, selectedRow: "uncommitted" }));
        },
      ),
    [client, runOptimisticMutationWithMessage, repoPath, setState],
  );
  const switchBranch = useCallback(
    (name: string) =>
      runMutation(async () => {
        await client.switchBranch(repoPath, name);
        setState((prev) => ({ ...prev, selectedRow: "uncommitted", pullOutcome: null }));
      }),
    [client, runMutation, repoPath, setState],
  );
  const deleteBranch = useCallback(
    (name: string, force: boolean) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, branches: prev.branches.filter((branch) => branch.name !== name) }),
        () => client.deleteBranch(repoPath, name, force),
      ),
    [client, runOptimisticMutation, repoPath],
  );
  const renameBranch = useCallback(
    (oldName: string, newName: string) =>
      runOptimisticMutation(
        (prev) => ({
          ...prev,
          branches: prev.branches.map((branch) => (branch.name === oldName ? { ...branch, name: newName } : branch)),
        }),
        () => client.renameBranch(repoPath, oldName, newName),
      ),
    [client, runOptimisticMutation, repoPath],
  );

  const openCreateBranchDraft = useCallback(
    (startPoint: string) => {
      setState((prev) => ({ ...prev, createBranchDraft: { startPoint } }));
    },
    [setState],
  );
  const closeCreateBranchDraft = useCallback(() => {
    setState((prev) => ({ ...prev, createBranchDraft: null }));
  }, [setState]);

  const setGraphBranchSelection = useCallback(
    (selectedBranches: string[]) =>
      runMutation(() => client.setGraphBranchSelection(repoPath, selectedBranches)),
    [client, runMutation, repoPath],
  );

  return {
    createBranch,
    switchBranch,
    deleteBranch,
    renameBranch,
    openCreateBranchDraft,
    closeCreateBranchDraft,
    setGraphBranchSelection,
  };
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx pnpm test -- --run useAppState`
Expected: PASS, all tests including the 5 new ones.

- [ ] **Step 5: Lint, typecheck, and commit**

Run: `cd frontend && npx pnpm build && npx pnpm lint`
Expected: clean.

```bash
git add frontend/src/state/useBranchActions.ts frontend/src/state/useAppState.test.ts
git commit -m "feat(frontend): make branch create/delete/rename optimistic"
```

---

### Task 5: Migrate `useWorktreeActions` (`createWorktree`, `removeWorktree`)

**Files:**
- Modify: `frontend/src/state/useWorktreeActions.ts`
- Test: `frontend/src/state/useAppState.test.ts`

**Interfaces:**
- Consumes: `runOptimisticMutation`, `runOptimisticMutationWithMessage` from Task 2.
- Produces: no signature change to `WorktreeActions`. `pruneWorktrees` is unchanged (its result —
  which worktrees get pruned — isn't known client-side without asking the backend).

- [ ] **Step 1: Write the failing tests**

Add to `frontend/src/state/useAppState.test.ts`:

```ts
  it("createWorktree adds the worktree to state before the backend call resolves", async () => {
    let resolveCreate: (() => void) | null = null;
    const client = transferClient({
      listWorktrees: async () => [],
      createWorktree: async () => new Promise<void>((resolve) => { resolveCreate = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let createPromise!: Promise<string | null>;
    act(() => {
      createPromise = result.current.createWorktree("feature", "/repo/../feature", "feature-branch", null);
    });

    expect(result.current.state.worktrees).toEqual([
      { name: "feature", path: "/repo/../feature", head: null, isMain: false, isLocked: false, isPrunable: false },
    ]);

    await act(async () => {
      resolveCreate?.();
      await createPromise;
    });
  });

  it("removeWorktree removes the worktree from state before the backend call resolves, restores it on failure", async () => {
    const worktreeA: WorktreeInfo = { name: "feature", path: "/repo/../feature", head: "abc", isMain: false, isLocked: false, isPrunable: false };
    const client = transferClient({
      listWorktrees: async () => [worktreeA],
      removeWorktree: async () => {
        throw new Error("worktree has uncommitted changes");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.removeWorktree("feature"));

    expect(result.current.state.worktrees).toEqual([worktreeA]);
    expect(result.current.state.error).toBe("worktree has uncommitted changes");
  });
```

Add `WorktreeInfo` to the file's `import type { ... } from "../ipc/RepoClient";` list.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx pnpm test -- --run useAppState -t "createWorktree adds the worktree"`
Expected: FAIL.

- [ ] **Step 3: Implement**

`runMutationWithMessage` is no longer called anywhere in this file once `createWorktree` is
migrated below — drop it from the parameter list and its type import to keep the file lint-clean:

```ts
import { useCallback } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import type { RunMutation, RunOptimisticMutation, RunOptimisticMutationWithMessage } from "./useMutationRunner";

export interface WorktreeActions {
  createWorktree(name: string, path: string, branch: string, startPoint: string | null): Promise<string | null>;
  removeWorktree(name: string): Promise<void>;
  pruneWorktrees(): Promise<void>;
}

export function useWorktreeActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runOptimisticMutation: RunOptimisticMutation,
  runOptimisticMutationWithMessage: RunOptimisticMutationWithMessage,
): WorktreeActions {
  const createWorktree = useCallback(
    (name: string, path: string, branch: string, startPoint: string | null) =>
      runOptimisticMutationWithMessage(
        (prev) => ({
          ...prev,
          worktrees: [...prev.worktrees, { name, path, head: null, isMain: false, isLocked: false, isPrunable: false }],
        }),
        () => client.createWorktree(repoPath, name, path, branch, startPoint),
      ),
    [client, runOptimisticMutationWithMessage, repoPath],
  );
  const removeWorktree = useCallback(
    (name: string) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, worktrees: prev.worktrees.filter((worktree) => worktree.name !== name) }),
        () => client.removeWorktree(repoPath, name),
      ),
    [client, runOptimisticMutation, repoPath],
  );
  const pruneWorktrees = useCallback(
    () => runMutation(() => client.pruneWorktrees(repoPath)),
    [client, runMutation, repoPath],
  );

  return { createWorktree, removeWorktree, pruneWorktrees };
}
```

Update the `useWorktreeActions` call site in `useAppState.ts` to match (drop the
`runMutationWithMessage` argument that Task 2 had passed):

```ts
  const { createWorktree, removeWorktree, pruneWorktrees } = useWorktreeActions(
    client,
    repoPath,
    runMutation,
    runOptimisticMutation,
    runOptimisticMutationWithMessage,
  );
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx pnpm test -- --run useAppState`
Expected: PASS.

- [ ] **Step 5: Lint, typecheck, and commit**

Run: `cd frontend && npx pnpm build && npx pnpm lint`
Expected: clean.

```bash
git add frontend/src/state/useWorktreeActions.ts frontend/src/state/useAppState.ts frontend/src/state/useAppState.test.ts
git commit -m "feat(frontend): make worktree create/remove optimistic"
```

---

### Task 6: Migrate `useStashActions` (`dropStash` only)

**Files:**
- Modify: `frontend/src/state/useStashActions.ts`
- Modify: `frontend/src/state/useAppState.ts` (the `useStashActions` call site — finally drops the `state` argument, see Task 2 Step 5's note)
- Test: `frontend/src/state/useAppState.test.ts`

**Interfaces:**
- Consumes: `runOptimisticMutation` from Task 2.
- Produces: no signature change to `StashActions`. `saveStash`/`applyStash` are unchanged (see
  spec's "Explicitly out of scope").

- [ ] **Step 1: Write the failing tests**

Add to `frontend/src/state/useAppState.test.ts`:

```ts
  it("dropStash removes the stash from state before the backend call resolves, restores it on failure", async () => {
    const stashA: StashEntry = { index: 0, message: "WIP", commitId: "abc" };
    const client = transferClient({
      listStashes: async () => [stashA],
      dropStash: async () => {
        throw new Error("drop failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.dropStash(0));

    expect(result.current.state.stashes).toEqual([stashA]);
    expect(result.current.state.error).toBe("drop failed");
  });

  it("dropStash clears the selected row optimistically when dropping the currently-selected stash", async () => {
    const stashA: StashEntry = { index: 0, message: "WIP", commitId: "abc" };
    let resolveDrop: (() => void) | null = null;
    const client = transferClient({
      listStashes: async () => [stashA],
      dropStash: async () => new Promise<void>((resolve) => { resolveDrop = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    act(() => result.current.selectRow({ commitId: "abc" }));
    expect(result.current.state.selectedRow).toEqual({ commitId: "abc" });

    let dropPromise!: Promise<void>;
    act(() => {
      dropPromise = result.current.dropStash(0);
    });

    expect(result.current.state.stashes).toEqual([]);
    expect(result.current.state.selectedRow).toBe("uncommitted");

    await act(async () => {
      resolveDrop?.();
      await dropPromise;
    });
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx pnpm test -- --run useAppState -t "dropStash removes the stash"`
Expected: FAIL.

- [ ] **Step 3: Implement**

```ts
import { useCallback } from "react";
import type { RepoClient } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type { RunMutation, RunOptimisticMutation } from "./useMutationRunner";

export interface StashActions {
  saveStash(): Promise<void>;
  applyStash(index: number): Promise<void>;
  dropStash(index: number): Promise<void>;
}

export function useStashActions(
  client: RepoClient,
  repoPath: string,
  runMutation: RunMutation,
  runOptimisticMutation: RunOptimisticMutation,
  setState: (updater: (prev: AppState) => AppState) => void,
): StashActions {
  const saveStash = useCallback(
    () => runMutation(() => client.saveStash(repoPath)),
    [client, runMutation, repoPath],
  );
  const applyStash = useCallback(
    (index: number) => runMutation(() => client.applyStash(repoPath, index)),
    [client, runMutation, repoPath],
  );
  const dropStash = useCallback(
    (index: number) =>
      runOptimisticMutation(
        (prev) => {
          const droppedCommitId = prev.stashes[index]?.commitId;
          const dropsSelectedStash =
            droppedCommitId !== undefined &&
            typeof prev.selectedRow === "object" &&
            prev.selectedRow.commitId === droppedCommitId;
          return {
            ...prev,
            stashes: prev.stashes.filter((_, i) => i !== index),
            selectedRow: dropsSelectedStash ? "uncommitted" : prev.selectedRow,
          };
        },
        () => client.dropStash(repoPath, index),
      ),
    [client, runOptimisticMutation, repoPath],
  );

  return { saveStash, applyStash, dropStash };
}
```

Note `setState` is now unused in this file (it was only ever used by the old `dropStash`'s
post-success side effect, which the optimistic transform now folds in) — remove the parameter
and update both this function's signature above (already done — it's dropped) and the call site.

Update the `useStashActions` call site in `frontend/src/state/useAppState.ts` (finally dropping
`state`, per Task 2 Step 5's deferred note, and dropping `setState` too since it's now unused):

```ts
  const { saveStash, applyStash, dropStash } = useStashActions(client, repoPath, runMutation, runOptimisticMutation);
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx pnpm test -- --run useAppState`
Expected: PASS.

- [ ] **Step 5: Lint, typecheck, and commit**

Run: `cd frontend && npx pnpm build && npx pnpm lint`
Expected: clean.

```bash
git add frontend/src/state/useStashActions.ts frontend/src/state/useAppState.ts frontend/src/state/useAppState.test.ts
git commit -m "feat(frontend): make dropStash optimistic"
```

---

### Task 7: Migrate tag actions in `useRemoteTransferActions` (`createTag`, `deleteTag`)

**Files:**
- Modify: `frontend/src/state/useRemoteTransferActions.ts`
- Test: `frontend/src/state/useAppState.test.ts`

**Interfaces:**
- Consumes: `runOptimisticMutation`, `runOptimisticMutationWithMessage` (real names now) from Task 2.
- Produces: no signature change.

`TagInfo`'s `targetId`/`taggerName`/`timestamp` fields aren't rendered by `TagPanel` (confirmed:
it only reads `tag.name` and `tag.annotated`), so a placeholder value for those three is safe —
nothing displays them before `refresh()` fills in the real values moments later.

- [ ] **Step 1: Write the failing tests**

Add to `frontend/src/state/useAppState.test.ts`:

```ts
  it("createTag adds the tag to state before the backend call resolves", async () => {
    let resolveCreate: (() => void) | null = null;
    const client = transferClient({
      listTags: async () => [],
      createTag: async () => new Promise<void>((resolve) => { resolveCreate = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let createPromise!: Promise<string | null>;
    act(() => {
      createPromise = result.current.createTag("v1.0.0", null);
    });

    expect(result.current.state.tags).toHaveLength(1);
    expect(result.current.state.tags[0].name).toBe("v1.0.0");
    expect(result.current.state.tags[0].annotated).toBe(false);

    await act(async () => {
      resolveCreate?.();
      await createPromise;
    });
  });

  it("deleteTag removes the tag from state before the backend call resolves, restores it on failure", async () => {
    const tagA: TagInfo = { name: "v1.0.0", targetId: "abc", annotated: false, message: null, taggerName: null, timestamp: null };
    const client = transferClient({
      listTags: async () => [tagA],
      deleteTag: async () => {
        throw new Error("delete failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.deleteTag("v1.0.0"));

    expect(result.current.state.tags).toEqual([tagA]);
    expect(result.current.state.error).toBe("delete failed");
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx pnpm test -- --run useAppState -t "createTag adds the tag"`
Expected: FAIL.

- [ ] **Step 3: Implement**

In `frontend/src/state/useRemoteTransferActions.ts`, replace the `createTag`/`deleteTag` bodies:

```ts
  const createTag = useCallback(
    (name: string, message: string | null) =>
      runOptimisticMutationWithMessage(
        (prev) => ({
          ...prev,
          tags: [
            ...prev.tags,
            { name, targetId: "", annotated: message !== null, message, taggerName: null, timestamp: null },
          ],
        }),
        () => client.createTag(repoPath, name, message),
      ),
    [client, runOptimisticMutationWithMessage, repoPath],
  );
  const deleteTag = useCallback(
    (name: string) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, tags: prev.tags.filter((tag) => tag.name !== name) }),
        () => client.deleteTag(repoPath, name),
      ),
    [client, runOptimisticMutation, repoPath],
  );
```

(The `runOptimisticMutation`/`runOptimisticMutationWithMessage`/`runOptimisticMutationWithOutcome`
parameters were already added to this hook's signature in Task 2 Step 4, currently `_`-prefixed —
rename `_runOptimisticMutation` to `runOptimisticMutation` and `_runOptimisticMutationWithMessage`
to `runOptimisticMutationWithMessage` in the function signature now that they're used; leave
`_runOptimisticMutationWithOutcome` prefixed until Task 8 uses it.)

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx pnpm test -- --run useAppState`
Expected: PASS.

- [ ] **Step 5: Lint, typecheck, and commit**

Run: `cd frontend && npx pnpm build && npx pnpm lint`
Expected: clean.

```bash
git add frontend/src/state/useRemoteTransferActions.ts frontend/src/state/useAppState.test.ts
git commit -m "feat(frontend): make tag create/delete optimistic"
```

---

### Task 8: Migrate remote CRUD in `useRemoteTransferActions` (`addRemote`, `removeRemote`, `renameRemote`, `updateRemoteUrls`)

**Files:**
- Modify: `frontend/src/state/useRemoteTransferActions.ts`
- Test: `frontend/src/state/useAppState.test.ts`

**Interfaces:**
- Consumes: `runOptimisticMutation`, `runOptimisticMutationWithMessage`, `runOptimisticMutationWithOutcome` (all real names now) from Task 2.
- Produces: no signature change. `saveHttpsCredential`/`forgetHttpsCredential`/`setRemoteAuthMode`
  are unchanged — not list-CRUD, out of this plan's scope.

`renameRemote`'s migration also simplifies its implementation: the current hand-rolled
`let renamed = false` closure-tracking pattern (working around `runMutation` swallowing the
success/failure signal) is replaced entirely by `runOptimisticMutationWithOutcome`, which
resolves the boolean directly.

- [ ] **Step 1: Write the failing tests**

Add to `frontend/src/state/useAppState.test.ts`:

```ts
  it("addRemote adds the remote to state before the backend call resolves", async () => {
    let resolveAdd: (() => void) | null = null;
    const client = transferClient({
      listRemotes: async () => [],
      addRemote: async () => new Promise<void>((resolve) => { resolveAdd = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let addPromise!: Promise<string | null>;
    act(() => {
      addPromise = result.current.addRemote("upstream", "../upstream.git", null);
    });

    expect(result.current.state.remotes).toEqual([
      { name: "upstream", fetchUrl: "../upstream.git", pushUrl: null, authMode: null, authUsername: null },
    ]);

    await act(async () => {
      resolveAdd?.();
      await addPromise;
    });
  });

  it("removeRemote removes the remote from state before the backend call resolves, restores it on failure", async () => {
    const remoteA: RemoteInfo = { name: "upstream", fetchUrl: "../upstream.git", pushUrl: null, authMode: null, authUsername: null };
    const client = transferClient({
      listRemotes: async () => [remoteA],
      removeRemote: async () => {
        throw new Error("remove failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    await act(() => result.current.removeRemote("upstream", false));

    expect(result.current.state.remotes).toEqual([remoteA]);
    expect(result.current.state.error).toBe("remove failed");
  });

  it("renameRemote renames the remote in state before the backend call resolves, resolves true on success", async () => {
    const remoteA: RemoteInfo = { name: "old", fetchUrl: "../r.git", pushUrl: null, authMode: null, authUsername: null };
    let resolveRename: (() => void) | null = null;
    const client = transferClient({
      listRemotes: async () => [remoteA],
      renameRemote: async () => new Promise<void>((resolve) => { resolveRename = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let renamePromise!: Promise<boolean>;
    act(() => {
      renamePromise = result.current.renameRemote("old", "new");
    });

    expect(result.current.state.remotes[0].name).toBe("new");

    let succeeded!: boolean;
    await act(async () => {
      resolveRename?.();
      succeeded = await renamePromise;
    });
    expect(succeeded).toBe(true);
  });

  it("renameRemote resolves false and restores the original name on failure", async () => {
    const remoteA: RemoteInfo = { name: "old", fetchUrl: "../r.git", pushUrl: null, authMode: null, authUsername: null };
    const client = transferClient({
      listRemotes: async () => [remoteA],
      renameRemote: async () => {
        throw new Error("rename failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    const succeeded = await act(() => result.current.renameRemote("old", "new"));

    expect(succeeded).toBe(false);
    expect(result.current.state.remotes).toEqual([remoteA]);
  });

  it("updateRemoteUrls updates the URLs in state before the backend call resolves", async () => {
    const remoteA: RemoteInfo = { name: "origin", fetchUrl: "../old.git", pushUrl: null, authMode: null, authUsername: null };
    let resolveUpdate: (() => void) | null = null;
    const client = transferClient({
      listRemotes: async () => [remoteA],
      updateRemoteUrls: async () => new Promise<void>((resolve) => { resolveUpdate = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let updatePromise!: Promise<void>;
    act(() => {
      updatePromise = result.current.updateRemoteUrls("origin", "../new.git", "../push.git");
    });

    expect(result.current.state.remotes[0].fetchUrl).toBe("../new.git");
    expect(result.current.state.remotes[0].pushUrl).toBe("../push.git");

    await act(async () => {
      resolveUpdate?.();
      await updatePromise;
    });
  });
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx pnpm test -- --run useAppState -t "addRemote adds the remote"`
Expected: FAIL.

- [ ] **Step 3: Implement**

Replace the four bodies in `frontend/src/state/useRemoteTransferActions.ts`:

```ts
  const addRemote = useCallback(
    (name: string, fetchUrl: string, pushUrl: string | null) =>
      runOptimisticMutationWithMessage(
        (prev) => ({
          ...prev,
          remotes: [...prev.remotes, { name, fetchUrl, pushUrl, authMode: null, authUsername: null }],
        }),
        () => client.addRemote(repoPath, name, fetchUrl, pushUrl),
      ),
    [client, runOptimisticMutationWithMessage, repoPath],
  );
  const renameRemote = useCallback(
    (oldName: string, newName: string) =>
      runOptimisticMutationWithOutcome(
        (prev) => ({
          ...prev,
          remotes: prev.remotes.map((remote) => (remote.name === oldName ? { ...remote, name: newName } : remote)),
        }),
        () => client.renameRemote(repoPath, oldName, newName),
      ),
    [client, runOptimisticMutationWithOutcome, repoPath],
  );
  const updateRemoteUrls = useCallback(
    (name: string, fetchUrl: string, pushUrl: string | null) =>
      runOptimisticMutation(
        (prev) => ({
          ...prev,
          remotes: prev.remotes.map((remote) => (remote.name === name ? { ...remote, fetchUrl, pushUrl } : remote)),
        }),
        () => client.updateRemoteUrls(repoPath, name, fetchUrl, pushUrl),
      ),
    [client, runOptimisticMutation, repoPath],
  );
  const removeRemote = useCallback(
    (name: string, clearUpstreams: boolean) =>
      runOptimisticMutation(
        (prev) => ({ ...prev, remotes: prev.remotes.filter((remote) => remote.name !== name) }),
        () => client.removeRemote(repoPath, name, clearUpstreams),
      ),
    [client, runOptimisticMutation, repoPath],
  );
```

Rename `_runOptimisticMutationWithOutcome` to `runOptimisticMutationWithOutcome` in this file's
function signature now that it's used.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx pnpm test -- --run useAppState`
Expected: PASS.

- [ ] **Step 5: Lint, typecheck, and commit**

Run: `cd frontend && npx pnpm build && npx pnpm lint`
Expected: clean.

```bash
git add frontend/src/state/useRemoteTransferActions.ts frontend/src/state/useAppState.test.ts
git commit -m "feat(frontend): make remote add/remove/rename/update-urls optimistic"
```

---

### Task 9: Migrate upstream actions in `useRemoteTransferActions` (`setCurrentUpstream`, `clearCurrentUpstream`)

**Files:**
- Modify: `frontend/src/state/useRemoteTransferActions.ts`
- Test: `frontend/src/state/useAppState.test.ts`

**Interfaces:**
- Consumes: `runOptimisticMutation` from Task 2 (already renamed in Task 7).
- Produces: no signature change. This is the last task in this plan.

`setCurrentUpstream` needs the current branch's name to build the `UpstreamInfo` it predicts —
read via `prev.branches.find((branch) => branch.isCurrent)?.name`. If no branch is marked
current (shouldn't happen in practice, but the type allows it), the transform is a no-op and the
mutation falls back to plain fetch-then-refresh behavior for that one call.

- [ ] **Step 1: Write the failing tests**

Add to `frontend/src/state/useAppState.test.ts`:

```ts
  it("setCurrentUpstream updates state before the backend call resolves", async () => {
    const branchA: BranchInfo = { name: "main", isCurrent: true };
    let resolveSet: (() => void) | null = null;
    const client = transferClient({
      listBranches: async () => [branchA],
      getCurrentUpstream: async () => null,
      setCurrentUpstream: async () => new Promise<void>((resolve) => { resolveSet = resolve; }),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());

    let setPromise!: Promise<void>;
    act(() => {
      setPromise = result.current.setCurrentUpstream("origin", "main");
    });

    expect(result.current.state.upstream).toEqual({ localBranch: "main", remoteName: "origin", remoteBranch: "main" });

    await act(async () => {
      resolveSet?.();
      await setPromise;
    });
  });

  it("clearCurrentUpstream clears state before the backend call resolves, restores it on failure", async () => {
    const branchA: BranchInfo = { name: "main", isCurrent: true };
    const client = transferClient({
      listBranches: async () => [branchA],
      getCurrentUpstream: async () => upstream,
      clearCurrentUpstream: async () => {
        throw new Error("clear failed");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    expect(result.current.state.upstream).toEqual(upstream);

    await act(() => result.current.clearCurrentUpstream());

    expect(result.current.state.upstream).toEqual(upstream);
    expect(result.current.state.error).toBe("clear failed");
  });
```

(`upstream` here is the file-level `const upstream: UpstreamInfo = { localBranch: "main",
remoteName: "origin", remoteBranch: "main" };` already declared near the top of
`useAppState.test.ts` — reuse it, don't redeclare.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd frontend && npx pnpm test -- --run useAppState -t "setCurrentUpstream updates state"`
Expected: FAIL.

- [ ] **Step 3: Implement**

Replace the two bodies in `frontend/src/state/useRemoteTransferActions.ts`:

```ts
  const setCurrentUpstream = useCallback(
    (remoteName: string, remoteBranch: string) =>
      runOptimisticMutation(
        (prev) => {
          const localBranch = prev.branches.find((branch) => branch.isCurrent)?.name;
          if (localBranch === undefined) return prev;
          return { ...prev, upstream: { localBranch, remoteName, remoteBranch }, pullOutcome: null };
        },
        () => client.setCurrentUpstream(repoPath, remoteName, remoteBranch),
      ),
    [client, runOptimisticMutation, repoPath],
  );
  const clearCurrentUpstream = useCallback(
    () =>
      runOptimisticMutation(
        (prev) => ({ ...prev, upstream: null, pullOutcome: null }),
        () => client.clearCurrentUpstream(repoPath),
      ),
    [client, runOptimisticMutation, repoPath],
  );
```

`setState` may now be unused in this file if nothing else references it — check: the
`subscribeTransferProgress` effect, `startTransfer`, and `pullCurrentUpstream` all still use
`setState` directly, so it stays in the parameter list.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd frontend && npx pnpm test -- --run`
Expected: PASS — full suite, not just `useAppState`, as the final check for this plan.

- [ ] **Step 5: Full verification and commit**

Run: `cd frontend && npx pnpm build && npx pnpm lint && npx pnpm test -- --run`
Expected: all clean — build, lint, and the complete test suite (490+ tests before this plan,
plus every test added across Tasks 1-9).

```bash
git add frontend/src/state/useRemoteTransferActions.ts frontend/src/state/useAppState.test.ts
git commit -m "feat(frontend): make upstream set/clear optimistic

Completes the optimistic-UI pass (issue #32/UX-005): every mutation with
a predictable, list-shaped result now updates AppState immediately, with
whole-state snapshot/restore on failure. Network/non-deterministic
mutations (fetch/push/pull/merge/rebase/forge, hunk-level staging,
commit, applyStash, switchBranch, submodule update) intentionally stay
fetch-then-refresh — see docs/superpowers/specs/2026-08-26-optimistic-ui-design.md
for why each is excluded.

Fixes #32."
```
