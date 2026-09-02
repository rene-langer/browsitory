# Task 6.C.02: CI VSIX artifact matrix

## Goal

Build and retain target-specific VSIX artifacts on every push to `main`, so each supported VSCode
platform has a package containing its matching release sidecar.

## Depends on

Task 6.C.01 (target-specific VSIX packager).

## TDD requirement

The packager tests from Task 6.C.01 must pass before the workflow is added. The workflow must
invoke that same package command with one explicit target, sidecar, and output path per matrix
entry; GitHub Actions YAML itself has no repository test harness.

## Acceptance criteria

- [ ] A dedicated extension job installs with the extension lockfile and runs tests, compile, and lint.
- [ ] Main-branch CI produces four seven-day VSIX artifacts: Linux x64, macOS x64, macOS arm64,
  and Windows x64.
- [ ] Each matrix entry builds `vscode-sidecar` for the same Rust target passed to `vsce`.
- [ ] The Windows entry packages `vscode-sidecar.exe`; all other entries package `vscode-sidecar`.

## Out of scope

Tag-release uploads, Marketplace publication, and Electron E2E coverage.
