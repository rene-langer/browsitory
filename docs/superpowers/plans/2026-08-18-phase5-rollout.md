# Phase 5 Rollout Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Apply the design-token/theming/layout-primitive/icon/motion system
from the Foundation plan to every remaining frontend component, so the whole
app reads as one visual system.

**Architecture:** Each component gets a co-located `.module.css` and adopts
`Panel`/`Toolbar` (and `ListRow` where the component has a genuinely
selectable list) from `frontend/src/components/primitives/`. No component's
props or `RepoClient` usage changes except where noted as a targeted UX fix.
`App.tsx` gets a final pass once every child is reskinned.

**Tech Stack:** React 19, TypeScript, Vite CSS Modules, `lucide-react`
(already added by the Foundation plan).

**Spec:** `docs/superpowers/specs/2026-08-18-browsitory-phase5-design.md`

**Depends on:** `docs/superpowers/plans/2026-08-18-phase5-foundation.md` must
be fully merged first — every task below imports `Panel`, `Toolbar`, and/or
`ListRow` from `frontend/src/components/primitives/`, and reads tokens from
`frontend/src/styles/tokens.css`, both produced there.

## Global Constraints

(Same as the Foundation plan — repeated here since this plan may execute in
a separate session.)

- No `RepoClient` method, DTO, Tauri command, worker message, or `git-core`
  function is added, removed, or changed in shape by this plan.
- Frontend tests mock `RepoClient`, never `@tauri-apps/api`.
- `pnpm lint`'s `no-restricted-imports` rule
  (`frontend/eslint.config.js:25-37`) must keep passing.
- Any new dependency must be permissively licensed and recorded in
  `docs/LICENSE_COMPLIANCE.md` in the same commit that adds it (none is
  expected in this plan — it only consumes what Foundation already added).
- `pnpm build`, `pnpm lint`, and `pnpm test -- --run` must pass after every
  task.
- Visual/structural changes only, except a targeted UX fix (keyboard focus
  order, unclear loading/error state) where a component is already being
  touched for its reskin — update the affected test when that happens,
  otherwise leave existing test assertions unmodified.
- Every touched component is checked in both light and dark theme
  (`document.documentElement.dataset.theme = "light" | "dark"` in the
  browser, or the OS-level toggle added by the Foundation plan).

---

### Task 1: Reskin `BranchSwitcher`

**Files:**
- Modify: `frontend/src/components/BranchSwitcher.tsx` (wrap the `return`
  block)
- Create: `frontend/src/components/BranchSwitcher.module.css`

**Interfaces:**
- Consumes: `Panel`, `Toolbar` from `frontend/src/components/primitives/`.

- [ ] **Step 1: Baseline the existing tests**

Run: `cd frontend && pnpm test -- --run BranchSwitcher.test.tsx`
Expected: PASS. `BranchSwitcher.test.tsx` queries by role/label text, not
markup structure, so this plan's reskin should not require test edits
unless Step 2 below is applied.

- [ ] **Step 2: Wrap the component in `Panel`, group its action buttons in
  `Toolbar`**

Read `frontend/src/components/BranchSwitcher.tsx`'s current `return` block
in full before editing (it renders a branch `<select>`, switch/delete/
rename controls, a create-branch draft form, and merge/rebase actions).
Wrap the whole thing in `<Panel title="Branches">...</Panel>`, and group
the switch/merge/rebase/delete/rename action buttons that currently sit as
bare siblings into one or more `<Toolbar>` blocks, keeping the `<select>`
and the create-branch draft form outside the toolbar (they are not
actions). Add:

```typescript
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./BranchSwitcher.module.css";
```

- [ ] **Step 3: Write `BranchSwitcher.module.css`**

```css
/* frontend/src/components/BranchSwitcher.module.css */
.select {
  padding: var(--space-1) var(--space-2);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  color: var(--color-text);
  font-size: var(--text-md);
}

.draftForm {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: var(--space-2);
  margin-top: var(--space-2);
}
```

