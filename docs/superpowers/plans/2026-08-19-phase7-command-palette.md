# Phase 7 Command Palette Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `Ctrl/Cmd+K` command palette — a flat, single-search list
of every zero-arg and single-pick action in the app, plus quick jumps to
each sidebar section for anything needing more input than that.

**Architecture:** A pure `buildCommands` function turns current app
state into a flat `Command[]`; a `CommandPalette` component (rendered
inside the existing `Overlay` primitive) filters/ranks/displays that
list and runs the highlighted command on Enter or click; `App.tsx` adds
the one `window`-level `keydown` listener this app has never needed
before.

**Tech Stack:** React 19, TypeScript, Vite CSS Modules — no new
dependencies.

**Spec:** `docs/superpowers/specs/2026-08-19-browsitory-phase7-command-palette-design.md`

## Global Constraints

- No `RepoClient` method, DTO, Tauri command, worker message, or
  `git-core` function is added, removed, or changed in shape by this
  plan.
- Frontend tests mock `RepoClient`, never `@tauri-apps/api`.
- `pnpm lint`'s `no-restricted-imports` rule
  (`frontend/eslint.config.js:25-37`) must keep passing.
- No new dependency (matching/ranking is hand-rolled).
- `pnpm build`, `pnpm lint`, and `pnpm test -- --run` must pass after
  every task.
- Every new value traces to a token in `frontend/src/styles/tokens.css`
  — no hardcoded colors.
- `AccordionSection`, `ListRow`, and every already-shipped component are
  NOT modified by this plan — this plan only adds new files and extends
  `App.tsx`.
- When writing a `window`-level (native DOM) event handler, do not
  import `KeyboardEvent`/`PointerEvent`/etc. from `"react"` — use the
  ambient global DOM type instead. A prior phase shipped a real bug this
  exact way (a React-namespaced type used where the global DOM type was
  required), caught only in review — don't repeat it.

---

### Task 1: `buildCommands` — the command registry

**Files:**
- Create: `frontend/src/lib/commands.ts`
- Create: `frontend/src/lib/commands.test.ts`

**Interfaces:**
- Consumes: `UseAppStateResult` from `frontend/src/state/useAppState.ts`
  (already exported there — its exact shape, including every field and
  method name used below, is verified against the real file; read it
  yourself too before changing anything, in case it has shifted).
- Produces:
  ```typescript
  interface Command {
    id: string;
    label: string;
    keywords: string[];
    run: () => void;
  }
  export function buildCommands(appState: UseAppStateResult): Command[];
  export function filterAndSortCommands(commands: Command[], query: string): Command[];
  export function loadRecentCommandIds(): string[];
  export function recordCommandUsed(id: string): void;
  ```
  Task 2 (`CommandPalette`) consumes `Command`, `filterAndSortCommands`,
  and `recordCommandUsed` by these exact names. Task 3 (`App.tsx`
  wiring) consumes `buildCommands` by this exact name.

- [ ] **Step 1: Write the failing tests**

