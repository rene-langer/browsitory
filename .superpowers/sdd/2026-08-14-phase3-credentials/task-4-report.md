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

## Follow-up: safe terminal credential-failure kind

- Added `MissingCredential` as a terminal transfer error kind. `git-core` classifies only its
  own stable missing-credential callback marker; the worker carries that enum value unchanged,
  the Tauri event DTO exposes only `MissingCredential` (never a diagnostic), and `RepoClient` /
  `useAppState` turn it into the HTTPS-token remediation message.
- Added a loopback-only E2E fixture that returns an HTTP authentication challenge. It configures
  only non-secret local Git metadata, stores no test token, and asserts the rendered remediation
  does not contain the fixture URL.
- Red: the focused git-core test initially failed because `TransferErrorKind::MissingCredential`
  did not exist. Green: `cargo test -p git-core --test remote` passed (22 tests),
  `cargo test -p tauri-app commands::tests::missing_credential_failure_is_emitted_as_a_safe_terminal_kind`
  passed, and `frontend/node_modules/.bin/vitest run src/state/useAppState.test.ts` passed
  (38 tests).
- Follow-up full verification: `cargo test --workspace` passed, `cargo clippy --workspace
  --all-targets -- -D warnings` passed, `cargo fmt --all -- --check` passed after formatting,
  and frontend ESLint/Vite build passed.
- The rebuilt targeted E2E command reached WebdriverIO but could not start its driver because
  the pre-existing harness ports were occupied (`127.0.0.1:4444` and `4445`). The unrelated
  TypeScript errors in `e2e/specs/merge.spec.ts` remain unchanged.

## Manual acceptance

- Manual HTTPS-token and SSH-agent acceptance was not performed: this environment has no
  authorized disposable HTTPS account/token or disposable SSH host/key loaded in an agent. The
  exact procedure and required secret-safety inspection are documented in `docs/ARCHITECTURE.md`.
- The loopback-only E2E assertion is implemented but has not completed in this environment due
  to the occupied existing WebDriver ports described above.

## Review follow-up: trusted marker and valid HTTPS fixture

- Tightened `MissingCredential` classification to the exact stable callback marker. A new
  adversarial regression proves that a remote-controlled diagnostic merely containing that text
  (including a credential-bearing URL) remains the generic `TransferFailed` kind.
- Restricted the marker's emission to `CredentialStore::get` returning `Ok(None)`. Keychain
  failures and invalid/non-HTTPS credential URLs now use a different internal generic error and
  therefore remain sanitized generic transfer failures. No provider error message crosses IPC.
- Replaced the loopback HTTP fixture with a valid `https://localhost:<port>` fixture. It creates
  a one-day local test certificate with a `localhost` SAN, trusts that certificate through the
  fixture repository's local `http.sslCAInfo`, and serves a 401 challenge. Thus the credential
  provider reaches its valid HTTPS lookup path without disabling certificate validation or using
  a live host/token.
- Red: the wrapped-diagnostic regression initially classified as `MissingCredential`; the store
  failure regression initially reused the missing-token marker. Green focused checks passed for
  both. `cargo test --workspace` passed (23 remote tests and 40 tauri-app tests), as did clippy
  and fmt. E2E typecheck still reports only the pre-existing `merge.spec.ts` errors.

## Final typed-error follow-up

- Fresh HTTPS challenges with no auth metadata now use the same safe `MissingCredential` kind.
  Keychain lookup and SSH-agent failures have distinct safe terminal kinds; no callback detail is
  emitted. Pull now retains its typed fetch/provider error until the terminal event, while its
  synchronous reply remains `pull failed`.
- `cargo test --workspace`, clippy, and fmt passed (41 tauri-app tests). Manual/E2E environment
  blockers remain unchanged.

## Pull terminal-event ordering follow-up

- A sanitized synchronous `pull failed` rejection now leaves the matching in-flight transfer for
  its terminal event to resolve, so typed credential remediation is not overwritten. State tests
  cover Pull ordering plus safe keychain Fetch and SSH-agent Push remediation without provider
  diagnostics. Focused frontend tests (39), lint, and build pass; tauri-app tests and fmt pass.

## E2E stale-artifact investigation

- The reported `Fetch failed` came from a stale embedded-app binary, not the current source
  path: `target/debug/tauri-app` was timestamped 14:16 while the frontend assets and typed-error
  changes were newer. Rebuilt in the required order: `frontend` with
  `VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo`, then `cargo build --workspace --features
  tauri-app/custom-protocol`. The rebuilt frontend asset is timestamped 15:11 and the binary
  15:12. Run WebDriver only after this paired rebuild; plain builds do not embed the fixture
  frontend.

## Direct-fetch terminal-event ordering follow-up

- The rebuilt E2E still reached `Fetch failed`. A state regression reproduced the actual
  ordering: a direct Fetch rejects with the sanitized `Fetch failed` before its matching typed
  terminal event. `startTransfer` now retains that Fetch latch so the terminal event supplies
  remediation; unrelated direct-fetch errors still render normally. Red: `Error: Fetch failed`.
  Green: 40 focused state tests, frontend lint, and build pass.

