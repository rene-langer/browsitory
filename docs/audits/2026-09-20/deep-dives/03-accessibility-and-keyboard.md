# Deep-dive 03: Accessibility and keyboard behavior

## 1. Purpose, scope, and audit questions

This deep-dive reviews keyboard navigation, focus management, ARIA structure, target sizes,
form semantics and motion, against WCAG 2.2 AA where applicable. It builds on deep-dive 05 of the
2026-09-05 audit (context-menu keyboard reachability, FE-001), which has since been resolved for
`ContextMenu` (arrow keys, Home/End and initial focus are now implemented).

Audit questions: Does focus return to the invoking control when a dialog closes? Are the custom
widgets (tabs, listboxes, menus, dialogs) valid ARIA? Are pointer targets at least 24x24 CSS
pixels? Do forms expose errors and requirements programmatically?

## 2. Architecture/execution path examined

- Dialog primitives: `primitives/Overlay.tsx`, `ConfirmDialog.tsx`, `FormDialog.tsx`, and the one
  raw `<dialog open>` in `RepoPicker.tsx:149`.
- Menus: `primitives/ContextMenu.tsx`. Tabs: `RepoTabs.tsx`. Lists: `ListRow.tsx`,
  `CommitGraph.tsx`, `DiffPane.tsx` (`role="listbox"` file lists).
- Splitters: `primitives/SplitView.tsx` (`role="separator"`).
- Forms: `TagPanel`, `WorktreePanel`, `PullRequestPanel`, `BranchTree` dialogs.

## 3. Evidence reviewed and checks run

- CDP-driven harness (see deep-dive 01) with real keyboard and mouse input events.
- **Focus restoration test:** focused the commit list, opened the command palette (Ctrl+K), pressed
  Escape: focus returned to the commit list (`Overlay` restores). Then opened a branch "…" menu,
  chose Delete, and pressed Escape on the confirm dialog: `document.activeElement` became `BODY`.
  Repeated with "New Branch…" then Escape: `BODY` again.
- **Target sizes** (from `getBoundingClientRect()` at 1440x900): branch swatch 10x10; branch "…",
  tab close, sidebar toolbar buttons and the "+" button 22x22; collapse and stage toggles 24x24;
  divider handles 4px wide.
- **Tab order:** DOM order is header tabs, close buttons, "+", theme, release notes, sidebar
  toolbar, Branches. Traversing three unstaged files of three hunks each took about 30 Tab presses (see UX-004).
- **Roles and headings:** headings are H1 "Browsitory" then H2 per accordion section; the three
  open repos each mount a full hidden `RepoWorkspace` (`display: none`), so hidden duplicates
  exist but are removed from the tab order and accessibility tree.
- Static sweeps: no `aria-invalid`, `aria-describedby` or `required` anywhere in `src`;
  `role="status"`/`aria-live` in 7 files.
- **Not run:** any screen reader, forced-colors mode, or 200% zoom (see Coverage gaps).

## 4. Findings

### AUD-2026-09-20-A11Y-001 — Focus is lost when a `ConfirmDialog` or `FormDialog` closes

- **Category:** Accessibility
- **Severity:** High
- **Confidence:** High (reproduced)
- **Location:** `frontend/src/components/primitives/ConfirmDialog.tsx:39-48`,
  `primitives/FormDialog.tsx:24-38`; contrast with `primitives/Overlay.tsx:32` (calls
  `dialog.close()` in its cleanup).
- **Evidence images:** [`08-force-delete-confirm.png`](../evidence/08-force-delete-confirm.png), [`09-new-branch-form.png`](../evidence/09-new-branch-form.png)
- **Problem:** Both components call `showModal()` on mount but have no cleanup that closes the
  dialog or restores focus. Callers unmount them, and removing an open modal `<dialog>` from the
  DOM does not return focus to the previously focused element.
- **Observed behavior:** After Escape on the force-delete confirm dialog, and after Escape on the
  New Branch dialog, `document.activeElement` was `BODY`.
- **Why it matters:** WCAG 2.4.3 (Focus Order) and the dialog pattern require focus to return to
  the invoking control. For a keyboard-first product, every dialog dumps the user at the top of
  the page. This affects every destructive confirmation and every form dialog.
