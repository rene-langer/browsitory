# Multi-select Commits + Interactive Rebase Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ctrl/Cmd+click commits in the graph to build a selection, then open the interactive rebase planner on it, or squash / fixup / drop the selection through the planner.

**Architecture:** Frontend only. The backend already takes `startRebase(onto, plan[])` with per-commit actions. Two pure `lib/` modules own selection logic and "selection → rebase base + presets". `CommitGraph` swaps its Shift-only `squashRange` for a real selection set and shows multi-select actions in the context menu and a toolbar. App state's `squashPreset: Set` becomes `rebasePreset: Map<id, action>`. `RebasePlanner` takes those presets and validates that every preset commit is in the planned span.

**Tech Stack:** React + TypeScript, Vitest + Testing Library, WebdriverIO (Tauri e2e).

**Spec:** `docs/superpowers/specs/2026-10-05-multi-select-rebase-design.md`

## Global Constraints

- No new dependencies (license policy in `CLAUDE.md`).
- Frontend tests mock `RepoClient`; never mock `@tauri-apps/api` or `postMessage`.
- No Rust changes. `git-core` / `repo-service` / Tauri / sidecar stay untouched.
- Planner span is always `commitsSince(onto)`: first-parent history from HEAD down to the oldest selected commit's parent. Unselected commits in the span stay `Pick`.
- Squash/Fixup fold a commit into the entry *before* it in the plan. Therefore Squash/Fixup are offered only when the selection is one unbroken first-parent run. Drop and "Interactive rebase…" work for any selection.
- Merge commits and root commits cannot be rebased: every selected commit must have exactly one parent.
- Pre-push hook (`scripts/check-changelog.py`) needs a `CHANGELOG.md` entry because `frontend/src/` changes (Task 6).
- Run frontend commands from `frontend/`: `pnpm test -- --run`, `pnpm lint`, `pnpm build`.

## File Structure

| File | Responsibility |
|---|---|
| Create `frontend/src/lib/commitSelection.ts` | Pure selection model: select-only, toggle, extend-to-range |
| Create `frontend/src/lib/commitSelection.test.ts` | Tests for the above |
| Create `frontend/src/lib/rebaseSelection.ts` | `PresetAction` type, selection → `{onto, contiguous, ids}`, preset map builder |
| Create `frontend/src/lib/rebaseSelection.test.ts` | Tests for the above |
| Modify `frontend/src/components/RebasePlanner.tsx` (+ test) | `presetActions` prop replaces `presetSquashIds`; missing-preset guard |
| Modify `frontend/src/state/useMergeRebaseActions.ts`, `useAppState.ts` (+ tests) | `openSquashPlanner` → `openRebaseSelection`; `squashPreset` → `rebasePreset` |
| Modify `frontend/src/components/CommitGraph.tsx` (+ test, + `.module.css`) | Selection state, Ctrl/Shift click, menu + toolbar |
| Modify `frontend/src/App.tsx` | Wire new props |
| Modify `frontend/src/lib/commands.test.ts`, `state/useMutationRunner.test.ts` | Rename fixtures |
| Modify `e2e/specs/rebase.spec.ts` | One multi-select flow |
| Modify `docs/USER_GUIDE.md`, `CHANGELOG.md` | Docs |

---

### Task 1: Selection model (`lib/commitSelection.ts`)

**Files:**
- Create: `frontend/src/lib/commitSelection.ts`
- Test: `frontend/src/lib/commitSelection.test.ts`

**Interfaces:**
- Produces:
  - `interface CommitSelection { ids: ReadonlySet<string>; anchorId: string | null }`
  - `const EMPTY_SELECTION: CommitSelection`
  - `selectOnly(id: string): CommitSelection`
  - `toggleCommit(selection: CommitSelection, id: string): CommitSelection`
  - `extendSelection(selection: CommitSelection, commits: GraphCommit[], id: string): CommitSelection`

- [ ] **Step 1: Write the failing tests**

```ts
// frontend/src/lib/commitSelection.test.ts
import { describe, expect, it } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import {
  EMPTY_SELECTION,
  extendSelection,
  selectOnly,
  toggleCommit,
} from "./commitSelection";

function commit(id: string): GraphCommit {
  return {
    id,
    shortId: id,
    summary: id,
    authorName: "Rene",
    authorEmail: "rene@example.com",
    timestamp: 1,
    parentIds: [],
    branchRefs: [],
    remoteBranchRefs: [],
  };
}

// Newest-first, like the graph: E is the newest, A the oldest.
const commits = ["E", "D", "C", "B", "A"].map(commit);

describe("commitSelection", () => {
  it("selectOnly selects one commit and anchors on it", () => {
    expect(selectOnly("C")).toEqual({ ids: new Set(["C"]), anchorId: "C" });
  });

  it("toggleCommit adds an unselected commit and moves the anchor to it", () => {
    const next = toggleCommit(selectOnly("E"), "C");
    expect(next.ids).toEqual(new Set(["E", "C"]));
    expect(next.anchorId).toBe("C");
  });

  it("toggleCommit removes a selected commit and re-anchors on a remaining one", () => {
    const both = toggleCommit(selectOnly("E"), "C");
    const next = toggleCommit(both, "C");
    expect(next.ids).toEqual(new Set(["E"]));
    expect(next.anchorId).toBe("E");
  });

  it("toggleCommit removing the last commit yields an empty selection", () => {
    expect(toggleCommit(selectOnly("E"), "E")).toEqual(EMPTY_SELECTION);
  });

  it("extendSelection selects the whole range between anchor and target, in either direction", () => {
    expect(extendSelection(selectOnly("D"), commits, "B").ids).toEqual(new Set(["D", "C", "B"]));
    expect(extendSelection(selectOnly("B"), commits, "D").ids).toEqual(new Set(["D", "C", "B"]));
  });

  it("extendSelection keeps the anchor so a later shift-click pivots on it", () => {
    const first = extendSelection(selectOnly("D"), commits, "B");
    expect(first.anchorId).toBe("D");
    expect(extendSelection(first, commits, "E").ids).toEqual(new Set(["E", "D"]));
  });

  it("extendSelection without a usable anchor falls back to selecting just the target", () => {
    expect(extendSelection(EMPTY_SELECTION, commits, "C")).toEqual(selectOnly("C"));
    expect(extendSelection({ ids: new Set(["Z"]), anchorId: "Z" }, commits, "C")).toEqual(
      selectOnly("C"),
    );
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/lib/commitSelection.test.ts`
Expected: FAIL (module `./commitSelection` not found).