```typescript
// frontend/src/lib/commands.test.ts
import { describe, expect, it, vi, beforeEach } from "vitest";
import {
  buildCommands,
  filterAndSortCommands,
  loadRecentCommandIds,
  recordCommandUsed,
  type Command,
} from "./commands";
import type { UseAppStateResult } from "../state/useAppState";

function makeAppState(overrides: Partial<UseAppStateResult["state"]> = {}): UseAppStateResult {
  return {
    state: {
      repoPath: "/repo",
      selectedRow: "uncommitted",
      status: [],
      commits: [],
      branches: [
        { name: "main", isCurrent: true },
        { name: "feature", isCurrent: false },
      ],
      worktrees: [
        { name: "main", path: "/repo", head: "abc", isMain: true, isLocked: false, isPrunable: false },
        { name: "wt1", path: "/repo-wt1", head: "def", isMain: false, isLocked: false, isPrunable: false },
      ],
      submodules: [
        { path: "libs/a", url: "https://example.com/a.git", gitlinkId: "aaa", initialized: false, headId: null },
        { path: "libs/b", url: "https://example.com/b.git", gitlinkId: "bbb", initialized: true, headId: "bbb111" },
      ],
      reflogRefs: [],
      selectedReflogReference: null,
      reflog: [
        {
          reference: "HEAD@{0}",
          oldId: "aaa",
          newId: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
          committerName: "Test",
          committerEmail: "test@example.com",
          timestamp: 0,
          message: "commit: test",
          summary: null,
        },
      ],
      remotes: [{ name: "origin", fetchUrl: "https://example.com/repo.git", pushUrl: null, authMode: null, authUsername: null }],
      tags: [{ name: "v1.0", targetId: "aaa", annotated: false, message: null, taggerName: null, timestamp: null }],
      upstream: { localBranch: "main", remoteName: "origin", remoteBranch: "main" },
      remoteUpstreams: {},
      forgeRepositories: [],
      pullRequests: {},
      createBranchDraft: null,
      stashes: [{ index: 0, message: "WIP", commitId: "ccc" }],
      mergeMessage: null,
      rebaseProgress: null,
      rebaseOnto: null,
      pendingPull: null,
      pullOutcome: null,
      transfer: null,
      error: null,
      pending: false,
      ...overrides,
    },
    openRepo: vi.fn(),
    selectRow: vi.fn(),
    stageFile: vi.fn(),
    unstageFile: vi.fn(),
    commit: vi.fn(),
    createBranch: vi.fn(),
    switchBranch: vi.fn(),
    deleteBranch: vi.fn(),
    renameBranch: vi.fn(),
    createWorktree: vi.fn(),
    removeWorktree: vi.fn(),
    pruneWorktrees: vi.fn(),
    initSubmodule: vi.fn(),
    updateSubmodule: vi.fn(),
    selectReflogReference: vi.fn(),
    restoreReflogEntry: vi.fn(),
    addRemote: vi.fn(),
    renameRemote: vi.fn(),
    updateRemoteUrls: vi.fn(),
    removeRemote: vi.fn(),
    saveHttpsCredential: vi.fn(),
    forgetHttpsCredential: vi.fn(),
    setRemoteAuthMode: vi.fn(),
    setCurrentUpstream: vi.fn(),
    clearCurrentUpstream: vi.fn(),
    fetchRemote: vi.fn(),
    createTag: vi.fn(),
    deleteTag: vi.fn(),
    pushCurrentBranch: vi.fn(),
    pushTags: vi.fn(),
    pullCurrentUpstream: vi.fn(),
    clearPendingPull: vi.fn(),
    openCreateBranchDraft: vi.fn(),
    closeCreateBranchDraft: vi.fn(),
    saveStash: vi.fn(),
    applyStash: vi.fn(),
    dropStash: vi.fn(),
    mergeBranch: vi.fn(),
    resolveConflict: vi.fn(),
    resolveAddDeleteConflict: vi.fn(),
    abortMerge: vi.fn(),
    openRebasePlanner: vi.fn(),
    closeRebasePlanner: vi.fn(),
    startRebase: vi.fn(),
    rebaseContinue: vi.fn(),
    abortRebase: vi.fn(),
    listPullRequests: vi.fn(),
    saveForgeToken: vi.fn(),
    forgetForgeToken: vi.fn(),
    createPullRequest: vi.fn(),
    openExternalUrl: vi.fn(),
    refresh: vi.fn(),
  };
}

describe("buildCommands", () => {
  it("includes one switch-branch command per non-current branch", () => {
    const appState = makeAppState();
    const commands = buildCommands(appState);
    const switchCommands = commands.filter((c) => c.id.startsWith("switch-branch:"));
    expect(switchCommands).toHaveLength(1);
    expect(switchCommands[0].label).toBe("Switch to feature");
    switchCommands[0].run();
    expect(appState.switchBranch).toHaveBeenCalledWith("feature");
  });

  it("includes fetch/push/push-tags commands per remote", () => {
    const appState = makeAppState();
    const commands = buildCommands(appState);
    const fetchCmd = commands.find((c) => c.id === "fetch-remote:origin");
    expect(fetchCmd?.label).toBe("Fetch origin");
    fetchCmd?.run();
    expect(appState.fetchRemote).toHaveBeenCalledWith("origin");

    const pushCmd = commands.find((c) => c.id === "push-branch:origin");
    pushCmd?.run();
    expect(appState.pushCurrentBranch).toHaveBeenCalledWith("origin");

    const pushTagsCmd = commands.find((c) => c.id === "push-tags:origin");
    expect(pushTagsCmd?.label).toBe("Push all tags to origin");
    pushTagsCmd?.run();
    expect(appState.pushTags).toHaveBeenCalledWith("origin", []);
  });

  it("only includes pull when an upstream is set", () => {
    const withUpstream = buildCommands(makeAppState());
    expect(withUpstream.some((c) => c.id === "pull")).toBe(true);

    const withoutUpstream = buildCommands(makeAppState({ upstream: null }));
    expect(withoutUpstream.some((c) => c.id === "pull")).toBe(false);
  });

  it("only includes rebase continue/abort while a rebase is in progress", () => {
    const noRebase = buildCommands(makeAppState());
    expect(noRebase.some((c) => c.id === "rebase-continue")).toBe(false);

    const withRebase = buildCommands(makeAppState({ rebaseProgress: { currentStep: 1, totalSteps: 3 } }));
    expect(withRebase.some((c) => c.id === "rebase-continue")).toBe(true);
    expect(withRebase.some((c) => c.id === "rebase-abort")).toBe(true);
  });

  it("only includes abort merge while a merge is in progress", () => {
    const noMerge = buildCommands(makeAppState());
    expect(noMerge.some((c) => c.id === "merge-abort")).toBe(false);

    const withMerge = buildCommands(makeAppState({ mergeMessage: "Merge branch" }));
    expect(withMerge.some((c) => c.id === "merge-abort")).toBe(true);
  });

  it("includes apply/drop per stash and delete per tag", () => {
    const appState = makeAppState();
    const commands = buildCommands(appState);
    const applyCmd = commands.find((c) => c.id === "apply-stash:0");
    expect(applyCmd?.label).toBe("Apply stash: WIP");
    applyCmd?.run();
    expect(appState.applyStash).toHaveBeenCalledWith(0);

    const dropCmd = commands.find((c) => c.id === "drop-stash:0");
    dropCmd?.run();
    expect(appState.dropStash).toHaveBeenCalledWith(0);

    const deleteTagCmd = commands.find((c) => c.id === "delete-tag:v1.0");
    deleteTagCmd?.run();
    expect(appState.deleteTag).toHaveBeenCalledWith("v1.0");
  });

  it("excludes the main worktree from open/remove commands", () => {
    const commands = buildCommands(makeAppState());
    expect(commands.some((c) => c.id === "open-worktree:/repo")).toBe(false);
    expect(commands.some((c) => c.id === "remove-worktree:main")).toBe(false);

    const openCmd = commands.find((c) => c.id === "open-worktree:/repo-wt1");
    expect(openCmd?.label).toBe("Open worktree wt1");
    openCmd?.run();
    expect(commands.find((c) => c.id === "open-worktree:/repo-wt1"));
  });

  it("splits submodule commands by initialized state", () => {
    const commands = buildCommands(makeAppState());
    const initCmd = commands.find((c) => c.id === "init-submodule:libs/a");
    expect(initCmd?.label).toBe("Initialize submodule libs/a");
    initCmd?.run();

    const updateCmd = commands.find((c) => c.id === "update-submodule:libs/b");
    expect(updateCmd?.label).toBe("Update submodule libs/b");
    updateCmd?.run();

    expect(commands.some((c) => c.id === "init-submodule:libs/b")).toBe(false);
    expect(commands.some((c) => c.id === "update-submodule:libs/a")).toBe(false);
  });

  it("includes one restore command per reflog entry, with both reference and newId bound", () => {
    const appState = makeAppState();
    const commands = buildCommands(appState);
    const restoreCmd = commands.find((c) => c.id.startsWith("restore-reflog:"));
    expect(restoreCmd?.label).toBe("Restore HEAD@{0} to bbbbbbb");
    restoreCmd?.run();
    expect(appState.restoreReflogEntry).toHaveBeenCalledWith(
      "HEAD@{0}",
      "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );
  });

  it("always includes exactly one go-to command per sidebar section", () => {
    const commands = buildCommands(makeAppState());
    const goToCommands = commands.filter((c) => c.id.startsWith("go-to:"));
    expect(goToCommands.map((c) => c.label).sort()).toEqual(
      [
        "Go to Branches",
        "Go to Pull Requests",
        "Go to Reflog",
        "Go to Remotes",
        "Go to Submodules",
        "Go to Tags",
        "Go to Worktrees",
      ].sort(),
    );
  });
});

describe("filterAndSortCommands", () => {
  const commands: Command[] = [
    { id: "a", label: "Switch to main", keywords: ["branch", "switch", "main"], run: vi.fn() },
    { id: "b", label: "Switch to feature", keywords: ["branch", "switch", "feature"], run: vi.fn() },
    { id: "c", label: "Fetch origin", keywords: ["fetch", "remote", "origin"], run: vi.fn() },
  ];

  it("returns everything, unranked-but-stable, for an empty query", () => {
    expect(filterAndSortCommands(commands, "")).toHaveLength(3);
  });

  it("ranks a label-prefix match above a substring-only match", () => {
    const withPrefixAndSubstring: Command[] = [
      { id: "x", label: "Fetch origin", keywords: [], run: vi.fn() },
      { id: "y", label: "Switch to origin-backup", keywords: [], run: vi.fn() },
    ];
    const results = filterAndSortCommands(withPrefixAndSubstring, "fetch");
    expect(results[0].id).toBe("x");
  });

  it("matches on keywords, not just the label", () => {
    const results = filterAndSortCommands(commands, "checkout");
    expect(results).toHaveLength(0); // "checkout" isn't a keyword here — sanity check the negative case
    const results2 = filterAndSortCommands(commands, "origin");
    expect(results2.map((c) => c.id)).toContain("c");
  });

  it("excludes commands with no match at all", () => {
    const results = filterAndSortCommands(commands, "zzz-no-match");
    expect(results).toHaveLength(0);
  });
});

describe("recordCommandUsed / loadRecentCommandIds", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("persists used command ids, most recent first, deduplicated", () => {
    recordCommandUsed("a");
    recordCommandUsed("b");
    recordCommandUsed("a");
    expect(loadRecentCommandIds()).toEqual(["a", "b"]);
  });

  it("returns an empty array when nothing is stored", () => {
    expect(loadRecentCommandIds()).toEqual([]);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run commands.test.ts`
