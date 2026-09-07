# Deep-dive 07: Verification Strategy and Test Reliability

## Purpose, scope, and audit questions

This deep-dive assesses whether Browsitory's test suites (Rust unit/integration
tests, frontend Vitest, desktop E2E) give reasonable confidence in
correctness, whether that confidence is evenly distributed across the
feature surface described in `docs/ARCHITECTURE.md`'s Roadmap, and whether
the project's history of test flakiness has been resolved at the root cause
or masked with longer timeouts. It does not re-adjudicate the VS Code
extension's own test posture (its source is absent from the audited working
tree; see the architecture/build deep-dives for that finding) beyond noting
its absence as a coverage gap here.

Audit questions:

1. Is coverage structurally present for every major feature area on the
   Roadmap, at both the unit and E2E level?
2. Where the project has hit flaky tests before, were the fixes root-caused
   or timeout-widened?
3. Is the DTO wire-format contract between `commands.rs` and
   `frontend/src/ipc/RepoClient.ts` actually enforced by a test, as
   `docs/ARCHITECTURE.md` claims?

## Evidence reviewed and checks run

- `crates/git-core/tests/*.rs`, `crates/config/tests/*.rs`,
  `crates/tauri-app/src/**/*.rs` (`#[test]` counts, via `grep -c`).
- `e2e/specs/*.spec.ts` (17 files) — `describe`/`it` titles.
- `extension/e2e/` (present on disk, but contains only `node_modules` and a
  `.vscode-test` Electron cache; no `*.spec.ts` files exist in the audited
  working tree under this path).
- `CHANGELOG.md`, and `git log` for commits touching flaky tests:
  `f28ed76`, `42e51bb`, `c50aaf3`, and PR #47 (`466ed81`).
- `crates/tauri-app/src/commands/mod.rs:1293-1305` (DTO contract test).
- `docs/ARCHITECTURE.md:109-112` (pnpm version-pin note).
- I did not run `cargo test --workspace` or `pnpm test` myself: both were
  already running as concurrent processes at audit time (`cargo test -p
  git-core -p config`, `vitest --run` under `frontend/`), started by another
  part of this same audit effort. Re-running them here would have raced
  the same `target/`/`node_modules` state for no additional evidence, so
  full pass/fail counts for this run are not reproduced in this file —
  cross-reference the deep-dive that captured that output (build/CI area)
  for the actual run result. This file instead relies on static structural
  evidence (test counts, spec contents, commit history), which does not
  depend on a specific run.

## Findings

### AUD-2026-09-05-TEST-001 — E2E coverage is single-scenario for most complex, stateful workflows

**Severity:** Medium
**Confidence:** Confirmed
**Affected components:** `e2e/specs/rebase.spec.ts`, `blame-viewer.spec.ts`,
`branch-management.spec.ts`, `commit-graph.spec.ts`, `first-flow.spec.ts`,
`multi-repo.spec.ts`, `reflog.spec.ts`, `remote-management.spec.ts`,
`stash-management.spec.ts`, `submodule.spec.ts`, `worktree.spec.ts`.

**Evidence:** 11 of the 17 E2E spec files contain exactly one `it()` block
each (verified by grep count), each covering one golden-path scenario. This
is most striking for interactive rebase: `crates/git-core/tests/rebase.rs`
carries 25 unit `#[test]`s (evidence of a large state space — conflict
resolution mid-rebase, edit steps, drop, abort, squash/combine), but
`e2e/specs/rebase.spec.ts` exercises exactly one flow ("Browsitory
interactive rebase", combine + drop, no abort or conflict-during-rebase
path). Similarly `merge.rs` has 20 unit tests but `merge.spec.ts` only
end-to-end-covers two scenarios (a per-hunk conflict resolve, and an
add/delete "keep theirs" conflict) — no octopus, no binary-file conflict, no
abort-merge-from-UI path.

**Impact:** Unit tests validate `git-core`'s git-level correctness in
isolation; they do not validate that the Tauri command layer, the worker
thread's state machine, and the React UI correctly surface these richer
states (e.g., an in-progress rebase's conflict markers rendering correctly,
or the UI correctly re-entering a "rebase in progress" state after an app
restart). A regression in that integration layer for an untested path (e.g.,
rebase abort via the UI, or an octopus merge) would not be caught by CI.

**Trigger/reproduction:** Not a live bug — a coverage gap. Demonstrated by
enumerating `it()` counts against the corresponding `git-core` test file's
scenario breadth.

