# Browsitory Phase 6 Design — Layout and Information Architecture

## Goal

Phase 5 gave every component a consistent visual language (tokens, `Panel`/
`Toolbar`/`ListRow` primitives, icons). It did not change the app's
*layout*: repo-management panels (branches, remotes, tags, worktrees,
submodules, pull requests) still stack full-width above the commit
history and diff view, pushing the app's primary purpose — reviewing
history and diffs — below the fold. Phase 6 restructures the layout so
history and diff get the full viewport by default, fast keyboard-driven
navigation is possible, and chrome stays minimal.

## Scope and delivery order

Same two-plan split as Phase 5, for the same reason — a foundation of new
primitives, proven working, before every consumer adopts them:

1. **Layout foundation** — new primitives (`Sidebar`, `AccordionSection`,
   `Overlay`) and an extension to the existing `SplitView` primitive
   (resize + collapse, which it does not currently have).
2. **Layout rollout** — migrate `BranchSwitcher`, `RemotePanel`,
   `TagPanel`, `WorktreePanel`, `SubmodulePanel`, `PullRequestPanel` into
   the sidebar; move `RebasePlanner` and `TransferPanel` into `Overlay`;
   recompose `App.tsx` around the new three-pane layout; full
   verification (unit tests, lint, build, GUI E2E).

Out of scope for Phase 6: a command palette (candidate for a later phase),
any `RepoClient`/DTO/Tauri-command/`git-core` change, and any change to
the six migrated components' internal behavior beyond removing their own
`Panel` wrapper.

## Shared architecture

The existing boundary remains unchanged:

`React components/state -> RepoClient -> Tauri command -> Worker -> git-core`

Phase 6 touches only `frontend/src/` — no `RepoClient` method, DTO, Tauri
command, worker message, or `git-core` function is added, removed, or
changed in shape.

## Layout architecture

Three-pane layout built from two nested `SplitView`s:

```
SplitView(
  left:  Sidebar,
  right: SplitView(
    left:  CommitHistory,
    right: Diff,
  ),
)
```

Each `SplitView` gets its own persisted split ratio (see Data flow below),
so the sidebar width and the history/diff split are independently
resizable and remembered across sessions. `Overlay` renders above this
entire layout, conditionally, only while a transient operation is active.

## Components

### New primitives

- **`Sidebar`** (`frontend/src/components/primitives/Sidebar.tsx`) — the
  outer column. Renders its children (six `AccordionSection`s) as a
  scrollable list. No state of its own beyond what `SplitView` already
  tracks for its width.
- **`AccordionSection`**
  (`frontend/src/components/primitives/AccordionSection.tsx`) — a
  collapsible header (title + expand/collapse affordance) plus a content
  region. Takes over the accessible-name and heading role that `Panel`
  currently provides for the six components moving into the sidebar.
  Persists its own open/closed state (see Data flow), defaulting to
  **closed**.
- **`Overlay`** (`frontend/src/components/primitives/Overlay.tsx`) — a
  centered surface with a backdrop, positioned above the rest of the
  layout. Built on native `<dialog>` with `showModal()`, matching the
  existing `<dialog>` elements from Phase 5 — this gets focus-trapping,
  Escape-to-close, and `::backdrop` click-through prevention for free
  from the browser instead of reimplementing them. Token-driven
  surface/border/shadow. Renders (and calls `showModal()`) only while
  active; calls `close()` and unmounts otherwise.

### Extended primitive

