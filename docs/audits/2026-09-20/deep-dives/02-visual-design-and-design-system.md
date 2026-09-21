# Deep-dive 02: Visual design and design system

## 1. Purpose, scope, and audit questions

This deep-dive examines the token layer, the shared primitives and how the main screens use them,
in light and dark themes: layout and density, hierarchy, semantic color, and consistency of
components.

Audit questions: Is there a coherent, enforced token system? Does color carry meaning where users
need it (status, conflict, danger)? Do screens use the primitives consistently, or have they
drifted? Does the commit dock and diff area give the primary content enough room?

## 2. Architecture/execution path examined

- Tokens: `frontend/src/styles/tokens.css` (color, space, radius, shadow, type, motion; light,
  `prefers-color-scheme: dark`, and `[data-theme="dark"]` blocks), `frontend/src/index.css`
  (reset, base controls).
- Primitives: `components/primitives/*` (`Panel`, `ListRow`, `ContextMenu`, `ConfirmDialog`,
  `FormDialog`, `Overlay`, `AccordionSection`, `Sidebar`, `SplitView`, `Toolbar`, `InlineError`).
- Screens: header and tabs (`App.tsx`, `RepoTabs`), sidebar (`BranchTree` and panels), commit graph
  (`CommitGraph`), diff pane and dock (`DiffPane`, `DiffView`, `CommitBox`), repo picker.

## 3. Evidence reviewed and checks run

- Screenshots at 1440x900 (light and dark), 1000x700, 700x900 and 400x800, plus merge, rebase,
  empty and dialog states, from the CDP-driven harness described in deep-dive 01.
- Static sweeps over `frontend/src` (all `*.css`):
  - Hard-coded colors outside `tokens.css`: **none** (`#hex` and `rgb()` patterns).
  - Font-size literals: one (`index.css:30`, `h1 { font-size: 20px }`).
  - Radius literals: three (`BranchTree.module.css:94,156` for circular swatches,
    `CommandPalette.module.css:12` `0`), all justified.
  - Literal small paddings and gaps (1-2px): 13 occurrences (for example
    `BranchTree.module.css:64,122,162`, `CommitGraph.module.css:31`). Minor.
  - `@media` rules: one (`AccordionSection.module.css:105`, reduced motion). No breakpoints.
- Contrast was not measured at runtime. Token values were checked by inspection only: muted text
  `#57606a` on `#f6f8fa` and `#a6a3b8` on `#1d1c26` look adequate.

## 4. Findings

### AUD-2026-09-20-VIS-001 — The commit dock is heavy, double-bordered, and lets diff content show beneath it

- **Category:** Visual / Layout
- **Severity:** Medium
- **Confidence:** High for the measurements; Medium for the cause of the strip
- **Location:** `frontend/src/components/DiffPane.module.css:82-89` (`.commitDock`),
  `DiffPane.tsx:552-580`, `CommitBox.tsx`, `primitives/SplitView.module.css:47-52`
  (`.rightInner` padding).
- **Evidence images:** [`01-main-1440-light.png`](../evidence/01-main-1440-light.png), [`02-main-1440-dark.png`](../evidence/02-main-1440-dark.png)
- **Problem:** The dock is `position: sticky; bottom: 0` inside a scroller with 12px of padding.
  It wraps a bordered `Panel` (the commit box) in a bordered dock, so the commit form has a double
  border. A "Stash" button sits above the textarea as an unrelated orphan.
- **Observed behavior:** At 1440x900 the dock occupies about 190px (roughly 21% of the window; at
  700px tall about 31%). A strip of diff content, roughly 12px, is visible beneath the dock's
  bottom edge (screenshots of the default and merge scenarios), and a clipped hunk header is cut
  off at its top edge.
- **Why it matters:** The diff is the primary content and the dock takes a fifth to a third of
  its space permanently. The visible strip reads as a rendering glitch.
- **Recommendation:** Make the dock compact by default (one-line summary field that expands on
  focus), drop the inner `Panel` chrome, move Stash into the file-group toolbar, and remove the
  scroller's bottom padding (or offset the sticky `bottom` by it).
- **Verification:** Screenshot comparison at 800px and 900px tall; dock height budget in a test
  or visual check.

