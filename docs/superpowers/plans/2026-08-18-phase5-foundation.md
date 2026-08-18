# Phase 5 Foundation (Design System) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish Browsitory's design-token/theming/layout-primitive/icon/motion
system and prove it on the core commit-review loop (`DiffPane` + the
`DiffView`/`BlameView` it renders, `CommitBox`, `CommitGraph`).

**Architecture:** CSS custom properties in a new `frontend/src/styles/tokens.css`
carry color/spacing/radius/shadow/typography/motion tokens for light and dark
themes; a small `frontend/src/lib/theme.ts` resolves and persists which theme is
active. Four new layout primitives
(`frontend/src/components/primitives/{Panel,SplitView,Toolbar,ListRow}.tsx`,
each with a co-located `.module.css`) replace bespoke per-component CSS.
`lucide-react` supplies icons. The three flagship components adopt all of the
above; `frontend/src/index.css` shrinks to resets plus a token import.

**Tech Stack:** React 19, TypeScript, Vite's built-in CSS Modules, `lucide-react`
(new dependency), Vitest + React Testing Library (existing).

**Spec:** `docs/superpowers/specs/2026-08-18-browsitory-phase5-design.md`

## Global Constraints

- No `RepoClient` method, DTO, Tauri command, worker message, or `git-core`
  function is added, removed, or changed in shape by this plan.
- Frontend tests mock `RepoClient`, never `@tauri-apps/api`.
- `pnpm lint`'s `no-restricted-imports` rule
  (`frontend/eslint.config.js:25-37`) must keep passing: no file under
  `src/components/` or `src/state/` may import `@tauri-apps/*`.
- Any new dependency must be a permissive license (MIT, Apache-2.0, ISC, BSD,
  MIT-0), verified with `npm info <package> license`, and recorded in
  `docs/LICENSE_COMPLIANCE.md` in the same commit that adds it.
- `pnpm build`, `pnpm lint`, and `pnpm test -- --run` must pass after every
  task.
- No component's props, `RepoClient` usage, or `git-core`/Tauri-facing
  behavior changes in this plan — visual and structural (markup/CSS) only,
  except the theme-toggle addition in Task 3, which is new UI, not a change
  to existing behavior.

---

### Task 1: Design tokens (light + dark)

**Files:**
- Create: `frontend/src/styles/tokens.css`
- Modify: `frontend/src/index.css:1-30` (import tokens, drop the ad hoc
  `:root` block being replaced)

**Interfaces:**
- Produces: the token custom-property names below — every later task in this
  plan (and the Rollout plan) reads colors/spacing/type/motion only through
  these names, never a raw hex/px value.

- [ ] **Step 1: Write `frontend/src/styles/tokens.css`**

```css
:root {
  /* Color */
  --color-bg: #ffffff;
  --color-bg-subtle: #f6f8fa;
  --color-surface: #ffffff;
  --color-border: #d0d7de;
  --color-text: #1f2328;
  --color-text-muted: #57606a;
  --color-selected-bg: #ede6ff;
  --color-accent: #7c3aed;
  --color-accent-text: #ffffff;
  --color-diff-add-bg: #e6ffed;
  --color-diff-add-text: #1a7f37;
  --color-diff-remove-bg: #ffeef0;
  --color-diff-remove-text: #b31d28;
  --color-diff-context-text: var(--color-text);

  /* Spacing */
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 24px;

  /* Radius */
  --radius-sm: 4px;
  --radius-md: 8px;
  --radius-pill: 999px;

  /* Elevation */
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.08);
  --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.12);

  /* Typography */
  --font-sans: system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
  --font-mono: ui-monospace, "SFMono-Regular", Consolas, "Liberation Mono", monospace;
  --text-xs: 11px;
  --text-sm: 13px;
  --text-md: 14px;
  --text-lg: 16px;
  --text-xl: 20px;
  --leading-dense: 1.3;
  --leading-normal: 1.5;

  /* Motion */
  --motion-duration-fast: 100ms;
  --motion-duration-base: 150ms;
  --motion-easing-standard: cubic-bezier(0.2, 0, 0, 1);
}

@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) {
    --color-bg: #14131a;
    --color-bg-subtle: #1d1c26;
    --color-surface: #1a1922;
    --color-border: #33323f;
    --color-text: #eceaf5;
    --color-text-muted: #a6a3b8;
    --color-selected-bg: #352a5c;
    --color-accent: #a370ff;
    --color-accent-text: #14131a;
    --color-diff-add-bg: #0f2b1a;
    --color-diff-add-text: #4ade80;
    --color-diff-remove-bg: #2b1013;
    --color-diff-remove-text: #f87171;
    --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.4);
    --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.5);
  }
}

:root[data-theme="dark"] {
  --color-bg: #14131a;
  --color-bg-subtle: #1d1c26;
  --color-surface: #1a1922;
  --color-border: #33323f;
  --color-text: #eceaf5;
  --color-text-muted: #a6a3b8;
  --color-selected-bg: #352a5c;
  --color-accent: #a370ff;
  --color-accent-text: #14131a;
  --color-diff-add-bg: #0f2b1a;
  --color-diff-add-text: #4ade80;
  --color-diff-remove-bg: #2b1013;
  --color-diff-remove-text: #f87171;
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.4);
  --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.5);
}
```

