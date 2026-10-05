# Multi-select commits + interactive rebase

## Goal

Ctrl/Cmd+click commits in the graph to build a selection, then squash, fixup, drop, or open the
interactive rebase planner on it. Gap vs Sublime Merge.

## Current state

- `CommitGraph.tsx`: single `selectedRow` plus Shift-only contiguous `squashRange`, offered from the
  context menu only, only for linear ranges (`isSquashableRange`), ending in
  `onSquashCommits(ontoId, squashIds)`.
- `RebasePlanner.tsx` already supports Pick/Reword/Edit/Squash/Fixup/Drop and reordering, with a
  `presetSquashIds` prop.
- Backend (`git-core/rebase.rs`, `startRebase(onto, plan[])`) needs no change.

## Decisions

- Non-contiguous selection (A, C, E with B, D between): planner shows the **full span** from the
  oldest selected commit's parent to HEAD. Selected commits highlighted and preset; the rest stay
  Pick.
- Quick actions (Squash, Fixup, Drop) **always open the planner** with the action preset. No direct
  execution path.

## Design

### 1. Selection model (`CommitGraph`)

- Replace `squashAnchorIndex` / `squashRange` with `selectedIds: Set<string>` plus an anchor. The
  primary row (`selectedRow`) still drives the diff pane.
- Click replaces selection. Ctrl/Cmd+click toggles. Shift+click selects range from anchor.
  Shift+Arrow keeps working. Esc collapses to the focused commit. Non-adjacent selection is
  mouse-only: the list has no separate focus cursor, so there is no keyboard toggle (follow-up).
- Highlight selected rows; `aria-multiselectable` on the listbox.
- Pure helper in `lib/` computes the rebase base: parent of the oldest selected commit, in graph
  order.

### 2. Actions

- With 2+ commits selected, context menu and a small action bar show: Interactive rebase…, Squash,
  Fixup, Drop.
- Base must be a linear first-parent chain from HEAD to the oldest selected commit. If the span
  includes a merge commit or a selected commit is not reachable from HEAD, actions are disabled with
  an explanatory tooltip. Replaces the `isSquashableRange` check.
- Squash and Fixup fold into the plan entry *before* them, so they are enabled only when the
  selection is one unbroken first-parent run; otherwise disabled with a tooltip. Drop and
  Interactive rebase… accept any selection.
- Squash/Fixup/Drop open the planner with the action preset on selected commits. Interactive
  rebase… opens it with nothing preset.
- `openSquashPlanner(ontoId, ids)` becomes `openRebaseSelection(onto, preset)`;
  `squashPreset: Set` becomes `rebasePreset: Map<id, "Squash" | "Fixup" | "Drop">`.
- Squash/Fixup fold into the preceding commit. If the oldest selected commit would be Squash/Fixup
  it stays Pick and the action applies to the rest (matches current squash flow).

### 3. Planner (`RebasePlanner`)

- If a preset commit is not in `commitsSince(onto)` (e.g. selected from another branch), the
  planner shows an alert and disables Start.
- `presetSquashIds` becomes `presetActions`. Selected commits highlighted and preset; others Pick.
- Reorder, Reword, Edit, conflict flow and `RebaseProgressPanel` unchanged.

## Testing

- Rust: none new (`git-core/tests/rebase.rs` covers actions).
- Vitest: selection reducer and base helper unit tests; `CommitGraph` tests for Ctrl-click toggle,
  Shift range, disabled-on-merge, menu entries; `RebasePlanner` tests for preset map and the
  oldest-commit squash guard.
- E2E (Tauri): select three non-contiguous commits, squash, assert resulting history.
- Docs: `USER_GUIDE.md` (Rebase section) and `CHANGELOG.md` (required by pre-push hook).
