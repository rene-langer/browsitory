# Phase 1: full repo view

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development`
> (recommended) or `superpowers:executing-plans` to implement this phase task-by-task. Each task
> is its own file, per `docs/TASK_TEMPLATE.md`'s convention. Dispatch/execute in the order the
> dependency graph below allows — a task's `## Depends on` section is authoritative if this
> index and a task file ever disagree.

**Design spec:** `docs/superpowers/specs/2026-08-12-browsitory-phase1-design.md`

**Goal:** commit history, a line-level diff viewer, whole-file stage/unstage, and commit
creation — wired end-to-end through a real repo picker, replacing Phase 0's always-empty status
view with a usable app.

## Workstreams

- **A — `git-core`** (sequential, all touch `crates/git-core/src/lib.rs`):
  [a-01-log](a-01-log.md), [a-02-diff](a-02-diff.md), [a-03-stage-and-commit](a-03-stage-and-commit.md)
- **B — `config`** (independent of A, can run in either order relative to it):
  [b-01-recent-repos](b-01-recent-repos.md)
- **C — `tauri-app` IPC layer** (needs A and B done first):
  [c-01-git-ipc-commands](c-01-git-ipc-commands.md), [c-02-repo-picker-commands](c-02-repo-picker-commands.md)
- **D — frontend IPC + state** (needs C done first):
  [d-01-repo-client](d-01-repo-client.md), [d-02-app-state](d-02-app-state.md)
- **E — frontend components** (needs D done first; the four component tasks are mutually
  independent of each other except where noted):
  [e-01-repo-picker-component](e-01-repo-picker-component.md),
  [e-02-diff-view](e-02-diff-view.md),
  [e-03-history-list](e-03-history-list.md),
  [e-04-diff-pane](e-04-diff-pane.md) (depends on e-02, not on e-01/e-03)
- **F — integration** (sequential, needs all of E done first):
  [f-01-wire-app](f-01-wire-app.md), [f-02-e2e-first-flow](f-02-e2e-first-flow.md)
- **G — docs** (needs everything above done first):
  [g-01-docs-update](g-01-docs-update.md)

## Dependency graph

```
A-01 (log)  ─┐
A-02 (diff) ─┼─► C-01 (git IPC commands) ─┐
A-03 (stage+commit) ─┘                     │
                                            ├─► D-01 (RepoClient) ─► D-02 (useAppState)
B-01 (recent repos) ─► C-02 (picker cmds) ─┘                              │
                                                                           ▼
                                                          ┌────────────────────────────┐
                                                          │  E-01 RepoPicker            │
                                                          │  E-02 DiffView               │
                                                          │  E-03 HistoryList             │
                                                          │  E-04 DiffPane (needs E-02)   │
                                                          └────────────────┬────────────┘
                                                                           ▼
                                                                   F-01 (wire App.tsx)
                                                                           ▼
                                                                   F-02 (E2E first flow)
                                                                           ▼
                                                                   G-01 (docs update)
```

Within workstream A, the three tasks all add a `pub mod ...;` line to the same
`crates/git-core/src/lib.rs` — keep them sequential (same workstream) rather than dispatched in
parallel, even though their actual module content is otherwise independent, to avoid two
concurrent edits to that one shared line. Same reasoning applies to C-01/C-02 both touching
`commands.rs`/`main.rs`.

## Global constraints (carried from the Phase 1 design spec and `CLAUDE.md`)

- TDD mandatory: `git-core`/`config` tests use real temp-dir repos/files, never a mocked
  `git2::Repository`. Frontend tests mock `RepoClient`, never `@tauri-apps/api`/
  `@tauri-apps/plugin-dialog` directly.
- `frontend/src/components`/`frontend/src/state` never import a transport directly — only
  `frontend/src/ipc/*` does (mechanically enforced by `frontend/eslint.config.js`'s
  `no-restricted-imports` rule; don't weaken it to make a task easier).
- `git-core` functions take `&git2::Repository` (or a path) as an explicit argument, never a
  singleton.
- License: MIT-only dependencies, with the one documented `git2`/libgit2 exception already
  recorded in `docs/LICENSE_COMPLIANCE.md`. Every new direct dependency gets a row added in the
  same commit that adds it (each task above says so explicitly where it applies).
- Commit messages use Conventional Commits prefixes (`feat:`, `fix:`, `docs:`, `test:`, `chore:`,
  `refactor:`).
- Whole-file staging only, line-level diff only, no multi-branch graph, no amend — see the design
  spec's "Non-goals" section for the full list; don't add scope beyond what each task's own file
  specifies.
