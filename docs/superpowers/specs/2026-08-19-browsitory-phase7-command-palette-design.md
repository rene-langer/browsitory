# Browsitory Phase 7 Design — Command Palette

## Goal

Give the app fast, keyboard-driven access to every action without leaving
the keyboard: a searchable palette, opened with a single shortcut, that
runs zero-argument actions directly, runs single-pick actions (switch to
a specific branch, fetch a specific remote) directly after one keystroke
of filtering, and jumps to the right sidebar section for anything more
complex. This is the app's first global keyboard shortcut of any kind.

## Scope and delivery order

Single plan, one delivery (no foundation/rollout split — the surface
area is small enough: one new component, one new pure function, one
additive primitive change).

Out of scope for Phase 7: rebuilding any existing form inside the
palette (multi-field commands navigate to the existing sidebar form
instead of duplicating it); a real fuzzy-matching library (simple
scored substring matching is enough for the current command count);
any change to `RepoClient`, DTO, Tauri command, worker message, or
`git-core` function.

## Shared architecture

The existing boundary remains unchanged:

`React components/state -> RepoClient -> Tauri command -> Worker -> git-core`

Phase 7 touches only `frontend/src/` — no `RepoClient` method, DTO,
Tauri command, worker message, or `git-core` function is added, removed,
or changed in shape. The palette calls only handlers `useAppState`
already exposes; it introduces no new state-mutation logic.

## Keyboard trigger