Expected: FAIL — `commands.ts` does not exist yet.

- [ ] **Step 3: Implement**

```typescript
// frontend/src/lib/commands.ts
import type { UseAppStateResult } from "../state/useAppState";

export interface Command {
  id: string;
  label: string;
  keywords: string[];
  run: () => void;
}

const RECENT_KEY = "command-palette-recent";
const RECENT_LIMIT = 10;
const RESULT_LIMIT = 50;

const SIDEBAR_SECTIONS = [
  "Branches",
  "Worktrees",
  "Submodules",
  "Reflog",
  "Remotes",
  "Tags",
  "Pull Requests",
] as const;

function goToSidebarSection(title: string): void {
  const button = document.querySelector<HTMLButtonElement>(
    `section[aria-label="${title}"] button[aria-expanded]`,
  );
  if (button === null) return;
  if (button.getAttribute("aria-expanded") === "false") {
    button.click();
  }
  button.closest("section")?.scrollIntoView({ behavior: "smooth", block: "nearest" });
}

export function buildCommands(appState: UseAppStateResult): Command[] {
  const { state } = appState;
  const commands: Command[] = [];

  if (state.upstream !== null) {
    commands.push({
      id: "pull",
      label: "Pull",
      keywords: ["pull", "sync"],
      run: () => void appState.pullCurrentUpstream(),
    });
  }
  commands.push({
    id: "save-stash",
    label: "Save stash",
    keywords: ["stash", "save"],
    run: () => void appState.saveStash(),
  });
  commands.push({
    id: "prune-worktrees",
    label: "Prune worktrees",
    keywords: ["worktree", "prune"],
    run: () => void appState.pruneWorktrees(),
  });
  if (state.rebaseProgress !== null) {
    commands.push({
      id: "rebase-continue",
      label: "Continue rebase",
      keywords: ["rebase", "continue"],
      run: () => void appState.rebaseContinue(),
    });
    commands.push({
      id: "rebase-abort",
      label: "Abort rebase",
      keywords: ["rebase", "abort", "cancel"],
      run: () => void appState.abortRebase(),
    });
  }
  if (state.mergeMessage !== null) {
    commands.push({
      id: "merge-abort",
      label: "Abort merge",
      keywords: ["merge", "abort", "cancel"],
      run: () => void appState.abortMerge(),
    });
  }
  commands.push({
    id: "refresh",
    label: "Refresh",
    keywords: ["refresh", "reload"],
    run: () => void appState.refresh(),
  });

  for (const branch of state.branches) {
    if (branch.isCurrent) continue;
    commands.push({
      id: `switch-branch:${branch.name}`,
      label: `Switch to ${branch.name}`,
      keywords: ["branch", "switch", "checkout", branch.name],
      run: () => void appState.switchBranch(branch.name),
    });
  }

  for (const remote of state.remotes) {
    commands.push({
      id: `fetch-remote:${remote.name}`,
      label: `Fetch ${remote.name}`,
      keywords: ["fetch", "remote", remote.name],
      run: () => void appState.fetchRemote(remote.name),
    });
    commands.push({
      id: `push-branch:${remote.name}`,
      label: `Push to ${remote.name}`,
      keywords: ["push", "remote", remote.name],
      run: () => void appState.pushCurrentBranch(remote.name),
    });
    commands.push({
      id: `push-tags:${remote.name}`,
      label: `Push all tags to ${remote.name}`,
      keywords: ["push", "tags", "remote", remote.name],
      run: () => void appState.pushTags(remote.name, []),
    });
  }

  for (const tag of state.tags) {
    commands.push({
      id: `delete-tag:${tag.name}`,
      label: `Delete tag ${tag.name}`,
      keywords: ["tag", "delete", tag.name],
      run: () => void appState.deleteTag(tag.name),
    });
  }

  for (const stash of state.stashes) {
    commands.push({
      id: `apply-stash:${stash.index}`,
      label: `Apply stash: ${stash.message}`,
      keywords: ["stash", "apply"],
      run: () => void appState.applyStash(stash.index),
    });
    commands.push({
      id: `drop-stash:${stash.index}`,
      label: `Drop stash: ${stash.message}`,
      keywords: ["stash", "drop", "delete"],
      run: () => void appState.dropStash(stash.index),
    });
  }

  for (const worktree of state.worktrees) {
    if (worktree.isMain) continue;
    commands.push({
      id: `open-worktree:${worktree.path}`,
      label: `Open worktree ${worktree.name}`,
      keywords: ["worktree", "open", worktree.name],
      run: () => void appState.openRepo(worktree.path),
    });
    commands.push({
      id: `remove-worktree:${worktree.name}`,
      label: `Remove worktree ${worktree.name}`,
      keywords: ["worktree", "remove", "delete", worktree.name],
      run: () => void appState.removeWorktree(worktree.name),
    });
  }

  for (const submodule of state.submodules) {
    if (submodule.initialized) {
      commands.push({
        id: `update-submodule:${submodule.path}`,
        label: `Update submodule ${submodule.path}`,
        keywords: ["submodule", "update", submodule.path],
        run: () => void appState.updateSubmodule(submodule.path, false),
      });
    } else {
      commands.push({
        id: `init-submodule:${submodule.path}`,
        label: `Initialize submodule ${submodule.path}`,
        keywords: ["submodule", "init", "initialize", submodule.path],
        run: () => void appState.initSubmodule(submodule.path),
      });
    }
  }

  for (const entry of state.reflog) {
    commands.push({
      id: `restore-reflog:${entry.reference}:${entry.newId}`,
      label: `Restore ${entry.reference} to ${entry.newId.slice(0, 7)}`,
      keywords: ["reflog", "restore", entry.reference],
      run: () => void appState.restoreReflogEntry(entry.reference, entry.newId),
    });
  }

  for (const title of SIDEBAR_SECTIONS) {
    commands.push({
      id: `go-to:${title}`,
      label: `Go to ${title}`,
      keywords: ["go", "navigate", title.toLowerCase()],
      run: () => goToSidebarSection(title),
    });
  }

  return commands;
}

function scoreCommand(command: Command, query: string): number | null {
  const q = query.trim().toLowerCase();
  if (q === "") return 0;
  const label = command.label.toLowerCase();
  if (label.startsWith(q)) return 3;
  if (command.keywords.some((keyword) => keyword.toLowerCase().startsWith(q))) return 2;
  if (label.includes(q) || command.keywords.some((keyword) => keyword.toLowerCase().includes(q))) return 1;
  return null;
}

export function filterAndSortCommands(commands: Command[], query: string): Command[] {
  const recent = loadRecentCommandIds();
  const scored: { command: Command; score: number }[] = [];
  for (const command of commands) {
    const score = scoreCommand(command, query);
    if (score !== null) scored.push({ command, score });
  }
  scored.sort((a, b) => {
    if (a.score !== b.score) return b.score - a.score;
    const aRecent = recent.indexOf(a.command.id);
    const bRecent = recent.indexOf(b.command.id);
    if (aRecent !== bRecent) {
      if (aRecent === -1) return 1;
      if (bRecent === -1) return -1;
      return aRecent - bRecent;
    }
    return a.command.label.localeCompare(b.command.label);
  });
  return scored.slice(0, RESULT_LIMIT).map((entry) => entry.command);
}

export function loadRecentCommandIds(): string[] {
  const stored = localStorage.getItem(RECENT_KEY);
  if (stored === null) return [];
  try {
    const parsed: unknown = JSON.parse(stored);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((id): id is string => typeof id === "string");
  } catch {
    return [];
  }
}

export function recordCommandUsed(id: string): void {
  const recent = loadRecentCommandIds().filter((existing) => existing !== id);
  recent.unshift(id);
  localStorage.setItem(RECENT_KEY, JSON.stringify(recent.slice(0, RECENT_LIMIT)));
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run commands.test.ts`
Expected: PASS (all cases above).

