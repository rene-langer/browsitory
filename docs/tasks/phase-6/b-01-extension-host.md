# Task 6.B.01: VSCode extension host

## Goal

Add the desktop VSCode extension host that renders Browsitory's dedicated webview bundle,
routes the five VSCode-native `RepoClient` operations directly, and lazily forwards every
repository request to `vscode-sidecar` over newline-delimited JSON-RPC.

## Depends on

Task 6.A.01 (webview frontend and complete VSCode client protocol).

## TDD requirement

`extension/src/sidecarBridge.test.ts` must first assert lazy single-process spawning, exact
one-request-per-line writes, UTF-8-safe split stdout framing, unchanged response and
notification relay, malformed-line recovery, native folder/external/version/global-state
routing, and invalid-parameter replies with the incoming id.
`extension/src/webviewHtml.test.ts` must first assert nonce uniqueness, CSP restrictions,
and conversion of both frontend assets through `webview.asWebviewUri`.

## Acceptance criteria

- [ ] `browsitory.open` creates or reveals one script-enabled webview panel.
- [ ] `localResourceRoots` contains only `frontend/dist-vscode`.
- [ ] The webview document uses a fresh nonce, restrictive CSP, and VSCode resource URIs.
- [ ] The five host-local methods never spawn or forward to the Rust sidecar.
- [ ] Other requests lazily start one sidecar and preserve JSON-RPC response/notification data.
- [ ] Malformed sidecar stdout is logged without terminating the extension host.
- [ ] `pnpm test -- --run && pnpm run compile && pnpm run lint` passes in `extension/`.

## Out of scope

Sidecar crash recovery, pending-request rejection, and restart status are Task 6.B.02.
Target-triple binary bundling and VSIX packaging belong to Phase 6d; real VSCode E2E tests
belong to Phase 6e.
