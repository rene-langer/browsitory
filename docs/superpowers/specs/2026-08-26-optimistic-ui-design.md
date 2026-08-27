# Optimistic UI for deterministic mutations (Pass 1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan task-by-task once
> `writing-plans` has turned this spec into a task list.

**Goal:** Every mutation in `useAppState.ts`'s split hooks is currently fetch-then-refresh
(`useMutationRunner.ts`'s `runMutation`/`runMutationWithOutcome`/`runMutationWithMessage`: set
`pending`, await the backend call, `await refresh()`, clear `pending`). For mutations whose
result is fully predictable client-side, this means a visible round-trip for what should feel
instant — working against the project's own stated UX bar of matching Sublime Merge's
staging-UI speed (`CLAUDE.local.md`). This pass adds an optimistic-update path for the subset of
mutations where the frontend can predict the outcome exactly, with rollback on failure.

**Spec:** GitHub issue https://github.com/rene-langer/browsitory/issues/32 (UX-005, from the
2026-08-26 full app audit, epic #33). The audit's own recommended fix was a single-row
stage/unstage slice, explicitly warning against "a blanket optimistic-everywhere rewrite" —
chat-approved scope decision for this pass: go broader than that (every mutation with a
predictable, list-shaped result), but not literally everywhere. See "Explicitly out of scope"
below for what stays fetch-then-refresh and why.

## Architecture

One new function in `frontend/src/state/useMutationRunner.ts`, alongside the existing three
`runMutation*` variants:

```ts
export type OptimisticUpdate = (prev: AppState) => AppState;
export type RunOptimisticMutation = (optimisticUpdate: OptimisticUpdate, mutate: () => Promise<void>) => Promise<void>;
export type RunOptimisticMutationWithMessage =
  (optimisticUpdate: OptimisticUpdate, mutate: () => Promise<void>) => Promise<string | null>;

function runOptimisticMutation(optimisticUpdate, mutate) {
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
}
```

