# Task 6.C.01: Target-specific VSIX packager

## Goal

Package the VSCode extension with exactly one matching `vscode-sidecar` binary and the
relocatable webview bundle, while retaining development-mode asset paths and removing generated
staging directories after every package attempt.

## Depends on

Task 6.B.02 (extension host and recoverable sidecar lifecycle).

## TDD requirement

`extension/scripts/package-vsix.staging.test.mjs` must first assert that the Linux target stages
only `bin/vscode-sidecar`, copies the webview bundle below `webview/`, invokes local `vsce` with
the matching `--target`, and removes generated directories. It must assert an unsupported target
is rejected before staging. `extension/src/extension.asset-root.test.ts` must first assert that
development resolves `../frontend/dist-vscode` while production resolves package-local
`webview/`.

## Acceptance criteria

- [ ] `pnpm run package:vsix -- --target linux-x64 --sidecar <release-binary> --out <file>`
  creates an archive with one Linux sidecar, `dist/`, and `webview/` assets.
- [ ] `darwin-x64`, `darwin-arm64`, and `win32-x64` map to the matching executable name; only
  Windows uses `vscode-sidecar.exe`.
- [ ] `extension/bin/`, `extension/webview/`, generated VSIX files, and artifact directories are
  ignored and removed after packaging.
- [ ] `@vscode/vsce` is pinned as a MIT-licensed development dependency and
  `scripts/check-license-compliance.py` verifies extension dependencies.
- [ ] `pnpm install --frozen-lockfile`, package tests, extension tests, compile, lint, and the
  license checker pass.

## Out of scope

GitHub Actions artifact matrices, release uploads, Marketplace publication, and VSCode Electron
E2E coverage.
