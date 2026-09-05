# Phase 6e VSCode Extension E2E Harness Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a real, black-box end-to-end test for the VSCode extension — open a disposable
repo, stage a file, commit, see it in history — driving the actual webview UI inside a real
VSCode (Electron) instance, mirroring `e2e/`'s existing "one flow per feature area" convention
for the Tauri app.

**Architecture:** `@vscode/test-electron` downloads and launches a real VSCode Extension
Development Host with the unpacked `extension/` loaded in development mode (no VSIX packaging
step — same "drive the real built app, not a distributable installer" choice `e2e/` already
makes for `tauri-app`). VSCode is launched with `--remote-debugging-port` so the Mocha suite
running inside the Extension Development Host can also attach `playwright-core` over CDP to the
same running Electron instance, locate the `vscode-webview://…` page VSCode renders the
extension's webview into, and drive real DOM clicks against it — the only way to reach webview
content, since `@vscode/test-electron` alone only exposes the `vscode` API surface, not webview
DOM. The extension's existing `frontend/dist-vscode` build already auto-opens a fixture repo
when built with `VITE_E2E_REPO_PATH` baked in (`frontend/src/App.tsx`), the same mechanism
`e2e/`'s `wdio.conf.ts` already relies on for the Tauri app, so no native folder-picker dialog
needs automating.

