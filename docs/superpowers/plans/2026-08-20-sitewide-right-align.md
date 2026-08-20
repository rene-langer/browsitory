# Sitewide right-aligned row actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Right-align every per-row action-button group sitewide (Sublime Merge style) so rows read as `label ......... [buttons]` instead of buttons packed against the label.

**Architecture:** `Toolbar` (the shared primitive every row-action group already renders through) gets `margin-left: auto` added to its own CSS class. In a `display: flex` row, that alone pushes the `Toolbar` — and only the `Toolbar` — to the row's trailing edge, leaving everything before it left-packed; no `justify-content` or per-child `flex` needed. Rows that already use the `ListRow` primitive (`.row` is already `display: flex`) get this for free. Rows that are still plain `<li>` (block layout) need one CSS-only addition per component: a `.list li { display: flex; flex-wrap: wrap; ... }` rule, so `Toolbar`'s `margin-left: auto` has a flex context to act inside.

**Tech Stack:** CSS Modules (no new JSX, no new tests — every change here is presentation-only; existing tests already assert on button/label *text*, not layout, so they keep passing unchanged).

**Spec:** `docs/superpowers/specs/2026-08-20-hunk-staging-design.md` (section 6). Section 6's own text guessed at "flex:1 on the label" as the mechanism — the actual audit below (Task 1) found `margin-left: auto` on `Toolbar` itself is simpler and needs zero per-component JSX changes; only `Toolbar.module.css` plus one CSS rule per still-block-layout panel. Also: the spec's per-component list (`WorktreePanel`, `TagPanel`, `RemotePanel`, `ReflogPanel`, `PullRequestPanel`) missed `SubmodulePanel`, which has the exact same plain-`<li>`-plus-`Toolbar` shape and needs the identical fix — added as Task 5 below. `PullRequestPanel`'s row (`.prRow`) doesn't use `Toolbar` at all (its one button is inline text, not a row-action group), so it needs no change — confirmed by inspection, not guessed.

## Global Constraints

- Every change in this plan is CSS-only (or a single CSS rule addition) — no component JSX changes, no new test files. TDD's red/green cycle doesn't apply to a property value change with no behavioral assertion to write; instead, each task's "test" is: full suite still green (proves no accidental text/structure regression) plus a manual visual check in the running app.
- `ListRow`-based rows (already `display: flex` via `.row` in `ListRow.module.css`) need **no per-component change** — `Toolbar`'s own `margin-left: auto` (Task 1) is sufficient. This covers `BranchSwitcher`'s stash rows and `ReflogPanel`'s entries.
- Plain-`<li>` rows (`WorktreePanel`, `TagPanel`, `RemotePanel`, `SubmodulePanel`) each need one `.list li { display: flex; ...}` rule added to that component's own `*.module.css` — Tasks 2-5.
- Top-level action bars that aren't row actions (`CommitBox`, the Stash/Push/Pull buttons, `TagPanel`'s "Push tags" section, `RemotePanel`'s "Add remote"/"Upstream" sections) are untouched — `Toolbar`'s `margin-left: auto` only pushes it within whatever flex row contains it, and none of those bars sit inside a `display: flex` row today, so this change has no effect on them.
- `DiffView`'s per-hunk `Toolbar` (added by the separate hunk-based-staging plan, `docs/superpowers/plans/2026-08-20-hunk-based-staging.md`) already right-aligns via its own `justify-content: space-between` on `.hunkHeader` — untouched by this plan, no conflict (that mechanism and this one are independent and compatible: `margin-left: auto` on `Toolbar` is a no-op there since `.hunkHeader`'s `space-between` already pins it to the end).

---

## Task 1: `Toolbar` — the core right-align mechanism

**Files:**
- Modify: `frontend/src/components/primitives/Toolbar.module.css`

**Interfaces:**
- Produces: `.toolbar` now includes `margin-left: auto`, applied to every existing `<Toolbar>` consumer sitewide

- [ ] **Step 1: Make the change**

In `frontend/src/components/primitives/Toolbar.module.css`:

```css
.toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--space-2);
  margin-left: auto;
}
```

(Only the new `margin-left: auto;` line is added — everything else in the rule is unchanged.)

- [ ] **Step 2: Run the full frontend test suite**

Run: `cd frontend && npx vitest run`
Expected: PASS, no regressions — every existing test asserts on text/role/click behavior, none on layout, so nothing here should break

- [ ] **Step 3: Typecheck and lint**