Apply `className={styles.select}` to the branch `<select>` and
`className={styles.draftForm}` to the create-branch draft form's wrapper.

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run BranchSwitcher.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/BranchSwitcher.tsx frontend/src/components/BranchSwitcher.module.css
git commit -m "feat(frontend): reskin BranchSwitcher with Panel/Toolbar primitives"
```

---

### Task 2: Reskin `RebaseProgressPanel`

**Files:**
- Modify: `frontend/src/components/RebaseProgressPanel.tsx`

**Interfaces:**
- Consumes: `Panel`, `Toolbar`.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run RebaseProgressPanel.test.tsx`
Expected: PASS.

- [ ] **Step 2: Wrap in `Panel` and `Toolbar`**

Replace the component's `return (<div>...</div>)` with:

```tsx
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";

export function RebaseProgressPanel({
  currentStep,
  totalSteps,
  disabled,
  onContinue,
  onAbort,
}: {
  currentStep: number;
  totalSteps: number;
  disabled: boolean;
  onContinue: () => void;
  onAbort: () => void;
}) {
  return (
    <Panel title="Rebase in progress">
      <p>
        Step {currentStep} of {totalSteps}
      </p>
      <Toolbar>
        <button onClick={onContinue} disabled={disabled}>
          Continue Rebase
        </button>
        <button onClick={onAbort}>Abort Rebase</button>
      </Toolbar>
    </Panel>
  );
}
```

Note the Abort button intentionally has no `disabled` prop, matching the
current implementation exactly — aborting must stay available even while
`disabled` is true elsewhere, so don't add one here.

- [ ] **Step 3: Verify no regression**

Run: `cd frontend && pnpm test -- --run RebaseProgressPanel.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/RebaseProgressPanel.tsx
git commit -m "feat(frontend): reskin RebaseProgressPanel with Panel/Toolbar primitives"
```

---

### Task 3: Reskin `ConflictResolutionPane`

**Files:**
- Modify: `frontend/src/components/ConflictResolutionPane.tsx`
- Create: `frontend/src/components/ConflictResolutionPane.module.css`

**Interfaces:**
- Consumes: `Panel`, `Toolbar`.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run ConflictResolutionPane.test.tsx`
Expected: PASS.

- [ ] **Step 2: Wrap in `Panel`, style conflict segments with diff tokens**

Read the current `return` block in full. Wrap it in `<Panel title="Resolve
conflict">`. The per-segment "ours/theirs/both" resolution controls become
a `<Toolbar>` per segment. Any rendered conflict-segment text that mirrors
add/remove content should use the same `--color-diff-add-*` /
`--color-diff-remove-*` tokens `DiffView.module.css` uses (Foundation Task
11), for visual consistency between the diff viewer and conflict
resolution.

```typescript
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./ConflictResolutionPane.module.css";
```

- [ ] **Step 3: Write `ConflictResolutionPane.module.css`**

```css
/* frontend/src/components/ConflictResolutionPane.module.css */
.segment {
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  white-space: pre-wrap;
  padding: var(--space-1) var(--space-2);
  margin-bottom: var(--space-2);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
}

.segmentOurs {
  background: var(--color-diff-add-bg);
}

.segmentTheirs {
  background: var(--color-diff-remove-bg);
}
```