### AUD-2026-09-20-VIS-002 — Status and severity carry no color semantics; the token set has no success/warning/info roles

- **Category:** Design System / Visual
- **Severity:** Medium
- **Confidence:** High
- **Location:** `frontend/src/styles/tokens.css:22-25` (`--color-danger` is a pink background,
  `--color-danger-text` the foreground; no warning, success or info tokens),
  `frontend/src/components/DiffPane.tsx:33` and `DiffPane.module.css` (`.statusIcon` is
  `--color-text-muted` for every kind).
- **Evidence images:** [`12-merge-conflict.png`](../evidence/12-merge-conflict.png)
- **Problem:** New, Modified, Deleted, Renamed and Conflicted file rows are told apart only by a
  14px glyph and a "(Kind)" suffix, all in muted text. A conflicted file has no warning color, no
  badge and no count anywhere else.
- **Observed behavior:** In the merge scenario the "crates/git-core/src/merge.rs (Conflicted)"
  heading is styled like any modified file, positioned mid-scroll among other files.
- **Why it matters:** The state that blocks the commit is the least visible. Color is also not
  available as a secondary channel for scanning a long file list.
- **Recommendation:** Add `--color-warning-bg/-text`, `--color-success-bg/-text`,
  `--color-info-bg/-text` (light and dark), rename `--color-danger` to `--color-danger-bg`, tint
  conflicted rows and show a conflict count badge in the section header. Keep the icon and text
  so color is never the only signal.
- **Verification:** Contrast check of each new pair at 4.5:1; visual check in both themes.

### AUD-2026-09-20-VIS-003 — Card-in-card chrome in the repo picker

- **Category:** Visual
- **Severity:** Low
- **Confidence:** High
- **Location:** `frontend/src/components/RepoPicker.tsx:88-146` (`Panel` "Open a repository"
  containing `Panel` "Workspaces").
- **Evidence images:** [`06-empty-state.png`](../evidence/06-empty-state.png)
- **Problem:** A bordered card contains the toolbar, list and another bordered card. The result
  is nested boxes with two headings of different weights, and everything else on the screen is
  blank.
- **Recommendation:** Flatten to one surface with two headed sections; use dividers, not borders.

### AUD-2026-09-20-VIS-004 — The "Uncommitted Changes" row does not align with commit rows

- **Category:** Visual
- **Severity:** Low
- **Confidence:** High
- **Location:** `frontend/src/components/CommitGraph.tsx:174-182` and `CommitGraph.module.css`.
- **Evidence images:** [`01-main-1440-light.png`](../evidence/01-main-1440-light.png)
- **Problem:** The row's text starts at x=292 while commit summaries start at x=328, because it
  has no lane column. It also has no node in the lane graphic, so the working tree is not visually
  connected to `HEAD`.
- **Recommendation:** Render a lane node for the working tree (hollow) and align the text column.

## 5. Coverage gaps, open questions, and accepted risks

- Contrast was reasoned from token values, not measured on rendered pixels.
- Branch swatch colors (`lib/laneColors.ts`) were not checked for color-blind separation. The
  hidden state does use a non-color cue (hollow ring), so this is an open question, not a
  finding.
- Only the panels reachable in the harness were viewed. Tags, Worktrees, Pull Requests, Stash,
  Submodules, Reflog, the workspace editor, the update banner, the release-notes modal and Blame
  were read but not seen rendered.
- Font choice is the system stack (`--font-sans`, `--font-mono`). No finding; rendering differs
  per OS and was only seen on Linux.

## 6. Strengths and controls that reduced risk

- The token layer is small, complete for its purpose and actually enforced: zero hard-coded colors
  outside `tokens.css`, and only one font-size literal.
- Dark mode is implemented three ways (`prefers-color-scheme`, explicit `data-theme`, and
  `color-scheme` so native controls follow), with a documented reason in `tokens.css`.
- The primitives are used consistently: dialogs share `ConfirmDialog`/`FormDialog`, lists share
  `ListRow`, and panels share `Panel`. The one drifted place is the workspace-delete dialog (see
  A11Y-005).
- The diff uses sign markers (`+`/`-`, `aria-hidden` on the marker) alongside the red/green
  backgrounds, so change is not conveyed by color alone.
