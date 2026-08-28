# Sidebar Panel Toggles and Unified Branch/Remote Tree Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users hide unused sidebar sections (Stash, Worktree, Submodule, Reflog, Tags, Pull Requests), and replace the separate `BranchSwitcher` dropdown and `RemotePanel` accordion with one always-expanded Branches tree (local branches + one folder per remote), with all mutating actions moved to right-click context menus and the command palette.

**Architecture:** Frontend-only. A new `ContextMenu` primitive (extracted from `CommitGraph.tsx`'s existing bespoke right-click menu) backs every context-menu action. A new `useSidebarPanelVisibility` hook persists panel show/hide to `localStorage`, surfaced via a new gear popover in the `Sidebar` primitive. `BranchTree.tsx` replaces `BranchSwitcher.tsx` and `RemotePanel.tsx` wholesale, calling the exact same `AppState` methods those two components call today.

**Tech Stack:** React 18 + TypeScript, Vitest + Testing Library, CSS Modules, `lucide-react` icons.

**Spec:** `docs/superpowers/specs/2026-08-28-sidebar-panel-toggles-and-branch-tree-design.md`

## Global Constraints

- No `RepoClient` method, DTO, Tauri command, worker message, or `git-core` function changes shape or is added/removed — every task is `frontend/src/` only.
- No brand/product names in any new file or comment (per the spec's stated reason: keep the reference product's name out of shipped docs/code).
- Every mutating action a deleted component exposed today must remain reachable in `BranchTree` — either via its context menu or (only where explicitly called out below) the command palette. Nothing quietly disappears.
- Follow existing patterns exactly where one exists: `localStorage` persistence mirrors `lib/theme.ts`/`AccordionSection.tsx`; confirmation dialogs reuse `ConfirmDialog`; disabled-button explanations reuse the `operationDisabledReason` `title` convention (issue #31/UX-003).

---

## Task 1: Extract a shared `ContextMenu` primitive from `CommitGraph`

`CommitGraph.tsx` already hand-rolls a right-click menu (local `contextMenu` state, `handleContextMenu`, a fixed-position `<ul>`, close-on-mouseleave). `BranchTree` (Task 7) needs the identical mechanism a second time — extract it now so neither component reimplements it.

**Files:**
- Create: `frontend/src/components/primitives/ContextMenu.tsx`
- Create: `frontend/src/components/primitives/ContextMenu.module.css`
- Create: `frontend/src/components/primitives/ContextMenu.test.tsx`
- Modify: `frontend/src/components/CommitGraph.tsx:1-7,40-49,66-69,92-97,147-193` (imports, state, and the inline menu JSX)

**Interfaces:**
- Produces: `ContextMenu({ x, y, items, onClose }: { x: number; y: number; items: ContextMenuItem[]; onClose: () => void })`, and `interface ContextMenuItem { label: string; onSelect: () => void; disabled?: boolean; destructive?: boolean }`, both exported from `ContextMenu.tsx`.
- Consumes: nothing from earlier tasks.

- [ ] **Step 1: Write the failing test**

```tsx
// frontend/src/components/primitives/ContextMenu.test.tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ContextMenu } from "./ContextMenu";

describe("ContextMenu", () => {
  it("renders items at the given position and calls onSelect then onClose when one is clicked", () => {
    const onSelect = vi.fn();
    const onClose = vi.fn();
    render(
      <ContextMenu
        x={10}
        y={20}
        onClose={onClose}
        items={[{ label: "Branch from here", onSelect }]}
      />,
    );
    const menu = screen.getByRole("menu");
    expect(menu).toHaveStyle({ left: "10px", top: "20px" });
    fireEvent.click(screen.getByRole("menuitem", { name: "Branch from here" }));
    expect(onSelect).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("disables an item marked disabled and does not call onSelect when clicked", () => {
    const onSelect = vi.fn();
    render(
      <ContextMenu x={0} y={0} onClose={() => {}} items={[{ label: "Rebase onto here", onSelect, disabled: true }]} />,
    );
    const item = screen.getByRole("menuitem", { name: "Rebase onto here" });
    expect(item).toBeDisabled();
    fireEvent.click(item);
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("marks a destructive item for styling via a data attribute", () => {
    render(
      <ContextMenu x={0} y={0} onClose={() => {}} items={[{ label: "Remove remote", onSelect: () => {}, destructive: true }]} />,
    );
    expect(screen.getByRole("menuitem", { name: "Remove remote" })).toHaveAttribute("data-destructive", "true");
  });

  it("closes on Escape", () => {
    const onClose = vi.fn();
    render(<ContextMenu x={0} y={0} onClose={onClose} items={[{ label: "X", onSelect: () => {} }]} />);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("closes on a click outside the menu", () => {
    const onClose = vi.fn();
    render(
      <div>
        <button>outside</button>
        <ContextMenu x={0} y={0} onClose={onClose} items={[{ label: "X", onSelect: () => {} }]} />
      </div>,
    );
    fireEvent.mouseDown(screen.getByRole("button", { name: "outside" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("closes on mouse leave, matching the menu it replaces in CommitGraph", () => {
    const onClose = vi.fn();
    render(<ContextMenu x={0} y={0} onClose={onClose} items={[{ label: "X", onSelect: () => {} }]} />);
    fireEvent.mouseLeave(screen.getByRole("menu"));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && pnpm vitest run src/components/primitives/ContextMenu.test.tsx`
Expected: FAIL — `Cannot find module './ContextMenu'`

- [ ] **Step 3: Write the primitive**

```tsx
// frontend/src/components/primitives/ContextMenu.tsx
import { useEffect, useRef } from "react";
import styles from "./ContextMenu.module.css";

export interface ContextMenuItem {
  label: string;
  onSelect: () => void;
  disabled?: boolean;
  destructive?: boolean;
}

export function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: ContextMenuItem[];
  onClose: () => void;
}) {
  const menuRef = useRef<HTMLUListElement>(null);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    function handleClickOutside(event: MouseEvent) {
      if (menuRef.current !== null && !menuRef.current.contains(event.target as Node)) onClose();
    }
    document.addEventListener("keydown", handleKeyDown);
    document.addEventListener("mousedown", handleClickOutside);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("mousedown", handleClickOutside);
    };
  }, [onClose]);

  return (
    <ul
      ref={menuRef}
      role="menu"
      className={styles.menu}
      style={{ position: "fixed", top: y, left: x }}
      onMouseLeave={onClose}
    >
      {items.map((item) => (
        <li key={item.label} role="none">
          <button
            type="button"
            role="menuitem"
            className={styles.item}
            disabled={item.disabled}
            data-destructive={item.destructive === true ? "true" : undefined}
            onClick={() => {
              if (item.disabled === true) return;
              item.onSelect();
              onClose();
            }}
          >
            {item.label}
          </button>
        </li>
      ))}
    </ul>
  );
}
```

```css
/* frontend/src/components/primitives/ContextMenu.module.css */
.menu {
  z-index: 20;
  display: grid;
  gap: 1px;
  margin: 0;
  padding: var(--space-1);
  list-style: none;
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
  min-width: 10rem;
}

.item {
  display: block;
  width: 100%;
  padding: var(--space-1) var(--space-2);
  border: none;
  background: transparent;
  color: var(--color-text);
  font-size: var(--text-sm);
  text-align: left;
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.item:hover:not(:disabled) {
  background: var(--color-selected-bg);
}

.item:disabled {
  color: var(--color-text-muted);
  cursor: not-allowed;
}

.item[data-destructive="true"] {
  color: var(--color-danger-text);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && pnpm vitest run src/components/primitives/ContextMenu.test.tsx`
Expected: PASS (all 6 tests)

- [ ] **Step 5: Refactor `CommitGraph.tsx` to consume it**

Replace the import block and the inline menu:

```tsx
// frontend/src/components/CommitGraph.tsx — imports
import { ListRow } from "./primitives/ListRow";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";
```

Replace the closing `{contextMenu !== null && ( ... )}` block (the old lines 147-193) with:

```tsx
      {contextMenu !== null && (
        <ContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          onClose={() => setContextMenu(null)}
          items={
            squashMenuActive && activeSquashRange !== null
              ? [
                  {
                    label: `Squash ${activeSquashRange.end - activeSquashRange.start + 1} commits`,
                    onSelect: () => {
                      const ontoId = commits[activeSquashRange.end].parentIds[0];
                      const squashIds = commits
                        .slice(activeSquashRange.start, activeSquashRange.end)
                        .map((commit) => commit.id);
                      onSquashCommits?.(ontoId, squashIds);
                    },
                  },
                ]
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
        />
      )}
```

No other lines in `CommitGraph.tsx` change — `contextMenu` state, `handleContextMenu`, and everything else stays as-is.

- [ ] **Step 6: Run the full frontend test suite to check for regressions**

Run: `cd frontend && pnpm vitest run src/components/CommitGraph.test.tsx`
Expected: PASS, unchanged — this is a pure implementation swap, no behavior change.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/primitives/ContextMenu.tsx frontend/src/components/primitives/ContextMenu.module.css frontend/src/components/primitives/ContextMenu.test.tsx frontend/src/components/CommitGraph.tsx
git commit -m "refactor(frontend): extract ContextMenu primitive from CommitGraph"
```

---

## Task 2: Extract a shared persisted open/closed helper from `AccordionSection`

`AccordionSection.tsx`'s `loadOpen` (read `"open"`/`"closed"`/default from `localStorage`) and its inline `setOpenState` write are about to be needed a second time by `BranchTree`'s remote-folder expand/collapse state (Task 8). Extract both into a tiny shared module now.

**Files:**
- Create: `frontend/src/lib/persistedOpenState.ts`
- Create: `frontend/src/lib/persistedOpenState.test.ts`
- Modify: `frontend/src/components/primitives/AccordionSection.tsx:1-14,29-38` (import and use the extracted functions instead of the local `loadOpen`/inline `localStorage.setItem`)

**Interfaces:**
- Produces: `loadPersistedOpen(storageKey: string, defaultOpen: boolean): boolean` and `persistOpen(storageKey: string, open: boolean): void`, both from `frontend/src/lib/persistedOpenState.ts`.
- Consumes: nothing from earlier tasks.

- [ ] **Step 1: Write the failing test**

```ts
// frontend/src/lib/persistedOpenState.test.ts
import { beforeEach, describe, expect, it } from "vitest";
import { loadPersistedOpen, persistOpen } from "./persistedOpenState";

describe("persistedOpenState", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("returns the default when nothing is stored", () => {
    expect(loadPersistedOpen("k", true)).toBe(true);
    expect(loadPersistedOpen("k", false)).toBe(false);
  });

  it("round-trips true and false through localStorage", () => {
    persistOpen("k", true);
    expect(loadPersistedOpen("k", false)).toBe(true);
    persistOpen("k", false);
    expect(loadPersistedOpen("k", true)).toBe(false);
  });

  it("ignores unrelated storage keys", () => {
    persistOpen("other-key", true);
    expect(loadPersistedOpen("k", false)).toBe(false);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && pnpm vitest run src/lib/persistedOpenState.test.ts`
Expected: FAIL — `Cannot find module './persistedOpenState'`

- [ ] **Step 3: Write the implementation**

```ts
// frontend/src/lib/persistedOpenState.ts
export function loadPersistedOpen(storageKey: string, defaultOpen: boolean): boolean {
  const stored = localStorage.getItem(storageKey);
  if (stored === "open") return true;
  if (stored === "closed") return false;
  return defaultOpen;
}

export function persistOpen(storageKey: string, open: boolean): void {
  localStorage.setItem(storageKey, open ? "open" : "closed");
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && pnpm vitest run src/lib/persistedOpenState.test.ts`
Expected: PASS (3 tests)

- [ ] **Step 5: Update `AccordionSection.tsx` to use it**

```tsx
// frontend/src/components/primitives/AccordionSection.tsx
import { ChevronRight, type LucideIcon } from "lucide-react";
import { createElement, useEffect, useRef, useState, type ReactNode } from "react";
import { loadPersistedOpen, persistOpen } from "../../lib/persistedOpenState";
import { useAccordionGroup } from "./AccordionGroup";
import styles from "./AccordionSection.module.css";
```

Remove the local `loadOpen` function entirely. In the component body:

```tsx
  const [open, setOpen] = useState(() => loadPersistedOpen(storageKey, defaultOpen));
  const headerRef = useRef<HTMLButtonElement>(null);
  const group = useAccordionGroup();

  function setOpenState(next: boolean) {
    setOpen(next);
    persistOpen(storageKey, next);
  }
```

- [ ] **Step 6: Run `AccordionSection`'s existing tests to confirm no regression**

Run: `cd frontend && pnpm vitest run src/components/primitives/AccordionSection.test.tsx src/components/primitives/Sidebar.test.tsx`
Expected: PASS, unchanged

- [ ] **Step 7: Commit**

```bash
git add frontend/src/lib/persistedOpenState.ts frontend/src/lib/persistedOpenState.test.ts frontend/src/components/primitives/AccordionSection.tsx
git commit -m "refactor(frontend): extract persisted open/closed state helper from AccordionSection"
```

---

## Task 3: `useSidebarPanelVisibility` hook

**Files:**
- Create: `frontend/src/state/useSidebarPanelVisibility.ts`
- Create: `frontend/src/state/useSidebarPanelVisibility.test.ts`

**Interfaces:**
- Produces: `type SidebarPanelId = "stash" | "worktree" | "submodule" | "reflog" | "tags" | "pullRequests"`; `SIDEBAR_PANEL_IDS: SidebarPanelId[]`; `useSidebarPanelVisibility(): { visibility: Record<SidebarPanelId, boolean>; setPanelVisible: (id: SidebarPanelId, visible: boolean) => void }`. Task 4 (Sidebar gear popover) and Task 9 (App.tsx wiring) both consume this exact shape.
- Consumes: nothing from earlier tasks.

- [ ] **Step 1: Write the failing test**

```ts
// frontend/src/state/useSidebarPanelVisibility.test.ts
import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { SIDEBAR_PANEL_IDS, useSidebarPanelVisibility } from "./useSidebarPanelVisibility";

describe("useSidebarPanelVisibility", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("defaults every known panel to visible", () => {
    const { result } = renderHook(() => useSidebarPanelVisibility());
    for (const id of SIDEBAR_PANEL_IDS) {
      expect(result.current.visibility[id]).toBe(true);
    }
  });

  it("setPanelVisible updates state and persists to localStorage", () => {
    const { result } = renderHook(() => useSidebarPanelVisibility());
    act(() => result.current.setPanelVisible("worktree", false));
    expect(result.current.visibility.worktree).toBe(false);
    expect(JSON.parse(localStorage.getItem("sidebar.panels") ?? "{}")).toMatchObject({ worktree: false });
  });

  it("loads a previously persisted map on mount", () => {
    localStorage.setItem("sidebar.panels", JSON.stringify({ reflog: false }));
    const { result } = renderHook(() => useSidebarPanelVisibility());
    expect(result.current.visibility.reflog).toBe(false);
    expect(result.current.visibility.stash).toBe(true);
  });

  it("falls back to defaults when the stored value is malformed", () => {
    localStorage.setItem("sidebar.panels", "not json");
    const { result } = renderHook(() => useSidebarPanelVisibility());
    expect(result.current.visibility.stash).toBe(true);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && pnpm vitest run src/state/useSidebarPanelVisibility.test.ts`
Expected: FAIL — `Cannot find module './useSidebarPanelVisibility'`

- [ ] **Step 3: Write the implementation**

```ts
// frontend/src/state/useSidebarPanelVisibility.ts
import { useState } from "react";

export type SidebarPanelId = "stash" | "worktree" | "submodule" | "reflog" | "tags" | "pullRequests";

export const SIDEBAR_PANEL_IDS: SidebarPanelId[] = [
  "stash",
  "worktree",
  "submodule",
  "reflog",
  "tags",
  "pullRequests",
];

const STORAGE_KEY = "sidebar.panels";

function defaults(): Record<SidebarPanelId, boolean> {
  return Object.fromEntries(SIDEBAR_PANEL_IDS.map((id) => [id, true])) as Record<SidebarPanelId, boolean>;
}

function loadVisibility(): Record<SidebarPanelId, boolean> {
  const result = defaults();
  const stored = localStorage.getItem(STORAGE_KEY);
  if (stored === null) return result;
  try {
    const parsed: unknown = JSON.parse(stored);
    if (typeof parsed !== "object" || parsed === null) return result;
    for (const id of SIDEBAR_PANEL_IDS) {
      const value = (parsed as Record<string, unknown>)[id];
      if (typeof value === "boolean") result[id] = value;
    }
    return result;
  } catch {
    return result;
  }
}

export function useSidebarPanelVisibility(): {
  visibility: Record<SidebarPanelId, boolean>;
  setPanelVisible: (id: SidebarPanelId, visible: boolean) => void;
} {
  const [visibility, setVisibility] = useState<Record<SidebarPanelId, boolean>>(loadVisibility);

  const setPanelVisible = (id: SidebarPanelId, visible: boolean) => {
    setVisibility((prev) => {
      const next = { ...prev, [id]: visible };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
      return next;
    });
  };

  return { visibility, setPanelVisible };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && pnpm vitest run src/state/useSidebarPanelVisibility.test.ts`
Expected: PASS (4 tests)

- [ ] **Step 5: Commit**

```bash
git add frontend/src/state/useSidebarPanelVisibility.ts frontend/src/state/useSidebarPanelVisibility.test.ts
git commit -m "feat(frontend): add useSidebarPanelVisibility hook"
```

---

## Task 4: Gear popover in the `Sidebar` primitive

**Files:**
- Modify: `frontend/src/components/primitives/Sidebar.tsx` (whole file — add the `panelToggles` prop and gear popover)
- Modify: `frontend/src/components/primitives/Sidebar.module.css` (append popover styles)
- Modify: `frontend/src/components/primitives/Sidebar.test.tsx` (append new tests)

**Interfaces:**
- Consumes: nothing from earlier tasks directly — `panelToggles` is a plain data shape `Sidebar` defines itself, so it stays a generic primitive; Task 9 supplies the array from `useSidebarPanelVisibility` (Task 3).
- Produces: `Sidebar({ children, panelToggles }: { children: ReactNode; panelToggles?: { id: string; label: string; visible: boolean; onToggle: (visible: boolean) => void }[] })`.

- [ ] **Step 1: Write the failing tests** (append to the existing file)

```tsx
// frontend/src/components/primitives/Sidebar.test.tsx — add inside the existing describe block
  it("does not render a settings button when panelToggles is omitted", () => {
    render(
      <Sidebar>
        <div>body</div>
      </Sidebar>,
    );
    expect(screen.queryByRole("button", { name: "Sidebar section settings" })).not.toBeInTheDocument();
  });

  it("opens a checkbox popover listing each panel toggle, and calls onToggle when one is checked", () => {
    const onToggle = vi.fn();
    render(
      <Sidebar
        panelToggles={[
          { id: "worktree", label: "Worktrees", visible: true, onToggle },
          { id: "reflog", label: "Reflog", visible: false, onToggle: vi.fn() },
        ]}
      >
        <div>body</div>
      </Sidebar>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Sidebar section settings" }));
    const worktreeCheckbox = screen.getByRole("checkbox", { name: "Worktrees" });
    const reflogCheckbox = screen.getByRole("checkbox", { name: "Reflog" });
    expect(worktreeCheckbox).toBeChecked();
    expect(reflogCheckbox).not.toBeChecked();
    fireEvent.click(worktreeCheckbox);
    expect(onToggle).toHaveBeenCalledWith(false);
  });

  it("closes the settings popover when its button is clicked again", () => {
    render(
      <Sidebar panelToggles={[{ id: "worktree", label: "Worktrees", visible: true, onToggle: vi.fn() }]}>
        <div>body</div>
      </Sidebar>,
    );
    const button = screen.getByRole("button", { name: "Sidebar section settings" });
    fireEvent.click(button);
    expect(screen.getByRole("checkbox", { name: "Worktrees" })).toBeInTheDocument();
    fireEvent.click(button);
    expect(screen.queryByRole("checkbox", { name: "Worktrees" })).not.toBeInTheDocument();
  });
```

Add `vi` to the existing `vitest` import at the top of the file.

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && pnpm vitest run src/components/primitives/Sidebar.test.tsx`
Expected: FAIL — no "Sidebar section settings" button exists yet

- [ ] **Step 3: Implement**

```tsx
// frontend/src/components/primitives/Sidebar.tsx
import { useRef, useState, type ReactNode } from "react";
import { ChevronsDownUp, ChevronsUpDown, Settings } from "lucide-react";
import { AccordionGroup, type AccordionGroupHandle } from "./AccordionGroup";
import styles from "./Sidebar.module.css";

export interface SidebarPanelToggle {
  id: string;
  label: string;
  visible: boolean;
  onToggle: (visible: boolean) => void;
}

export function Sidebar({ children, panelToggles }: { children: ReactNode; panelToggles?: SidebarPanelToggle[] }) {
  const groupRef = useRef<AccordionGroupHandle | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);

  return (
    <aside className={styles.sidebar} aria-label="Repository sections">
      <div className={styles.toolbar}>
        <button
          type="button"
          className={styles.toolbarButton}
          aria-label="Expand all sections"
          onClick={() => groupRef.current?.expandAll()}
        >
          <ChevronsUpDown size={14} aria-hidden="true" />
        </button>
        <button
          type="button"
          className={styles.toolbarButton}
          aria-label="Collapse all sections"
          onClick={() => groupRef.current?.collapseAll()}
        >
          <ChevronsDownUp size={14} aria-hidden="true" />
        </button>
        {panelToggles !== undefined && (
          <div className={styles.settingsWrapper}>
            <button
              type="button"
              className={styles.toolbarButton}
              aria-label="Sidebar section settings"
              aria-expanded={settingsOpen}
              onClick={() => setSettingsOpen((open) => !open)}
            >
              <Settings size={14} aria-hidden="true" />
            </button>
            {settingsOpen && (
              <div className={styles.settingsPopover} role="menu" aria-label="Toggle sidebar sections">
                {panelToggles.map((toggle) => (
                  <label key={toggle.id} className={styles.settingsRow}>
                    <input
                      type="checkbox"
                      checked={toggle.visible}
                      onChange={(event) => toggle.onToggle(event.target.checked)}
                    />
                    {toggle.label}
                  </label>
                ))}
              </div>
            )}
          </div>
        )}
      </div>
      <AccordionGroup groupRef={groupRef}>
        <div className={styles.sections}>{children}</div>
      </AccordionGroup>
    </aside>
  );
}
```

```css
/* frontend/src/components/primitives/Sidebar.module.css — append */
.settingsWrapper {
  position: relative;
}

.settingsPopover {
  position: absolute;
  top: 100%;
  right: 0;
  z-index: 20;
  display: grid;
  gap: var(--space-1);
  margin-top: var(--space-1);
  padding: var(--space-2);
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
  white-space: nowrap;
}

.settingsRow {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  font-size: var(--text-sm);
  color: var(--color-text);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && pnpm vitest run src/components/primitives/Sidebar.test.tsx`
Expected: PASS (all tests, old and new)

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/primitives/Sidebar.tsx frontend/src/components/primitives/Sidebar.module.css frontend/src/components/primitives/Sidebar.test.tsx
git commit -m "feat(frontend): add sidebar section visibility settings popover"
```

---

## Task 5: `addRemoteDraftOpen` state, so "Add remote" is reachable from the command palette

Mirrors the existing `createBranchDraft` pattern exactly (`useAppState.ts`'s `AppState.createBranchDraft` / `useBranchActions.ts`'s `openCreateBranchDraft`/`closeCreateBranchDraft`), so both the tree's own "+" button (Task 8) and a new command-palette entry (Task 6) can open the same form.

**Files:**
- Modify: `frontend/src/state/useAppState.ts:39-... ` (add `addRemoteDraftOpen: boolean` to `AppState`, `false` to the initial state, and wire the two new methods through)
- Modify: `frontend/src/state/useRemoteTransferActions.ts` (add `openAddRemoteDraft`/`closeAddRemoteDraft`)
- Modify: `frontend/src/state/useRemoteTransferActions.test.ts` (append tests — if this file does not exist yet, check for it first; if remote-transfer actions are only covered indirectly today, create it following the naming convention of sibling `use*Actions.test.ts` files)

**Interfaces:**
- Produces: `AppState.addRemoteDraftOpen: boolean`; `UseAppStateResult.openAddRemoteDraft(): void`; `UseAppStateResult.closeAddRemoteDraft(): void`.
- Consumes: nothing from earlier tasks.

- [ ] **Step 1: Check for an existing test file and its `setState`-mocking convention**

Run: `ls frontend/src/state/use*Actions.test.ts 2>/dev/null || echo none`

If `useBranchActions.test.ts` (or similar) exists, open it and copy its exact mock/harness setup for `client`, `runMutation`-family stubs, and `setState`. Use that same harness for the new test below instead of reinventing one.

- [ ] **Step 2: Write the failing test**

```ts
// frontend/src/state/useRemoteTransferActions.test.ts — add this case using the file's existing test harness
it("openAddRemoteDraft/closeAddRemoteDraft toggle addRemoteDraftOpen", () => {
  const { result, setState } = renderRemoteTransferActions(); // use whatever harness helper this file already exposes
  result.current.openAddRemoteDraft();
  expect(setState).toHaveBeenCalled();
  const updater = setState.mock.calls[setState.mock.calls.length - 1][0];
  expect(updater({ addRemoteDraftOpen: false } as never).addRemoteDraftOpen).toBe(true);
  result.current.closeAddRemoteDraft();
  const closeUpdater = setState.mock.calls[setState.mock.calls.length - 1][0];
  expect(closeUpdater({ addRemoteDraftOpen: true } as never).addRemoteDraftOpen).toBe(false);
});
```

If no such file/harness exists yet for this hook family, instead add coverage at the `useAppState.ts` level (check for `useAppState.test.ts` first) asserting that calling `result.current.openAddRemoteDraft()` on a rendered `useAppState` flips `result.current.state.addRemoteDraftOpen` to `true`, and `closeAddRemoteDraft()` flips it back — following whatever `RepoClient` mock that file already builds.

- [ ] **Step 3: Run test to verify it fails**

Run: `cd frontend && pnpm vitest run src/state/useRemoteTransferActions.test.ts` (or `useAppState.test.ts`, matching whichever file Step 2 used)
Expected: FAIL — `openAddRemoteDraft is not a function`

- [ ] **Step 4: Implement**

In `useAppState.ts`, add to the `AppState` interface (near `createBranchDraft`):

```ts
  createBranchDraft: { startPoint: string } | null;
  // Mirrors createBranchDraft: lets both BranchTree's own "+" button and the command palette's
  // "Add remote" entry open the same inline add-remote form.
  addRemoteDraftOpen: boolean;
```

Add `addRemoteDraftOpen: false,` next to `createBranchDraft: null,` in the initial `useState` object.

In `useRemoteTransferActions.ts`, add to the `RemoteTransferActions` interface:

```ts
  openAddRemoteDraft(): void;
  closeAddRemoteDraft(): void;
```

And in the function body, alongside the other `useCallback`s:

```ts
  const openAddRemoteDraft = useCallback(() => {
    setState((prev) => ({ ...prev, addRemoteDraftOpen: true }));
  }, [setState]);
  const closeAddRemoteDraft = useCallback(() => {
    setState((prev) => ({ ...prev, addRemoteDraftOpen: false }));
  }, [setState]);
```

Add both to the returned object.

In `useAppState.ts`'s destructuring of `useRemoteTransferActions(...)` and its own returned object, add `openAddRemoteDraft` and `closeAddRemoteDraft` alongside the existing `addRemote, renameRemote, ...` list (both at the destructure site and the final `return { ... }`).

- [ ] **Step 5: Run test to verify it passes**

Run: same command as Step 3
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add frontend/src/state/useAppState.ts frontend/src/state/useRemoteTransferActions.ts frontend/src/state/useRemoteTransferActions.test.ts
git commit -m "feat(frontend): add addRemoteDraftOpen state for command-palette-triggered add-remote"
```

(Adjust the `git add` path if Step 2 instead added coverage to `useAppState.test.ts`.)

---

## Task 6: `commands.ts` — fold "Remotes" into "Branches", add "Add remote"

**Files:**
- Modify: `frontend/src/lib/commands.ts:15-24,118` (drop `"Remotes"` from `SIDEBAR_SECTIONS`; add the new command)
- Modify: `frontend/src/lib/commands.test.ts` (append tests)

**Interfaces:**
- Consumes: `appState.openAddRemoteDraft` from Task 5.

- [ ] **Step 1: Write the failing tests**

```ts
// frontend/src/lib/commands.test.ts — add these cases, using the file's existing appState-mock builder
it("no longer emits a separate 'Go to Remotes' command (folded into Branches)", () => {
  const commands = buildCommands(makeAppState());
  expect(commands.find((c) => c.id === "go-to:Remotes")).toBeUndefined();
  expect(commands.find((c) => c.id === "go-to:Branches")).toBeDefined();
});

it("emits an Add remote command that opens the add-remote draft", () => {
  const appState = makeAppState();
  const commands = buildCommands(appState);
  const addRemote = commands.find((c) => c.id === "add-remote");
  expect(addRemote).toBeDefined();
  addRemote?.run();
  expect(appState.openAddRemoteDraft).toHaveBeenCalledOnce();
});

it("omits Add remote while a repository operation is in progress", () => {
  const appState = makeAppState({ pending: true });
  const commands = buildCommands(appState);
  expect(commands.find((c) => c.id === "add-remote")).toBeUndefined();
});
```

Use whatever `makeAppState`/mock-builder helper this test file already has (it already builds a mocked `UseAppStateResult` for the other `buildCommands` tests — extend it with `openAddRemoteDraft: vi.fn()` if it isn't already a blanket `vi.fn()`-per-method mock).

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && pnpm vitest run src/lib/commands.test.ts`
Expected: FAIL — `"Remotes"` still present in `SIDEBAR_SECTIONS`; no `add-remote` command exists

- [ ] **Step 3: Implement**

```ts
// frontend/src/lib/commands.ts
const SIDEBAR_SECTIONS = [
  "Branches",
  "Stashes",
  "Worktrees",
  "Submodules",
  "Reflog",
  "Tags",
  "Pull Requests",
] as const;
```

Add, alongside the other `!repositoryOperationDisabled` remote-scoped commands (right after the `for (const remote of state.remotes) { ... }` loop closes, still inside the `if (!repositoryOperationDisabled) { ... }` block):

```ts
    commands.push({
      id: "add-remote",
      label: "Add remote",
      keywords: ["remote", "add"],
      run: () => appState.openAddRemoteDraft(),
    });
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && pnpm vitest run src/lib/commands.test.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/commands.ts frontend/src/lib/commands.test.ts
git commit -m "feat(frontend): fold Remotes into Branches in command palette, add Add-remote command"
```

---

## Task 7: `BranchTree` — local branches (replaces `BranchSwitcher`)

This is the first half of the new component: local-branch listing and every action `BranchSwitcher` offered, now via right-click instead of a dropdown popover. `RemotePanel` keeps running side-by-side in the sidebar until Task 8 absorbs it — this task alone leaves the app fully working.

**Files:**
- Create: `frontend/src/components/BranchTree.tsx`
- Create: `frontend/src/components/BranchTree.module.css`
- Create: `frontend/src/components/BranchTree.test.tsx`
- Modify: `frontend/src/App.tsx:159-175` (replace `<BranchSwitcher ... />` with `<BranchTree ... />`, dropping the `graphBranchSelection`/`onSetGraphBranchSelection` props unchanged and adding placeholder remote props wired from Task 8 — see that task's App.tsx step; for this task, pass `remotes={[]}` and no-op stubs for the remote-only props so the component type-checks before Task 8 fills them in for real)

**Interfaces:**
- Consumes: `ContextMenu`/`ContextMenuItem` (Task 1), `loadPersistedOpen`/`persistOpen` (Task 2).
- Produces: `BranchTree` accepting every prop `BranchSwitcher` accepted today (`branches`, `createBranchDraft`, `onSwitchBranch`, `onCreateBranch`, `onDeleteBranch`, `onRenameBranch`, `onOpenCreateBranchDraft`, `onCloseCreateBranchDraft`, `onMergeBranch`, `isMerging`, `isRebasing`, `operationDisabled`, `operationDisabledReason`, `graphBranchSelection`, `onSetGraphBranchSelection`) — Task 8 adds the remote-facing props to this same component/file.

### Behavior mapping (BranchSwitcher → BranchTree), preserved exactly

| BranchSwitcher today | BranchTree |
|---|---|
| Toggle button + popover list | Always-expanded "Local" folder inside the "Branches" `AccordionSection` (no popover) |
| Click a branch name to switch | Same — a plain button per branch row |
| Per-row Rename/Merge/Delete buttons + graph checkbox | Right-click → `ContextMenu` with the same actions; graph-visibility checkbox stays a visible inline checkbox per row (it's a toggle, not an action — keeping it visible avoids a menu round-trip for something users flip often while browsing history) |
| Force-delete `ConfirmDialog` | Unchanged, same component, same trigger flow |
| "New Branch…" toolbar button | Section-header "+" button (Task 8 adds "Add Remote" as a second option in the same menu) |

- [ ] **Step 1: Write the failing tests**

```tsx
// frontend/src/components/BranchTree.test.tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { BranchTree } from "./BranchTree";

const baseBranches = [
  { name: "main", isCurrent: true },
  { name: "feat/foo", isCurrent: false },
];

type BranchTreeProps = Parameters<typeof BranchTree>[0];

function renderTree(overrides: Partial<BranchTreeProps> = {}) {
  const props: BranchTreeProps = {
    branches: baseBranches,
    createBranchDraft: null,
    onSwitchBranch: vi.fn(),
    onCreateBranch: vi.fn().mockResolvedValue(null),
    onDeleteBranch: vi.fn().mockResolvedValue(undefined),
    onRenameBranch: vi.fn(),
    onOpenCreateBranchDraft: vi.fn(),
    onCloseCreateBranchDraft: vi.fn(),
    onMergeBranch: vi.fn(),
    isMerging: false,
    isRebasing: false,
    operationDisabled: false,
    operationDisabledReason: null,
    graphBranchSelection: null,
    onSetGraphBranchSelection: vi.fn(),
    remotes: [],
    upstream: null,
    remoteUpstreams: {},
    onAddRemote: vi.fn().mockResolvedValue(null),
    onRenameRemote: vi.fn().mockResolvedValue(true),
    onUpdateRemoteUrls: vi.fn().mockResolvedValue(undefined),
    onRemoveRemote: vi.fn().mockResolvedValue(undefined),
    onSaveHttpsCredential: vi.fn().mockResolvedValue(undefined),
    onForgetHttpsCredential: vi.fn().mockResolvedValue(undefined),
    onSetRemoteAuthMode: vi.fn().mockResolvedValue(true),
    onSetUpstream: vi.fn().mockResolvedValue(undefined),
    onClearUpstream: vi.fn().mockResolvedValue(undefined),
    onListRemoteBranches: vi.fn().mockResolvedValue([]),
    onFetchRemote: vi.fn().mockResolvedValue(undefined),
    onPushCurrentBranch: vi.fn().mockResolvedValue(undefined),
    onPull: vi.fn().mockResolvedValue(undefined),
    pendingPull: null,
    pullOutcome: null,
    onMergePull: vi.fn().mockResolvedValue(undefined),
    onRebasePull: vi.fn(),
    onCancelPull: vi.fn(),
    addRemoteDraftOpen: false,
    onOpenAddRemoteDraft: vi.fn(),
    onCloseAddRemoteDraft: vi.fn(),
    ...overrides,
  };
  return { ...render(<BranchTree {...props} />), props };
}

describe("BranchTree — local branches", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("lists every local branch and marks the current one", () => {
    renderTree();
    expect(screen.getByText("main")).toBeInTheDocument();
    expect(screen.getByText(/main.*\(current\)/)).toBeInTheDocument();
    expect(screen.getByText("feat/foo")).toBeInTheDocument();
  });

  it("clicking a non-current branch switches to it", () => {
    const { props } = renderTree();
    fireEvent.click(screen.getByRole("button", { name: "feat/foo" }));
    expect(props.onSwitchBranch).toHaveBeenCalledWith("feat/foo");
  });

  it("right-clicking a branch opens a context menu with Rename/Delete, and Merge for non-current branches", () => {
    renderTree();
    fireEvent.contextMenu(screen.getByText("feat/foo"));
    expect(screen.getByRole("menuitem", { name: "Rename" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Merge into current branch" })).toBeInTheDocument();
  });

  it("does not offer Merge from the current branch's context menu", () => {
    renderTree();
    fireEvent.contextMenu(screen.getByText(/main.*\(current\)/));
    expect(screen.queryByRole("menuitem", { name: "Merge into current branch" })).not.toBeInTheDocument();
  });

  it("Delete calls onDeleteBranch with force=false, then confirming Force Delete calls it again with force=true", async () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByText("feat/foo"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(props.onDeleteBranch).toHaveBeenCalledWith("feat/foo", false);
    fireEvent.click(await screen.findByRole("button", { name: "Force Delete" }));
    expect(props.onDeleteBranch).toHaveBeenCalledWith("feat/foo", true);
  });

  it("Rename shows an inline input; Enter calls onRenameBranch", () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByText("feat/foo"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    const input = screen.getByDisplayValue("feat/foo");
    fireEvent.change(input, { target: { value: "feat/bar" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(props.onRenameBranch).toHaveBeenCalledWith("feat/foo", "feat/bar");
  });

  it("Merge into current branch calls onMergeBranch with that branch's name", () => {
    const { props } = renderTree();
    fireEvent.contextMenu(screen.getByText("feat/foo"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Merge into current branch" }));
    expect(props.onMergeBranch).toHaveBeenCalledWith("feat/foo");
  });

  it("disables Rename/Delete/Merge menu items while a rebase is in progress", () => {
    renderTree({ isRebasing: true });
    fireEvent.contextMenu(screen.getByText("feat/foo"));
    expect(screen.getByRole("menuitem", { name: "Rename" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Delete" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Merge into current branch" })).toBeDisabled();
  });

  it("with no saved graph selection, every branch's graph checkbox is checked by default", () => {
    renderTree();
    expect(screen.getByRole("checkbox", { name: "Show main in graph" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Show feat/foo in graph" })).toBeChecked();
  });

  it("unchecking a branch's graph checkbox while showing all calls onSetGraphBranchSelection with every other branch", () => {
    const { props } = renderTree();
    fireEvent.click(screen.getByRole("checkbox", { name: "Show feat/foo in graph" }));
    expect(props.onSetGraphBranchSelection).toHaveBeenCalledWith(["main"]);
  });

  it("the header '+' menu's New Branch opens the create-branch draft with startPoint HEAD", () => {
    const { props } = renderTree();
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "New Branch…" }));
    expect(props.onOpenCreateBranchDraft).toHaveBeenCalledWith("HEAD");
  });

  it("a non-null createBranchDraft shows the create form; submitting calls onCreateBranch with its startPoint", async () => {
    const { props } = renderTree({ createBranchDraft: { startPoint: "HEAD" } });
    fireEvent.change(screen.getByPlaceholderText("New branch name"), { target: { value: "feat/new" } });
    fireEvent.click(screen.getByRole("button", { name: "Create" }));
    expect(props.onCreateBranch).toHaveBeenCalledWith("feat/new", "HEAD");
  });
});
```

Port every remaining `BranchSwitcher.test.tsx` case not listed above using this exact transformation (open-popover-then-click-row-button → right-click-row-then-click-menuitem); nothing else about the assertions changes. The full list still to port, by its current test name in `BranchSwitcher.test.tsx`:

- "shows a failed create-branch's message next to the draft form and keeps the entered name"
- "clears the create-branch failure message once the name is edited again"
- "dismissing the create-branch failure message clears it"
- "Cancel in the create form calls onCloseCreateBranchDraft"
- "a branch absent from an explicit selection renders unchecked; checking it adds it back"
- "Cancel in the force-delete dialog dismisses it without deleting, leaving Delete in place"
- "Enter on an empty/whitespace-only rename value does not call onRenameBranch"
- "disables the merge action while a merge is already in progress" (pass `isMerging: true`)
- "disables the merge action while another repository operation is pending" (pass `operationDisabled: true`)
- "explains why the merge action is disabled via its title" (assert the `title` attribute using `operationDisabledReason`)
- "does not switch branches while a rebase is in progress" (pass `isRebasing: true`, click the branch button, assert `onSwitchBranch` was not called — the button itself is `disabled`, not omitted, matching today's behavior)

The two tests about closing the popover clearing pending rename/force-delete state ("closing the popover clears a pending force-delete...", "closing the popover clears an in-progress rename...") do not port — there is no popover to close anymore. Instead add one replacement test asserting the equivalent guard still holds for the new interaction model:

```tsx
  it("right-clicking a different branch after starting a rename on one clears that rename's input", () => {
    renderTree();
    fireEvent.contextMenu(screen.getByText("feat/foo"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    expect(screen.getByDisplayValue("feat/foo")).toBeInTheDocument();
    fireEvent.contextMenu(screen.getByText(/main.*\(current\)/));
    expect(screen.queryByDisplayValue("feat/foo")).not.toBeInTheDocument();
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm vitest run src/components/BranchTree.test.tsx`
Expected: FAIL — `Cannot find module './BranchTree'`

- [ ] **Step 3: Implement `BranchTree.tsx` (local-branches portion; remote props accepted but unused until Task 8)**

```tsx
// frontend/src/components/BranchTree.tsx
import { useState, type KeyboardEvent } from "react";
import { ChevronRight, GitBranch, Plus } from "lucide-react";
import type {
  BranchInfo,
  PullOutcome,
  RemoteAuthMode,
  RemoteInfo,
  UpstreamInfo,
} from "../ipc/RepoClient";
import { loadPersistedOpen, persistOpen } from "../lib/persistedOpenState";
import { AccordionSection } from "./primitives/AccordionSection";
import { ConfirmDialog } from "./primitives/ConfirmDialog";
import { ContextMenu, type ContextMenuItem } from "./primitives/ContextMenu";
import { InlineError } from "./primitives/InlineError";
import { ListRow } from "./primitives/ListRow";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./BranchTree.module.css";

const LOCAL_FOLDER_KEY = "branchtree.local";

type RowContextMenu =
  | { kind: "local-branch"; name: string; x: number; y: number }
  | { kind: "add"; x: number; y: number };

export function BranchTree({
  branches,
  createBranchDraft,
  onSwitchBranch,
  onCreateBranch,
  onDeleteBranch,
  onRenameBranch,
  onOpenCreateBranchDraft,
  onCloseCreateBranchDraft,
  onMergeBranch,
  isMerging,
  isRebasing,
  operationDisabled,
  operationDisabledReason,
  graphBranchSelection,
  onSetGraphBranchSelection,
}: {
  branches: BranchInfo[];
  createBranchDraft: { startPoint: string } | null;
  onSwitchBranch: (name: string) => void;
  onCreateBranch: (name: string, startPoint: string) => Promise<string | null>;
  onDeleteBranch: (name: string, force: boolean) => Promise<void>;
  onRenameBranch: (oldName: string, newName: string) => void;
  onOpenCreateBranchDraft: (startPoint: string) => void;
  onCloseCreateBranchDraft: () => void;
  onMergeBranch: (name: string) => void;
  isMerging: boolean;
  isRebasing: boolean;
  operationDisabled: boolean;
  operationDisabledReason: string | null;
  graphBranchSelection: string[] | null;
  onSetGraphBranchSelection: (selectedBranches: string[]) => void;
  // Remote props: accepted from Task 7 onward for the type to match App.tsx's eventual single
  // call site, rendered starting in Task 8.
  remotes: RemoteInfo[];
  upstream: UpstreamInfo | null;
  remoteUpstreams: Record<string, UpstreamInfo[]>;
  onAddRemote: (name: string, fetchUrl: string, pushUrl: string | null) => Promise<string | null>;
  onRenameRemote: (oldName: string, newName: string) => Promise<boolean>;
  onUpdateRemoteUrls: (name: string, fetchUrl: string, pushUrl: string | null) => Promise<void>;
  onRemoveRemote: (name: string, clearUpstreams: boolean) => Promise<void>;
  onSaveHttpsCredential: (remoteName: string, username: string, token: string) => Promise<void>;
  onForgetHttpsCredential: (remoteName: string) => Promise<void>;
  onSetRemoteAuthMode: (remoteName: string, mode: RemoteAuthMode, username: string | null) => Promise<boolean>;
  onSetUpstream: (remoteName: string, remoteBranch: string) => Promise<void>;
  onClearUpstream: () => Promise<void>;
  onListRemoteBranches: (remoteName: string) => Promise<string[]>;
  onFetchRemote: (remoteName: string) => Promise<void>;
  onPushCurrentBranch: (remoteName: string) => Promise<void>;
  onPull: () => Promise<void>;
  pendingPull: { upstreamRef: string } | null;
  pullOutcome: PullOutcome | null;
  onMergePull: (upstreamRef: string) => Promise<void>;
  onRebasePull: (upstreamRef: string) => void;
  onCancelPull: () => void;
  addRemoteDraftOpen: boolean;
  onOpenAddRemoteDraft: () => void;
  onCloseAddRemoteDraft: () => void;
}) {
  const [newBranchName, setNewBranchName] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const [pendingForceFor, setPendingForceFor] = useState<string | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [rowMenu, setRowMenu] = useState<RowContextMenu | null>(null);
  const [localOpen, setLocalOpen] = useState(() => loadPersistedOpen(LOCAL_FOLDER_KEY, true));

  const submitCreate = async () => {
    if (newBranchName.trim() === "" || createBranchDraft === null) return;
    const failure = await onCreateBranch(newBranchName.trim(), createBranchDraft.startPoint);
    if (failure !== null) {
      setCreateError(failure);
      return;
    }
    setNewBranchName("");
    setCreateError(null);
  };

  const handleDeleteClick = async (name: string) => {
    await onDeleteBranch(name, false);
    setPendingForceFor(name);
  };

  const toggleGraphBranch = (name: string) => {
    const shown = graphBranchSelection ?? branches.map((b) => b.name);
    const next = shown.includes(name) ? shown.filter((n) => n !== name) : [...shown, name];
    onSetGraphBranchSelection(next);
  };

  const handleRenameKeyDown = (event: KeyboardEvent<HTMLInputElement>, oldName: string) => {
    if (event.key === "Enter") {
      if (renameValue.trim() === "") return;
      onRenameBranch(oldName, renameValue);
      setRenaming(null);
    }
  };

  function branchContextItems(branch: BranchInfo): ContextMenuItem[] {
    const items: ContextMenuItem[] = [
      {
        label: "Rename",
        disabled: isRebasing,
        onSelect: () => {
          setRenaming(branch.name);
          setRenameValue(branch.name);
        },
      },
    ];
    if (!branch.isCurrent) {
      items.push({
        label: "Merge into current branch",
        disabled: isMerging || isRebasing || operationDisabled,
        onSelect: () => onMergeBranch(branch.name),
      });
    }
    items.push({
      label: "Delete",
      disabled: isRebasing,
      destructive: true,
      onSelect: () => void handleDeleteClick(branch.name),
    });
    return items;
  }

  return (
    <AccordionSection title="Branches" storageKey="sidebar-branches" icon={GitBranch} count={branches.length} defaultOpen>
      <Toolbar aria-label="Branches actions">
        <button
          type="button"
          aria-label="Add"
          onClick={(event) => setRowMenu({ kind: "add", x: event.clientX, y: event.clientY })}
        >
          <Plus size={14} aria-hidden="true" />
        </button>
      </Toolbar>

      {createBranchDraft !== null && (
        <div className={styles.draftForm}>
          <input
            value={newBranchName}
            onChange={(event) => {
              setNewBranchName(event.target.value);
              setCreateError(null);
            }}
            placeholder="New branch name"
            onKeyDown={(event) => {
              if (event.key === "Enter") void submitCreate();
            }}
          />
          <button onClick={() => void submitCreate()} disabled={newBranchName.trim() === "" || isRebasing}>
            Create
          </button>
          <button
            onClick={() => {
              setCreateError(null);
              onCloseCreateBranchDraft();
            }}
          >
            Cancel
          </button>
          {createError !== null && <InlineError message={createError} onDismiss={() => setCreateError(null)} />}
        </div>
      )}

      <ul className={styles.tree}>
        <li className={styles.folder}>
          <button
            type="button"
            className={styles.folderHeader}
            aria-expanded={localOpen}
            onClick={() => {
              const next = !localOpen;
              setLocalOpen(next);
              persistOpen(LOCAL_FOLDER_KEY, next);
            }}
          >
            <ChevronRight
              size={14}
              aria-hidden="true"
              className={localOpen ? `${styles.chevron} ${styles.chevronOpen}` : styles.chevron}
            />
            Local
          </button>
          {localOpen && (
          <ul className={styles.folderBody}>
            {branches.map((branch) => (
              <ListRow key={branch.name}>
                <Toolbar>
                  <input
                    type="checkbox"
                    aria-label={`Show ${branch.name} in graph`}
                    checked={(graphBranchSelection ?? branches.map((b) => b.name)).includes(branch.name)}
                    onChange={() => toggleGraphBranch(branch.name)}
                  />
                  {renaming === branch.name ? (
                    <input
                      value={renameValue}
                      onChange={(event) => setRenameValue(event.target.value)}
                      onKeyDown={(event) => handleRenameKeyDown(event, branch.name)}
                    />
                  ) : (
                    <button
                      disabled={isRebasing}
                      onContextMenu={(event) => {
                        event.preventDefault();
                        setRowMenu({ kind: "local-branch", name: branch.name, x: event.clientX, y: event.clientY });
                      }}
                      onClick={() => {
                        if (!isRebasing) onSwitchBranch(branch.name);
                      }}
                    >
                      {branch.name}
                      {branch.isCurrent && " (current)"}
                    </button>
                  )}
                </Toolbar>
              </ListRow>
            ))}
          </ul>
          )}
        </li>
      </ul>

      {pendingForceFor !== null && (
        <ConfirmDialog
          ariaLabel={`Force delete ${pendingForceFor}`}
          message={
            <p>Force delete "{pendingForceFor}"? This discards any unmerged commits and cannot be undone.</p>
          }
          confirmLabel="Force Delete"
          confirmDisabled={isRebasing}
          onConfirm={() => {
            void onDeleteBranch(pendingForceFor, true);
            setPendingForceFor(null);
          }}
          onCancel={() => setPendingForceFor(null)}
        />
      )}

      {rowMenu !== null && rowMenu.kind === "local-branch" && (
        <ContextMenu
          x={rowMenu.x}
          y={rowMenu.y}
          onClose={() => setRowMenu(null)}
          items={branchContextItems(branches.find((b) => b.name === rowMenu.name)!)}
        />
      )}
      {rowMenu !== null && rowMenu.kind === "add" && (
        <ContextMenu
          x={rowMenu.x}
          y={rowMenu.y}
          onClose={() => setRowMenu(null)}
          items={[
            { label: "New Branch…", onSelect: () => onOpenCreateBranchDraft("HEAD") },
            { label: "Add Remote…", onSelect: onOpenAddRemoteDraft },
          ]}
        />
      )}
    </AccordionSection>
  );
}
```

```css
/* frontend/src/components/BranchTree.module.css */
.draftForm {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: var(--space-2);
  margin: var(--space-2) 0;
}

.tree {
  list-style: none;
  margin: 0;
  padding: 0;
}

.folder {
  margin-bottom: var(--space-1);
}

.folderHeader {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  width: 100%;
  padding: var(--space-1) var(--space-2);
  border: none;
  background: transparent;
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--color-text-muted);
  text-align: left;
  cursor: pointer;
}

.folderHeader:hover {
  background: var(--color-selected-bg);
}

.chevron {
  flex: 0 0 auto;
  transition: transform 120ms ease;
}

.chevronOpen {
  transform: rotate(90deg);
}

.folderBody {
  list-style: none;
  margin: 0;
  padding-left: var(--space-4);
}
```

Note: the right-click handler lives on the inner `<button>` (the actual clickable target), not on `ListRow` itself — `ListRow`'s own `onContextMenu` prop is left unset here, matching how the remote-branch rows in Task 8 attach their context-menu handler to their inner `<span>` rather than the row.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm vitest run src/components/BranchTree.test.tsx`
Expected: PASS

- [ ] **Step 5: Wire into `App.tsx`, replacing `BranchSwitcher`**

Replace the `<BranchSwitcher ... />` block (lines 159-175) with:

```tsx
            <BranchTree
              branches={appState.state.branches}
              createBranchDraft={appState.state.createBranchDraft}
              onSwitchBranch={appState.switchBranch}
              onCreateBranch={appState.createBranch}
              onDeleteBranch={appState.deleteBranch}
              onRenameBranch={appState.renameBranch}
              onOpenCreateBranchDraft={appState.openCreateBranchDraft}
              onCloseCreateBranchDraft={appState.closeCreateBranchDraft}
              onMergeBranch={appState.mergeBranch}
              isMerging={appState.state.mergeMessage !== null}
              isRebasing={appState.state.rebaseProgress !== null}
              operationDisabled={repositoryOperationDisabled}
              operationDisabledReason={operationDisabledReason}
              graphBranchSelection={appState.state.graphBranchSelection}
              onSetGraphBranchSelection={appState.setGraphBranchSelection}
              remotes={[]}
              upstream={null}
              remoteUpstreams={{}}
              onAddRemote={() => Promise.resolve(null)}
              onRenameRemote={() => Promise.resolve(false)}
              onUpdateRemoteUrls={() => Promise.resolve()}
              onRemoveRemote={() => Promise.resolve()}
              onSaveHttpsCredential={() => Promise.resolve()}
              onForgetHttpsCredential={() => Promise.resolve()}
              onSetRemoteAuthMode={() => Promise.resolve(false)}
              onSetUpstream={() => Promise.resolve()}
              onClearUpstream={() => Promise.resolve()}
              onListRemoteBranches={() => Promise.resolve([])}
              onFetchRemote={() => Promise.resolve()}
              onPushCurrentBranch={() => Promise.resolve()}
              onPull={() => Promise.resolve()}
              pendingPull={null}
              pullOutcome={null}
              onMergePull={() => Promise.resolve()}
              onRebasePull={() => {}}
              onCancelPull={() => {}}
              addRemoteDraftOpen={false}
              onOpenAddRemoteDraft={() => {}}
              onCloseAddRemoteDraft={() => {}}
            />
```

Remove the now-unused `BranchSwitcher` import. Leave `<RemotePanel ... />` (further down) exactly as it is — Task 8 replaces both the stub props above and the separate `RemotePanel` block in one pass. Leave the `BranchSwitcher.tsx`/`.module.css`/`.test.tsx` files in place for now — Task 8 deletes them once `BranchTree.test.tsx` has absorbed every case.

- [ ] **Step 6: Type-check and build**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm build`
Expected: no errors (the stub remote props above exist solely to satisfy the type checker until Task 8)

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/BranchTree.tsx frontend/src/components/BranchTree.module.css frontend/src/components/BranchTree.test.tsx frontend/src/App.tsx
git commit -m "feat(frontend): add BranchTree local-branches view, replacing the BranchSwitcher dropdown"
```

---

## Task 8: `BranchTree` — remote folders (absorbs `RemotePanel`)

Adds the remote side of the tree: one folder per remote, lazily-loaded remote branches, and every `RemotePanel` action via context menus, the add-remote inline form, and a small set-upstream dialog. Deletes `BranchSwitcher.tsx` and `RemotePanel.tsx` (and their CSS/tests) once `BranchTree.test.tsx` covers everything they did.

**Files:**
- Modify: `frontend/src/components/BranchTree.tsx` (add remote-folder rendering, dialogs, and the checkout/set-upstream/credentials logic)
- Modify: `frontend/src/components/BranchTree.module.css` (append remote-folder and dialog-form styles, reusing `RemotePanel.module.css`'s class bodies)
- Modify: `frontend/src/components/BranchTree.test.tsx` (append remote-side tests)
- Delete: `frontend/src/components/BranchSwitcher.tsx`, `frontend/src/components/BranchSwitcher.module.css`, `frontend/src/components/BranchSwitcher.test.tsx`
- Delete: `frontend/src/components/RemotePanel.tsx`, `frontend/src/components/RemotePanel.module.css`, `frontend/src/components/RemotePanel.test.tsx`
- Modify: `frontend/src/App.tsx` (remove `<RemotePanel ... />`, fill in `BranchTree`'s remote props for real, remove the `RemotePanel` import)

### Backend-capability notes locked in during planning (read before implementing)

The spec described "push to upstream remote" and "set upstream" as generic per-branch actions, but the actual `AppState` methods only ever act on the **current** branch (`pushCurrentBranch`, `setCurrentUpstream`/`onSetUpstream`, `pullCurrentUpstream`) — there is no backend call to push or set upstream for a branch that isn't checked out, and adding one is out of scope (Global Constraints: no backend changes). So:

- "Push to `<remote>`" and "Set upstream…" appear **only on the current branch's row's context menu**, never on other local branches.
- "Checkout" on a remote-branch row: if a local branch with that short name already exists, it just switches to it (`onSwitchBranch`); otherwise it creates one via `onCreateBranch(branchName, "<remote>/<branchName>")` (backend `create_branch` resolves any revspec as its `start_point`, confirmed in `crates/git-core/src/branch.rs`'s `resolve_start_point`) and then calls `onSetUpstream(remoteName, branchName)` so the new branch tracks it — composing two existing calls, no backend change.
- "Set as upstream for current branch" on a remote-branch row is a direct one-click `onSetUpstream(remote.name, branchName)` — no dialog needed (this replaces the old form-based upstream-branch datalist flow for the common case; the "Set upstream…" dialog on the current-branch row remains for typing an arbitrary remote-branch name that isn't in the lazily-loaded list yet).

### Behavior mapping (RemotePanel → BranchTree), preserved exactly except where noted above

| RemotePanel today | BranchTree |
|---|---|
| Flat remote list, each row always showing fetch/push URLs + icon-button toolbar | One folder per remote; folder body lazy-loads that remote's branches on first expand via `onListRemoteBranches` |
| Fetch/Push/Edit/Credentials/Remove icon buttons per remote row | Right-click the remote folder header → `ContextMenu` with the same five actions |
| Inline edit-remote form nested in the row | Modal `<dialog>` (same `showModal()` mechanics `ConfirmDialog` already uses), opened from the context menu |
| Inline credentials form nested in the row | Modal `<dialog>`, opened from the context menu |
| "Add remote" toolbar button + inline form | Inline form shown when `addRemoteDraftOpen` is true (opened via the shared "+" menu from Task 7, or the command palette from Task 6) |
| Remove-remote `ConfirmDialog` (with the clear-upstreams variant) | Unchanged, same component, same trigger flow, now opened from the context menu |
| "Upstream" section: status text, Pull button, set/clear-upstream form | Kept as a small section under the tree (not per-remote — it describes the *current branch's* upstream, same as today), with "Set upstream…" opening a dialog instead of an always-visible form |
| Pull-outcome `<dialog>` (merge/rebase/cancel) | Unchanged, verbatim |

- [ ] **Step 1: Write the failing tests** (append to `BranchTree.test.tsx`; add a second `describe` block)

```tsx
// frontend/src/components/BranchTree.test.tsx — append
describe("BranchTree — remotes", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  const oneRemote = [{ name: "origin", fetchUrl: "git@github.com:user/repo.git", pushUrl: null, authMode: null, authUsername: null }];

  it("lists remote folders and lazily loads their branches on first expand", async () => {
    const onListRemoteBranches = vi.fn().mockResolvedValue(["main", "feat/foo"]);
    renderTree({ remotes: oneRemote, onListRemoteBranches });
    expect(onListRemoteBranches).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    expect(onListRemoteBranches).toHaveBeenCalledWith("origin");
    expect(await screen.findByText("feat/foo")).toBeInTheDocument();
  });

  it("does not re-fetch remote branches on a second expand", async () => {
    const onListRemoteBranches = vi.fn().mockResolvedValue(["main"]);
    renderTree({ remotes: oneRemote, onListRemoteBranches });
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // expand
    await screen.findByText("main");
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // collapse
    fireEvent.click(screen.getByRole("button", { name: "origin" })); // expand again
    expect(onListRemoteBranches).toHaveBeenCalledOnce();
  });

  it("right-clicking a remote branch offers Checkout and Set as upstream", async () => {
    renderTree({ remotes: oneRemote, onListRemoteBranches: vi.fn().mockResolvedValue(["feat/foo"]) });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    fireEvent.contextMenu(await screen.findByText("feat/foo"));
    expect(screen.getByRole("menuitem", { name: "Checkout" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Set as upstream for current branch" })).toBeInTheDocument();
  });

  it("Checkout switches to an existing same-named local branch instead of creating a new one", async () => {
    const onSwitchBranch = vi.fn();
    const onCreateBranch = vi.fn();
    renderTree({
      branches: [...baseBranches],
      remotes: oneRemote,
      onListRemoteBranches: vi.fn().mockResolvedValue(["feat/foo"]),
      onSwitchBranch,
      onCreateBranch,
    });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    fireEvent.contextMenu(await screen.findByText("feat/foo"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Checkout" }));
    expect(onSwitchBranch).toHaveBeenCalledWith("feat/foo");
    expect(onCreateBranch).not.toHaveBeenCalled();
  });

  it("Checkout creates and tracks a new local branch when none exists locally", async () => {
    const onCreateBranch = vi.fn().mockResolvedValue(null);
    const onSetUpstream = vi.fn().mockResolvedValue(undefined);
    renderTree({
      branches: [{ name: "main", isCurrent: true }],
      remotes: oneRemote,
      onListRemoteBranches: vi.fn().mockResolvedValue(["feat/only-remote"]),
      onCreateBranch,
      onSetUpstream,
    });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    fireEvent.contextMenu(await screen.findByText("feat/only-remote"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Checkout" }));
    await Promise.resolve();
    expect(onCreateBranch).toHaveBeenCalledWith("feat/only-remote", "origin/feat/only-remote");
    expect(onSetUpstream).toHaveBeenCalledWith("origin", "feat/only-remote");
  });

  it("Set as upstream for current branch calls onSetUpstream directly, no dialog", async () => {
    const onSetUpstream = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onListRemoteBranches: vi.fn().mockResolvedValue(["main"]), onSetUpstream });
    fireEvent.click(screen.getByRole("button", { name: "origin" }));
    fireEvent.contextMenu(await screen.findByText("main"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Set as upstream for current branch" }));
    expect(onSetUpstream).toHaveBeenCalledWith("origin", "main");
  });

  it("right-clicking a remote folder offers Fetch/Push/Edit/Credentials/Remove", () => {
    renderTree({ remotes: oneRemote });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    expect(screen.getByRole("menuitem", { name: "Fetch" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Push current branch here" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Edit remote" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Manage credentials" })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: "Remove remote" })).toBeInTheDocument();
  });

  it("Fetch calls onFetchRemote with the remote's name", () => {
    const onFetchRemote = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onFetchRemote });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Fetch" }));
    expect(onFetchRemote).toHaveBeenCalledWith("origin");
  });

  it("Push current branch here calls onPushCurrentBranch with the remote's name", () => {
    const onPushCurrentBranch = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onPushCurrentBranch });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Push current branch here" }));
    expect(onPushCurrentBranch).toHaveBeenCalledWith("origin");
  });

  it("Edit remote opens a dialog prefilled with the remote's URLs; saving calls onUpdateRemoteUrls", async () => {
    const onUpdateRemoteUrls = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onUpdateRemoteUrls });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Edit remote" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit origin" });
    expect(within(dialog).getByDisplayValue("git@github.com:user/repo.git")).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Save remote" }));
    expect(onUpdateRemoteUrls).toHaveBeenCalledWith("origin", "git@github.com:user/repo.git", null);
  });

  it("Manage credentials opens a dialog; saving an HTTPS credential calls onSaveHttpsCredential", async () => {
    const onSetRemoteAuthMode = vi.fn().mockResolvedValue(true);
    const onSaveHttpsCredential = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onSetRemoteAuthMode, onSaveHttpsCredential });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Manage credentials" }));
    const dialog = await screen.findByRole("dialog", { name: "Credentials for origin" });
    fireEvent.change(within(dialog).getByLabelText("HTTPS username"), { target: { value: "me" } });
    fireEvent.change(within(dialog).getByLabelText("Access token"), { target: { value: "tok" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save HTTPS credential" }));
    expect(onSetRemoteAuthMode).toHaveBeenCalledWith("origin", "HttpsToken", "me");
    expect(onSaveHttpsCredential).toHaveBeenCalledWith("origin", "me", "tok");
  });

  it("Remove remote opens the existing confirmation flow and calls onRemoveRemote", async () => {
    const onRemoveRemote = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onRemoveRemote });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Remove remote" }));
    fireEvent.click(await screen.findByRole("button", { name: "Confirm remove" }));
    expect(onRemoveRemote).toHaveBeenCalledWith("origin", false);
  });

  it("offers the explicit clear-upstreams removal route for a remote that has upstreams", async () => {
    renderTree({
      remotes: oneRemote,
      remoteUpstreams: { origin: [{ localBranch: "main", remoteName: "origin", remoteBranch: "main" }] },
    });
    fireEvent.contextMenu(screen.getByRole("button", { name: "origin" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Remove remote" }));
    expect(await screen.findByText(/clear upstreams for main/)).toBeInTheDocument();
  });

  it("the header '+' menu's Add Remote opens the add-remote draft", () => {
    const onOpenAddRemoteDraft = vi.fn();
    renderTree({ onOpenAddRemoteDraft });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Add Remote…" }));
    expect(onOpenAddRemoteDraft).toHaveBeenCalledOnce();
  });

  it("addRemoteDraftOpen shows the add-remote form; submitting calls onAddRemote", async () => {
    const onAddRemote = vi.fn().mockResolvedValue(null);
    renderTree({ addRemoteDraftOpen: true, onAddRemote });
    fireEvent.change(screen.getByTestId("add-remote-fetch-url"), {
      target: { value: "git@github.com:user/repo.git" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add remote" }));
    expect(onAddRemote).toHaveBeenCalledWith("origin", "git@github.com:user/repo.git", null);
  });

  it("shows the current branch's upstream status and a Pull button", () => {
    renderTree({ upstream: { localBranch: "main", remoteName: "origin", remoteBranch: "main" } });
    expect(screen.getByText(/main tracks origin\/main/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Pull" })).toBeEnabled();
  });

  it("Set upstream… on the current branch's context menu opens a dialog; submitting calls onSetUpstream", async () => {
    const onSetUpstream = vi.fn().mockResolvedValue(undefined);
    renderTree({ remotes: oneRemote, onSetUpstream });
    fireEvent.contextMenu(screen.getByText(/main.*\(current\)/));
    fireEvent.click(screen.getByRole("menuitem", { name: "Set upstream…" }));
    const dialog = await screen.findByRole("dialog", { name: "Set upstream for main" });
    fireEvent.change(within(dialog).getByLabelText("Upstream remote"), { target: { value: "origin" } });
    fireEvent.change(within(dialog).getByLabelText("Upstream branch"), { target: { value: "main" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Set upstream" }));
    expect(onSetUpstream).toHaveBeenCalledWith("origin", "main");
  });

  it("does not offer Set upstream… or Push on a non-current branch's context menu", () => {
    renderTree({ remotes: oneRemote, upstream: { localBranch: "main", remoteName: "origin", remoteBranch: "main" } });
    fireEvent.contextMenu(screen.getByText("feat/foo"));
    expect(screen.queryByRole("menuitem", { name: "Set upstream…" })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /^Push to/ })).not.toBeInTheDocument();
  });

  it("offers Push to <remote> on the current branch's context menu when it has an upstream", () => {
    const onPushCurrentBranch = vi.fn().mockResolvedValue(undefined);
    renderTree({ upstream: { localBranch: "main", remoteName: "origin", remoteBranch: "main" }, onPushCurrentBranch });
    fireEvent.contextMenu(screen.getByText(/main.*\(current\)/));
    fireEvent.click(screen.getByRole("menuitem", { name: "Push to origin" }));
    expect(onPushCurrentBranch).toHaveBeenCalledWith("origin");
  });

  it("offers merge or rebase only after a divergent pull, unchanged from RemotePanel", () => {
    renderTree({ pendingPull: { upstreamRef: "origin/main" } });
    expect(screen.getByRole("dialog", { name: "Pull has diverged" })).toBeInTheDocument();
  });
});
```

Port every remaining `RemotePanel.test.tsx` case not covered above (clipboard-copy of fetch/push URLs, SSH-agent credential mode, credential-save-failure clearing the token, add-remote name auto-derivation from the URL slug and its edit-tracking behavior, add-remote validation/failure-message display, disabled-button `title` explanations, remove-button danger styling, icon-only row actions, the up-to-date pull-outcome message, `pendingPull` focus/disabled behavior) using the same transformation: wherever the old test opened an always-visible inline form or clicked an icon button in the row, the new test right-clicks the remote folder header (or the remote-branch row, for anything upstream-related) and clicks the equivalent `menuitem`, then asserts against the dialog/form that appears — the assertions on the resulting call (`onAddRemote`, `onSaveHttpsCredential`, etc.) are unchanged.

Add `within` to the `@testing-library/react` import at the top of `BranchTree.test.tsx`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm vitest run src/components/BranchTree.test.tsx`
Expected: FAIL — remote folders, dialogs, and checkout/upstream logic don't exist yet

- [ ] **Step 3: Implement the remote side of `BranchTree.tsx`**

Extend the component with remote-folder state and rendering. Add these imports:

```tsx
import { useEffect, useRef, useState, type KeyboardEvent, type RefObject } from "react";
import { Cloud, ChevronRight, GitBranch, KeyRound, Pencil, Plus, Trash2 } from "lucide-react";
```

Add state (alongside the existing local-branch state):

```tsx
  const [openRemotes, setOpenRemotes] = useState<Record<string, boolean>>({});
  const [remoteBranches, setRemoteBranches] = useState<Record<string, string[]>>({});
  const [removeConfirmation, setRemoveConfirmation] = useState<string | null>(null);
  const [editingRemote, setEditingRemote] = useState<RemoteInfo | null>(null);
  const [editName, setEditName] = useState("");
  const [editFetchUrl, setEditFetchUrl] = useState("");
  const [editPushUrl, setEditPushUrl] = useState("");
  const [credentialRemote, setCredentialRemote] = useState<string | null>(null);
  const [credentialMode, setCredentialMode] = useState<RemoteAuthMode>("HttpsToken");
  const [credentialUsername, setCredentialUsername] = useState("");
  const accessTokenRef = useRef<HTMLInputElement>(null);
  const editDialogRef = useRef<HTMLDialogElement>(null);
  const credentialDialogRef = useRef<HTMLDialogElement>(null);
  const upstreamFormDialogRef = useRef<HTMLDialogElement>(null);
  const [upstreamDialogOpen, setUpstreamDialogOpen] = useState(false);
  const [upstreamRemoteField, setUpstreamRemoteField] = useState("");
  const [upstreamBranchField, setUpstreamBranchField] = useState("");
  const [remoteBranchOptions, setRemoteBranchOptions] = useState<string[]>([]);
  const [newRemoteName, setNewRemoteName] = useState("");
  const [newFetchUrl, setNewFetchUrl] = useState("");
  const [newPushUrl, setNewPushUrl] = useState("");
  const [nameTouched, setNameTouched] = useState(false);
  const [showPushUrl, setShowPushUrl] = useState(false);
  const [addError, setAddError] = useState<string | null>(null);
  const [checkoutError, setCheckoutError] = useState<string | null>(null);
  const pullDialogRef = useRef<HTMLDialogElement>(null);
```

Add the `deriveRemoteName` helper verbatim (moved from `RemotePanel.tsx`):

```tsx
function deriveRemoteName(fetchUrl: string, existingNames: string[]): string {
  if (!existingNames.includes("origin")) return "origin";
  const withoutGitSuffix = fetchUrl.replace(/\.git\/?$/, "");
  const slug = withoutGitSuffix.split(/[/:]/).filter((part) => part !== "").pop();
  return slug ?? "";
}
```
(module scope, same as it was in `RemotePanel.tsx`)

Add the pull-dialog `useEffect` verbatim (moved from `RemotePanel.tsx`'s existing one, same `pendingPull`/`pullDialogRef` mechanics):

```tsx
  useEffect(() => {
    const dialog = pullDialogRef.current;
    if (pendingPull === null || dialog === null) return;
    if (!dialog.open && typeof dialog.showModal === "function") {
      dialog.showModal();
    } else if (!dialog.open) {
      dialog.setAttribute("open", "");
    }
    dialog.querySelector<HTMLButtonElement>("[data-autofocus]")?.focus();
  }, [pendingPull]);
```

Add a small shared opener (same `showModal()`/`open`-attribute fallback as the effect above, factored out since the three dialogs below all need it) plus one effect per dialog:

```tsx
  function openNativeDialog(ref: RefObject<HTMLDialogElement>): void {
    const dialog = ref.current;
    if (dialog === null || dialog.open) return;
    if (typeof dialog.showModal === "function") {
      dialog.showModal();
    } else {
      dialog.setAttribute("open", "");
    }
  }

  useEffect(() => {
    if (editingRemote !== null) openNativeDialog(editDialogRef);
  }, [editingRemote]);
  useEffect(() => {
    if (credentialRemote !== null) openNativeDialog(credentialDialogRef);
  }, [credentialRemote]);
  useEffect(() => {
    if (upstreamDialogOpen) openNativeDialog(upstreamFormDialogRef);
  }, [upstreamDialogOpen]);
```

Add remote-folder toggle + lazy load. Each remote's expanded/collapsed state persists under its own
`localStorage` key via the Task 2 helper, exactly like `AccordionSection`'s own sections — but since
the set of remotes is dynamic (unlike a section's fixed `storageKey`), the persisted value is read as
a fallback wherever `openRemotes` hasn't been touched yet this session, rather than via a `useState`
lazy initializer:

```tsx
  function remoteFolderKey(remoteName: string): string {
    return `branchtree.remote.${remoteName}`;
  }

  function isRemoteOpen(remoteName: string): boolean {
    return openRemotes[remoteName] ?? loadPersistedOpen(remoteFolderKey(remoteName), false);
  }

  const toggleRemote = (remoteName: string) => {
    const willOpen = !isRemoteOpen(remoteName);
    setOpenRemotes((prev) => ({ ...prev, [remoteName]: willOpen }));
    persistOpen(remoteFolderKey(remoteName), willOpen);
    if (willOpen && remoteBranches[remoteName] === undefined) {
      void onListRemoteBranches(remoteName).then((names) =>
        setRemoteBranches((prev) => ({ ...prev, [remoteName]: names })),
      );
    }
  };
```

Add checkout logic:

```tsx
  const checkoutRemoteBranch = async (remoteName: string, branchName: string) => {
    const existingLocal = branches.find((b) => b.name === branchName);
    if (existingLocal !== undefined) {
      onSwitchBranch(branchName);
      return;
    }
    const failure = await onCreateBranch(branchName, `${remoteName}/${branchName}`);
    if (failure !== null) {
      setCheckoutError(failure);
      return;
    }
    await onSetUpstream(remoteName, branchName);
  };
```

Add remote-folder and remote-branch context-menu item builders:

```tsx
  function remoteFolderItems(remote: RemoteInfo): ContextMenuItem[] {
    return [
      { label: "Fetch", disabled: operationDisabled, onSelect: () => void onFetchRemote(remote.name) },
      {
        label: "Push current branch here",
        disabled: operationDisabled,
        onSelect: () => void onPushCurrentBranch(remote.name),
      },
      {
        label: "Edit remote",
        onSelect: () => {
          setEditingRemote(remote);
          setEditName(remote.name);
          setEditFetchUrl(remote.fetchUrl);
          setEditPushUrl(remote.pushUrl ?? "");
        },
      },
      {
        label: "Manage credentials",
        onSelect: () => {
          setCredentialRemote(remote.name);
          setCredentialMode(remote.authMode ?? "HttpsToken");
          setCredentialUsername(remote.authUsername ?? "");
        },
      },
      {
        label: "Remove remote",
        destructive: true,
        disabled: operationDisabled,
        onSelect: () =>
          setRemoveConfirmation(
            remoteUpstreams[remote.name].length > 0 ? `clear:${remote.name}` : remote.name,
          ),
      },
    ];
  }

  function remoteBranchItems(remoteName: string, branchName: string): ContextMenuItem[] {
    return [
      { label: "Checkout", onSelect: () => void checkoutRemoteBranch(remoteName, branchName) },
      {
        label: "Set as upstream for current branch",
        onSelect: () => void onSetUpstream(remoteName, branchName),
      },
    ];
  }
```

Extend `branchContextItems` (from Task 7) to add the current-branch-only items, and thread the current branch's `upstream`-derived state into it:

```tsx
  function branchContextItems(branch: BranchInfo): ContextMenuItem[] {
    const items: ContextMenuItem[] = [
      {
        label: "Rename",
        disabled: isRebasing,
        onSelect: () => {
          setRenaming(branch.name);
          setRenameValue(branch.name);
        },
      },
    ];
    if (!branch.isCurrent) {
      items.push({
        label: "Merge into current branch",
        disabled: isMerging || isRebasing || operationDisabled,
        onSelect: () => onMergeBranch(branch.name),
      });
    }
    if (branch.isCurrent) {
      if (upstream !== null) {
        items.push({
          label: `Push to ${upstream.remoteName}`,
          disabled: operationDisabled,
          onSelect: () => void onPushCurrentBranch(upstream.remoteName),
        });
      }
      items.push({
        label: "Set upstream…",
        onSelect: () => {
          setUpstreamRemoteField("");
          setUpstreamBranchField("");
          setUpstreamDialogOpen(true);
        },
      });
    }
    items.push({
      label: "Delete",
      disabled: isRebasing,
      destructive: true,
      onSelect: () => void handleDeleteClick(branch.name),
    });
    return items;
  }
```

Add the add-remote submit handler (moved from `RemotePanel.tsx`'s `submitAdd`, same logic):

```tsx
  const submitAddRemote = async () => {
    setAddError(null);
    const fetchUrl = newFetchUrl.trim();
    const name = (newRemoteName.trim() || deriveRemoteName(fetchUrl, remotes.map((r) => r.name))).trim();
    if (name === "" || fetchUrl === "") return;
    const failure = await onAddRemote(name, fetchUrl, newPushUrl.trim() || null);
    if (failure !== null) {
      setAddError(failure);
      return;
    }
    setNewRemoteName("");
    setNewFetchUrl("");
    setNewPushUrl("");
    setNameTouched(false);
    setShowPushUrl(false);
  };
```

Render, inside the `<AccordionSection>`, after the existing `<ul className={styles.tree}>` Local `<li>` (from Task 7) and before its closing `</ul>`, one `<li>` per remote:

```tsx
        {remotes.map((remote) => (
          <li key={remote.name} className={styles.folder}>
            <button
              type="button"
              className={styles.folderHeader}
              aria-expanded={isRemoteOpen(remote.name)}
              onClick={() => toggleRemote(remote.name)}
              onContextMenu={(event) => {
                event.preventDefault();
                setRowMenu({ kind: "remote-folder", name: remote.name, x: event.clientX, y: event.clientY });
              }}
            >
              <ChevronRight
                size={14}
                aria-hidden="true"
                className={isRemoteOpen(remote.name) ? `${styles.chevron} ${styles.chevronOpen}` : styles.chevron}
              />
              <Cloud size={14} aria-hidden="true" />
              {remote.name}
            </button>
            {isRemoteOpen(remote.name) && (
              <ul className={styles.folderBody}>
                {(remoteBranches[remote.name] ?? []).map((branchName) => (
                  <ListRow key={branchName}>
                    <span
                      onContextMenu={(event) => {
                        event.preventDefault();
                        setRowMenu({
                          kind: "remote-branch",
                          remoteName: remote.name,
                          branchName,
                          x: event.clientX,
                          y: event.clientY,
                        });
                      }}
                    >
                      {branchName}
                    </span>
                  </ListRow>
                ))}
              </ul>
            )}
          </li>
        ))}
```

Extend the `RowContextMenu` union (Task 7 declared it above the component) to:

```tsx
type RowContextMenu =
  | { kind: "local-branch"; name: string; x: number; y: number }
  | { kind: "add"; x: number; y: number }
  | { kind: "remote-folder"; name: string; x: number; y: number }
  | { kind: "remote-branch"; remoteName: string; branchName: string; x: number; y: number };
```

Extend the "+" menu's items array (Task 7's `{ kind: "add" }` branch) to include Add Remote:

```tsx
          items={[
            { label: "New Branch…", onSelect: () => onOpenCreateBranchDraft("HEAD") },
            { label: "Add Remote…", onSelect: onOpenAddRemoteDraft },
          ]}
```

(This is the same array Task 7 already wrote — Task 7's version already includes both entries; no further change needed here if Task 7 was implemented as specified above.)

Add the two new `ContextMenu` render branches alongside the existing ones:

```tsx
      {rowMenu !== null && rowMenu.kind === "remote-folder" && (
        <ContextMenu
          x={rowMenu.x}
          y={rowMenu.y}
          onClose={() => setRowMenu(null)}
          items={remoteFolderItems(remotes.find((r) => r.name === rowMenu.name)!)}
        />
      )}
      {rowMenu !== null && rowMenu.kind === "remote-branch" && (
        <ContextMenu
          x={rowMenu.x}
          y={rowMenu.y}
          onClose={() => setRowMenu(null)}
          items={remoteBranchItems(rowMenu.remoteName, rowMenu.branchName)}
        />
      )}
```

Add the add-remote inline form (rendered when `addRemoteDraftOpen`), moved from `RemotePanel.tsx`'s existing form markup, right after the local-branch create-branch draft form:

```tsx
      {addRemoteDraftOpen && (
        <form
          className={styles.form}
          aria-label="Add remote"
          onSubmit={(event) => {
            event.preventDefault();
            void submitAddRemote();
          }}
        >
          <label className={styles.label}>
            Remote name
            <input
              placeholder="origin"
              value={newRemoteName}
              onChange={(event) => {
                setNameTouched(true);
                setNewRemoteName(event.target.value);
              }}
            />
          </label>
          <label className={styles.label}>
            Fetch URL
            <input
              data-testid="add-remote-fetch-url"
              placeholder="git@github.com:user/repo.git"
              value={newFetchUrl}
              onChange={(event) => {
                const value = event.target.value;
                setNewFetchUrl(value);
                setAddError(null);
                if (!nameTouched) {
                  setNewRemoteName(deriveRemoteName(value, remotes.map((r) => r.name)));
                }
              }}
            />
          </label>
          {addError !== null && <InlineError message={addError} onDismiss={() => setAddError(null)} />}
          <details
            open={showPushUrl}
            onToggle={(event) => setShowPushUrl(event.currentTarget.open)}
          >
            <summary
              onClick={(event) => {
                event.preventDefault();
                setShowPushUrl((open) => !open);
              }}
            >
              Push URL (optional)
            </summary>
            {showPushUrl && (
              <label className={styles.label}>
                Push URL
                <input
                  placeholder="git@github.com:user/repo.git"
                  value={newPushUrl}
                  onChange={(event) => setNewPushUrl(event.target.value)}
                />
              </label>
            )}
          </details>
          <button type="submit" disabled={operationDisabled} title={operationDisabled ? (operationDisabledReason ?? undefined) : undefined}>
            Add remote
          </button>
          <button type="button" onClick={onCloseAddRemoteDraft}>
            Cancel
          </button>
        </form>
      )}
```

Add the edit-remote dialog, credentials dialog, upstream section + set-upstream dialog, remove-remote `ConfirmDialog`, checkout-error `InlineError`, and the unchanged pull-outcome dialog, all after the tree `<ul>` and before the closing `</AccordionSection>`:

```tsx
      {checkoutError !== null && <InlineError message={checkoutError} onDismiss={() => setCheckoutError(null)} />}

      {removeConfirmation !== null && (
        <ConfirmDialog
          ariaLabel="Remove remote confirmation"
          message={
            removeConfirmation.startsWith("clear:") ? (
              <p>
                Remove {removeConfirmation.slice(6)} and clear upstreams for{" "}
                {remoteUpstreams[removeConfirmation.slice(6)].map((item) => item.localBranch).join(", ")}?
              </p>
            ) : (
              <p>Remove remote {removeConfirmation}?</p>
            )
          }
          confirmLabel="Confirm remove"
          confirmDisabled={operationDisabled}
          onConfirm={() => {
            const target = removeConfirmation.startsWith("clear:") ? removeConfirmation.slice(6) : removeConfirmation;
            const clearUpstreams = removeConfirmation.startsWith("clear:");
            void onRemoveRemote(target, clearUpstreams).then(() => setRemoveConfirmation(null));
          }}
          onCancel={() => setRemoveConfirmation(null)}
        />
      )}

      {editingRemote !== null && (
        <dialog
          ref={editDialogRef}
          aria-label={`Edit ${editingRemote.name}`}
          onCancel={(event) => {
            event.preventDefault();
            setEditingRemote(null);
          }}
        >
          <form
            onSubmit={async (event) => {
              event.preventDefault();
              const name = editName.trim();
              const fetchUrl = editFetchUrl.trim();
              if (name === "" || fetchUrl === "") return;
              if (name !== editingRemote.name && !(await onRenameRemote(editingRemote.name, name))) return;
              await onUpdateRemoteUrls(name, fetchUrl, editPushUrl.trim() || null);
              setEditingRemote(null);
            }}
          >
            <label className={styles.label}>
              Remote name
              <input value={editName} onChange={(event) => setEditName(event.target.value)} />
            </label>
            <label className={styles.label}>
              Fetch URL
              <input value={editFetchUrl} onChange={(event) => setEditFetchUrl(event.target.value)} />
            </label>
            <label className={styles.label}>
              Push URL
              <input value={editPushUrl} onChange={(event) => setEditPushUrl(event.target.value)} />
            </label>
            <button type="submit">Save remote</button>
            <button type="button" onClick={() => setEditingRemote(null)}>
              Cancel
            </button>
          </form>
        </dialog>
      )}

      {credentialRemote !== null && (
        <dialog
          ref={credentialDialogRef}
          aria-label={`Credentials for ${credentialRemote}`}
          onCancel={(event) => {
            event.preventDefault();
            setCredentialRemote(null);
          }}
        >
          <form
            onSubmit={async (event) => {
              event.preventDefault();
              const username = credentialUsername.trim();
              const token = accessTokenRef.current?.value ?? "";
              if (credentialMode === "SshAgent") {
                await onSetRemoteAuthMode(credentialRemote, "SshAgent", null);
              } else if (username !== "" && token !== "") {
                const configured = await onSetRemoteAuthMode(credentialRemote, "HttpsToken", username);
                if (configured) await onSaveHttpsCredential(credentialRemote, username, token);
              }
              if (accessTokenRef.current !== null) accessTokenRef.current.value = "";
              setCredentialRemote(null);
            }}
          >
            <label className={styles.label}>
              Authentication for {credentialRemote}
              <select value={credentialMode} onChange={(event) => setCredentialMode(event.target.value as RemoteAuthMode)}>
                <option value="HttpsToken">HTTPS token</option>
                <option value="SshAgent">SSH agent</option>
              </select>
            </label>
            {credentialMode === "HttpsToken" ? (
              <>
                <label className={styles.label}>
                  HTTPS username
                  <input value={credentialUsername} onChange={(event) => setCredentialUsername(event.target.value)} autoComplete="off" />
                </label>
                <label className={styles.label}>
                  Access token
                  <input ref={accessTokenRef} type="password" autoComplete="off" />
                </label>
                <button type="submit">Save HTTPS credential</button>
                <button type="button" onClick={() => void onForgetHttpsCredential(credentialRemote)}>
                  Forget HTTPS credential
                </button>
              </>
            ) : (
              <button type="submit">Use SSH agent</button>
            )}
            <button type="button" onClick={() => setCredentialRemote(null)}>
              Cancel credentials
            </button>
          </form>
        </dialog>
      )}

      <section>
        <h3>Upstream</h3>
        {upstream === null ? <p>No upstream for the current branch.</p> : <p>{upstream.localBranch} tracks {upstream.remoteName}/{upstream.remoteBranch}.</p>}
        <button
          type="button"
          disabled={operationDisabled || upstream === null || pendingPull !== null}
          title={operationDisabled ? (operationDisabledReason ?? undefined) : undefined}
          onClick={() => void onPull()}
        >
          Pull
        </button>
        {pullOutcome?.kind === "UpToDate" && <p role="status">Already up to date.</p>}
        {upstream !== null && (
          <button type="button" onClick={() => void onClearUpstream()}>
            Clear upstream
          </button>
        )}
      </section>

      {upstreamDialogOpen && (
        <dialog
          ref={upstreamFormDialogRef}
          aria-label={`Set upstream for ${branches.find((b) => b.isCurrent)?.name ?? ""}`}
          onCancel={(event) => {
            event.preventDefault();
            setUpstreamDialogOpen(false);
          }}
        >
          <form
            onSubmit={async (event) => {
              event.preventDefault();
              const branch = upstreamBranchField.trim();
              if (upstreamRemoteField === "" || branch === "") return;
              await onSetUpstream(upstreamRemoteField, branch);
              setUpstreamDialogOpen(false);
            }}
          >
            <label className={styles.label}>
              Upstream remote
              <select
                value={upstreamRemoteField}
                onChange={(event) => {
                  const remoteName = event.target.value;
                  setUpstreamRemoteField(remoteName);
                  setRemoteBranchOptions([]);
                  if (remoteName !== "") {
                    void onListRemoteBranches(remoteName).then(setRemoteBranchOptions).catch(() => setRemoteBranchOptions([]));
                  }
                }}
              >
                <option value="">Choose a remote</option>
                {remotes.map((remote) => (
                  <option key={remote.name} value={remote.name}>
                    {remote.name}
                  </option>
                ))}
              </select>
            </label>
            <label className={styles.label}>
              Upstream branch
              <input
                list="upstream-branch-options"
                value={upstreamBranchField}
                onChange={(event) => setUpstreamBranchField(event.target.value)}
              />
            </label>
            <datalist id="upstream-branch-options">
              {remoteBranchOptions.map((name) => (
                <option key={name} value={name} />
              ))}
            </datalist>
            <button type="submit">Set upstream</button>
            <button type="button" onClick={() => setUpstreamDialogOpen(false)}>
              Cancel
            </button>
          </form>
        </dialog>
      )}

      {pendingPull !== null && (
        <dialog
          ref={pullDialogRef}
          aria-label="Pull has diverged"
          onCancel={(event) => {
            event.preventDefault();
            onCancelPull();
          }}
        >
          <p>The pull has diverged from {pendingPull.upstreamRef}.</p>
          <button type="button" disabled={operationDisabled} onClick={() => void onMergePull(pendingPull.upstreamRef)}>
            Merge
          </button>
          <button type="button" disabled={operationDisabled} onClick={() => onRebasePull(pendingPull.upstreamRef)}>
            Rebase
          </button>
          <button type="button" data-autofocus onClick={onCancelPull}>
            Cancel
          </button>
        </dialog>
      )}
```

Add a `.form`/`.label` pair to `BranchTree.module.css` (copied from `RemotePanel.module.css`'s equivalents):

```css
/* frontend/src/components/BranchTree.module.css — append */
.form {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: var(--space-2);
  margin: var(--space-2) 0;
}

.label {
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: var(--text-sm);
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm vitest run src/components/BranchTree.test.tsx`
Expected: PASS (all cases, local and remote)

- [ ] **Step 5: Delete the retired components**

```bash
git rm frontend/src/components/BranchSwitcher.tsx frontend/src/components/BranchSwitcher.module.css frontend/src/components/BranchSwitcher.test.tsx
git rm frontend/src/components/RemotePanel.tsx frontend/src/components/RemotePanel.module.css frontend/src/components/RemotePanel.test.tsx
```

- [ ] **Step 6: Wire `App.tsx`'s real remote props into `BranchTree`, remove `RemotePanel`**

Replace the stub remote props from Task 7's `<BranchTree ... />` call with the real values (matching exactly what `RemotePanel` received in the old `App.tsx`):

```tsx
              remotes={appState.state.remotes}
              upstream={appState.state.upstream}
              remoteUpstreams={appState.state.remoteUpstreams}
              onAddRemote={appState.addRemote}
              onRenameRemote={appState.renameRemote}
              onUpdateRemoteUrls={appState.updateRemoteUrls}
              onRemoveRemote={appState.removeRemote}
              onSaveHttpsCredential={appState.saveHttpsCredential}
              onForgetHttpsCredential={appState.forgetHttpsCredential}
              onSetRemoteAuthMode={appState.setRemoteAuthMode}
              onSetUpstream={appState.setCurrentUpstream}
              onClearUpstream={appState.clearCurrentUpstream}
              onListRemoteBranches={appState.listRemoteBranches}
              onFetchRemote={appState.fetchRemote}
              onPushCurrentBranch={appState.pushCurrentBranch}
              onPull={appState.pullCurrentUpstream}
              pendingPull={appState.state.pendingPull}
              pullOutcome={appState.state.pullOutcome}
              onMergePull={async (upstreamRef) => {
                appState.clearPendingPull();
                await appState.mergeBranch(upstreamRef);
              }}
              onRebasePull={(upstreamRef) => {
                appState.clearPendingPull();
                appState.openRebasePlanner(upstreamRef);
              }}
              onCancelPull={appState.clearPendingPull}
              addRemoteDraftOpen={appState.state.addRemoteDraftOpen}
              onOpenAddRemoteDraft={appState.openAddRemoteDraft}
              onCloseAddRemoteDraft={appState.closeAddRemoteDraft}
```

Delete the entire `<RemotePanel ... />` block further down in `App.tsx` and its now-unused import.

- [ ] **Step 7: Type-check, build, and run the full frontend test suite**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm vitest run -- --run && pnpm build`
Expected: no type errors; full suite green; production build succeeds

- [ ] **Step 8: Commit**

```bash
git add -A frontend/src/components/BranchTree.tsx frontend/src/components/BranchTree.module.css frontend/src/components/BranchTree.test.tsx frontend/src/App.tsx
git commit -m "feat(frontend): fold RemotePanel into BranchTree's remote folders, retire BranchSwitcher/RemotePanel"
```

---

## Task 9: Panel visibility wiring in `App.tsx`

**Files:**
- Modify: `frontend/src/App.tsx` (import and call `useSidebarPanelVisibility`, pass `panelToggles` to `<Sidebar>`, conditionally render Stash/Worktree/Submodule/Reflog/Tag/PullRequest panels)

**Interfaces:**
- Consumes: `useSidebarPanelVisibility`, `SIDEBAR_PANEL_IDS` (Task 3); `Sidebar`'s `panelToggles` prop (Task 4).

No dedicated test file — this repo has no `App.test.tsx` today (thin composition-root wiring is verified by type-checking, the existing component-level tests for each panel, and the e2e suite; see `CLAUDE.md`'s testing-conventions note that thin pass-through code doesn't get its own test).

- [ ] **Step 1: Add the hook call inside `RepoWorkspace`**

Near the top of `RepoWorkspace`, alongside `const appState = useAppState(...)`:

```tsx
  const panelVisibility = useSidebarPanelVisibility();
```

Add the import:

```tsx
import { useSidebarPanelVisibility } from "./state/useSidebarPanelVisibility";
```

- [ ] **Step 2: Build the `panelToggles` array and pass it to `<Sidebar>`**

```tsx
          left={
            <Sidebar
              panelToggles={[
                { id: "stash", label: "Stashes", visible: panelVisibility.visibility.stash, onToggle: (v) => panelVisibility.setPanelVisible("stash", v) },
                { id: "worktree", label: "Worktrees", visible: panelVisibility.visibility.worktree, onToggle: (v) => panelVisibility.setPanelVisible("worktree", v) },
                { id: "submodule", label: "Submodules", visible: panelVisibility.visibility.submodule, onToggle: (v) => panelVisibility.setPanelVisible("submodule", v) },
                { id: "reflog", label: "Reflog", visible: panelVisibility.visibility.reflog, onToggle: (v) => panelVisibility.setPanelVisible("reflog", v) },
                { id: "tags", label: "Tags", visible: panelVisibility.visibility.tags, onToggle: (v) => panelVisibility.setPanelVisible("tags", v) },
                { id: "pullRequests", label: "Pull Requests", visible: panelVisibility.visibility.pullRequests, onToggle: (v) => panelVisibility.setPanelVisible("pullRequests", v) },
              ]}
            >
              <BranchTree ... /* unchanged from Task 8 */ />
```

- [ ] **Step 3: Conditionally render each toggleable panel**

Wrap each of the six panels (leave `BranchTree` always rendered):

```tsx
              {panelVisibility.visibility.stash && (
                <StashPanel
                  stashes={appState.state.stashes}
                  onSelectRow={appState.selectRow}
                  onApplyStash={appState.applyStash}
                  onDropStash={appState.dropStash}
                  operationDisabled={repositoryOperationDisabled}
                  operationDisabledReason={operationDisabledReason}
                />
              )}
              {panelVisibility.visibility.worktree && (
                <WorktreePanel
                  worktrees={appState.state.worktrees}
                  branches={appState.state.branches}
                  onOpenWorktree={onOpenRepoTab}
                  onCreateWorktree={appState.createWorktree}
                  onRemoveWorktree={appState.removeWorktree}
                  onPruneWorktrees={appState.pruneWorktrees}
                  operationDisabled={repositoryOperationDisabled}
                  operationDisabledReason={operationDisabledReason}
                />
              )}
              {panelVisibility.visibility.submodule && (
                <SubmodulePanel
                  submodules={appState.state.submodules}
                  onInit={appState.initSubmodule}
                  onUpdate={appState.updateSubmodule}
                  operationDisabled={repositoryOperationDisabled}
                />
              )}
              {panelVisibility.visibility.reflog && (
                <ReflogPanel
                  references={appState.state.reflogRefs}
                  selectedReference={appState.state.selectedReflogReference}
                  entries={appState.state.reflog}
                  onSelectReference={appState.selectReflogReference}
                  onRestore={appState.restoreReflogEntry}
                  operationDisabled={repositoryOperationDisabled}
                />
              )}
              {panelVisibility.visibility.tags && (
                <TagPanel
                  tags={appState.state.tags}
                  remotes={appState.state.remotes}
                  onCreate={appState.createTag}
                  onDelete={appState.deleteTag}
                  onPush={appState.pushTags}
                  pushDisabled={repositoryOperationDisabled}
                  operationDisabledReason={operationDisabledReason}
                />
              )}
              {panelVisibility.visibility.pullRequests && (
                <PullRequestPanel
                  forgeRepositories={appState.state.forgeRepositories}
                  pullRequests={appState.state.pullRequests}
                  onListPullRequests={appState.listPullRequests}
                  onSaveForgeToken={appState.saveForgeToken}
                  onForgetForgeToken={appState.forgetForgeToken}
                  onCreatePullRequest={appState.createPullRequest}
                  onOpenExternalUrl={appState.openExternalUrl}
                  operationDisabled={repositoryOperationDisabled}
                  operationDisabledReason={operationDisabledReason}
                />
              )}
            </Sidebar>
          }
```

Every prop on every panel stays byte-for-byte identical to what it already was in `App.tsx` — only the wrapping `{condition && (...)}` is new.

- [ ] **Step 4: Type-check and build**

Run: `cd frontend && pnpm exec tsc --noEmit && pnpm build`
Expected: no errors

- [ ] **Step 5: Manual smoke check**

Run: `cargo tauri dev` (from `crates/tauri-app`, per `CLAUDE.md`'s Commands section), open a repo, click the new gear icon in the sidebar toolbar, uncheck a couple of panels, confirm they disappear, re-check them, confirm they come back and their state (e.g. an open stash list) is fresh (not preserved — expected, per the spec's "Out of scope" note that hidden panels fully unmount). Right-click a local branch, a remote folder, and a remote branch in the new Branches tree; confirm every menu item from Task 7/8's mapping tables works end-to-end against a real repo.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/App.tsx
git commit -m "feat(frontend): wire sidebar panel visibility toggles into App"
```

---

## Task 10: Full verification pass

**Files:** none (verification only)

- [ ] **Step 1: Full frontend test suite**

Run: `cd frontend && pnpm test -- --run`
Expected: all tests pass

- [ ] **Step 2: Lint**

Run: `cd frontend && pnpm lint`
Expected: no violations (in particular, no `@tauri-apps/*` import outside `tauriRepoClient.ts` — `BranchTree.tsx` must not import it, matching the `RepoClient` isolation rule)

- [ ] **Step 3: Frontend build**

Run: `cd frontend && pnpm build`
Expected: succeeds

- [ ] **Step 4: Rust workspace (unaffected, but confirms nothing broke by proxy)**

Run: `cargo build --workspace && cargo test --workspace`
Expected: succeeds unchanged — no Rust files were touched by this plan

- [ ] **Step 5: E2E suite**

Run the sequence from `CLAUDE.md`'s "Commands" section (`VITE_E2E_REPO_PATH=... pnpm build`, `cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override`, then `cd e2e && pnpm test`). If any spec references `BranchSwitcher`/`RemotePanel` markup or copy directly (e.g. by role name or text that changed), update that spec to match `BranchTree`'s new structure — check `e2e/specs/*.spec.ts` for any such reference before running.

Expected: all specs pass.

- [ ] **Step 6: Update `CHANGELOG.md`**

Per the repo's pre-push hook (`scripts/check-changelog.py`), any push touching `frontend/src/` needs a matching entry. Add one under the relevant unreleased section describing the sidebar panel toggles and the unified Branches/Remotes tree.

- [ ] **Step 7: Commit**

```bash
git add CHANGELOG.md
git commit -m "docs: update changelog for sidebar panel toggles and branch tree"
```