Apply `className={styles.segment}` (plus `styles.segmentOurs` /
`styles.segmentTheirs` where the segment's origin side is known) to each
rendered conflict segment.

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run ConflictResolutionPane.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/ConflictResolutionPane.tsx frontend/src/components/ConflictResolutionPane.module.css
git commit -m "feat(frontend): reskin ConflictResolutionPane with Panel/Toolbar and diff tokens"
```

---

### Task 4: Reskin `RebasePlanner`

**Files:**
- Modify: `frontend/src/components/RebasePlanner.tsx`
- Create: `frontend/src/components/RebasePlanner.module.css`

**Interfaces:**
- Consumes: `Panel`, `Toolbar`, `ListRow`.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run RebasePlanner.test.tsx`
Expected: PASS.

- [ ] **Step 2: Wrap in `Panel`; render each plan row via `ListRow`**

Read the current file in full (it renders one row per `Row` in its
`rows` state, each with an action-kind selector and up/down move
controls via `moveRow`). Wrap the whole planner in `<Panel title="Rebase
plan">`. Each row is a candidate for `ListRow` — pass `selected={false}`
(rebase-plan rows aren't a single-selection list; if the component tracks
a currently-focused row, wire that instead) and keep the row's existing
`onClick`/move-button behavior inside `ListRow`'s children. Group the
"Start rebase" / "Cancel" actions at the bottom in a `Toolbar`.

```typescript
import { ListRow } from "./primitives/ListRow";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./RebasePlanner.module.css";
```

- [ ] **Step 3: Write `RebasePlanner.module.css`**

```css
/* frontend/src/components/RebasePlanner.module.css */
.list {
  list-style: none;
  margin: 0;
  padding: 0;
}

.actionSelect {
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  padding: var(--space-1);
  border: 1px solid var(--color-border);
  border-radius: var(--radius-sm);
  background: var(--color-bg);
  color: var(--color-text);
}
```

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run RebasePlanner.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/RebasePlanner.tsx frontend/src/components/RebasePlanner.module.css
git commit -m "feat(frontend): reskin RebasePlanner with Panel/Toolbar/ListRow primitives"
```

---

### Task 5: Reskin `RemotePanel` (and its credential UI)

**Files:**
- Modify: `frontend/src/components/RemotePanel.tsx:151-160` (outer
  wrapper) plus its form sections
- Create: `frontend/src/components/RemotePanel.module.css`
- Modify: `frontend/src/index.css` (remove the migrated `.remote-panel`
  rule block)

**Interfaces:**
- Consumes: `Panel`, `Toolbar`.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run RemotePanel.test.tsx`
Expected: PASS. This component includes credential-saving UI — re-confirm
no test asserts on a token value leaking into the DOM before and after
this task (it shouldn't; this task doesn't touch that logic).

- [ ] **Step 2: Replace `<section className="remote-panel">` with `Panel`**

Replace `frontend/src/components/RemotePanel.tsx:151` (`<section
className="remote-panel" aria-labelledby="remote-panel-heading">`) and its
matching `</section>` with `<Panel title="Remotes">` / `</Panel>` (drop the
now-redundant `aria-labelledby`/heading id — `Panel` already renders an
`<h2>` from `title`). Replace `<ul className="remote-list">` (line 156)
with `<ul className={styles.list}>`. Group each remote's fetch/push/pull
buttons in a `<Toolbar>`.

```typescript
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./RemotePanel.module.css";
```

- [ ] **Step 3: Write `RemotePanel.module.css`**

```css
/* frontend/src/components/RemotePanel.module.css */
.list {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.list li > span {
  color: var(--color-text-muted);
}

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

Apply `className={styles.form}` to each `<form>`/action row and
`className={styles.label}` to each `<label>`, replacing what
`index.css`'s `.remote-panel form` / `.remote-panel label` rules currently
provide.

- [ ] **Step 4: Remove the migrated rule from `index.css`**

`.remote-panel, .tag-panel { ... }`, `.remote-panel h2, ... { ... }`,
`.remote-panel form, .remote-list li, ... { ... }`, and `.remote-panel
label, .tag-panel label { ... }` currently share one selector list with
`.tag-panel` (Task 6 migrates the `.tag-panel` half). Once both this task
and Task 6 are done, delete these shared rule blocks and
`.remote-list`/`.remote-list li > span` entirely from `index.css`.

- [ ] **Step 5: Verify no regression**

Run: `cd frontend && pnpm test -- --run RemotePanel.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/RemotePanel.tsx frontend/src/components/RemotePanel.module.css
git commit -m "feat(frontend): reskin RemotePanel with Panel/Toolbar primitives"
```

---

### Task 6: Reskin `TagPanel`

**Files:**
- Modify: `frontend/src/components/TagPanel.tsx:57-70`
- Create: `frontend/src/components/TagPanel.module.css`
- Modify: `frontend/src/index.css` (finish removing the shared
  `.remote-panel, .tag-panel` rule blocks left pending by Task 5, plus
  `.tag-list`/`.tag-list li > span`)

**Interfaces:**
- Consumes: `Panel`, `Toolbar`.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run TagPanel.test.tsx`
Expected: PASS.

- [ ] **Step 2: Replace `<section className="tag-panel">` with `Panel`**

Same pattern as Task 5: replace `frontend/src/components/TagPanel.tsx:57`
(`<section className="tag-panel" aria-labelledby="tag-panel-heading">`)
with `<Panel title="Tags">`, replace `<ul className="tag-list">` (line 68)
with `<ul className={styles.list}>`, group the create/delete/push actions
in `<Toolbar>`.

```typescript
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./TagPanel.module.css";
```

- [ ] **Step 3: Write `TagPanel.module.css`**

```css
/* frontend/src/components/TagPanel.module.css */
.list {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.list li > span {
  color: var(--color-text-muted);
}

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

- [ ] **Step 4: Delete the now-fully-migrated shared rules from
  `index.css`**

Delete `.remote-panel, .tag-panel { ... }`, `.remote-panel h2, ... { ...
}`, `.remote-panel form, .remote-list li, .tag-panel form, .tag-list li,
.tag-panel section { ... }`, `.remote-panel label, .tag-panel label { ...
}`, `.remote-list`, `.remote-list li > span`, `.tag-list`, and `.tag-list
li > span` entirely.

- [ ] **Step 5: Verify no regression**

Run: `cd frontend && pnpm test -- --run TagPanel.test.tsx RemotePanel.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/TagPanel.tsx frontend/src/components/TagPanel.module.css frontend/src/index.css
git commit -m "feat(frontend): reskin TagPanel, retire shared remote/tag-panel CSS"
```

---

### Task 7: Reskin `TransferPanel`

**Files:**
- Modify: `frontend/src/components/TransferPanel.tsx`
- Create: `frontend/src/components/TransferPanel.module.css`

**Interfaces:**
- Consumes: `Panel`.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run TransferPanel.test.tsx`
Expected: PASS.

- [ ] **Step 2: Replace the `<section>` with `Panel`, add a progress bar**

```tsx
import { Panel } from "./primitives/Panel";
import styles from "./TransferPanel.module.css";

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  return `${(bytes / 1024).toFixed(1)} KB`;
}

export function TransferPanel({ progress }: { progress: TransferProgress | null }) {
  if (progress === null) return null;

  const percent = progress.total > 0 ? Math.round((progress.current / progress.total) * 100) : 0;

  return (
    <Panel title="Transferring">
      <p>{progress.phase}</p>
      <div className={styles.progressTrack} aria-hidden="true">
        <div className={styles.progressFill} style={{ width: `${percent}%` }} />
      </div>
      <p>{progress.current} / {progress.total} objects</p>
      <p>{formatBytes(progress.receivedBytes)} received</p>
    </Panel>
  );
}
```

Keep the original `aria-live="polite" aria-label="Transfer progress"`
attributes on `Panel`'s rendered `<section>` — since `Panel` doesn't
currently accept extra props, add an optional `ariaLive`/`ariaLabel` pair
to `Panel`'s props in `frontend/src/components/primitives/Panel.tsx`
instead of dropping this accessibility behavior:

```typescript
export function Panel({
  title,
  actions,
  children,
  ariaLive,
  ariaLabel,
}: {
  title?: string;
  actions?: ReactNode;
  children: ReactNode;
  ariaLive?: "polite" | "assertive";
  ariaLabel?: string;
}) {
  return (
    <section className={styles.panel} aria-live={ariaLive} aria-label={ariaLabel}>
```

and pass `ariaLive="polite" ariaLabel="Transfer progress"` from
`TransferPanel`. Update `frontend/src/components/primitives/Panel.test.tsx`
to add one assertion that these props pass through:

```typescript
  it("passes through aria-live and aria-label when provided", () => {
    render(
      <Panel ariaLive="polite" ariaLabel="Status">
        <p>content</p>
      </Panel>,
    );
    expect(screen.getByLabelText("Status")).toHaveAttribute("aria-live", "polite");
  });
```

- [ ] **Step 3: Write `TransferPanel.module.css`**

```css
/* frontend/src/components/TransferPanel.module.css */
.progressTrack {
  height: 6px;
  border-radius: var(--radius-pill);
  background: var(--color-bg-subtle);
  overflow: hidden;
  margin: var(--space-1) 0;
}

.progressFill {
  height: 100%;
  background: var(--color-accent);
  transition: width var(--motion-duration-base) var(--motion-easing-standard);
}
```

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run TransferPanel.test.tsx Panel.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/TransferPanel.tsx frontend/src/components/TransferPanel.module.css frontend/src/components/primitives/Panel.tsx frontend/src/components/primitives/Panel.test.tsx
git commit -m "feat(frontend): reskin TransferPanel with a token-driven progress bar"
```

---

### Task 8: Reskin `PullRequestPanel`

**Files:**
- Modify: `frontend/src/components/PullRequestPanel.tsx`
- Create: `frontend/src/components/PullRequestPanel.module.css`

**Interfaces:**
- Consumes: `Panel`, `Toolbar`; `GitPullRequest`, `ExternalLink` icons from
  `lucide-react` for PR-status/open-link affordances.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run PullRequestPanel.test.tsx`
Expected: PASS. Confirm no test asserts against a raw token string
appearing anywhere in rendered output before proceeding (it shouldn't —
`ForgeRepositorySectionProps`'s token fields are write-only per the
component's existing doc comments).

- [ ] **Step 2: Wrap `ForgeRepositorySection` in `Panel`, add PR icons**

Read the current file in full (it renders one `ForgeRepositorySection` per
detected forge repository, each with a token-save form, a "list pull
requests" action, a PR list, and a create-PR form). Wrap each section in
`<Panel title={repository.slug}>` (or the equivalent identifying field —
confirm the exact `ForgeRepository` field name against
`frontend/src/ipc/RepoClient.ts` before writing this). Add a
`<GitPullRequest size={14} />` before each listed PR's title, and an
`<ExternalLink size={14} />` inside the "open PR" action. Group the
token-save/forget actions in one `Toolbar`, and the list/create actions in
another.

```typescript
import { ExternalLink, GitPullRequest } from "lucide-react";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./PullRequestPanel.module.css";
```

- [ ] **Step 3: Write `PullRequestPanel.module.css`**

```css
/* frontend/src/components/PullRequestPanel.module.css */
.prList {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: var(--space-1);
}

.prRow {
  display: flex;
  align-items: center;
  gap: var(--space-1);
}

.form {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: var(--space-2);
  margin: var(--space-2) 0;
}
```

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run PullRequestPanel.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/PullRequestPanel.tsx frontend/src/components/PullRequestPanel.module.css
git commit -m "feat(frontend): reskin PullRequestPanel with Panel/Toolbar and PR icons"
```

---

### Task 9: Reskin `WorktreePanel`

**Files:**
- Modify: `frontend/src/components/WorktreePanel.tsx:54,97`
- Create: `frontend/src/components/WorktreePanel.module.css`

**Interfaces:**
- Consumes: `Panel`, `Toolbar`.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run WorktreePanel.test.tsx`
Expected: PASS.

- [ ] **Step 2: Replace the outer element and list className**

Replace the element at `frontend/src/components/WorktreePanel.tsx:54`
(`className="worktree-panel"`) with `<Panel title="Worktrees">` /
`</Panel>` around the component's existing content, and replace `<ul
className="worktree-list">` (line 97) with `<ul className={styles.list}>`.
Group the create/remove/prune actions in `<Toolbar>`.

```typescript
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./WorktreePanel.module.css";
```

- [ ] **Step 3: Write `WorktreePanel.module.css`**

```css
/* frontend/src/components/WorktreePanel.module.css */
.list {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}

.form {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: var(--space-2);
  margin: var(--space-2) 0;
}
```

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run WorktreePanel.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/WorktreePanel.tsx frontend/src/components/WorktreePanel.module.css
git commit -m "feat(frontend): reskin WorktreePanel with Panel/Toolbar primitives"
```

---

### Task 10: Reskin `SubmodulePanel`

**Files:**
- Modify: `frontend/src/components/SubmodulePanel.tsx:28,39`
- Create: `frontend/src/components/SubmodulePanel.module.css`

**Interfaces:**
- Consumes: `Panel`, `Toolbar`.

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run SubmodulePanel.test.tsx`
Expected: PASS.

- [ ] **Step 2: Replace the outer element and list className**

Replace `<section className="submodule-panel"
aria-labelledby="submodule-panel-heading">` (line 28) with `<Panel
title="Submodules">`, and `<ul className="submodule-list">` (line 39)
with `<ul className={styles.list}>`. Group the init/update actions in
`<Toolbar>`.

```typescript
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./SubmodulePanel.module.css";
```

- [ ] **Step 3: Write `SubmodulePanel.module.css`**

```css
/* frontend/src/components/SubmodulePanel.module.css */
.list {
  display: grid;
  gap: var(--space-1);
  margin: 0;
  padding: 0;
  list-style: none;
}
```

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run SubmodulePanel.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/SubmodulePanel.tsx frontend/src/components/SubmodulePanel.module.css
git commit -m "feat(frontend): reskin SubmodulePanel with Panel/Toolbar primitives"
```

---

### Task 11: Reskin `ReflogPanel`

**Files:**
- Modify: `frontend/src/components/ReflogPanel.tsx:25,41`
- Create: `frontend/src/components/ReflogPanel.module.css`

**Interfaces:**
- Consumes: `Panel`, `Toolbar`, `ListRow` (the reflog reference/entry lists
  are genuinely single-selection: `selectedReference` and per-entry
  restore actions).

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run ReflogPanel.test.tsx`
Expected: PASS.

- [ ] **Step 2: Replace the outer element; use `ListRow` for the reference
  list**

Replace `<section className="reflog-panel"
aria-labelledby="reflog-panel-heading">` (line 25) with `<Panel
title="Reflog">`. Replace `<ul className="reflog-list">` (line 41) — read
the current file to confirm whether this `<ul>` renders `references` or
`entries`; render whichever one is the selectable reference list via
`ListRow`, passing `selected={reference === selectedReference}` and
`onClick={() => onSelectReference(reference)}`. Group each entry's
"Restore" action (and the existing `restoreConfirmation` confirmation
step) inside `<Toolbar>`.

```typescript
import { ListRow } from "./primitives/ListRow";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./ReflogPanel.module.css";
```

- [ ] **Step 3: Write `ReflogPanel.module.css`**

```css
/* frontend/src/components/ReflogPanel.module.css */
.entryList {
  list-style: none;
  margin: 0;
  padding: 0;
}

.entryMeta {
  font-family: var(--font-mono);
  font-size: var(--text-xs);
  color: var(--color-text-muted);
}
```

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run ReflogPanel.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/ReflogPanel.tsx frontend/src/components/ReflogPanel.module.css
git commit -m "feat(frontend): reskin ReflogPanel with Panel/Toolbar/ListRow primitives"
```

---

### Task 12: Reskin `RepoPicker`

**Files:**
- Modify: `frontend/src/components/RepoPicker.tsx`
- Create: `frontend/src/components/RepoPicker.module.css`

**Interfaces:**
- Consumes: `Panel`, `Toolbar`, `ListRow` (the recent-repos list is a
  navigable list — clicking a row opens that repo).

- [ ] **Step 1: Baseline**

Run: `cd frontend && pnpm test -- --run RepoPicker.test.tsx`
Expected: PASS.

- [ ] **Step 2: Wrap in `Panel`; render recent repos via `ListRow`**

Read the current file in full. Wrap its content in `<Panel title="Open a
repository">`. Render each entry in `recentRepos` via `ListRow` with
`selected={false}` (no persistent selection concept here — each row is a
one-shot open action) and `onClick={() => onOpenRepo(path)}`. Keep the
native folder-picker button (`client.pickRepoFolder()`'s trigger) in a
`Toolbar` above the list.

```typescript
import { ListRow } from "./primitives/ListRow";
import { Panel } from "./primitives/Panel";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./RepoPicker.module.css";
```

- [ ] **Step 3: Write `RepoPicker.module.css`**

```css
/* frontend/src/components/RepoPicker.module.css */
.list {
  list-style: none;
  margin: var(--space-2) 0 0;
  padding: 0;
}
```

- [ ] **Step 4: Verify no regression**

Run: `cd frontend && pnpm test -- --run RepoPicker.test.tsx && pnpm build`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/RepoPicker.tsx frontend/src/components/RepoPicker.module.css
git commit -m "feat(frontend): reskin RepoPicker with Panel/Toolbar/ListRow primitives"
```

---

### Task 13: `App.tsx` shell pass and final `index.css` cleanup

**Files:**
- Modify: `frontend/src/App.tsx` (spacing between panels)
- Create: `frontend/src/App.module.css`
- Modify: `frontend/src/index.css` (delete anything now unused)

**Interfaces:**
- None new — this task only adds spacing/layout around already-reskinned
  children.

- [ ] **Step 1: Baseline the full test suite**

Run: `cd frontend && pnpm test -- --run`
Expected: PASS — every component task above already verified its own
tests; this is the final full-suite confirmation.

- [ ] **Step 2: Add vertical rhythm between the top-level panels**

`App.tsx`'s post-open branch renders `BranchSwitcher`, `WorktreePanel`,
`SubmodulePanel`, `ReflogPanel`, `RemotePanel`, `TagPanel`,
`PullRequestPanel`, `TransferPanel`, then the `SplitView`, as direct
siblings inside `<main>`. Wrap that stack in a `<div
className={styles.panelStack}>` and write:

```css
/* frontend/src/App.module.css */
.panelStack {
  display: flex;
  flex-direction: column;
  gap: var(--space-3);
  padding: var(--space-3) var(--space-4);
}
```

```typescript
import styles from "./App.module.css";
```

- [ ] **Step 3: Delete now-unused rules from `index.css`**

Run: `cd frontend && grep -n "^\." src/index.css`

For each remaining selector, confirm (by searching `src/components/` and
`src/App.tsx` for the same string) it is still referenced somewhere; delete
any that is not. By this point `index.css` should contain only the
`@import`, the reset block, and the `body` rule from Foundation Task 1.

- [ ] **Step 4: Full verification**

Run: `cd frontend && pnpm build && pnpm lint && pnpm test -- --run`
Expected: all pass.

- [ ] **Step 5: Manual theme check**

Open the app (`cargo tauri dev` from `crates/tauri-app`, or `pnpm dev` in
`frontend/` against a mock), toggle the theme button added in Foundation
Task 4, and visually confirm every panel (branches, worktrees, submodules,
reflog, remotes, tags, pull requests, transfer, history, diff) renders
legibly in both themes.

- [ ] **Step 6: Run the GUI E2E suite**

Run (from the repo root, per `CLAUDE.md`'s E2E block):
```bash
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
cd e2e && pnpm test
```
Expected: all specs pass — Phase 5 is structural/visual, so no spec should
need updating unless a targeted UX fix in an earlier task changed an
element's role/label, in which case update that spec now.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/App.tsx frontend/src/App.module.css frontend/src/index.css
git commit -m "feat(frontend): finish Phase 5 rollout — App shell spacing and index.css cleanup"
```