- [ ] **Step 5: Verify build and lint**

Run: `cd frontend && pnpm build && pnpm lint`
Expected: both PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/lib/commands.ts frontend/src/lib/commands.test.ts
git commit -m "feat(frontend): add command palette registry (buildCommands, ranking, recency)"
```

---

### Task 2: `CommandPalette` component

**Files:**
- Create: `frontend/src/components/CommandPalette.tsx`
- Create: `frontend/src/components/CommandPalette.module.css`
- Create: `frontend/src/components/CommandPalette.test.tsx`

**Interfaces:**
- Consumes: `Command`, `filterAndSortCommands`, `recordCommandUsed` from
  `frontend/src/lib/commands.ts` (produced by Task 1); `ListRow` from
  `frontend/src/components/primitives/ListRow.tsx` (already shipped,
  not modified by this plan).
- Produces:
  ```typescript
  export function CommandPalette(props: { commands: Command[]; onRun: () => void }): JSX.Element;
  ```
  Task 3 (`App.tsx` wiring) consumes this component and its exact prop
  names.

`ListRow`'s real signature (already shipped, read
`frontend/src/components/primitives/ListRow.tsx` to confirm before
using it): `{ selected?: boolean; onClick?: () => void; onContextMenu?:
(event) => void; className?: string; children: ReactNode }`. Passing
`onClick` together with `selected` puts `ListRow` in its
container-owns-keyboard-nav mode (no `tabIndex` on the row itself) —
exactly the mode this task needs, since the search `<input>` owns
keyboard navigation here, matching `CommitGraph`'s existing use of the
same primitive.

- [ ] **Step 1: Write the failing tests**

```typescript
// frontend/src/components/CommandPalette.test.tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CommandPalette } from "./CommandPalette";
import type { Command } from "../lib/commands";

