# Deep-dive 05: Feedback, states, forms, and content

## 1. Purpose, scope, and audit questions

This deep-dive covers system status and feedback (loading, success, error), empty and first-use
states, the state model of merge and rebase, form design, and UX writing (labels, casing,
terminology).

Audit questions: After each significant action, does the user learn what happened? Does the UI
ever show a false state while loading? When an action is unavailable, is the reason visible? Is
each concept named once and cased consistently?

## 2. Architecture/execution path examined

- Feedback: `primitives/InlineError.tsx`, `TransferPanel.tsx`, `state/useMutationRunner.ts`,
  `state/useAppState.ts` (error, pending, transfer, pullOutcome).
- States: `DiffPane.tsx`, `DiffView.tsx`, `RebaseProgressPanel.tsx`, `CommitBox.tsx`,
  `RepoPicker.tsx`, `App.tsx` (repositoryOperationDisabled and its reason).
- Forms: `BranchTree.tsx` (new branch, remote and credential dialogs), `TagPanel.tsx`,
  `WorktreePanel.tsx`, `PullRequestPanel.tsx`.
- Content: all user-facing strings in the files above, `ShortcutHint.tsx`, `lib/commands.ts`.

## 3. Evidence reviewed and checks run

- Harness scenarios `main`, `empty`, `merge` and `rebase`, plus an injected transport error
  (`window.__publishError`) and the New Branch flow, screenshotted at 1440x900.
- Static reading of the loading paths: both file-section components initialize
  `useState<DiffHunk[]>([])` (`DiffPane.tsx:108,599`) and `DiffView.tsx:33` renders
  `No differences` for an empty array.
- Static sweep of string literals and `aria-label`s for casing and terminology (results in
  FB-007).
- No toast, snackbar or status-region component exists in `frontend/src` (searched for `toast`,
  `Toast`, `snackbar`).

## 4. Findings

### AUD-2026-09-20-FB-001 — Every diff shows "No differences" while it is loading

- **Category:** UX / Feedback
- **Severity:** High
- **Confidence:** High (code trace; the flash was not captured as a frame)
- **Location:** `frontend/src/components/DiffPane.tsx:108,599` (initial `hunks` is `[]`),
  `frontend/src/components/DiffView.tsx:32-34`.
- **Evidence images:** [`01-main-1440-light.png`](../evidence/01-main-1440-light.png)
- **Problem:** "Empty array" is used both as the initial (loading) value and as the real
  "no hunks" result. Until `getWorkingDiff` or `getCommitDiff` resolves, each file section renders
  the text "No differences".
- **Observed behavior:** From code: every file section mounts with an empty list and shows the
  message until its diff arrives. The message is also the only text for a binary file or a
  mode-only change, where "no differences" is false.
- **Why it matters:** A user glancing at a large change sees "No differences" under files that do
  have changes, and it reads as an error or a reliable statement. The problem scales with the
  number of files, because every diff is fetched eagerly (see PERF-001).
- **Recommendation:** Represent the state as `DiffHunk[] | null`, render a skeleton or "Loading
  diff…" for `null`, and use distinct copy for binary and mode-only changes ("Binary file changed
  — no text diff").
- **Verification:** Test with a client whose `getWorkingDiff` is pending: the text "No differences"
  must not appear; a resolved empty list shows the true empty copy.

### AUD-2026-09-20-FB-002 — Merge and rebase are invisible as a global state, and disabled actions do not say why

- **Category:** UX / Feedback
- **Severity:** High
- **Confidence:** High (scenarios rendered)
- **Location:** `frontend/src/App.tsx:86-110` (`repositoryOperationDisabled`,
  `operationDisabledReason`), `DiffPane.tsx:552-580`, `RebaseProgressPanel.tsx:23-26`,
  `CommitBox.tsx` (`disabled` prop), sidebar "Pull" button.
- **Evidence images:** [`12-merge-conflict.png`](../evidence/12-merge-conflict.png), [`13-rebase-in-progress.png`](../evidence/13-rebase-in-progress.png)
- **Problem:** While merging or rebasing, the only indicators are a pre-filled commit message plus
  "Abort merge" (merge) or a "Rebase in progress / Step 2 of 5" card at the bottom of the diff
  pane (rebase). There is no banner in the header or above the graph. "Commit" and "Continue
  Rebase" are disabled (because conflicts remain) with no visible explanation; "Pull" and other
  sidebar actions are disabled and their reason exists only in a `title` attribute (45 `title=`
  attributes in the tree).
- **Observed behavior:** In the merge scenario the conflicted file appears mid-scroll among other
  file sections; "Commit" is disabled with no text. In the rebase scenario "Continue Rebase" is
  disabled and greyed out.
