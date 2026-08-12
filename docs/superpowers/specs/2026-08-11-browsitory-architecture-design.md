# Browsitory Architecture Design

Date: 2026-08-11
Status: Approved

## Context

Browsitory is a modern graphical git client. Two prior implementations exist in git
history: a browser PWA (React + TypeScript + isomorphic-git, `main` branch) and a native Rust
desktop app (`git2` + `egui`, branch `feat/rust_from_scratch`, phases 1-3 complete). Both were
deleted from the working tree to start a clean architecture pass, seeded by
`InitialSetupTask.md`.

The egui approach was fast and single-language but produces a UI that cannot be reused as a
VSCode webview — a stated future requirement ("frontend shall be either the standalone app or
a vscode integration"). This design replaces it with a backend/frontend split that satisfies
that requirement from the start, while keeping the git operations layer in Rust.

## Goals

- MVP: standalone desktop app, multi-repo, with commit/stage/diff/log, branch/stash/merge/
  rebase/blame/graph, push/pull/fetch.
- Frontend code must be reusable, without a rewrite, as a VSCode extension webview later.
- MIT-licensed project; dependencies must comply, with exceptions documented explicitly rather
  than silently accepted.
- TDD mandatory for all committed code; E2E tests where appropriate.
- Claude Code skills in place so future sessions follow the same TDD and task-execution
  conventions without re-deriving them.

## Non-goals (this pass)

- Implementing any git feature itself. This task produces architecture docs, a task template,
  and the first setup tasks only — not Phase 1 functionality.
- The actual VSCode extension. Only the frontend/backend boundary needs to make it *possible*
  later; building it is out of scope now.

## Architecture

### Stack

| Layer | Technology | License | Notes |
|---|---|---|---|
| Git operations | `git2` (libgit2 bindings) | binding MIT/Apache-2.0; libgit2 GPL-2.0-with-linking-exception | Same deliberate, documented exception as the prior Rust pass. Mature blame/rebase/merge/stash support. |
| Backend shell | Tauri | MIT/Apache-2.0 | Rust process, exposes IPC commands, packages the frontend as a standalone desktop app. |
| Config/registry | `serde` + `toml` + `directories` | MIT/Apache-2.0 | Same as before: repo registry + preferences as one TOML file in the OS config dir. |
| Frontend | React + TypeScript + Vite | MIT | Plain web app, no Tauri-specific code outside the IPC adapter (see below). |
| Frontend tests | Vitest + Testing Library | MIT | Unit/component tests. |
| E2E tests | Playwright | Apache-2.0/MIT | Drives the Tauri dev build. |

### Crate/package layout

```
browsitory/
├── crates/
│   ├── git-core/        # unchanged pattern from the prior pass: plain functions taking
│   │                     # &git2::Repository, one module per concern (status, log, diff,
│   │                     # stage, commit, branch, stash, merge, rebase, conflict, blame,
│   │                     # graph, remote, credentials, transfer), tested against real
│   │                     # temp-dir repos, never a mocked Repository.
│   ├── config/           # repo registry + preferences, TOML, unit-tested against a real
│   │                     # temp-dir file.
│   └── tauri-app/        # Tauri commands wrapping git-core, one worker thread per open
│                          # repo (same command/event-over-mpsc-channel shape as the old
│                          # app/src/worker.rs), owns the git2::Repository handle since it
│                          # isn't Send.
├── frontend/
│   ├── src/
│   │   ├── ipc/           # RepoClient interface + the Tauri `invoke`-based implementation.
│   │   │                   # This is the ONLY place that imports @tauri-apps/api. A future
│   │   │                   # VSCode webview implementation (postMessage-based) lives beside
│   │   │                   # it, behind the same interface — UI code never imports either
│   │   │                   # transport directly.
│   │   ├── components/    # panels: status, diff, log, branch, stash, blame, graph, remote
│   │   └── state/          # app state, driven by RepoClient events
│   └── tests/               # Vitest unit/component tests
├── e2e/                    # Playwright specs against the Tauri dev build
└── docs/
```

### IPC boundary: `RepoClient`

The frontend never calls `@tauri-apps/api` directly outside `frontend/src/ipc/`. All repo
operations go through a `RepoClient` interface (e.g. `openRepo`, `getStatus`, `commit`,
`push`, plus an event subscription for async progress like fetch/push). `frontend/src/ipc/
tauri.ts` implements it for the standalone app. This is what makes the VSCode path additive
later: a `frontend/src/ipc/vscode.ts` implementing the same interface over `postMessage`, no
changes to `components/` or `state/`.

### Threading model (backend)

Same rationale as the prior Rust pass: `git2::Repository` is not `Send`. Each opened repo gets
one dedicated worker thread owning its own `Repository` handle for the thread's lifetime.
Tauri commands send a `Command` over an `mpsc` channel and the worker thread emits `Event`s
back via Tauri's event system (replacing the old `egui::Context::request_repaint()` call —
Tauri's `emit` serves the same "tell the UI a background op finished" purpose). UI code (now:
frontend `state/`) never talks to `git-core` directly, only through `RepoClient`.

