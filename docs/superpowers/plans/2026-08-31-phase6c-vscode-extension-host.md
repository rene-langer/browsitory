# Phase 6c VSCode Extension Host Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run the existing Browsitory React UI inside a desktop VSCode webview, backed by one
window-scoped `vscode-sidecar` process, with the five VSCode-native `RepoClient` operations and
recoverable sidecar failures.

**Architecture:** Keep `RepoClient` as the frontend's only backend seam. Parameterize the React
root with a client, retaining the Tauri entry point while adding a VSCode-webview entry that uses
the already-complete `vscodeRepoClient`. A new `extension/` TypeScript package owns the webview,
the JSON-RPC forwarding bridge, direct VSCode APIs, and the sidecar process; it forwards all
other calls and server notifications unchanged.

**Tech Stack:** TypeScript, React 19, Vite 8, VSCode Extension API, Node.js child processes,
JSON-RPC 2.0, Vitest, TypeScript compiler.

**Spec:** `docs/superpowers/specs/2026-08-30-vscode-extension-design.md`

## Global Constraints

- Desktop VSCode only; vscode.dev and Codespaces are out of scope.
- `RepoClient` remains the sole frontend transport seam. No component or state module imports
  VSCode or a transport API.
- The webview sends and receives the same JSON-RPC 2.0 objects as `vscode-sidecar`; the host
  only correlates and forwards them.
- `pickRepoFolder`, `openExternalUrl`, `getAppVersion`, `getLastSeenVersion`, and
  `setLastSeenVersion` are implemented in `extension/` with VSCode APIs, never forwarded to the
  sidecar.
- Start one sidecar per VSCode window lazily on the first forwarded repository request; kill it
  on extension deactivation. A crash/exited process rejects every outstanding request, reports
  its state to the webview, and may be restarted by a later request.
- Use no new Rust dependencies and no new runtime frontend dependencies. Add only the extension
  development dependencies necessary to compile and unit-test the extension host; record them in
  `docs/LICENSE_COMPLIANCE.md` before committing.
- Sub-phase 6d owns target-specific `.vsix` packaging/CI and sidecar-binary bundling; sub-phase
  6e owns `@vscode/test-electron` end-to-end tests. This plan provides only host/frontend unit
  tests and a manual development launch path.
- Preserve Tauri behavior: `pnpm build`, `pnpm lint`, and `pnpm test -- --run` in `frontend/`,
  plus the workspace Rust gates, must remain green.
- Commit after every task.

## File Structure

| Path | Responsibility |
| --- | --- |
| `frontend/src/App.tsx` | Accepts a `RepoClient` and uses it throughout instead of importing the Tauri adapter. |
| `frontend/src/main.tsx` | Tauri bootstrap; supplies `tauriRepoClient` to `App`. |
| `frontend/src/vscode-main.tsx` | VSCode-webview bootstrap; supplies `vscodeRepoClient` to `App`. |
| `frontend/vite.vscode.config.ts` | Produces a relocatable, CSP-compatible webview bundle without changing Tauri's build. |
| `frontend/src/ipc/vscodeRepoClient.ts` | Completes webview message handling for host-native methods and transport lifecycle events. |
| `extension/package.json`, `tsconfig.json` | Extension manifest, VSCode API typing, build/test scripts, and development metadata. |
| `extension/src/extension.ts` | Activation, `Browsitory: Open` command, one reusable `WebviewPanel`, and deactivation. |
| `extension/src/sidecarBridge.ts` | Lazy process spawn, newline-delimited JSON-RPC forwarding, direct VSCode-native dispatch, and restart state. |
| `extension/src/webviewHtml.ts` | Nonce/CSP-safe HTML referencing the VSCode frontend bundle through `asWebviewUri`. |
| `extension/src/*.test.ts` | Isolated extension-host tests using mocked VSCode API and child-process seams. |
| `docs/tasks/phase-6/` | Task briefs conforming to `docs/TASK_TEMPLATE.md`. |
| `docs/ARCHITECTURE.md`, `docs/LICENSE_COMPLIANCE.md`, `CHANGELOG.md` | Record the new host boundary, approved dependencies, and delivered sub-phase. |

### Task 1: Make the frontend transport-selectable and finish the webview protocol

**Files:**
- Create: `frontend/src/vscode-main.tsx`
- Create: `frontend/vite.vscode.config.ts`
- Modify: `frontend/src/App.tsx`, `frontend/src/main.tsx`, `frontend/src/ipc/vscodeRepoClient.ts`
- Modify: `frontend/src/App.test.tsx`, `frontend/src/ipc/vscodeRepoClient.test.ts`
- Create: `docs/tasks/phase-6/a-01-webview-frontend.md`

**Interfaces:**
- Consumes: `RepoClient` from `frontend/src/ipc/RepoClient.ts` and existing JSON-RPC replies and
  `transferProgress` notifications from `vscodeRepoClient`.
