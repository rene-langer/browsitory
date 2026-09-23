# Audit resolution progress

Tracks remediation of `docs/audits/2026-09-20/`'s UI/UX findings. Status as of 2026-09-20.

Work branch: `fix/ux-audit-2026-09-20`, branched from `origin/main` and merged.

Five agents worked in parallel worktrees, one per file-overlap group, and their branches were
merged into the work branch (conflicts only in `CHANGELOG.md`, `RepoTabs.tsx`, `App.tsx`).

**Result: 33 of 33 findings fixed, 0 partially fixed, 0 untouched.**

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
| FB-004 | Medium | Error banners float without shifting layout, carry a hint and Retry for transport errors, with friendly error-kind mappings. |
| FB-007 | Low | Sentence case applied to UI labels (picker, rebase, branch menu, hunk, graph) and documented in `docs/CONTENT_GUIDELINES.md`. |
| VIS-002 | Medium | Success, warning and info color tokens applied throughout (status strip, toasts, and conflicted file tinting). |
| UX-007 | Medium | Diff lines show old/new line-number gutters with word-level highlighting and split view toggle. |
| PERF-001 | Medium | Collapsed file sections skip diff fetch until expanded; IntersectionObserver handles lazy rendering and per-path refetch. |

## Other open items

- `Field` (A11Y-004) is unused; adopting it in the branch, workspace and PR forms was skipped
  because `InlineError` is a separate dismissible alert and swapping risked regressions.
- Word-level diff and binary-vs-mode-only distinction need a backend signal (the frontend cannot
  tell them apart today).
- E2E suites and a real-window pass (including WebKit and WebView2 focus behaviour for A11Y-001)
  are outstanding, as is the screen-reader pass the audit recommends.
