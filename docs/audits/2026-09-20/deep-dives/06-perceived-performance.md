# Deep-dive 06: Perceived performance

## 1. Purpose, scope, and audit questions

This deep-dive connects UI cost to implementation decisions: how much work the interface does on
mount and on each state change, and what the user perceives as a result (lag, flashing content,
layout shifts, unbounded lists).

Audit questions: What is fetched and rendered eagerly? What multiplies with the number of files,
commits or open repositories? Does progress reporting stay accurate for large operations?

## 2. Architecture/execution path examined

- Diff loading: `frontend/src/components/DiffPane.tsx` (`UncommittedFileSection` effect at
  ~lines 111-135, `CommitFileSection`), `DiffView.tsx`.
- Workspace mounting: `frontend/src/App.tsx:123` (`display: active ? "contents" : "none"`) and the
  `openRepos.openRepos.map(...)` render at ~line 605.
- History rendering: `CommitGraph.tsx` (`useMemo` over `assignLanes`), `useAppState.ts`.
- Transfer feedback: `TransferPanel.tsx`.

## 3. Evidence reviewed and checks run

- Static read of the effect dependency lists and render paths above.
- In the harness, three open repositories produced three full copies of the sidebar and diff DOM
  (the hidden ones were found while counting headings: "Branches", "Stashes" and the other
  section headings appear three times).
- **No timings were measured.** The harness uses an in-memory fake client, so it cannot show real
  IPC or Git latency, and no large repository was opened. Findings here are therefore
  code-derived credible risks, not measured defects.

## 4. Findings

### AUD-2026-09-20-PERF-001 — Every changed file's diff is fetched and rendered eagerly, and refetched on every status change; inactive tabs stay mounted

- **Category:** Performance
- **Severity:** Medium
- **Confidence:** Medium (code trace; not measured on a large change set)
- **Location:** `frontend/src/components/DiffPane.tsx:111-135` (file-section effect whose
  dependency list includes `status`; the source comment explains that "every file's diff is
  fetched eagerly"), `DiffPane.tsx:70` (`UncommittedFileSection` mounted per file),
  `frontend/src/App.tsx:123,605`.
- **Problem:** Each file section calls `getWorkingDiff` on mount, and again whenever the `status`
  array identity changes, which happens after every stage, unstage or refresh. All sections render
  expanded. There is no size guard for large files, and no lazy loading for collapsed or
  off-screen sections. Every open repository's `RepoWorkspace` stays mounted and only hidden with
  CSS.
- **Observed behavior:** From code: staging one hunk in a change set of N files issues N diff
  requests and re-renders N sections. With three open tabs the DOM holds three workspaces.
- **Why it matters:** A merge, a dependency update or a generated file can list hundreds of files.
  Each stage action then costs hundreds of IPC calls and a large DOM, and users see the flash from
  FB-001 across many sections. The cost also multiplies with the number of open tabs.
- **Recommendation:** Fetch a file's diff only when its section is expanded and near the viewport
  (`IntersectionObserver`). Refetch only the affected path after a mutation. Auto-collapse files
  above a line threshold with a "Show diff" button. Unmount or freeze inactive workspaces, or
  cap eager work to the active tab.
- **Verification:** Benchmark or test with a fixture of 200 changed files: count `getWorkingDiff`
  calls after one stage action (should be 1), and measure time to first interaction.

### AUD-2026-09-20-PERF-002 — Transfer size formatting stops at KB, and the transfer modal cannot be cancelled

- **Category:** Performance / Feedback
- **Severity:** Low
- **Confidence:** High for the formatting; Medium for the modal (Escape behavior not exercised)
- **Location:** `frontend/src/components/TransferPanel.tsx:5-8,24`; `App.tsx` (the transfer
  `Overlay` has no `onClose`).
- **Problem:** `formatBytes` handles only bytes and KB, so 50 MB renders as "51200.0 KB". When
  `total` is 0 the bar shows 0% rather than an indeterminate state. The modal blocks the whole UI
  for the duration of a fetch or push with no Cancel control, and the phase is shown as the raw
  enum text (for example "Receiving").
- **Why it matters:** Large clones and pushes are exactly where progress needs to be believable
  and interruptible.
- **Recommendation:** Add MB and GB tiers, an indeterminate bar when `total` is 0, humanized phase
  labels, and a Cancel action if the backend supports cancellation (otherwise say so in the modal).

## 5. Coverage gaps, open questions, and accepted risks

- No large repository (thousands of commits, hundreds of changed files) was used. All
  performance conclusions are code-derived.
- Initial load time, route or tab-switch latency and animation frame rates were not measured.
- Commit list virtualization was not audited: the list renders every returned row (up to the
  300 limit, see UX-002). At that size this is probably fine, and it becomes relevant once
  UX-002 is fixed.
- Whether Escape dismisses the transfer overlay while the transfer continues (the `Overlay` has no
  `onClose`) was not verified at runtime.

## 6. Strengths and controls that reduced risk

- The lane layout computation is memoized (`CommitGraph.tsx`, `useMemo(() => assignLanes(...))`).
- Diff requests are guarded against out-of-order responses with an `ignore` flag in each effect
  cleanup, so stale responses cannot overwrite newer ones.
- Long lists use text truncation and a single tab stop per list, which keeps the DOM and focus
  cost predictable.
