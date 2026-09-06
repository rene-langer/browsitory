# Deep-dive 05: Frontend quality and accessibility

## 1. Purpose, scope, and audit questions

This deep-dive examines `frontend/src` (React 18 + TypeScript + Vite) for:

- State management correctness (`src/state/*`), especially the shared mutation/error/pending
  bookkeeping (`useMutationRunner.ts`) that every domain action hook composes.
- The Phase 5 design-system rollout status (primitives under `src/components/primitives/`).
- Keyboard reachability of the interaction model described in `CHANGELOG.md`'s 0.2.0 entry,
  where branch/remote mutating actions moved to right-click context menus and a command
  palette.
- Accessibility: ARIA roles/labels on custom interactive widgets, focus management in
  modals/menus, live-region usage for async status.
- Rendering performance for large repos (commit graph, always-expanded diff view).
- Safe handling of repository-controlled content (commit messages, diffs, branch/file names)
  that is rendered into the DOM — a malicious or adversarial repository is untrusted input.

Audit questions: Can every mutating action reachable via a context menu also be reached without
a mouse? Does the app render any repository-controlled string as raw HTML? Are loading/async
states announced to assistive technology? Do the test/lint suites pass cleanly at the audit
commit?

## 2. Architecture/execution path examined

- `src/App.tsx`, `src/state/useAppState.ts` and its composed hooks (`useBranchActions`,
  `useStagingActions`, `useMergeRebaseActions`, `useRemoteTransferActions`, etc.) — all built on
  `useMutationRunner.ts`'s six `runMutation*`/`runOptimisticMutation*` variants.
- `src/components/primitives/*` — `ContextMenu`, `Overlay`, `ConfirmDialog`, `Panel`, `Toolbar`,
  `Sidebar`, `AccordionSection`, `ListRow`, `SplitView` — the Phase 5 primitive layer.
