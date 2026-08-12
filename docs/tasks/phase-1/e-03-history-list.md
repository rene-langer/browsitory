# Task 1.E.03: `HistoryList` component

## Goal

The unified history list: a synthetic "Uncommitted Changes" row on top (badge =
change count), then one row per commit. Up/down arrow keys (or j/k) move the selection; click
also selects. This is the keyboard-nav piece called out as a Phase 1 goal in the design spec.

## Depends on

1.D.02 (`SelectedRow` type, and the `state.status`/`state.log`/`state.selectedRow`/`selectRow`
shape this component's props mirror).

## Interfaces produced

`frontend/src/components/HistoryList.tsx`:
```tsx
export function HistoryList({
  status,
  log,
  selectedRow,
  onSelectRow,
}: {
  status: StatusEntry[];
  log: CommitInfo[];
  selectedRow: SelectedRow;
  onSelectRow: (row: SelectedRow) => void;
}) {
  // ...
}
```
Pure presentational component — no `RepoClient`, no `useAppState` call. Task 1.F.01 renders
`<HistoryList status={appState.state.status} log={appState.state.log}
selectedRow={appState.state.selectedRow} onSelectRow={appState.selectRow} />`.

## Implementation notes

```tsx
import type { KeyboardEvent } from "react";
import type { CommitInfo, StatusEntry } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";

function rowsEqual(a: SelectedRow, b: SelectedRow): boolean {
  if (a === "uncommitted" || b === "uncommitted") {
    return a === b;
  }
  return a.commitId === b.commitId;
}

export function HistoryList({
  status,
  log,
  selectedRow,
  onSelectRow,
}: {
  status: StatusEntry[];
  log: CommitInfo[];
  selectedRow: SelectedRow;
  onSelectRow: (row: SelectedRow) => void;
}) {
  const rows: SelectedRow[] = [
    "uncommitted",
    ...log.map((commit) => ({ commitId: commit.id })),
  ];
  const selectedIndex = rows.findIndex((row) => rowsEqual(row, selectedRow));

  const handleKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    if (event.key === "ArrowDown" || event.key === "j") {
      event.preventDefault();
      onSelectRow(rows[Math.min(selectedIndex + 1, rows.length - 1)]);
    } else if (event.key === "ArrowUp" || event.key === "k") {
      event.preventDefault();
      onSelectRow(rows[Math.max(selectedIndex - 1, 0)]);
    }
  };

  return (
    <ul onKeyDown={handleKeyDown} tabIndex={0}>
      <li
        aria-selected={selectedRow === "uncommitted"}
        onClick={() => onSelectRow("uncommitted")}
      >
        Uncommitted Changes{status.length > 0 && ` (${status.length})`}
      </li>
      {log.map((commit) => (
        <li
          key={commit.id}
          aria-selected={
            typeof selectedRow === "object" && selectedRow.commitId === commit.id
          }
          onClick={() => onSelectRow({ commitId: commit.id })}
        >
          {commit.shortId} {commit.summary}
        </li>
      ))}
    </ul>
  );
}
```
`aria-selected` doubles as both the accessible-selection signal and this task's test hook (query
by role + that attribute, rather than a bespoke `data-*` attribute or CSS class). Arrow-key
navigation clamps at both ends (`Math.min`/`Math.max`) rather than wrapping around.

## TDD requirement

`frontend/src/components/HistoryList.test.tsx` (new file). Fixture: `status` with 2 entries,
`log` with 2 `CommitInfo`s (`id: "aaa111...", shortId: "aaa1111", summary: "second commit"` and
`id: "bbb222...", shortId: "bbb2222", summary: "first commit"`).

- `renders the Uncommitted Changes row with a change-count badge`: assert
  `screen.getByText("Uncommitted Changes (2)")` renders (2 = `status.length`).
- `renders each commit's short id and summary`: assert `screen.getByText(/aaa1111/)` and
  `screen.getByText(/second commit/)` render (and same for the other commit).
- `clicking a commit row calls onSelectRow with that commit's id`: `onSelectRow` as `vi.fn()`.
  Click the first commit's `<li>` (`screen.getByText(/second commit/).closest("li")` or query by
  text within the row). Assert the spy was called with `{ commitId: "aaa111..." }`.
- `ArrowDown moves from Uncommitted Changes to the first commit`: render with
  `selectedRow="uncommitted"`. Get the list (`screen.getByRole("list")`), fire
  `fireEvent.keyDown(list, { key: "ArrowDown" })`. Assert the spy was called with
  `{ commitId: "aaa111..." }` (the first commit, since `rows` is `["uncommitted", ...log]`).
- `ArrowUp from the first row does nothing (clamped, not wrapped)`: render with
  `selectedRow="uncommitted"`, fire `ArrowUp`, assert the spy was called with `"uncommitted"`
  (stays put — `Math.max(selectedIndex - 1, 0)` with `selectedIndex === 0` yields `0` again).
- `ArrowDown from the last commit does nothing (clamped)`: render with
  `selectedRow={{ commitId: "bbb222..." }}` (the last row), fire `ArrowDown`, assert the spy was
  called with `{ commitId: "bbb222..." }` again (unchanged).

Write these six tests first (module doesn't exist), confirm they fail, then implement
`HistoryList.tsx` per the code above and re-run until green.

## Acceptance criteria

- [ ] `pnpm test -- --run` passes (6 new tests + all existing tests still passing).
- [ ] `pnpm build` succeeds.
- [ ] `pnpm lint` clean.
- [ ] Commit: `git add frontend/src/components/HistoryList.tsx frontend/src/components/HistoryList.test.tsx && git commit -m "feat(frontend): add HistoryList component with keyboard navigation"`.

## Out of scope

Multi-branch/graph-line rendering (Phase 2). Virtualized/windowed scrolling for very long lists
(fine at the 300-commit cap Task 1.D.02 uses). Vim-style `gg`/`G` jump-to-start/end, page-up/
page-down. Search/filter within the list.
