# Task 6.D.02: Scaffold `extension/e2e/` and connect to the webview over raw CDP

## Goal

Scaffold a standalone `extension/e2e/` pnpm package — sibling to `extension/`, not a workspace
member of it — that drives the unpacked VSCode extension inside a real Extension Development
Host via `@vscode/test-electron`, and provide `connectToWebview`, a raw-CDP helper (Node's
native `WebSocket`/`fetch`, no browser-automation library) that attaches to the extension's
`vscode-webview://` target, finds its nested content frame, and returns a `WebviewSession` for
driving DOM interaction via `Runtime.evaluate`. This is the outer harness and Mocha loader only;
it does not yet contain a test spec.

(The initial scaffold used `playwright-core` over CDP for this; Task 6.D.03's fix loop replaced
it with a hand-rolled raw-CDP client after discovering Playwright's frame API doesn't reliably
surface VSCode's nested webview content frame — see that task's update below.)

## Depends on

Task 6.D.01 (fixed `extension/src/extension.ts` so `vscode.ExtensionMode.Test` resolves
dev-mode webview/sidecar asset paths, which is the mode `@vscode/test-electron` runs under).

## TDD requirement

None of this task's own — there is no spec file yet for the harness to run against. Task
6.D.03 writes the first real assertion (`extension/e2e/src/specs/*.spec.ts`, a first-flow test)
against this scaffold; this task's own bar is that the harness compiles and typechecks cleanly.

## Acceptance criteria

- [ ] `pnpm --dir extension/e2e typecheck` passes.
- [x] `extension/e2e/package.json` pins exact resolved versions of
  `@vscode/test-electron` and `mocha` (matching how `extension/package.json` pins `@vscode/vsce`),
  and has no `"type": "module"` field — `suite/index.ts` is loaded via `require()` by
  `@vscode/test-electron`'s extension-host machinery, which requires CommonJS. `playwright-core`
  was added, then removed once the raw-CDP client replaced it (see the D.03 update below) — it
  is not a dependency of the final harness.
- [x] `extension/e2e/pnpm-lock.yaml` is committed, generated standalone (not folded into
  `extension/`'s own workspace/lockfile).
- [x] `docs/LICENSE_COMPLIANCE.md` has a `## JavaScript, \`extension/e2e/\`` section documenting
  `@vscode/test-electron`'s real license (MIT). No `playwright-core` row — it's not a dependency.
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
- [x] **Verified: `xvfb-run --auto-servernum pnpm test` passes reliably.** Getting here took
  four fix rounds; see `.superpowers/sdd/2026-09-01-phase6e-vscode-e2e/task-3-report.md` (a
  git-ignored SDD workspace file, not in the repo) for the full trail. Summary of the two real
  bugs found and fixed, independent of the environment noise below:
  1. **Wrong CDP target shape.** VSCode exposes the webview as an `iframe`-type CDP target that
     is a *child frame* of the single top-level workbench `page`-type target, never its own
     top-level page — the original Playwright-based `connectToWebview` searched
     `browser.contexts()[].pages()` and could never find it.
  2. **Wrapper-frame false match.** The webview's real content lives in a *nested* leaf frame
     inside an outer sandbox wrapper frame that has no DOM of its own; before that nested frame
     is created, the wrapper frame is itself a childless "leaf" whose URL also starts with
     `vscode-webview://`, so a naive first-match search can grab the empty wrapper and then every
     subsequent selector times out. Fixed by excluding the tree root from the leaf search
     (`nestedLeafFrames`) and polling (`findContentFrameContext`) until a candidate frame's
     execution context evaluates `!!document.body`, not just until *a* frame matches the URL
     prefix.

  Also present, and unrelated to the above: a `dconf watch /system/proxy/` helper process
  intermittently squats the CDP debug port (GTK/GLib's proxy-resolver mechanism, triggered by
  Electron). Mitigated (not eliminated) via `GSETTINGS_BACKEND=memory`,
  `GIO_USE_PROXY_RESOLVER=dummy`, a best-effort `pkill -f "dconf watch"` before launch, and
  picking a dynamically free CDP port instead of a fixed one — this cut failures sharply but the
  final round's reviewer confirmed the helper can still reappear and squat the *next* run's port,
  so CI (Task 6.D.03's follow-on CI task) needs the same cleanup discipline, not just a one-time
  fix here. Final verification: 20/20 passes across four independent 5-run sets (implementer) and
  a further 19/19 across three load regimes including a deliberately heavy-load run (independent
  re-reviewer) — see the report file for full pasted evidence.
