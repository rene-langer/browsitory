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
