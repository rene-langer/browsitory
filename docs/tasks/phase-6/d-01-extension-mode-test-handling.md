# Task 6.D.01: Treat `ExtensionMode.Test` as development-mode for asset/sidecar resolution

## Goal
Make `@vscode/test-electron`'s `ExtensionMode.Test` resolve dev-mode assets (the sibling `frontend/dist-vscode` directory) and development-mode sidecar paths so the VSCode extension E2E harness can launch against an unpacked extension without requiring a pre-built packaged layout.

## Depends on
None.

## TDD requirement
`extension/src/extension.asset-root.test.ts` must:
1. Extend the `vscode` mock's `ExtensionMode` to include `Test: 3` (the real value from the vscode module).
2. Add a test case asserting that mode 3 (Test) resolves `resolveWebviewAssetRoot` to the dev-mode path (`/workspace/extension/../frontend/dist-vscode`), matching the behavior of mode 1 (Development).

The test must run and fail before any implementation changes are made.

## Acceptance criteria
- [ ] `extension/src/extension.asset-root.test.ts` includes a passing test for `ExtensionMode.Test` mode resolution.
- [ ] `extension/src/extension.ts`'s `resolveWebviewAssetRoot` function changes from `mode === vscode.ExtensionMode.Development` to `mode !== vscode.ExtensionMode.Production`.
- [ ] `extension/src/extension.ts`'s `activate()` function's sidecar path resolution changes from `context.extensionMode === vscode.ExtensionMode.Development` to `context.extensionMode !== vscode.ExtensionMode.Production`.
- [ ] `cd extension && pnpm test -- --run` passes (all 21 tests across 5 files).
- [ ] `cd extension && pnpm run compile` succeeds with no TypeScript errors.
- [ ] `cd extension && tsc -p . --noEmit --pretty false` passes (linting check).

## Out of scope
The E2E harness and test runner itself (Task 6.D.02 and later tasks in this workstream). This task only ensures the extension's asset and sidecar resolution logic handles Test mode correctly.
