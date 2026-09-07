# Browsitory Audit — Executive Summary

**Audit date:** 2026-09-05
**Auditor:** Claude (Anthropic), acting as lead auditor in an interactive session with the
repository owner
**Deliverables:** this summary plus nine deep-dives under `deep-dives/`

## Audit identity and limitations

- **Audited commit:** `cd0cb63` (local branch `main`, "docs(specs): add VSCode extension design
  spec (Phase 6)"), working tree otherwise clean except for a set of untracked build artifacts
  described below.
- **Critical scope caveat, stated up front because it shapes every deep-dive:** the audited
  commit is **four commits behind `origin/main`**. Those four commits
  (`9bfd49e` "Ship Browsitory as a VSCode extension (Phase 6) (#66)", `bb8a9eb`, `3318c44`,
  `3ebc59c`) contain a fully implemented, tested, and partially CI-wired VSCode extension
  (`extension/`, `crates/vscode-sidecar`, `crates/repo-service`) that **does not exist anywhere
  in the audited commit's history**. The audited commit's own last local commit — the one this
  audit was commissioned against — is titled as adding a design spec "not yet
  planned/implemented," which was already false when it was written: `origin/main` had shipped
  the feature two days earlier. See `deep-dives/09-documentation-and-operability.md`
  (AUD-2026-09-05-DOC-001) for the full evidence trail.
- **Consequence for method:** everything about the desktop app (Phases 0-5: `git-core`, `config`,
  `tauri-app`, `frontend`) was audited directly against the checked-out working tree, including
  running the full local verification suite. Everything about the VSCode extension (Phase 6) was
  audited **read-only against `origin/main`** via `git show`/`git ls-tree`/`git diff` — the
  working tree and `HEAD` were never checked out, merged, or modified. Findings about the
  extension are clearly labeled as sourced from `origin/main`, not from the literal audit commit.
- **Working tree also contains stray, untracked, ungitignored build output** —
  `extension/dist/*.js`, `extension/artifacts/browsitory-0.1.0-linux-x64.vsix` (6.9MB),
  `extension/node_modules/`, `extension/e2e/` (including a 1.3GB VS Code/Electron test-runner
  cache), and `frontend/dist-vscode/` — with no corresponding source anywhere in the audited
  commit's reachable history. This is almost certainly leftover from a build against an
  `origin/main`-equivalent checkout, left behind when the working tree was switched back to this
  stale local branch. See CI-001/VSCE-002.
- **Environment:** Linux (`ubuntu`), sandboxed, no live network egress assumed available for
  `cargo audit`/`pnpm audit --prod` (not run — see Coverage gaps). No GUI/display server; desktop
  E2E (`e2e/`) and VS Code E2E (`extension/e2e/`) were not executed. No disposable git
  hosting/token/SSH key was provisioned, so the manual "Credential release acceptance" checklist
  in `docs/ARCHITECTURE.md` was verified by code trace, not by a live run.
- **Commands actually run and their results**, against the audited commit's working tree:
  - `cargo build --workspace` — clean, exit 0.
  - `cargo test --workspace` — **clean, exit 0. 425 tests passed, 0 failed**
    (`config` 32, `git-core` 173, `tauri-app` 90 default-feature + 90 with
    `--features forge-fixture-override`, plus 0 doc-tests). Full per-file breakdown in
    `deep-dives/02-git-domain-correctness-and-data-safety.md`.
  - `cargo clippy --workspace --all-targets -- -D warnings` — clean, exit 0, zero warnings.
  - `cargo fmt --all -- --check` — clean, exit 0.
  - `pnpm lint` (frontend) — clean, exit 0.
  - `pnpm test -- --run` (frontend) — **clean, exit 0. 576 tests passed across 47 test files.**
  - `python3 scripts/check-license-compliance.py` — clean, exit 0 (scope limited to the audited
    commit's manifests; does not cover `extension/`, which doesn't exist there — see CI-003).
  - Not run: `cargo audit`, `pnpm audit --prod` (network-dependent, not attempted), `e2e/` and
    `extension/e2e/` (require a display server and, for the latter, source absent from this tree).
- This session ran many of the above concurrently across parallel review threads, which produced
  transient build-lock contention; every command above reflects its **final, completed** result,
  not an interrupted attempt. Where an individual deep-dive's evidence section still describes an
  earlier stalled attempt, it also records the later successful re-run.

## Scope and method

Nine focus areas were audited, matching the commissioning brief's suggested minimum set almost
exactly — the repository's actual shape (a 3-crate desktop app locally, a 5-crate + extension
architecture on `origin/main`) did not require adding or merging areas beyond what was proposed,
though area 6 required the read-only `origin/main` method described above and area 9 absorbed
what would otherwise have been a cross-cutting "branch hygiene" concern.

