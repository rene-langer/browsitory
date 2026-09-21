# Deep-dive 04: Responsive behavior and narrow-window layout

## 1. Purpose, scope, and audit questions

Browsitory is a desktop tool (Tauri window, default 1200x800) and a VS Code webview (which can be
docked into a narrow side panel). This deep-dive therefore treats "responsive" as **narrow-window
and narrow-panel behavior**, not phone UX. Touch, thumb reach and one-handed use are out of scope
because the product has no mobile target; the prompt's mobile section was evaluated only as a
narrow-viewport stress test.

Audit questions: What happens to the three-column layout as the window shrinks? Does every
control stay reachable? Does the interaction model remain appropriate?

## 2. Architecture/execution path examined

- `frontend/src/App.tsx` (~lines 218-330): outer `SplitView` (sidebar, default 260px, min 200,
  max 420, collapsible) containing an inner `SplitView` (commit graph, default 420px, min 280, max
  800, not collapsible) and the diff pane.
- `frontend/src/components/primitives/SplitView.tsx` and `.module.css`.
- `frontend/src/components/RepoTabs.tsx` and `.module.css` (`overflow-x: auto`).
- `crates/tauri-app/tauri.conf.json:16-17` (default 1200x800; no minimum size configured).
- CSS sweep: the only `@media` rule in `frontend/src` is `prefers-reduced-motion`.

## 3. Evidence reviewed and checks run

- Emulated viewports of 1440x900, 1000x700, 700x900 and 400x800 in headless Chrome.
- At 700px and 400px, measured the width of the unstaged-changes list (inside the diff pane): **0px
  at both**. At 400px `document.documentElement.scrollWidth` was 400 (no page-level horizontal
  scroll).
- At 1000px the diff pane was about 300px wide; screenshot shows wrapped hunk headers and a
  clipped long token.
- The header tab strip (`role="tablist"`) at 700px: `scrollWidth` 515 vs `clientWidth` 359,
  `overflow-x: auto`.
- `grep -n "minWidth\|min_inner_size" crates/tauri-app` found no minimum window size.

## 4. Findings

### AUD-2026-09-20-RESP-001 — Below about 800px the diff pane collapses to zero width

- **Category:** Responsive
- **Severity:** High
- **Confidence:** High (measured)
- **Location:** `frontend/src/App.tsx` inner `SplitView` (`defaultWidth={420}`, `minWidth={280}`,
  not `collapsible`) inside the outer `SplitView` (sidebar 260px); `SplitView.module.css`
  (`.right` has `overflow: hidden`); no breakpoints.
- **Evidence images:** [`04-main-700.png`](../evidence/04-main-700.png), [`05-main-400.png`](../evidence/05-main-400.png)
- **Problem:** The sidebar (about 260px) and the graph pane (about 420px) take priority, and the
  diff pane gets whatever remains. There is no layout change for narrow widths.
- **Observed behavior:** At 700px and 400px the diff pane is 0px wide. Only the sidebar and the
  commit graph are visible, so the user cannot see a diff, stage, commit or resolve a conflict.
- **Why it matters:** Tauri windows can be resized below 800px (no minimum is configured), and
  the VS Code webview is often docked into a side panel. In either case the app's core workflow
  is unavailable, with no message explaining why. (Rated High, not Critical, because the default
  window is 1200px and the main desktop workflow at that width is intact.)
- **Recommendation:** Set a sensible minimum window size in `tauri.conf.json` as an immediate
  guard. In the UI, below about 900px collapse to one pane at a time (history list, then detail
  with a back control) or auto-collapse the sidebar and let the graph pane shrink to its minimum,
  and give the diff pane a minimum width so it never reaches zero.
- **Implementation notes:** `SplitView` already supports `collapsible`; the outer sidebar can
  auto-collapse via `matchMedia` or a `ResizeObserver` on the container. A list/detail mode
  needs one piece of shared state (`selectedRow` already exists).