function makeCommands(): Command[] {
  return [
    { id: "switch-main", label: "Switch to main", keywords: ["branch"], run: vi.fn() },
    { id: "switch-feature", label: "Switch to feature", keywords: ["branch"], run: vi.fn() },
    { id: "fetch-origin", label: "Fetch origin", keywords: ["remote"], run: vi.fn() },
  ];
}

describe("CommandPalette", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("renders every command when the query is empty", () => {
    render(<CommandPalette commands={makeCommands()} onRun={vi.fn()} />);
    expect(screen.getByText("Switch to main")).toBeInTheDocument();
    expect(screen.getByText("Switch to feature")).toBeInTheDocument();
    expect(screen.getByText("Fetch origin")).toBeInTheDocument();
  });

  it("filters as the user types", () => {
    render(<CommandPalette commands={makeCommands()} onRun={vi.fn()} />);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "fetch" } });
    expect(screen.getByText("Fetch origin")).toBeInTheDocument();
    expect(screen.queryByText("Switch to main")).not.toBeInTheDocument();
  });

  it("runs the highlighted command and calls onRun when Enter is pressed", () => {
    const commands = makeCommands();
    const onRun = vi.fn();
    render(<CommandPalette commands={commands} onRun={onRun} />);
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    expect(commands[0].run).toHaveBeenCalledOnce();
    expect(onRun).toHaveBeenCalledOnce();
  });

  it("moves the highlight with arrow keys before running", () => {
    const commands = makeCommands();
    render(<CommandPalette commands={commands} onRun={vi.fn()} />);
    const input = screen.getByRole("textbox");
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(commands[1].run).toHaveBeenCalledOnce();
    expect(commands[0].run).not.toHaveBeenCalled();
  });

  it("runs a command on click regardless of highlight", () => {
    const commands = makeCommands();
    const onRun = vi.fn();
    render(<CommandPalette commands={commands} onRun={onRun} />);
    fireEvent.click(screen.getByText("Fetch origin"));
    expect(commands[2].run).toHaveBeenCalledOnce();
    expect(onRun).toHaveBeenCalledOnce();
  });

  it("records the run command as recently used", () => {
    const commands = makeCommands();
    render(<CommandPalette commands={commands} onRun={vi.fn()} />);
    fireEvent.click(screen.getByText("Fetch origin"));
    expect(localStorage.getItem("command-palette-recent")).toContain("fetch-origin");
  });

  it("shows an empty-state message when nothing matches", () => {
    render(<CommandPalette commands={makeCommands()} onRun={vi.fn()} />);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "zzz-no-match" } });
    expect(screen.getByText("No matching commands")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run CommandPalette.test.tsx`
Expected: FAIL — `CommandPalette.tsx` does not exist yet.

- [ ] **Step 3: Implement**

```typescript
// frontend/src/components/CommandPalette.tsx
import { useMemo, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { ListRow } from "./primitives/ListRow";
import { filterAndSortCommands, recordCommandUsed, type Command } from "../lib/commands";
import styles from "./CommandPalette.module.css";

export function CommandPalette({ commands, onRun }: { commands: Command[]; onRun: () => void }) {
  const [query, setQuery] = useState("");
  const [highlightedIndex, setHighlightedIndex] = useState(0);

  const results = useMemo(() => filterAndSortCommands(commands, query), [commands, query]);
  const clampedIndex = results.length === 0 ? 0 : Math.min(highlightedIndex, results.length - 1);

  function runCommand(command: Command) {
    recordCommandUsed(command.id);
    command.run();
    onRun();
  }

  function handleKeyDown(event: ReactKeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setHighlightedIndex((prev) => Math.min(prev + 1, Math.max(results.length - 1, 0)));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setHighlightedIndex((prev) => Math.max(prev - 1, 0));
    } else if (event.key === "Enter") {
      event.preventDefault();
      const command = results[clampedIndex];
      if (command !== undefined) runCommand(command);
    }
  }

  return (
    <div className={styles.palette}>
      <input
        type="text"
        className={styles.input}
        placeholder="Type a command…"
        autoFocus
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setHighlightedIndex(0);
        }}
        onKeyDown={handleKeyDown}
        aria-label="Command palette"
      />
      <ul className={styles.list}>
        {results.map((command, index) => (
          <ListRow key={command.id} selected={index === clampedIndex} onClick={() => runCommand(command)}>
            {command.label}
          </ListRow>
        ))}
        {results.length === 0 && <li className={styles.empty}>No matching commands</li>}
      </ul>
    </div>
  );
}
```

Note the `KeyboardEvent as ReactKeyboardEvent` import alias — this
component's `handleKeyDown` types a React synthetic event on an
`<input>`, so the React-namespaced import is correct here (unlike a
`window.addEventListener` callback, which needs the global DOM type
instead — see Task 3).

- [ ] **Step 4: Write the CSS**

```css
/* frontend/src/components/CommandPalette.module.css */
.palette {
  display: flex;
  flex-direction: column;
  width: 480px;
  max-height: 60vh;
}