| # | Deep-dive | Included because |
|---|-----------|-------------------|
| 1 | Architecture and boundaries | `RepoClient` IPC seam, ESLint-enforced transport isolation, and (via `origin/main`) the `repo-service` extraction are all real, checkable boundaries |
| 2 | Git-domain correctness and data safety | `git-core` is the sole libgit2 owner; a full local test/clippy/fmt pass was possible and run |
| 3 | Concurrency, lifecycle, and recovery | Worker-thread-per-repo model with documented async-command discipline; a concrete panic-recovery question was directly testable by code trace |
| 4 | Remote, forge, and credential security | A documented, testable manual acceptance checklist exists (`docs/ARCHITECTURE.md`) and a real keychain-backed credential store was available to trace |
| 5 | Frontend quality and accessibility | Explicit product goal (Sublime Merge-level keyboard-driven UX, per `CLAUDE.local.md`) makes accessibility a real requirement, not aspirational polish |
| 6 | VS Code extension and transport bridge | Materially present on `origin/main` and in stray local artifacts, even though absent from the audited commit's history — omitting it would have ignored the single largest recent change to the project |
| 7 | Verification strategy and test reliability | Distinguishes "tests exist" from "tests are trustworthy"; project has documented flake history worth assessing for root-cause vs. timeout-papering |
| 8 | Build, CI, release, and supply chain | Release pipeline, license policy, and (critically) whether the audited commit's CI surface reflects reality all needed direct evidence |
| 9 | Documentation and operability | The branch-divergence finding is fundamentally a documentation/status-tracking failure and belongs in its own accountable place |

No candidate area was dropped. No area outside the proposed list was added — the repository's
risks mapped cleanly onto the nine areas once the `origin/main` divergence was understood.

## Severity table

