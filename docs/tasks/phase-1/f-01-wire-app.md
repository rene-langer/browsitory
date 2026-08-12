# Task 1.F.01: Wire `RepoPicker`/`HistoryList`/`DiffPane` into `App.tsx`

## Goal

Replace Phase 0's `StatusView`-only `App.tsx` with the real Phase 1 layout: `RepoPicker` when no
repo is open, otherwise `HistoryList` + `DiffPane` side by side, both driven by one
`useAppState` instance. This is the task that makes the app actually usable end-to-end.

## Depends on

1.E.01 (`RepoPicker`), 1.E.03 (`HistoryList`), 1.E.04 (`DiffPane`) — the three components being
composed here.

## Interfaces produced

`frontend/src/App.tsx` (full new contents, replaces the Phase 0 version):
```tsx
import { DiffPane } from "./components/DiffPane";
import { HistoryList } from "./components/HistoryList";
import { RepoPicker } from "./components/RepoPicker";
import { tauriRepoClient } from "./ipc/tauriRepoClient";
import { useAppState } from "./state/useAppState";

export default function App() {
  const appState = useAppState(tauriRepoClient);

  if (appState.state.repoPath === null) {
    return (
      <main>
        <h1>Browsitory</h1>
        <RepoPicker client={tauriRepoClient} onOpenRepo={appState.openRepo} />
      </main>
    );
  }

  return (
    <main>
      <h1>Browsitory</h1>
      {appState.state.error !== null && <p role="alert">{appState.state.error}</p>}
      <div className="app-layout">
        <HistoryList
          status={appState.state.status}
          log={appState.state.log}
          selectedRow={appState.state.selectedRow}
          onSelectRow={appState.selectRow}
        />
        <DiffPane
          client={tauriRepoClient}
          selectedRow={appState.state.selectedRow}
          status={appState.state.status}
          onStageFile={appState.stageFile}
          onUnstageFile={appState.unstageFile}
          onCommit={appState.commit}
        />
      </div>
    </main>
  );
}
```
This is the app's one composition root — the only place `tauriRepoClient` (the concrete
transport) meets the presentational components (which all take `client`/data/callbacks as
props, never importing a transport themselves).

## Implementation notes

`Phase 0`'s `StatusView`/`StatusView.tsx`/`StatusView.test.tsx` are now fully superseded: the
uncommitted-file list with per-file diffs lives in `DiffPane` (Task 1.E.04), and the change-count
badge lives in `HistoryList` (Task 1.E.03). Delete both files as part of this task —
`git rm frontend/src/components/StatusView.tsx frontend/src/components/StatusView.test.tsx`
rather than leaving superseded, unused code in the tree.

`appState.state.error` (surfaced whenever any `useAppState` action rejects — a stale recent-repo
path failing to open, a stage/unstage/commit failure mid-session, etc.) renders as a
top-of-`main` alert once a repo is open; `RepoPicker` already handles its own error display for
`pickRepoFolder`/`listRecentRepos` failures internally (Task 1.E.01), so this only needs to cover
errors from actions taken *after* a repo is open.

## TDD requirement

No new test file for `App.tsx` — consistent with Phase 0, where `App.tsx` never had one. It's a
thin composition root that hardcodes the real `tauriRepoClient` transport, so a meaningful unit
test would require mocking `@tauri-apps/api` at the module level (the one thing `CLAUDE.md`'s
testing conventions say frontend tests never do — they mock `RepoClient`, not the transport
under it). `App.tsx`'s correctness is verified by: (a) `pnpm build`'s type-check, since every
prop passed to `RepoPicker`/`HistoryList`/`DiffPane` must satisfy their typed interfaces, and (b)
Task 1.F.02's E2E test, which exercises the real composed app end-to-end.

What this task must still verify manually (note in the task report): run `cargo tauri dev` from
`crates/tauri-app/`, confirm `RepoPicker` renders on launch, opening a real repo (via "Open
Folder") transitions to the `HistoryList`/`DiffPane` layout, and the previously-deleted
`StatusView`'s functionality (seeing uncommitted changes) is now covered by `DiffPane`'s
"Uncommitted Changes" branch.

## Acceptance criteria

- [ ] `pnpm build` succeeds (TypeScript compiles clean with the new `App.tsx`).
- [ ] `pnpm lint` clean.
- [ ] `pnpm test -- --run` passes — `StatusView.test.tsx` is gone (deleted), all other existing
      and Task 1.E.01/1.E.02/1.E.03/1.E.04 tests still pass.
- [ ] Manual `cargo tauri dev` check above performed and noted in the task report.
- [ ] Commit: `git add frontend/src/App.tsx && git rm frontend/src/components/StatusView.tsx frontend/src/components/StatusView.test.tsx && git commit -m "feat(frontend): wire RepoPicker/HistoryList/DiffPane into App, retire StatusView"`.

## Out of scope

Any new functionality beyond composing the already-built components — this task is pure wiring.
Responsive/mobile layout, window-resize handling, theming (`app-layout` is referenced as a CSS
class here but this task doesn't add styling — Phase 1's design spec is functionality-first;
visual polish is an explicit later pass).
