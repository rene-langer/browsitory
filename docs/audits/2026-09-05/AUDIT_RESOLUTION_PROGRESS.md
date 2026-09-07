# Audit resolution progress

Tracks remediation of `docs/audits/2026-09-05/`'s findings. Status as of 2026-09-06.

Resolution commit: `03f6f20` ("fix(security,concurrency,a11y): resolve 2026-09-05 audit
findings"), on `main`, 1 commit ahead of `origin/main`, not yet pushed.

## Resolved by the branch-sync rebase (before this session's fixes)

`main` was rebased onto `origin/main` prior to this remediation pass. That alone resolved:

| ID | Title | Verified how |
|----|-------|---------------|
| DOC-001 | Project-status docs falsely claimed the extension is unimplemented | `CLAUDE.md`/spec now describe Phase 6 as shipped |
| DOC-002 | `docs/ARCHITECTURE.md`'s crate diagram incomplete | Diagram lists all 5 crates + `extension/` |
| CI-001 | Untracked `.vsix`/build output with no `.gitignore` coverage | `.gitignore` now covers `extension/`, `frontend/dist-vscode` |
| CI-002 | Local `main` 4 commits behind `origin/main` | `git log --oneline main..origin/main` empty |
| CI-003 | License-compliance gate didn't cover `extension/package.json` | `scripts/check-license-compliance.py` now walks it; `python3 scripts/check-license-compliance.py` passes |
| VSCE-001 | Local `main` missing the whole VSCode extension | Same as CI-002 |
| VSCE-002 | Extension build artifacts untracked | Same as CI-001 |

## Fixed in commit `03f6f20`

| ID | Severity | Fix |
|----|----------|-----|
| GIT-001 | High | `resolve_conflict` (`crates/git-core/src/merge.rs`) now calls `find_conflict` before writing to the working tree; regression test covers a `.git/`-relative path. |
| VSCE-003 | High | `vscode-sidecar`'s JSON-RPC loop dispatches each request on its own thread (`Arc<Mutex<HashMap<String, Worker>>>`) instead of one shared blocking loop; concurrency test proves a slow request on one repo doesn't delay a fast one on another. |
| VSCE-004 | Informational | `sidecarBridge.ts` only forwards a response if its `id` is actually pending. |
| ARCH-001 | Medium | ESLint transport-isolation rule widened to `src/**/*.{ts,tsx}` (was missing `src/lib/**`) and now also bans `vscode` imports outside `src/ipc/**`. |
| ARCH-002 | Medium | Frontend error logging now goes through a new `RepoClient.logFrontendError` method instead of importing `@tauri-apps/plugin-log` directly from shared code; VSCode transport wires it to a real output channel (previously silently dropped). |
| CONC-001 | Medium | New `ensure_worker` helper (`crates/repo-service/src/worker/mod.rs`) detects a dead worker thread and evicts+respawns it instead of leaving the repo permanently stuck. |
| CONC-002 | Low | Worker-death errors now render "connection to this repository was lost, reopen it" (`useMutationRunner.ts`/`useAppState.ts`) instead of the raw internal string. |
| CONC-003 | Low | Panic-recovery test added alongside the CONC-001 fix (test-only `Command::PanicNow` + `crash_for_test()`). |
| CONC-004 | Informational | Fixed the one remaining stale `commands.rs` reference in `docs/ARCHITECTURE.md` (now `commands/mod.rs`). |
| FE-001 | Medium | `ContextMenu` implements the WAI-ARIA menu keyboard pattern (roving tabindex, arrow/Home/End, initial focus); remote-tracking branch rows are keyboard-focusable via a new "…" button; command-palette fallback added for remote-branch checkout/set-upstream. |
| FE-002 | Low | Commit-message textarea got `aria-label="Commit message"`. |
| CI-004 | Low | All 7 `pnpm/action-setup` version pins in `ci.yml` now carry a comment naming which `package.json` they track — confirmed deliberate (extension pins pnpm 11, everything else pins 9), not drift. |
| DOC-003 | Low | Added a "Diagnosing a failure" section to `docs/USER_GUIDE.md` with per-OS log paths. |

Verified clean after all fixes: `cargo build/test/clippy/fmt --workspace`, `pnpm lint`/`pnpm test`
in `frontend/`, `extension`'s test/compile/lint, and `pnpm audit --prod` across all four JS
packages (`frontend`, `e2e`, `extension`, `extension/e2e`) — no known vulnerabilities.

## Resolved in this follow-up session (2026-09-06)

The background-task kills described below did not recur on retry — `free -h` showed swap with
headroom again, and every pipeline ran to completion:

| Item | Result |
|------|--------|
| `cargo audit` | Installed `cargo-audit v0.22.2` (`cargo install cargo-audit --locked`), ran clean: exit 0, zero `Vulnerability` entries, only 19 allowed advisory warnings (unmaintained/unsound/yanked crates — no fix action implied). |
| TEST-001 build prerequisite: Tauri E2E pipeline | `cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override` and the frontend E2E build now complete cleanly. Along the way, `pnpm build`'s `tsc -b` step caught 5 test files with `fakeClient()` mocks not updated for ARCH-002's new required `RepoClient.logFrontendError` method (`App.test.tsx`, `RebasePlanner.test.tsx`, `RepoPicker.test.tsx`, `DiffPane.test.tsx`, `ConflictResolutionPane.test.tsx`) — fixed by adding the field to each mock. Full suite then run under `xvfb-run`: **17/17 specs passing**. |
| TEST-001 build prerequisite: VSCode extension E2E pipeline | `cargo build --workspace`, the VSCode-target frontend build (`vite.vscode.config.ts`), and `extension`'s `pnpm run compile` all completed cleanly (no stale-mock issues here — extension has no unit tests using `fakeClient`). `extension/e2e`'s `pnpm install --ignore-workspace && pnpm test` run under `xvfb-run`: **1/1 passing**, exit code 0. |

## Resolved: TEST-001 (2026-09-07)

Added the two highest-risk paths the remediation named — rebase abort/conflict-resume and an
abort-merge-from-UI path — to `e2e/specs/rebase.spec.ts` and `e2e/specs/merge.spec.ts`:

- `rebase.spec.ts`: "pauses on a rebase conflict, resolves it, and continues to completion" and
  "aborts a rebase mid-conflict and restores the pre-rebase state". Both construct a genuine
  cherry-pick conflict (drop a commit whose content a later kept commit depends on, matching
  `git-core::rebase`'s own `a_conflicting_pick_pauses_and_resolving_then_continuing_lands_it`
  test) and drive it entirely through the UI.
- `merge.spec.ts`: "aborts a conflicted merge from the UI, leaving the working tree clean" — the
  "no abort-merge-from-UI path" gap the deep-dive called out (octopus merges aren't supported by
  `git-core` at all, so that half of the finding doesn't apply).

Writing the conflict-resume test surfaced a real integration bug exactly as TEST-001 predicted:
`current_upstream` (`crates/git-core/src/remote.rs`) reused `current_local_branch_name`, which
returns `RemoteError::DetachedHead` ("cannot pull while HEAD is detached") for *any* detached
HEAD — including the normal detached-HEAD state of an in-progress rebase. `useAppState.ts`'s
`refresh()` calls `getCurrentUpstream` inside the same `Promise.all` as `getStatus`/
`getRebaseProgress` on every mutation, so this error rejected the whole refresh, silently
discarding the freshly-fetched conflict state and leaving the UI stuck on stale pre-rebase
content with a confusing "cannot pull while HEAD is detached" banner. Fixed by having
`current_upstream` return `Ok(None)` on a detached HEAD instead of erroring (it's a read-only
status query with every legitimate reason to tolerate a state `pull`/`set_current_upstream`
correctly still reject) — covered by a new `git-core` test,
`current_upstream_returns_none_on_a_detached_head_instead_of_erroring`.

Verified: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo fmt --all -- --check` all clean; full `e2e` suite (`xvfb-run -a pnpm test`) **17/17 spec
files passing**.

## Not yet started

| Item | Needs |
|------|-------|
| Live "credential release acceptance" checklist (`docs/ARCHITECTURE.md`) | Disposable git-hosting credentials (HTTPS token + SSH key) the user provisions — needs the user to supply throwaway credentials, or to run the manual checklist themselves. |

## Not attempted (out of this remediation's scope)

- `docs/audits/2026-09-05/deep-dives/04-remote-forge-and-credential-security.md` reported zero
  confirmed defects — nothing to fix.
- `docs/audits/2026-09-05/deep-dives/07-verification-strategy-and-test-reliability.md`'s
  TEST-002 (VSCode extension had zero E2E specs) and TEST-003 (flaky-test fix history) were
  already resolved/were strengths, not open findings — TEST-002 in particular was resolved by
  the branch-sync rebase, since `extension/e2e/` now exists.
