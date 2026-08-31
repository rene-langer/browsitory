# Task 6.B.02: Recoverable sidecar lifecycle

## Goal

Make loss of the VSCode Rust sidecar visible and recoverable without replaying repository
mutations: reject every unresolved JSON-RPC request, surface one transport diagnostic in the
webview, and lazily start one fresh process when the user retries.

## Depends on

Task 6.A.01 (webview transport-status presentation) and Task 6.B.01 (extension host and lazy
sidecar bridge).

## TDD requirement

`extension/src/sidecarBridge.test.ts` must first assert that process exit, process error, and
stdin write failure preserve and reject all pending request ids with JSON-RPC error code
`-32001`, publish one matching `transportStatus` notification, never replay a pending mutation,
and let a later repository request start one replacement process. It must also cover a failed
replacement spawn and deterministic listener/timer cleanup. `extension/src/extension.test.ts`
must first assert that both panel closure and `deactivate()` kill the live child, reject pending
work, and remove host message listeners. Existing frontend adapter and `App` tests must continue
to prove that lifecycle errors reject promises and use the global inline-error presentation.

## Acceptance criteria

- [ ] The bridge maintains `idle`, `running`, `reconnecting`, and `failed` lifecycle states.
- [ ] Process exit/error and stdin failure reject each unresolved id exactly once with `-32001`.
- [ ] One `transportStatus` notification carries the same diagnostic to the React app.
- [ ] No repository request is replayed; the next user request makes at most one fresh spawn.
- [ ] Panel closure, extension-context disposal, and `deactivate()` dispose process/listener state.
- [ ] Frontend and extension tests, lint, builds, and `cargo build -p vscode-sidecar` pass.

## Out of scope

Target-triple sidecar bundling and VSIX packaging remain Phase 6d. Automated Extension
Development Host coverage with `@vscode/test-electron` remains Phase 6e.