- **Verification:** Playwright/E2E check at 700px that the diff pane is visible after selecting a
  row; unit test for the collapse rule.

### AUD-2026-09-20-RESP-002 — The header's "+" button scrolls out of view when tabs overflow

- **Category:** Responsive / Navigation
- **Severity:** Low
- **Confidence:** Medium (measured overflow; scrolled-away state inferred from layout)
- **Location:** `frontend/src/components/RepoTabs.tsx:76-104` (the "+" button is inside the
  scrolling `tablist`), `RepoTabs.module.css:7`.
- **Evidence images:** [`04-main-700.png`](../evidence/04-main-700.png)
- **Problem:** The Add button is a child of the scrolling tab container, so with more open repos
  than fit, it is scrolled away with them. At 700px with three tabs the container overflows (515
  vs 359px) and the third tab and "+" are off-screen with no scroll affordance.
- **Why it matters:** Opening another repo is not reachable from the header without scrolling a
  strip that shows no scrollbar or fade.
- **Recommendation:** Pin the "+" outside the scroll area, add an overflow indicator or a "tabs"
  dropdown, and scroll the active tab into view on switch. Note that the command palette already
  offers repo switching as a fallback.

### AUD-2026-09-20-RESP-003 — At medium widths the hunk header wraps and long unbreakable tokens are clipped

- **Category:** Responsive / Visual
- **Severity:** Medium
- **Confidence:** Medium (observed in a screenshot; CSS cause not pinned down)
- **Location:** `frontend/src/components/DiffView.module.css:1-12` (`.hunkHeader` is a flex row
  without wrap), `.line` (`white-space: pre-wrap`), `SplitView.module.css` (`.right` overflow).
- **Evidence images:** [`03-main-1000-merge.png`](../evidence/03-main-1000-merge.png)
- **Problem:** At 1000px the diff pane is about 300px wide. The `@@ -40,6 +40,7 @@` header wraps
  over two lines, "Discard Hunk" stacks onto two lines, and the long identifier in the harness
  data is cut off at the right edge with no horizontal scroll to reach the rest.
- **Observed behavior:** In the 1000x700 screenshot, `-  const veryLongLineThatShouldOver` ends
  at the pane edge.
- **Why it matters:** Hidden code in a diff means an incomplete review; `pre-wrap` only breaks at
  whitespace, so a single long token (minified code, a URL, a lockfile hash) is clipped.
- **Recommendation:** Add `overflow-wrap: anywhere` (or `word-break: break-word`) to `.line`,
  make the hunk header `flex-wrap: wrap` with `white-space: nowrap` on its text, or give `.rightInner`
  explicit `overflow-x: auto`. Verify with a long-token fixture.

## 5. Coverage gaps, open questions, and accepted risks

- No tablet, phone or touch device was tested; this is a desktop product and no mobile target is
  documented in `docs/ARCHITECTURE.md`. The narrow-viewport findings apply to small windows and
  docked VS Code panels only.
- The VS Code webview's actual minimum width and its behavior with the sidebar collapsed were not
  observed (the extension host was not run).
- Browser zoom (200%) effectively reduces the CSS width and would probably trigger RESP-001 at
  1440px; not tested.
- The cause of the clipped token (RESP-003) is not confirmed. `overflow-y: auto` on `.rightInner`
  implies `overflow-x: auto`, so a scrollbar may be hidden by an ancestor's overflow.

## 6. Strengths and controls that reduced risk

- Pane sizes persist (`storageKey`) and dividers are keyboard operable with arrow keys.
- The page itself never scrolls horizontally at 400px (`scrollWidth` 400).
- Text truncation is used deliberately for long names (tab labels, branch names, commit
  summaries) with the full path available as a `title`.
- The sidebar can be collapsed (`collapsible` with double-click on the divider), giving users a
  manual way to reclaim space.
