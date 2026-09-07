# Deep-dive 01: Architecture and Boundaries

## Purpose, scope, audit questions

Does the codebase actually enforce the architectural boundaries `docs/ARCHITECTURE.md` and
`CLAUDE.md` claim — the `RepoClient` transport-isolation seam, a mechanically-enforced ban on UI
code depending on a concrete transport, and (per the VSCode extension design spec) a clean
extraction of transport-agnostic dispatch logic (`crates/repo-service`) shared between the
desktop (`tauri-app`) and VSCode (`vscode-sidecar`) transports? Is the Tauri command surface a
faithful, consistent mapping onto the worker's `Command` enum? As with deep-dives 06/08/09, this
required reading `origin/main` read-only via `git show`/`git ls-tree` for anything touching the
VSCode extension, since that subsystem is entirely absent from the audited working tree/history
(see `09`'s DOC-001 and `08`'s CI-002 for the full branch-divergence evidence — not repeated
here).

## Architecture examined

- `frontend/src/ipc/RepoClient.ts` (the interface contract) and `tauriRepoClient.ts` (its Tauri
  implementation), both read locally.
- `frontend/eslint.config.js`'s `no-restricted-imports` transport-isolation rule, read locally
  and diffed against `origin/main`'s copy via `git show origin/main:frontend/eslint.config.js`.
- `crates/tauri-app/src/commands/mod.rs`'s `worker_handle` helper and its callers
  (`commands/status.rs` and siblings) against `crates/tauri-app/src/worker/mod.rs`'s `Command`
  enum and dispatch loop, both read locally.