| ID | Title | Severity | Confidence | Deep-dive |
|----|-------|----------|------------|-----------|
| GIT-001 | `resolve_conflict` writes unvalidated paths/content into the working tree, including into `.git/` | **High** | Confirmed (static; not live-exploited) | [02](deep-dives/02-git-domain-correctness-and-data-safety.md) |
| VSCE-003 | VSCode sidecar's JSON-RPC loop is single-threaded/synchronous, serializing all repos' requests behind one slow operation | **High** | Confirmed (static trace of `origin/main`) | [06](deep-dives/06-vscode-extension-and-transport-bridge.md) |
| CI-001 | Untracked, unverifiable `.vsix` binary and build output sit in the working tree with no `.gitignore` coverage | **High** | High | [08](deep-dives/08-build-ci-release-and-supply-chain.md) |
| CI-002 | Audited commit is 4 commits behind `origin/main`, missing the entire VS Code extension | **High** | High | [08](deep-dives/08-build-ci-release-and-supply-chain.md) |
| DOC-001 | Project-status docs (`CLAUDE.md`, the Phase 6 spec) falsely claim the extension is unimplemented | **High** | Confirmed | [09](deep-dives/09-documentation-and-operability.md) |
| ARCH-001 | Transport-isolation ESLint rule not extended to the VSCode transport, despite the spec committing to it | Medium | Confirmed | [01](deep-dives/01-architecture-and-boundaries.md) |
| ARCH-002 | `frontend/src/lib/logger.ts` imports a Tauri-only plugin outside the isolation rule's glob | Medium | Confirmed | [01](deep-dives/01-architecture-and-boundaries.md) |
| CONC-001 | No automatic recovery after a worker-thread panic; repo stuck until manual reopen | Medium | High (static; not live-injected) | [03](deep-dives/03-concurrency-lifecycle-and-recovery.md) |
| FE-001 | Context-menu-only mutating actions unreachable by keyboard for remote-tracking branches; menu lacks ARIA menu keyboard pattern | Medium | Confirmed (static) | [05](deep-dives/05-frontend-quality-and-accessibility.md) |
| VSCE-001 | Local `main` 4 commits behind `origin/main`; extension missing from audit-commit history | Medium | Confirmed | [06](deep-dives/06-vscode-extension-and-transport-bridge.md) |
| TEST-001 | E2E coverage is single-scenario for most complex, stateful workflows (rebase, merge) | Medium | Confirmed | [07](deep-dives/07-verification-strategy-and-test-reliability.md) |
| CI-003 | License-compliance gate doesn't yet cover the extension's dependency manifest (unconfirmed on `origin/main`) | Medium | Medium | [08](deep-dives/08-build-ci-release-and-supply-chain.md) |
| DOC-002 | `docs/ARCHITECTURE.md`'s 3-crate diagram is already incomplete relative to `origin/main` | Medium | Confirmed | [09](deep-dives/09-documentation-and-operability.md) |
| CONC-002 | Worker-death errors not distinguished from ordinary errors in the frontend | Low | High | [03](deep-dives/03-concurrency-lifecycle-and-recovery.md) |
| CONC-003 | No test exercises worker-thread panic/crash recovery | Low | High | [03](deep-dives/03-concurrency-lifecycle-and-recovery.md) |
| FE-002 | Commit-message textarea has no accessible name once placeholder isn't visible as text | Low | Confirmed | [05](deep-dives/05-frontend-quality-and-accessibility.md) |
| VSCE-002 | Extension build artifacts (incl. a `.vsix`) untracked, no `.gitignore` coverage, 1.3GB test cache alongside | Low | Confirmed | [06](deep-dives/06-vscode-extension-and-transport-bridge.md) |
| CI-004 | No CI check that the `pnpm@9.15.9` pin is actually honored at install time; `origin/main`'s new jobs pin a different pnpm major | Low | Medium | [08](deep-dives/08-build-ci-release-and-supply-chain.md) |
| DOC-003 | Failure-log file location (0.2.0 feature) undocumented for end users/support | Low | Confirmed | [09](deep-dives/09-documentation-and-operability.md) |
| CONC-004 | `docs/ARCHITECTURE.md`/`CLAUDE.md` describe `worker.rs`/`commands.rs` as files; both are now directories | Informational | High | [03](deep-dives/03-concurrency-lifecycle-and-recovery.md) |
| VSCE-004 | `sidecarBridge.ts` forwards any well-formed JSON-RPC message without confirming a matching request | Informational | Confirmed | [06](deep-dives/06-vscode-extension-and-transport-bridge.md) |
| TEST-002 | VS Code extension has zero E2E specs in the audited tree (cross-ref VSCE-001's root cause) | Informational | Confirmed | [07](deep-dives/07-verification-strategy-and-test-reliability.md) |
| TEST-003 | Flaky-test fix history is genuinely root-caused, not timeout-papered (strength) | Informational | Confirmed | [07](deep-dives/07-verification-strategy-and-test-reliability.md) |

**Critical findings: none.** No confirmed likely-data-loss, credential-compromise,
arbitrary-code-execution, or unusable-core-workflow defect was found.

**Read the High-severity row together, not five-deep:** DOC-001, CI-002, and VSCE-001 (Medium)
are three angles on **one root cause** — the local branch being stale relative to `origin/main`
— not three independent defects; fixing the branch sync (see Immediate, below) resolves all
three at once. CI-001 is a *consequence* of that same staleness (the stray artifacts have no
source to explain them locally) and is fixed by the same remediation plus a `.gitignore` update.
GIT-001 and VSCE-003 are the two genuinely independent, code-level High-severity findings and
deserve first attention on their own merits.

## Readiness assessment

**Desktop app (Phases 0-5, audited directly): materially ready, with one High-severity code fix
outstanding.** The full local verification suite is clean (425 Rust tests, 576 frontend tests,
zero clippy warnings, clean `fmt`, clean `pnpm lint`) — this is a healthy, currently-green
codebase, not one coasting on stale CI. Credential handling is a genuine strength: every
guarantee in the documented manual acceptance checklist was independently confirmed by code
trace and is backed by adversarial unit tests (deep-dive 04 found zero confirmed defects). The
worker-thread concurrency model is correctly implemented against its own documented design, with
one real gap (no automatic recovery from a worker panic — bounded, recoverable by the user,
Medium severity). The one High-severity desktop-app finding, GIT-001, is a small, local,
low-risk fix (add the same conflict-existence check its sibling function already has) — **this
should be fixed before the next release**, but it does not indicate a systemic quality problem;
every other conflict-resolution path already has the guard this one is missing.
**Confidence: High** — based on running the actual local verification suite, not just reading
documentation.

**VS Code extension (Phase 6, audited via `origin/main` only): functionally substantial and
well-engineered in most respects, but not verifiable from this repository's checked-out state,
and carries one confirmed architectural regression.** The extension's webview CSP, sidecar crash
recovery, and code-sharing (`repo-service`) are all genuinely well-built — this is not a rushed
bolt-on. But VSCE-003 (synchronous, single-threaded request dispatch) reintroduces exactly the
UI-freeze failure mode the desktop app's own architecture was built to prevent, and it currently
ships (on `origin/main`) without a test that would catch a regression there. Because this
deep-dive worked entirely from `git show` rather than a live build, **confidence is Medium, not
High** — the code was read carefully but never compiled or exercised in this session.
**Recommend fixing VSCE-003 and closing GIT-001 before treating either subsystem as fully
release-ready**, and, independent of any code finding, **resolving the branch divergence before
any further audit, planning, or release work references "current state" at all** — right now two
different, materially different versions of "the project" exist depending on which branch is
consulted, and that ambiguity is itself the top risk on this list.

## Remediation roadmap

**Immediate (before the next release or any further planning against this branch):**
1. Fast-forward/rebase local `main` onto `origin/main` (DOC-001, CI-002, VSCE-001's shared root
   cause). There is exactly one local-only commit (`cd0cb63`); it needs rewording once synced,
   since its content is no longer accurate.
2. Fix `resolve_conflict` (GIT-001): add the same `find_conflict` existence check
   `resolve_add_delete_conflict` already has. Small, local, no behavior change for legitimate
   callers.
3. Delete the stray untracked `extension/`/`frontend/dist-vscode/` artifacts and add the missing
   `.gitignore` entries (CI-001/VSCE-002), so this class of accidental-binary-commit risk can't
   recur.

**Near-term:**
4. Fix VSCE-003 (spawn-per-request or an async runtime in the sidecar's stdin loop) before the
   extension sees broader rollout — this is a real, reproducible multi-repo UX regression, not a
   theoretical one.
5. Extend the transport-isolation ESLint rule to the VSCode transport (ARCH-001) and to
   `src/lib/**` (ARCH-002), closing both isolation gaps the same way.
6. Add worker-panic recovery (CONC-001/002/003): evict/respawn a dead worker on reopen, surface a
   distinct "connection lost, reopen" UI state, and add the missing crash-recovery test.
7. Fix keyboard reachability for context-menu-only branch/remote actions (FE-001) — this is a
   product-goal gap (Sublime Merge-level keyboard workflow), not generic a11y polish.
8. Once synced to `origin/main`, confirm `scripts/check-license-compliance.py` actually covers
   `extension/package.json` (CI-003) and reconcile the pnpm major-version split between the
   desktop and extension CI jobs (CI-004).
9. Broaden E2E coverage for rebase-abort/conflict-resume and non-trivial merge topologies
   (TEST-001) — the highest-risk, hardest-to-recover-by-hand stateful flows currently have only
   one golden-path scenario each.

**Strategic:**
10. Update `docs/ARCHITECTURE.md`'s crate diagram and file-path references (DOC-002, CONC-004)
    and document the failure-log location for end users (DOC-003) — all straightforward doc
    fixes once the branch sync (item 1) makes the target state stable.
11. Run the documented "Credential release acceptance" checklist live against disposable
    credentials at least once per release, since this audit could only confirm it by code trace.
12. Run `cargo audit`/`pnpm audit --prod` (this audit's environment had no network egress to do
    so) and treat a clean result as a release gate if not already enforced.

## Controls and strengths worth preserving

- **A genuinely green baseline, verified, not assumed:** 425 Rust tests, 576 frontend tests, zero
  clippy warnings, clean formatting, clean lint — all independently re-run in this session, not
  taken on faith from CI history.
- **Credential handling is a model for the rest of the codebase**: structural (type-level)
  separation between the test-fixture in-memory store and the production keychain store,
  adversarial test doubles that encode the exact security invariants the docs claim, and a
  minimal, auditable `.git/config` write path that never touches a secret.
- **The `repo-service` extraction (Phase 6) is real, not aspirational** — verified structurally,
  not just by reading the design spec, and it means the desktop and VSCode transports share one
  dispatch implementation rather than maintaining two copies of an ~85-method surface.
- **Flaky-test fix history shows genuine root-cause discipline** (deep-dive 07's TEST-003): every
  inspected fix corrected an actual logic/synchronization bug, including one case where the team
  explicitly measured CI contention before choosing a wait increase, rather than guessing.
- **The `no-restricted-imports` transport-isolation pattern works exactly as designed** for the
  case it currently covers (Tauri) — its gaps (ARCH-001/002) are an incomplete rollout of a
  proven, effective pattern, not evidence the pattern itself is unreliable.
- **The inline engineering-rigor bar is unusually high in places**: the `pick_repo_folder`
  async-command-deadlock comment in `commands/mod.rs` cites a specific `tauri-macros` codegen
  path, a locally-reproduced freeze measurement, and a previously-observed regression from an
  over-correction — a documentation standard worth holding other concurrency decisions to.
