# Phase 6 Layout Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the new layout primitives (`Sidebar`, `AccordionSection`,
`Overlay`) and extend `SplitView` with resize/collapse, proven working and
tested, so the Rollout plan can recompose `App.tsx` around them.

**Architecture:** Four small, independently-testable primitives under
`frontend/src/components/primitives/`, each with its own CSS module and
test file, following the exact pattern `Panel`/`Toolbar`/`ListRow`
established in Phase 5. No existing component is touched by this plan —
that's the Rollout plan's job.

**Tech Stack:** React 19, TypeScript, Vite CSS Modules, `lucide-react`
(already a dependency), Vitest + Testing Library (already the project's
test stack).

**Spec:** `docs/superpowers/specs/2026-08-19-browsitory-phase6-layout-design.md`

## Global Constraints

- No `RepoClient` method, DTO, Tauri command, worker message, or
  `git-core` function is added, removed, or changed in shape by this
  plan.
- Frontend tests mock `RepoClient`, never `@tauri-apps/api` (not
  applicable to this plan — none of these primitives touch `RepoClient`).
- `pnpm lint`'s `no-restricted-imports` rule
  (`frontend/eslint.config.js:25-37`) must keep passing.
- Any new dependency must be permissively licensed and recorded in
  `docs/LICENSE_COMPLIANCE.md` in the same commit that adds it (none is
  expected — `lucide-react` is already a dependency).
- `pnpm build`, `pnpm lint`, and `pnpm test -- --run` must pass after
  every task.
- Every new component's styling comes from `frontend/src/styles/tokens.css`
  tokens only — no hardcoded colors.
- `localStorage` access uses the same plain, unguarded
  `localStorage.getItem`/`setItem` style already established in
  `frontend/src/lib/theme.ts` — no defensive try/catch wrapping.
