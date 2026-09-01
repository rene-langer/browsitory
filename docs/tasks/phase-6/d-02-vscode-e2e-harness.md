# Task 6.D.02: Scaffold `extension/e2e/` and connect Playwright to the webview over CDP

## Goal

Scaffold a standalone `extension/e2e/` pnpm package — sibling to `extension/`, not a workspace
member of it — that drives the packaged VSCode extension inside a real Extension Development
Host via `@vscode/test-electron`, and provide `connectToWebview`, a Playwright-over-CDP helper
that resolves the extension's `vscode-webview://` page once it appears among the attached
browser's pages. This is the outer harness and Mocha loader only; it does not yet contain a
test spec.

## Depends on

Task 6.D.01 (fixed `extension/src/extension.ts` so `vscode.ExtensionMode.Test` resolves
dev-mode webview/sidecar asset paths, which is the mode `@vscode/test-electron` runs under).

## TDD requirement

None of this task's own — there is no spec file yet for the harness to run against. Task
6.D.03 writes the first real assertion (`extension/e2e/src/specs/*.spec.ts`, a first-flow test)
against this scaffold; this task's own bar is that the harness compiles and typechecks cleanly.

## Acceptance criteria

- [ ] `pnpm --dir extension/e2e typecheck` passes.
- [ ] `extension/e2e/package.json` pins exact resolved versions of
  `@vscode/test-electron`, `mocha`, and `playwright-core` (matching how `extension/package.json`
  pins `@vscode/vsce`), and has no `"type": "module"` field — `suite/index.ts` is loaded via
  `require()` by `@vscode/test-electron`'s extension-host machinery, which requires CommonJS.
- [ ] `extension/e2e/pnpm-lock.yaml` is committed, generated standalone (not folded into
  `extension/`'s own workspace/lockfile).
- [ ] `docs/LICENSE_COMPLIANCE.md` has a `## JavaScript, \`extension/e2e/\`` section documenting
  `@vscode/test-electron` and `playwright-core`'s real licenses (both permissive: MIT and
  Apache-2.0 respectively).
- [ ] `.gitignore` excludes `extension/e2e/node_modules`, `extension/e2e/out`, and
  `extension/e2e/.vscode-test`.

## Out of scope

The actual first-flow E2E spec (open the extension, connect to the webview, assert something
about it) — that is Task 6.D.03. Wiring this harness into CI. Per-spec fixture files beyond the
base fixture repo `runTests.ts` creates (added in a Mocha `before` hook in Task 6.D.03).