- Produces: `App({ client }: { client: RepoClient })`; Tauri and VSCode entry modules which each
  mount it with their own adapter; host message shapes
  `{ jsonrpc: "2.0", id, method, params }` and
  `{ jsonrpc: "2.0", method: "transportStatus", params: { state: "reconnecting" | "failed", message: string } }`.

- [ ] **Step 1: Write failing frontend tests.**

  In `App.test.tsx`, render `App` with a complete fake `RepoClient` and assert initial repository
  loading calls that fake, proving `App` no longer reaches the Tauri adapter. In
  `vscodeRepoClient.test.ts`, mock `acquireVsCodeApi`, post host replies for each of the five
  native method names, and assert the matching promises resolve. Add a rejected `transportStatus`
  case asserting all currently pending calls reject with the supplied message and a later call
  posts a fresh request.

- [ ] **Step 2: Run the focused tests and confirm they fail because `App` cannot accept a client
  and the VSCode-native/lifecycle messages are not handled.**

  Run: `cd frontend && pnpm test -- --run src/App.test.tsx src/ipc/vscodeRepoClient.test.ts`

- [ ] **Step 3: Parameterize `App` and add both bootstraps.**

  Replace every `tauriRepoClient` reference in `App.tsx` (including `RepoWorkspace`, release-note
  version checks, and updater-related child props) with its injected `client`. Keep
  `main.tsx` as `render(<App client={tauriRepoClient} />)`. Add `vscode-main.tsx` with
  `render(<App client={vscodeRepoClient} />)`. Do not change component/state signatures other
  than passing the same client down from `App`.

- [ ] **Step 4: Complete `vscodeRepoClient`'s host-local protocol.**

  Treat a successful or error JSON-RPC response for each native method as any other response;
  the extension bridge supplies it. Recognize only `transportStatus` as an additional
  notification: on `reconnecting` or `failed`, remove and reject every entry in `pending`, then
  notify an exported, unsubscribe-based status listener consumed at the `App` boundary to render
  the existing inline error treatment. Keep `transferProgress` behavior unchanged. Ensure
  listener cleanup is deterministic in tests via a test-only reset export rather than module
  re-import ordering.

- [ ] **Step 5: Add a dedicated webview Vite configuration.**

  Configure `vite.vscode.config.ts` to use `src/vscode-main.tsx`, emit deterministic assets under
  `dist-vscode/`, use relative asset URLs (`base: "./"`), and retain React/TypeScript checking.
  Do not modify the existing Tauri `vite.config.ts` or its `dist/` output.

- [ ] **Step 6: Run verification and commit.**

  Run: `cd frontend && pnpm test -- --run src/App.test.tsx src/ipc/vscodeRepoClient.test.ts && pnpm lint && pnpm build && pnpm exec vite build --config vite.vscode.config.ts`

  Commit: `feat(frontend): add vscode webview bootstrap`

### Task 2: Create the extension host and JSON-RPC sidecar bridge

**Files:**
- Create: `extension/package.json`, `extension/tsconfig.json`
- Create: `extension/src/extension.ts`, `extension/src/sidecarBridge.ts`, `extension/src/webviewHtml.ts`
- Create: `extension/src/sidecarBridge.test.ts`, `extension/src/webviewHtml.test.ts`
- Modify: root `.gitignore` only if TypeScript output is not already covered
- Create: `docs/tasks/phase-6/b-01-extension-host.md`

**Interfaces:**
- Consumes: webview JSON-RPC requests, `vscode-sidecar` stdin/stdout, `vscode.ExtensionContext`,
  and `vscode.WebviewPanel`.
- Produces: `activate(context)`, `deactivate()`, and `SidecarBridge.handleWebviewMessage(message)`;
  direct replies retain the incoming request id; sidecar stdout objects are relayed byte-for-byte.

- [ ] **Step 1: Write failing host tests with injected seams.**

  Define `SidecarBridge` constructor dependencies for `spawn`, `ExtensionContext`, and a
  `postToWebview` callback. Test that a non-native request lazily spawns exactly one process,
  writes one JSON line to stdin, and relays its parsed stdout response and notification unchanged.
  Test that the five native methods do not spawn a process and reply with: a folder path or null
  from `showOpenDialog`, a boolean success result from `env.openExternal`, the manifest version,
  and `globalState` values for version read/write.

- [ ] **Step 2: Run extension unit tests and confirm they fail before the package and bridge
  exist.**

  Run: `cd extension && pnpm test -- --run src/sidecarBridge.test.ts src/webviewHtml.test.ts`

- [ ] **Step 3: Create the extension manifest and compiler setup.**

  Add the `browsitory.open` command contribution and `onCommand:browsitory.open` activation.
  Use the VSCode engine range supported by the current desktop development environment, declare
  `@types/vscode` as a development dependency, and add compile, lint, and test scripts. Record
  every new package and its permissive license in `docs/LICENSE_COMPLIANCE.md` before it is used.