- **Why it matters:** The user is in a mode that constrains everything, and the interface does not
  say which mode, how far along it is, what remains, or why the primary button is dead.
  `title` tooltips do not show on disabled buttons in some engines and are not available on touch
  or to most assistive technology.
- **Recommendation:** Add a persistent status strip below the header ("Merging feature/x into
  main · 1 conflict · Next conflict · Abort", "Rebasing 2 of 5 · 1 conflict"). Show a visible
  one-line reason near the disabled primary action ("Resolve 1 conflict to continue"), tied with
  `aria-describedby`. Reuse `operationDisabledReason` for that text.
- **Verification:** Tests that the strip renders per state and that the reason text is present
  whenever Commit or Continue is disabled.

### AUD-2026-09-20-FB-003 — No positive feedback after actions

- **Category:** UX / Feedback
- **Severity:** Medium
- **Confidence:** Medium (no toast component found; success text after fetch/push/pull was not
  exercised)
- **Location:** `frontend/src` (no toast or status-region primitive);
  `state/useMutationRunner.ts`, `state/useRemoteTransferActions.ts`.
- **Problem:** Stage, commit, checkout, discard, branch create and delete change the list and
  produce no message. Errors are surfaced through `InlineError`, and transfers through a modal
  overlay that simply disappears.
- **Why it matters:** "Did that actually commit?" A user who commits and sees the dock clear and a
  new row appear may infer success, but a push, fetch or pull that finishes with nothing visible
  gives no confirmation of what changed.
- **Recommendation:** Add a single `aria-live="polite"` status region and a toast component for
  results ("Committed a1b2c3d to main", "Pushed 3 commits to origin/main", "Already up to
  date"), with an Undo action where possible (branch delete, discard).

### AUD-2026-09-20-FB-004 — Errors are raw, undirected, and shift the layout

- **Category:** UX / Content
- **Severity:** Medium
- **Confidence:** High (rendered)
- **Location:** `frontend/src/components/primitives/InlineError.tsx`,
  `DiffPane.tsx:132,616,705` (`setError(String(err))`), `App.tsx:559-565`.
- **Evidence images:** [`10-transport-error-banner.png`](../evidence/10-transport-error-banner.png)
- **Problem:** Errors are shown as the raw string of the thrown value. Injecting a transport error
  rendered "Transport failed: sidecar exited unexpectedly (code 1)" in a full-width banner with a
  dismiss button only: no Retry, no "copy details", no guidance. The banner appears above the
  workspace and pushes it down by about 25px.
- **Why it matters:** Technical text without a next step leaves the user stuck, and the layout
  shift moves the controls the user was about to click.
- **Recommendation:** Map known error kinds to plain-language messages with an action (Retry,
  Reopen repository, Copy details), reserve the banner's space or overlay it, and keep the raw
  text under a "Details" disclosure.

### AUD-2026-09-20-FB-005 — The inline "New Branch" form is thin and disturbs the sidebar layout

- **Category:** UX / Forms
- **Severity:** Medium
- **Confidence:** High (rendered; validation behavior read from code only in part)
- **Location:** `frontend/src/components/BranchTree.tsx:824` (`New Branch…` calls
  `onOpenCreateBranchDraft("HEAD")`), the draft form at `:606`, submit handler at `:501`, `BranchTree.module.css`.
- **Evidence images:** [`09-new-branch-form.png`](../evidence/09-new-branch-form.png)
- **Problem:** The form is one text field plus Create and Cancel. It does not show the base
  ("from HEAD" or the selected commit), whether the new branch will be checked out, or the rules
  for a valid name. Opening it inserts a row at the top of the Branches section, pushes the list
  down and produces a nested scrollbar inside the sidebar scroller, which clips the "Upstream"
  block.
- **Why it matters:** Creating a branch from a commit versus HEAD is a real decision that the UI
  hides, and nested scroll containers make part of the sidebar hard to reach.
- **Recommendation:** Show "New branch from main" (or the commit id), add a "Check out after
  creating" option, validate ref names inline with a message tied to the field, and avoid the
  nested scroller by letting the sidebar be the only scroll container for these sections.

### AUD-2026-09-20-FB-006 — First-use and empty states offer no guidance

- **Category:** UX / Onboarding
- **Severity:** Medium
- **Confidence:** High (rendered)
- **Location:** `frontend/src/components/RepoPicker.tsx:88-146`, `RepoPicker.module.css`.
- **Evidence images:** [`06-empty-state.png`](../evidence/06-empty-state.png)
- **Problem:** With no open repository the screen is one card: "Open Folder", "Open Workspace
  Root", the recent list (or "No recent repositories") and "No saved workspaces". There is no
  clone or init path, no hint about the command palette, and no explanation of a "Workspace".
  Recents show full absolute paths without emphasizing the folder name.
- **Why it matters:** A first-time user has one action and no orientation. "Open Workspace Root" is
  unclear.
- **Recommendation:** Lead each recent item with the repository name (path muted), add "Clone…"
  and "Initialize…" if the backend supports them, describe a workspace in one line, and surface
  Ctrl/Cmd+K. Flatten the nested card (VIS-003).

### AUD-2026-09-20-FB-007 — Capitalization and terminology are inconsistent

- **Category:** Content
- **Severity:** Low
- **Confidence:** High
- **Location:** see table.
- **Evidence images:** [`07-branch-menu.png`](../evidence/07-branch-menu.png), [`11-command-palette.png`](../evidence/11-command-palette.png)
- **Problem:** Button and menu labels mix Title Case and sentence case, and several concepts have
  more than one name.

| Concern | Examples (with location) |
|---|---|
| Title vs sentence case | "Stage Hunk", "Discard Hunk", "Open Workspace Root", "Force Delete", "Continue Rebase", "New Branch…" vs "Stage all", "Save stash", "Confirm remove", "Delete workspace" |
| Stash | "Stash" (`DiffPane.tsx:553`), "Save stash" (`lib/commands.ts`), "Stashes" (panel) |
| Working-tree changes | "Uncommitted Changes" (`CommitGraph.tsx:181`), "Changes"/"Staged" (`DiffPane.tsx`), "Unstaged changes" (aria-label) |
| Graph filtering | "Isolate branch" (`BranchTree.tsx:571`), "Show {name} in graph" (swatch), "Show all branches" (`:827`) |
| Push | "Push to origin" (palette) vs "Push current branch here" (`BranchTree.tsx:439`) |
| Delete | "Discard Hunk" then "Confirm Discard"; "Delete {name}" vs "Delete workspace"; "Confirm remove" |

- **Why it matters:** Inconsistent labels weaken scanning and make the palette, menus and buttons
  feel like separate products.
- **Recommendation:** Adopt sentence case everywhere, one noun per concept ("Stash",
  "Working tree changes"), and one phrase for graph filtering ("Show in graph", "Show only this
  branch"). Keep a short content guideline in `docs/`.

### AUD-2026-09-20-FB-008 — Small content and icon mismatches in the header

- **Category:** Content / Interaction
- **Severity:** Low
- **Confidence:** High
- **Location:** `frontend/src/components/ShortcutHint.tsx:7`, `frontend/src/App.tsx:549-556`.
- **Problem:** The hint reads literally "Ctrl/Cmd+K" on every platform, and the release-notes
  button uses a "?" (`HelpCircle`) icon that users will read as Help.
- **Recommendation:** Render the platform's modifier (⌘K or Ctrl+K), and use a "What's new" icon
  and tooltip for release notes.

### AUD-2026-09-20-FB-009 — Pull request source and target branches are free-text fields

- **Category:** Forms
- **Severity:** Low
- **Confidence:** Medium (code read; the form was not rendered)
- **Location:** `frontend/src/components/PullRequestPanel.tsx:240-256`.
- **Problem:** Source and target branch are plain text inputs with no default, autocomplete or
  list, although the local and remote branch lists are known to the app.
- **Recommendation:** Default the source to the current branch and the target to the repository's
  default branch, as selects or comboboxes.

## 5. Coverage gaps, open questions, and accepted risks

- Success feedback after fetch, push and pull was not exercised in the harness (the fake client
  returns empty strings); `pullOutcome` and `pendingPull` exist in state and may already surface
  some text. FB-003 is bounded to what the code shows.
- The Tags, Worktrees, Submodules, Reflog, Stash and Pull Requests panel states (empty, loading,
  error) and the update banner were read but not rendered.
- Real backend error messages were not seen; FB-004's wording concern is based on the injected
  transport error and on `String(err)` in the code.
- Inline validation of ref names and URLs (`validateRemoteUrls.ts`) was not exercised.

## 6. Strengths and controls that reduced risk

- Confirmation dialogs state the consequence in plain words ("This discards any unmerged commits
  and cannot be undone"), autofocus the safe action and name the item.
- Blocking behavior is centralized: `repositoryOperationDisabled` plus `operationDisabledReason`
  gives the sidebar panels one consistent reason string, which FB-002 asks to surface visibly.
- Mutation errors are surfaced rather than swallowed (`InlineError` with `role="alert"`), and
  `useMutationRunner` gives every action the same pending and rollback behavior.
- Close buttons on a busy tab are disabled with an explanation
  (`RepoTabs.tsx`, "This repo has an operation in progress"), preventing orphaned operations.
- The command palette shows a clear empty result ("No matching commands") and a keyboard hint
  footer.
