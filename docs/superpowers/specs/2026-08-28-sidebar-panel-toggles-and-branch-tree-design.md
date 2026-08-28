# Sidebar Panel Toggles and Unified Branch/Remote Tree

## Goal

The sidebar accumulated one accordion section per feature area (Branches,
Stash, Worktree, Submodule, Reflog, Remotes, Tags, Pull Requests) as each
phase landed. Two problems fall out of that growth:

1. Most repos never touch worktrees, submodules, or the reflog — those
   sections are permanent visual weight for a large majority of users.
2. Branches and remotes are modeled as two separate, differently-shaped UI
   elements (a dropdown popover vs. a big always-open accordion) even though
   conceptually they're one graph: local branches, and the remote-tracking
   branches attached to each remote.

This design (a) lets a user hide sidebar sections they don't use, and (b)
replaces the `BranchSwitcher` dropdown and the `RemotePanel` accordion with a
single always-expanded tree: local branches, and one folder per remote
containing that remote's branches — the layout used by fast, keyboard-driven
git clients generally, not tied to any single product's branding.

## Shared architecture

No change to the existing boundary:

`React components/state -> RepoClient -> Tauri command -> Worker -> git-core`

Both features are frontend-only (`frontend/src/`). No `RepoClient` method,
DTO, Tauri command, worker message, or `git-core` function changes shape —
`BranchTree` calls the exact same `AppState` methods `BranchSwitcher` and
`RemotePanel` call today (`onSwitchBranch`, `onAddRemote`,
`onListRemoteBranches`, etc.); only the component(s) issuing those calls
change.

## Part 1: Panel visibility toggles

### Scope

Toggleable: Stash, Worktree, Submodule, Reflog, Tags, Pull Requests.
Not toggleable: the Branches tree (Part 2) — it's the core section and
always shown.

### Persistence

New `frontend/src/state/useSidebarPanelVisibility.ts`, following the same
plain-`localStorage` pattern already used for theme (`lib/theme.ts`) and
per-section open/closed state (`AccordionSection.tsx`'s `loadOpen`/
`setOpenState`) — no backend config involved, this is a local UI
preference, not repo or workspace state.

- Storage key: `sidebar.panels`, value a JSON object mapping a fixed set of
  panel ids (`"stash" | "worktree" | "submodule" | "reflog" | "tags" |
  "pullRequests"`) to `boolean`. Missing keys (new panel added later, or
  first run) default to `true`.