**Tech Stack:** Node.js 24, pnpm 11/9 (mirrors `e2e/`'s pnpm 9), TypeScript, `@vscode/test-electron`,
Mocha (the framework `@vscode/test-electron` requires for `extensionTestsPath`), `playwright-core`
(CDP client only — no browser download, since it attaches to the already-launched VSCode),
Rust/Cargo, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-08-30-vscode-extension-design.md` (Testing section,
Roadmap sub-phase (e)).

## Global Constraints

- Desktop VSCode only — no vscode.dev/Codespaces, no Marketplace publishing. (Inherited from the
  spec's top-level constraint; this sub-phase doesn't touch either.)
- Land exactly one flow: open repo → stage a file → commit → see it in history, mirroring
  `e2e/specs/first-flow.spec.ts`. Do not seed additional flows in this pass.
- Test against the **unpacked development extension** (`extensionDevelopmentPath` = `extension/`,
  dev sidecar debug build, `frontend/dist-vscode` dev build) — never a packaged `.vsix`. No
  changes to the Task 6d packaging pipeline.
- `extension/e2e/` gets its own `package.json` (separate pnpm package, same pattern as top-level
  `e2e/` relative to `frontend/`/`extension/`), not folded into `extension/package.json`.
- Add `@vscode/test-electron` and `playwright-core` only as `extension/e2e/` devDependencies,
  verify their licenses with `npm info <package>@<version> license`, and record them in
  `docs/LICENSE_COMPLIANCE.md` before committing.
- Generated `extension/e2e/out/`, `extension/e2e/.vscode-test/`, and any fixture repo paths under
  `os.tmpdir()` must stay untracked.
- Commit after every task.

## File Structure

| Path | Responsibility |
| --- | --- |
| `extension/src/extension.ts` | Modified: treat `vscode.ExtensionMode.Test` the same as `Development` for both webview asset root and sidecar path resolution, so a test-electron-launched host reads dev-mode assets instead of nonexistent packaged ones. |
| `extension/src/extension.asset-root.test.ts` | Modified: pins the `Test`-mode case alongside the existing `Development`/`Production` cases. |
| `extension/src/sidecarBridge.test.ts` (existing) | Modified: adds the equivalent `Test`-mode pinning for sidecar path resolution, if not already covered by an existing test at that call site. |
| `extension/e2e/package.json`, `extension/e2e/pnpm-lock.yaml` | New standalone pnpm package: `@vscode/test-electron`, `playwright-core`, `mocha`, `@types/mocha`, `@types/node`, `typescript`. |
| `extension/e2e/tsconfig.json` | Compiles `extension/e2e/src/**/*.ts` to `extension/e2e/out/`, since `@vscode/test-electron` needs a compiled `extensionTestsPath` module. |
| `extension/e2e/src/runTests.ts` | Outer harness (invoked by `pnpm test`): builds the fixture repo, resolves the dev sidecar/webview build paths, calls `@vscode/test-electron`'s `runTests()` with `--remote-debugging-port`. |
| `extension/e2e/src/suite/index.ts` | The `extensionTestsPath` entry `@vscode/test-electron` loads inside the Extension Development Host; globs and runs `specs/**/*.spec.js` under Mocha. |
| `extension/e2e/src/support/webviewPage.ts` | `connectToWebview(cdpEndpoint): Promise<Page>` — attaches `playwright-core` over CDP and returns the `vscode-webview://…` page. |
| `extension/e2e/src/specs/first-flow.spec.ts` | The one flow: open the panel via the `browsitory.open` command, then drive the webview page's DOM the same way `e2e/specs/first-flow.spec.ts` does. |
| `.gitignore` | Adds `extension/e2e/out`, `extension/e2e/.vscode-test`, `extension/e2e/node_modules`. |
| `.github/workflows/ci.yml` | New `e2e-vscode` job parallel to `e2e`; added to the `needs:` list of `build-vsix` and `build-release`. |
| `docs/LICENSE_COMPLIANCE.md` | Records `@vscode/test-electron` and `playwright-core`. |
| `docs/tasks/phase-6/d-01-extension-mode-test-handling.md`, `d-02-vscode-e2e-harness.md`, `d-03-vscode-e2e-ci.md` | Task briefs following `docs/TASK_TEMPLATE.md`. |
| `docs/ARCHITECTURE.md`, `CHANGELOG.md` | Documents the new E2E layer; marks Phase 6 fully complete. |

## Task 1: Treat `ExtensionMode.Test` as development-mode for asset/sidecar resolution

**Files:**
- Create: `docs/tasks/phase-6/d-01-extension-mode-test-handling.md`
- Modify: `extension/src/extension.ts:15-22,47-50`, `extension/src/extension.asset-root.test.ts`

**Interfaces:**
- Consumes: `vscode.ExtensionMode` (`Production = 1`, `Development = 2`, `Test = 3` — real
  values from the `vscode` module; the current test file's mock only defines the first two and
  must be extended).
- Produces: `resolveWebviewAssetRoot(extensionUri, mode)` and the sidecar `executablePath`
  ternary in `activate()` both route `Test` through the same branch as `Development`.

- [ ] **Step 1: Write the failing test for `Test` mode.**

  In `extension/src/extension.asset-root.test.ts`, extend the `vscode` mock's `ExtensionMode` to
  `{ Development: 1, Test: 3, Production: 2 }` and add a case:

  ```ts
  expect(extension.resolveWebviewAssetRoot(uri, 3 as ExtensionMode).fsPath).toBe(
    "/workspace/extension/../frontend/dist-vscode",
  );
  ```

- [ ] **Step 2: Run the test and confirm it fails.**

  Run: `cd extension && pnpm test -- --run src/extension.asset-root.test.ts`

  Expected: FAIL — `resolveWebviewAssetRoot` currently treats anything other than `Development`
  as production, so mode `3` resolves to `.../extension/webview` instead.

- [ ] **Step 3: Fix `resolveWebviewAssetRoot` and the sidecar path ternary.**

  In `extension/src/extension.ts`, change both checks from `mode === vscode.ExtensionMode.Development`
  to `mode !== vscode.ExtensionMode.Production` (equivalently, `!== Production` reads as "anything
  but a real installed/packaged run is dev-mode assets" — `Test` and `Development` both qualify):

  ```ts
  export function resolveWebviewAssetRoot(
    extensionUri: vscode.Uri,
    mode: vscode.ExtensionMode,
  ): vscode.Uri {
    return mode !== vscode.ExtensionMode.Production
      ? vscode.Uri.joinPath(extensionUri, "..", "frontend", "dist-vscode")
      : vscode.Uri.joinPath(extensionUri, "webview");
  }
  ```

  And in `activate()`:

  ```ts
  const executablePath =
    context.extensionMode !== vscode.ExtensionMode.Production
      ? resolveDevelopmentSidecarPath(context.extensionUri.fsPath)
      : resolvePackagedSidecarPath(context.extensionUri.fsPath);
  ```

- [ ] **Step 4: Run the test and confirm it passes; run the full extension suite.**

  Run: `cd extension && pnpm test -- --run && pnpm run compile && pnpm run lint`

  Expected: PASS.

- [ ] **Step 5: Write the task brief and commit.**

  `docs/tasks/phase-6/d-01-extension-mode-test-handling.md` follows `docs/TASK_TEMPLATE.md`:
  goal (make `@vscode/test-electron`'s `ExtensionMode.Test` resolve dev-mode assets so Task 2's
  harness can launch against an unpacked extension), depends on none, TDD requirement matches
  Step 1, acceptance criteria matches Step 4, out of scope: the E2E harness itself (Task 2).

  Commit: `fix(extension): resolve dev-mode assets under ExtensionMode.Test`

## Task 2: Scaffold `extension/e2e/` and connect Playwright to the webview over CDP

**Files:**
- Create: `extension/e2e/package.json`, `extension/e2e/pnpm-lock.yaml`, `extension/e2e/tsconfig.json`,
  `extension/e2e/src/runTests.ts`, `extension/e2e/src/suite/index.ts`,
  `extension/e2e/src/support/webviewPage.ts`, `docs/tasks/phase-6/d-02-vscode-e2e-harness.md`
- Modify: `.gitignore`, `docs/LICENSE_COMPLIANCE.md`

**Interfaces:**
- Consumes: the dev sidecar binary at `target/debug/vscode-sidecar[.exe]` (built by
  `cargo build --workspace`), `frontend/dist-vscode` (built with `VITE_E2E_REPO_PATH` baked in,
  same as `e2e/`'s frontend build step), `extension/dist` (built by `pnpm --dir extension run compile`).
- Produces: `connectToWebview(cdpHttpUrl: string): Promise<import("playwright-core").Page>` —
  resolves once a page whose URL starts with `vscode-webview://` appears among the attached
  browser's pages, polling with a timeout since the panel opens asynchronously after the
  `browsitory.open` command runs.

- [ ] **Step 1: Add the package manifest and lockfile.**

  Deliberately no `"type": "module"` field (defaults to CommonJS) — `suite/index.ts` below is
  loaded via `require()` by `@vscode/test-electron`'s extension-host machinery, which requires a
  CommonJS module, and the `tsconfig.json` below compiles everything in this package to
  CommonJS, so `runTests.ts` must be CommonJS too rather than mixing module systems within one
  package.

  ```json
  {
    "name": "browsitory-vscode-e2e",
    "private": true,
    "packageManager": "pnpm@9.15.9",
    "scripts": {
      "test": "tsc -p tsconfig.json && node out/runTests.js",
      "typecheck": "tsc -p tsconfig.json --noEmit"
    },
    "devDependencies": {
      "@types/mocha": "^10",
      "@types/node": "^24",
      "@vscode/test-electron": "^2.5.2",
      "mocha": "^11",
      "playwright-core": "^1.49.0",
      "typescript": "~6.0.2"
    }
  }
  ```

  Run `pnpm install` inside `extension/e2e/` to generate `pnpm-lock.yaml` (pin the exact
  installed versions of `@vscode/test-electron`, `mocha`, and `playwright-core` into the manifest
  above once resolved, matching how `extension/package.json` pins `@vscode/vsce`).

- [ ] **Step 2: Add `tsconfig.json`.**

  ```json
  {
    "compilerOptions": {
      "target": "ES2022",
      "module": "CommonJS",
      "moduleResolution": "Node",
      "rootDir": "src",
      "outDir": "out",
      "strict": true,
      "esModuleInterop": true,
      "skipLibCheck": true,
      "types": ["node", "mocha"]
    },
    "include": ["src/**/*.ts"]
  }
  ```

  `@vscode/test-electron`'s `extensionTestsPath` loads a CommonJS module via Node's `require`
  inside the Extension Development Host process, so this package compiles to CommonJS rather
  than following `extension/`'s own `Node16`/ESM config.

- [ ] **Step 3: Write `webviewPage.ts`.**

  ```ts
  import { chromium, type Page } from "playwright-core";

  export async function connectToWebview(
    cdpHttpUrl: string,
    timeoutMs = 15000,
  ): Promise<Page> {
    const browser = await chromium.connectOverCDP(cdpHttpUrl);
    const deadline = Date.now() + timeoutMs;
    while (Date.now() < deadline) {
      for (const context of browser.contexts()) {
        for (const page of context.pages()) {
          if (page.url().startsWith("vscode-webview://")) return page;
        }
      }
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
    throw new Error(`No vscode-webview:// page found within ${timeoutMs}ms`);
  }
  ```

- [ ] **Step 4: Write `suite/index.ts`, the `extensionTestsPath` entry.**

  ```ts
  import path from "node:path";
  import Mocha from "mocha";
  import { globSync } from "node:fs";

  export function run(): Promise<void> {
    const mocha = new Mocha({ ui: "bdd", timeout: 60000, color: true });
    const specsRoot = path.resolve(__dirname, "..", "specs");
    const files = globSync("**/*.spec.js", { cwd: specsRoot });
    for (const file of files) mocha.addFile(path.join(specsRoot, file));

    return new Promise((resolve, reject) => {
      mocha.run((failures) => {
        if (failures > 0) reject(new Error(`${failures} test(s) failed`));
        else resolve();
      });
    });
  }
  ```

  (`node:fs`'s `globSync` requires Node 22+; the repo already targets Node 24 — see the
  `Tech Stack` line above and `.github/workflows/ci.yml`'s existing `node-version: 24`.)

- [ ] **Step 5: Write `runTests.ts`, the outer harness.**

  ```ts
  import { execFileSync } from "node:child_process";
  import fs from "node:fs";
  import os from "node:os";
  import path from "node:path";
  import { runTests } from "@vscode/test-electron";

  // Plain CommonJS `__dirname` (this package compiles to CommonJS — see package.json's note
  // above) rather than the `fileURLToPath(import.meta.url)` ESM idiom.
  const extensionDevelopmentPath = path.resolve(__dirname, "..", "..");
  const extensionTestsPath = path.resolve(__dirname, "suite", "index.js");
  const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-vscode-e2e-repo");
  const CDP_PORT = 9229;

  function setupFixtureRepo() {
    fs.rmSync(E2E_REPO_PATH, { recursive: true, force: true });
    fs.mkdirSync(E2E_REPO_PATH, { recursive: true });
    execFileSync("git", ["init"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["config", "user.name", "Test User"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["config", "user.email", "test@example.com"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    fs.writeFileSync(path.join(E2E_REPO_PATH, "README.md"), "vscode e2e fixture repo\n");
    execFileSync("git", ["add", "README.md"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: base commit"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
  }

  async function main() {
    setupFixtureRepo();
    process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"] = String(CDP_PORT);
    await runTests({
      extensionDevelopmentPath,
      extensionTestsPath,
      launchArgs: [
        E2E_REPO_PATH,
        `--remote-debugging-port=${CDP_PORT}`,
        "--disable-workspace-trust",
      ],
    });
  }

  main().catch((error) => {
    console.error(error);
    process.exit(1);
  });
  ```

  `E2E_REPO_PATH` is passed as a launch arg (a folder path) so VSCode opens it as the workspace
  folder, same rationale `e2e/wdio.conf.ts` documents for needing the fixture repo to exist
  before the app/host launches — `first-flow-fixture.txt`-style per-spec fixture files are added
  in a Mocha `before` hook instead, in Task 3.

- [ ] **Step 6: Add license compliance entries and update `.gitignore`.**

  Run `npm info @vscode/test-electron@<resolved version> license` and
  `npm info playwright-core@<resolved version> license`; add both to a new
  `## JavaScript, \`extension/e2e/\`` table section in `docs/LICENSE_COMPLIANCE.md`, following the
  existing `## JavaScript, \`extension/\`` section's format.

  Add to `.gitignore`:
  ```
  extension/e2e/node_modules
  extension/e2e/out
  extension/e2e/.vscode-test
  ```

- [ ] **Step 7: Write the task brief and commit.**

  `docs/tasks/phase-6/d-02-vscode-e2e-harness.md`: goal (scaffold the harness and CDP webview
  connector), depends on Task 6.D.01, TDD requirement (Task 3's spec is the first real assertion
  against this scaffold — this task has no standalone test of its own beyond `tsc`/`pnpm typecheck`
  passing), acceptance criteria (`pnpm --dir extension/e2e typecheck` passes), out of scope: the
  actual first-flow spec (Task 3).

  Commit: `feat(extension): scaffold vscode-electron e2e harness`

## Task 3: Write the first-flow E2E spec

**Files:**
- Create: `extension/e2e/src/specs/first-flow.spec.ts`
- Modify: none (verification-only task against Task 2's scaffold)

**Interfaces:**
- Consumes: `connectToWebview` from Task 2, the `vscode` module (available ambiently inside the
  Extension Development Host process Mocha runs in — no `@types/vscode` import needed for runtime
  use, though add `@types/vscode` as a devDependency for typechecking the `vscode.commands`
  calls), `process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"]` set by `runTests.ts`.
- Produces: nothing further downstream — this is the leaf spec.

- [ ] **Step 1: Add `@types/vscode` to `extension/e2e/package.json` devDependencies.**

  Pin it to the same `^1.134.0` version `extension/package.json` already uses. Run
  `pnpm --dir extension/e2e install` and record it in `docs/LICENSE_COMPLIANCE.md`'s new
  `extension/e2e/` table (MIT — same package already verified for `extension/`, cite that prior
  verification rather than re-running `npm info`).

- [ ] **Step 2: Write the spec.**

  ```ts
  import * as vscode from "vscode";
  import { connectToWebview } from "../support/webviewPage";

  describe("Browsitory VSCode extension first flow", () => {
    it("opens a repo, stages a file, commits, and sees it in history", async function () {
      this.timeout(30000);

      const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
      if (!workspaceFolder) throw new Error("no workspace folder open");
      const fixtureFile = vscode.Uri.joinPath(workspaceFolder.uri, "first-flow-fixture.txt");
      await vscode.workspace.fs.writeFile(fixtureFile, Buffer.from("first flow\n"));

      await vscode.commands.executeCommand("browsitory.open");

      const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
      if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
      const page = await connectToWebview(`http://127.0.0.1:${cdpPort}`);

      const stageButton = page.locator('button[aria-label="Stage first-flow-fixture.txt"]');
      await stageButton.waitFor({ state: "attached", timeout: 10000 });
      await stageButton.evaluate((el) => (el as HTMLElement).click());

      const commitMessageInput = page.locator("textarea[placeholder='Commit message']");
      await commitMessageInput.fill("e2e: first commit");
      await page.locator("button", { hasText: "Commit" }).click();

      const historyEntry = page.locator("li", { hasText: "e2e: first commit" });
      await historyEntry.waitFor({ state: "attached", timeout: 10000 });
    });
  });
  ```

  The `evaluate(... .click())` indirection for the stage button mirrors
  `e2e/specs/first-flow.spec.ts`'s own comment: the control is `opacity: 0` until its row is
  hovered/focused, and Playwright's own actionability check (like WebdriverIO's) rejects a
  fully transparent element for a plain `.click()`.

- [ ] **Step 3: Build the harness's dependencies and run the suite.**

  Run:
  ```bash
  cargo build --workspace
  cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-vscode-e2e-repo pnpm build && pnpm exec vite build --config vite.vscode.config.ts
  cd ../extension && pnpm run compile
  cd e2e && pnpm install && xvfb-run --auto-servernum pnpm test
  ```

  Expected: PASS — one passing Mocha test, VSCode window opens headlessly under Xvfb, the
  webview page is found over CDP, and the commit shows up in history.

- [ ] **Step 4: Write the task brief and commit.**

  `docs/tasks/phase-6/d-02-vscode-e2e-harness.md` already covers the scaffold; fold this spec
  into the same brief's acceptance criteria rather than creating a new file (this task is the
  scaffold's first real consumer, not an independent workstream item).

  Commit: `test(extension): add vscode e2e first-flow spec`

## Task 4: Wire CI and finish documentation

**Files:**
- Create: `docs/tasks/phase-6/d-03-vscode-e2e-ci.md`
- Modify: `.github/workflows/ci.yml`, `docs/ARCHITECTURE.md`, `CHANGELOG.md`

**Interfaces:**
- Consumes: Task 2/3's `extension/e2e/` package, the existing `rust`/`frontend`/`extension` CI
  jobs' build outputs.
- Produces: an `e2e-vscode` CI job; `build-vsix` and `build-release` gain it in their `needs:`.

- [ ] **Step 1: Write the CI contract as a task brief before editing.**

  `docs/tasks/phase-6/d-03-vscode-e2e-ci.md`: require a Linux job that builds the dev sidecar,
  `frontend/dist-vscode` (with `VITE_E2E_REPO_PATH` baked in), and the compiled extension, then
  runs `extension/e2e`'s suite under `xvfb-run`; require `build-vsix` and `build-release` to
  depend on it passing, same as they already depend on `e2e`.

- [ ] **Step 2: Add the `e2e-vscode` job to `ci.yml`.**

  Add, parallel to the existing `e2e` job (`.github/workflows/ci.yml:232`):

  ```yaml
  e2e-vscode:
    runs-on: ubuntu-latest
    env:
      VSCODE_E2E_REPO_PATH: /tmp/browsitory-vscode-e2e-repo
    steps:
      - uses: actions/checkout@v7
      - name: Install Xvfb
        run: sudo apt-get update && sudo apt-get install -y xvfb
      - uses: dtolnay/rust-toolchain@stable
      - uses: pnpm/action-setup@v6
        with:
          version: 9
      - uses: actions/setup-node@v7
        with:
          node-version: 24
          cache: pnpm
          cache-dependency-path: |
            frontend/pnpm-lock.yaml
            extension/pnpm-lock.yaml
            extension/e2e/pnpm-lock.yaml
      - name: Build dev sidecar
        run: cargo build --workspace
      - name: Build vscode frontend (with E2E fixture auto-open baked in)
        working-directory: frontend
        run: |
          pnpm install --frozen-lockfile
          VITE_E2E_REPO_PATH="$VSCODE_E2E_REPO_PATH" pnpm exec vite build --config vite.vscode.config.ts
      - name: Compile extension
        working-directory: extension
        run: |
          pnpm install --frozen-lockfile
          pnpm run compile
      - name: Install e2e dependencies
        working-directory: extension/e2e
        run: pnpm install --frozen-lockfile
      - name: Run VSCode E2E suite
        working-directory: extension/e2e
        run: xvfb-run --auto-servernum pnpm test
  ```

- [ ] **Step 3: Gate the packaging/release jobs on it.**

  Add `e2e-vscode` to the `needs:` array at `.github/workflows/ci.yml:135` (`build-vsix`) and
  `:180` (`build-release`), alongside the existing `rust`, `frontend`, `extension`, `e2e` entries.

- [ ] **Step 4: Verify workflow syntax locally, then run the full local sequence.**

  Run the same commands as Task 3 Step 3 end-to-end once more to confirm nothing regressed from
  the CI wiring changes (no CI-only logic was introduced — the job is a straight port of the
  local sequence).

- [ ] **Step 5: Update documentation.**

  In `docs/ARCHITECTURE.md`, replace any note that the VSCode extension lacks E2E coverage with
  a short description of `extension/e2e/`'s `@vscode/test-electron` + Playwright-over-CDP
  approach and its one flow, cross-referencing `e2e/`'s existing pattern. Add an `## [Unreleased]`
  `### Added` entry to `CHANGELOG.md` noting the new VSCode E2E layer. If `docs/ARCHITECTURE.md`
  currently states Phase 6 is "sub-phases c-d complete", update it to reflect sub-phase (e) also
  landing — check the current wording at `docs/ARCHITECTURE.md:195` before editing, since Phase 6
  may be fully complete after this plan lands.

- [ ] **Step 6: Final verification and commit.**

  Run:
  ```bash
  cargo build --workspace && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check
  cd frontend && pnpm lint && pnpm test -- --run && pnpm build && pnpm exec vite build --config vite.vscode.config.ts
  cd ../extension && pnpm install --frozen-lockfile && pnpm test -- --run && pnpm run compile && pnpm run lint
  cd e2e && pnpm install --frozen-lockfile && xvfb-run --auto-servernum pnpm test
  cd ../extension/e2e && pnpm install --frozen-lockfile && xvfb-run --auto-servernum pnpm test
  python3 ../../scripts/check-license-compliance.py
  ```

  Commit: `ci: run vscode extension e2e suite`

## Final Verification

- [ ] `cargo build --workspace && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
- [ ] `cd frontend && pnpm lint && pnpm test -- --run && pnpm build && pnpm exec vite build --config vite.vscode.config.ts`
- [ ] `cd extension && pnpm install --frozen-lockfile && pnpm test -- --run && pnpm run compile && pnpm run lint`
- [ ] `cd extension/e2e && pnpm install --frozen-lockfile && xvfb-run --auto-servernum pnpm test` passes locally
- [ ] `python3 scripts/check-license-compliance.py`
- [ ] `.github/workflows/ci.yml`'s `e2e-vscode` job is syntactically valid and gates `build-vsix`/`build-release`

## Self-Review

- **Spec coverage:** implements the spec's Testing section in full ("New E2E layer under
  `extension/e2e/`, using `@vscode/test-electron` to drive the packaged extension inside a real
  VSCode instance... starting with open repo → stage a file → commit → see it in history"). The
  spec doesn't specify webview DOM automation mechanism (test-electron alone can't reach it);
  Task 2/3 fill that gap with Playwright-over-CDP, confirmed with the user during brainstorming
  as the option matching the spec's "mirroring e2e/specs/'s existing first flow" black-box intent.
  The spec's "packaged extension" wording is satisfied in spirit but not literally — Task 2
  deliberately tests the unpacked dev-mode extension instead of a `.vsix`, matching how `e2e/`
  itself tests the unpacked `tauri-app` debug binary rather than a distributable installer; this
  was confirmed with the user as the intended reading, not a spec deviation worth a written
  spec amendment.
- **Placeholder scan:** none — every step has runnable code or an exact command.
- **Type consistency:** `connectToWebview(cdpHttpUrl: string): Promise<Page>` (Task 2) is called
  identically in Task 3's spec; `runTests.ts`'s `BROWSITORY_VSCODE_E2E_CDP_PORT` env var name
  matches the spec's `process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"]` read exactly.
