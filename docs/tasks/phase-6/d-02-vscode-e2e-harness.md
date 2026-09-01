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

## Task 6.D.03 update: first-flow spec added

- [x] `extension/e2e/src/specs/first-flow.spec.ts` written (open repo via `browsitory.open`,
  stage `first-flow-fixture.txt`, commit, assert it shows up in history), consuming
  `connectToWebview` from this scaffold.
- [x] `@types/vscode@^1.134.0` added to `extension/e2e/package.json` devDependencies (same
  version `extension/package.json` pins) for typechecking the `vscode.*` calls in the spec;
  `pnpm --dir extension/e2e install` run standalone (`--ignore-workspace`, to avoid the outer
  `extension/pnpm-workspace.yaml` folding it into `extension/`'s own lockfile — see the note in
  this scaffold's acceptance criteria about `extension/e2e` staying a standalone package).
  `pnpm --dir extension/e2e typecheck` passes.
- [ ] **Not verified: a passing run of `xvfb-run --auto-servernum pnpm test`.** `cargo build
  --workspace`, both frontend builds (`pnpm build` and `vite build --config
  vite.vscode.config.ts`), and `extension`'s `pnpm run compile` all succeed. The harness itself
  runs — `@vscode/test-electron` downloads VSCode 1.135.0, launches the Extension Development
  Host under Xvfb with `--remote-debugging-port=9229`, and the extension activates and loads —
  but the test times out (30s) before `connectToWebview` finds a `vscode-webview://` page (first
  attempt: `connectToWebview`'s own 15s timeout fired directly with "No vscode-webview:// page
  found within 15000ms"; a second attempt hit the outer 30s Mocha timeout instead). While
  debugging, port 9229's CDP HTTP endpoint became completely unresponsive to plain `curl
  http://127.0.0.1:9229/json/list` as well (not just to Playwright) — `ss -ltnp` showed the
  listening socket attributed to a `dconf watch /system/proxy/` child process rather than the
  Electron/VSCode process, suggesting the fd got inherited into that helper process without
  `CLOEXEC` in this sandboxed environment, breaking the debug port for *any* client, not just
  this test. This looks like an environment/sandboxing quirk of the container this was run in
  (GTK/Electron's proxy-resolver helper stealing the listening fd) rather than a defect in
  `connectToWebview` or the spec's selectors — the same code should be re-tried in CI or a
  different sandbox before concluding the selectors themselves are wrong.
