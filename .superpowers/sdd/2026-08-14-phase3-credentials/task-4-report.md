# Task 4 report

## Delivered

- Extended the local-bare-remote E2E flow to choose SSH-agent authentication, verify that no
  password/token field is rendered, and inspect the real fixture repository's local config to
  prove it contains only `browsitory.remote.transfer-origin.auth-mode ssh-agent`.
- Added the release-acceptance procedure for disposable HTTPS and SSH repositories to
  `docs/ARCHITECTURE.md`, including the required no-secret inspection points.
- Added a focused state regression and the minimal failure mapping that turns a sanitized
  `missing credential` fetch failure into **Save an HTTPS token for this remote before
  retrying.** The mapping preserves generic transfer feedback for every other sanitized error,
  so arbitrary URLs and callback text stay hidden.

## Automated coverage

- Red: `frontend/node_modules/.bin/vitest run src/state/useAppState.test.ts` failed as expected:
  the new missing-credential assertion received `Fetch failed`.
- Green: the same focused command passed (38 tests) after the minimal state mapping.
- `cargo test --workspace`: passed (all Rust tests, including 38 `tauri-app` tests).
- `cargo fmt --all -- --check`: passed.
- `frontend/node_modules/.bin/eslint .`: passed.
- `frontend/node_modules/.bin/vite build`: passed.
- `git diff --check`: passed.
- Full `frontend/node_modules/.bin/vitest run` was started twice but did not complete before the
  execution environment's 30-second command cap; no test failure was emitted before the cap.
- `e2e/node_modules/.bin/tsc --noEmit` remains blocked by pre-existing errors in
  `e2e/specs/merge.spec.ts` (unsupported `ChainablePromiseElement.then` and implicit `any` at
  lines 50 and 130). This task does not modify that harness issue.
- The actual WebDriver E2E run was not performed: rebuilding the embedded Tauri binary is
  blocked waiting on the shared Cargo target-directory lock in this environment. It also needs
  the existing `tauri-driver` and display prerequisites.

## Manual acceptance and HTTPS E2E limitation

- Manual HTTPS-token and SSH-agent acceptance was not performed: this environment has no
  authorized disposable HTTPS account/token or disposable SSH host/key loaded in an agent. The
  exact procedure and required secret-safety inspection are documented in `docs/ARCHITECTURE.md`.
- A live missing-HTTPS-credential E2E assertion remains blocked by the existing IPC contract:
  `git-core` sanitizes transport messages and `Worker` emits only the generic `TransferFailed`
  terminal kind, with no safe missing-credential discriminator. Consequently a real async fetch
  currently reaches the UI as generic `Fetch failed`; the focused state test proves remediation
  whenever a future safe event supplies `missing credential`, but does not change the IPC error
  protocol. Expanding that protocol was explicitly out of Task 4 scope.
