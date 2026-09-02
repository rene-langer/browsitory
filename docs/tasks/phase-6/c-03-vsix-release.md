# Task 6.C.03: Release VSIX attachments

## Goal

Attach the four target-specific VSIX packages to the existing tag-triggered draft GitHub Release,
with the extension manifest version derived from the release tag.

## Depends on

Task 6.C.02 (CI VSIX artifact matrix).

## TDD requirement

The extension package and staging tests from Task 6.C.01 must pass before the release workflow
is added. The release workflow reuses the CI package command and is validated by YAML parsing;
GitHub Actions release publication has no repository-local test harness.

## Acceptance criteria

- [ ] A `release/MAJOR.MINOR.PATCH` tag creates four target-specific VSIX archives whose manifest
  version is `MAJOR.MINOR.PATCH`.
- [ ] VSIX artifact build jobs use the matching native sidecar target and executable name.
- [ ] The publish job waits for both desktop bundles and VSIX builds, then attaches all VSIX files
  to the draft GitHub Release.

## Out of scope

VSCode Marketplace publishing, signing, and Phase 6e Electron E2E coverage.