Run: `cd frontend && npx tsc --noEmit -p tsconfig.app.json && npx eslint src/components/primitives/Toolbar.module.css`
Expected: no errors (the CSS lint call is a no-op/harmless if eslint doesn't lint `.css` — the point is confirming nothing else broke)

- [ ] **Step 4: Manual visual check**

Launch the app (dev server, per this repo's `run` skill/CLAUDE.md instructions) and open the sidebar's Branches section with at least one stash present, and the Reflog section with at least one entry. Confirm: the stash message / reflog metadata sit left, Apply/Drop or Restore buttons sit right, with visible space between — not still packed together. (Plain-`<li>` panels won't show any change yet — that's Tasks 2-5.)

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/primitives/Toolbar.module.css
git commit -m "feat(frontend): right-align Toolbar within its flex row"
```

---

## Task 2: `WorktreePanel` — flex row for plain `<li>`

**Files:**
- Modify: `frontend/src/components/WorktreePanel.module.css`

**Interfaces:**
- Consumes: `Toolbar`'s `margin-left: auto` from Task 1

- [ ] **Step 1: Make the change**

In `frontend/src/components/WorktreePanel.module.css`, add a new rule (there is currently no `li`-specific rule in this file at all):

```css
.list li {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-1) var(--space-2);
}
```

- [ ] **Step 2: Run the full frontend test suite**

Run: `cd frontend && npx vitest run src/components/WorktreePanel.test.tsx`
Expected: PASS, no regressions

- [ ] **Step 3: Manual visual check**

Open the sidebar's Worktrees section with at least one linked worktree. Confirm: path/head/Locked/Prunable labels sit left, Open/Remove buttons sit right.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/WorktreePanel.module.css
git commit -m "feat(frontend): right-align WorktreePanel row actions"
```

---

## Task 3: `TagPanel` — flex row for plain `<li>`

**Files:**
- Modify: `frontend/src/components/TagPanel.module.css`

**Interfaces:**
- Consumes: `Toolbar`'s `margin-left: auto` from Task 1

- [ ] **Step 1: Make the change**

In `frontend/src/components/TagPanel.module.css`, extend the existing `.list li > span` block with a sibling rule right after it:

```css
.list li > span {
  color: var(--color-text-muted);
}

.list li {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-1) var(--space-2);
}
```

- [ ] **Step 2: Run the full frontend test suite**

Run: `cd frontend && npx vitest run src/components/TagPanel.test.tsx`
Expected: PASS, no regressions

- [ ] **Step 3: Manual visual check**

Open the sidebar's Tags section with at least one tag. Confirm: checkbox/name/Annotated-or-Lightweight label sit left, the Delete button sits right.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/TagPanel.module.css
git commit -m "feat(frontend): right-align TagPanel row actions"
```

---

## Task 4: `RemotePanel` — flex row for plain `<li>`

**Files:**
- Modify: `frontend/src/components/RemotePanel.module.css`

**Interfaces:**
- Consumes: `Toolbar`'s `margin-left: auto` from Task 1

- [ ] **Step 1: Make the change**

In `frontend/src/components/RemotePanel.module.css`, extend the existing `.list li > span` block with a sibling rule right after it:

```css
.list li > span {
  color: var(--color-text-muted);
}

.list li {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-1) var(--space-2);
}
```

- [ ] **Step 2: Run the full frontend test suite**

Run: `cd frontend && npx vitest run src/components/RemotePanel.test.tsx`
Expected: PASS, no regressions

- [ ] **Step 3: Manual visual check**

Open the sidebar's Remotes section with at least one remote configured. Confirm: name/Fetch-URL/Push-URL labels sit left, the Fetch/Push/Edit/Credentials/Remove button group sits right (this row has 5 buttons — confirm it still reads cleanly wrapped, not overflowing; if it looks cramped even right-aligned, that's a separate follow-up, not a blocker for this plan since the fix here is exactly what was scoped).

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/RemotePanel.module.css
git commit -m "feat(frontend): right-align RemotePanel row actions"
```

---

## Task 5: `SubmodulePanel` — flex row for plain `<li>` (not in the original spec list, same shape)

**Files:**
- Modify: `frontend/src/components/SubmodulePanel.module.css`

**Interfaces:**
- Consumes: `Toolbar`'s `margin-left: auto` from Task 1

- [ ] **Step 1: Make the change**

In `frontend/src/components/SubmodulePanel.module.css`, add a new rule right after the existing `.list` block:

```css
.list li {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--space-1) var(--space-2);
}
```

- [ ] **Step 2: Run the full frontend test suite**

Run: `cd frontend && npx vitest run src/components/SubmodulePanel.test.tsx`
Expected: PASS, no regressions

- [ ] **Step 3: Manual visual check**

Open the sidebar's Submodules section with at least one submodule listed. Confirm: path/URL/gitlink/init-state labels sit left, the Initialize/Update buttons sit right.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/SubmodulePanel.module.css
git commit -m "feat(frontend): right-align SubmodulePanel row actions"
```

---

## Final Verification

- [ ] Run `cd frontend && npx tsc --noEmit -p tsconfig.app.json && npx eslint . && npx vitest run` — all green
- [ ] Run `cd e2e && npx wdio run wdio.conf.ts` — all green (these specs click/read by text, not layout, so should be unaffected)
- [ ] Manually launch the app and click through every touched section (Branches+stash, Worktrees, Submodules, Reflog, Remotes, Tags) at both a wide and a narrow sidebar width (drag the `SplitView` sidebar-width handle) — confirm the right-aligned buttons wrap sensibly rather than overflowing or clipping at narrow widths
- [ ] Confirm `PullRequestPanel` is visually unchanged (expected — it was never touched)
