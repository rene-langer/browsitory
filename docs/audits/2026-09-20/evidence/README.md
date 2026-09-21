# Evidence

Screenshots captured during the audit of 2026-09-20 from the real `App` rendered in headless Chrome
(Chrome DevTools Protocol) with a fake `RepoClient` (multi-lane graph, nested and long branch
names, two remotes, merge, rebase, empty and error scenarios). They show the harness data, not a
real repository. All are 1x scale; 1440x900 unless the name says otherwise.

| File | Viewport, state | Referenced by |
|---|---|---|
| `01-main-1440-light.png` | 1440x900, light, uncommitted changes selected | UX-001, UX-007, VIS-001, VIS-004, FB-001 |
| `02-main-1440-dark.png` | 1440x900, dark | VIS-001, VIS-002 |
| `03-main-1000-merge.png` | 1000x700, light, merge in progress | RESP-003, FB-002 |
| `04-main-700.png` | 700x900, light; diff pane is 0px wide | RESP-001, RESP-002 |
| `05-main-400.png` | 400x800, light; diff pane is 0px wide | RESP-001 |
| `06-empty-state.png` | 1440x900, no open repository | FB-006, VIS-003 |
| `07-branch-menu.png` | 1440x900, local branch "…" menu open (no Checkout item) | UX-003, FB-007 |
| `08-force-delete-confirm.png` | 1440x900, force-delete confirmation dialog | A11Y-001 (dialog whose close loses focus) |
| `09-new-branch-form.png` | 1440x900, inline New Branch form and nested sidebar scrollbar | FB-005 |
| `10-transport-error-banner.png` | 1440x900, injected transport error banner | FB-004 |
| `11-command-palette.png` | 1440x900, command palette open | strengths; UX-006, FB-007 |
| `12-merge-conflict.png` | 1440x900, merge with a conflicted file and disabled Commit | FB-002, VIS-002 |
| `13-rebase-in-progress.png` | 1440x900, rebase step 2 of 5, Continue Rebase disabled | FB-002 |

## Reproducing

The harness was an untracked `harness/main.tsx` plus `harness.html` under `frontend/` (a fake
`RepoClient` rendered through the real `App`, scenario chosen with `?s=main|empty|rebase|merge` and
`&theme=light|dark`), served with `pnpm exec vite`. It was removed after the audit and is not
part of the repository. Non-visual measurements quoted in the findings (tab order, target sizes,
focus after dialog close, pane widths) came from CDP scripts driving the same page; they are
described in deep-dive 01 (section 3) and deep-dive 03 (section 3).