- [ ] **Step 2: Import tokens and trim the ad hoc `:root` block in `index.css`**

Replace `frontend/src/index.css:1-30` (the `/* Reset */` block through the
old `:root { ... }` block ending at the `background: var(--bg);` line) with:

```css
@import "./styles/tokens.css";

/* Reset */
*,
*::before,
*::after {
  box-sizing: border-box;
}

html,
body,
#root {
  height: 100%;
  margin: 0;
}

body {
  font-family: var(--font-sans);
  color: var(--color-text);
  background: var(--color-bg);
}
```

Every remaining rule in `index.css` still references the old token names
(`var(--text)`, `var(--bg)`, `var(--border)`, `var(--hunk-bg)`,
`var(--add-bg)`, etc.) — leave those rules as-is for now; Steps 3-11 below
migrate each one into its owning component's CSS Module and delete it from
`index.css` as part of that component's task. Add temporary aliases so the
file still builds in the meantime:

```css
:root {
  --text: var(--color-text);
  --text-muted: var(--color-text-muted);
  --bg: var(--color-bg);
  --border: var(--color-border);
  --selected-bg: var(--color-selected-bg);
  --hunk-bg: var(--color-bg-subtle);
  --add-bg: var(--color-diff-add-bg);
  --add-text: var(--color-diff-add-text);
  --remove-bg: var(--color-diff-remove-bg);
  --remove-text: var(--color-diff-remove-text);
}
```

(This alias block is deleted in Task 11 once every consumer has migrated.)

- [ ] **Step 3: Verify the build**

Run: `cd frontend && pnpm build`
Expected: succeeds, no CSS parse errors.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/styles/tokens.css frontend/src/index.css
git commit -m "feat(frontend): add design tokens with light/dark theming"
```

---

### Task 2: Theme resolution and persistence

**Files:**
- Create: `frontend/src/lib/theme.ts`
- Test: `frontend/src/lib/theme.test.ts`

**Interfaces:**
- Produces: `Theme` (`"light" | "dark"`), `resolveTheme(stored: string | null,
  prefersDark: boolean): Theme`, `loadStoredTheme(): string | null`,
  `persistTheme(theme: Theme): void` — Task 3 (App.tsx wiring) is the
  consumer.

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/src/lib/theme.test.ts
import { describe, expect, it } from "vitest";
import { loadStoredTheme, persistTheme, resolveTheme } from "./theme";

describe("resolveTheme", () => {
  it("uses the stored theme when it is a valid value", () => {
    expect(resolveTheme("dark", false)).toBe("dark");
    expect(resolveTheme("light", true)).toBe("light");
  });

  it("falls back to the OS preference when nothing is stored", () => {
    expect(resolveTheme(null, true)).toBe("dark");
    expect(resolveTheme(null, false)).toBe("light");
  });

  it("falls back to the OS preference when the stored value is invalid", () => {
    expect(resolveTheme("purple", true)).toBe("dark");
  });
});

describe("persistTheme / loadStoredTheme", () => {
  it("round-trips through localStorage and sets the document's data-theme", () => {
    persistTheme("dark");
    expect(loadStoredTheme()).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");

    persistTheme("light");
    expect(loadStoredTheme()).toBe("light");
    expect(document.documentElement.dataset.theme).toBe("light");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd frontend && pnpm test -- --run theme.test.ts`
Expected: FAIL — `theme.ts` does not exist yet.

- [ ] **Step 3: Write `frontend/src/lib/theme.ts`**

```typescript
export type Theme = "light" | "dark";

const STORAGE_KEY = "browsitory-theme";

export function resolveTheme(stored: string | null, prefersDark: boolean): Theme {
  if (stored === "light" || stored === "dark") {
    return stored;
  }
  return prefersDark ? "dark" : "light";
}

export function loadStoredTheme(): string | null {
  return localStorage.getItem(STORAGE_KEY);
}

export function persistTheme(theme: Theme): void {
  localStorage.setItem(STORAGE_KEY, theme);
  document.documentElement.dataset.theme = theme;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd frontend && pnpm test -- --run theme.test.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/theme.ts frontend/src/lib/theme.test.ts
git commit -m "feat(frontend): add theme resolution and persistence"
```

---

### Task 3: `lucide-react` dependency

