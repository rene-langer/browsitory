# Working-tree view: file list, one diff viewer, one options toolbar

Issues #639 (UX-05) and #640 (UX-06), bundle B2 of the 0.10.0 plan (#692). Frontend only.

## Problem

- The working tree renders every changed file as an expanded diff, Changes first and Staged
  below all of them (`UncommittedDiffPane`, `UncommittedFileSection`). With many files the
  Staged group is far down, and there is no compact overview.
- Every `DiffView` renders its own options row (Split, Wrap, whitespace, context, whole file,
  syntax highlighting): it repeats per file, wraps in a 600 px pane, and Split view, the
  per-diff Wrap override and "Show whole file" are local state that resets on remount.

## Design

### Working tree (`UncommittedDiffPane`)

```
[Changes (n)   Stage all · Stage modified · Discard all… · Delete untracked…]
  M  src/a.ts                                   ⋯  +     <- compact rows, own scroll
  U  notes.md                                   ⋯  +
[Staged (n)    Unstage all · Save as patch…]
  A  b.txt                                      ⋯  −     <- own scroll, always on screen
[Unified|Split  Wrap  Whole file  Ignore ws  Show ws  − 3 +  Syntax]   <- one toolbar
[src/a.ts (Modified)            Blame · History · Folder history]
  diff of the current file only (hunk/line staging, image, conflict, blame)
[commit dock, sticky]
```

- **File list.** Both groups render whenever there are changes; an empty group shows a one-line
  placeholder instead of disappearing, so Staged is always visible. Each group's list has a
  capped height and scrolls on its own. A row is a status letter (A, U for untracked, M, D, R,
  T, C for a conflict; its kind is in the title and in the accessible name), the path, the
  "⋯" file menu and the Stage/Unstage toggle. The merge-tool notice stays on its row. The
  multi-select model (#449), bulk actions and discard dialogs are unchanged.
- **One viewer.** `UncommittedFileViewer` shows the current file: a header (path, kind, renamed
  from, Blame, History, Folder history) and the body that a section used to have (diff with
  hunk/line actions, blame, conflict pane, image diff, SVG text toggle). Only this file's diff
  is fetched, so the lazy IntersectionObserver loading and per-file collapse go away. With
  "Expand untracked files" off (#623) an untracked file's contents are not loaded until
  "Show contents" is pressed.
- **Current file.** The first file is current when the pane opens. When the current file leaves
  its group (staged, unstaged, discarded), the file now at the same position in that group
  becomes current, else the first file of the other group.
- **Keyboard.** The listbox keys stay (#449). Down/j on a group's last file moves to the other
  group's first file, Up/k on its first file to the other group's last. Enter moves focus into
  the diff. In the viewer, j/k (no modifier) go to the next/previous file in list order and focus
  stays in the diff.

### One options toolbar (`DiffOptionsToolbar`)

- Rendered once per pane: the working tree (above the viewer), a commit, a stash, a compare and
  the commit file tree browser. `DiffView` no longer renders it.
- Unified | Split is a two-button group with `aria-pressed`. Wrap, whole file, ignore whitespace,
  show whitespace and syntax highlighting are icon buttons with tooltips and the old accessible
  names; the context stepper stays. The row does not wrap.
- What persists: Split and "Show whole file" in `localStorage` (`diff.split`,
  `diff.wholeFile`, per-viewer conveniences like the pane widths), Wrap in the diff settings'
  `wordWrap` (the button sets On or Off; Auto shows as `aria-pressed="mixed"`). The rest already
  persisted in the diff settings.
- `DiffViewOptionsProvider` (in `DiffSettingsProvider`) holds Split and whole file for the app;
  a pane rendered without one (tests) gets its own. `useWholeFile` keeps only the per-file
  "too long to show" fallback. The palette's "Wrap lines" / "Show whole file" act on the same
  shared options.

## Left for a follow-up issue (#729)

- A tree layout and a path filter for the file list (#639).
- The stacked all-diffs view as an option (#639).

## Testing

Vitest with a mocked `RepoClient`: the list and viewer, current-file fallback, group-crossing
keys, the shared toolbar and its persistence. Existing staging, conflict, image, external tool
and multi-select tests are moved onto the viewer. E2E selectors for diffs and stage controls are
updated in `e2e/specs` and `extension/e2e`.
