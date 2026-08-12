# Task 1.E.02: `DiffView` component

## Goal

A shared, dumb diff renderer: takes `DiffHunk[]`, renders hunk headers and per-line add/remove/
context styling. Used by both branches of Task 1.E.04's `DiffPane` (working-tree diffs and
commit diffs alike) — no staging controls, no data fetching, just rendering.

## Depends on

1.D.01 (only for the `DiffHunk`/`DiffLine`/`DiffLineOrigin` types — no runtime dependency).

## Interfaces produced

`frontend/src/components/DiffView.tsx`:
```tsx
export function DiffView({ hunks }: { hunks: DiffHunk[] }) {
  // ...
}
```
Task 1.E.04 renders `<DiffView hunks={...} />` for whichever file is currently selected within
`DiffPane`.

## Implementation notes

```tsx
import type { DiffHunk, DiffLineOrigin } from "../ipc/RepoClient";

export function DiffView({ hunks }: { hunks: DiffHunk[] }) {
  if (hunks.length === 0) {
    return <p>No differences</p>;
  }

  return (
    <div>
      {hunks.map((hunk, hunkIndex) => (
        <div key={hunkIndex}>
          <div className="diff-hunk-header">
            @@ -{hunk.oldStart},{hunk.oldLines} +{hunk.newStart},{hunk.newLines} @@
          </div>
          <pre>
            {hunk.lines.map((line, lineIndex) => (
              <div
                key={lineIndex}
                className={`diff-line diff-line-${line.origin.toLowerCase()}`}
              >
                <span aria-hidden="true">{originPrefix(line.origin)}</span>
                {line.content}
              </div>
            ))}
          </pre>
        </div>
      ))}
    </div>
  );
}

function originPrefix(origin: DiffLineOrigin): string {
  switch (origin) {
    case "Add":
      return "+";
    case "Remove":
      return "-";
    case "Context":
      return " ";
  }
}
```
Hunks/lines have no stable identity of their own (they're positional data, not entities with
IDs), so array-index `key`s are the correct choice here — this is one of the narrow cases where
index keys are fine, since the list is never reordered or filtered in place, only replaced
wholesale when `hunks` changes.

## TDD requirement

`frontend/src/components/DiffView.test.tsx` (new file):

- `renders each line's content`: a fixed `DiffHunk[]` with one hunk containing a `Context` line
  `"unchanged"`, a `Remove` line `"old value"`, and an `Add` line `"new value"`. Assert
  `screen.getByText(/unchanged/)`, `screen.getByText(/old value/)`, `screen.getByText(/new
  value/)` all render (use a regex or `{ exact: false }` match since each line also renders its
  `+`/`-`/` ` prefix span alongside the text node).
- `added and removed lines get distinct CSS classes`: same fixture. Assert the removed line's
  containing element has class `diff-line-remove` and the added line's has `diff-line-add`
  (query via `screen.getByText("old value", { exact: false }).closest(".diff-line")` or similar,
  then check `classList`/`className`).
- `renders a message when there are no hunks`: render with `hunks={[]}`, assert
  `screen.getByText("No differences")`.

Write these three tests first (module doesn't exist), confirm they fail, then implement
`DiffView.tsx` per the code above and re-run until green.

## Acceptance criteria

- [ ] `pnpm test -- --run` passes (3 new tests + all existing tests still passing).
- [ ] `pnpm build` succeeds.
- [ ] `pnpm lint` clean.
- [ ] Commit: `git add frontend/src/components/DiffView.tsx frontend/src/components/DiffView.test.tsx && git commit -m "feat(frontend): add DiffView component"`.

## Out of scope

Word-level highlighting within a line (Phase 1 is line-level only, per the design spec). Syntax
highlighting. Collapsible/expandable hunks. Line numbers in the gutter (the `+`/`-`/` ` prefix is
the only per-line marker this phase).
