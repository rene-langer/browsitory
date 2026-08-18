# Browsitory Phase 5 Design

## Goal

Give the full feature set built in Phases 1-4 a coherent, distinctive visual
design — matching the speed and polish bar of Sublime Merge — without altering
the `RepoClient` boundary, so the same visual system carries unmodified into a
future VSCode webview frontend.

## Scope and delivery order

Phase 5 is deliberately split into two implementation plans and releases:

1. **Design system foundation** — tokens, typography, layout primitives, an
   icon system, and a motion/interaction system, proven by reskinning the core
   commit-review loop: `DiffPane` (and the `DiffView`/`BlameView` it renders),
   `CommitBox`, `CommitGraph` (and the `CommitLaneGraphic` it renders — this
   is the commit/stash history list; there is no separate `HistoryList`
   component).
2. **Rollout** — apply the system from (1) to every remaining component:
   `BranchSwitcher`, `RebaseProgressPanel`, `ConflictResolutionPane`,
   `RebasePlanner`, `RemotePanel`, `TagPanel`, `TransferPanel`,
   `PullRequestPanel`, `WorktreePanel`, `SubmodulePanel`, `ReflogPanel`,
   `RepoPicker`, `App.tsx` shell.
3. **App icon and favicon** — replace the placeholder desktop app icon set
   with a real mark; independent of (1) and (2), so it can ship in parallel
   with either.

The foundation plan must leave the app in a working, tested state with a
visible before/after on the flagship views; the rollout plan does not begin
until the foundation has shipped, so its primitives are stable before every
other component adopts them. The app-icon plan has no dependency on the
other two and may be scheduled independently.

## Shared architecture

The existing boundary remains unchanged:

`React components/state -> RepoClient -> Tauri command -> Worker -> git-core`

Phase 5 touches only `frontend/src/` — no `RepoClient` method, DTO, Tauri
command, worker message, or `git-core` function is added, removed, or
changed in shape. Styling and markup only; where a component's interaction
is a known rough edge (see "Behavior policy" below), its props/logic may
change, but never its `RepoClient` usage.

## Design tokens and theming

`frontend/src/styles/tokens.css` defines CSS custom properties on `:root`
for the light palette: surface/text/border/accent tiers, three diff tiers
(add/remove/context), a spacing scale, radius scale, and an elevation/shadow
scale. A dark palette overrides the same token names under
`@media (prefers-color-scheme: dark)`, redefined again under
`:root[data-theme="dark"]` so a manual toggle wins over the OS setting in
both directions (mirroring the Artifact theming contract this project's
tooling already uses elsewhere). No component reads a raw color value —
only tokens.

Two font-family tokens exist: a sans stack for UI chrome (labels, buttons,
panel headers) and a mono stack for git data — commit hashes, diff lines,
refs, branch names, blame commit IDs. A 5-6 step type scale is token-driven;
line-height is tighter for dense list views (`CommitGraph`, `DiffPane`,
`BlameView`) than for forms and panels (`CommitBox`, credential dialogs).

## Layout primitives

New shared components under `frontend/src/components/primitives/`:

- `Panel` — a bordered, elevated content container with an optional header.
- `SplitView` — the resizable two/three-pane layout `App.tsx` and `DiffPane`
  currently hand-roll.
- `Toolbar` — a horizontal action/button row with consistent spacing.
- `ListRow` — a dense, hoverable, keyboard-navigable row for history, diff,
  blame, and reflog lists.

Every component touched in either sub-project composes from these instead
of bespoke per-component CSS. Each existing component gains a co-located
CSS Module (`ComponentName.module.css`); the single global
`frontend/src/index.css` shrinks to resets and token imports only.

## Icons

Adopt `lucide-react` (MIT license) as a new `frontend/package.json`
dependency, verified and recorded in `docs/LICENSE_COMPLIANCE.md` per the
project's license policy. Replaces ad hoc text/symbol markers for
stage/unstage, merge-conflict states, branch, stash, tag, worktree,
submodule, and PR status across every component in scope.

## Motion and interaction

Transition durations and easings are token-driven
(`--motion-duration-*`, `--motion-easing-*`). Hover, focus, and active
states get consistent treatment app-wide, including visible keyboard-focus
rings for every interactive element — the interaction-speed half of the
Sublime Merge bar, applied uniformly rather than per-component.

## App icon and favicon

`crates/tauri-app/icons/` currently holds five files — `32x32.png`,
`128x128.png`, `128x128@2x.png`, `icon.icns`, `icon.ico` — all referenced by
`crates/tauri-app/tauri.conf.json`'s `bundle.icon` array, and all a flat
solid-blue placeholder square with no mark. `frontend/public/favicon.svg`,
by contrast, is a real angular purple logo mark (clip-masked layered
ellipses), used only as the browser-tab favicon and disconnected from the
desktop app icon.

Phase 5 designs one mark, derived from or replacing the `favicon.svg`
shape, expressed as the token palette's accent color rather than the
favicon's current one-off purple, and regenerates the full desktop icon
set from it — all five files Tauri's bundler expects, at their existing
sizes, via `cargo tauri icon` or equivalent. `frontend/public/favicon.svg`
and `frontend/public/icons.svg` (the UI sprite sheet) are updated to match
the same mark and palette so the web favicon, desktop icon, and in-app
iconography read as one brand across both target frontends (Tauri app and
future VSCode webview).

## Behavior policy

Phase 5 is visual-first, not visual-only: component logic and `RepoClient`
usage are otherwise frozen, but a targeted UX fix is in scope where the
current interaction is a clear rough edge — for example keyboard focus
order, or an unclear loading/error state — discovered while a component is
already being touched for its reskin. Existing vitest/RTL tests are updated
to match any such change rather than left frozen; tests for components with
no behavior change keep passing unmodified.

## VSCode-webview constraint

Every new primitive and token file is static CSS plus React — no
`@tauri-apps/api` import, no Tauri-specific API, in any new or restyled
component. `tauriRepoClient.ts` remains the sole `@tauri-apps/*` importer
(enforced today by `frontend/eslint.config.js`'s `no-restricted-imports`
rule), so the entire visual system is verified portable to a future
`vscodeRepoClient.ts`-backed frontend without a design pass repeated.

## Testing and acceptance

- Frontend tests continue to mock `RepoClient`, never `@tauri-apps/api`.
- `pnpm lint`'s `no-restricted-imports` check must keep passing unmodified —
  Phase 5 introduces no new `@tauri-apps/*` imports outside
  `tauriRepoClient.ts`.
- `pnpm build` and the existing GUI E2E suite (`e2e/`) must keep passing
  against the restyled app; E2E specs assert behavior and structure, not
  pixel appearance, so a pure restyle should not require spec changes,
  while a targeted UX fix may.
- `lucide-react` must be recorded in `docs/LICENSE_COMPLIANCE.md` before
  first use.
- Both light and dark themes are exercised for every touched component
  (visual review, not an automated visual-regression suite in this phase).
- The regenerated icon set is verified with `cargo tauri build` (or
  `cargo build --workspace` at minimum) so `tauri.conf.json`'s
  `bundle.icon` array resolves against real files at every listed path
  and size.