A single `window`-level `keydown` listener, mounted once in `App.tsx`,
watches for `Ctrl+K` (Windows/Linux) / `Cmd+K` (macOS) and calls
`preventDefault()` before toggling the palette open. It fires regardless
of focus — including while a text input has focus elsewhere in the
app — since `Ctrl/Cmd+K` is not a native text-editing shortcut and no
existing shortcut in this app conflicts with it (confirmed: this is the
app's first keyboard shortcut of any kind). Closing reuses the `Overlay`
primitive's existing Escape-to-close behavior — no new close logic.

`CommandPalette` is mounted only while open, the same conditional-mount
pattern `RebasePlanner`/`TransferPanel` already use inside `Overlay`.

## Components

### New: `CommandPalette`

`frontend/src/components/CommandPalette.tsx` — a search input plus a
results list, rendered as `Overlay`'s child. Each result row uses the
`ListRow` primitive (matching `CommitGraph`'s and `RepoPicker`'s
existing list pattern). Arrow keys move the highlighted row, Enter runs
it, typing filters the list live. No two-step "pick a command, then
pick an argument" flow — single-pick commands are already expanded into
individual rows before the list renders (see Data model below), so one
search box handles everything.

### New: `buildCommands`

`frontend/src/lib/commands.ts` — a pure function,
`buildCommands(appState: UseAppStateResult): Command[]`, called fresh
each time the palette opens. Command shape:

```typescript
interface Command {
  id: string;
  label: string;
  keywords: string[];
  run: () => void;
}
```

Three kinds of entries come out of this one flat array:
- **Zero-argument actions** — one `Command` each: `id: "fetch-all"`,
  `label: "Fetch"`, `run: () => appState.fetchRemote(...)`, etc.
- **Single-pick actions, pre-expanded** — one `Command` per option at
  build time: `appState.state.branches` produces one `"Switch to main"`,
  one `"Switch to feature/x"`, etc., each `run` closing over that
  branch's name and calling `appState.switchBranch(name)` directly.
  Same pattern for remotes (fetch a specific remote), tags (push a
  specific tag), worktrees (open a specific worktree), stashes (apply/
  drop a specific stash).
- **Navigate commands** — one `Command` per multi-field action
  ("Create pull request…", "Add remote…", "Save credential…"). `run`
  expands the relevant `AccordionSection` (see below) and scrolls its
  root element into view via
  `document.querySelector('section[aria-label="<title>"]')?.scrollIntoView({ behavior: "smooth", block: "nearest" })`
  — the same `section[aria-label=...]` selector Phase 6's
  `e2e/support/sidebar.ts` helper already uses, so this isn't a new
  targeting convention, just the first place it's used from application
  code instead of a test.

### `AccordionSection` is not modified

Navigate commands need to force a closed section open, but never need
to close an open one. Rather than adding controlled-mode props to
`AccordionSection`, a navigate command's `run` finds the section's own
header button via
`document.querySelector('section[aria-label="<title>"] button[aria-expanded]')`
and, only if its `aria-expanded` attribute is currently `"false"`,
dispatches a real `.click()` on it — the same DOM node a user would
click, so `AccordionSection`'s existing `onClick={toggle}` handler runs
unchanged (a script-dispatched `click()` reaches React's synthetic
event system exactly like a user click does). Then
`scrollIntoView({ behavior: "smooth", block: "nearest" })` on the
section element itself. This needs zero new props, zero new state, and
zero changes to any of the 7 already-shipped sidebar components — it
drives the same public, already-accessible affordance a keyboard-only
user would otherwise have to Tab to and press Enter on.

### Changed: `App.tsx`

Adds the `keydown` listener and conditionally renders
`<Overlay><CommandPalette .../></Overlay>`, following the exact pattern
already used for `RebasePlanner`/`TransferPanel`.

## Data flow and state

- Command list is rebuilt on every palette open (not memoized across
  opens) — cheap at this scale (a few dozen commands), and guarantees
  branch/remote/tag/worktree/stash lists are never stale.
- Recently-used command ids persist to `localStorage` under a single
  key (`command-palette-recent`, a JSON array, capped at the last 10),
  read on each build to apply a small ranking boost — same plain,
  unguarded `localStorage` convention already established in
  `frontend/src/lib/theme.ts` and Phase 6's primitives.
- Matching: case-insensitive score per command — label-prefix match
  ranks highest, keyword match next, substring-anywhere last; results
  capped to the top 50. No new dependency.
- No `RepoClient`/DTO/Tauri-command change anywhere in this phase.

## Testing

- `buildCommands.test.ts` — pure function, easy to test in isolation:
  given a mock `appState`-shaped object, assert the right zero-arg,
  single-pick, and navigate commands are produced, with correct `run`
  closures (spy on the underlying `appState` handler being called with
  the right argument).
- `CommandPalette.test.tsx` — render with a fixed command list, assert
  typing filters, arrow keys move selection, Enter calls the
  highlighted command's `run`, Escape closes (via `Overlay`, already
  covered by its own tests).
- `App.tsx` still has no dedicated test file (a pre-existing gap noted
  in Phase 6's final review, not created by this phase) — the
  `Ctrl/Cmd+K` listener is covered indirectly by the E2E suite instead.
- GUI E2E: one new spec exercising the full loop — open the palette,
  type to filter, run a zero-arg command (assert its effect, e.g. theme
  toggles), run a single-pick command (assert e.g. branch switches),
  run a navigate command (assert the target section is now expanded and
  scrolled into view region-visible).
- `pnpm build`, `pnpm lint`, and `pnpm test -- --run` must pass after
  every task, same Global Constraint as Phases 5 and 6.

## Global constraints

- No `RepoClient` method, DTO, Tauri command, worker message, or
  `git-core` function is added, removed, or changed in shape.
- Frontend tests mock `RepoClient`, never `@tauri-apps/api`.
- `pnpm lint`'s `no-restricted-imports` rule
  (`frontend/eslint.config.js:25-37`) must keep passing.
- No new dependency (matching/ranking is hand-rolled, no fuzzy-search
  library).
- `pnpm build`, `pnpm lint`, and `pnpm test -- --run` must pass after
  every task.
- Every new/changed surface is checked in both light and dark theme.
- `AccordionSection` is not modified by this phase — see Components
  section for how navigate commands force a section open without it.