- `src/components/BranchTree.tsx` (837+ lines) as the primary case study for the context-menu/
  command-palette interaction model, cross-referenced against `src/lib/commands.ts` (the command
  palette's registry).
- `src/components/CommandPalette.tsx`, `src/components/DiffView.tsx`, `src/components/
  CommitGraph.tsx`, `src/components/CommitBox.tsx`.
- `src/ipc/RepoClient.ts` / `tauriRepoClient.ts` only at the boundary (transport itself is
  deep-dive 01/06's scope, not repeated here).

## 3. Evidence reviewed and checks run

Commands run at the audit commit (`cd0cb63`), from `frontend/` (dependencies already installed
via `pnpm install`):

- `pnpm lint` → **exit 0**, no output beyond the ESLint invocation banner (clean).
- `pnpm test -- --run` → **exit 0**. `Test Files 47 passed (47)`, `Tests 576 passed (576)`,
  duration 177.51s. Full log tail:
  ```
   Test Files  47 passed (47)
        Tests  576 passed (576)
     Start at  17:02:39
     Duration  177.51s (...)
  EXIT:0
  ```
- `pnpm build` was not run separately (out of budget for this deep-dive; `pnpm test` already
  exercises `vite`'s transform pipeline via Vitest, and CI's `frontend` job runs `pnpm build`
  independently — see deep-dive 08).
- `grep -rn "dangerouslySetInnerHTML" src/` → no matches (0 occurrences repo-wide).
- Manual read-through of `ContextMenu.tsx`, `Overlay.tsx`, `ConfirmDialog.tsx`,
  `CommandPalette.tsx`, `BranchTree.tsx` (trigger wiring for all four `ContextMenu` call sites),
  `DiffView.tsx`, `useMutationRunner.ts`, and a `grep` sweep for `aria-`/`role=` across
  `src/components/*.tsx` and for `role="status"`/`aria-live` (async status/live-region usage).

## 4. Findings

### AUD-2026-09-05-FE-001 — Context-menu-only mutating actions are not reachable by keyboard for remote-tracking branches; the menu widget itself doesn't follow the ARIA menu keyboard pattern

- **Severity:** Medium
- **Confidence:** Confirmed (static trace of the exact trigger and widget code; not confirmed
  interactively in a live browser, since this deep-dive was performed via static review only —
  see Coverage gaps).
- **Affected components:** `frontend/src/components/BranchTree.tsx:614-625` (remote-tracking
  branch row), `frontend/src/components/BranchTree.tsx:570-576` (local-branch row),
  `frontend/src/components/BranchTree.tsx:596-602` (remote-folder row),
  `frontend/src/components/primitives/ContextMenu.tsx:13-70`.
- **Evidence:**
  - `BranchTree.tsx:614` renders a remote-tracking branch name inside a bare `<span
    onContextMenu={...}>`, with no `tabIndex`, `role="button"`, or `onKeyDown` handler. A
    `<span>` is not a focusable element by default, so it can never receive keyboard focus —
    which means the native `contextmenu` DOM event (normally also dispatchable via the keyboard
    "Menu"/"Application" key or Shift+F10 *on a focused element*) can never fire for this row by
    keyboard. `commands.ts` (the command-palette registry, `frontend/src/lib/commands.ts:1-337`)
    has no `id`s for renaming/deleting/checking-out a remote-tracking branch — `remoteBranchItems`
    (`BranchTree.tsx:313`) is the only place those actions are defined, and it is only invoked
    from this unreachable `onContextMenu` handler.
  - `BranchTree.tsx:570` (local branch) and `:596` (remote folder) attach `onContextMenu` to real
    `<button>` elements, which *are* focusable, so Shift+F10/Menu-key would fire the native event
    there — but this is a non-obvious, undocumented path with no visible affordance (no "…" menu
    button, no `aria-haspopup`), and `commands.ts` likewise has no `rename-branch`,
    `delete-branch`, `merge-branch`, `add-remote` (only `add-remote`'s draft-open path is exposed
    a different way, per `commands.ts:177`), `edit-remote`, or `manage-credentials` entries as a
    keyboard-discoverable fallback for the actions defined in `branchContextItems`
    (`BranchTree.tsx:370`) and `remoteFolderItems` (`BranchTree.tsx:257`).
  - Once a `ContextMenu` is open, `ContextMenu.tsx:26-39` handles only `Escape` (close) and
    click-outside (close). There is no `ArrowDown`/`ArrowUp` navigation, no `Home`/`End`, and no
    focus is programmatically moved into the menu on open (`useEffect` at `:26` registers
    listeners but never calls `.focus()` on the first `<li>`/`<button>`). The WAI-ARIA Authoring
    Practices menu pattern (which `role="menu"`/`role="menuitem"` at `ContextMenu.tsx:44,53`
    explicitly opts into) requires roving-tabindex arrow-key navigation and initial focus
    placement; neither exists, so a keyboard/screen-reader user who does reach the menu (via the
    button path above) has no idiomatic way to move between its items other than raw Tab, which
    also tabs out of the menu into whatever follows in DOM order.
- **Impact:** A keyboard-only or screen-reader user cannot rename, delete, or check out a
  remote-tracking branch at all (no path exists), and cannot reliably discover *any*
  branch/remote mutating action, since the only route is an undocumented native-event pattern on
  a subset of rows, into a menu widget that doesn't implement standard menu keyboard navigation.
  This directly affects the product's own goal (`docs/ARCHITECTURE.md`, CLAUDE.md local product
  inspiration note) of Sublime-Merge-level keyboard-driven workflow — the 0.2.0 changelog frames
  the context-menu/command-palette redesign as *replacing* a keyboard-reachable dropdown UI, but
  the command palette was not backfilled with the actions that moved into context-menu-only
  territory.
- **Trigger/reproduction:** Tab through the Branches panel using only the keyboard; observe that
  focus never lands on a remote-tracking branch row (no visual focus ring appears on it, and
  DOM inspection confirms no `tabindex`). For a local branch, focus the branch button and press
  Shift+F10 or the keyboard Menu key; the menu opens, but arrow keys do nothing and there is no
  visible indication of which item (if any) is focused.
- **Remediation:** (a) Give remote-tracking branch rows a focusable element (a `<button>` instead
  of `<span>`) with an explicit affordance (e.g., a "…" icon button) that opens the same menu on
  `Enter`/`Space`, not only on the native `contextmenu` event. (b) Add roving-tabindex
  arrow-key navigation and initial-item focus to `ContextMenu.tsx`, matching the APG menu
  pattern already implied by its `role` attributes. (c) Either register the branch/remote
  mutating actions in `commands.ts` as a keyboard-discoverable fallback, or explicitly document
  (in-app or in `docs/USER_GUIDE.md`) the keyboard path to open each context menu.
- **Verification:** Add a frontend test (Testing Library, `userEvent.tab()` +
  `userEvent.keyboard()`) asserting a remote-tracking branch row is reachable by Tab and that
  `ArrowDown` moves focus between `ContextMenu` items; add an E2E or manual keyboard-only pass
  over the Branches panel's full action set.

### AUD-2026-09-05-FE-002 — Commit-message textarea has no accessible name once the placeholder is not visible as text

- **Severity:** Low
- **Confidence:** Confirmed
- **Affected components:** `frontend/src/components/CommitBox.tsx:62-68`.
- **Evidence:** The `<textarea>` at `CommitBox.tsx:62` has only a `placeholder="Commit message"`
  attribute — no `aria-label`, `aria-labelledby`, or associated `<label>`. Placeholder text is
  commonly used by assistive-technology users as a de facto label, but it is not guaranteed to
  be exposed as the field's accessible name across all screen-reader/browser combinations, and it
  disappears from the visual/AX tree once the field has a value, so a screen-reader user
  revisiting a partially-typed commit message may hear no field description at all.
- **Impact:** Low — the field is easy to identify from context (it is the only textarea in the
  commit panel, immediately preceded by nothing else needing a name), but this is a real,
  bounded accessibility deviation from WCAG 4.1.2 (Name, Role, Value) best practice.
- **Trigger/reproduction:** Inspect the accessible-name computation for the textarea in a
  screen-reader or the browser's Accessibility tree panel after typing a message; placeholder
  reliance is visible directly in the source at the cited line.
- **Remediation:** Add `aria-label="Commit message"` (or a visually-hidden `<label>`) alongside
  the existing `placeholder`.
- **Verification:** Testing-Library `getByRole("textbox", { name: "Commit message" })` query,
  which currently would only pass if RTL/jsdom falls back to placeholder-as-name (it does in
  jsdom, but that is not authoritative for real assistive technology).

## 5. Coverage gaps, open questions, and accepted risks

- **No live browser/screen-reader pass.** Both findings above are static-code confirmations of a
  concrete, traceable defect (missing `tabIndex`, missing keyboard handlers, missing ARIA
  labels) — not confirmed via an actual screen reader (NVDA/VoiceOver) or a running Tauri window,
  since this deep-dive's budget did not include launching the desktop app. Confidence is
  "Confirmed" for *code-level* absence of the expected mechanism, not for the full assistive-tech
  user experience.
- **`pnpm build` was not run** in this pass; CI's `frontend` job (deep-dive 08) already covers
  it, and `pnpm test`/`pnpm lint` both exercise the same TypeScript/Vite toolchain surface that
  would catch a build-breaking regression.
- **Color contrast** was not evaluated — no automated contrast-checking tool was run against the
  design tokens (`src/lib/theme.ts`), and this would need a rendered page (axe-core or a browser
  DevTools contrast check), which the static-only method used here cannot substitute for.
- **Rendering performance under real large-repo load** (thousands of commits, very large diffs)
  was not measured — `CommitGraph.tsx:100` does memoize its layout computation (`useMemo` keyed
  on `commits`), which is the one obvious perf trap for that component and it is already
  guarded against; no equivalent large-N stress test was run against `DiffView.tsx` or the
  commit list, so a real perf regression under pathological input (e.g., a single 50,000-line
  diff) remains an open question, not a confirmed defect.
- **`BlameView.tsx`, `RebasePlanner.tsx`, `RebaseProgressPanel.tsx`, `ReflogPanel.tsx`** were
  flagged by the `aria-`/`role=` grep sweep as having zero matches. This was not escalated to a
  finding because a manual read was only done for `CommitBox.tsx`/`ContextMenu.tsx`/
  `CommandPalette.tsx`; whether the zero-match components are primarily read-only display
  (where the gap is low-impact) or contain unlabeled interactive controls (where it would not
  be) is an open question a follow-up pass should resolve.

## 6. Strengths and controls that reduced risk

- **No `dangerouslySetInnerHTML` anywhere in `frontend/src`.** Repository-controlled content
  (commit messages, diff line content, branch/file names) is rendered exclusively through JSX
  text interpolation (e.g. `DiffView.tsx:75`'s `{line.content}`), which React escapes
  automatically — confirmed no HTML/script-injection vector from adversarial repository content
  in the paths reviewed.
- **`ConfirmDialog` (`primitives/ConfirmDialog.tsx`) is a well-built shared safety net for
  destructive actions**: uses the native `<dialog>` element's `showModal()` (built-in focus trap
  and top-layer stacking), autofocuses the *Cancel* button by default rather than the destructive
  confirm action (guards against an accidental stray Enter confirming something destructive), and
  intercepts the native `cancel` event so Escape routes through the same `onCancel` callback
  instead of desyncing from React state. This is exactly the kind of deliberate, documented
  design decision (see the component's own doc comment) that reduces the class of bug this audit
  looks for.
- **`useMutationRunner.ts` gives every domain action hook (branches, stashes, worktrees,
  submodules, remotes, merge/rebase) identical, centralized pending/error/optimistic-rollback
  semantics** — a single well-tested abstraction rather than N ad hoc copies, which lowers the
  chance of a state-consistency bug in any one feature area. The optimistic-mutation snapshot/
  rollback strategy (exact pre-call state restore on failure, documented rationale in the source
  comment) is a sound, defensible design.
- **Async status is exposed via `role="status"`/`aria-live` in multiple places**
  (`PullRequestPanel.tsx:209`, `BranchTree.tsx:883`, `UpdateBanner.tsx:38`,
  `primitives/Panel.tsx:23`'s `aria-live` prop) — live-region usage is not absent from the
  codebase, it is inconsistently applied (see Coverage gaps on the four zero-ARIA components).
- **Test coverage breadth**: 47 frontend test files, 576 passing tests, all green at the audit
  commit; `pnpm lint` clean. This is a healthy foundation that made this deep-dive's static
  findings easier to trust were not already caught and regression-guarded elsewhere.