- [ ] **Step 4: Implement the panel and secure HTML.**

  `activate` registers `browsitory.open`, creates/reveals one `WebviewPanel`, enables scripts,
  restricts `localResourceRoots` to `frontend/dist-vscode`, and registers a single message
  handler. `webviewHtml.ts` emits a fresh nonce, a CSP allowing only that nonce and VSCode's
  webview resource origin, and script/style URIs generated through `webview.asWebviewUri`; no
  inline executable code or workspace file access is permitted.

- [ ] **Step 5: Implement forwarding and VSCode-native dispatch.**

  Parse newline-delimited sidecar stdout as JSON-RPC objects, relay valid replies/notifications
  to the current panel, and log malformed lines to the extension output channel without crashing
  the host. Forward non-native webview requests as one newline-delimited JSON object. For the
  native methods, validate the small parameter shapes, invoke the VSCode API listed in the spec,
  and send a JSON-RPC result/error with the original id. Resolve the development sidecar from the
  workspace `target/debug` path; expose the packaged-binary resolver as one isolated function for
  Phase 6d to replace with its target-triple location.

- [ ] **Step 6: Run verification and commit.**

  Run: `cd extension && pnpm test -- --run && pnpm run compile && pnpm run lint`

  Commit: `feat(extension): add vscode webview host and sidecar bridge`

### Task 3: Make sidecar failure visible and recoverable

**Files:**
- Modify: `extension/src/sidecarBridge.ts`, `extension/src/extension.ts`
- Modify: `extension/src/sidecarBridge.test.ts`, `frontend/src/ipc/vscodeRepoClient.test.ts`, `frontend/src/App.test.tsx`
- Create: `docs/tasks/phase-6/b-02-sidecar-lifecycle.md`

**Interfaces:**
- Consumes: child-process `error`, `exit`, stdin write errors, and webview request ids.
- Produces: `dispose(): Promise<void>` and `transportStatus` notifications; every unresolved
  request receives `{ jsonrpc: "2.0", id, error: { code: -32001, message } }` on process loss.

- [ ] **Step 1: Write failing lifecycle tests.**

  Start two forwarded requests with a fake child process, emit `exit`, and assert two error
  replies with their original ids plus one `transportStatus` notification. Then issue a third
  request and assert a new child is spawned and receives it. Test process `error` and stdin
  write failure identically. Test `deactivate` kills a live child, rejects pending requests, and
  leaves no restart timer or message listener behind.

- [ ] **Step 2: Run the focused lifecycle tests and confirm failure.**

  Run: `cd extension && pnpm test -- --run src/sidecarBridge.test.ts`

- [ ] **Step 3: Add the lifecycle state machine.**

  Maintain bridge states `idle`, `running`, `reconnecting`, and `failed`. A process loss moves
  to `reconnecting`, rejects the pending-id set, and posts `transportStatus`; a failed immediate
  restart or spawn error moves to `failed` with its diagnostic. The next repository request
  attempts one fresh spawn. Do not silently replay mutating requests, retain stale pending ids,
  or start more than one process for a window.

- [ ] **Step 4: Connect disposal and frontend presentation.**

  Register `context.subscriptions` disposal so closing the panel or deactivating VSCode calls
  bridge disposal. Verify `vscodeRepoClient` converts the bridge error/status into rejected
  promises and `App` displays it through its existing global inline-error path; Tauri has no
  status listener and keeps its existing updater/error behavior.

- [ ] **Step 5: Run all local gates and manually smoke-test development mode.**

  Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build && pnpm exec vite build --config vite.vscode.config.ts`

  Run: `cd extension && pnpm test -- --run && pnpm run compile && pnpm run lint`

  Run: `cargo build -p vscode-sidecar`

  Manual: launch an Extension Development Host, run **Browsitory: Open**, choose a repository,
  verify status/history load, stop the sidecar, verify an in-app failure message rather than a
  hung action, then retry an action and verify a fresh sidecar serves it.

- [ ] **Step 6: Update documentation and commit.**

  Update `docs/ARCHITECTURE.md` with the host/webview/sidecar lifecycle and native-method split,
  and add the delivered sub-phase to `CHANGELOG.md`. Commit:
  `feat(extension): recover from vscode sidecar failures`

## Final Verification

- [ ] `cargo build --workspace`
- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo fmt --all -- --check`
- [ ] `cd frontend && pnpm lint && pnpm test -- --run && pnpm build && pnpm exec vite build --config vite.vscode.config.ts`
- [ ] `cd extension && pnpm test -- --run && pnpm run compile && pnpm run lint`
- [ ] The manual Extension Development Host smoke test in Task 3 succeeds.

## Self-Review

- Spec coverage: Tasks 1–3 cover the webview frontend, extension host, direct native methods,
  sidecar lifecycle/crash handling, and unit-level verification. The design's packaging/CI and
  VSCode Electron E2E requirements are expressly retained for 6d and 6e.
- Placeholder scan: none.
- Type consistency: webview requests and replies use existing JSON-RPC shapes; the only added
  host notification is `transportStatus`, which is deliberately separate from `RepoClient` so
  the interface remains transport-neutral.
