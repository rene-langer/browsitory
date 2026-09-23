# Audit resolution progress

Tracks remediation of `docs/audits/2026-09-20/`'s UI/UX findings. Status as of 2026-09-23.

Work branch: `fix/ux-audit-2026-09-20`, branched from `origin/main` and merged.

Five agents worked in parallel worktrees, one per file-overlap group, and their branches were
merged into the work branch (conflicts only in `CHANGELOG.md`, `RepoTabs.tsx`, `App.tsx`).

**Result: 31 of 33 findings fixed, 2 partially fixed (FB-004, FB-007), 0 untouched.**

## Verification

Run on the branch after the final review's fix wave (2026-09-23), in `frontend/`: `pnpm lint`,
`pnpm exec tsc -p tsconfig.app.json --noEmit` (plain `tsc --noEmit` checks nothing here — the root
`tsconfig.json` only holds project references), `pnpm test -- --run` (66 files, 827 tests); all pass. The
fix wave touched no Rust; `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D
warnings` and `cargo test --workspace` last passed on the pre-fix-wave branch. **Not run:** the Tauri and VSCode e2e suites, and no manual
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
| A11Y-003 | Medium | Per-tab close is `aria-hidden`/mouse-only (still inside the tablist wrapper). Keyboard: Delete closes the focused tab (APG tabs pattern), the palette has "Close tab", and Ctrl/Cmd+W closes the active tab where the host doesn't claim it first (Tauri's default macOS menu binds Cmd+W to Close Window; a VSCode webview may forward Ctrl+W to the workbench — not verified live). |
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
| FB-004 | Medium | **Partial.** Error banners float without shifting layout. Diff-pane fetch failures (diff, blame, commit files) keep the raw text, add a `describeError` hint for known error kinds and offer Retry; a failed repo open offers Retry; the transport-lost banner has a fixed hint but no Retry. Mutation failures (`runMutation` → `state.error` → `App`'s top-level banner) have no hint and no Retry, and there is no "Copy details"/"Details" disclosure. |
| FB-007 | Low | **Partial.** Sentence case applied to picker, workspace, rebase, branch "+" menu, hunk, conflict-resolution and blame ("Back to diff") labels and documented in `docs/CONTENT_GUIDELINES.md`, which lists the Title Case labels still left ("Pull Requests", "Uncommitted Changes", "Release Notes", workspace editor). |
| VIS-002 | Medium | Success, warning and info color tokens applied throughout (status strip, toasts, and conflicted file tinting). |
| UX-007 | Medium | Diff lines show old/new line-number gutters with client-side word-level highlighting (token LCS in `lib/wordDiff.ts`, capped at 40,000 table cells per line pair and memoized per loaded diff) and a split view toggle. |
| PERF-001 | Medium | Collapsed and off-screen file sections skip their diff fetch (IntersectionObserver); only the active tab's workspace is mounted. Open diffs still refetch on every `refresh()` (`state.refreshGeneration`) and on both sides of a hunk-mutated path — correctness over the narrower per-path-only refetch an earlier pass shipped, which left diffs stale. Commit drafts and tab busy state survive the unmount. |

## Other open items

- `Field` (A11Y-004) is unused; adopting it in the branch, workspace and PR forms was skipped
  because `InlineError` is a separate dismissible alert and swapping risked regressions.
- The binary-vs-mode-only distinction needs a backend signal (the frontend cannot tell them apart
  today). Word-level diff no longer does: it shipped client-side (UX-007).
- FB-004's remaining half (hint and Retry for mutation errors) and FB-007's leftover labels, as
  described in their rows above.
- E2E suites and a real-window pass (including WebKit and WebView2 focus behaviour for A11Y-001)
  are outstanding, as is the screen-reader pass the audit recommends.