- **Recommendation:** Store `document.activeElement` on mount and call `.focus()` on it in the
  effect cleanup (fall back to a known container if it is detached), in both components, or move
  both onto `Overlay`'s close-in-cleanup pattern.
- **Verification:** Testing Library test per component: focus a trigger, open, cancel, assert
  `document.activeElement` is the trigger.

### AUD-2026-09-20-A11Y-002 — Pointer targets below 24x24 CSS pixels

- **Category:** Accessibility
- **Severity:** Medium
- **Confidence:** High (measured)
- **Location:** `frontend/src/components/BranchTree.module.css:86-96` (`.swatch` is 10x10);
  `RepoTabs.module.css` (close 22x22); `primitives/Sidebar.module.css` (toolbar buttons);
  branch "…" buttons; `primitives/SplitView.module.css:16` (`flex: 0 0 4px`).
- **Problem:** WCAG 2.5.8 (Target Size, Minimum) requires 24x24 unless spacing exempts. The
  swatch is the only branch graph-visibility toggle and is 10px in tightly packed rows. The
  split-pane dividers are 4px wide.
- **Why it matters:** Mis-clicks on adjacent controls (switching vs toggling), difficult use with
  a trackpad or reduced motor precision.
- **Recommendation:** Keep the visual size and add transparent padding or a pseudo-element hit
  area so each control is at least 24x24. Give dividers about 8px of invisible grab area on top
  of the 4px line.
- **Verification:** A test asserting the bounding box of these controls (in a real browser
  environment) or an axe rule run in E2E.

### AUD-2026-09-20-A11Y-003 — `tablist` structure is invalid and has no arrow-key navigation

- **Category:** Accessibility
- **Severity:** Medium
- **Confidence:** High (DOM structure observed; no screen reader run)
- **Location:** `frontend/src/components/RepoTabs.tsx:53,76`.
- **Problem:** `role="tablist"` directly contains group wrapper `div`s, the tab `div`s, close
  buttons (inside the tab wrappers) and the Add button. Only the label buttons carry `role="tab"`.
  ARIA requires a tablist's owned elements to be tabs (or groups of tabs). No roving `tabindex`,
  no Left/Right/Home/End handling, and no `role="tabpanel"` or `aria-controls`.
- **Observed behavior:** Every tab label and close button is a separate tab stop; arrow keys do
  nothing.
- **Why it matters:** Screen readers announce inconsistent structure ("tab 1 of 3" vs extra
  buttons), and keyboard users tab through 7+ stops in the header before reaching content.
- **Recommendation:** Use roving tabindex with arrow-key navigation, keep only tabs inside the
  tablist, move the Add button and any per-tab close controls outside (or expose close as a
  keyboard action such as Ctrl/Cmd+W), and add `aria-controls` to the active workspace.
- **Verification:** Keyboard test for Left/Right/Home/End; axe rule `aria-required-children`.

### AUD-2026-09-20-A11Y-004 — Forms expose no validation or requirement semantics

- **Category:** Accessibility / Forms
- **Severity:** Medium
- **Confidence:** Medium (static sweep; not every form was rendered)
- **Location:** `TagPanel.tsx:78-81`, `WorktreePanel.tsx:74-106`, `PullRequestPanel.tsx:168-255`,
  `BranchTree.tsx:651-692,893-913,960-975`.
- **Problem:** Labels are correctly wrapped around inputs, but `aria-invalid`,
  `aria-describedby` and `required` are used nowhere in `frontend/src`. Errors are shown in a
  separate `InlineError` block, not tied to the field that caused them.
- **Why it matters:** A screen-reader user submitting an invalid tag name hears an alert but does
  not learn which field is wrong. Required fields are conveyed only by a disabled submit button.
- **Recommendation:** Introduce a small `Field` primitive (label, optional hint, error text, ids
  wired to `aria-describedby` and `aria-invalid`) and adopt it in all forms.
- **Verification:** Testing Library `toHaveAccessibleDescription` assertions on error states.

### AUD-2026-09-20-A11Y-005 — Workspace deletion uses a non-modal raw `<dialog open>`