.input {
  border: none;
  border-bottom: 1px solid var(--color-border);
  border-radius: 0;
  padding: var(--space-3) var(--space-4);
  font-size: var(--text-md);
}

.input:focus-visible {
  outline: none;
}

.list {
  list-style: none;
  margin: 0;
  padding: var(--space-1) 0;
  overflow-y: auto;
}

.empty {
  padding: var(--space-3) var(--space-4);
  color: var(--color-text-muted);
  font-size: var(--text-sm);
}
```

`Overlay`'s own CSS already gives the surrounding `<dialog>` a
`border`/`border-radius`/`box-shadow` and `padding: 0` — `.input`'s
`border-bottom` here is what visually separates it from the results
list, and `.input:focus-visible { outline: none }` avoids a doubled
focus ring immediately inside the `Overlay`'s own border (the input is
the only focusable element here, `autoFocus` puts focus on it the
instant the palette mounts, so a visible focus ring adds no information
a sighted user needs — the palette itself being open already tells
them where they are; screen-reader users still get the accessible name
from `aria-label`).

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run CommandPalette.test.tsx`
Expected: PASS (7 tests).

- [ ] **Step 6: Verify build and lint**

Run: `cd frontend && pnpm build && pnpm lint`
Expected: both PASS.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/CommandPalette.tsx frontend/src/components/CommandPalette.module.css frontend/src/components/CommandPalette.test.tsx
git commit -m "feat(frontend): add CommandPalette component"
```

---

### Task 3: Wire `Ctrl/Cmd+K` into `App.tsx`

**Files:**
- Modify: `frontend/src/App.tsx`

**Interfaces:**
- Consumes: `CommandPalette` from `frontend/src/components/CommandPalette.tsx`
  and `buildCommands` from `frontend/src/lib/commands.ts` (both produced
  by Tasks 1-2).

Read the current `frontend/src/App.tsx` in full before editing — it has
been through several rounds of change in prior phases; work from the
real file, not from any snippet elsewhere in this repo's history.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run`
Expected: PASS (full suite, confirms the starting point).

