# Task 6.A.01: VS Code webview frontend

## Goal

Make the React application transport-selectable and produce a dedicated VS Code webview
bundle, so the existing UI can run through the `RepoClient` JSON-RPC bridge without changing
component or state-layer transport boundaries.

## Depends on

The existing `RepoClient` interface and `vscodeRepoClient` sidecar protocol.

## TDD requirement

`frontend/src/App.test.tsx` must prove initial repository restoration uses an injected fake
`RepoClient` and that transport failures use the existing inline error treatment.
`frontend/src/ipc/vscodeRepoClient.test.ts` must prove the five host-local methods resolve
through JSON-RPC replies, both terminal transport states reject all pending calls, later calls
start fresh, status subscriptions unsubscribe, and test lifecycle state resets deterministically.

## Acceptance criteria

- [ ] Tauri and VS Code bootstraps mount `App` with their respective `RepoClient` adapters.
- [ ] `transportStatus` notifications reject pending requests and surface their message in `App`.
- [ ] Focused frontend tests and lint pass.
- [ ] The standard frontend build still emits `dist/`.
- [ ] The VS Code Vite build emits fixed-name, relative assets under `dist-vscode/`.

## Out of scope

The VS Code extension host, sidecar process lifecycle, secure webview HTML, and extension packaging.
