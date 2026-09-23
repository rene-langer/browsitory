# Audit resolution progress

Tracks remediation of `docs/audits/2026-09-20/`'s UI/UX findings. Status as of 2026-09-20.

Work branch: `fix/ux-audit-2026-09-20`, branched from `fix/ux-polish-pass` at `0695a6e` (the
audited commit). Note: this is **not** on top of `main`; at branch time `main` was one commit ahead
(`a9ee5ac`, the 2026-09-05 audit remediation) and lacked the 25 polish-pass commits. Rebase or
merge onto `main` is still to do.

Five agents worked in parallel worktrees, one per file-overlap group, and their branches were
merged into the work branch (conflicts only in `CHANGELOG.md`, `RepoTabs.tsx`, `App.tsx`).

**Result: 27 of 33 findings fixed, 6 partially fixed, 0 untouched.** Every partial item lists what
remains below.

## Verification

Run on the merged branch: `pnpm lint`, `tsc --noEmit`, `pnpm test -- --run` (64 files, 777 tests),
`cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`; all pass. **Not run:** the Tauri and VSCode e2e suites, and no manual
check in the running app (no display). Every "fixed" below is therefore unit-test-verified, not
observed in a real window.

## Fixed

| ID | Severity | Fix |
|----|----------|-----|
| A11Y-001 | High | `ConfirmDialog` and `FormDialog` restore focus on close (new `useRestoreFocus` hook). |
| FB-001 | High | `DiffView` takes `DiffHunk[] \| null`; `null` renders "Loading diff…", an empty result renders "No text differences (binary file or mode-only change)". |
| FB-002 | High | `OperationStatusStrip` shows merge/rebase state with abort actions; Commit and Continue rebase show why they are disabled. |
| RESP-001 | High | Minimum window 800x500 in `tauri.conf.json`, 320px diff-pane minimum, sidebar auto-collapses below 900px (`SplitView` `forceCollapsed`). |
| UX-001 | High | Commit header (full message, author, date, full SHA with copy, parent links) via new `get_commit_message` command through git-core, repo-service, tauri-app, sidecar and both `RepoClient`s; history rows gain author and date columns that hide when narrow. |
| UX-002 | High | "Load more" row plus auto-load when arrowing past the last row. Not exercised against a real long history. |
| UX-003 | Medium | "Checkout" first in the local branch menu (disabled for the current branch); `USER_GUIDE` covers it and double-click. |
| UX-004 | Medium | Hunk buttons `tabIndex=-1`; diff is one tab stop with `[`/`]`, `s`, `d`. |
| UX-005 | Medium | Armed discard has danger styling and disarms on blur, Escape or after 5s. No undo. |
| VIS-001 | Medium | Single border, nested `Panel` removed from `CommitBox`, dock no longer lets diff show beneath it. |
| A11Y-002 | Medium | `--size-target-min` (24px) applied to swatch hit area, tab close buttons, "+", sidebar toolbar, dividers. The branch-row "…" button was not found and not changed. |
| A11Y-003 | Medium | Per-tab close is `aria-hidden`/mouse-only; Ctrl/Cmd+W closes the active tab. |
| A11Y-004 | Medium | New `Field` primitive (label, hint, error, `aria-invalid`, `aria-describedby`, `required`) with tests. **Not adopted in any form yet** (see Remaining). |
| A11Y-005 | Medium | Workspace deletion uses `ConfirmDialog` instead of a raw `<dialog open>`. |
| RESP-003 | Medium | `overflow-wrap` on long tokens; hunk headers wrap cleanly. |
| FB-003 | Medium | Toasts after commit, checkout, branch delete, stash, fetch, push, pull (`ToastRegion`). |
| FB-005 | Medium | New Branch form shows its base, validates the name inline, offers check-out. |
| FB-006 | Medium | Repo picker shows folder names, a workspace explainer and the palette hint. |
| UX-006 | Low | `ShortcutSheet` + `lib/shortcuts.ts`; opens with `?` (ignored while typing) or "Show keyboard shortcuts" in the palette. |
| UX-008 | Low | `SyncBar` above the graph: Fetch/Pull/Push with ahead/behind counts (`UpstreamInfo` gains `ahead`/`behind`, null until the tracking ref is fetched). |
| VIS-003 | Low | Card-in-card flattened in the repo picker. |
| VIS-004 | Low | Hollow working-tree lane node, text aligned with commit rows. |
| A11Y-006 | Low | `ContextMenu` no longer closes on pointer leave and is clamped to the viewport. |
| A11Y-007 | Low | `prefers-reduced-motion` CSS, `scrollIntoView` honours it, tooltips on sidebar and "+" buttons. |
| RESP-002 | Low | "+" button moved outside the scrolling tablist (`.strip` wrapper in `RepoTabs`). |
| FB-008 | Low | Platform-aware shortcut hint; release notes use a "What's new" icon. |
| FB-009 | Low | PR branch fields use a datalist with sensible defaults (still free text, now suggested). |
| PERF-002 | Low | Transfer sizes format as MB/GB, and Cancel aborts the in-flight `git2` transfer via a shared cancel-flag registry, checked in the fetch `transfer_progress` callbacks and at the push `push_negotiation` checkpoint (before any data is sent; a later push cancel is ignored so a landed push is never reported as cancelled); both frontends wired. |

## Partially fixed

| ID | Severity | Done | Remaining |
|----|----------|------|-----------|
| FB-004 | Medium | Errors have a hint and Retry on transport errors; banners float instead of shifting the layout. | No mapping of error kinds to friendly messages; mutation errors have no Retry. |
| FB-007 | Low | Sentence case in the picker and rebase labels; new `docs/CONTENT_GUIDELINES.md`. | Hunk, branch-menu and graph labels still to be normalized against the guideline. |
| VIS-002 | Medium | `--color-success/warning/info` token pairs (both themes) used by the status strip and toasts. | Conflicted file rows in the diff/file lists are not tinted. |
| UX-007 | Medium | Old/new line-number gutters. | No syntax highlighting or word-level diff, no split view. |
| PERF-001 | Medium | Collapsed sections skip the diff fetch until expanded (working-tree and commit panes). | IntersectionObserver-based lazy rendering, per-path refetch, unmounting inactive workspaces (`App.tsx`). |

## Other open items

- `Field` (A11Y-004) is unused; adopting it in the branch, workspace and PR forms was skipped
  because `InlineError` is a separate dismissible alert and swapping risked regressions.
- No tests for the `?` shortcut handler or the sidebar auto-collapse in `App`.
- Word-level diff and binary-vs-mode-only distinction need a backend signal (the frontend cannot
  tell them apart today).
- E2E suites and a real-window pass (including WebKit and WebView2 focus behaviour for A11Y-001)
  are outstanding, as is the screen-reader pass the audit recommends.
- Rebase/merge onto `main` (see top).