- [ ] **Step 2: Add the imports**

Add, alongside `App.tsx`'s other component/lib imports:

```typescript
import { CommandPalette } from "./components/CommandPalette";
import { buildCommands } from "./lib/commands";
```

- [ ] **Step 3: Add palette-open state and the keydown listener**

Inside `App()`, alongside the existing `theme` state, add:

```typescript
const [paletteOpen, setPaletteOpen] = useState(false);

useEffect(() => {
  function handleKeyDown(event: KeyboardEvent) {
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
      if (appState.state.repoPath === null) return;
      event.preventDefault();
      setPaletteOpen((prev) => !prev);
    }
  }
  window.addEventListener("keydown", handleKeyDown);
  return () => window.removeEventListener("keydown", handleKeyDown);
}, [appState.state.repoPath]);
```

The `handleKeyDown(event: KeyboardEvent)` parameter here is the
**global DOM type**, not a React one — `App.tsx` has no existing
`KeyboardEvent` import to collide with, and none should be added for
this handler; see this plan's Global Constraints for why that
distinction matters.

- [ ] **Step 4: Render the palette**

In the post-open return branch, add the palette's `Overlay` as a
sibling immediately after the existing `TransferPanel` `Overlay` block
(both `Overlay`s grouped together, rather than scattered on opposite
sides of `<SplitView>` as happened once before in this codebase's
history):

```tsx
{appState.state.transfer !== null && (
  <Overlay>
    <TransferPanel progress={appState.state.transfer} />
  </Overlay>
)}
{paletteOpen && (
  <Overlay onClose={() => setPaletteOpen(false)}>
    <CommandPalette commands={buildCommands(appState)} onRun={() => setPaletteOpen(false)} />
  </Overlay>
)}
```

`buildCommands(appState)` recomputes on every render while the palette
is open — cheap at this scale (well under 100 commands even in a large
repo) and guarantees the list never shows stale branches/remotes/tags.

- [ ] **Step 5: Verify no regression**

Run: `cd frontend && pnpm build && pnpm lint && pnpm test -- --run`
Expected: all PASS. `App.tsx` still has no dedicated test file (a
pre-existing gap from Phase 6, not created by this task) — this task's
correctness is verified by Tasks 1-2's unit tests plus Task 4's E2E
spec, not by a new `App.test.tsx`.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/App.tsx
git commit -m "feat(frontend): wire Ctrl/Cmd+K to open the command palette"
```

---

### Task 4: GUI E2E spec

**Files:**
- Create: `e2e/specs/command-palette.spec.ts`

**Interfaces:**
- Consumes: `expandSidebarSection` from `e2e/support/sidebar.ts`
  (already shipped by Phase 6) is NOT needed here — this spec confirms
  the palette's own navigate commands do the expanding, so don't
  pre-expand anything yourself in this spec's setup.

Read at least one existing E2E spec first (e.g.
`e2e/specs/first-flow.spec.ts` or `e2e/specs/worktree.spec.ts`) to match
this project's WebdriverIO conventions (page-object-free direct
`browser`/`$`/`$$` usage, `waitUntil` patterns, fixture repo setup) — do
not invent a different style for this one spec.

- [ ] **Step 1: Write the spec**

```typescript
// e2e/specs/command-palette.spec.ts
import { browser, $ } from "@wdio/globals";