## Frontend build-gate fixture follow-up

- The Phase 3 `RepoClient` expansion left the same three required credential methods absent
  from four component-test fakes. Added explicit unused implementations, preserving the required
  interface rather than weakening it. `corepack pnpm build`, `corepack pnpm lint`, and the four
  focused component suites passed (56 tests).

## Live E2E timing investigation follow-up

- The configured local fetch assertion was testing a transient implementation detail: a local
  bare-repository fetch can complete between React renders, so `Transfer progress` may never be
  observable even when the operation succeeded. The E2E now waits for
  `refs/remotes/transfer-origin/main` to equal the known source `main` commit, then verifies the
  Fetch button is enabled. This proves the durable fetch outcome without imposing artificial
  delay on production transfer UX.
- The HTTPS credential fixture now counts received requests and the test requires at least one
  loopback HTTPS request before evaluating the terminal alert. The next live run therefore has a
  decisive boundary result: failure at that wait means TLS/fixture setup prevented the provider
  path; a received request followed by `Fetch failed` proves a live worker/event propagation
  defect. The counter and assertion contain neither credentials nor provider diagnostics.
- Verification: E2E `tsc --noEmit` reaches only the pre-existing four `merge.spec.ts` errors;
  the changed spec contributes none. Rebuilt the embedded app in required order with
  `VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo corepack pnpm build`, then
  `cargo fmt --all -- --check` and
  `cargo build --workspace --features tauri-app/custom-protocol`; all three passed. A live
  WebDriver rerun remains required outside this sandbox/display/port-constrained environment.

## Live E2E cross-test ordering follow-up

- The stabilized configured Fetch now reaches its durable tracking-ref assertion. The next
  failure occurred before any credential transfer: `Fetch credential-origin` did not render
  after adding the remote. This is a test ordering race, not a credential-provider result. The
  immediately preceding SSH-agent test waited only for the backend Git config write; its
  `runMutation` refresh was still asynchronous. A later add-remote refresh could therefore be
  overwritten by that earlier, stale remote list.
- Moved the independent SSH UI acceptance case to the end of the transfer sequence, after the
  credential, Pull, and Push cases. This preserves the same real UI/config assertion while
  removing the stale-refresh overlap from dependent E2E setup. The credential test can now
  establish and observe `credential-origin` before it configures and Fetches it; Pull follows
  the terminal credential failure rather than a concurrent remote refresh.
- This also identified a narrow product race: Add remote was actionable while the existing
  repository-operation latch was set by a credential mutation. A fast second mutation could
  race its refresh against the first and retain an old remote list. The Add remote submit control
  now uses that same latch. Red: the focused `RemotePanel` test found it enabled during an active
  operation. Green: the focused suite passes with 16 tests, and no interface was weakened.
- Replaced the remaining transient branch-Push progress-panel assertion with the existing
  durable bare-remote branch-ref assertion. Local transfers may complete between React renders;
  the remote ref is the user-visible operation outcome and avoids another timing-only failure.
- Verification: `git diff --check`, frontend lint, and the focused `RemotePanel` suite passed.
  E2E `tsc --noEmit` still reaches only the known four `merge.spec.ts` errors. Rebuilt with
  `VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo corepack pnpm build`, then passed
  `cargo fmt --all -- --check` and
  `cargo build --workspace --features tauri-app/custom-protocol`. A fresh live WebDriver run is
  required to verify the now-isolated credential request probe and Pull fast-forward result.

## Live E2E HTTPS transport and Pull precondition follow-up

- The live request-count probe remained zero after the remote was visibly added, proving that
  the credential provider was not involved. The exact cause is trust initialization: this build
  uses vendored libgit2/OpenSSL, which obtains its CA location at process initialization rather
  than from repository-local `http.sslCAInfo`. TLS therefore rejected the per-test self-signed
  server before its HTTP 401 challenge or the credential callback.
- The WebDriver prepare hook now creates the short-lived localhost certificate before
  `tauri-driver` starts, sets `SSL_CERT_FILE` for the driver/app process tree, and supplies the
  corresponding key/certificate paths only to the E2E server fixture. The test server still
  binds loopback, the certificate still has a localhost SAN, and no TLS verification is disabled.
  The server directory is removed after the session. The credential case additionally asserts
  the stored remote URL before Fetching, so a missing fixture request cannot be misattributed to
  a different remote configuration.
- Pull's unchanged HEAD was a fixture precondition failure, not a fast-forward implementation
  defect: `setupFixtureRepo` created an untracked `README.md`, while the transfer suite committed
  only its seed file. `pull_after_fetch` correctly rejects dirty worktrees. The suite now commits
  both files in its initial baseline and asserts a clean porcelain status immediately before
  configuring/Pulling the upstream.
- Verification: `git diff --check`, `cargo fmt --all -- --check`, and
  `cargo build --workspace --features tauri-app/custom-protocol` passed. Rebuilt the embedded
  frontend with `VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo corepack pnpm build`. E2E
  `tsc --noEmit` continues to stop only at the existing four `merge.spec.ts` errors. A live
  WebDriver rerun is required to exercise the inherited process trust chain.