- Hook returns `{ visibility, setPanelVisible }`; `setPanelVisible` updates
  React state and writes the whole map back to `localStorage` on each
  change (mirrors `theme.ts`'s `persistTheme`).

### UI

- `Sidebar.tsx` toolbar gets a third button (gear icon, `lucide-react`
  `Settings` or `SlidersHorizontal`) alongside the existing expand-all/
  collapse-all buttons.
- Clicking it opens a small popover (same floating/positioning approach as
  `BranchSwitcher`'s current popover, since that code moves into
  `BranchTree` context menus in Part 2 and is otherwise about to be
  orphaned) listing the six toggleable panels as checkboxes, backed by the
  hook above.
- `App.tsx` wraps each toggleable panel's JSX in `visibility.<id> && (...)`
  so a hidden panel unmounts entirely (state inside it — e.g. an open
  create-tag draft — is discarded on hide, same as any other conditionally
  rendered component; nothing here needs to survive a hide/show cycle).

### Testing

`useSidebarPanelVisibility.test.ts`: defaults, persistence round-trip,
missing-key defaulting. `Sidebar.test.tsx` gets coverage for the new gear
popover rendering and toggling checkboxes. No `git-core`/backend tests
needed — nothing crosses the IPC boundary.

## Part 2: Unified Branches/Remotes tree

### Component

New `frontend/src/components/BranchTree.tsx` replaces both
`BranchSwitcher.tsx` and `RemotePanel.tsx` in `App.tsx`'s sidebar, and both
files are deleted (along with their `.module.css` and `.test.tsx`) once
`BranchTree` absorbs their behavior. It renders as one `AccordionSection`
titled "Branches", always mounted (Part 1 doesn't cover it).

### Tree structure

```
Branches
├─ Local
│   ├─ main            (current — marked)
│   ├─ feat/foo
│   └─ ...
├─ origin
│   ├─ main
│   ├─ feat/foo
│   └─ ...
└─ upstream
    └─ ...
```

- **Local** node: flat list from `branches: BranchInfo[]` (unchanged prop,
  from `appState.state.branches`), current branch visually marked exactly
  as `BranchSwitcher` marks it today.
- One folder per entry in `remotes: RemoteInfo[]`. Each folder's branch list
  loads lazily on first expand via `onListRemoteBranches(remoteName)` —
  the same lazy-fetch already in `RemotePanel`, just triggered by expanding
  a tree folder instead of an accordion sub-section. Collapsed/expanded
  state per folder persists via the same `storageKey` convention
  `AccordionSection` uses (`branchtree.remote.<name>`), so it doesn't reset
  on every refresh.
- Folders and rows use the existing `ListRow` primitive; nesting is a CSS
  indent level, no new list primitive needed.

### Actions: context menu + command palette, not inline buttons

Per the agreed design, mutating actions do **not** get their own visible
buttons in the tree (unlike the current `RemotePanel`, which has permanent
add/edit/remove chrome). They live in:

1. A right-click context menu on the relevant row, using a new shared
   `frontend/src/components/primitives/ContextMenu.tsx`.
2. Equivalent entries in `lib/commands.ts` (command palette) — several
   already exist (`Pull`, `Fetch <remote>`, `Push to <remote>`,
   `Push all tags to <remote>`) and stay; new ones are added for actions
   that today only exist as `RemotePanel` form buttons (add remote, edit
   remote, manage credentials).

`ContextMenu` is extracted from the bespoke right-click menu already
implemented ad hoc in `CommitGraph.tsx` (local `contextMenu` state,
`handleContextMenu`, positioned `x`/`y` popup, close-on-mouseleave/escape).
That logic is about to be needed a second time in `BranchTree`; extracting
it now avoids a second hand-rolled copy. `ContextMenu` takes `{ x, y, items,
onClose }` where `items` is a list of `{ label, onSelect, disabled?,
destructive? }`; `CommitGraph.tsx` is refactored to consume it, and its
existing tests are expected to keep passing unchanged since this changes
implementation, not markup or behavior.

Menu contents by row type:

- **Local branch row**: Checkout, New Branch from here, Rename, Delete
  (force-delete confirmation dialog — reuses `ConfirmDialog` exactly as
  `BranchSwitcher` does today), Merge into current, Push to `<upstream
  remote>` (only if the branch has an upstream), Set upstream…
- **Remote branch row**: Checkout (creates a local tracking branch), Set as
  upstream for current branch, Copy branch name.
- **Remote folder row** (e.g. "origin"): Fetch, Push current branch here,
  Edit remote (opens a dialog — rename/URL edit, reusing the existing form
  markup from `RemotePanel.tsx`), Remove remote (confirmation dialog,
  reusing the existing "clear upstreams too?" choice), Manage credentials
  (HTTPS username/token save/forget, set auth mode — reuses the existing
  dialog from `RemotePanel.tsx`).
- **"Branches" section header**: a "+" button (via the existing `Toolbar`
  primitive, same as other sections' add buttons) opens a small menu with
  "New Branch" and "Add Remote" — the latter reuses `RemotePanel`'s
  `deriveRemoteName` helper and add-remote form/validation.

Pull-outcome handling (`pendingPull` → merge-or-rebase choice dialog) is
unchanged in behavior and wiring (`onMergePull`/`onRebasePull`/
`onCancelPull` stay exactly as `App.tsx` passes them today) — it's a global
overlay independent of which component triggers a pull, so it just moves
from being passed to `RemotePanel` to being passed to `BranchTree`.

### Error handling

No new error paths — every action reuses the existing `AppState` methods,
which already surface failures the way `BranchSwitcher`/`RemotePanel` do
today (inline error strings, rejected promises shown via `InlineError`,
etc.). The context-menu/dialog wrapper is presentational only.

### Testing

- `BranchTree.test.tsx` replaces `BranchSwitcher.test.tsx` and absorbs
  `RemotePanel.test.tsx`'s coverage: switch/create/delete/rename branch,
  merge, add/rename/remove remote, credentials save/forget, set/clear
  upstream, list remote branches lazily, fetch/push/pull, pull-outcome
  merge-vs-rebase choice — all driven through the new context menus and
  the section-header "+" menu instead of the old inline forms.
- `ContextMenu.test.tsx` (new): positioning, item rendering, disabled
  items, close on escape/outside-click/item-select.
- `CommitGraph.test.tsx`: no behavioral changes expected; run as
  regression coverage for the extraction.

## Out of scope

- No changes to `git-core`, Tauri commands, or `RepoClient` — confirmed
  above, restated here because it's easy to assume a tree-view rewrite
  needs new backend calls; it doesn't, since remote-branch listing, add/
  remove-remote, and credential management are already exposed.
- No multi-select or drag-and-drop in the tree.
- No workspace- or repo-scoped panel visibility (Part 1) — it's a single
  global `localStorage` preference, same scope as the theme setting.
