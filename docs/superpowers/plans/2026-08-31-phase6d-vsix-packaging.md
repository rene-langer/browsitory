# Phase 6d VSIX Packaging and CI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce platform-specific Browsitory VSIX packages that each contain the matching release-mode `vscode-sidecar` binary, and build/upload them in CI and release automation.

**Architecture:** A small Node packaging script stages the shared webview bundle and one already-built native sidecar into ignored extension directories, invokes VS Code's package tool for exactly one supported platform target, then removes the staged inputs. The extension host resolves the sibling frontend bundle only in development; installed VSIX packages instead read their bundled `webview/` and `bin/` directories. CI builds the four host/target combinations natively and uploads their VSIX files; tag releases attach the same four outputs to the draft GitHub Release after the existing Tauri artifacts are built.

**Tech Stack:** Node.js 24, pnpm 11, TypeScript, Node built-in test runner, Rust/Cargo, `@vscode/vsce` 3.9.2, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-08-30-vscode-extension-design.md`

## Global Constraints

- Desktop VSCode only; vscode.dev, Codespaces, Marketplace publishing, and `@vscode/test-electron` E2E are out of scope.
- Package exactly these platform/sidecar pairs: `linux-x64`/`x86_64-unknown-linux-gnu`, `darwin-x64`/`x86_64-apple-darwin`, `darwin-arm64`/`aarch64-apple-darwin`, and `win32-x64`/`x86_64-pc-windows-msvc`.
- Each VSIX includes only its matching `bin/vscode-sidecar` (`.exe` on Windows), never all four binaries.
- Development mode continues using `../target/debug/vscode-sidecar` and `../frontend/dist-vscode`; production mode uses only package-local `bin/` and `webview/` assets.
- Keep `RepoClient`, JSON-RPC protocol, bridge lifecycle, and the Tauri distributable pipeline unchanged.
- Add `@vscode/vsce` only as an extension development dependency, verify its MIT license with `npm info`, and record it in `docs/LICENSE_COMPLIANCE.md` before committing.
- Generated `extension/bin/`, `extension/webview/`, `extension/*.vsix`, and CI artifact directories must stay untracked and be removed by the packaging script even when packaging fails.
- Commit after every task.

## File Structure

| Path | Responsibility |
| --- | --- |
| `extension/scripts/package-vsix.mjs` | Validates a VS Code platform target, stages one sidecar and the webview bundle, invokes local `vsce`, and cleans generated staging paths. |
| `extension/scripts/package-vsix.test.mjs` | Node built-in tests for validation, staged contents, output naming, and cleanup on success/failure. |
| `extension/package.json`, `extension/pnpm-lock.yaml` | Declares the packager, strictly lists distributable files, and exposes deterministic package scripts. |
| `extension/src/extension.ts`, `extension/src/extension.test.ts` | Resolves asset roots differently for development versus an installed VSIX. |
| `extension/README.md`, `extension/LICENSE` | Supplies the package-local metadata and license required by a distributable VSIX. |
| `.gitignore` | Excludes all transient VSIX staging/output paths. |
| `scripts/check-license-compliance.py`, `docs/LICENSE_COMPLIANCE.md` | Makes the existing dependency ledger check cover the extension package and records `@vscode/vsce`. |
| `.github/workflows/ci.yml` | Runs extension gates on PRs and produces four short-retention VSIX artifacts from `main`. |
| `.github/workflows/release.yml` | Builds four tag-versioned VSIX artifacts and attaches them to the existing draft release. |
| `docs/tasks/phase-6/c-01-vsix-packager.md`, `c-02-vsix-ci.md`, `c-03-vsix-release.md` | Implementation task briefs following `docs/TASK_TEMPLATE.md`. |
| `docs/ARCHITECTURE.md`, `CHANGELOG.md` | Documents the installed-extension asset layout and delivered Phase 6d distribution surface. |

### Task 1: Make a clean, target-specific VSIX packager

**Files:**
- Create: `extension/scripts/package-vsix.mjs`, `extension/scripts/package-vsix.test.mjs`, `extension/README.md`, `extension/LICENSE`, `docs/tasks/phase-6/c-01-vsix-packager.md`
- Modify: `extension/package.json`, `extension/pnpm-lock.yaml`, `extension/src/extension.ts`, `extension/src/extension.test.ts`, `.gitignore`, `scripts/check-license-compliance.py`, `docs/LICENSE_COMPLIANCE.md`

**Interfaces:**
- Consumes: `--target <linux-x64|darwin-x64|darwin-arm64|win32-x64>`, `--sidecar <absolute binary path>`, `--out <absolute VSIX path>`, `frontend/dist-vscode/`, and the package's local `@vscode/vsce` executable.
- Produces: `packageVsix(options): Promise<string>` and `resolveWebviewAssetRoot(extensionUri, mode): vscode.Uri`; a valid VSIX containing `extension/dist/`, `extension/webview/`, `extension/bin/vscode-sidecar[.exe]`, `extension/README.md`, `extension/LICENSE`, and `extension/package.json` only.

- [ ] **Step 1: Write failing packager and installed-layout tests.**

  Create `extension/scripts/package-vsix.test.mjs` using `node:test` and a temporary directory. Inject a fake command runner so the test can inspect the staging tree at package time, then assert that a Linux invocation passes exactly `package --target linux-x64 --out <out>` to `pnpm exec vsce`, stages `bin/vscode-sidecar` and every `frontend/dist-vscode` file below `webview/`, and removes both staging directories afterwards. Add failure cases for a missing sidecar, an unsupported target, and a rejecting runner; each must leave no `bin/` or `webview/` directory. In `extension/src/extension.test.ts`, assert that development maps an extension root to `../frontend/dist-vscode`, while production maps it to `<extension root>/webview`.

  ```js
  await packageVsix({
    extensionRoot,
    frontendDist,
    sidecarPath,
    target: "linux-x64",
    outputPath,
    run,
  });
  assert.deepEqual(seenArgs, ["exec", "vsce", "package", "--target", "linux-x64", "--out", outputPath]);
  assert.equal(await readFile(path.join(extensionRoot, "bin", "vscode-sidecar"), "utf8"), "sidecar");
  await assert.rejects(() => packageVsix({ ...options, target: "linux-arm64" }), /unsupported VSIX target/);
  assert.equal(existsSync(path.join(extensionRoot, "bin")), false);
  ```

- [ ] **Step 2: Run the focused tests and confirm they fail.**

  Run: `cd extension && node --test scripts/package-vsix.test.mjs && pnpm test -- --run src/extension.test.ts`

  Expected: FAIL because neither the packager nor `resolveWebviewAssetRoot` exists.

- [ ] **Step 3: Add the production asset-root seam and package manifest.**

  Extract the webview asset-root decision from `activate` into this exported helper and use it for `localResourceRoots`, script, and stylesheet URIs:

  ```ts
  export function resolveWebviewAssetRoot(
    extensionUri: vscode.Uri,
    mode: vscode.ExtensionMode,
  ): vscode.Uri {
    return mode === vscode.ExtensionMode.Development
      ? vscode.Uri.joinPath(extensionUri, "..", "frontend", "dist-vscode")
      : vscode.Uri.joinPath(extensionUri, "webview");
  }
  ```

  Add `@vscode/vsce: "3.9.2"` to `devDependencies`; add `package:vsix` as `node scripts/package-vsix.mjs`; and set the package's `files` allowlist to `dist/**`, `bin/**`, `webview/**`, `README.md`, and `LICENSE`. Create a concise package-local README with desktop-only installation (`code --install-extension <file>.vsix`) and a copy of the repository MIT `LICENSE`. Ignore `.vsix`, `bin/`, `webview/`, and `artifacts/` below `extension/`.

- [ ] **Step 4: Implement deterministic staging and cleanup.**

  Parse the three required arguments, reject targets outside the four-item map, and use `fs.cp` to stage the webview and `fs.copyFile` to stage the supplied release binary under `bin/${platform === "win32" ? "vscode-sidecar.exe" : "vscode-sidecar"}`. Invoke only the extension-local packager, never a globally installed `vsce`; execute cleanup in `finally`.

  ```js
  const supportedTargets = new Set(["linux-x64", "darwin-x64", "darwin-arm64", "win32-x64"]);
  try {
    await fs.cp(frontendDist, path.join(extensionRoot, "webview"), { recursive: true });
    await fs.mkdir(path.join(extensionRoot, "bin"), { recursive: true });
    await fs.copyFile(sidecarPath, stagedSidecarPath);
    await run("pnpm", ["exec", "vsce", "package", "--target", target, "--out", outputPath], extensionRoot);
  } finally {
    await Promise.all([fs.rm(path.join(extensionRoot, "bin"), { recursive: true, force: true }), fs.rm(path.join(extensionRoot, "webview"), { recursive: true, force: true })]);
  }
  ```

- [ ] **Step 5: Extend dependency compliance and record the approved dependency.**

  Change `scripts/check-license-compliance.py` to collect `extension/package.json` with the same `js_deps` helper and compare it to a dedicated `## JavaScript, \`extension/\`` table section. Add `@vscode/vsce 3.9.2 | MIT | dev only; packages target-specific VSIX artifacts` to that section, after verifying with `npm info @vscode/vsce@3.9.2 license`. Regenerate `extension/pnpm-lock.yaml` with a frozen-lockfile-compatible pnpm install.

- [ ] **Step 6: Run focused verification and commit.**

  Run: `python3 scripts/check-license-compliance.py && cd extension && pnpm install --frozen-lockfile && node --test scripts/package-vsix.test.mjs && pnpm test -- --run src/extension.test.ts && pnpm run compile && pnpm run lint`

  Commit: `feat(extension): package target-specific vsix artifacts`

### Task 2: Add PR gates and main-branch VSIX artifacts

**Files:**
- Create: `docs/tasks/phase-6/c-02-vsix-ci.md`
- Modify: `.github/workflows/ci.yml`

**Interfaces:**
- Consumes: the Task 1 `package:vsix` CLI, native Cargo release outputs, Node 24, pnpm 11, and GitHub Actions artifact storage.
- Produces: an `extension` PR job and a `build-vsix` main-only matrix job; artifacts named `browsitory-vsix-<target>` containing exactly `browsitory-<version>-<target>.vsix`.

- [ ] **Step 1: Write the CI contract as a workflow review checklist before editing.**

  Add the task brief that requires every pull request to run the extension's frozen install, unit tests, compiler check, lint, and package-script tests. It must also require four `main` artifact legs, one sidecar target per VSIX target, and prohibit using the macOS universal Tauri binary for either VSIX.

- [ ] **Step 2: Add the extension quality-gate job.**

  In `ci.yml`, add an Ubuntu `extension` job parallel to `frontend`. It checks out source, configures pnpm 11 and Node 24 with `extension/pnpm-lock.yaml` cache dependency, runs `pnpm install --frozen-lockfile`, `pnpm test -- --run`, `node --test scripts/package-vsix.test.mjs`, `pnpm run compile`, and `pnpm run lint` from `extension/`.

- [ ] **Step 3: Add the four-leg packaging matrix.**

  Add a `build-vsix` job gated to `refs/heads/main`, dependent on `rust`, `frontend`, `extension`, and `e2e`. Its include matrix must be:

  ```yaml
  - os: ubuntu-latest
    vsix_target: linux-x64
    rust_target: x86_64-unknown-linux-gnu
  - os: macos-latest
    vsix_target: darwin-x64
    rust_target: x86_64-apple-darwin
  - os: macos-latest
    vsix_target: darwin-arm64
    rust_target: aarch64-apple-darwin
  - os: windows-latest
    vsix_target: win32-x64
    rust_target: x86_64-pc-windows-msvc
  ```

  Each leg installs its Rust target, builds `cargo build --release -p vscode-sidecar --target ${{ matrix.rust_target }}`, builds `frontend/dist-vscode` with `pnpm exec vite build --config vite.vscode.config.ts`, compiles the extension, and invokes `pnpm run package:vsix -- --target ... --sidecar ... --out ...`. Use a PowerShell-specific sidecar/output path only where Windows path syntax requires it. Upload just that output as `browsitory-vsix-${{ matrix.vsix_target }}` for seven days.

- [ ] **Step 4: Verify workflow syntax and local package behavior.**

  Run: `cd frontend && pnpm exec vite build --config vite.vscode.config.ts`

  Run: `cargo build --release -p vscode-sidecar`

  Run: `cd extension && pnpm run compile && pnpm run package:vsix -- --target linux-x64 --sidecar ../target/release/vscode-sidecar --out artifacts/browsitory-0.1.0-linux-x64.vsix`

  Run: `unzip -l extension/artifacts/browsitory-0.1.0-linux-x64.vsix`

  Expected: one `extension/bin/vscode-sidecar`, `extension/webview/assets/vscode-main.js`, `extension/dist/extension.js`, and no `.git`, `node_modules`, source TypeScript, or non-Linux sidecar executable.

- [ ] **Step 5: Commit.**

  Commit: `ci: build platform-specific vscode extension artifacts`

### Task 3: Attach release VSIXs and document the distribution boundary

**Files:**
- Create: `docs/tasks/phase-6/c-03-vsix-release.md`
- Modify: `.github/workflows/release.yml`, `docs/ARCHITECTURE.md`, `CHANGELOG.md`

**Interfaces:**
- Consumes: a strict `release/MAJOR.MINOR.PATCH` tag, the Task 2 platform matrix, and the existing draft release created by Tauri's release job.
- Produces: four version-matched VSIX assets attached to the same draft GitHub Release as the Tauri installers.

- [ ] **Step 1: Write the release acceptance brief.**

  Require the release workflow to set `extension/package.json`'s version from the tag alongside `tauri.conf.json`, build all four VSIXs, and attach them only after `build-release` completes. State that no Marketplace upload, token, publisher registration, or automatic extension update mechanism is introduced.

- [ ] **Step 2: Add a release-only VSIX build matrix.**

  Add `build-vsix-release`, using the Task 2 target matrix and the same install/build/package sequence. Before packaging, set the extension manifest version to `${GITHUB_REF_NAME#release/}` with a Node one-liner. Upload the results as temporary workflow artifacts, retaining them until the final publish job consumes them.

- [ ] **Step 3: Attach the outputs after the Tauri release exists.**

  Add a `publish-vsix-release` Ubuntu job with `needs: [build-release, build-vsix-release]`. Download all four VSIX artifacts to one directory and use `softprops/action-gh-release` with `draft: true`, `tag_name: ${{ github.ref_name }}`, and this exact glob:

  ```yaml
  files: release-vsix/*.vsix
  ```

  This ordering avoids racing the existing `tauri-apps/tauri-action` jobs that create the draft release and keeps release permissions limited to the workflow's existing `contents: write` grant.

- [ ] **Step 4: Update delivery documentation.**

  In `docs/ARCHITECTURE.md`, replace the statement that VSIX bundling remains future work with the development/package asset-root split and the four published targets. Add an Unreleased changelog entry naming the platform-specific VSIX packages and bundled sidecar. Do not describe Phase 6e as delivered.

- [ ] **Step 5: Run final verification and commit.**

  Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`

  Run: `cd frontend && pnpm lint && pnpm test -- --run && pnpm build && pnpm exec vite build --config vite.vscode.config.ts`

  Run: `cd extension && pnpm install --frozen-lockfile && node --test scripts/package-vsix.test.mjs && pnpm test -- --run && pnpm run compile && pnpm run lint && pnpm run package:vsix -- --target linux-x64 --sidecar ../target/release/vscode-sidecar --out artifacts/browsitory-0.1.0-linux-x64.vsix`

  Run: `python3 scripts/check-license-compliance.py`

  Manual: install the generated Linux VSIX with `code --install-extension extension/artifacts/browsitory-0.1.0-linux-x64.vsix`, run **Browsitory: Open**, select a disposable repository, and verify the webview loads plus status/history are served by the package-local sidecar.

  Commit: `ci: publish vscode extension packages with releases`

## Final Verification

- [ ] `cargo build --workspace`
- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `cargo fmt --all -- --check`
- [ ] `cd frontend && pnpm lint && pnpm test -- --run && pnpm build && pnpm exec vite build --config vite.vscode.config.ts`
- [ ] `cd extension && pnpm install --frozen-lockfile && node --test scripts/package-vsix.test.mjs && pnpm test -- --run && pnpm run compile && pnpm run lint`
- [ ] A local Linux VSIX passes the archive-content check and opens in a desktop VSCode install using the packaged webview and sidecar.
- [ ] `python3 scripts/check-license-compliance.py`

## Self-Review

- Spec coverage: the tasks implement every Phase 6d requirement from the approved extension design: per-target VSIX packaging, a matching native sidecar in each package, and CI production. Phase 6e's real VSCode Electron E2E harness remains excluded.
- Placeholder scan: none.
- Type consistency: the package target names are VS Code's `--target` values; each maps once to its Cargo target. Production `extension.ts` resolves `webview/` while existing `resolvePackagedSidecarPath` continues resolving `bin/`, so the two packaged asset directories are coherent.