- **`SplitView`** (`frontend/src/components/primitives/SplitView.tsx`) —
  currently a static two-column split with no resize or collapse. Gains:
  a draggable divider, an optional `defaultRatio` prop, an optional
  `collapsible` prop, and an `onRatioChange` callback so a consumer can
  persist the ratio. Existing consumer (`App.tsx`'s history/diff split)
  keeps working with default props; behavior for existing usage does not
  change unless the new props are passed.

### Changed components

- `BranchSwitcher`, `RemotePanel`, `TagPanel`, `WorktreePanel`,
  `SubmodulePanel`, `PullRequestPanel` — drop their own `<Panel>`
  wrapper. They render inside an `AccordionSection` now, which owns the
  title/chrome, so wrapping them in `Panel` too would double the card
  chrome. Everything else (state, handlers, `Toolbar`/`ListRow` usage,
  icons) is unchanged.
- `RebasePlanner` and `TransferPanel` — no internal changes. `App.tsx`
  repositions them inside `Overlay` instead of the old panel stack (both
  are currently rendered at the app-shell level today — `RebasePlanner`
  as a conditional sibling, `TransferPanel` inside the panel stack — so
  this is a pure relocation).
- `RebaseProgressPanel` and `ConflictResolutionPane` are **not** touched
  by this phase. They are rendered inside `DiffPane`, not at the
  app-shell level (verified against the current `DiffPane.tsx`, which
  conditionally renders both based on its `rebaseProgress`/`mergeMessage`
  props) — contextual to the diff view itself, not page-level chrome.
  Moving them into `Overlay` would be a real UX change (turning an
  in-context panel into a full-screen interruption), not a pure
  relocation, and is out of scope for this phase.
- `App.tsx` — becomes the layout composer: the nested-`SplitView`
  structure above, with `Overlay` conditionally rendered on top when
  `appState` indicates an active rebase-plan or transfer.

## Data flow and state

- `SplitView`'s split ratio persists to `localStorage`, keyed per
  instance (the sidebar split and the history/diff split get independent
  keys) so each can be resized and remembered independently.
- Each `AccordionSection`'s open/closed state persists to `localStorage`,
  keyed by section id, defaulting to **closed** on first load — so the
  initial view gives history/diff the full remaining width, which is the
  point of this phase.
- `Overlay`'s visibility is derived from existing `appState` fields
  already tracked today for the old panel-stack/sibling rendering
  (`appState.state.rebaseOnto !== null` for `RebasePlanner`,
  `appState.state.transfer !== null` for `TransferPanel`) — no new
  state, only a new rendering location.
- `localStorage` access uses the same plain, unguarded
  `localStorage.getItem`/`setItem` style already established in
  `frontend/src/lib/theme.ts` — no new defensive wrapping introduced by
  this phase.
- No `RepoClient`/DTO/Tauri-command change anywhere in this phase.

## Testing

- New/extended primitives get their own test files: `AccordionSection`
  (render, toggle, persisted-state round-trip via a mocked
  `localStorage`), `Sidebar` (renders its children), `Overlay` (renders
  null when inactive, renders content + backdrop when active),
  `SplitView` (existing tests plus new assertions for the resize/collapse
  behavior, mocked drag interaction, persisted-ratio round-trip).
- The six migrated components' existing test files need updating wherever
  they currently assert against `Panel`'s region role/accessible name —
  those assertions retarget to `AccordionSection`'s heading/region
  instead. No other test assertion should need to change, since none of
  these components' behavior changes.
- Full GUI E2E suite (`e2e/specs/`) re-run at the end of the rollout plan,
  same as Phase 5's final task — expect several selector updates
  wherever a spec locates an element via the old panel-stack DOM
  structure (e.g. any selector assuming a component's `Panel` wraps it
  directly under `<main>`).
- `pnpm build`, `pnpm lint`, and `pnpm test -- --run` must all pass after
  every task in both plans, same Global Constraint as Phase 5.

## Global constraints

(Same pattern as Phase 5's Global Constraints — repeated here since these
plans may execute in separate sessions.)

- No `RepoClient` method, DTO, Tauri command, worker message, or
  `git-core` function is added, removed, or changed in shape by either
  Phase 6 plan.
- Frontend tests mock `RepoClient`, never `@tauri-apps/api`.
- `pnpm lint`'s `no-restricted-imports` rule
  (`frontend/eslint.config.js:25-37`) must keep passing.
- Any new dependency must be permissively licensed and recorded in
  `docs/LICENSE_COMPLIANCE.md` in the same commit that adds it (none is
  expected — this phase only adds new files under `frontend/src/`, no new
  packages).
- `pnpm build`, `pnpm lint`, and `pnpm test -- --run` must pass after
  every task.
- Every touched or new component is checked in both light and dark theme.
- No behavior change to the six migrated components beyond removing their
  own `Panel` wrapper; no behavior change to the two `Overlay`-hosted
  components at all.