- `origin/main`'s crate layout (`git ls-tree -r origin/main -- crates/`) and
  `docs/ARCHITECTURE.md` (`git show origin/main:docs/ARCHITECTURE.md`), to check whether the
  `crates/repo-service` extraction the design spec calls for (sub-phase (a): "Extract the
  transport-agnostic parts into a new crate... a refactor of existing tested code, not new
  logic") actually happened, and whether it's now documented.
- `docs/superpowers/specs/2026-08-30-vscode-extension-design.md`'s "Sharing the dispatch logic"
  section, which is the source of the ESLint-extension claim this deep-dive's main finding tests.

## Evidence and checks run

- `git ls-tree -r origin/main --name-only -- crates/ | cut -d/ -f1-2 | sort -u` →
  `crates/config`, `crates/git-core`, `crates/repo-service`, `crates/tauri-app`,
  `crates/vscode-sidecar`. Confirmed `crates/repo-service/src/{lib.rs,credentials.rs,
  pull_requests.rs,worker/*.rs}` exists with real content (not a stub), and that
  `crates/tauri-app/src/` on `origin/main` **no longer contains** `credentials.rs`,
  `pull_requests.rs`, or a `worker/` directory (all present locally at the audited commit,
  confirmed via local `ls crates/tauri-app/src/`) — a structural, verifiable confirmation that
  the extraction actually moved code rather than merely duplicating it.
- `git show origin/main:docs/ARCHITECTURE.md | head -20` → the crate diagram is updated to list
  all five crates plus `extension/`, and describes `vscode-sidecar` as wiring "every `RepoClient`
  method except five VSCode-native ones." This resolves `09`'s DOC-002 finding once the branch is
  synced — recorded here as confirmation, not re-litigated.
- `git show origin/main:frontend/eslint.config.js` (full `no-restricted-imports` block, lines
  ~24-41) → **unchanged** from the local copy: the rule's `patterns` array has exactly one entry,
  `{ group: ['@tauri-apps/*'], message: '...' }`, scoped to `src/components/**` and
  `src/state/**`. No second pattern for a `vscode` import group exists on `origin/main`.
- `grep -c` sanity check: `frontend/src/ipc/RepoClient.ts` declares 85 methods, matching the
  design spec's "~85-method surface" claim almost exactly.
- Local trace of `worker_handle` (`commands/mod.rs:934-943`) and a representative command
  (`commands/status.rs:5-17`, `get_status`) confirms the mapping from a Tauri command to a
  `Command` enum variant to a `WorkerHandle` method is 1:1 and consistent, with no
  command bypassing the worker abstraction for a worker-owned operation.

## Findings

### AUD-2026-09-05-ARCH-001 — The transport-isolation ESLint rule was not extended to the VSCode transport, despite the design spec explicitly committing to it

- **Severity:** Medium
- **Confidence:** Confirmed
- **Affected components:** `frontend/eslint.config.js` (on `origin/main`), the VSCode transport
  isolation boundary (`frontend/src/ipc/vscodeRepoClient.ts`, `extension/`).
- **Evidence:** `docs/superpowers/specs/2026-08-30-vscode-extension-design.md`'s "Sharing the
  dispatch logic (`repo-service`)" section states, in reference to `vscodeRepoClient.ts`: "No
  component in `frontend/src/components`/`frontend/src/state` changes — same rule the existing
  `no-restricted-imports` ESLint override enforces for `tauriRepoClient.ts`, **extended to also
  ban `vscode`-API imports outside `vscodeRepoClient.ts` and `extension/`**." This is an explicit,
  written commitment in the approved design document. `git show
  origin/main:frontend/eslint.config.js` shows the rule's `patterns` array still contains only
  the original `@tauri-apps/*` group; no `vscode` (or `'vscode'`, the actual import specifier a
  VSCode extension host module would use) group was added anywhere in the file on `origin/main`,
  four commits and one shipped extension after the spec was approved.
- **Impact:** The mechanical guarantee that makes the `RepoClient` boundary trustworthy for the
  Tauri transport — "a violation fails `pnpm lint` (and CI)," per `docs/ARCHITECTURE.md` — does
  not exist for the VSCode transport. A future change that accidentally imports `vscode` (or any
  Node-only API only the extension host can provide) into `frontend/src/components/**` or
  `frontend/src/state/**` would compile and pass `pnpm lint`/CI cleanly in the desktop
  build, then fail only at runtime inside the Tauri webview (which has no `vscode` global) — or
  worse, silently do nothing useful there while working by accident in the extension's webview.
  This is exactly the class of drift the existing rule was built to make impossible for the
  Tauri case; it is currently possible for the VSCode case.
- **Trigger / reproduction:** Add a stray `import * as vscode from "vscode"` to any file under
  `frontend/src/components/` on `origin/main` and run `pnpm lint` — nothing in the current rule
  set would flag it (confirmed by reading the rule; not executed live, since `origin/main` isn't
  checked out in this environment — see coverage gaps).
- **Remediation:** Add a second `no-restricted-imports` pattern entry (or extend the existing
  group) banning a `vscode` import specifier from `src/components/**` and `src/state/**`, scoped
  identically to the existing `@tauri-apps/*` rule, and confirm `vscodeRepoClient.ts` (under
  `src/ipc/`) is itself exempted from it the same way `tauriRepoClient.ts` already is.
- **Verification:** Add the stray `import * as vscode from "vscode"` reproduction above as a
  regression fixture (or a `pnpm lint`-level test) that fails without the rule change and passes
  with it.

### AUD-2026-09-05-ARCH-002 — `frontend/src/lib/logger.ts` imports a Tauri-only plugin from transport-agnostic startup code, outside the isolation rule's reach

- **Severity:** Medium
- **Confidence:** Confirmed
- **Affected components:** `frontend/src/lib/logger.ts`, `frontend/src/main.tsx`,
  `frontend/eslint.config.js`, `docs/ARCHITECTURE.md`
- **Evidence:** `docs/ARCHITECTURE.md` states `tauriRepoClient.ts` is "the *only* file
  allowed to import `@tauri-apps/api`." `frontend/src/lib/logger.ts:1` imports
  `@tauri-apps/plugin-log` directly at module scope, and `frontend/src/main.tsx:5,7`
  unconditionally wires it into global `window` `error`/`unhandledrejection` listeners at
  app startup — code that runs identically regardless of transport. `grep -rln
  "@tauri-apps" frontend/src | grep -v src/ipc/` confirms `lib/logger.ts` is the only
  offender. The ESLint `no-restricted-imports` rule's `files` glob
  (`src/components/**`, `src/state/**`) does not cover `src/lib/**`, so `pnpm lint`
  passes clean (confirmed by an actual run) despite the violation. Unchanged on
  `origin/main` (`git show origin/main:frontend/src/lib/logger.ts` is byte-identical),
  so the already-shipped VSCode extension work did not address it.
- **Impact:** In the VSCode webview (which reuses `frontend/dist` unmodified, per the
  extension design spec), `@tauri-apps/plugin-log`'s `invoke()` has no Tauri IPC bridge to
  call into. Every uncaught error and unhandled rejection in the extension UI silently
  fails to log (the call site wraps it in `void`), removing exactly the
  live-session-independent failure-diagnosis path the 0.2.0 changelog entry describes,
  with no visible symptom pointing at the cause. This is the same class of gap as
  ARCH-001 above (an isolation guarantee that holds for the Tauri transport but not the
  VSCode one) in a different file, so the two should likely be fixed together.
- **Trigger/reproduction:** Load `frontend/dist` in the VSCode webview, throw an uncaught
  error, and observe no entry lands in the shared rotated log file.
- **Remediation:** Route `logFrontendError` through `RepoClient` itself (a method each
  transport implements — Tauri via `@tauri-apps/plugin-log`, VSCode via `postMessage` to
  the extension host's own log sink) instead of importing a Tauri plugin from shared
  code; widen the ESLint rule (or invert it to allow `@tauri-apps/*` only inside
  `src/ipc/**`) so `src/lib/**` and any other future directory is covered by construction
  rather than by remembering to update a glob.
- **Verification:** After the fix, `grep -rln "@tauri-apps" frontend/src | grep -v
  src/ipc/` returns nothing, and a widened lint rule fails on a reintroduced violation.

## Coverage gaps and open questions

- **No systematic method-by-method parity check** between `RepoClient.ts`'s 85 declared methods
  and `vscodeRepoClient.ts`'s actual implementation was performed (a rough grep-based method-count
  comparison was attempted but the two files use different declaration styles that a simple
  regex doesn't reliably count across both). Deep-dive 06 flagged the same gap independently.
  **Smallest next action:** a short script diffing `RepoClient`'s interface member names against
  both `tauriRepoClient.ts`'s and `vscodeRepoClient.ts`'s implemented method names would close
  this decisively; not done here due to time budget.
- **Whether `crates/repo-service`'s extraction left `tauri-app`'s desktop test suite passing
  unchanged**, as the spec's sub-phase (a) requires ("the desktop app must keep passing its
  existing test suite unchanged after the extraction"), was not independently verified — this
  would require checking out `origin/main` and running `cargo test --workspace` there, which was
  out of scope for this read-only, working-tree-preserving pass. Deep-dive 06 flagged this gap
  independently; recommend a follow-up run once the branch is synced (see `09`'s DOC-001
  remediation).
- **DTO/wire-format consistency** (the `StatusKind` pinning test) is covered in depth by
  deep-dive 07 and not re-audited here to avoid duplication.
- `cargo clippy --workspace --all-targets -- -D warnings` initially stalled on a
  `target/`-directory build lock (contention from other audit deep-dives building/testing
  concurrently in the same checkout) but subsequently completed with **exit code 0** —
  a clean pass, no warnings or errors under `-D warnings`. `pnpm lint` also passed clean.
  Both are moved to "Strengths" below.

## Strengths and positive controls

- **The `repo-service` extraction is real, not aspirational.** `crates/tauri-app/src/` on
  `origin/main` structurally lost exactly the files the spec said would move
  (`credentials.rs`, `pull_requests.rs`, `worker/`), and `crates/repo-service/src/` gained them —
  verified by directory listing, not just by reading the spec's intent. This is a materially
  stronger confirmation than trusting the design document alone, and it means both `tauri-app`
  and `vscode-sidecar` genuinely share one dispatch implementation rather than maintaining two
  independently-evolving copies of ~85 methods' worth of logic.
- **The Tauri command → `Command` enum → `WorkerHandle` method chain is consistent and
  traceable** at every hop checked: no command was found that reimplements worker logic inline or
  bypasses the channel abstraction for an operation that should go through it.
- **`cargo clippy --workspace --all-targets -- -D warnings` and `pnpm lint` both pass
  clean** at the audited commit (confirmed by direct execution, not assumed from CI
  config) — the workspace has zero outstanding lint/clippy debt as of `cd0cb63`.
- **The existing (Tauri-side) transport-isolation rule works exactly as documented** and was
  independently confirmed by reading the rule's actual `patterns` config, not just its doc
  description — it is a real, narrowly-scoped, enforced control, which makes ARCH-001's gap an
  incomplete rollout of a proven pattern rather than a sign the pattern itself doesn't work.
