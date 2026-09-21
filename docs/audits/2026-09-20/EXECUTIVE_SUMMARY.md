# Browsitory UI/UX Audit — Executive Summary

**Audit date:** 2026-09-20
**Auditor:** Claude (Anthropic), acting as lead product-design and accessibility auditor in an
interactive session with the repository owner
**Deliverables:** this summary, six deep-dives under `deep-dives/`, the commissioning brief in
`AUDIT_PROMPT.md`, and screenshots under `evidence/`

## Audit identity and limitations

- **Audited commit:** `0695a6e` ("fix(frontend): keep accordion header actions clear of the
  overlay scrollbar"), branch `fix/ux-polish-pass`. Working tree: one modified file
  (`graphify-out/cache/last_query_stamp`, tooling cache) and one untracked file
  (`docs/superpowers/plans/2026-09-19-ux-polish-pass.md`, the plan for the polish pass in
  progress). No production source was modified by this audit.
- **Relationship to earlier work:** this is a UI/UX audit, complementary to the engineering audit
  of 2026-09-05 (`../2026-09-05/`), whose frontend deep-dive is `05-frontend-quality-and-
  accessibility.md`. One of its findings (context-menu keyboard reachability, FE-001) is
  partly resolved in the current tree (`ContextMenu` now has arrow-key navigation) and is not
  repeated. The in-flight polish pass on this branch (branch tree, ListRow key handling, toolbar,
  context menu, palette, repo picker, tabs) covers some of the same components; findings here
  were checked against the current source, not against that plan.
- **Method:** the real `App` was rendered in headless Chrome (Chrome DevTools Protocol, real
  key and mouse input events) with a fake `RepoClient` supplying multi-lane graph data, nested and
  long branch names, two remotes, and merge, rebase, empty, and error scenarios. The harness is
  the untracked one left from the earlier polish pass; it was copied into `frontend/` temporarily
  and removed afterwards, so the working tree is unchanged. Viewports: 1440x900 (light and dark),
  1000x700, 700x900, 400x800. Source was read for everything the harness could not show.
  Screenshots are committed under `evidence/` (index and reproduction notes in
  `evidence/README.md`) and linked from the findings they support.
- **Environment:** Linux, Google Chrome headless, Node 24. No display server for the Tauri
  window, no VS Code host, no real Git backend, no screen reader.
- **Commands actually run:** the harness `vite` dev server plus CDP scripts (tab-order walk,
  focus-restoration checks, bounding-box measurement, viewport resizing), and repeated static
  `grep` sweeps over `frontend/src` (hard-coded values, `@media`, ARIA attributes, `title=`
  counts, toast/snackbar). The repository's own build, lint and test suites were **not** re-run:
  no source changed, and the engineering audit already established the baseline.
- **Limitations that shape the findings:** see "Audit limitations" below. In particular, findings
  tagged *code trace* in their confidence line were not reproduced at runtime, and no timing was
  measured.

## Scope and method

The commissioning brief listed 26 review dimensions. They were folded into six deep-dives, chosen
by where evidence for each dimension actually lived, rather than mirrored one to one.

| # | Deep-dive | Covers (brief sections) | Included because |
|---|-----------|--------------------------|-------------------|
| 1 | Information architecture and core workflows | 2, 3, 4, 7, 16, 18, 21 | The core Git workflows (review, commit, branch, sync, discard) are where a keyboard-first tool wins or loses; the search for hidden gestures, missing metadata and silent limits needed direct evidence |
| 2 | Visual design and design system | 5, 6, 15, 19 | A real token layer and primitives exist, so the audit could compare screens against them and sweep for drift mechanically |
| 3 | Accessibility and keyboard behavior | 11 | The product goal is keyboard-driven speed, and focus, ARIA and target-size defects were directly measurable in a browser |
| 4 | Responsive behavior and narrow-window layout | 12, 13 | The app has no breakpoints and ships to a resizable Tauri window and a docked VS Code panel. Mobile was reduced to a narrow-window stress test, since there is no mobile target |
| 5 | Feedback, states, forms, and content | 8, 9, 10, 14 | Loading, merge/rebase, error and first-use states, plus forms and UX writing, are where users lose their bearings |
| 6 | Perceived performance | 17 | Implementation choices (eager diffs, always-mounted tabs) have visible UX consequences, though only reasoned from code here |

Brief sections 20 (screen-by-screen), 22 (heuristics), 25 (priorities) and 26 (final deliverable)
are answered in this summary. Sections 1 (methodology) and 23 to 24 (format, severity) are
applied throughout. Nothing from the brief was dropped; where the product does not have the
surface (payments, authentication flows, import/export, collaboration), no finding was invented.

## Severity table

| ID | Title | Severity | Confidence | Deep-dive |
|----|-------|----------|------------|-----------|
| A11Y-001 | Focus is lost when a `ConfirmDialog` or `FormDialog` closes | **High** | High (reproduced) | [03](deep-dives/03-accessibility-and-keyboard.md) |
| FB-001 | Every diff shows "No differences" while it is loading | **High** | High (code trace) | [05](deep-dives/05-feedback-states-forms-and-content.md) |
| FB-002 | Merge and rebase are invisible as a global state; disabled actions do not say why | **High** | High (rendered) | [05](deep-dives/05-feedback-states-forms-and-content.md) |
| RESP-001 | Below about 800px the diff pane collapses to zero width | **High** | High (measured) | [04](deep-dives/04-responsive-and-layout.md) |
| UX-001 | Selecting a commit shows files and diffs only; no commit metadata | **High** | High | [01](deep-dives/01-information-architecture-and-workflows.md) |
| UX-002 | Commit history silently stops at 300 commits | **High** | Medium (code trace) | [01](deep-dives/01-information-architecture-and-workflows.md) |
| UX-003 | Checking out a local branch is a hidden gesture | Medium | High | [01](deep-dives/01-information-architecture-and-workflows.md) |
| UX-004 | Hunk actions add two tab stops per hunk and have no shortcut | Medium | High (measured) | [01](deep-dives/01-information-architecture-and-workflows.md) |
| UX-005 | Discard Hunk is confirmed in place, with no disarm and no undo | Medium | High (code trace) | [01](deep-dives/01-information-architecture-and-workflows.md) |
| UX-007 | Diff view lacks line numbers, highlighting and word-level diff | Medium | High | [01](deep-dives/01-information-architecture-and-workflows.md) |
| VIS-001 | The commit dock is heavy, double-bordered, and shows diff content beneath it | Medium | High / Medium | [02](deep-dives/02-visual-design-and-design-system.md) |
| VIS-002 | Status and severity carry no color semantics; no success/warning/info tokens | Medium | High | [02](deep-dives/02-visual-design-and-design-system.md) |
| A11Y-002 | Pointer targets below 24x24 CSS pixels | Medium | High (measured) | [03](deep-dives/03-accessibility-and-keyboard.md) |
| A11Y-003 | `tablist` structure is invalid and has no arrow-key navigation | Medium | High | [03](deep-dives/03-accessibility-and-keyboard.md) |
| A11Y-004 | Forms expose no validation or requirement semantics | Medium | Medium | [03](deep-dives/03-accessibility-and-keyboard.md) |
| A11Y-005 | Workspace deletion uses a non-modal raw `<dialog open>` | Medium | High | [03](deep-dives/03-accessibility-and-keyboard.md) |
| RESP-003 | Hunk header wraps and long unbreakable tokens are clipped at medium widths | Medium | Medium | [04](deep-dives/04-responsive-and-layout.md) |
| FB-003 | No positive feedback after actions | Medium | Medium | [05](deep-dives/05-feedback-states-forms-and-content.md) |
| FB-004 | Errors are raw, undirected, and shift the layout | Medium | High | [05](deep-dives/05-feedback-states-forms-and-content.md) |
| FB-005 | The inline "New Branch" form is thin and disturbs the sidebar layout | Medium | High | [05](deep-dives/05-feedback-states-forms-and-content.md) |
| FB-006 | First-use and empty states offer no guidance | Medium | High | [05](deep-dives/05-feedback-states-forms-and-content.md) |
| PERF-001 | Every diff is fetched and rendered eagerly; inactive tabs stay mounted | Medium | Medium (code trace) | [06](deep-dives/06-perceived-performance.md) |
| UX-006 | Keyboard shortcuts are undisclosed | Low | High | [01](deep-dives/01-information-architecture-and-workflows.md) |
| UX-008 | Fetch, push and pull have no primary entry point | Low | Medium | [01](deep-dives/01-information-architecture-and-workflows.md) |
| VIS-003 | Card-in-card chrome in the repo picker | Low | High | [02](deep-dives/02-visual-design-and-design-system.md) |
| VIS-004 | "Uncommitted Changes" row does not align with commit rows | Low | High | [02](deep-dives/02-visual-design-and-design-system.md) |
| A11Y-006 | `ContextMenu` closes on pointer leave and is not clamped to the viewport | Low | Medium | [03](deep-dives/03-accessibility-and-keyboard.md) |
| A11Y-007 | Reduced motion and icon tooltips only partly handled | Low | High | [03](deep-dives/03-accessibility-and-keyboard.md) |
| RESP-002 | The header "+" button scrolls out of view when tabs overflow | Low | Medium | [04](deep-dives/04-responsive-and-layout.md) |
| FB-007 | Capitalization and terminology are inconsistent | Low | High | [05](deep-dives/05-feedback-states-forms-and-content.md) |
| FB-008 | Shortcut hint reads "Ctrl/Cmd+K"; release notes use a help icon | Low | High | [05](deep-dives/05-feedback-states-forms-and-content.md) |
| FB-009 | Pull request branches are free-text fields | Low | Medium | [05](deep-dives/05-feedback-states-forms-and-content.md) |
| PERF-002 | Transfer sizes stop at KB, and the transfer modal has no cancel | Low | High / Medium | [06](deep-dives/06-perceived-performance.md) |

Finding IDs in the tables and deep-dives are shortened. The full stable ID is
`AUD-2026-09-20-<ID>` (for example `AUD-2026-09-20-A11Y-001`).

**33 findings: Critical 0, High 6, Medium 16, Low 11, Informational 0.**

**Critical findings: none.** Nothing prevents the primary desktop workflow at the default window
size (1200x800), and the audit found no unguarded destructive action. RESP-001 was initially
considered Critical; it is rated High because it requires a window narrower than about 800px,
which the default size avoids, though the Tauri window has no minimum size and the VS Code panel
can be docked narrow.

**Read the High-severity rows in this order:** A11Y-001 and FB-001 are both one-component fixes with
wide reach. FB-002 and UX-001 are the two most consequential user-facing gaps. RESP-001 is
cheapest to contain (a minimum window size), and UX-002 needs a real repository to confirm.

## Readiness assessment

**Desktop workflow at the default window size: usable and coherent, with six High-severity gaps
to close before the UX can be called Sublime Merge-class.** The visual layer is disciplined
(tokens, primitives, both themes), the keyboard model is unusually deliberate for a Git GUI
(command palette, single-tab-stop lists, `j`/`k`/`s`), and destructive actions are consistently
confirmed. The gaps are mostly about **missing information** (commit metadata, merge and rebase
state, loading versus empty), **hidden affordances** (double-click checkout, undisclosed
shortcuts) and **focus and ARIA details** that a keyboard-first product cannot afford
(A11Y-001, A11Y-003). None of these indicate a systemic quality problem; most are small, local
fixes to shared primitives.

**Confidence: Medium to High.** Findings tagged "reproduced" or "measured" were observed in a real
browser. About a third of the findings, including all performance conclusions and UX-002, rest on
code reading and were not exercised against a real repository or the real Tauri and VS Code hosts.

## Screens audited

| Screen | Primary goal | Verdict | Main findings |
|---|---|---|---|
| Main workspace, 1440px | Review, stage and commit changes; browse history | Dense, clear hierarchy, strong keyboard model; weak on commit detail and fidelity | UX-001, UX-004, UX-007, VIS-001, FB-001 |
| Main workspace, 1000px | Same, in a smaller window | Usable, but hunk headers wrap and tokens clip | RESP-003 |
| Main workspace, 700px and below | Same | Diff pane is 0px wide; core workflow unavailable | RESP-001, RESP-002 |
| Sidebar: Branches | Switch, create, merge, delete; graph visibility | Good tree; hidden checkout, tiny targets, thin create form | UX-003, A11Y-002, FB-005, FB-007 |
| Sidebar: other panels | Stashes, worktrees, tags, PRs | Read, not rendered | A11Y-004, FB-009 (code only) |
| Commit graph | Find and select a commit | Clear lanes and badges; no author or date, fixed 300 cap | UX-001, UX-002, VIS-004 |
| Diff pane and commit dock | Stage, discard, commit | Efficient by pointer, expensive by keyboard | UX-004, UX-005, VIS-001, VIS-002 |
| Merge, rebase and conflicts | Finish an in-progress operation | No global state, dead buttons unexplained | FB-002, VIS-002 |
| Command palette | Run any command by keyboard | Best-in-class screen of the app | (none; see strengths) |
| Confirm and form dialogs | Confirm destructive actions; enter names | Clear copy; focus lost on close | A11Y-001, A11Y-005 |
| Repo picker (empty state) | Open a repository | Minimal, unguided | FB-006, VIS-003 |
| Transfer overlay | Watch fetch and push | Accurate for small transfers only; no cancel | PERF-002 |
| Header, tabs, release notes | Switch repositories | Functional; ARIA and overflow gaps | A11Y-003, RESP-002, FB-008 |

## Journey summary

| Journey | Friction | Fix |
|---|---|---|
| Stage and commit | Hunk-level actions are pointer-first, the dock is heavy, no confirmation after committing | UX-004, VIS-001, FB-003 |
| Review history | No author, date or body; silent 300-commit cap | UX-001, UX-002 |
| Merge or rebase with conflicts | No persistent state indicator, dead buttons without reasons, conflicted files not visually flagged | FB-002, VIS-002 |
| Branch and checkout | Checkout is a hidden gesture; create form hides its base | UX-003, FB-005 |
| Sync with a remote | Fetch, push and pull live in menus; no ahead/behind; no success message | UX-008, FB-003 |
| Discard work | In-place confirmation, no undo | UX-005 |
| First launch | One card, no guidance | FB-006 |

## Heuristic mapping

| Heuristic | Where it fails | Findings |
|---|---|---|
| Visibility of system status | Merge and rebase state, success results, false "No differences", transfer size | FB-001, FB-002, FB-003, PERF-002 |
| Match to the real world | "Isolate branch", "Open Workspace Root", "?" for release notes | FB-007, FB-008 |
| User control and freedom | No undo for discard; no cancel for transfers | UX-005, PERF-002 |
| Consistency and standards | Mixed capitalization, raw `<dialog>`, checkout in one menu only | FB-007, A11Y-005, UX-003 |
| Error prevention | Good overall; in-place discard confirmation is the weak spot | UX-005 |
| Recognition over recall | Hidden gestures and shortcuts | UX-003, UX-006 |
| Flexibility and efficiency | Strong palette and list navigation; hunk actions and sync are slow | UX-004, UX-008 |
| Minimalist design | Card-in-card, heavy dock | VIS-001, VIS-003 |
| Error recovery | Raw messages, no Retry | FB-004 |
| Help and documentation | No shortcut help; guide omits gestures | UX-006 |

## Remediation roadmap

**Immediate (small, high reach; roughly one PR each):**
1. Restore focus when `ConfirmDialog` and `FormDialog` close, and swap the raw workspace-delete
   `<dialog>` for `ConfirmDialog` (A11Y-001, A11Y-005).
2. Replace the "empty array means empty diff" state with a tri-state and add a loading and a
   binary-file message (FB-001).
3. Add "Checkout" to the local branch menu and document it (UX-003).
4. Set a minimum window size in `tauri.conf.json` as a containment for RESP-001, and give the
   diff pane a minimum width.
5. Pad hit areas of the swatches, tab close buttons and dividers to 24px (A11Y-002).

**Near-term:**
6. Build a persistent merge and rebase status strip with visible reasons for disabled actions
   (FB-002) and add conflict color semantics (VIS-002).
7. Add a commit header (author, date, body, SHA, parents) and graph columns (UX-001), then
   paginate history (UX-002).
8. Make the layout adapt below about 900px: one pane at a time or an auto-collapsing sidebar
   (RESP-001), and pin the "+" tab button (RESP-002).
9. Add hunk keyboard shortcuts and a shortcut sheet (UX-004, UX-006); harden discard (UX-005).
10. Add a shared `Field` primitive (A11Y-004) and rework the tab strip to valid ARIA (A11Y-003).
11. Add a toast and status region and improve error messages (FB-003, FB-004).

**Strategic:**
12. Move diff fetching to lazy, per-file, and unmount inactive workspaces (PERF-001); then improve
    diff fidelity: line numbers, word-level diff, split view (UX-007).
13. Add header sync controls with ahead/behind counts (UX-008), a first-use flow (FB-006), and a
    content guideline for casing and terminology (FB-007).
14. Run a screen reader pass (NVDA, VoiceOver, Orca) over the tab strip, file lists and dialogs,
    and re-measure once A11Y-001 to A11Y-003 are fixed.

## Prioritized recommendations

| # | Change | Problem addressed | User benefit | Effort | Affected | Type |
|---|--------|-------------------|--------------|--------|----------|------|
| 1 | Focus restore in dialog primitives | A11Y-001, A11Y-005 | Keyboard users keep their place after every dialog | S | `ConfirmDialog`, `FormDialog`, `RepoPicker` | Systemic |
| 2 | Tri-state diff loading | FB-001 | No false "No differences" | S | `DiffPane`, `DiffView` | Local |
| 3 | Merge/rebase status strip and visible disabled reasons | FB-002 | Users know the mode and next step | M | Header, dock, sidebar | Systemic |
| 4 | Commit header and graph columns | UX-001 | Answer "who, when, why" in-app | M | Graph, diff pane | Local |
| 5 | Status color tokens and conflict styling | VIS-002 | Conflicts are seen; color is a second channel | S | Tokens, file rows | Systemic |
| 6 | Layout adaptation and minimum size | RESP-001 | Diff usable in narrow windows and VS Code panels | M-L | `SplitView`, App layout, Tauri config | Systemic |
| 7 | `Field` primitive | A11Y-004 | Errors tied to fields | M | All forms | Systemic |
| 8 | Toast and status region | FB-003, FB-004 | Results and recoverable errors | M | Everywhere | Systemic |
| 9 | History pagination | UX-002 | Complete history | M | Graph, `useAppState` | Local |
| 10 | Hunk shortcuts and shortcut sheet | UX-004, UX-006 | Faster staging, discoverability | M | Diff pane | Local |

**Quick wins** (a single small change each): A11Y-001, FB-001, UX-003, A11Y-002, FB-008,
PERF-002 formatting, RESP-003 (`overflow-wrap`), VIS-004, A11Y-006 (drop `onMouseLeave`).

## Design system recommendations

- **Tokens to add:** `--color-warning-*`, `--color-success-*`, `--color-info-*` (bg and text pairs,
  both themes); rename `--color-danger` to a `-bg` form so it is not confused with a foreground; a
  `--size-target-min: 24px`; named breakpoints (for example 900px and 600px); a hairline
  spacing value for the 1-2px literals used in 13 places.
- **Components to add or standardize:** `Field` (label, hint, error), `Toast`/status region,
  `StatusBanner` (merge/rebase), `Badge` (refs, counts), `IconButton` with tooltip, and a real
  `Tabs` primitive. Route every confirmation through `ConfirmDialog`.
- **Rules to codify:** sentence-case labels, one noun per concept, a visible reason for every
  disabled action, focus restored by every modal, a 24px minimum target, and a rule that
  loading, empty and error are three distinct render states.
- **Keep as is:** the token discipline (zero hard-coded colors), the three-layer dark mode, the
  primitive set, and the diff's use of sign markers alongside color.

## UX debt map

| Theme | Recurring issues |
|---|---|
| Navigation | Tab strip overflow and ARIA (A11Y-003, RESP-002); hidden checkout (UX-003) |
| Forms | No validation semantics (A11Y-004); thin new-branch form (FB-005); free-text PR branches (FB-009) |
| Feedback | No success feedback, raw errors, false empty state, no state banner (FB-001 to FB-004) |
| Hierarchy | Heavy dock (VIS-001); conflict not emphasized (VIS-002); card-in-card (VIS-003) |
| Accessibility | Focus restoration, target sizes, tablist, raw dialog, menu behavior (A11Y-001 to A11Y-007) |
| Responsive | No breakpoints; diff pane vanishes; clipped tokens (RESP-001 to RESP-003) |
| Content | Casing and terminology drift; hint and icon mismatches (FB-007, FB-008) |
| Consistency | One raw dialog, checkout in one menu only, two hunk-confirm patterns (A11Y-005, UX-003, UX-005) |
| Interaction patterns | Two tab stops per hunk, undisclosed shortcuts, in-place discard (UX-004 to UX-006) |
| Performance | Eager diffs, mounted tabs, transfer display (PERF-001, PERF-002) |

## Controls and strengths worth preserving

- **A real design system, mechanically enforced:** no hard-coded colors outside `tokens.css`, one
  font-size literal, three radius literals, and shared primitives used across almost every screen.
- **Three-layer dark mode:** `prefers-color-scheme`, explicit `data-theme`, and `color-scheme` for
  native controls, so the in-app toggle works in both directions.
- **A keyboard model designed on purpose:** command palette with `combobox` semantics, a single tab
  stop per list with arrow and `j`/`k` movement, `s` to stage, `aria-keyshortcuts` declared, and
  `ContextMenu` with roving focus.
- **Safe destructive actions:** `ConfirmDialog` names the consequence, autofocuses Cancel, and
  routes Escape through `onCancel`. Busy tabs cannot be closed.
- **Change is not conveyed by color alone in the diff** (sign markers), and the hidden-branch swatch
  uses a hollow ring rather than dimming.
- **Solid, present accessibility foundations:** native `<dialog>` and `showModal()` everywhere but
  one place, `role="alert"` errors, live regions in seven files, visible 2px focus outlines.
- **`Overlay` already restores focus correctly**, so the fix for A11Y-001 has a working in-repo
  reference.

## Audit limitations

- **No real backend or real repositories.** Everything ran against a fake `RepoClient`. Real
  latency, real Git and forge error messages, the 300-commit cap on a real history, success text
  after fetch, push and pull, and large change sets were not observed. UX-002, FB-003 and PERF-001
  are code-derived.
- **No screen reader or forced-colors run.** A11Y-003 and A11Y-004 are DOM and code based. The
  announcement of nested buttons inside listbox options remains an open question.
- **Chrome only, on Linux.** WebKit (macOS Tauri) and WebView2 (Windows Tauri) may differ; the
  focus-loss result in particular should be confirmed there.
- **Screens read but not rendered:** Tags, Worktrees, Submodules, Reflog, Stash, Pull Requests
  panels, the workspace editor, the update banner, the release-notes modal, Blame.
- **Not tested:** the transfer overlay's Escape behavior, browser zoom at 200%, touch input, the
  VS Code webview at narrow widths, and contrast measured on rendered pixels (token values were
  reasoned by inspection only).
- **Mobile and tablet:** not a product target; only narrow-window behavior was assessed (deep-dive
  04). No thumb-reach, gesture or one-handed findings are claimed.
- **Not applicable to this product and therefore not audited:** payments, authentication,
  import/export, permissions, collaboration.
- **Screenshots** show harness data, not a real repository, and cover only the states listed in
  `evidence/README.md`. Some findings (for example focus loss, target sizes, tab order) were
  measured by script and have no image.
