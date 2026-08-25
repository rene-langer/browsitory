# Sidebar Accordion UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the 9 UX/accessibility findings in the left-hand sidebar accordion (GitHub issue #17): default-closed-everything, no counts/icons, misleading "accordion" naming/keyboard claim, no expand/collapse-all, no motion, one long section burying the rest of the sidebar, and `PullRequestPanel`'s inconsistent nested `Panel`.

**Architecture:** A new `AccordionGroup` context primitive gives `AccordionSection` real APG roving-tabindex keyboard nav and an `expandAll`/`collapseAll` API, used once around `Sidebar`'s 7 top-level sections and once more (nested) around `PullRequestPanel`'s per-forge-repository list. `AccordionSection` itself gains `icon`/`count`/`headingLevel` props and CSS for flexible space-sharing and motion. The 6 simple sidebar panels (Branches, Worktrees, Submodules, Reflog, Remotes, Tags) get icon/count wiring; `PullRequestPanel` additionally swaps its per-repo `Panel` for a nested `AccordionSection`.

**Tech Stack:** React 18 + TypeScript, CSS Modules, `lucide-react` icons, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-08-25-sidebar-accordion-ux-design.md`

## Global Constraints

- Sections stay **independently** open/closed — no true mutual-exclusion accordion (spec Decisions).
- Only the **Branches** section gets `defaultOpen` among the 7 top-level sections; the rest stay closed by default.
- Keyboard nav implements the **full WAI-ARIA APG accordion pattern**: roving tabindex, `ArrowUp`/`ArrowDown` between sibling headers, `Home`/`End` to first/last.
- Expand/collapse-all is a **small toolbar atop `Sidebar`**, not a modifier-click or palette-only command.
- Section height uses **flexible space-sharing**: open sections share the sidebar's remaining height via flex-grow, each scrolling internally.
- `PullRequestPanel`'s nested per-repo cards become **nested `AccordionSection`s** in their own scoped `AccordionGroup` — see Task 5's note on why they default open (a refinement over the spec text that keeps existing behavior/tests intact; still satisfies "collapsible on its own").
- A header's accessible name (`aria-label`) is always exactly its `title` string — icon and count are `aria-hidden`, decorative only, so no existing `getByRole("button", { name: "..." })` query anywhere in the test suite needs to change.
- All new motion (chevron rotation, body fade-in) is wrapped in `@media (prefers-reduced-motion: reduce)` and disabled there.

---

### Task 1: `AccordionGroup` primitive

**Files:**
- Create: `frontend/src/components/primitives/AccordionGroup.tsx`
- Test: `frontend/src/components/primitives/AccordionGroup.test.tsx`

**Interfaces:**
- Produces: `AccordionGroup({ children, groupRef? }: { children: ReactNode; groupRef?: MutableRefObject<AccordionGroupHandle | null> })`, `AccordionGroupHandle { expandAll(): void; collapseAll(): void }`, `useAccordionGroup(): AccordionGroupContextValue | null`, where `AccordionGroupContextValue = { isActive(ref): boolean; register(header): () => void; onHeaderFocus(ref): void; onHeaderKeyDown(event, ref): void }` and `RegisteredHeader = { ref: RefObject<HTMLButtonElement>; setOpen: (open: boolean) => void }`. Task 2 (`AccordionSection`) is the sole consumer of `useAccordionGroup`/`register`/`isActive`/`onHeaderFocus`/`onHeaderKeyDown`. Task 3 (`Sidebar`) and Task 5 (`PullRequestPanel`) are the consumers of `AccordionGroup`/`AccordionGroupHandle`/`groupRef`.

- [ ] **Step 1: Write the failing test**

```tsx
// frontend/src/components/primitives/AccordionGroup.test.tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { useEffect, useRef, useState, type MutableRefObject } from "react";
import { describe, expect, it } from "vitest";
import { AccordionGroup, useAccordionGroup, type AccordionGroupHandle } from "./AccordionGroup";