- [ ] **Step 3: Implement**

```ts
// frontend/src/lib/commitSelection.ts
import type { GraphCommit } from "../ipc/RepoClient";

// Which graph commits are selected for a multi-commit action. `anchorId` is the pivot for
// Shift-extend; it is always a member of `ids` unless `ids` is empty.
export interface CommitSelection {
  ids: ReadonlySet<string>;
  anchorId: string | null;
}

export const EMPTY_SELECTION: CommitSelection = { ids: new Set(), anchorId: null };

export function selectOnly(id: string): CommitSelection {
  return { ids: new Set([id]), anchorId: id };
}

export function toggleCommit(selection: CommitSelection, id: string): CommitSelection {
  const ids = new Set(selection.ids);
  if (ids.has(id)) {
    ids.delete(id);
    if (ids.size === 0) return EMPTY_SELECTION;
    const anchorId =
      selection.anchorId !== null && ids.has(selection.anchorId)
        ? selection.anchorId
        : [...ids][0];
    return { ids, anchorId };
  }
  ids.add(id);
  return { ids, anchorId: id };
}

// Replaces the selection with every commit between the anchor and `id` (inclusive), in graph
// order. Without a usable anchor it degrades to a plain single selection.
export function extendSelection(
  selection: CommitSelection,
  commits: GraphCommit[],
  id: string,
): CommitSelection {
  const anchorIndex = commits.findIndex((commit) => commit.id === selection.anchorId);
  const targetIndex = commits.findIndex((commit) => commit.id === id);
  if (anchorIndex === -1 || targetIndex === -1) return selectOnly(id);
  const start = Math.min(anchorIndex, targetIndex);
  const end = Math.max(anchorIndex, targetIndex);
  return {
    ids: new Set(commits.slice(start, end + 1).map((commit) => commit.id)),
    anchorId: selection.anchorId,
  };
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd frontend && pnpm test -- --run src/lib/commitSelection.test.ts`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/commitSelection.ts frontend/src/lib/commitSelection.test.ts
git commit -m "feat(frontend): commit selection model for multi-select"
```

---

### Task 2: Selection → rebase plan helper (`lib/rebaseSelection.ts`)

**Files:**
- Create: `frontend/src/lib/rebaseSelection.ts`
- Test: `frontend/src/lib/rebaseSelection.test.ts`

**Interfaces:**
- Consumes: `isSquashableRange(commits, startIndex, endIndex)` from `lib/commitGraphLayout.ts`.
- Produces:
  - `type PresetAction = "Squash" | "Fixup" | "Drop"`
  - `interface RebaseSelection { onto: string; contiguous: boolean; idsOldestFirst: string[] }`
  - `type RebaseSelectionResult = { ok: true; selection: RebaseSelection } | { ok: false; reason: string }`
  - `planRebaseSelection(commits: GraphCommit[], selectedIds: ReadonlySet<string>): RebaseSelectionResult`
  - `presetForAction(selection: RebaseSelection, action: PresetAction): Map<string, PresetAction>`

- [ ] **Step 1: Write the failing tests**

```ts
// frontend/src/lib/rebaseSelection.test.ts
import { describe, expect, it } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import { planRebaseSelection, presetForAction } from "./rebaseSelection";

function commit(id: string, parentIds: string[]): GraphCommit {
  return {
    id,
    shortId: id,
    summary: id,
    authorName: "Rene",
    authorEmail: "rene@example.com",
    timestamp: 1,
    parentIds,
    branchRefs: [],
    remoteBranchRefs: [],
  };
}

// Newest-first chain: E -> D -> C -> B -> A (A is a root, so it has no parent).
const chain = [
  commit("E", ["D"]),
  commit("D", ["C"]),
  commit("C", ["B"]),
  commit("B", ["A"]),
  commit("A", []),
];

describe("planRebaseSelection", () => {
  it("needs at least two selected commits", () => {
    const result = planRebaseSelection(chain, new Set(["C"]));
    expect(result).toEqual({ ok: false, reason: "Select two or more commits." });
  });

  it("bases on the parent of the oldest selected commit, oldest first", () => {
    const result = planRebaseSelection(chain, new Set(["D", "B"]));
    expect(result).toEqual({
      ok: true,
      selection: { onto: "A", contiguous: false, idsOldestFirst: ["B", "D"] },
    });
  });

  it("marks an unbroken first-parent run as contiguous", () => {
    const result = planRebaseSelection(chain, new Set(["D", "C", "B"]));
    expect(result).toMatchObject({ ok: true, selection: { onto: "A", contiguous: true } });
  });

  it("is not contiguous across a fork point even when graph rows are adjacent", () => {
    const fork = [commit("F1", ["M1"]), commit("M2", ["M1"]), commit("M1", ["R"])];
    const result = planRebaseSelection(fork, new Set(["F1", "M2"]));
    expect(result).toMatchObject({ ok: true, selection: { contiguous: false } });
  });

  it("rejects a selection containing a merge commit", () => {
    const withMerge = [commit("M", ["B", "X"]), commit("B", ["A"]), commit("A", ["R"])];
    const result = planRebaseSelection(withMerge, new Set(["M", "B"]));
    expect(result).toEqual({
      ok: false,
      reason: "Merge and root commits can't be rebased.",
    });
  });

  it("rejects a selection whose oldest commit is a root commit", () => {
    const result = planRebaseSelection(chain, new Set(["B", "A"]));
    expect(result).toEqual({
      ok: false,
      reason: "Merge and root commits can't be rebased.",
    });
  });

  it("ignores selected ids that are not in the loaded commits", () => {
    const result = planRebaseSelection(chain, new Set(["D", "B", "gone"]));
    expect(result).toMatchObject({ ok: true, selection: { idsOldestFirst: ["B", "D"] } });
  });
});

