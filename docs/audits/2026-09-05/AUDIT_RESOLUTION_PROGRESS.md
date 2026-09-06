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

## Deferred — needs something this environment didn't reliably have

| Item | Needs | Status |
|------|-------|--------|
| TEST-001 (broaden E2E: rebase-abort/conflict-resume, complex merge topologies) | A display server to write new specs against safely (not blind/untested) | Not started. `Xvfb`/`xvfb-run` and `tauri-driver` are present on this machine, but the desktop/VSCode E2E build pipelines (`cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override`, the two frontend E2E builds, `extension/e2e`'s `pnpm install --ignore-workspace && pnpm test`) haven't yet completed — see "Blocked" below. |
| `cargo audit` | Installing `cargo-audit` (network + compile time) | Not completed — see "Blocked" below. `pnpm audit --prod` (the JS half of this roadmap item) is done and clean. |
| Live "credential release acceptance" checklist (`docs/ARCHITECTURE.md`) | Disposable git-hosting credentials (HTTPS token + SSH key) the user provisions | Not started — needs the user to supply throwaway credentials, or to run the manual checklist themselves. |

## Blocked, actively being investigated

Every background shell task started for the deferred items above (`cargo install cargo-audit`,
`cargo build --workspace --features ...`, the VSCode-target frontend build) has been killed
almost immediately after starting — including a solo run with no other background job competing.

- First round: plausibly explained by genuine memory pressure — `free -h` showed swap fully
  exhausted (2.0Gi/2.0Gi used) and several cgroups' `memory.events` showed `oom_kill 2`.
- Second round (solo `cargo-audit` install, no parallel jobs): the `oom_kill` counters were
  **unchanged** from before the attempt, and the job died before any `Compiling` output
  appeared — much faster than the kernel OOM killer usually acts, and faster than the earlier
  attempt that ran for 240s before being manually cut off. This rules out the kernel OOM killer
  for that specific kill.

Conclusion so far: something outside the kernel and outside this session's own tool calls is
terminating backgrounded Bash tasks near-instantly and consistently. No `dmesg` access and no
passwordless `sudo` in this environment, so it can't be confirmed from inside the sandbox.
Asked the user to check their end (an accidental interrupt/stop gesture, or a background-task
policy in this terminal session) — awaiting a retry or diagnosis before re-attempting the
build/install pipelines above.

## Not attempted (out of this remediation's scope)

- `docs/audits/2026-09-05/deep-dives/04-remote-forge-and-credential-security.md` reported zero
  confirmed defects — nothing to fix.
- `docs/audits/2026-09-05/deep-dives/07-verification-strategy-and-test-reliability.md`'s
  TEST-002 (VSCode extension had zero E2E specs) and TEST-003 (flaky-test fix history) were
  already resolved/were strengths, not open findings — TEST-002 in particular was resolved by
  the branch-sync rebase, since `extension/e2e/` now exists.