### Error handling

`git-core` functions return typed errors (`thiserror`-based enums per module, same as before).
`tauri-app` commands map these to a serializable error shape crossing the IPC boundary;
`RepoClient` surfaces them as rejected promises. No error is swallowed at the IPC boundary —
if a git-core call fails, the frontend must see it, not a generic "operation failed."

### Testing strategy

- `git-core`/`config`: `cargo test`, real temp-dir repos/files, no mocks — unchanged from the
  prior pass, and non-negotiable per the TDD mandate.
- `tauri-app`: unit tests around command handlers where logic exists beyond thin
  delegation; thin pass-through commands don't need their own tests (the git-core function
  they call is already tested).
- `frontend`: Vitest for components/state, mocking `RepoClient` (an interface, so this is a
  real seam, not a git2 mock).
- `e2e/`: Playwright, one flow per major feature area (open repo, stage+commit, branch switch,
  merge conflict resolution, push/pull), run against the Tauri dev build. Added where a flow
  spans backend+frontend in a way unit tests can't catch, not for every feature.

### License policy

Permissive-only (MIT, Apache-2.0, ISC, BSD, MIT-0) with the same one documented exception as
before: `git2`'s vendored libgit2 is GPL-2.0-with-linking-exception. Every new dependency gets
verified (`cargo info` / `npm info`) and recorded in `docs/LICENSE_COMPLIANCE.md` (created as
part of the first setup tasks, not this doc).

## Roadmap (unchanged from the prior pass, different implementation)

- **Phase 1**: open repo, status, log, diff, stage/unstage, commit.
- **Phase 2**: branch management, stash, merge with conflict resolution, interactive rebase,
  blame viewer, multi-branch commit graph.
- **Phase 3**: push/pull/fetch with progress, multi-remote, tag push, credential handling.
- **Phase 4** (not started): worktrees, submodules, reflog viewer, PR integration.

## Task template

Project-local task files (used by `subagent-driven-development`/`executing-plans` workstreams)
follow this shape:

```markdown
# Task <phase>.<id>: <title>

## Goal
One paragraph: what this task adds and why.

## Depends on
List of task IDs that must land first, or "none".

## TDD requirement
Which test file(s) get written first, and what they must assert before any implementation
code is written.

## Acceptance criteria
Checklist of observable outcomes (tests passing, a command working end-to-end, etc).

## Out of scope
What this task deliberately does not do (keeps tasks isolated).
```

First tasks (Phase 0 — repo setup, produced alongside this design, executed via the plan that
follows this spec):

1. Cargo + pnpm workspace scaffold (`crates/git-core`, `crates/config`, `crates/tauri-app`,
   `frontend/`), MIT `LICENSE`, CI (fmt/clippy/test for Rust, lint/test for frontend).
2. `git-core::repo` + `git-core::status`, TDD from real temp-dir repos.
3. Tauri shell + `RepoClient` interface (Tauri implementation) + minimal status view wired
   end-to-end (open repo → see status), proving the IPC boundary works before building more
   panels on top of it.

## Claude Code skills

No new TDD or task-workflow skill needed — the global `superpowers` plugin already provides
`test-driven-development`, `writing-plans`, `subagent-driven-development`, and
`executing-plans`. `CLAUDE.md` will point at these explicitly as mandatory for this repo.

One project-local skill is added (via `writing-skills`) for conventions that are specific to
Browsitory and not covered by the global skills:
- git-core tests use real temp-dir repos, never a mocked `Repository`.
- The task-file template above, and the Phase/Workstream numbering convention.
- The `RepoClient` rule: UI code never imports a transport (`@tauri-apps/api`, `postMessage`)
  directly.