**Files:**
- Modify: `frontend/package.json` (`dependencies`)
- Modify: `docs/LICENSE_COMPLIANCE.md` (JavaScript table)

**Interfaces:**
- Produces: the `lucide-react` package, importable as
  `import { IconName } from "lucide-react"` — every later task's icon usage
  depends on this.

- [ ] **Step 1: Verify the license**

Run: `npm info lucide-react license`
Expected: `MIT`

- [ ] **Step 2: Install the dependency**

Run: `cd frontend && pnpm add lucide-react`
Expected: `frontend/package.json`'s `dependencies` gains a `"lucide-react":
"^<version>"` line; `frontend/pnpm-lock.yaml` updates.

- [ ] **Step 3: Record it in `docs/LICENSE_COMPLIANCE.md`**

Add a row to the "JavaScript (`npm info <package> license`)" table (after
the `react-dom` row):

```markdown
| lucide-react | MIT | Icon set adopted in Phase 5 for stage/unstage, merge-conflict, branch, stash, tag, worktree, submodule, PR-status, and theme-toggle iconography. |
```

- [ ] **Step 4: Verify the build**

Run: `cd frontend && pnpm build`
Expected: succeeds.

- [ ] **Step 5: Commit**

```bash
git add frontend/package.json frontend/pnpm-lock.yaml docs/LICENSE_COMPLIANCE.md
git commit -m "feat(frontend): add lucide-react for Phase 5 iconography"
```

---

### Task 4: Theme toggle in `App.tsx`

**Files:**
- Modify: `frontend/src/App.tsx:1,17-18,36-48,50-53`

**Interfaces:**
- Consumes: `Theme`, `resolveTheme`, `loadStoredTheme`, `persistTheme` from
  `frontend/src/lib/theme.ts` (Task 2); `Sun`, `Moon` from `lucide-react`
  (Task 3).

- [ ] **Step 1: Add theme state and a toggle button**

In `frontend/src/App.tsx`, change line 1's import and add the theme import:

```typescript
import { useEffect, useState } from "react";
import { Moon, Sun } from "lucide-react";
import { loadStoredTheme, persistTheme, resolveTheme, type Theme } from "./lib/theme";
```

Inside `App()`, right after the `appState` declaration on line 18, add:

```typescript
  const [theme, setTheme] = useState<Theme>(() =>
    resolveTheme(
      loadStoredTheme(),
      window.matchMedia("(prefers-color-scheme: dark)").matches,
    ),
  );
  useEffect(() => {
    persistTheme(theme);
  }, [theme]);

  const themeToggle = (
    <button
      type="button"
      aria-label={theme === "dark" ? "Switch to light theme" : "Switch to dark theme"}
      onClick={() => setTheme(theme === "dark" ? "light" : "dark")}
    >
      {theme === "dark" ? <Sun size={16} /> : <Moon size={16} />}
    </button>
  );
```

Then render `{themeToggle}` next to `<h1>Browsitory</h1>` in both return
branches (the pre-open branch around line 39 and the post-open branch
around line 52), e.g.:

```tsx
<h1>Browsitory{themeToggle}</h1>
```

- [ ] **Step 2: Verify the build and existing tests**

Run: `cd frontend && pnpm build && pnpm test -- --run && pnpm lint`
Expected: all succeed — this task adds new UI but changes no existing
component's props or `RepoClient` usage, so no existing test should need
updating.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/App.tsx
git commit -m "feat(frontend): add theme toggle to App header"
```

---

### Task 5: `Panel` primitive

**Files:**
- Create: `frontend/src/components/primitives/Panel.tsx`
- Create: `frontend/src/components/primitives/Panel.module.css`
- Test: `frontend/src/components/primitives/Panel.test.tsx`

**Interfaces:**
- Produces: `Panel({ title, actions, children }: { title?: string; actions?:
  ReactNode; children: ReactNode })` — used by Tasks 8-9 and by the Rollout
  plan for every form-like component (`RemotePanel`, `TagPanel`,
  `PullRequestPanel`, etc.).

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/src/components/primitives/Panel.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Panel } from "./Panel";