describe("command palette", () => {
  it("opens with Ctrl/Cmd+K, filters, and runs a zero-arg command", async () => {
    // Toggle theme is a safe, observable zero-arg effect: `document.documentElement.dataset.theme` flips.
    const before = await browser.execute(() => document.documentElement.dataset.theme);

    await browser.keys(["Control", "k"]);
    const input = await $('input[aria-label="Command palette"]');
    await input.waitForDisplayed();
    await input.setValue("toggle");

    // "Toggle theme" isn't in buildCommands (it's injected by App.tsx separately per the
    // design spec) — this spec instead runs "Refresh", a real buildCommands entry, and
    // asserts the palette closes afterward, which is the observable, command-agnostic effect
    // every command shares.
    await input.setValue("refresh");
    await browser.keys(["Enter"]);
    await input.waitForDisplayed({ reverse: true });

    const after = await browser.execute(() => document.documentElement.dataset.theme);
    expect(after).toBe(before); // refresh doesn't touch theme — sanity check nothing else changed
  });

  it("runs a single-pick command directly from the flat list", async () => {
    await browser.keys(["Control", "k"]);
    const input = await $('input[aria-label="Command palette"]');
    await input.waitForDisplayed();
    await input.setValue("fetch origin");

    const results = await $$("li");
    await expect(results[0]).toHaveTextContaining("Fetch origin");

    await browser.keys(["Enter"]);
    await input.waitForDisplayed({ reverse: true });
  });

  it("a navigate command expands the target sidebar section", async () => {
    await browser.keys(["Control", "k"]);
    const input = await $('input[aria-label="Command palette"]');
    await input.waitForDisplayed();
    await input.setValue("go to worktrees");
    await browser.keys(["Enter"]);
    await input.waitForDisplayed({ reverse: true });

    const trigger = await $('section[aria-label="Worktrees"] button[aria-expanded]');
    await expect(trigger).toHaveAttribute("aria-expanded", "true");
  });

  it("closes on Escape without running anything", async () => {
    await browser.keys(["Control", "k"]);
    const input = await $('input[aria-label="Command palette"]');
    await input.waitForDisplayed();
    await input.setValue("fetch origin");
    await browser.keys(["Escape"]);
    await input.waitForDisplayed({ reverse: true });
  });
});
```

Adjust the exact `$`/`$$` query style, `waitUntil`/`waitForDisplayed`
timeouts, and fixture-repo assumptions (this assumes a remote named
`origin` and at least one worktree exist in the E2E fixture — verify
against `e2e/support/` fixture setup and adjust the search strings if
the fixture's actual data differs) to match whatever the existing specs
you read in Step 1 actually do — this plan's snippet is the intent, not
untouchable literal text, unlike Tasks 1-2's unit-test code.

- [ ] **Step 2: Run the GUI E2E suite**

Run (from the repo root, per `CLAUDE.md`'s E2E block):
```bash
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
cd e2e && pnpm test
```
Expected: all specs pass, including the new one. If this environment
has no display/`tauri-driver` available, say so explicitly rather than
skipping silently — a prior phase found `DISPLAY`/`Xvfb`/`tauri-driver`
present in this environment, so check before assuming either way.

- [ ] **Step 3: Commit**

```bash
git add e2e/specs/command-palette.spec.ts
git commit -m "test(e2e): add command palette spec"
```

---

### Task 5: Full verification and theme check

**Files:** none (verification only).

- [ ] **Step 1: Full test suite, build, lint**

Run: `cd frontend && pnpm build && pnpm lint && pnpm test -- --run`
Expected: all PASS.

- [ ] **Step 2: Manual theme check**

Open the app (`cargo tauri dev` from `crates/tauri-app`, or `pnpm dev`
in `frontend/` against a mock), press `Ctrl/Cmd+K`, and visually
confirm the palette (input, results list, empty state by typing
garbage, the highlighted row) renders legibly in both light and dark
theme. If this environment can't render a real window, say so
explicitly rather than skipping silently.

- [ ] **Step 3: Confirm scope**

Run: `git diff --stat main..HEAD` (or against whatever base this
branch forked from) and confirm the only files touched are
`frontend/src/lib/commands.ts`, `frontend/src/lib/commands.test.ts`,
`frontend/src/components/CommandPalette.tsx`,
`frontend/src/components/CommandPalette.module.css`,
`frontend/src/components/CommandPalette.test.tsx`,
`frontend/src/App.tsx`, and `e2e/specs/command-palette.spec.ts` — no
existing component, primitive, or `RepoClient`/DTO/Tauri/`git-core`
file appears in the list.

- [ ] **Step 4: Commit (only if Steps 1-3 required a fix)**

No commit expected from this task under normal circumstances — it is a
verification checkpoint only.
