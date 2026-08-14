---
name: browsitory-conventions
description: Use when adding or modifying code in this repo's git-core, config, tauri-app, or frontend crates/packages — covers Browsitory-specific test, transport-isolation, and task-file rules not covered by generic skills.
---

# Browsitory Conventions

## Overview

Project-local rules for Browsitory that generic skills (TDD, writing-plans, etc.) don't
cover. Source of truth is `AGENTS.md` and `docs/ARCHITECTURE.md` — this skill is a short
pointer into them, not a replacement. If this skill and those docs ever disagree, the docs
win; fix this skill to match.

## Rules

1. **Real repos in tests, never mocks.** `git-core`/`tauri-app` tests always exercise a real
   temp-dir repo (`tempfile` + `git2::Repository::init`), never a mocked `git2::Repository`.
   `frontend` tests always mock the `RepoClient` interface, never `@tauri-apps/api` or
   `postMessage` directly. See AGENTS.md's "Testing conventions".

2. **`RepoClient` is the only transport seam.** `frontend/src/components` and
   `frontend/src/state` never import `@tauri-apps/api` or any transport directly — only files
   under `frontend/src/ipc/` do. Enforced mechanically by the `no-restricted-imports`
   override in `frontend/eslint.config.js` (`pnpm lint`). See AGENTS.md's "`RepoClient`: why
   it exists" and `docs/ARCHITECTURE.md`'s "The `RepoClient` IPC boundary".

3. **Task files follow the template.** New implementation tasks (Phase 1 onward) follow
   `docs/TASK_TEMPLATE.md`'s shape and are named
   `docs/tasks/phase-<N>/<workstream>-<id>-<slug>.md`.

## When in doubt

Read `AGENTS.md` and `docs/ARCHITECTURE.md` directly — they're authoritative. This skill only
exists to flag, at a glance, when it's worth going to look.
