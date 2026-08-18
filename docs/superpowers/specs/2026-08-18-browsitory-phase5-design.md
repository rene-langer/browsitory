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
   commit-review loop: `DiffPane`, `CommitBox`, `HistoryList`.
2. **Rollout** — apply the system from (1) to every remaining component:
   `BranchSwitcher`, stash UI, `ConflictResolutionPane`, `RebasePlanner`,
   `BlameView`, `CommitGraph`/`CommitLaneGraphic`, `RemotePanel`,
   `PullRequestPanel`, credential UI, `WorktreePanel`, submodule UI,
   `ReflogPanel`, `RepoPicker`, `App.tsx` shell.

The foundation plan must leave the app in a working, tested state with a
visible before/after on the flagship views; the rollout plan does not begin
until the foundation has shipped, so its primitives are stable before every
other component adopts them.

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
line-height is tighter for dense list views (`HistoryList`, `DiffPane`,
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
