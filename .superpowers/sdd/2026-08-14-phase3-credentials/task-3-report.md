# Task 3 report

## Delivered

- Added Tauri commands and `RepoClient` methods to save/forget HTTPS credentials and select HTTPS-token or SSH-agent authentication per remote.
- Remote list DTOs expose only non-secret auth metadata (mode and HTTPS username); no credential DTO exists.
- Added RemotePanel credential controls. The access token is an uncontrolled password input with `autocomplete="off"`; it is read only while invoking the save callback and cleared in `finally`.
- Wired app state actions and non-secret remediation messages for missing credentials, unavailable keychains, and SSH-agent failures.
- Added mocked UI, RepoClient, and app-state coverage, plus a Rust wire-format test for auth modes.

## Verification

- Red: `pnpm test -- --run RemotePanel` could not start because `pnpm` is not installed in the environment.
- Green: `frontend/node_modules/.bin/vitest run src/components/RemotePanel.test.tsx src/state/useAppState.test.ts src/ipc/tauriRepoClient.test.ts` — 56 passed.
- `frontend/node_modules/.bin/eslint .` — passed.
- `frontend/node_modules/.bin/vite build` — passed.
- `cargo fmt --all -- --check` — passed.
- `cargo test -p tauri-app` — 38 passed.
- `git diff --check` — passed.
