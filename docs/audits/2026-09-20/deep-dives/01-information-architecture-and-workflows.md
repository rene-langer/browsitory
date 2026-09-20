# Deep-dive 01: Information architecture and core workflows

## 1. Purpose, scope, and audit questions

This deep-dive examines whether the shell (header tabs, sidebar, commit graph, diff pane, commit
dock) matches the conceptual model of a Git client, and whether the core workflows are
discoverable and efficient: review changes, stage and commit, browse history, branch and
checkout, and sync with a remote. The product goal in `CLAUDE.local.md` is Sublime Merge-level
speed and keyboard-driven flow, so efficiency and discoverability are audited against that bar.

Audit questions: Can a user answer "who changed this, when, and why?" from the history view? Are
the primary actions (checkout, fetch, push, pull, stage, discard) reachable and labelled where a
user looks for them? Are hidden gestures documented anywhere? Does anything silently bound what
the user sees?

## 2. Architecture/execution path examined

- Shell: `frontend/src/App.tsx` (`RepoWorkspace`, nested `SplitView`s at ~lines 218-330).
- History: `components/CommitGraph.tsx`, `state/useAppState.ts` (`GRAPH_LIMIT`, refresh).
- Detail: `components/DiffPane.tsx` (`UncommittedDiffPane`, `CommitDiffPane`,
  `CommitFileSection`), `components/DiffView.tsx`.
- Branch and remote actions: `components/BranchTree.tsx` (row menus, `branchContextItems`,
  `remoteBranchItems`, `remoteFolderItems`), `lib/commands.ts` (command palette registry).
- Documentation of gestures: `docs/USER_GUIDE.md`.

## 3. Evidence reviewed and checks run

- Ran the real `App` in headless Chrome (CDP-driven) with a fake `RepoClient` harness (the
  untracked harness from the earlier polish pass, copied in temporarily and removed afterwards).
  Viewports: 1440x900, 1000x700, 700x900, 400x800; light and dark.
- Recorded the real tab order at 1440px by pressing Tab 40 times and logging the focused element.
- Opened the row context menus of a local branch and the commit graph by real mouse events and
  read the item lists from the DOM.
- Read the source paths above. `grep -in "shortcut\|double" docs/USER_GUIDE.md` returns no
  matches (no shortcut or double-click documentation).
- Not run: the real Tauri or VS Code hosts, any real repository (see Coverage gaps).

## 4. Findings

### AUD-2026-09-20-UX-001 — Selecting a commit shows files and diffs only; no commit metadata

- **Category:** UX
- **Severity:** High
- **Confidence:** High (code trace plus screenshot; the harness data does include authors and
  timestamps, and none are shown)
- **Location:** `frontend/src/components/DiffPane.tsx:678` (`CommitDiffPane`),
  `frontend/src/components/CommitGraph.tsx:205-207` (row content).
- **Evidence images:** [`01-main-1440-light.png`](../evidence/01-main-1440-light.png)
- **Problem:** `CommitDiffPane` renders a "Collapse all" toggle and one section per changed file.
  It shows no author, date, full message or body, parent commit(s) or full SHA. The graph row
  renders only `{commit.shortId} {commit.summary}` plus ref badges, with no author or date
  column, although `GraphCommit` carries `authorName`, `authorEmail` and `timestamp`.
- **Observed behavior:** Clicking any commit row replaces the right pane with file sections. There
  is no place in the UI that shows who made the commit or when.
- **Why it matters:** Identifying the author, date and full message of a commit is the primary
  reason to open a history view. Users must leave the app to answer it.
- **Recommendation:** Add a commit header at the top of `CommitDiffPane` with subject, body,
  author, absolute and relative date, full SHA with a copy action, and parents as links that call
  `onSelectRow`. Add optional author and date columns to the graph row that hide when the pane is
  narrow.
- **Implementation notes:** All fields already exist on `GraphCommit`, so this needs no backend
  change for subject, author and date; the full message body and parent list may need a
  `RepoClient` method if `GraphCommit` only carries the summary.
- **Verification:** Vitest test that selecting a commit renders author and date; visual check at
  1000px that the columns hide.

### AUD-2026-09-20-UX-002 — Commit history silently stops at 300 commits