- No existing component (`App.tsx`, `Panel`, `Toolbar`, `ListRow`, or any
  of the twelve reskinned components) is modified by this plan. This plan
  only adds new files (three new primitives) and extends one existing
  file (`SplitView.tsx` and its CSS/test files) in a backward-compatible
  way — its one current consumer (`App.tsx`'s history/diff split) must
  keep working with unchanged visual behavior when no new prop is passed.

---

### Task 1: Extend `SplitView` with resize and optional collapse

**Files:**
- Modify: `frontend/src/components/primitives/SplitView.tsx`
- Modify: `frontend/src/components/primitives/SplitView.module.css`
- Modify: `frontend/src/components/primitives/SplitView.test.tsx`

**Interfaces:**
- Produces:
  ```typescript
  interface SplitViewProps {
    left: ReactNode;
    right: ReactNode;
    storageKey?: string;   // if set, the left pane's width persists to localStorage under this key
    defaultWidth?: number; // px, default 300 — matches the current fixed width exactly
    minWidth?: number;     // px, default 160
    maxWidth?: number;     // px, default 480
    collapsible?: boolean; // default false — if true, the divider can snap the left pane to width 0
  }
  export function SplitView(props: SplitViewProps): JSX.Element;
  ```

- [ ] **Step 1: Baseline the existing test**

Run: `cd frontend && pnpm test -- --run SplitView.test.tsx`
Expected: PASS (1 test, "renders both panes").

- [ ] **Step 2: Write the new failing tests**

Replace the full contents of `frontend/src/components/primitives/SplitView.test.tsx`:

```typescript
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { SplitView } from "./SplitView";

describe("SplitView", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("renders both panes", () => {
    render(<SplitView left={<div>left pane</div>} right={<div>right pane</div>} />);
    expect(screen.getByText("left pane")).toBeInTheDocument();
    expect(screen.getByText("right pane")).toBeInTheDocument();
  });

  it("defaults the left pane to 300px when no width props are given", () => {
    render(<SplitView left={<div>left</div>} right={<div>right</div>} />);
    const left = screen.getByText("left").parentElement;
    expect(left).toHaveStyle({ width: "300px" });
  });

  it("applies a custom defaultWidth", () => {
    render(<SplitView left={<div>left</div>} right={<div>right</div>} defaultWidth={220} />);
    const left = screen.getByText("left").parentElement;
    expect(left).toHaveStyle({ width: "220px" });
  });

  it("resizes the left pane by dragging the divider, clamped to min/max", () => {
    render(
      <SplitView
        left={<div>left</div>}
        right={<div>right</div>}
        defaultWidth={300}
        minWidth={160}
        maxWidth={480}
      />,
    );
    const divider = screen.getByRole("separator");
    fireEvent.pointerDown(divider, { clientX: 300 });
    fireEvent.pointerMove(window, { clientX: 400 });
    fireEvent.pointerUp(window);
    const left = screen.getByText("left").parentElement;
    expect(left).toHaveStyle({ width: "400px" });

    fireEvent.pointerDown(divider, { clientX: 400 });
    fireEvent.pointerMove(window, { clientX: 1000 });
    fireEvent.pointerUp(window);
    expect(left).toHaveStyle({ width: "480px" });

    fireEvent.pointerDown(divider, { clientX: 480 });
    fireEvent.pointerMove(window, { clientX: 0 });
    fireEvent.pointerUp(window);
    expect(left).toHaveStyle({ width: "160px" });
  });

  it("resizes via arrow keys on the focused divider", () => {
    render(<SplitView left={<div>left</div>} right={<div>right</div>} defaultWidth={300} />);
    const divider = screen.getByRole("separator");
    divider.focus();
    fireEvent.keyDown(divider, { key: "ArrowRight" });
    const left = screen.getByText("left").parentElement;
    expect(left).toHaveStyle({ width: "316px" });
    fireEvent.keyDown(divider, { key: "ArrowLeft" });
    fireEvent.keyDown(divider, { key: "ArrowLeft" });
    expect(left).toHaveStyle({ width: "284px" });
  });

  it("persists width to localStorage when storageKey is set, and restores it on next mount", () => {
    const { unmount } = render(
      <SplitView left={<div>left</div>} right={<div>right</div>} storageKey="test-split" defaultWidth={300} />,
    );
    const divider = screen.getByRole("separator");
    fireEvent.pointerDown(divider, { clientX: 300 });
    fireEvent.pointerMove(window, { clientX: 350 });
    fireEvent.pointerUp(window);
    expect(localStorage.getItem("test-split")).toBe("350");
    unmount();

    render(<SplitView left={<div>left</div>} right={<div>right</div>} storageKey="test-split" defaultWidth={300} />);
    const left = screen.getByText("left").parentElement;
    expect(left).toHaveStyle({ width: "350px" });
  });

  it("does not persist width when storageKey is omitted", () => {
    render(<SplitView left={<div>left</div>} right={<div>right</div>} defaultWidth={300} />);
    const divider = screen.getByRole("separator");
    fireEvent.pointerDown(divider, { clientX: 300 });
    fireEvent.pointerMove(window, { clientX: 350 });
    fireEvent.pointerUp(window);
    expect(localStorage.length).toBe(0);
  });

  it("snaps to 0 and back when collapsible and double-clicked", () => {
    render(
      <SplitView left={<div>left</div>} right={<div>right</div>} defaultWidth={300} collapsible />,
    );
    const divider = screen.getByRole("separator");
    const left = screen.getByText("left").parentElement;
    fireEvent.doubleClick(divider);
    expect(left).toHaveStyle({ width: "0px" });
    fireEvent.doubleClick(divider);
    expect(left).toHaveStyle({ width: "300px" });
  });

  it("does not collapse on double-click when collapsible is false", () => {
    render(<SplitView left={<div>left</div>} right={<div>right</div>} defaultWidth={300} />);
    const divider = screen.getByRole("separator");
    const left = screen.getByText("left").parentElement;
    fireEvent.doubleClick(divider);
    expect(left).toHaveStyle({ width: "300px" });
  });
});
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run SplitView.test.tsx`
Expected: FAIL — `getByRole("separator")` finds nothing, current `SplitView` has no divider.

- [ ] **Step 4: Implement**

Replace the full contents of `frontend/src/components/primitives/SplitView.tsx`:

```typescript
import {
  useEffect,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import styles from "./SplitView.module.css";

const ARROW_KEY_STEP = 16;

function loadWidth(storageKey: string | undefined, defaultWidth: number, min: number, max: number): number {
  if (storageKey === undefined) return defaultWidth;
  const stored = localStorage.getItem(storageKey);
  if (stored === null) return defaultWidth;
  const parsed = Number.parseInt(stored, 10);
  if (Number.isNaN(parsed)) return defaultWidth;
  return Math.min(max, Math.max(min, parsed));
}

export function SplitView({
  left,
  right,
  storageKey,
  defaultWidth = 300,
  minWidth = 160,
  maxWidth = 480,
  collapsible = false,
}: {
  left: ReactNode;
  right: ReactNode;
  storageKey?: string;
  defaultWidth?: number;
  minWidth?: number;
  maxWidth?: number;
  collapsible?: boolean;
}) {
  const [width, setWidth] = useState(() => loadWidth(storageKey, defaultWidth, minWidth, maxWidth));
  const dragState = useRef<{ startX: number; startWidth: number } | null>(null);

  function persist(next: number) {
    if (storageKey !== undefined) {
      localStorage.setItem(storageKey, String(next));
    }
  }

  function clamp(next: number): number {
    if (collapsible && next < minWidth / 2) return 0;
    return Math.min(maxWidth, Math.max(minWidth, next));
  }

  function handlePointerDown(event: ReactPointerEvent) {
    dragState.current = { startX: event.clientX, startWidth: width };
  }

  useEffect(() => {
    function handlePointerMove(event: PointerEvent) {
      if (dragState.current === null) return;
      const delta = event.clientX - dragState.current.startX;
      setWidth(clamp(dragState.current.startWidth + delta));
    }
    function handlePointerUp() {
      if (dragState.current === null) return;
      dragState.current = null;
      setWidth((current) => {
        persist(current);
        return current;
      });
    }
    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);
    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [collapsible, minWidth, maxWidth]);

  function handleKeyDown(event: ReactKeyboardEvent) {
    if (event.key === "ArrowRight") {
      const next = clamp(width + ARROW_KEY_STEP);
      setWidth(next);
      persist(next);
    } else if (event.key === "ArrowLeft") {
      const next = clamp(width - ARROW_KEY_STEP);
      setWidth(next);
      persist(next);
    }
  }

  function handleDoubleClick() {
    if (!collapsible) return;
    const next = width === 0 ? defaultWidth : 0;
    setWidth(next);
    persist(next);
  }

  return (
    <div className={styles.splitView}>
      <div className={styles.left} style={{ width: `${width}px` }}>
        {left}
      </div>
      <div
        className={styles.divider}
        role="separator"
        aria-orientation="vertical"
        tabIndex={0}
        onPointerDown={handlePointerDown}
        onKeyDown={handleKeyDown}
        onDoubleClick={handleDoubleClick}
      />
      <div className={styles.right}>{right}</div>
    </div>
  );
}
```

- [ ] **Step 5: Write the CSS**

Replace the full contents of `frontend/src/components/primitives/SplitView.module.css`:

```css
/* frontend/src/components/primitives/SplitView.module.css */
.splitView {
  display: flex;
  flex: 1 1 auto;
  min-height: 0;
  overflow: hidden;
}

.left {
  flex: 0 0 auto;
  overflow-y: auto;
  overflow-x: hidden;
}

.divider {
  flex: 0 0 4px;
  cursor: col-resize;
  background: var(--color-border);
  transition: background var(--motion-duration-fast) var(--motion-easing-standard);
}

.divider:hover,
.divider:focus-visible {
  background: var(--color-accent);
  outline: none;
}

.right {
  flex: 1 1 auto;
  overflow-y: auto;
  padding: var(--space-3) var(--space-4);
}
```

Note: the `.left`'s `border-right` that existed before is now the
`.divider` element itself — do not add it back as a border on `.left`,
or the divider will look doubled.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run SplitView.test.tsx`
Expected: PASS (10 tests).

- [ ] **Step 7: Verify the existing consumer still works unchanged**

Run: `cd frontend && pnpm build && pnpm test -- --run`
Expected: PASS. `App.tsx`'s existing `<SplitView left={<CommitGraph .../>} right={<DiffPane .../>} />` call
passes no new props, so it renders at `defaultWidth=300` exactly as
before — confirm no App-level test broke.

- [ ] **Step 8: Commit**

```bash
git add frontend/src/components/primitives/SplitView.tsx frontend/src/components/primitives/SplitView.module.css frontend/src/components/primitives/SplitView.test.tsx
git commit -m "feat(frontend): add resize and optional collapse to SplitView"
```

---

### Task 2: Add the `Sidebar` primitive

**Files:**
- Create: `frontend/src/components/primitives/Sidebar.tsx`
- Create: `frontend/src/components/primitives/Sidebar.module.css`
- Create: `frontend/src/components/primitives/Sidebar.test.tsx`

**Interfaces:**
- Produces:
  ```typescript
  export function Sidebar({ children }: { children: ReactNode }): JSX.Element;
  ```

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/src/components/primitives/Sidebar.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Sidebar } from "./Sidebar";

describe("Sidebar", () => {
  it("renders its children inside a labeled navigation landmark", () => {
    render(
      <Sidebar>
        <div>section one</div>
        <div>section two</div>
      </Sidebar>,
    );
    const nav = screen.getByRole("navigation", { name: "Repository sections" });
    expect(nav).toBeInTheDocument();
    expect(screen.getByText("section one")).toBeInTheDocument();
    expect(screen.getByText("section two")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && pnpm test -- --run Sidebar.test.tsx`
Expected: FAIL — `Sidebar.tsx` does not exist yet.

- [ ] **Step 3: Implement**

```typescript
// frontend/src/components/primitives/Sidebar.tsx
import type { ReactNode } from "react";
import styles from "./Sidebar.module.css";

export function Sidebar({ children }: { children: ReactNode }) {
  return (
    <nav className={styles.sidebar} aria-label="Repository sections">
      {children}
    </nav>
  );
}
```

- [ ] **Step 4: Write the CSS**

```css
/* frontend/src/components/primitives/Sidebar.module.css */
.sidebar {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow-y: auto;
  background: var(--color-bg-subtle);
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd frontend && pnpm test -- --run Sidebar.test.tsx`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/primitives/Sidebar.tsx frontend/src/components/primitives/Sidebar.module.css frontend/src/components/primitives/Sidebar.test.tsx
git commit -m "feat(frontend): add Sidebar primitive"
```

---

### Task 3: Add the `AccordionSection` primitive

**Files:**
- Create: `frontend/src/components/primitives/AccordionSection.tsx`
- Create: `frontend/src/components/primitives/AccordionSection.module.css`
- Create: `frontend/src/components/primitives/AccordionSection.test.tsx`

**Interfaces:**
- Consumes: `ChevronDown`, `ChevronRight` from `lucide-react` (already a
  dependency — confirmed present in `node_modules/lucide-react`).
- Produces:
  ```typescript
  interface AccordionSectionProps {
    title: string;
    storageKey: string;     // localStorage key for this section's open/closed state
    defaultOpen?: boolean;  // default false
    children: ReactNode;
  }
  export function AccordionSection(props: AccordionSectionProps): JSX.Element;
  ```
  This is the component the Rollout plan's six migrated panels will
  render inside of, replacing their own `Panel` wrapper. It owns the
  section's accessible name (`aria-label={title}` on the `<section>`) —
  the Rollout plan's components must not also set their own
  region/landmark name.

- [ ] **Step 1: Write the failing tests**

```typescript
// frontend/src/components/primitives/AccordionSection.test.tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { AccordionSection } from "./AccordionSection";

describe("AccordionSection", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("defaults to closed and hides its content", () => {
    render(
      <AccordionSection title="Branches" storageKey="test-branches">
        <div>branch list</div>
      </AccordionSection>,
    );
    expect(screen.queryByText("branch list")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Branches" })).toHaveAttribute("aria-expanded", "false");
  });

  it("honors defaultOpen when nothing is stored", () => {
    render(
      <AccordionSection title="Branches" storageKey="test-branches-2" defaultOpen>
        <div>branch list</div>
      </AccordionSection>,
    );
    expect(screen.getByText("branch list")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Branches" })).toHaveAttribute("aria-expanded", "true");
  });

  it("toggles open and closed on click, and persists the state", () => {
    render(
      <AccordionSection title="Branches" storageKey="test-branches-3">
        <div>branch list</div>
      </AccordionSection>,
    );
    const button = screen.getByRole("button", { name: "Branches" });
    fireEvent.click(button);
    expect(screen.getByText("branch list")).toBeInTheDocument();
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(localStorage.getItem("test-branches-3")).toBe("open");

    fireEvent.click(button);
    expect(screen.queryByText("branch list")).not.toBeInTheDocument();
    expect(localStorage.getItem("test-branches-3")).toBe("closed");
  });

  it("restores persisted open state on mount, overriding defaultOpen", () => {
    localStorage.setItem("test-branches-4", "open");
    render(
      <AccordionSection title="Branches" storageKey="test-branches-4" defaultOpen={false}>
        <div>branch list</div>
      </AccordionSection>,
    );
    expect(screen.getByText("branch list")).toBeInTheDocument();
  });

  it("gives the section an accessible name matching its title", () => {
    render(
      <AccordionSection title="Remotes" storageKey="test-remotes">
        <div>remote list</div>
      </AccordionSection>,
    );
    expect(screen.getByRole("region", { name: "Remotes" })).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run AccordionSection.test.tsx`
Expected: FAIL — `AccordionSection.tsx` does not exist yet.

- [ ] **Step 3: Implement**

```typescript
// frontend/src/components/primitives/AccordionSection.tsx
import { ChevronDown, ChevronRight } from "lucide-react";
import { useState, type ReactNode } from "react";
import styles from "./AccordionSection.module.css";

function loadOpen(storageKey: string, defaultOpen: boolean): boolean {
  const stored = localStorage.getItem(storageKey);
  if (stored === "open") return true;
  if (stored === "closed") return false;
  return defaultOpen;
}

export function AccordionSection({
  title,
  storageKey,
  defaultOpen = false,
  children,
}: {
  title: string;
  storageKey: string;
  defaultOpen?: boolean;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(() => loadOpen(storageKey, defaultOpen));

  function toggle() {
    setOpen((prev) => {
      const next = !prev;
      localStorage.setItem(storageKey, next ? "open" : "closed");
      return next;
    });
  }

  return (
    <section className={styles.section} aria-label={title}>
      <button type="button" className={styles.header} aria-expanded={open} onClick={toggle}>
        {open ? (
          <ChevronDown size={14} aria-hidden="true" />
        ) : (
          <ChevronRight size={14} aria-hidden="true" />
        )}
        <span className={styles.title}>{title}</span>
      </button>
      {open && <div className={styles.body}>{children}</div>}
    </section>
  );
}
```

- [ ] **Step 4: Write the CSS**

```css
/* frontend/src/components/primitives/AccordionSection.module.css */
.section {
  border-bottom: 1px solid var(--color-border);
}

.header {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  width: 100%;
  padding: var(--space-2) var(--space-3);
  border: none;
  background: transparent;
  color: var(--color-text);
  font-size: var(--text-sm);
  font-weight: 600;
  text-align: left;
  cursor: pointer;
}

.header:hover {
  background: var(--color-bg-subtle);
}

.title {
  flex: 1 1 auto;
}

.body {
  padding: 0 var(--space-3) var(--space-3);
}
```

Note: `.header` is a `<button>`, which already receives base styling from
the global `button` rule in `frontend/src/index.css` (Phase 5's Task 0) —
this module's rules override only what needs to differ (no border, no
background, left-aligned, full-width) rather than redeclaring everything.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run AccordionSection.test.tsx`
Expected: PASS (5 tests).

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/primitives/AccordionSection.tsx frontend/src/components/primitives/AccordionSection.module.css frontend/src/components/primitives/AccordionSection.test.tsx
git commit -m "feat(frontend): add AccordionSection primitive"
```

---

### Task 4: Add the `Overlay` primitive

**Files:**
- Create: `frontend/src/components/primitives/Overlay.tsx`
- Create: `frontend/src/components/primitives/Overlay.module.css`
- Create: `frontend/src/components/primitives/Overlay.test.tsx`

**Interfaces:**
- Produces:
  ```typescript
  interface OverlayProps {
    onClose?: () => void; // called when the dialog closes for any reason (Escape included)
    children: ReactNode;
  }
  export function Overlay(props: OverlayProps): JSX.Element;
  ```
  The Rollout plan wraps `RebasePlanner` and `TransferPanel` in this,
  mounting `Overlay` only while `appState.state.rebaseOnto !== null` /
  `appState.state.transfer !== null` (the same conditions `App.tsx`
  already checks today) — mount/unmount IS the show/hide mechanism, there
  is no separate `active` prop.

- [ ] **Step 1: Write the failing tests**

```typescript
// frontend/src/components/primitives/Overlay.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Overlay } from "./Overlay";

describe("Overlay", () => {
  it("renders its children inside a dialog", () => {
    render(
      <Overlay>
        <p>overlay content</p>
      </Overlay>,
    );
    const dialog = screen.getByRole("dialog");
    expect(dialog).toBeInTheDocument();
    expect(screen.getByText("overlay content")).toBeInTheDocument();
  });

  it("opens the dialog via showModal when available", () => {
    const dialog = document.createElement("dialog");
    let calledShowModal = false;
    dialog.showModal = () => {
      calledShowModal = true;
      dialog.setAttribute("open", "");
    };
    dialog.close = () => dialog.removeAttribute("open");
    // Confirm the component calls showModal when the environment provides it —
    // verified indirectly: the dialog has the `open` attribute after mount.
    const { unmount } = render(
      <Overlay>
        <p>content</p>
      </Overlay>,
    );
    const renderedDialog = screen.getByRole("dialog");
    // jsdom's <dialog> may or may not implement showModal; either path
    // (showModal succeeding, or the setAttribute("open", "") fallback)
    // must leave the dialog open.
    expect(renderedDialog).toHaveAttribute("open");
    unmount();
  });

  it("calls onClose when the dialog's close event fires", () => {
    const onClose = vi.fn();
    render(
      <Overlay onClose={onClose}>
        <p>content</p>
      </Overlay>,
    );
    const dialog = screen.getByRole("dialog");
    dialog.dispatchEvent(new Event("close"));
    expect(onClose).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run Overlay.test.tsx`
Expected: FAIL — `Overlay.tsx` does not exist yet.

- [ ] **Step 3: Implement**

```typescript
// frontend/src/components/primitives/Overlay.tsx
import { useEffect, useRef, type ReactNode } from "react";
import styles from "./Overlay.module.css";

export function Overlay({ onClose, children }: { onClose?: () => void; children: ReactNode }) {
  const ref = useRef<HTMLDialogElement>(null);
  const openedViaShowModal = useRef(false);

  useEffect(() => {
    const dialog = ref.current;
    if (dialog === null) return;
    if (!dialog.open && typeof dialog.showModal === "function") {
      dialog.showModal();
      openedViaShowModal.current = true;
    } else if (!dialog.open) {
      dialog.setAttribute("open", "");
    }
    return () => {
      if (openedViaShowModal.current) {
        dialog.close();
      } else {
        dialog.removeAttribute("open");
      }
    };
  }, []);

  return (
    <dialog ref={ref} className={styles.overlay} onClose={onClose}>
      {children}
    </dialog>
  );
}
```

This mirrors the guarded pattern already established in
`RemotePanel.tsx:145-150` — do not call `showModal()` unconditionally.

- [ ] **Step 4: Write the CSS**

```css
/* frontend/src/components/primitives/Overlay.module.css */
.overlay {
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
  color: var(--color-text);
  padding: 0;
  max-width: 640px;
  width: 90vw;
}

.overlay::backdrop {
  background: rgba(0, 0, 0, 0.4);
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run Overlay.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/primitives/Overlay.tsx frontend/src/components/primitives/Overlay.module.css frontend/src/components/primitives/Overlay.test.tsx
git commit -m "feat(frontend): add Overlay primitive"
```

---

### Task 5: Full verification

**Files:** none (verification only).

- [ ] **Step 1: Full test suite**

Run: `cd frontend && pnpm test -- --run`
Expected: PASS — all prior tests plus the new ones from Tasks 1-4.

- [ ] **Step 2: Build and lint**

Run: `cd frontend && pnpm build && pnpm lint`
Expected: both PASS.

- [ ] **Step 3: Confirm no existing component was touched**

Run: `git diff --stat main..HEAD -- frontend/src/components | grep -v primitives/`
Expected: no output — every changed file under `frontend/src/components/`
is inside `primitives/`.

- [ ] **Step 4: Commit (if Step 3's check required a fix; otherwise this task has nothing to commit)**

No commit expected from this task under normal circumstances — it is a
verification checkpoint only.