- **Category:** Accessibility / Consistency
- **Severity:** Medium
- **Confidence:** High (code)
- **Location:** `frontend/src/components/RepoPicker.tsx:149-170`.
- **Problem:** The confirmation opens with the `open` attribute (non-modal), so there is no focus
  trap, no inert background, no Escape handling and no autofocus on the safe action. Every other
  destructive confirmation in the app uses `ConfirmDialog`.
- **Why it matters:** Keyboard users can tab out of the dialog into the page behind it, and
  Escape does nothing. It is also the only inconsistent confirmation pattern.
- **Recommendation:** Replace with `ConfirmDialog` (also fixes the focus behavior once
  A11Y-001 is fixed).

### AUD-2026-09-20-A11Y-006 — `ContextMenu` closes when the pointer leaves it and is not clamped to the viewport

- **Category:** Interaction / Accessibility
- **Severity:** Low
- **Confidence:** Medium (code trace; edge-of-window behavior not tested)
- **Location:** `frontend/src/components/primitives/ContextMenu.tsx:114-115`.
- **Problem:** The menu is `position: fixed` at the pointer coordinates with
  `onMouseLeave={onClose}`. There is no flip or clamp near the right or bottom window edge, and
  moving the pointer off the list, even by a few pixels, dismisses it.
- **Why it matters:** Menus opened near the bottom or right edge can be partly off-screen, and
  accidental dismissal hurts users with tremor or when moving toward a submenu-less far item.
- **Recommendation:** Clamp `x`/`y` to the viewport after measuring the menu, and drop
  `onMouseLeave` (outside click and Escape already close it).

### AUD-2026-09-20-A11Y-007 — Reduced motion and icon tooltips are only partly handled

- **Category:** Accessibility / Interaction
- **Severity:** Low
- **Confidence:** High (code)
- **Location:** `AccordionSection.module.css:105` (only `prefers-reduced-motion` rule);
  `lib/commands.ts:79,98` (`scrollIntoView({ behavior: "smooth" })`); icon-only buttons in
  `App.tsx:450-557`, `RepoTabs.tsx`, `Sidebar.tsx`.
- **Problem:** Smooth scrolling ignores the user's motion preference; icon-only buttons carry an
  `aria-label` but no visible tooltip, and the sidebar toolbar (expand all, collapse all,
  settings) has no visible labels at all.
- **Recommendation:** Use `behavior: "auto"` when `matchMedia("(prefers-reduced-motion:
  reduce)")` matches; add `title`/tooltip text to icon buttons.

## 5. Coverage gaps, open questions, and accepted risks

- No screen reader (NVDA, VoiceOver, Orca) was available; ARIA findings (A11Y-003, A11Y-004) are
  DOM- and code-based. Announcements of the diff `listbox` with nested buttons remain an open
  question, already acknowledged in `ListRow.tsx`'s doc comment.
- No forced-colors or 200% zoom run. The divider focus style uses an outline, which suggests
  forced-colors was considered (`SplitView.module.css:24-31`), but this was not tested.
- Menus for tags, worktrees, PRs and remote credentials were not exercised interactively.
- The focus-loss finding (A11Y-001) was reproduced in Chrome only. Behavior in WebKit (macOS Tauri)
  and WebView2 (Windows Tauri) may differ and should be confirmed.

## 6. Strengths and controls that reduced risk

- Native `<dialog>` with `showModal()` for all overlays gives a real focus trap and inert
  background (`Overlay`, `ConfirmDialog`, `FormDialog`).
- `ConfirmDialog` autofocuses Cancel and routes Escape through `onCancel`, so a stray Enter never
  confirms a destructive action.
- `Overlay` restores focus correctly (palette test above); the fix for A11Y-001 already exists in
  the codebase.
- `ContextMenu` now implements arrow keys, Home/End and initial focus (resolves the earlier
  FE-001 in part).
- The command palette is a well-formed `combobox` with `aria-activedescendant`; the divider is a
  keyboard-operable `separator` with `aria-valuenow`; `aria-keyshortcuts` is declared on the
  lists that own shortcuts.
- Focus rings are visible everywhere they were measured (2px accent outline).