describe("presetForAction", () => {
  const selection = { onto: "A", contiguous: true, idsOldestFirst: ["B", "C", "D"] };

  it("Squash and Fixup skip the oldest commit, which has nothing before it to fold into", () => {
    expect(presetForAction(selection, "Squash")).toEqual(
      new Map([
        ["C", "Squash"],
        ["D", "Squash"],
      ]),
    );
    expect(presetForAction(selection, "Fixup")).toEqual(
      new Map([
        ["C", "Fixup"],
        ["D", "Fixup"],
      ]),
    );
  });

  it("Drop applies to every selected commit", () => {
    expect(presetForAction(selection, "Drop")).toEqual(
      new Map([
        ["B", "Drop"],
        ["C", "Drop"],
        ["D", "Drop"],
      ]),
    );
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/lib/rebaseSelection.test.ts`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement**

```ts
// frontend/src/lib/rebaseSelection.ts
import type { GraphCommit } from "../ipc/RepoClient";
import { isSquashableRange } from "./commitGraphLayout";

// Actions a graph multi-select can preset in the rebase planner. "Pick" is the planner's own
// default, so it is never preset.
export type PresetAction = "Squash" | "Fixup" | "Drop";

export interface RebaseSelection {
  // Parent of the oldest selected commit: the base the planner rebases onto.
  onto: string;
  // True when the selection is one unbroken first-parent run. Squash/Fixup fold into the plan
  // entry *before* them, so they only mean "squash these together" for such a run.
  contiguous: boolean;
  idsOldestFirst: string[];
}

export type RebaseSelectionResult =
  | { ok: true; selection: RebaseSelection }
  | { ok: false; reason: string };

export function planRebaseSelection(
  commits: GraphCommit[],
  selectedIds: ReadonlySet<string>,
): RebaseSelectionResult {
  // `commits` is newest-first, so a larger index is an older commit.
  const indices: number[] = [];
  commits.forEach((commit, index) => {
    if (selectedIds.has(commit.id)) indices.push(index);
  });
  if (indices.length < 2) return { ok: false, reason: "Select two or more commits." };

  if (indices.some((index) => commits[index].parentIds.length !== 1)) {
    return { ok: false, reason: "Merge and root commits can't be rebased." };
  }

  const newest = indices[0];
  const oldest = indices[indices.length - 1];
  return {
    ok: true,
    selection: {
      onto: commits[oldest].parentIds[0],
      contiguous:
        indices.length === oldest - newest + 1 && isSquashableRange(commits, newest, oldest),
      idsOldestFirst: [...indices].reverse().map((index) => commits[index].id),
    },
  };
}

export function presetForAction(
  selection: RebaseSelection,
  action: PresetAction,
): Map<string, PresetAction> {
  // The oldest selected commit is the first entry of the plan; Squash/Fixup there have no
  // predecessor to fold into (the planner disables them on row 0), so it stays a Pick and the
  // rest fold into it.
  const ids = action === "Drop" ? selection.idsOldestFirst : selection.idsOldestFirst.slice(1);
  return new Map(ids.map((id) => [id, action]));
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd frontend && pnpm test -- --run src/lib/rebaseSelection.test.ts`
Expected: PASS (9 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/rebaseSelection.ts frontend/src/lib/rebaseSelection.test.ts
git commit -m "feat(frontend): derive rebase base and presets from a commit selection"
```

---

### Task 3: Planner takes `presetActions`, state carries `rebasePreset`

Planner, state hook, and App wiring change together: they share one renamed type, so splitting them leaves a non-compiling tree.

**Files:**
- Modify: `frontend/src/components/RebasePlanner.tsx` (`presetSquashIds` prop ~L127-158)
- Modify: `frontend/src/components/RebasePlanner.test.tsx:381-403`
- Modify: `frontend/src/state/useMergeRebaseActions.ts`
- Modify: `frontend/src/state/useAppState.ts` (L82, L170, L223, L422, L486)
- Modify: `frontend/src/state/useAppState.test.ts:795-815`
- Modify: `frontend/src/state/useMutationRunner.test.ts:33`, `frontend/src/lib/commands.test.ts:60,119`
- Modify: `frontend/src/App.tsx:449,495` (the `onSquashCommits` line is finished in Task 4; this task only adds the planner prop)

**Interfaces:**
- Consumes: `PresetAction` from `lib/rebaseSelection.ts` (Task 2).
- Produces:
  - `AppState.rebasePreset: ReadonlyMap<string, PresetAction> | null` (replaces `squashPreset`)
  - `AppActions.openRebaseSelection(onto: string, preset: ReadonlyMap<string, PresetAction>): void` (replaces `openSquashPlanner`)
  - `RebasePlanner` prop `presetActions?: ReadonlyMap<string, PresetAction>` (replaces `presetSquashIds`)

- [ ] **Step 1: Update the planner test, add a missing-preset test**

Replace the test at `RebasePlanner.test.tsx:381-403` with:

```tsx
  it("pre-marks rows named in presetActions, with the leader's combined message filled in", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner
        repoPath={TEST_REPO_PATH}
        client={client}
        onto="base"
        onStartRebase={vi.fn()}
        onCancel={vi.fn()}
        presetActions={new Map([["bbb", "Squash"]])}
      />,
    );
    await screen.findAllByLabelText("Action");

    // Row 0 = "add a" (Pick, the leader), row 1 = "add b" (preset Squash).
    expect(screen.getAllByLabelText("Action")[0]).toHaveValue("Pick");
    expect(screen.getAllByLabelText("Action")[1]).toHaveValue("Squash");

    const combinedFields = await screen.findAllByLabelText("Combined message");
    expect(combinedFields).toHaveLength(1);
    expect(combinedFields[0]).toHaveValue("add a\n\nadd b");
  });

  it("pre-marks a preset Drop on a non-adjacent row and leaves the others as Pick", async () => {
    const client = fakeClient({ commitsSince: async () => commits });

    render(
      <RebasePlanner
        repoPath={TEST_REPO_PATH}
        client={client}
        onto="base"
        onStartRebase={vi.fn()}
        onCancel={vi.fn()}
        presetActions={new Map([["aaa", "Drop"]])}
      />,
    );
    await screen.findAllByLabelText("Action");

    expect(screen.getAllByLabelText("Action")[0]).toHaveValue("Drop");
    expect(screen.getAllByLabelText("Action")[1]).toHaveValue("Pick");
  });

  it("blocks Start and explains when a preset commit is not in the planned history", async () => {
    const client = fakeClient({ commitsSince: async () => commits });
    const onStartRebase = vi.fn();

    render(
      <RebasePlanner
        repoPath={TEST_REPO_PATH}
        client={client}
        onto="base"
        onStartRebase={onStartRebase}
        onCancel={vi.fn()}
        presetActions={new Map([["not-on-this-branch", "Drop"]])}
      />,
    );

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Some selected commits are not on the current branch",
    );
    expect(screen.getByRole("button", { name: "Start rebase" })).toBeDisabled();
  });
```

(`commits` here is the existing fixture in that file: ids `aaa` = "add a", `bbb` = "add b", oldest first. If the fixture ids differ, use its first two ids.)

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/RebasePlanner.test.tsx`
Expected: FAIL (`presetActions` unknown / TS error, no alert).

- [ ] **Step 3: Update `RebasePlanner.tsx`**

Add the import near the other imports:

```tsx
import type { PresetAction } from "../lib/rebaseSelection";
```

Replace the prop in the signature and its type (keep the surrounding props):

```tsx
  presetActions,
}: {
  // ...existing props unchanged...
  // Per-commit actions to default to instead of "Pick" — set when the plan is opened from a
  // multi-select action in the commit graph, so the selection arrives already marked instead of
  // the user re-marking every row by hand. Every id must appear in the planned span (see
  // `presetMissing`).
  presetActions?: ReadonlyMap<string, PresetAction>;
}) {
  const [rows, setRows] = useState<Row[]>([]);
  // True when a preset commit isn't in `commitsSince(onto)`, e.g. the selection includes a
  // commit from another branch. Starting would silently ignore part of the selection.
  const [presetMissing, setPresetMissing] = useState(false);
```

Replace the effect body:

```tsx
  useEffect(() => {
    let ignore = false;
    client.commitsSince(repoPath, onto).then((commits) => {
      if (!ignore) {
        const initialRows = commits.map((commit) => ({
          commit,
          actionKind: (presetActions?.get(commit.id) ?? "Pick") as ActionKind,
          rewordMessage: commit.summary,
          combinedMessage: null,
        }));
        const planned = new Set(commits.map((commit) => commit.id));
        setPresetMissing(
          presetActions !== undefined && [...presetActions.keys()].some((id) => !planned.has(id)),
        );
        setRows(recomputeGroupLeaders(initialRows, initialRows));
      }
    });
    return () => {
      ignore = true;
    };
  }, [repoPath, client, onto, presetActions]);
```

In the JSX, above `<Toolbar>`, add:

```tsx
      {presetMissing && (
        <p role="alert">
          Some selected commits are not on the current branch&apos;s history above this base, so
          they can&apos;t be part of this rebase.
        </p>
      )}
```

Change the Start button:

```tsx
        <button onClick={start} disabled={operationDisabled || presetMissing}>
```

- [ ] **Step 4: Update state: `useMergeRebaseActions.ts`**

```ts
// imports: add
import type { PresetAction } from "../lib/rebaseSelection";

// interface MergeRebaseActions: replace the openSquashPlanner line
  openRebaseSelection(onto: string, preset: ReadonlyMap<string, PresetAction>): void;

// replace openRebasePlanner + openSquashPlanner implementations
  const openRebasePlanner = useCallback(
    (commitId: string) => {
      setState((prev) => ({ ...prev, rebaseOnto: commitId, rebasePreset: null }));
    },
    [setState],
  );
  const openRebaseSelection = useCallback(
    (onto: string, preset: ReadonlyMap<string, PresetAction>) => {
      setState((prev) => ({ ...prev, rebaseOnto: onto, rebasePreset: preset }));
    },
    [setState],
  );
  const closeRebasePlanner = useCallback(() => {
    setState((prev) => ({ ...prev, rebaseOnto: null, rebasePreset: null }));
  }, [setState]);
```

In `startRebase`, change `squashPreset: null` to `rebasePreset: null`. In the returned object, rename `openSquashPlanner` to `openRebaseSelection`.

- [ ] **Step 5: Update state: `useAppState.ts`**

- L82: replace the comment and field with
  ```ts
  // Per-commit actions to default to in the rebase planner, set only when it was opened via a
  // CommitGraph multi-select action (as opposed to "Rebase onto here").
  rebasePreset: ReadonlyMap<string, PresetAction> | null;
  ```
  and import `PresetAction` from `../lib/rebaseSelection`.
- L170: `openRebaseSelection(onto: string, preset: ReadonlyMap<string, PresetAction>): void;`
- L223: `rebasePreset: null,`
- L422 and L486: rename `openSquashPlanner` to `openRebaseSelection`.

- [ ] **Step 6: Update the other tests/fixtures**

- `useMutationRunner.test.ts:33` and `commands.test.ts:60`: `squashPreset: null` → `rebasePreset: null`.
- `commands.test.ts:119`: `openSquashPlanner: vi.fn(),` → `openRebaseSelection: vi.fn(),`.
- `useAppState.test.ts:795-815`: replace the two tests with

```ts
  it("opens the rebase planner with the given preset when acting on a graph selection", async () => {
    const client = transferClient({});
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    act(() =>
      result.current.openRebaseSelection(
        "aaa",
        new Map([
          ["ccc", "Squash"],
          ["bbb", "Squash"],
        ]),
      ),
    );

    expect(result.current.state.rebaseOnto).toBe("aaa");
    expect(result.current.state.rebasePreset).toEqual(
      new Map([
        ["ccc", "Squash"],
        ["bbb", "Squash"],
      ]),
    );
  });

  it("clears the rebase preset when the rebase planner is closed", async () => {
    const client = transferClient({});
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    act(() => result.current.openRebaseSelection("aaa", new Map([["ccc", "Drop"]])));

    act(() => result.current.closeRebasePlanner());

    expect(result.current.state.rebaseOnto).toBeNull();
    expect(result.current.state.rebasePreset).toBeNull();
  });
```

- [ ] **Step 7: Update `App.tsx`**

L495: `presetSquashIds={appState.state.squashPreset ?? undefined}` → `presetActions={appState.state.rebasePreset ?? undefined}`.
L449: leave `onSquashCommits={appState.openSquashPlanner}` for now only if the tree must compile at this commit; it will not (the action is renamed). Instead make Task 3 compile by changing it to a temporary no-op-free form: **do Tasks 3 and 4 in one working session and commit Task 3's files with the L449 line replaced by `onRebaseSelection={appState.openRebaseSelection}`**. `CommitGraph` gains that prop in Task 4; until then `pnpm build` fails type-check, so run only the targeted vitest files below at this step.

- [ ] **Step 8: Run targeted tests**

Run: `cd frontend && pnpm test -- --run src/components/RebasePlanner.test.tsx src/state src/lib/commands.test.ts`
Expected: PASS.

- [ ] **Step 9: Commit (together with Task 4's `CommitGraph` change if the tree must build per commit; otherwise as a WIP commit)**

```bash
git add frontend/src/components/RebasePlanner.tsx frontend/src/components/RebasePlanner.test.tsx \
  frontend/src/state frontend/src/lib/commands.test.ts frontend/src/App.tsx
git commit -m "feat(frontend): rebase planner takes per-commit preset actions"
```

---

### Task 4: `CommitGraph` multi-select, menu and toolbar

**Files:**
- Modify: `frontend/src/components/CommitGraph.tsx`
- Modify: `frontend/src/components/CommitGraph.module.css` (add `.selectionBar`)
- Modify: `frontend/src/components/CommitGraph.test.tsx` (squash tests at ~L488-570, keyboard test at ~L656-675, `setup()` at ~L593)

**Interfaces:**
- Consumes: `selectOnly`, `toggleCommit`, `extendSelection`, `EMPTY_SELECTION`, `CommitSelection` (Task 1); `planRebaseSelection`, `presetForAction`, `PresetAction` (Task 2).
- Produces: prop `onRebaseSelection?: (onto: string, preset: ReadonlyMap<string, PresetAction>) => void` replacing `onSquashCommits`.
- UI contract (tests rely on these names): menu items and toolbar buttons labeled `Interactive rebase…`, `Squash N commits`, `Fixup N commits`, `Drop N commits`; toolbar is `role="toolbar"` with `aria-label="Selected commits"`.

- [ ] **Step 1: Rewrite the squash-related tests**

Replace the three squash tests (`shift-clicking a second commit…`, `clicking Squash N commits…`, `does not offer Squash across a fork point…`) with the block below. Add this shared fixture just above them inside the same `describe`:

```tsx
  const chainCommits: GraphCommit[] = [
    { ...commits[0], id: "D", shortId: "D", summary: "D", parentIds: ["C"] },
    { ...commits[0], id: "C", shortId: "C", summary: "C", parentIds: ["B"] },
    { ...commits[0], id: "B", shortId: "B", summary: "B", parentIds: ["A"] },
    { ...commits[0], id: "A", shortId: "A", summary: "A", parentIds: ["R"] },
  ];

  function renderChain(overrides: Partial<React.ComponentProps<typeof CommitGraph>> = {}) {
    const onRebaseSelection = vi.fn();
    render(
      <CommitGraph
        status={status}
        commits={chainCommits}
        selectedRow="uncommitted"
        pending={false}
        onSelectRow={vi.fn()}
        onBranchFromCommit={vi.fn()}
        onRebaseFromCommit={vi.fn()}
        onRebaseSelection={onRebaseSelection}
        {...overrides}
      />,
    );
    return { onRebaseSelection };
  }

  const row = (name: string) => screen.getByText(new RegExp(`^${name} `)).closest("li")!;
```

```tsx
  it("shift-clicking a second commit and right-clicking within the range shows the multi-select menu", () => {
    renderChain();

    fireEvent.click(row("C"));
    fireEvent.click(row("B"), { shiftKey: true });
    fireEvent.contextMenu(row("B"));

    expect(screen.getByRole("menuitem", { name: "Squash 2 commits" })).toBeEnabled();
    expect(screen.getByRole("menuitem", { name: "Interactive rebase…" })).toBeEnabled();
    expect(screen.queryByText("Branch from here")).not.toBeInTheDocument();
  });

  it("Squash N commits opens the planner on the oldest selected commit's parent with the newer ones preset", () => {
    const { onRebaseSelection } = renderChain();

    fireEvent.click(row("C"));
    fireEvent.click(row("B"), { shiftKey: true });
    fireEvent.contextMenu(row("B"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Squash 2 commits" }));

    // C and B fold together onto B's parent A; B (oldest) stays the surviving Pick.
    expect(onRebaseSelection).toHaveBeenCalledWith("A", new Map([["C", "Squash"]]));
  });

  it("Ctrl+click toggles a non-adjacent commit into the selection", () => {
    renderChain();

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });

    expect(row("D")).toHaveAttribute("aria-selected", "true");
    expect(row("C")).toHaveAttribute("aria-selected", "false");
    expect(row("B")).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("toolbar", { name: "Selected commits" })).toHaveTextContent(
      "2 commits selected",
    );
  });

  it("Cmd+click toggles too, and toggling a selected commit off removes it", () => {
    renderChain();

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { metaKey: true });
    fireEvent.click(row("B"), { metaKey: true });

    expect(row("B")).toHaveAttribute("aria-selected", "false");
    expect(screen.queryByRole("toolbar", { name: "Selected commits" })).not.toBeInTheDocument();
  });

  it("a non-adjacent selection offers Drop and Interactive rebase but disables Squash and Fixup", () => {
    const { onRebaseSelection } = renderChain();

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });

    expect(screen.getByRole("button", { name: "Squash 2 commits" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Fixup 2 commits" })).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "Drop 2 commits" }));
    expect(onRebaseSelection).toHaveBeenLastCalledWith(
      "A",
      new Map([
        ["B", "Drop"],
        ["D", "Drop"],
      ]),
    );

    fireEvent.click(screen.getByRole("button", { name: "Interactive rebase…" }));
    expect(onRebaseSelection).toHaveBeenLastCalledWith("A", new Map());
  });

  it("disables every multi-select action while a repository operation is pending", () => {
    renderChain({ pending: true });

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });

    for (const name of ["Interactive rebase…", "Drop 2 commits"]) {
      expect(screen.getByRole("button", { name })).toBeDisabled();
    }
  });

  it("explains why actions are disabled when the selection holds a root commit", () => {
    const rootChain: GraphCommit[] = [
      { ...commits[0], id: "B", shortId: "B", summary: "B", parentIds: ["A"] },
      { ...commits[0], id: "A", shortId: "A", summary: "A", parentIds: [] },
    ];
    renderChain({ commits: rootChain });

    fireEvent.click(row("B"));
    fireEvent.click(row("A"), { shiftKey: true });

    const drop = screen.getByRole("button", { name: "Drop 2 commits" });
    expect(drop).toBeDisabled();
    expect(drop).toHaveAttribute("title", "Merge and root commits can't be rebased.");
  });

  it("Escape collapses a multi-selection back to the focused commit", () => {
    renderChain({ selectedRow: { commitId: "D" } });

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });
    fireEvent.keyDown(screen.getByRole("listbox"), { key: "Escape" });

    expect(screen.queryByRole("toolbar", { name: "Selected commits" })).not.toBeInTheDocument();
  });

  it("right-clicking a commit outside the selection opens the normal single-commit menu", () => {
    renderChain();

    fireEvent.click(row("D"));
    fireEvent.click(row("B"), { ctrlKey: true });
    fireEvent.contextMenu(row("C"));

    expect(screen.getByRole("menuitem", { name: "Branch from here" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Drop 2 commits" })).not.toBeInTheDocument();
  });
```

Note on the Escape test: `selectedRow` is a controlled prop and `onSelectRow` is a mock, so the test passes `selectedRow={{ commitId: "D" }}` and the effective selection (derived in the component) is `{D, B}` once B is toggled. After Escape it must collapse to `{D}`.

Replace the fork test (`does not offer Squash across a fork point`) with:

```tsx
  it("disables Squash across a fork point but still offers Interactive rebase", () => {
    const forkCommits: GraphCommit[] = [
      { ...commits[0], id: "F1", shortId: "F1", summary: "F1", parentIds: ["M1"] },
      { ...commits[0], id: "M2", shortId: "M2", summary: "M2", parentIds: ["M1"] },
      { ...commits[0], id: "M1", shortId: "M1", summary: "M1", parentIds: ["R"] },
    ];
    renderChain({ commits: forkCommits });

    fireEvent.click(row("F1"));
    fireEvent.click(row("M2"), { shiftKey: true });
    fireEvent.contextMenu(row("M2"));

    expect(screen.getByRole("menuitem", { name: "Squash 2 commits" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Interactive rebase…" })).toBeEnabled();
  });
```

In the "keyboard access" describe: in `setup()` replace `onSquashCommits: vi.fn(),` with `onRebaseSelection: vi.fn(),`, and replace the `Shift+ArrowDown extends a squash range…` test with:

```tsx
  it("Shift+ArrowDown extends the selection and the menu then offers Squash", () => {
    const { rerender, list, onSelectRow, onRebaseSelection, ...rest } = setup({ commitId: "C" });
    fireEvent.click(screen.getByText(/^C /).closest("li")!);
    fireEvent.keyDown(list, { key: "ArrowDown", shiftKey: true });
    expect(onSelectRow).toHaveBeenLastCalledWith({ commitId: "B" });
    rerender(
      <CommitGraph
        status={status}
        commits={chain}
        selectedRow={{ commitId: "B" }}
        pending={false}
        onSelectRow={onSelectRow}
        onBranchFromCommit={rest.onBranchFromCommit}
        onRebaseFromCommit={rest.onRebaseFromCommit}
        onRebaseSelection={onRebaseSelection}
      />,
    );
    fireEvent.keyDown(list, { key: "Enter" });
    fireEvent.click(screen.getByRole("menuitem", { name: "Squash 2 commits" }));
    expect(onRebaseSelection).toHaveBeenCalledWith("A", new Map([["C", "Squash"]]));
  });
```

(In that describe `chain` is `C -> B -> A` with `A` a root (`parentIds: []`). Squash on `C`,`B` has oldest `B` with parent `A`, so it is allowed; `A` itself is never selected there.)

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && pnpm test -- --run src/components/CommitGraph.test.tsx`
Expected: FAIL (no `onRebaseSelection`, no Ctrl handling, no toolbar).

- [ ] **Step 3: Edit `CommitGraph.tsx`**

Imports: drop `isSquashableRange` from the `commitGraphLayout` import (keep `assignLanes`), and add:

```tsx
import {
  EMPTY_SELECTION,
  extendSelection,
  selectOnly,
  toggleCommit,
  type CommitSelection,
} from "../lib/commitSelection";
import { planRebaseSelection, presetForAction, type PresetAction } from "../lib/rebaseSelection";
```

Props: replace `onSquashCommits` (destructured name and the type + its comment) with:

```tsx
  onRebaseSelection,
```
```tsx
  // Called when the user acts on a multi-commit selection (Interactive rebase…, Squash, Fixup,
  // Drop). `onto` is the oldest selected commit's own parent — the base the planner rebases onto;
  // `preset` maps commit ids to the action to pre-mark in the planner (empty for a plain
  // Interactive rebase…).
  onRebaseSelection?: (onto: string, preset: ReadonlyMap<string, PresetAction>) => void;
```

State: replace the `squashAnchorIndex` / `squashRange` `useState`s with:

```tsx
  const [selection, setSelection] = useState<CommitSelection>(EMPTY_SELECTION);
```

Right after `selectedIndex` is computed, add the derived effective selection:

```tsx
  // The multi-selection only counts while the focused commit belongs to it. Anything that moves
  // the focus elsewhere (a click outside the graph, search, the Uncommitted row) therefore
  // collapses it without an effect to keep two sources of truth in sync.
  const primaryId = typeof selectedRow === "object" ? selectedRow.commitId : null;
  const effectiveSelection: CommitSelection =
    primaryId === null
      ? EMPTY_SELECTION
      : selection.ids.has(primaryId)
        ? selection
        : selectOnly(primaryId);
```

Scroll effect and menu rect: replace the `querySelector('[aria-selected="true"]')` lookups (a multi-selection has several) with the focused row by index:

```tsx
  useEffect(() => {
    const selected = listRef.current?.children[selectedIndex] as HTMLElement | undefined;
    if (typeof selected?.scrollIntoView === "function") selected.scrollIntoView({ block: "nearest" });
  }, [selectedIndex]);
```
and in `openMenuForSelected`:
```tsx
    const row = list.children[selectedIndex] as HTMLElement | undefined;
```

`moveSelection`: replace the body's `if (extendRange && next >= 1) {…} else {…}` block with:

```tsx
    if (next < 1) {
      setSelection(EMPTY_SELECTION);
    } else if (extendRange) {
      // Row 0 is "Uncommitted Changes"; commit rows are offset by one.
      setSelection(extendSelection(effectiveSelection, commits, commits[next - 1].id));
    } else {
      setSelection(selectOnly(commits[next - 1].id));
    }
```

`handleKeyDown`: add a branch before the `Enter` branch:

```tsx
    } else if (event.key === "Escape") {
      if (effectiveSelection.ids.size > 1 && primaryId !== null) {
        event.preventDefault();
        setSelection(selectOnly(primaryId));
      }
```
and append `Escape` to the `aria-keyshortcuts` string.

`handleCommitClick`: replace the whole function (and drop its `index` argument; update the call site) with:

```tsx
  const handleCommitClick = (event: MouseEvent | undefined, commitId: string) => {
    if (event?.ctrlKey || event?.metaKey) {
      const next = toggleCommit(effectiveSelection, commitId);
      if (next.ids.size === 0) return; // the only selected commit can't be toggled off
      setSelection(next);
      onSelectRow({ commitId: next.ids.has(commitId) ? commitId : (next.anchorId ?? commitId) });
      return;
    }
    setSelection(
      event?.shiftKey ? extendSelection(effectiveSelection, commits, commitId) : selectOnly(commitId),
    );
    onSelectRow({ commitId });
  };
```

Uncommitted row `onClick`: replace the two `setSquash…(null)` calls with `setSelection(EMPTY_SELECTION);`.

Remove `activeSquashRange`, `contextMenuIndex`, `squashMenuActive`. Add (after `handleCommitClick`):

```tsx
  const multiSelected = effectiveSelection.ids.size >= 2;
  const rebasePlan = multiSelected
    ? planRebaseSelection(commits, effectiveSelection.ids)
    : null;

  // The same four actions back the context menu and the toolbar under the list.
  const multiSelectItems: ContextMenuItem[] = (() => {
    if (rebasePlan === null) return [];
    const count = effectiveSelection.ids.size;
    if (!rebasePlan.ok) {
      const blocked = (label: string): ContextMenuItem => ({
        label,
        onSelect: () => {},
        disabled: true,
        title: rebasePlan.reason,
      });
      return [
        blocked("Interactive rebase…"),
        blocked(`Squash ${count} commits`),
        blocked(`Fixup ${count} commits`),
        blocked(`Drop ${count} commits`),
      ];
    }
    const { selection: plan } = rebasePlan;
    const needsRun = "Squash and Fixup need commits that are next to each other.";
    const act = (action: PresetAction) => () =>
      onRebaseSelection?.(plan.onto, presetForAction(plan, action));
    return [
      {
        label: "Interactive rebase…",
        onSelect: () => onRebaseSelection?.(plan.onto, new Map()),
        disabled: pending,
      },
      {
        label: `Squash ${count} commits`,
        onSelect: act("Squash"),
        disabled: pending || !plan.contiguous,
        title: plan.contiguous ? undefined : needsRun,
      },
      {
        label: `Fixup ${count} commits`,
        onSelect: act("Fixup"),
        disabled: pending || !plan.contiguous,
        title: plan.contiguous ? undefined : needsRun,
      },
      {
        label: `Drop ${count} commits`,
        onSelect: act("Drop"),
        disabled: pending,
        destructive: true,
      },
    ];
  })();
```

Row JSX: change the commit `ListRow` props to

```tsx
          selected={effectiveSelection.ids.has(commit.id)}
          onClick={(event) => handleCommitClick(event, commit.id)}
```
(`index` is still used by the `commitLayouts[index]` mouse handlers; keep the `.map((commit, index)` signature.)

`<ul>`: add `aria-multiselectable="true"`.

Context menu `items`: replace the ternary with

```tsx
          items={
            multiSelected && effectiveSelection.ids.has(contextMenu.commitId)
              ? multiSelectItems
              : ([
                  {
                    label: "Branch from here",
                    onSelect: () => onBranchFromCommit(contextMenu.commitId),
                  },
                  {
                    label: "Rebase onto here",
                    onSelect: () => onRebaseFromCommit(contextMenu.commitId),
                    disabled: pending,
                  },
                ] satisfies ContextMenuItem[])
          }
```

Toolbar: after the closing `</ul>` and before the `hasMore` block add

```tsx
    {multiSelected && (
      <div className={styles.selectionBar} role="toolbar" aria-label="Selected commits">
        <span>{effectiveSelection.ids.size} commits selected</span>
        {multiSelectItems.map((item) => (
          <button
            key={item.label}
            type="button"
            onClick={item.onSelect}
            disabled={item.disabled}
            title={item.title}
          >
            {item.label}
          </button>
        ))}
      </div>
    )}
```

- [ ] **Step 4: CSS**

Append to `CommitGraph.module.css`, mirroring `.loadMore`:

```css
.selectionBar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-3);
  font-size: var(--text-sm);
  color: var(--color-text-muted);
}
```

- [ ] **Step 5: Update `App.tsx` line 449**

`onSquashCommits={appState.openSquashPlanner}` → `onRebaseSelection={appState.openRebaseSelection}`.

- [ ] **Step 6: Run the full frontend suite, lint, build**

Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build`
Expected: all PASS, no lint errors, build succeeds.

If a pre-existing `CommitGraph` test other than the ones above fails because it relied on `onSquashCommits` or `aria-selected` lookups, fix the test to the new prop/behavior (do not restore the old prop).

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/CommitGraph.tsx frontend/src/components/CommitGraph.module.css \
  frontend/src/components/CommitGraph.test.tsx frontend/src/App.tsx
git commit -m "feat(frontend): multi-select commits in the graph for interactive rebase"
```

---

### Task 5: E2E flow (Tauri)

**Files:**
- Modify: `e2e/specs/rebase.spec.ts` (append one `it` at the end of the `describe`, using the existing `writeConflictCommit` helper defined mid-file)

**Interfaces:**
- Consumes: UI labels from Task 4 (`Drop N commits` toolbar button), planner `Action` selects, `Start rebase`.

- [ ] **Step 1: Add the test**

Insert before the final `});` of the `describe`:

```ts
  it("ctrl-selects two non-adjacent commits, drops them through the planner, and keeps the rest", async () => {
    writeConflictCommit("multi-1.txt", "1\n", "e2e: multi 1 (drop)");
    writeConflictCommit("multi-2.txt", "2\n", "e2e: multi 2 (keep)");
    writeConflictCommit("multi-3.txt", "3\n", "e2e: multi 3 (drop)");
    writeConflictCommit("multi-4.txt", "4\n", "e2e: multi 4 (keep)");

    await browser.refresh();

    const first = await $("li*=e2e: multi 1 (drop)");
    await first.waitForExist({ timeout: 10000 });
    await first.click();

    // A synthetic `click` carrying `ctrlKey` instead of a real modifier key press: a held
    // modifier driven through WebKitGTK/tauri-driver leaves Shift/Ctrl state stuck for the rest
    // of the session (see the context-menu workaround in the first test). `CommitGraph` only
    // reads `ctrlKey` off the click event.
    const third = await $("li*=e2e: multi 3 (drop)");
    await browser.execute((el) => {
      el.dispatchEvent(new MouseEvent("click", { bubbles: true, cancelable: true, ctrlKey: true }));
    }, third);

    const dropButton = await $("button=Drop 2 commits");
    await dropButton.waitForExist({ timeout: 10000 });
    await dropButton.click();

    // The planner lists the whole span; only the two selected commits are pre-marked Drop.
    const dropRow1 = await $("//li[contains(., 'multi 1 (drop)')]//select[@aria-label='Action']");
    await dropRow1.waitForExist({ timeout: 10000 });
    await expect(dropRow1).toHaveValue("Drop");
    const keepRow = await $("//li[contains(., 'multi 2 (keep)')]//select[@aria-label='Action']");
    await expect(keepRow).toHaveValue("Pick");
    const dropRow3 = await $("//li[contains(., 'multi 3 (drop)')]//select[@aria-label='Action']");
    await expect(dropRow3).toHaveValue("Drop");

    await $("button=Start rebase").click();

    await browser.waitUntil(
      async () => !(await $("li*=e2e: multi 1 (drop)").isExisting()),
      { timeout: 10000 },
    );
    await expect($("li*=e2e: multi 3 (drop)")).not.toBeExisting();
    await expect($("li*=e2e: multi 2 (keep)")).toBeExisting();
    await expect($("li*=e2e: multi 4 (keep)")).toBeExisting();
  });
```

- [ ] **Step 2: Run the Tauri e2e suite**

Follow the "E2E: Tauri app" block in `CLAUDE.md` exactly (build order matters), then `cd e2e && pnpm test`.
Expected: all specs PASS, including the new one.

If the sandbox has no display, use `xvfb-run` as the repo docs say. If e2e can't run here, say so in the hand-off and do not claim it passed.

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/rebase.spec.ts
git commit -m "test(e2e): multi-select drop through the rebase planner"
```

---

### Task 6: Docs and changelog

**Files:**
- Modify: `docs/USER_GUIDE.md` (the `## Rebase` section, ~L122-128)
- Modify: `CHANGELOG.md` (under `## [Unreleased]`, new `###` section at the top)

- [ ] **Step 1: User guide**

Append to the `## Rebase` section:

```markdown
**Selecting several commits.** Ctrl/Cmd+click commits in the graph to build a selection, or
Shift+click / Shift+Arrow to select a range. With two or more selected, a toolbar under the graph
(and the right-click menu) offers **Interactive rebase…**, **Squash**, **Fixup** and **Drop**.
All of them open the rebase planner, based on the parent of the oldest selected commit and showing
every commit from there to the current HEAD; the selected ones are highlighted by being pre-marked,
the rest stay **Pick**. Squash and Fixup are available only for commits that sit next to each
other (they fold into the commit before them); Drop and Interactive rebase… work for any selection.
Selections that include a merge commit or a root commit, or commits that aren't on the current
branch, can't be rebased. Esc collapses a selection back to one commit.
```

- [ ] **Step 2: Changelog**

Add as the first section under `## [Unreleased]`:

```markdown
### Multi-select commits for interactive rebase

- Ctrl/Cmd+click commits in the graph to select several, including non-adjacent ones (Shift
  selects a range). A toolbar and the context menu offer Interactive rebase…, Squash, Fixup and
  Drop, each opening the rebase planner with the selection pre-marked. Squash and Fixup require
  adjacent commits; Drop and Interactive rebase… accept any selection.
```

- [ ] **Step 3: Verify the changelog hook passes**

Run: `python3 scripts/check-changelog.py` (or `SKIP_CHANGELOG_CHECK=` unset `git push --dry-run` equivalent per the script's usage).
Expected: no "missing CHANGELOG" error.

- [ ] **Step 4: Commit**

```bash
git add docs/USER_GUIDE.md CHANGELOG.md
git commit -m "docs: document multi-select interactive rebase"
```

---

## Self-Review

- **Spec coverage:** selection model (Task 1), Ctrl/Cmd/Shift/Esc + highlight + `aria-multiselectable` (Task 4), base = parent of oldest selected (Task 2), full-span planner with preset + unselected stay Pick (Tasks 3-4), quick actions always via planner (Task 4), merge/root/unreachable guards (Tasks 2 + 3), oldest-commit Squash/Fixup guard (Task 2 `presetForAction`), tests incl. one e2e (Tasks 1-5), USER_GUIDE + CHANGELOG (Task 6).
- **Deviations from the spec, found while reading the code (spec updated to match):**
  1. Squash/Fixup are disabled for non-adjacent selections. A plan entry folds into the entry before it, so presetting Squash on C in `A, B(pick), C(squash)` would fold C into B, not A. Drop and Interactive rebase… accept any selection.
  2. `Ctrl+Space` removed. There is no separate focus cursor in the list, so keyboard multi-select is Shift+Arrow ranges only. Non-adjacent selection is mouse-only for now.
- **Placeholder scan:** none.
- **Type consistency:** `PresetAction` (Task 2) → `rebasePreset`/`openRebaseSelection` (Task 3) → `presetActions`/`onRebaseSelection` (Tasks 3-4). `RebaseSelection.idsOldestFirst`, `.onto`, `.contiguous` used identically in Tasks 2 and 4.
- **Known risk:** Task 3 and Task 4 must land together for a green `pnpm build` (renamed action consumed by `CommitGraph`); the plan says so at the Task 3 commit step.
