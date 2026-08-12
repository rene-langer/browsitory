# Task 1.G.01: Update `CLAUDE.md`/`docs/ARCHITECTURE.md` for Phase 1 completion

## Goal

Bring the process docs current now that Phase 1 is done — same role Phase 0's Task 4 played for
Phase 0. `docs/LICENSE_COMPLIANCE.md` doesn't need a bulk update here: Tasks 1.B.01 and 1.C.02
already added their own rows as part of their own acceptance criteria, incrementally, the same
way Phase 0's tasks were expected to (`docs/LICENSE_COMPLIANCE.md`'s own "Process" section says
"add a row... in the same commit that adds the dependency" — this task only double-checks that
actually happened, it doesn't do the auditing itself).

## Depends on

All of 1.A.01 through 1.F.02 — this is the phase's closing task, describing what actually shipped
rather than what was planned (the two can diverge in small ways during implementation; document
reality).

## Interfaces produced

None consumed by other tasks — this is Phase 1's last task.

## Implementation notes

**`CLAUDE.md`'s "Project status" section** currently reads (from Phase 0):
```
Phase 0 (this pass) is setup only: workspace scaffold, CI, `git-core::repo`/`status` with
tests, and a Tauri shell proving the IPC boundary end-to-end with a minimal status view.
Phases 1-4 (see `docs/ARCHITECTURE.md`) are not started.
```
Update it to describe Phase 1 as complete and summarize what it added: commit history
(`git-core::log`), a diff viewer (`git-core::diff`, line-level), whole-file staging
(`git-core::stage`), commit creation (`git-core::commit`), a recent-repos registry
(`config`), and the unified `RepoPicker`/`HistoryList`/`DiffPane` frontend layout with basic
keyboard navigation. Note Phase 2 (branch management, stash, merge, rebase, blame, multi-branch
graph) is next and not started.

**`CLAUDE.md`'s "Architecture" section's crate summary** — `crates/config` is no longer "TOML
registry/prefs, stub so far" (that phrase appears in both `CLAUDE.md` and
`docs/ARCHITECTURE.md`'s crate-tree comment) — update both to reflect it now holds the
recent-repos registry.

**`CLAUDE.md`'s "Testing conventions" section** gains a line about the new E2E layer:
`e2e/` holds `tauri-driver` + WebdriverIO specs against the real built app, one flow per major
feature area, run separately from `cargo test`/`pnpm test` (note the actual commands from Task
1.F.02's final CI job).

**`CLAUDE.md`'s "Commands" section** gains the `e2e/` commands (`cd e2e && pnpm test`, noting it
requires `tauri-driver` running separately and a debug build already built) — copy the exact
commands Task 1.F.02's CI job ended up using, not a guess.

**`docs/ARCHITECTURE.md`**:
- The crate/package layout tree's `config/` comment line updates from "stub until Phase 1" to a
  short description of what it now holds.
- The "Roadmap" section's Phase 0/Phase 1 bullets: mark Phase 1 with what it actually shipped
  (mirroring the `CLAUDE.md` update above — keep the two consistent, don't duplicate at length,
  `CLAUDE.md` can point to this file for detail the way it already does elsewhere).
- If `RepoClient`'s interface grew in ways not anticipated by the "IPC boundary" section's code
  sample (it will have — that section still shows only `openRepo`/`getStatus`), update the code
  sample to the current shape or replace it with a pointer to `frontend/src/ipc/RepoClient.ts`
  as the source of truth rather than re-duplicating an 11-method interface inline (avoids this
  doc going stale again the next time the interface grows in Phase 2).

## TDD requirement

No tests — this is a documentation-only task. Its own verification step (below) is the closest
equivalent to a test: every command this task documents must actually be run and confirmed
working, not assumed.

## Acceptance criteria

- [ ] Run every command now listed in `CLAUDE.md`'s "Commands" section (the pre-existing ones
      plus whatever this task adds for `e2e/`) and confirm each one succeeds as documented —
      same verification discipline as Phase 0's Task 4.
- [ ] `docs/LICENSE_COMPLIANCE.md` reviewed against the final `Cargo.toml`/`package.json` state
      after all of Phase 1's tasks landed — confirm every direct dependency Phase 1 added
      (`directories`, `toml`, `tauri-plugin-dialog`, any WebdriverIO/`e2e/` packages) has a row;
      add any that were missed by an earlier task's own acceptance criteria.
- [ ] `docs/ARCHITECTURE.md`'s crate-tree comment for `config/` no longer says "stub".
- [ ] Commit: `git add CLAUDE.md docs/ARCHITECTURE.md docs/LICENSE_COMPLIANCE.md && git commit -m "docs: update CLAUDE.md and ARCHITECTURE.md to reflect Phase 1 completion"`.

## Out of scope

Rewriting `docs/superpowers/specs/2026-08-12-browsitory-phase1-design.md` itself to match final
reality — that spec is a point-in-time design record (like Phase 0's architecture spec before
it), not a living doc; `CLAUDE.md`/`docs/ARCHITECTURE.md` are the living docs this task updates.
Writing a new architecture spec for Phase 2 — that's Phase 2's own first step, via
`superpowers:brainstorming`, when that phase starts.