**Remediation:** Prioritize E2E coverage for rebase abort/conflict-resume
and at least one non-trivial merge topology, since these are the workflows
most likely to leave a repository in a confusing on-disk state if the UI
layer (not `git-core`) has a bug. Full parity with unit-level scenario count
is not necessary or proportionate — E2E tests are expensive and slow by
design — but the highest-risk, most-recoverable-only-by-git-CLI states
(mid-rebase, mid-merge) deserve more than one path each.

**Verification:** After adding specs, confirm they fail against a
deliberately reintroduced bug in the corresponding worker/command code
(mutation-test style spot check), to confirm the new spec actually exercises
the layer it claims to.

---

### AUD-2026-09-05-TEST-002 — VS Code extension has zero E2E specs in the audited tree despite a documented `@vscode/test-electron` plan

**Severity:** Informational (full finding and root cause tracked in the
architecture/build deep-dive; recorded here only as a coverage-gap
cross-reference so this file's coverage inventory is complete)
**Confidence:** Confirmed
**Affected components:** `extension/e2e/`

**Evidence:** `docs/superpowers/specs/2026-08-30-vscode-extension-design.md`
commits to "New E2E layer under `extension/e2e/`, using
`@vscode/test-electron`... starting with open repo → stage a file → commit →
see it in history." The audited working tree's `extension/e2e/` directory
contains only `node_modules/` and a `.vscode-test/` Electron download cache
— no `*.spec.ts` file exists anywhere under `extension/` in this tree
(confirmed by `find ... -name "*.spec.ts"` returning empty, and by the
absence of any `extension/src` or `extension/package.json`). This is
consistent with, and likely explained by, this being a stale local
branch state (see the build/CI deep-dive's finding on `main`'s divergence
from `origin/main`) rather than the E2E layer never having been built.

**Impact/remediation:** None owed from this file — see the owning deep-dive.

---

### AUD-2026-09-05-TEST-003 — Flaky-test fix history is genuinely root-caused, not timeout-papered (strength, recorded as a finding for traceability)

**Severity:** Informational
**Confidence:** Confirmed
**Affected components:** `crates/git-core/tests/rebase.rs`,
`e2e/specs/blame-viewer.spec.ts`, `rebase.spec.ts`, `remote-transfer.spec.ts`,
`e2e/specs/workspaces.spec.ts`.

**Evidence:** Three independent flake-fix commits were inspected in full:

- `f28ed76` (`fix(git-core): stop windows rebase-abort test flake`): the
  original assertion compared post-abort tip against the pre-everything
  commit, which only matched by coincidence when two independently
  generated committer timestamps landed in the same second. The fix
  corrects the assertion's target commit, not the timeout — an actual
  logic bug in the test, root-caused and fixed.
- `c50aaf3` (`fix(e2e): stabilize final flaky specs`): replaces a plain
  `.click()` with `browser.execute((el) => el.click(), blameButton)` to work
  around WebKitGTK's WebDriver implementation reporting stale/incorrect
  click targets (documented inline), and replaces a `waitForExist` called
  after a value was already captured with a `waitUntil` around the
  capture itself — both are race-condition fixes in the test's own
  synchronization logic, not blind timeout increases.
- `42e51bb` (`debug(e2e): remove diagnostic, land the real fix`): explicitly
  documents an evidence-gathering pass (a prior diagnostic-instrumentation
  commit) that measured actual CI runner contention (test runtime swinging
  42.5s → 72.7s across runs with no code changes) before concluding the
  correct fix was a real-margin wait increase plus a retry — the commit
  message explicitly rejects "guessing a bigger timeout a third time" as
  the approach.

**Assessment:** All three instances is root-cause analysis, not symptom
suppression, even in the one case (`42e51bb`) where the eventual fix does
include a longer wait — that wait was arrived at by measurement, not guess.
This is a positive control worth preserving: future test-flake fixes should
be held to the same "diagnose with evidence, then fix" bar, and PR review
should treat a bare timeout increase with no accompanying evidence as a
regression in process, even if it makes CI green.

**Remediation:** None required. Recorded as a strength.

## DTO wire-format contract test — confirmed present and correctly scoped

`crates/tauri-app/src/commands/mod.rs:1293-1305` contains
`expected_wire_value(kind: StatusKind) -> &'static str`, an exhaustive match
over every `StatusKind` variant, asserted against the `Debug`-derived wire
string each variant serializes to. The comment at line 1293 states this is
the wire format's contract test, matched against the `StatusKind` union in
`frontend/src/ipc/RepoClient.ts`. This matches `docs/ARCHITECTURE.md`'s
claim exactly (the "Testing strategy" section says exactly this test exists
and this is its purpose) — documentation and implementation agree here, and
the match's exhaustiveness means an added `StatusKind` variant fails
compilation until the frontend-facing string is also decided, not just
silently missing from the test.

## Coverage inventory (unit test counts by crate, structural, not a run result)

- `git-core`: 137 `#[test]` functions across 18 test files (`diff` 6,
  `forge` 13, `blame` 4, `reflog_head` 3, `rebase` 25, `reflog_safety` 1,
  `remote` 27, `status` 9, `stash` 6, `branch` 11, `merge` 20,
  `reflog_missing_log` 2, `reflog` 1, `repo` 3, `stage_commit` 13,
  `worktree` 6, `submodule` 12, `graph` 7).
- `config`: 32 `#[test]` functions (`graph_branch_selection` 4, `workspaces`
  10, `scan_repos` 4, `last_seen_version` 3, `recent_repos` 11).
- `tauri-app`: 94 inline `#[test]` functions across `credentials.rs` (16),
  `pull_requests.rs` (24), `commands/mod.rs` (18), `worker/mod.rs` (35),
  `worker/remote.rs` (1).
- `e2e/`: 17 spec files, 38 total `it()` scenarios (per-file counts above);
  weighted heavily toward remote-transfer (5), pull-requests (3, though the
  file lists 3 `it()` titles for what the code comments describe as
  interdependent sequential scenarios sharing app state — see the note at
  `pull-requests.spec.ts:120`), command-palette (4), and workspaces (3).

This is a reasonably deep unit layer (`git-core`'s 137 tests against real
temp-dir repos, per `docs/ARCHITECTURE.md`'s stated no-mocks policy) with a
comparatively thin E2E layer, which is a defensible tradeoff for a project
this size *if* the highest-risk stateful flows (rebase/merge) get
proportionately more E2E attention than a single-file, low-value flow
(e.g., `blame-viewer.spec.ts`) — see TEST-001.

## Coverage gaps and open questions

- No visibility into current `cargo test --workspace` / `pnpm test` pass/fail
  counts from this file's own run — deferred to the concurrently-running
  processes (see "Evidence reviewed" above). If those runs surfaced
  failures, they are not re-litigated here.
- No data on frontend test coverage percentage (no coverage tool output was
  inspected); Vitest is configured but this audit did not confirm a coverage
  threshold is enforced in CI (`ci.yml`'s `frontend` job runs `pnpm test --
  --run` with no `--coverage` flag or coverage gate visible).
- Extension E2E: fully deferred to the architecture/build deep-dive per
  TEST-002.
- `pull-requests.spec.ts`'s three `it()` blocks are documented (inline
  comment at line 120) as sequentially dependent on shared app state rather
  than independent, self-contained scenarios. This is a coverage-fragility
  pattern (a failure in the first `it()` cascades to false failures in the
  following two) that was not present in another finding elsewhere in this
  file's scope; recorded here as an open question for the maintainers on
  whether that tradeoff (session-realism vs. isolation) is intentional.

## Accepted risks

- The documented pnpm-major-version pin (`docs/ARCHITECTURE.md:109-112`,
  `packageManager: "pnpm@9.15.9"` in both `frontend/package.json` and
  `e2e/package.json`) is a known, previously-triggered failure mode
  (broke `e2e/`'s frozen-lockfile install during Phase 1) that the project
  has chosen to mitigate via a version pin plus documentation rather than a
  CI-side enforcement check. This is a reasonable, low-cost mitigation for a
  failure mode that is easy to diagnose when it recurs (a `pnpm install`
  error), so it is recorded as an accepted risk rather than a finding.

## Strengths and positive controls

- The DTO wire-format contract test (`commands/mod.rs:1293`) is exactly what
  the architecture doc claims, closing a common "docs vs. code" drift risk
  at this specific boundary.
- `git-core`'s no-mocks-on-real-temp-dir-repos policy (137 tests) gives high
  confidence in git-level correctness independent of the UI/transport
  layers.
- The three inspected flake-fix commits (TEST-003) show a consistent,
  evidence-first debugging discipline rather than timeout-inflation as a
  reflex — worth explicitly preserving as a team norm going forward.
