# Task 6.D.03: Wire the VSCode E2E harness into CI

## Goal

Add a Linux `e2e-vscode` CI job that builds the dev sidecar, `frontend/dist-vscode` (with
`VITE_E2E_REPO_PATH` baked in), and the compiled extension, then runs `extension/e2e`'s suite
under `xvfb-run`; gate `build-vsix` on it passing, the same way it already gates on `e2e`.

## Depends on

Task 6.D.02 (the `extension/e2e/` harness itself — raw-CDP webview connection, first-flow spec,
confirmed passing reliably across multiple independent verification rounds).

## TDD requirement

None of this task's own — it wires an existing, already-passing local suite into CI. The bar is
that the new job is a straight port of the local command sequence (no CI-only logic) and that
`.github/workflows/ci.yml` stays syntactically valid.

## Acceptance criteria

- [x] `.github/workflows/ci.yml` gains an `e2e-vscode` job, parallel to `e2e`: installs Xvfb,
  builds the dev sidecar (`cargo build --workspace`), builds `frontend/dist-vscode` via
  `vite build --config vite.vscode.config.ts` with `VITE_E2E_REPO_PATH` set to a fixed fixture
  path, compiles the extension, installs `extension/e2e`'s dependencies from its own lockfile,
  and runs `xvfb-run --auto-servernum pnpm test`.
- [x] `build-vsix`'s `needs:` gains `e2e-vscode`, alongside `rust`, `frontend`, `extension`, `e2e`.
- [x] A comment on the E2E step documents the known `dconf watch /system/proxy/` CDP-port-squat
  limitation (mitigated, not eliminated, by `extension/e2e/src/runTests.ts`) and why it isn't a
  live problem on GitHub-hosted `ubuntu-latest` runners (fresh VM per job, so there's no
  leftover process from a prior run to squat this run's port).
- [x] `scripts/check-license-compliance.py` also scans `extension/e2e/package.json` against the
  `## JavaScript, \`extension/e2e/\`` table in `docs/LICENSE_COMPLIANCE.md` (that table already
  existed from Task 6.D.02; only the script enforcement was missing).
- [x] `docs/ARCHITECTURE.md` describes `extension/e2e/`'s actual mechanism — `@vscode/test-electron`
  driving a real Extension Development Host, plus a hand-rolled raw-CDP client (not Playwright)
  for the webview's nested content frame — cross-referencing `e2e/`'s existing WebdriverIO
  pattern, and the Phase 6 roadmap entry reflects sub-phase (e) landing.
- [x] `CHANGELOG.md`'s `## [Unreleased]` gets an `### Added` entry describing the new VSCode E2E
  layer accurately (raw-CDP based, not Playwright-based).

## Out of scope

Any change to `extension/e2e/src/**` (the harness itself is done as of Task 6.D.02) or to
`.github/workflows/release.yml` (VSIX release automation is a separate, already-completed prior
phase — not part of this plan).