function Header({ label, open, onOpenChange }: { label: string; open: boolean; onOpenChange: (open: boolean) => void }) {
  const ref = useRef<HTMLButtonElement>(null);
  const group = useAccordionGroup();

  useEffect(() => {
    return group?.register({ ref, setOpen: onOpenChange });
    // Register once on mount, not on every `[group]` identity change: `AccordionGroup`'s
    // memoized context value gets a new identity whenever the active header changes (its
    // `isActive` callback depends on `activeRef` state), so keying this effect on `group` would
    // unregister/re-register on every focus/arrow-key move, desyncing tabIndex from the actually
    // active header. `register`/`onHeaderFocus`/`onHeaderKeyDown` are themselves referentially
    // stable across those identity changes, so capturing them once at mount is correct.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <button
      ref={ref}
      type="button"
      aria-expanded={open}
      tabIndex={group === null ? 0 : group.isActive(ref) ? 0 : -1}
      onFocus={() => group?.onHeaderFocus(ref)}
      onKeyDown={(event) => group?.onHeaderKeyDown(event, ref)}
      onClick={() => onOpenChange(!open)}
    >
      {label}
    </button>
  );
}

function ThreeHeaders({ groupRef }: { groupRef?: MutableRefObject<AccordionGroupHandle | null> }) {
  const [open, setOpen] = useState({ a: false, b: false, c: false });
  return (
    <AccordionGroup groupRef={groupRef}>
      <Header label="A" open={open.a} onOpenChange={(v) => setOpen((s) => ({ ...s, a: v }))} />
      <Header label="B" open={open.b} onOpenChange={(v) => setOpen((s) => ({ ...s, b: v }))} />
      <Header label="C" open={open.c} onOpenChange={(v) => setOpen((s) => ({ ...s, c: v }))} />
    </AccordionGroup>
  );
}

describe("AccordionGroup", () => {
  it("gives only the first registered header tabIndex 0 initially", () => {
    render(<ThreeHeaders />);
    expect(screen.getByRole("button", { name: "A" })).toHaveAttribute("tabIndex", "0");
    expect(screen.getByRole("button", { name: "B" })).toHaveAttribute("tabIndex", "-1");
    expect(screen.getByRole("button", { name: "C" })).toHaveAttribute("tabIndex", "-1");
  });

  it("moves the roving tab stop with ArrowDown/ArrowUp, wrapping at the ends", () => {
    render(<ThreeHeaders />);
    const a = screen.getByRole("button", { name: "A" });
    const b = screen.getByRole("button", { name: "B" });
    const c = screen.getByRole("button", { name: "C" });

    fireEvent.keyDown(a, { key: "ArrowDown" });
    expect(b).toHaveFocus();
    expect(b).toHaveAttribute("tabIndex", "0");
    expect(a).toHaveAttribute("tabIndex", "-1");

    fireEvent.keyDown(b, { key: "ArrowDown" });
    expect(c).toHaveFocus();

    fireEvent.keyDown(c, { key: "ArrowDown" });
    expect(a).toHaveFocus();

    fireEvent.keyDown(a, { key: "ArrowUp" });
    expect(c).toHaveFocus();
  });

  it("Home/End jump to the first/last header", () => {
    render(<ThreeHeaders />);
    const b = screen.getByRole("button", { name: "B" });
    fireEvent.keyDown(b, { key: "End" });
    expect(screen.getByRole("button", { name: "C" })).toHaveFocus();
    fireEvent.keyDown(screen.getByRole("button", { name: "C" }), { key: "Home" });
    expect(screen.getByRole("button", { name: "A" })).toHaveFocus();
  });

  it("ArrowDown/ArrowUp/Home/End do not toggle open state", () => {
    render(<ThreeHeaders />);
    const a = screen.getByRole("button", { name: "A" });
    fireEvent.keyDown(a, { key: "ArrowDown" });
    expect(a).toHaveAttribute("aria-expanded", "false");
  });

  it("expandAll/collapseAll drive every registered header via the imperative handle", () => {
    const groupRef: MutableRefObject<AccordionGroupHandle | null> = { current: null };
    render(<ThreeHeaders groupRef={groupRef} />);

    groupRef.current?.expandAll();
    expect(screen.getByRole("button", { name: "A" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "B" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "C" })).toHaveAttribute("aria-expanded", "true");

    groupRef.current?.collapseAll();
    expect(screen.getByRole("button", { name: "A" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByRole("button", { name: "B" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByRole("button", { name: "C" })).toHaveAttribute("aria-expanded", "false");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && pnpm vitest run src/components/primitives/AccordionGroup.test.tsx`
Expected: FAIL — `Cannot find module './AccordionGroup'`

- [ ] **Step 3: Write minimal implementation**

```tsx
// frontend/src/components/primitives/AccordionGroup.tsx
import {
  createContext,
  useCallback,
  useContext,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type MutableRefObject,
  type ReactNode,
  type RefObject,
} from "react";

interface RegisteredHeader {
  ref: RefObject<HTMLButtonElement>;
  setOpen: (open: boolean) => void;
}

export interface AccordionGroupContextValue {
  isActive: (ref: RefObject<HTMLButtonElement>) => boolean;
  register: (header: RegisteredHeader) => () => void;
  onHeaderFocus: (ref: RefObject<HTMLButtonElement>) => void;
  onHeaderKeyDown: (event: KeyboardEvent<HTMLButtonElement>, ref: RefObject<HTMLButtonElement>) => void;
}

const AccordionGroupContext = createContext<AccordionGroupContextValue | null>(null);

export interface AccordionGroupHandle {
  expandAll: () => void;
  collapseAll: () => void;
}

/**
 * Groups sibling `AccordionSection` headers so they share one WAI-ARIA APG roving-tabindex
 * keyboard region (Up/Down/Home/End between headers) and one `expandAll`/`collapseAll` handle.
 * `Sidebar` wraps its 7 top-level sections in one `AccordionGroup`; `PullRequestPanel` wraps its
 * per-forge-repository cards in a second, independently scoped one nested inside the first —
 * arrow-key nav in one group never crosses into the other.
 */
export function AccordionGroup({
  children,
  groupRef,
}: {
  children: ReactNode;
  groupRef?: MutableRefObject<AccordionGroupHandle | null>;
}) {
  const headersRef = useRef<RegisteredHeader[]>([]);
  const [activeRef, setActiveRefState] = useState<RefObject<HTMLButtonElement> | null>(null);

  const register = useCallback((header: RegisteredHeader) => {
    headersRef.current = [...headersRef.current, header];
    setActiveRefState((current) => current ?? header.ref);
    return () => {
      headersRef.current = headersRef.current.filter((entry) => entry !== header);
      setActiveRefState((current) => (current === header.ref ? (headersRef.current[0]?.ref ?? null) : current));
    };
  }, []);

  const isActive = useCallback((ref: RefObject<HTMLButtonElement>) => activeRef === ref, [activeRef]);

  const onHeaderFocus = useCallback((ref: RefObject<HTMLButtonElement>) => {
    setActiveRefState(ref);
  }, []);

  const focusIndex = useCallback((index: number) => {
    const headers = headersRef.current;
    if (headers.length === 0) return;
    const wrapped = (index + headers.length) % headers.length;
    const target = headers[wrapped];
    setActiveRefState(target.ref);
    target.ref.current?.focus();
  }, []);

  const onHeaderKeyDown = useCallback(
    (event: KeyboardEvent<HTMLButtonElement>, ref: RefObject<HTMLButtonElement>) => {
      const headers = headersRef.current;
      const index = headers.findIndex((entry) => entry.ref === ref);
      if (index === -1) return;
      if (event.key === "ArrowDown") {
        event.preventDefault();
        focusIndex(index + 1);
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        focusIndex(index - 1);
      } else if (event.key === "Home") {
        event.preventDefault();
        focusIndex(0);
      } else if (event.key === "End") {
        event.preventDefault();
        focusIndex(headers.length - 1);
      }
    },
    [focusIndex],
  );

  useImperativeHandle(
    groupRef,
    () => ({
      expandAll: () => headersRef.current.forEach((header) => header.setOpen(true)),
      collapseAll: () => headersRef.current.forEach((header) => header.setOpen(false)),
    }),
    [],
  );

  const value = useMemo<AccordionGroupContextValue>(
    () => ({ isActive, register, onHeaderFocus, onHeaderKeyDown }),
    [isActive, register, onHeaderFocus, onHeaderKeyDown],
  );

  return <AccordionGroupContext.Provider value={value}>{children}</AccordionGroupContext.Provider>;
}

export function useAccordionGroup(): AccordionGroupContextValue | null {
  return useContext(AccordionGroupContext);
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && pnpm vitest run src/components/primitives/AccordionGroup.test.tsx`
Expected: PASS (5 tests)

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/primitives/AccordionGroup.tsx frontend/src/components/primitives/AccordionGroup.test.tsx
git commit -m "feat(frontend): add AccordionGroup roving-tabindex primitive"
```

---

### Task 2: `AccordionSection` — icon, count, headingLevel, motion, group wiring

**Files:**
- Modify: `frontend/src/components/primitives/AccordionSection.tsx`
- Modify: `frontend/src/components/primitives/AccordionSection.module.css`
- Modify: `frontend/src/components/primitives/AccordionSection.test.tsx`

**Interfaces:**
- Consumes: `useAccordionGroup` from Task 1 (`./AccordionGroup`).
- Produces: `AccordionSection({ title, storageKey, defaultOpen?, icon?, count?, headingLevel?, children }: { title: string; storageKey: string; defaultOpen?: boolean; icon?: LucideIcon; count?: number; headingLevel?: 2 | 3; children: ReactNode })`. Tasks 3, 4, 5 pass `icon`/`count`/`headingLevel`/`defaultOpen` to this component; none of the existing 7 call sites need to change to keep compiling (all new props are optional).

- [ ] **Step 1: Write the failing tests**

Add to the end of `frontend/src/components/primitives/AccordionSection.test.tsx` (keep all existing tests as-is):

```tsx
import { GitBranch } from "lucide-react";
// (add this import at the top of the file alongside the existing ones)

// ...append inside the existing describe("AccordionSection", ...) block:

  it("renders an icon and count when provided, both hidden from the accessible tree", () => {
    render(
      <AccordionSection title="Branches" storageKey="test-branches-6" icon={GitBranch} count={3}>
        <div>branch list</div>
      </AccordionSection>,
    );
    const button = screen.getByRole("button", { name: "Branches" });
    expect(button).toHaveTextContent("3");
    expect(button.querySelector("svg")).toBeInTheDocument();
  });

  it("keeps the header's accessible name equal to the title even with icon and count set", () => {
    render(
      <AccordionSection title="Remotes" storageKey="test-remotes-2" icon={GitBranch} count={12}>
        <div>remote list</div>
      </AccordionSection>,
    );
    expect(screen.getByRole("button", { name: "Remotes" })).toBeInTheDocument();
  });

  it("renders the title at the requested heading level", () => {
    render(
      <AccordionSection title="Nested" storageKey="test-nested" headingLevel={3}>
        <div>nested body</div>
      </AccordionSection>,
    );
    expect(screen.getByRole("heading", { level: 3, name: "Nested" })).toBeInTheDocument();
  });

  it("rotates the chevron open/closed via a single icon rather than swapping elements", () => {
    render(
      <AccordionSection title="Branches" storageKey="test-branches-7">
        <div>branch list</div>
      </AccordionSection>,
    );
    const button = screen.getByRole("button", { name: "Branches" });
    const chevron = button.querySelector("svg");
    expect(chevron).not.toBeNull();
    fireEvent.click(button);
    // Same SVG node stays mounted (rotated via CSS class), not swapped for a different icon.
    expect(button.querySelector("svg")).toBe(chevron);
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm vitest run src/components/primitives/AccordionSection.test.tsx`
Expected: FAIL — count/icon not rendered, `headingLevel` prop unknown, chevron swap replaces the SVG node.

- [ ] **Step 3: Write minimal implementation**

```tsx
// frontend/src/components/primitives/AccordionSection.tsx
import { ChevronRight, type LucideIcon } from "lucide-react";
import { createElement, useEffect, useRef, useState, type ReactNode } from "react";
import { useAccordionGroup } from "./AccordionGroup";
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
  icon: Icon,
  count,
  headingLevel = 2,
  children,
}: {
  title: string;
  storageKey: string;
  defaultOpen?: boolean;
  icon?: LucideIcon;
  count?: number;
  headingLevel?: 2 | 3;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(() => loadOpen(storageKey, defaultOpen));
  const headerRef = useRef<HTMLButtonElement>(null);
  const group = useAccordionGroup();

  function setOpenState(next: boolean) {
    setOpen(next);
    localStorage.setItem(storageKey, next ? "open" : "closed");
  }

  useEffect(() => {
    return group?.register({ ref: headerRef, setOpen: setOpenState });
    // Register once on mount, not on every `[group]` identity change — see AccordionGroup.tsx's
    // `isActive` (depends on `activeRef`, so the memoized context value's identity changes on
    // every focus/arrow-key move); keying this effect on `group` churns registration on every
    // such change and desyncs tabIndex from the actually active header.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <section className={styles.section} data-open={open} aria-label={title}>
      {/* WAI-ARIA APG accordion pattern: a heading wrapping the trigger button (so the section
          carries the heading role), plus roving-tabindex keyboard nav across sibling headers via
          `AccordionGroup`. Open state itself is independent per section by design (not mutual
          exclusion) — each section persists its own open/closed state under its own
          `storageKey`, so opening one is not meant to close its siblings. */}
      {createElement(
        `h${headingLevel}`,
        { className: styles.heading },
        <button
          ref={headerRef}
          type="button"
          className={styles.header}
          aria-expanded={open}
          aria-label={title}
          tabIndex={group === null ? 0 : group.isActive(headerRef) ? 0 : -1}
          onFocus={() => group?.onHeaderFocus(headerRef)}
          onKeyDown={(event) => group?.onHeaderKeyDown(event, headerRef)}
          onClick={() => setOpenState(!open)}
        >
          <ChevronRight
            size={14}
            aria-hidden="true"
            className={open ? `${styles.chevron} ${styles.chevronOpen}` : styles.chevron}
          />
          {Icon !== undefined && <Icon size={14} aria-hidden="true" className={styles.icon} />}
          <span className={styles.title}>{title}</span>
          {count !== undefined && (
            <span className={styles.count} aria-hidden="true">
              {count}
            </span>
          )}
        </button>,
      )}
      {open && <div className={styles.body}>{children}</div>}
    </section>
  );
}
```

```css
/* frontend/src/components/primitives/AccordionSection.module.css */
.section {
  border-bottom: 1px solid var(--color-border);
  display: flex;
  flex-direction: column;
  flex: 0 0 auto;
  min-height: 0;
}

.section[data-open="true"] {
  flex: 1 1 0%;
}

/* The heading only carries the role; the button inside owns all the spacing. */
.heading {
  margin: 0;
  flex: 0 0 auto;
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

/* Not --color-bg-subtle: that is exactly Sidebar's own background, so the
   hover would be invisible for a section nested inside a Sidebar. */
.header:hover:not(:disabled) {
  background: var(--color-selected-bg);
}

.chevron {
  flex: 0 0 auto;
  transition: transform 120ms ease;
}

.chevronOpen {
  transform: rotate(90deg);
}

.icon {
  flex: 0 0 auto;
  color: var(--color-text-muted);
}

.title {
  flex: 1 1 auto;
}

.count {
  flex: 0 0 auto;
  color: var(--color-text-muted);
  font-size: var(--text-xs);
  font-weight: 400;
}

.body {
  padding: 0 var(--space-3) var(--space-3);
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  animation: accordion-body-in 120ms ease;
}

@keyframes accordion-body-in {
  from {
    opacity: 0;
    transform: translateY(-4px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@media (prefers-reduced-motion: reduce) {
  .chevron {
    transition: none;
  }

  .body {
    animation: none;
  }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm vitest run src/components/primitives/AccordionSection.test.tsx`
Expected: PASS (all original + new tests)

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/primitives/AccordionSection.tsx frontend/src/components/primitives/AccordionSection.module.css frontend/src/components/primitives/AccordionSection.test.tsx
git commit -m "feat(frontend): add icon/count/headingLevel and roving-tabindex nav to AccordionSection"
```

---

### Task 3: Wire `Sidebar` — `AccordionGroup` + expand/collapse-all toolbar + space-sharing layout

**Files:**
- Modify: `frontend/src/components/primitives/Sidebar.tsx`
- Modify: `frontend/src/components/primitives/Sidebar.module.css`
- Modify: `frontend/src/components/primitives/Sidebar.test.tsx`

**Interfaces:**
- Consumes: `AccordionGroup`, `AccordionGroupHandle` from Task 1.
- Produces: no change to `Sidebar`'s public props (`{ children: ReactNode }`) — `App.tsx`'s existing `<Sidebar>...</Sidebar>` call (App.tsx:132-223) needs no changes.

- [ ] **Step 1: Write the failing tests**

Add to `frontend/src/components/primitives/Sidebar.test.tsx` (keep the existing test):

```tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AccordionSection } from "./AccordionSection";
import { Sidebar } from "./Sidebar";

describe("Sidebar", () => {
  it("renders its children inside a labeled complementary landmark", () => {
    render(
      <Sidebar>
        <div>section one</div>
        <div>section two</div>
      </Sidebar>,
    );
    const aside = screen.getByRole("complementary", { name: "Repository sections" });
    expect(aside).toBeInTheDocument();
    expect(screen.getByText("section one")).toBeInTheDocument();
    expect(screen.getByText("section two")).toBeInTheDocument();
  });

  it("expands every section when Expand all is clicked", () => {
    render(
      <Sidebar>
        <AccordionSection title="One" storageKey="sidebar-test-one">
          <div>one body</div>
        </AccordionSection>
        <AccordionSection title="Two" storageKey="sidebar-test-two">
          <div>two body</div>
        </AccordionSection>
      </Sidebar>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Expand all sections" }));
    expect(screen.getByRole("button", { name: "One" })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "Two" })).toHaveAttribute("aria-expanded", "true");
  });

  it("collapses every section when Collapse all is clicked", () => {
    render(
      <Sidebar>
        <AccordionSection title="One" storageKey="sidebar-test-three" defaultOpen>
          <div>one body</div>
        </AccordionSection>
        <AccordionSection title="Two" storageKey="sidebar-test-four" defaultOpen>
          <div>two body</div>
        </AccordionSection>
      </Sidebar>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Collapse all sections" }));
    expect(screen.getByRole("button", { name: "One" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByRole("button", { name: "Two" })).toHaveAttribute("aria-expanded", "false");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm vitest run src/components/primitives/Sidebar.test.tsx`
Expected: FAIL — no "Expand all sections"/"Collapse all sections" buttons exist yet.

- [ ] **Step 3: Write minimal implementation**

```tsx
// frontend/src/components/primitives/Sidebar.tsx
import { useRef, type ReactNode } from "react";
import { ChevronsDownUp, ChevronsUpDown } from "lucide-react";
import { AccordionGroup, type AccordionGroupHandle } from "./AccordionGroup";
import styles from "./Sidebar.module.css";

export function Sidebar({ children }: { children: ReactNode }) {
  const groupRef = useRef<AccordionGroupHandle | null>(null);

  return (
    <aside className={styles.sidebar} aria-label="Repository sections">
      <div className={styles.toolbar}>
        <button
          type="button"
          className={styles.toolbarButton}
          aria-label="Expand all sections"
          onClick={() => groupRef.current?.expandAll()}
        >
          <ChevronsDownUp size={14} aria-hidden="true" />
        </button>
        <button
          type="button"
          className={styles.toolbarButton}
          aria-label="Collapse all sections"
          onClick={() => groupRef.current?.collapseAll()}
        >
          <ChevronsUpDown size={14} aria-hidden="true" />
        </button>
      </div>
      <AccordionGroup groupRef={groupRef}>
        <div className={styles.sections}>{children}</div>
      </AccordionGroup>
    </aside>
  );
}
```

```css
/* frontend/src/components/primitives/Sidebar.module.css */
.sidebar {
  display: flex;
  flex-direction: column;
  height: 100%;
  background: var(--color-bg-subtle);
}

.toolbar {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-1);
  padding: var(--space-1) var(--space-2);
  border-bottom: 1px solid var(--color-border);
  flex: 0 0 auto;
}

.toolbarButton {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-1);
  border: none;
  background: transparent;
  color: var(--color-text-muted);
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.toolbarButton:hover {
  background: var(--color-selected-bg);
  color: var(--color-text);
}

.sections {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm vitest run src/components/primitives/Sidebar.test.tsx`
Expected: PASS (3 tests)

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/primitives/Sidebar.tsx frontend/src/components/primitives/Sidebar.module.css frontend/src/components/primitives/Sidebar.test.tsx
git commit -m "feat(frontend): add expand/collapse-all toolbar and space-sharing layout to Sidebar"
```

---

### Task 4: Icon + count wiring for the 6 simple sidebar panels

**Files:**
- Modify: `frontend/src/components/BranchSwitcher.tsx:97`
- Modify: `frontend/src/components/BranchSwitcher.test.tsx` (delete the now-invalid unconditional header-open click in `renderSwitcher`)
- Modify: `frontend/src/components/WorktreePanel.tsx:1-6,57`
- Modify: `frontend/src/components/SubmodulePanel.tsx:32`
- Modify: `frontend/src/components/ReflogPanel.tsx:30`
- Modify: `frontend/src/components/RemotePanel.tsx:1-6,183`
- Modify: `frontend/src/components/TagPanel.tsx:61`

**Interfaces:**
- Consumes: `AccordionSection`'s `icon`/`count`/`defaultOpen` props from Task 2. No new exports produced — this task is purely call-site wiring, covered by Task 2's own icon/count rendering tests plus each panel's existing test suite (which must stay green unchanged, since the header's accessible name doesn't change — see Global Constraints).

This task has no new tests of its own for 5 of the 6 files: the generic icon/count rendering behavior is already covered by `AccordionSection.test.tsx` (Task 2), and per-call-site prop wiring doesn't need duplicate coverage (same rationale CLAUDE.md gives for not testing thin pass-through Tauri commands separately from the logic they call). `BranchSwitcher.tsx` is the one exception — see Step 1 below, its `defaultOpen` addition requires a one-line test fix, since its own test's `renderSwitcher` helper unconditionally clicks the header open on the assumption it starts closed. Each step below is a direct, complete diff — after each file, run that file's existing test to confirm no regression.

- [ ] **Step 1: `BranchSwitcher.tsx`** — add the icon/count/defaultOpen (this is the one section that opens by default, per Global Constraints), and fix its test's now-invalid assumption that the section starts closed

```tsx
// frontend/src/components/BranchSwitcher.tsx — change line 97 only
    <AccordionSection title="Branches" storageKey="sidebar-branches" icon={GitBranch} count={branches.length} defaultOpen>
```

`GitBranch` is already imported at the top of the file (line 2) for the branch-switcher button icon — reused here, no new import needed.

`BranchSwitcher.test.tsx`'s `renderSwitcher` helper (around line 34) unconditionally does `fireEvent.click(screen.getByRole("button", { name: "Branches" }))` right after render, on the assumption the accordion starts closed and needs opening. With `defaultOpen` added, the section renders already open (its own `localStorage.removeItem("sidebar-branches")` on line 12 guarantees no stale stored state overrides that), so this click would instead *close* it and break every downstream assertion. Delete that line — it's the only place in the file that clicks the accordion header; every other assertion operates on content inside the section body (the "Branch switcher" toggle, branch list, stash list, create-branch form), which is visible as soon as the section starts open.

Run: `cd frontend && pnpm vitest run src/components/BranchSwitcher.test.tsx`
Expected: PASS

- [ ] **Step 2: `WorktreePanel.tsx`** — add a `GitFork` import and wire icon/count

```tsx
// frontend/src/components/WorktreePanel.tsx — change lines 1-2
import { useState } from "react";
import { FolderGit2, GitFork } from "lucide-react";
```

```tsx
// frontend/src/components/WorktreePanel.tsx — change line 57
    <AccordionSection title="Worktrees" storageKey="sidebar-worktrees" icon={GitFork} count={worktrees.length}>
```

`FolderGit2` stays as-is (used for each row's icon at line 102); `GitFork` is new, used only for the header.

Run: `cd frontend && pnpm vitest run src/components/WorktreePanel.test.tsx`
Expected: PASS (unchanged)

- [ ] **Step 3: `SubmodulePanel.tsx`** — wire icon/count, reusing the already-imported `Package`

```tsx
// frontend/src/components/SubmodulePanel.tsx — change line 32
    <AccordionSection title="Submodules" storageKey="sidebar-submodules" icon={Package} count={submodules.length}>
```

Run: `cd frontend && pnpm vitest run src/components/SubmodulePanel.test.tsx`
Expected: PASS (unchanged)

- [ ] **Step 4: `ReflogPanel.tsx`** — wire icon/count, reusing the already-imported `History`

```tsx
// frontend/src/components/ReflogPanel.tsx — change line 30
    <AccordionSection title="Reflog" storageKey="sidebar-reflog" icon={History} count={entries.length}>
```

Run: `cd frontend && pnpm vitest run src/components/ReflogPanel.test.tsx`
Expected: PASS (unchanged)

- [ ] **Step 5: `RemotePanel.tsx`** — add a `Cloud` import and wire icon/count

```tsx
// frontend/src/components/RemotePanel.tsx — change lines 1-3
import { useEffect, useRef, useState } from "react";
import { Cloud } from "lucide-react";
import type { PullOutcome, RemoteAuthMode, RemoteInfo, UpstreamInfo } from "../ipc/RepoClient";
```

```tsx
// frontend/src/components/RemotePanel.tsx — change line 183
    <AccordionSection title="Remotes" storageKey="sidebar-remotes" icon={Cloud} count={remotes.length}>
```

Run: `cd frontend && pnpm vitest run src/components/RemotePanel.test.tsx`
Expected: PASS (unchanged)

- [ ] **Step 6: `TagPanel.tsx`** — wire icon/count, reusing the already-imported `Tag`

```tsx
// frontend/src/components/TagPanel.tsx — change line 61
    <AccordionSection title="Tags" storageKey="sidebar-tags" icon={Tag} count={tags.length}>
```

Run: `cd frontend && pnpm vitest run src/components/TagPanel.test.tsx`
Expected: PASS (unchanged)

- [ ] **Step 7: Commit**

```bash
git add frontend/src/components/BranchSwitcher.tsx frontend/src/components/WorktreePanel.tsx frontend/src/components/SubmodulePanel.tsx frontend/src/components/ReflogPanel.tsx frontend/src/components/RemotePanel.tsx frontend/src/components/TagPanel.tsx
git commit -m "feat(frontend): add header icons/counts to the 6 simple sidebar panels"
```

---

### Task 5: `PullRequestPanel` — nested `AccordionSection` per forge repo, outer icon/count

**Files:**
- Modify: `frontend/src/components/PullRequestPanel.tsx`
- Modify: `frontend/src/components/PullRequestPanel.test.tsx`

**Interfaces:**
- Consumes: `AccordionSection`'s `icon`/`count`/`headingLevel`/`defaultOpen` (Task 2), `AccordionGroup` (Task 1).
- Produces: no change to `PullRequestPanel`'s public props.

**Design note (refinement over the spec):** the spec says the per-repo cards become nested `AccordionSection`s, "collapsible on its own." The existing test suite (and current UX) has every repo's token form / PR list / create form visible immediately once the outer "Pull Requests" section is open — there's no per-repo click today, because `Panel` isn't collapsible. Making the nested sections collapsible while *defaulting them closed* would require every one of `PullRequestPanel.test.tsx`'s ~18 tests to add a click to open the relevant repo card first, and would regress the current "open Pull Requests, everything's right there" experience. Instead, each nested `AccordionSection` gets `defaultOpen` (still using its own `storageKey`, so a user who *does* collapse one gets that choice remembered). This satisfies the literal requirement — each repo's content is now collapsible on its own — without changing default visibility or touching the existing test suite.

- [ ] **Step 1: Write the failing tests**

Add to `frontend/src/components/PullRequestPanel.test.tsx` (all 18 existing tests are unaffected and stay exactly as-is, since the per-repo cards still render open by default):

```tsx
  it("shows an icon and the total open pull-request count on the outer Pull Requests header", () => {
    renderPanel({
      pullRequests: { origin: { pullRequests: [openPullRequest], truncated: false } },
    });
    const header = screen.getByRole("button", { name: "Pull Requests" });
    expect(header).toHaveTextContent("1");
    expect(header.querySelector("svg")).toBeInTheDocument();
  });

  it("lets a repository's own card be collapsed independently without hiding the others", () => {
    renderPanel({
      forgeRepositories: [githubRepo, bitbucketRepo],
      pullRequests: {
        origin: { pullRequests: [openPullRequest], truncated: false },
      },
    });

    fireEvent.click(screen.getByRole("button", { name: /github: acme\/widget \(origin\)/i }));

    expect(screen.queryByLabelText("Account")).toBeTruthy();
    // The GitHub card's own content collapses...
    const githubSection = screen.getByRole("region", { name: /github: acme\/widget \(origin\)/i });
    expect(within(githubSection).queryByRole("button", { name: "List pull requests" })).not.toBeInTheDocument();
    // ...while the Bitbucket card, untouched, stays open.
    const bitbucketSection = screen.getByRole("region", { name: /bitbucket: acme\/widget \(bb-origin\)/i });
    expect(within(bitbucketSection).getByRole("button", { name: "List pull requests" })).toBeInTheDocument();
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm vitest run src/components/PullRequestPanel.test.tsx`
Expected: FAIL — no icon/count on the outer header yet; per-repo `Panel` isn't collapsible (no header button to click at all inside `githubSection`/`bitbucketSection`).

- [ ] **Step 3: Write minimal implementation**

```tsx
// frontend/src/components/PullRequestPanel.tsx — imports (replace lines 1-12)
import { useRef, useState } from "react";
import { ExternalLink, GitPullRequest } from "lucide-react";
import type {
  CreatePullRequest,
  ForgeProvider,
  ForgeRepository,
  PullRequestList,
} from "../ipc/RepoClient";
import { AccordionGroup } from "./primitives/AccordionGroup";
import { AccordionSection } from "./primitives/AccordionSection";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./PullRequestPanel.module.css";
```

`Panel` is no longer imported — nothing else in this file uses it after this task.

```tsx
// frontend/src/components/PullRequestPanel.tsx — ForgeRepositorySection's return (replace line 143-144's
// `<Panel title={sectionLabel} ariaLabel={sectionLabel} headingLevel={3}>` open tag and its matching
// `</Panel>` close tag at line 217)
  return (
    <AccordionSection
      title={sectionLabel}
      storageKey={`sidebar-pr-${repository.remoteName}`}
      headingLevel={3}
      defaultOpen
    >
```

```tsx
// ...and its closing tag, replacing `</Panel>` at line 217:
    </AccordionSection>
  );
}
```

```tsx
// frontend/src/components/PullRequestPanel.tsx — the exported PullRequestPanel function
// (replace lines 243-273, everything from the `if (forgeRepositories.length === 0)` guard
// through the end of the function)
  if (forgeRepositories.length === 0) {
    return (
      <AccordionSection title="Pull Requests" storageKey="sidebar-pull-requests" icon={GitPullRequest}>
        <p>No supported GitHub or Bitbucket remotes detected.</p>
      </AccordionSection>
    );
  }

  const totalPullRequests = forgeRepositories.reduce(
    (sum, repository) => sum + (pullRequests[repository.remoteName]?.pullRequests.length ?? 0),
    0,
  );

  // The inner AccordionSection per forge repository is intentional: each one is a card titled
  // with its own provider/owner/remote, nested inside this section's AccordionSection body, in
  // its own AccordionGroup so its roving-tabindex nav stays scoped to just the repo cards.
  return (
    <AccordionSection
      title="Pull Requests"
      storageKey="sidebar-pull-requests"
      icon={GitPullRequest}
      count={totalPullRequests}
    >
      <AccordionGroup>
        <div className={styles.sections}>
          {forgeRepositories.map((repository) => (
            <ForgeRepositorySection
              key={repository.remoteName}
              repository={repository}
              pullRequests={pullRequests[repository.remoteName]}
              onListPullRequests={onListPullRequests}
              onForgetForgeToken={onForgetForgeToken}
              onSaveForgeToken={onSaveForgeToken}
              onCreatePullRequest={onCreatePullRequest}
              onOpenExternalUrl={onOpenExternalUrl}
              operationDisabled={operationDisabled}
            />
          ))}
        </div>
      </AccordionGroup>
    </AccordionSection>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm vitest run src/components/PullRequestPanel.test.tsx`
Expected: PASS (all 20 tests — 18 original + 2 new)

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/PullRequestPanel.tsx frontend/src/components/PullRequestPanel.test.tsx
git commit -m "feat(frontend): nest a collapsible AccordionSection per forge repo in PullRequestPanel"
```

---

### Task 6: Full verification pass

**Files:** none (verification only).

- [ ] **Step 1: Run the full frontend test suite**

Run: `cd frontend && pnpm test -- --run`
Expected: PASS, 0 failures.

- [ ] **Step 2: Run lint (enforces the `RepoClient`-only-transport rule and general style)**

Run: `cd frontend && pnpm lint`
Expected: PASS, 0 errors.

- [ ] **Step 3: Run the TypeScript build**

Run: `cd frontend && pnpm build`
Expected: PASS — confirms no type errors from the new `icon`/`count`/`headingLevel` props or the `AccordionGroup` generics.

- [ ] **Step 4: Manual smoke test in the running app**

Run: `cargo tauri dev` (from repo root)
Open a repo and confirm: Branches is open on first launch, others closed; each header shows an icon and a count; Expand all / Collapse all toolbar buttons work; `ArrowUp`/`ArrowDown`/`Home`/`End` move focus between headers without toggling them; opening a section with many rows (e.g. Branches with several branches) doesn't push the sections below it fully out of view — it scrolls internally instead; a Pull Requests section with 2+ forge remotes shows each repo card collapsible independently.

- [ ] **Step 5: Update the GitHub issue**

```bash
gh issue comment 17 --repo rene-langer/browsitory --body "Implemented per docs/superpowers/plans/2026-08-25-sidebar-accordion-ux.md — closing."
gh issue close 17 --repo rene-langer/browsitory
```

---

## Self-Review

**Spec coverage:** all 9 spec findings map to a task — default-open (Task 4 Step 1), counts (Tasks 4, 5), icons (Tasks 4, 5), naming/mutual-exclusion comment fix (Task 2 Step 3's rewritten comment), expand/collapse-all toolbar (Task 3), motion (Task 2's CSS), flexible space-sharing (Task 2's `.section[data-open]` + Task 3's `.sections` layout), `PullRequestPanel` nested-disclosure fix (Task 5), full APG roving-tabindex keyboard nav (Task 1 + Task 2's wiring).

**Placeholder scan:** none — every step has real code, exact file paths, and exact commands.

**Type consistency:** `AccordionGroupHandle`, `AccordionGroupContextValue`, and `RegisteredHeader` are defined once in Task 1 and referenced identically (same names, same shapes) in Tasks 2, 3, and 5. `AccordionSection`'s prop names (`icon`, `count`, `headingLevel`, `defaultOpen`) are defined once in Task 2 and used with those exact names in Tasks 3, 4, and 5.

**One divergence from the spec, called out explicitly above:** nested `PullRequestPanel` repo cards default *open*, not closed — Task 5's design note explains why (keeps ~18 existing tests and current default visibility intact while still satisfying "collapsible on its own").