- **Category:** UX
- **Severity:** High
- **Confidence:** Medium (code trace; no real repository exercised)
- **Location:** `frontend/src/state/useAppState.ts:35` (`GRAPH_LIMIT = 300`) and `:221`;
  `frontend/src/components/CommitGraph.tsx` (no pagination path found).
- **Problem:** The graph is fetched once with a fixed limit of 300, and the component renders all
  returned rows with no "load more" row, scroll-triggered fetch, or truncation notice.
- **Observed behavior:** Not exercised with a large repo. From the code, a repo with more than 300
  commits shows exactly 300 rows and nothing indicating that older history exists.
- **Why it matters:** Users searching for an old commit will conclude it does not exist.
- **Recommendation:** Fetch the next page when the selection nears the end of the list or the user
  scrolls near it, and show a "Showing latest 300" row with a "Load more" button as a fallback.
  Consider list virtualization when the row count grows.
- **Verification:** Test that a full page of results triggers a next-page request; manual check on
  a repo above 300 commits.

### AUD-2026-09-20-UX-003 — Checking out a local branch is a hidden gesture

- **Category:** UX / Interaction
- **Severity:** Medium
- **Confidence:** High (menu items read from the live DOM; handler read from source)
- **Location:** `frontend/src/components/BranchTree.tsx:226` (`onDoubleClick`),
  `:536-576` (local branch menu), `:476` (remote branch menu "Checkout").
- **Evidence images:** [`07-branch-menu.png`](../evidence/07-branch-menu.png)
- **Problem:** The context menu of a local branch lists Rename, Merge into current branch, Isolate
  branch and Delete. Checkout exists only as double-click or Enter on the row. The remote-branch
  menu does contain "Checkout", so the same action is available in one place and hidden in the
  other. No hint, tooltip or documentation mentions the gesture.
- **Observed behavior:** Menu opened on `docs/hunk-staging-plans` listed exactly those four
  items.
- **Why it matters:** Checkout is the most frequent branch action. First-time users will look in
  the menu and not find it.
- **Recommendation:** Add "Checkout" as the first item of the local branch menu (showing "Enter"
  as its shortcut), and mention the gesture in `docs/USER_GUIDE.md`.
- **Verification:** BranchTree test asserting the menu item exists and calls `onSwitchBranch`.

### AUD-2026-09-20-UX-004 — Hunk actions add two tab stops per hunk and have no keyboard shortcut

- **Category:** Interaction
- **Severity:** Medium
- **Confidence:** High (measured)
- **Location:** `frontend/src/components/DiffView.tsx:45-66` (per-hunk buttons),
  `frontend/src/components/DiffPane.tsx:454-469` (`handleGroupKeyDown`: only `j`/`k`/arrows/`s`
  for whole files).
- **Problem:** Every hunk renders "Stage Hunk" and "Discard Hunk" as ordinary tab stops; whole
  files get `s`, but hunks have no key.
- **Observed behavior:** With three unstaged files of three hunks each, reaching the "Staged"
  list took about 30 Tab presses (per file: Collapse, Blame, Stage, then two stops per hunk).
- **Why it matters:** Hunk-level staging is the core interactive loop in this product, and the
  keyboard path is the stated differentiator.
- **Recommendation:** Add hunk navigation (`n`/`p` or `]`/`[`) and stage/unstage (`Enter` or `s`)
  on the focused hunk, and discard behind a modified key. Keep the buttons for pointer users but
  consider `tabIndex={-1}` once the shortcuts exist.
- **Verification:** Testing Library keyboard test staging the second hunk with keys only.

### AUD-2026-09-20-UX-005 — Discard Hunk is confirmed in place with no disarm and no undo

- **Category:** UX / Trust
- **Severity:** Medium
- **Confidence:** High (code trace)
- **Location:** `frontend/src/components/DiffView.tsx:53-66`.
- **Problem:** The first click turns "Discard Hunk" into "Confirm Discard" at the same position.
  The armed state has no timeout, does not disarm on blur or Escape, and is not visually
  distinguished as destructive. The discarded changes are not recoverable afterwards.
- **Observed behavior:** From code: the armed state persists until the hunk list changes.
- **Why it matters:** A double-click, or a stray second click after reading, discards work
  irreversibly. The button gives no warning that the action cannot be undone.