`runOptimisticMutationWithMessage` is the same shape as `runMutationWithMessage` (resolves to
`null` on success, the failure message on failure — used by the create-form actions whose
failure surfaces inline next to the form per issue #30, not on the shared banner), with the same
snapshot/restore added. A third variant, `runOptimisticMutationWithOutcome` (mirrors
`runMutationWithOutcome`: resolves `true`/`false`), is needed for the one Tier-1 mutation with
that shape — `renameRemote` (`Promise<boolean>`, so `RemotePanel` can tell success from failure
without going through `state.error`).

**Why whole-`AppState` snapshot/restore, not a per-call-site inverse function:** cheap (one
object), and correctness is trivial to reason about — failure always means "exactly what it
looked like before this call touched anything," full stop, no per-mutation rollback logic to
get subtly wrong. The one race this would otherwise have — two optimistic mutations overlapping,
one's rollback clobbering the other's already-applied change — is already ruled out by the
existing global `pending` lock (`AppState.pending`), which every `runMutation*` variant
(including this one) still sets during its round-trip. Chat-approved: `pending` stays global
for this pass rather than being narrowed to per-item locking — that's issue #31's territory
(already addressed with a tooltip explaining *why*, not scope-narrowed), and conflating the two
would expand this pass well beyond "make predictable mutations feel instant."

## Scope: which mutations get the optimistic treatment

**Tier 1 — list CRUD** (add/remove/rename an entry in a flat array/record already in
`AppState`, mechanical and low-risk):

| Hook | Mutations |
|---|---|
| `useBranchActions` | `createBranch`, `deleteBranch`, `renameBranch` |
| `useWorktreeActions` | `createWorktree`, `removeWorktree` |
| `useRemoteTransferActions` | `addRemote`, `removeRemote`, `renameRemote`, `updateRemoteUrls`, `setCurrentUpstream`, `clearCurrentUpstream` |
| `useStashActions` | `dropStash` |
| `useRemoteTransferActions` (tags) | `createTag`, `deleteTag` |

**Tier 2 — status toggle** (flip `StatusEntry.staged` on an existing entry in `state.status`):

| Hook | Mutations |
|---|---|
| `useStagingActions` | `stageFile`, `unstageFile`, `stageAllFiles`, `unstageAllFiles` |

Each keeps its exact current return type — `Promise<void>`, or `Promise<string | null>` for the
create-form ones (`createBranch`, `createWorktree`, `createTag`, `addRemote`) per issue #30.
`runOptimisticMutation`/`runOptimisticMutationWithMessage` replace the plain variant underneath;
no caller-facing signature changes.

### Explicitly out of scope (stays fetch-then-refresh), and why

- **`applyStash`** — removing the entry from the stash list is a safe Tier-1-shaped optimistic
  update, but stash *apply*'s effect on `state.status` (and potentially conflicts) is unknown
  until the backend actually applies it. Only a true no-op-if-wrong optimistic slice is worth
  doing; faking the status-list result isn't.
- **`saveStash`** — unlike every other Tier-1 create action, there's no way to construct a
  non-misleading placeholder `StashEntry`: `message` is backend-auto-generated (not a user
  input, unlike `createTag`'s message) and `commitId` (used as `BranchSwitcher`'s React `key` for
  the row) is a brand-new commit oid the backend computes — both entirely unknowable
  client-side. `dropStash` stays in scope since it only needs the already-known `index` to
  filter the existing entry out, no synthesis required.
- **Hunk-level `stageHunk`/`unstageHunk`/`discardHunk`** — diff content isn't held in `AppState`
  at all; `DiffPane` fetches hunks on demand via IPC. An optimistic hunk change would need new
  local state in `DiffPane` (or lifting diffs into shared state), not the
  `runOptimisticMutation` pattern this spec adds. A larger, separate effort if pursued.
- **`commit`** — would mean synthesizing a fake commit into `state.commits` ahead of the graph
  layout (`commitGraphLayout.ts`'s lane assignment), real risk of a visually-wrong graph
  flashing before `refresh()` corrects it. Not worth the risk for one action.
- **`switchBranch`** — touches nearly everything at once (status, diffs, current-branch
  pointer); not a "flip one field" case.
- **`initSubmodule`/`updateSubmodule`** — a network fetch (see issue #23/SEC-002's validation
  work), same class as fetch/push/pull below.
- **`fetchRemote`, `pushCurrentBranch`, `pushTags`, `pullCurrentUpstream`, merge/rebase actions,
  forge/PR actions** — network-dependent or genuinely unpredictable outcomes (conflicts). These
  already have their own progress/outcome UI (`TransferPanel`, merge/rebase panels, pull-diverged
  dialog) — chat-confirmed in this design's first question: optimism here would mean showing a
  result that might get visibly reverted, which the audit itself warned against and this pass
  explicitly avoids.

## Data flow

Example, `deleteBranch(name)` (in `useBranchActions.ts`):

```ts
const deleteBranch = useCallback(
  (name: string) =>
    runOptimisticMutation(
      (prev) => ({ ...prev, branches: prev.branches.filter((branch) => branch.name !== name) }),
      () => client.deleteBranch(repoPath, name),
    ),
  [client, repoPath, runOptimisticMutation],
);
```

`stageFile(path)` (in `useStagingActions.ts`):

```ts
const stageFile = useCallback(
  (path: string) =>
    runOptimisticMutation(
      (prev) => ({
        ...prev,
        status: prev.status.map((entry) => (entry.path === path ? { ...entry, staged: true } : entry)),
      }),
      () => client.stageFile(repoPath, path),
    ),
  [client, repoPath, runOptimisticMutation],
);
```

`stageAllFiles(paths)`/`unstageAllFiles(paths)` map the same transform over every path in the
batch in one `optimisticUpdate`, matching the existing single-`runMutation`-per-batch behavior
(one `pending`/refresh cycle for the whole batch, not one per file — see the existing doc
comment on these in `useAppState.ts`).

Create-form actions (`createBranch`, `addRemote`, `createTag`, `createWorktree`) use
`runOptimisticMutationWithMessage`: optimistic list-append immediately, and on failure both the
snapshot-restore (removes the optimistically-added entry) and the message resolve so the form
can show its own inline error and keep the user's typed input, exactly like today.

## Error handling

Unchanged surfacing pattern from issue #30: `runOptimisticMutation` failures set `state.error`
(dismissible banner); `runOptimisticMutationWithMessage` failures resolve the message for the
calling form to render inline instead. What's new is *only* that the optimistically-applied
change gets rolled back (via the whole-state snapshot restore) before the error is shown, so the
UI never lingers showing a change that didn't actually happen.

## Testing

Two-part test per migrated mutation, using this repo's existing `RepoClient`-mock convention
(`frontend/src/state/*.test.ts`, no `@tauri-apps/api` mocking):

1. **Optimistic-application check** — call the action with a mock `RepoClient` method whose
   promise is held open (not yet resolved/rejected), then assert the relevant `AppState` slice
   already reflects the change *before* awaiting the action's own returned promise. This is the
   behavior this spec exists to add — every migrated mutation needs this assertion or the
   change isn't actually tested.
2. **Rollback-on-failure check** — reject the mock, await the action, assert the relevant
   `AppState` slice is back to its pre-call value and `state.error` (or the resolved message, for
   the `WithMessage` variants) reflects the failure.

`useMutationRunner.test.ts` (new, or added to if it exists after the #27 split) covers
`runOptimisticMutation`/`runOptimisticMutationWithMessage` directly at the unit level
(snapshot/apply/restore mechanics); each domain hook's existing test file adds the two checks
above per migrated mutation rather than re-testing the mechanism.

## Global constraints

- No new npm dependencies.
- Zero change to any exported hook's function signatures/return types — this is an internal
  implementation swap under `useMutationRunner`, not a public-API change (mirrors issue #27's
  own constraint: `useAppState`'s `AppState`/`UseAppStateResult` contract stays stable).
- `pending` stays a single global flag on `AppState` for this pass (see Architecture above for
  why) — do not narrow it to per-item state as part of this work.
- Every migrated mutation must have both a passing optimistic-application test and a
  rollback-on-failure test before its task is considered done (this repo's
  `superpowers:test-driven-development` convention).
