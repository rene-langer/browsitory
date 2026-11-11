---
name: browsitory-conventions
description: Use when adding or modifying code in this repo's crates, frontend or extension — covers Browsitory-specific test, transport-isolation, and task-file rules not covered by generic skills.
---

# Browsitory Conventions

## Overview

Project-local rules for Browsitory that generic skills (TDD, writing-plans, etc.) don't
cover. Source of truth is `AGENTS.md` and `docs/ARCHITECTURE.md` — this skill is a short
pointer into them, not a replacement. If this skill and those docs ever disagree, the docs
win; fix this skill to match.

## Rules

1. **Real repos in tests, never mocks.** `git-core`/`repo-service` tests always exercise a real
   temp-dir repo (`tempfile` + `git2::Repository::init`), never a mocked `git2::Repository`.
   `frontend` tests always mock the `RepoClient` interface, never `@tauri-apps/api` or
   `postMessage` directly. See AGENTS.md's "Testing conventions".

2. **`RepoClient` is the only transport seam.** `frontend/src/components` and
   `frontend/src/state` never import `@tauri-apps/api` or any transport directly — only files
   under `frontend/src/ipc/` do. Enforced mechanically by the `no-restricted-imports`
   override in `frontend/eslint.config.js` (`pnpm lint`). See `docs/ARCHITECTURE.md`'s "The
   `RepoClient` IPC boundary".

3. **Triage the mutants comment.** A PR changing `git-core`/`repo-service` source gets a
   "Mutation testing" comment from CI's `mutants` job. Before reporting done, kill each real
   missed mutant with a test or explain in the PR why it is equivalent or harmless; ignore
   mutants in `#[cfg(windows)]` files. See AGENTS.md's "Testing conventions".

4. **Specs and plans are dated records.** Larger work gets a design spec in
   `docs/superpowers/specs/` and a plan in `docs/superpowers/plans/`, named
   `YYYY-MM-DD-<slug>[-design].md`. Older specs and plans describe the code as it was; never
   treat one as current without checking the code. (`docs/tasks/` and `docs/TASK_TEMPLATE.md`
   are the retired Phase 1/6 process.)

5. **Tests first, never weakened.** Tests go in their own commit before the code, with the
   red run's real output quoted. Never change an expected value, drop or loosen an assertion,
   or skip a test to get green; when the code behaves differently than a planned test expects,
   stop and report to the orchestrator (or the owner), who decides. A deliberate test change is
   its own `Test change:` commit, explained in the PR. CI's `removed-assertions` comment lists
   removed or rewritten assertions; answer it in the PR. See AGENTS.md's "Task workflow".

## When in doubt

Read `AGENTS.md` and `docs/ARCHITECTURE.md` directly — they're authoritative. This skill only
exists to flag, at a glance, when it's worth going to look.