describe("Panel", () => {
  it("renders a title and children", () => {
    render(
      <Panel title="Remotes">
        <p>content</p>
      </Panel>,
    );
    expect(screen.getByRole("heading", { name: "Remotes" })).toBeInTheDocument();
    expect(screen.getByText("content")).toBeInTheDocument();
  });

  it("renders actions next to the title when provided", () => {
    render(
      <Panel title="Remotes" actions={<button>Add</button>}>
        <p>content</p>
      </Panel>,
    );
    expect(screen.getByRole("button", { name: "Add" })).toBeInTheDocument();
  });

  it("renders without a title", () => {
    render(<Panel>{<p>content</p>}</Panel>);
    expect(screen.queryByRole("heading")).not.toBeInTheDocument();
    expect(screen.getByText("content")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd frontend && pnpm test -- --run Panel.test.tsx`
Expected: FAIL — `Panel.tsx` does not exist yet.

- [ ] **Step 3: Write `Panel.tsx` and `Panel.module.css`**

```tsx
// frontend/src/components/primitives/Panel.tsx
import type { ReactNode } from "react";
import styles from "./Panel.module.css";

export function Panel({
  title,
  actions,
  children,
}: {
  title?: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className={styles.panel}>
      {title !== undefined && (
        <header className={styles.header}>
          <h2 className={styles.title}>{title}</h2>
          {actions !== undefined && <div className={styles.actions}>{actions}</div>}
        </header>
      )}
      <div className={styles.body}>{children}</div>
    </section>
  );
}
```

```css
/* frontend/src/components/primitives/Panel.module.css */
.panel {
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-sm);
}

.header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
  padding: var(--space-3) var(--space-4);
  border-bottom: 1px solid var(--color-border);
}

.title {
  margin: 0;
  font-size: var(--text-lg);
  font-weight: 600;
}

.actions {
  display: flex;
  gap: var(--space-2);
}

.body {
  padding: var(--space-4);
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd frontend && pnpm test -- --run Panel.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/primitives/Panel.tsx frontend/src/components/primitives/Panel.module.css frontend/src/components/primitives/Panel.test.tsx
git commit -m "feat(frontend): add Panel layout primitive"
```

---

### Task 6: `SplitView` primitive

**Files:**
- Create: `frontend/src/components/primitives/SplitView.tsx`
- Create: `frontend/src/components/primitives/SplitView.module.css`
- Test: `frontend/src/components/primitives/SplitView.test.tsx`

**Interfaces:**
- Produces: `SplitView({ left, right }: { left: ReactNode; right: ReactNode
  })` — used by Task 10 to replace `App.tsx`'s hand-rolled
  `<div className="app-layout">`.

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/src/components/primitives/SplitView.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { SplitView } from "./SplitView";

describe("SplitView", () => {
  it("renders both panes", () => {
    render(<SplitView left={<div>left pane</div>} right={<div>right pane</div>} />);
    expect(screen.getByText("left pane")).toBeInTheDocument();
    expect(screen.getByText("right pane")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd frontend && pnpm test -- --run SplitView.test.tsx`
Expected: FAIL — `SplitView.tsx` does not exist yet.

- [ ] **Step 3: Write `SplitView.tsx` and `SplitView.module.css`**

```tsx
// frontend/src/components/primitives/SplitView.tsx
import type { ReactNode } from "react";
import styles from "./SplitView.module.css";

export function SplitView({ left, right }: { left: ReactNode; right: ReactNode }) {
  return (
    <div className={styles.splitView}>
      <div className={styles.left}>{left}</div>
      <div className={styles.right}>{right}</div>
    </div>
  );
}
```

```css
/* frontend/src/components/primitives/SplitView.module.css */
.splitView {
  display: flex;
  flex: 1 1 auto;
  min-height: 0;
  overflow: hidden;
}

.left {
  flex: 0 0 300px;
  width: 300px;
  overflow-y: auto;
  border-right: 1px solid var(--color-border);
}

.right {
  flex: 1 1 auto;
  overflow-y: auto;
  padding: var(--space-3) var(--space-4);
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd frontend && pnpm test -- --run SplitView.test.tsx`
Expected: PASS (1 test).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/primitives/SplitView.tsx frontend/src/components/primitives/SplitView.module.css frontend/src/components/primitives/SplitView.test.tsx
git commit -m "feat(frontend): add SplitView layout primitive"
```

---

### Task 7: `Toolbar` primitive

**Files:**
- Create: `frontend/src/components/primitives/Toolbar.tsx`
- Create: `frontend/src/components/primitives/Toolbar.module.css`
- Test: `frontend/src/components/primitives/Toolbar.test.tsx`

**Interfaces:**
- Produces: `Toolbar({ children }: { children: ReactNode })`, rendered with
  `role="toolbar"` — used by Task 9 (`CommitBox`'s Commit/Abort-merge
  buttons) and by the Rollout plan wherever a component has a row of action
  buttons.

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/src/components/primitives/Toolbar.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Toolbar } from "./Toolbar";

describe("Toolbar", () => {
  it("renders its children inside a toolbar landmark", () => {
    render(
      <Toolbar>
        <button>One</button>
        <button>Two</button>
      </Toolbar>,
    );
    const toolbar = screen.getByRole("toolbar");
    expect(toolbar).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "One" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Two" })).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd frontend && pnpm test -- --run Toolbar.test.tsx`
Expected: FAIL — `Toolbar.tsx` does not exist yet.

- [ ] **Step 3: Write `Toolbar.tsx` and `Toolbar.module.css`**

```tsx
// frontend/src/components/primitives/Toolbar.tsx
import type { ReactNode } from "react";
import styles from "./Toolbar.module.css";

export function Toolbar({ children }: { children: ReactNode }) {
  return (
    <div className={styles.toolbar} role="toolbar">
      {children}
    </div>
  );
}
```

```css
/* frontend/src/components/primitives/Toolbar.module.css */
.toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--space-2);
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd frontend && pnpm test -- --run Toolbar.test.tsx`
Expected: PASS (1 test).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/primitives/Toolbar.tsx frontend/src/components/primitives/Toolbar.module.css frontend/src/components/primitives/Toolbar.test.tsx
git commit -m "feat(frontend): add Toolbar layout primitive"
```

---

### Task 8: `ListRow` primitive

**Files:**
- Create: `frontend/src/components/primitives/ListRow.tsx`
- Create: `frontend/src/components/primitives/ListRow.module.css`
- Test: `frontend/src/components/primitives/ListRow.test.tsx`

**Interfaces:**
- Produces: `ListRow({ selected, onClick, onContextMenu, children }: {
  selected: boolean; onClick: () => void; onContextMenu?: (event:
  React.MouseEvent) => void; children: ReactNode })`, rendered as an `<li
  aria-selected={selected}>` — used by Task 10 (`CommitGraph`'s rows).

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/src/components/primitives/ListRow.test.tsx
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ListRow } from "./ListRow";

describe("ListRow", () => {
  it("renders children and reflects the selected state", () => {
    render(
      <ul>
        <ListRow selected={true} onClick={vi.fn()}>
          row content
        </ListRow>
      </ul>,
    );
    const row = screen.getByText("row content").closest("li");
    expect(row).toHaveAttribute("aria-selected", "true");
  });

  it("calls onClick when clicked", () => {
    const onClick = vi.fn();
    render(
      <ul>
        <ListRow selected={false} onClick={onClick}>
          row content
        </ListRow>
      </ul>,
    );
    fireEvent.click(screen.getByText("row content"));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("calls onContextMenu when provided and right-clicked", () => {
    const onContextMenu = vi.fn();
    render(
      <ul>
        <ListRow selected={false} onClick={vi.fn()} onContextMenu={onContextMenu}>
          row content
        </ListRow>
      </ul>,
    );
    fireEvent.contextMenu(screen.getByText("row content"));
    expect(onContextMenu).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd frontend && pnpm test -- --run ListRow.test.tsx`
Expected: FAIL — `ListRow.tsx` does not exist yet.

- [ ] **Step 3: Write `ListRow.tsx` and `ListRow.module.css`**

```tsx
// frontend/src/components/primitives/ListRow.tsx
import type { MouseEvent, ReactNode } from "react";
import styles from "./ListRow.module.css";

export function ListRow({
  selected,
  onClick,
  onContextMenu,
  children,
}: {
  selected: boolean;
  onClick: () => void;
  onContextMenu?: (event: MouseEvent) => void;
  children: ReactNode;
}) {
  return (
    <li
      className={styles.row}
      aria-selected={selected}
      onClick={onClick}
      onContextMenu={onContextMenu}
    >
      {children}
    </li>
  );
}
```

```css
/* frontend/src/components/primitives/ListRow.module.css */
.row {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  padding: var(--space-1) var(--space-3);
  cursor: pointer;
  white-space: normal;
  overflow: hidden;
  transition: background var(--motion-duration-fast) var(--motion-easing-standard);
}

.row:hover {
  background: var(--color-bg-subtle);
}

.row[aria-selected="true"] {
  background: var(--color-selected-bg);
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd frontend && pnpm test -- --run ListRow.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/primitives/ListRow.tsx frontend/src/components/primitives/ListRow.module.css frontend/src/components/primitives/ListRow.test.tsx
git commit -m "feat(frontend): add ListRow layout primitive"
```

---

### Task 9: Reskin `CommitBox`

**Files:**
- Modify: `frontend/src/components/CommitBox.tsx:56-65` (the `return`
  block)
- Create: `frontend/src/components/CommitBox.module.css`

**Interfaces:**
- Consumes: `Panel` (Task 5), `Toolbar` (Task 7).

- [ ] **Step 1: Confirm existing tests still describe the component's
  behavior**

`frontend/src/components/CommitBox.test.tsx` queries by placeholder text
and button text/role — none of that changes in this task, so no test edits
are expected. Re-run it now to record the pre-change baseline:

Run: `cd frontend && pnpm test -- --run CommitBox.test.tsx`
Expected: PASS (existing tests, before the reskin).

- [ ] **Step 2: Reskin the `return` block**

Replace `frontend/src/components/CommitBox.tsx:56-65` with:

```tsx
  return (
    <Panel title="Commit">
      <textarea
        className={styles.textarea}
        value={message}
        onChange={(event) => setMessage(event.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Commit message"
      />
      <Toolbar>
        <button onClick={commitIfReady} disabled={disabled || message.trim() === ""}>
          Commit
        </button>
        {initialMessage !== undefined && <button onClick={onAbortMerge}>Abort merge</button>}
      </Toolbar>
    </Panel>
  );
```

Add the two new imports at the top of the file:

```typescript
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./CommitBox.module.css";
```

- [ ] **Step 3: Write `CommitBox.module.css`**

```css
/* frontend/src/components/CommitBox.module.css */
.textarea {
  width: 100%;
  min-height: 4.5em;
  margin-bottom: var(--space-2);
  padding: var(--space-2);
  font-family: var(--font-sans);
  font-size: var(--text-md);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  color: var(--color-text);
  resize: vertical;
}
```

- [ ] **Step 4: Run the existing tests to verify no regression**

Run: `cd frontend && pnpm test -- --run CommitBox.test.tsx`
Expected: PASS — identical assertions as Step 1, now against the reskinned
markup.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/CommitBox.tsx frontend/src/components/CommitBox.module.css
git commit -m "feat(frontend): reskin CommitBox with Panel/Toolbar primitives and tokens"
```

---

### Task 10: Reskin `CommitGraph` (the commit/stash history list)

**Files:**
- Modify: `frontend/src/components/CommitGraph.tsx:79-155` (the `return`
  block)
- Create: `frontend/src/components/CommitGraph.module.css`
- Modify: `frontend/src/index.css` (remove the migrated
  `.app-layout > ul`, `.stash-row`, `.commit-row`, `.branch-badge` rules —
  lines currently at `108-196` per the pre-Task-1 file; re-locate by
  selector text since Task 1 does not touch these lines)

**Interfaces:**
- Consumes: `ListRow` (Task 8).

Note: `CommitLaneGraphic.tsx`'s hardcoded `LANE_COLORS` array is
categorical branch-lane coloring, not part of the token palette — out of
scope for this task, left unchanged.

- [ ] **Step 1: Confirm existing tests still describe the component's
  behavior**

`frontend/src/components/CommitGraph.test.tsx` queries by role/text
(`getByRole("listitem")`, row text, button labels) — confirm this by
reading the file, then re-run it as a baseline:

Run: `cd frontend && pnpm test -- --run CommitGraph.test.tsx`
Expected: PASS (existing tests, before the reskin).

- [ ] **Step 2: Replace the two `<li className="stash-row">` /
  `<li className="commit-row">` blocks with `ListRow`**

In `frontend/src/components/CommitGraph.tsx`, add the import:

```typescript
import { ListRow } from "./primitives/ListRow";
import styles from "./CommitGraph.module.css";
```

Replace the "Uncommitted Changes" `<li>` (lines 80-85) with:

```tsx
      <ListRow selected={selectedRow === "uncommitted"} onClick={() => onSelectRow("uncommitted")}>
        Uncommitted Changes{status.length > 0 && ` (${status.length})`}
      </ListRow>
```

Replace the stash `<li className="stash-row">` block (lines 86-104) with:

```tsx
      {stashes.map((stash) => (
        <ListRow
          key={stash.commitId}
          selected={typeof selectedRow === "object" && selectedRow.commitId === stash.commitId}
          onClick={() => onSelectRow({ commitId: stash.commitId })}
        >
          <span className={styles.stashMessage}>{stash.message}</span>
          <button
            disabled={pending}
            onClick={(event: MouseEvent<HTMLButtonElement>) => {
              event.stopPropagation();
              onApplyStash(stash.index);
            }}
          >
            Apply
          </button>
          <button
            disabled={pending}
            onClick={(event: MouseEvent<HTMLButtonElement>) => {
              event.stopPropagation();
              onDropStash(stash.index);
            }}
          >
            Drop
          </button>
        </ListRow>
      ))}
```

Replace the commit `<li className="commit-row">` block (lines 105-121)
with:

```tsx
      {commits.map((commit, index) => (
        <ListRow
          key={commit.id}
          selected={typeof selectedRow === "object" && selectedRow.commitId === commit.id}
          onClick={() => onSelectRow({ commitId: commit.id })}
          onContextMenu={(event) => handleContextMenu(event, commit.id)}
        >
          <CommitLaneGraphic layout={commitLayouts[index]} totalLanes={laneCount} />
          {commit.branchRefs.map((ref) => (
            <span key={ref} className={styles.branchBadge}>
              {ref}
            </span>
          ))}
          <span className={styles.commitSummary}>
            {commit.shortId} {commit.summary}
          </span>
        </ListRow>
      ))}
```

- [ ] **Step 3: Write `CommitGraph.module.css`**

```css
/* frontend/src/components/CommitGraph.module.css */
.stashMessage {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
  flex: 1 1 auto;
}

.commitSummary {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
  flex: 1 1 auto;
  font-family: var(--font-mono);
  font-size: var(--text-sm);
}

.branchBadge {
  font-size: var(--text-xs);
  padding: 1px var(--space-2);
  border-radius: var(--radius-pill);
  background: var(--color-bg-subtle);
  color: var(--color-text-muted);
  border: 1px solid var(--color-border);
  flex-shrink: 0;
}
```

- [ ] **Step 4: Remove the migrated rules from `index.css`**

Delete the `.app-layout > ul > li.stash-row`, `.app-layout > ul >
li.stash-row > span`, `.app-layout > ul > li.commit-row`, `.app-layout >
ul > li.commit-row > span.commit-summary`, and `.branch-badge` rule blocks
from `frontend/src/index.css` (their content now lives in
`CommitGraph.module.css`). Leave `.app-layout`, `.app-layout > ul`, and
`.app-layout > ul > li` (the base row rule, still used by the "Uncommitted
Changes" row via `ListRow`'s own module styles — verify visually that
`ListRow.module.css`'s `.row` fully replaces what `.app-layout > ul > li`
provided) — if `.row` already covers padding/cursor/truncation, also
delete `.app-layout > ul > li` and `.app-layout > ul > li[aria-selected="true"]`
here.

- [ ] **Step 5: Run the tests to verify no regression**

Run: `cd frontend && pnpm test -- --run CommitGraph.test.tsx && pnpm build`
Expected: PASS — identical assertions as Step 1, now against the reskinned
markup.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/CommitGraph.tsx frontend/src/components/CommitGraph.module.css frontend/src/index.css
git commit -m "feat(frontend): reskin CommitGraph with ListRow primitive and tokens"
```

---

### Task 11: Reskin `DiffPane`, `DiffView`, `BlameView`, and wire `SplitView` into `App.tsx`

**Files:**
- Modify: `frontend/src/App.tsx:141-171` (replace the `<div
  className="app-layout">` wrapper)
- Create: `frontend/src/components/DiffPane.module.css`
- Modify: `frontend/src/components/DiffView.tsx` (className strings ->
  CSS Module)
- Create: `frontend/src/components/DiffView.module.css`
- Modify: `frontend/src/components/BlameView.tsx` (add table styling)
- Create: `frontend/src/components/BlameView.module.css`
- Modify: `frontend/src/index.css` (remove `.app-layout`, `.diff-hunk-header`,
  `.diff-line*` rules once migrated)

**Interfaces:**
- Consumes: `SplitView` (Task 6).

- [ ] **Step 1: Confirm existing tests still describe behavior**

Run: `cd frontend && pnpm test -- --run DiffPane.test.tsx DiffView.test.tsx BlameView.test.tsx`
Expected: PASS (existing tests, before the reskin).

- [ ] **Step 2: Replace `App.tsx`'s `.app-layout` div with `SplitView`**

In `frontend/src/App.tsx`, add the import:

```typescript
import { SplitView } from "./components/primitives/SplitView";
```

Replace `frontend/src/App.tsx:141-171` (from `<div className="app-layout">`
through its closing `</div>`) with:

```tsx
      <SplitView
        left={
          <CommitGraph
            status={appState.state.status}
            commits={appState.state.commits}
            stashes={appState.state.stashes}
            selectedRow={appState.state.selectedRow}
            pending={repositoryOperationDisabled}
            onSelectRow={appState.selectRow}
            onBranchFromCommit={appState.openCreateBranchDraft}
            onRebaseFromCommit={appState.openRebasePlanner}
            onApplyStash={appState.applyStash}
            onDropStash={appState.dropStash}
          />
        }
        right={
          <DiffPane
            client={tauriRepoClient}
            selectedRow={appState.state.selectedRow}
            status={appState.state.status}
            onStageFile={appState.stageFile}
            onUnstageFile={appState.unstageFile}
            onCommit={appState.commit}
            onSaveStash={appState.saveStash}
            onSelectRow={appState.selectRow}
            onResolveConflict={appState.resolveConflict}
            onResolveAddDeleteConflict={appState.resolveAddDeleteConflict}
            mergeMessage={appState.state.mergeMessage}
            onAbortMerge={appState.abortMerge}
            rebaseProgress={appState.state.rebaseProgress}
            onRebaseContinue={appState.rebaseContinue}
            onRebaseAbort={appState.abortRebase}
          />
        }
      />
```

- [ ] **Step 3: Convert `DiffView.tsx`'s className strings to a CSS Module**

Add the import to `frontend/src/components/DiffView.tsx`:

```typescript
import styles from "./DiffView.module.css";
```

Replace `<div className="diff-hunk-header">` with:

```tsx
<div className={styles.hunkHeader}>
```

Replace the line-rendering `className` with:

```tsx
              <div
                key={lineIndex}
                className={`${styles.line} ${styles[`line${line.origin}`]}`}
              >
```

- [ ] **Step 4: Write `DiffView.module.css`**

```css
/* frontend/src/components/DiffView.module.css */
.hunkHeader {
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  background: var(--color-bg-subtle);
  color: var(--color-text-muted);
  padding: var(--space-1) var(--space-2);
  margin-top: var(--space-2);
}

.line {
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  line-height: var(--leading-dense);
  white-space: pre-wrap;
  padding: 0 var(--space-2);
}

.lineAdd {
  background: var(--color-diff-add-bg);
  color: var(--color-diff-add-text);
}

.lineRemove {
  background: var(--color-diff-remove-bg);
  color: var(--color-diff-remove-text);
}

.lineContext {
  color: var(--color-diff-context-text);
}
```

- [ ] **Step 5: Style `BlameView.tsx`'s table**

Add the import and apply `className` to the existing `<table>`/`<tr>`:

```typescript
import styles from "./BlameView.module.css";
```

```tsx
    <table className={styles.table}>
      <tbody>
        {lines.map((line) => (
          <tr key={line.lineNumber} className={styles.row} onClick={() => onSelectRow({ commitId: line.commitId })}>
```

- [ ] **Step 6: Write `BlameView.module.css`**

```css
/* frontend/src/components/BlameView.module.css */
.table {
  width: 100%;
  border-collapse: collapse;
  font-family: var(--font-mono);
  font-size: var(--text-sm);
}

.row {
  cursor: pointer;
  transition: background var(--motion-duration-fast) var(--motion-easing-standard);
}

.row:hover {
  background: var(--color-bg-subtle);
}
```

- [ ] **Step 7: Write `DiffPane.module.css` and apply it to the file-list
  buttons**

```css
/* frontend/src/components/DiffPane.module.css */
.fileList {
  list-style: none;
  margin: 0;
  padding: 0;
}
```

Add the import to `frontend/src/components/DiffPane.tsx` and apply
`className={styles.fileList}` to the `<ul>` that lists changed files in
`UncommittedDiffPane`/`CommitDiffPane`.

- [ ] **Step 8: Remove migrated rules from `index.css`**

Delete `.app-layout`, `.app-layout > ul`, `.app-layout > ul > li`,
`.app-layout > ul > li[aria-selected="true"]`, `.app-layout > div`,
`.diff-hunk-header`, `.diff-line`, `.diff-line-add`, `.diff-line-remove`,
and `.diff-line-context` from `frontend/src/index.css` — all now live in
`SplitView.module.css` (Task 6) and `DiffView.module.css` (this task).

- [ ] **Step 9: Run the tests to verify no regression**

Run: `cd frontend && pnpm test -- --run && pnpm build`
Expected: PASS — full suite green.

- [ ] **Step 10: Commit**

```bash
git add frontend/src/App.tsx frontend/src/components/DiffPane.tsx frontend/src/components/DiffPane.module.css frontend/src/components/DiffView.tsx frontend/src/components/DiffView.module.css frontend/src/components/BlameView.tsx frontend/src/components/BlameView.module.css frontend/src/index.css
git commit -m "feat(frontend): reskin DiffPane/DiffView/BlameView, wire SplitView into App"
```

---

### Task 12: Retire the temporary token aliases and verify the full suite

**Files:**
- Modify: `frontend/src/index.css` (delete the alias `:root { --text: ...
  }` block added in Task 1 Step 2)

**Interfaces:**
- None — this task only removes now-unused code.

- [ ] **Step 1: Search for any remaining use of the old token names**

Run: `cd frontend && grep -rn "var(--text)\|var(--text-muted)\|var(--bg)\|var(--border)\|var(--selected-bg)\|var(--hunk-bg)\|var(--add-bg)\|var(--add-text)\|var(--remove-bg)\|var(--remove-text)" src/`
Expected: no matches outside the alias block itself (Tasks 9-11 already
migrated every consumer).

If any match remains, migrate that rule to the new token name (`var(--color-*)`)
in place before continuing — don't delete the alias block until this
search is clean.

- [ ] **Step 2: Delete the alias block from `index.css`**

Remove the `:root { --text: var(--color-text); ... }` block added in Task 1
Step 2.

- [ ] **Step 3: Run the full verification suite**

Run: `cd frontend && pnpm build && pnpm lint && pnpm test -- --run`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/index.css
git commit -m "chore(frontend): drop temporary token aliases now every consumer is migrated"
```