- **Recommendation:** Give the armed state danger styling and revert it on blur, Escape or after
  about 4 seconds. State "cannot be undone" in the label or a tooltip. If `git-core` can save the
  discarded patch (for example to a stash-like ref), offer a real Undo after discard.
- **Verification:** Test that blur disarms; if Undo is built, a round-trip test in `git-core`.

### AUD-2026-09-20-UX-006 — Keyboard shortcuts are undisclosed

- **Category:** UX / Content
- **Severity:** Low
- **Confidence:** High
- **Location:** `frontend/src/components/CommitBox.tsx:53` (Ctrl/Cmd+Enter),
  `DiffPane.tsx:454-469` (`j`, `k`, `s`), `CommitGraph.tsx:92` (`handleKeyDown`: arrows, Home, End, PageUp,
  PageDown, Enter to open the menu), `docs/USER_GUIDE.md`.
- **Problem:** The shortcuts are advertised only through `aria-keyshortcuts`. The commit shortcut
  appears neither in the placeholder ("Commit message") nor in the button. There is no cheat
  sheet, and the user guide does not mention them.
- **Why it matters:** Recognition over recall; speed users cannot adopt what they cannot find.
- **Recommendation:** Add the shortcut to the Commit button tooltip and label, show shortcuts
  next to menu and palette entries, and add a "?" shortcut sheet.

### AUD-2026-09-20-UX-007 — Diff view lacks the fidelity expected of a Sublime Merge-class tool

- **Category:** UX / Visual
- **Severity:** Medium
- **Confidence:** High (code trace; the harness shows the rendering)
- **Location:** `frontend/src/components/DiffView.tsx:37-84`, `DiffView.module.css`.
- **Evidence images:** [`01-main-1440-light.png`](../evidence/01-main-1440-light.png)
- **Problem:** Each line renders as an origin marker plus content. There are no line numbers,
  syntax highlighting, word-level change highlighting, side-by-side mode or line-level staging.
- **Why it matters:** Reviewing a change and staging part of it requires line context. Line
  numbers also make review comments and conflict discussion possible.
- **Recommendation:** Add old and new line-number gutters first (cheap; the hunk header has
  `oldStart` and `newStart`). Then word-level highlights, then a unified/split toggle. Syntax
  highlighting and line-level staging are larger and can follow.

### AUD-2026-09-20-UX-008 — Fetch, push and pull have no primary entry point

- **Category:** UX / Findability
- **Severity:** Low
- **Confidence:** Medium (code and menu reading; not verified against real usage)
- **Location:** `BranchTree.tsx:420,439` (remote-folder menu "Fetch", "Push current branch here"),
  the "Upstream" block in the Branches section (Pull, Clear upstream), `lib/commands.ts`.
- **Problem:** The three most common sync actions sit in a remote's context menu, an Upstream
  block that scrolls with the branch list, and the command palette. The header has no sync
  controls and shows no ahead/behind counts.
- **Why it matters:** Sync is a daily action, and the user cannot see whether they are ahead or
  behind without acting.
- **Recommendation:** Add Fetch, Pull and Push buttons to the header or graph toolbar with
  ahead/behind counts.

## 5. Coverage gaps, open questions, and accepted risks

- Real repositories were not opened, so graph pagination (UX-002) and metadata fields beyond
  `GraphCommit` (UX-001) are code-derived.
- Search and filter in the graph were not found in the code; whether that is intended for a later
  phase was not established.
- Whether double-click checkout is deliberate in the design (it is what Sublime Merge does) was
  not established; the finding is about discoverability, not the gesture.

## 6. Strengths and controls that reduced risk

- The command palette (`CommandPalette.tsx`) is a strong keyboard entry: grouped results, filter
  and rank, `combobox` semantics with `aria-activedescendant`, and focus restored on close.
- Graph and file lists have a single tab stop with arrow/`j`/`k` navigation, avoiding a thousand
  tab stops (design rationale documented in `ListRow.tsx`).
- The "Uncommitted Changes" row at the top of the history matches a familiar mental model.
- Branch visibility swatches use a hollow ring for the hidden state (not only dimming) and expose
  `aria-pressed` (`BranchTree.module.css:86-100`).
