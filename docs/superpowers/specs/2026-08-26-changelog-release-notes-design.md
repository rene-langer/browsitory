# Changelog enforcement + in-app release notes

Date: 2026-08-26

## Goal

Keep `CHANGELOG.md` up to date as a forcing function (not optional hygiene),
and surface it to end users inside the app the way VSCode shows release
notes: automatically on first launch of a new version, and on demand at any
time.

Two independent pieces, each gets its own implementation:

1. **Changelog enforcement** (bounded) — CI + local hook require
   `CHANGELOG.md` to change when code changes.
2. **In-app release notes viewer** (new subsystem) — parses the changelog at
   build time, tracks last-seen version, shows a modal.

## Part 1: Changelog enforcement

### Format

`CHANGELOG.md` at repo root, [Keep a Changelog](https://keepachangelog.com)
format:

```markdown
# Changelog

## [Unreleased]

### Added
- ...

## [0.5.0] - 2026-08-26

### Added
- ...

### Fixed
- ...
```

Subsections limited to `Added`, `Changed`, `Fixed`, `Removed` (Keep a
Changelog's fuller set — `Deprecated`, `Security` — allowed but not required).
`[Unreleased]` always exists at the top, even if empty; entries move from
`[Unreleased]` into a new versioned section as part of cutting a release
(manual step, same as the existing manual-version-bump-free tag flow —
see CLAUDE.md's Versioning & releases section. This spec does not change
that process, only adds a gate that the file gets touched).

### Local hook

New script, e.g. `scripts/hooks/pre-push`, wired via the repo's existing hook
setup (or `.git/hooks/pre-push` install step documented in CLAUDE.md if there
isn't a hook manager already). Logic:

- Diff current branch against `merge-base` with `main`.
- If the diff touches `crates/`, `frontend/src/`, or `e2e/` and does **not**
  touch `CHANGELOG.md`, block the push with a short message pointing at the
  file.
- Escape hatch: `SKIP_CHANGELOG_CHECK=1 git push`.

### CI backstop

New job in `.github/workflows/ci.yml`, runs on `pull_request`:

- Same diff logic, base ref = PR base.
- Skips the check if the PR carries a `skip-changelog` label.
- Fails the job otherwise, with a message matching the local hook's.

### Out of scope for part 1

- Automating the `[Unreleased]` → versioned-section move at release time.
- Generating `CHANGELOG.md` from commit messages (rejected in favor of
  human-edited Keep a Changelog, per existing preference for manual,
  reviewable release notes).

## Part 2: In-app release notes viewer

### Build-time changelog → JSON

New build step (e.g. `scripts/changelog-to-json.mjs`, run before/at the start
of `pnpm build` and `pnpm dev`) parses `CHANGELOG.md` and emits
`frontend/src/generated/releaseNotes.json`:

```ts
type ReleaseNotesEntry = {
  version: string;       // "0.5.0"
  date: string;           // "2026-08-26"
  sections: Partial<Record<"added" | "changed" | "fixed" | "removed", string[]>>;
};
```

Parser is line-based (headings, subheadings, bullets) — no markdown library
needed, since the input shape is fixed by Keep a Changelog. `[Unreleased]` is
skipped (never shown to end users; it's a working area, not a release).

`frontend/src/generated/` is gitignored — the JSON is derived data,
regenerated every build, never hand-edited or committed.

### Version tracking (backend)

- `crates/config`: add `last_seen_version: Option<String>` to the existing
  app-config TOML struct (same file that backs the recent-repos registry),
  with `get_last_seen_version` / `set_last_seen_version` accessors.
- New Tauri commands (`crates/tauri-app/src/commands.rs`):
  - `get_last_seen_version() -> Option<String>`
  - `set_last_seen_version(version: String)`
  - `get_app_version() -> String` (reads `AppHandle::package_info().version`
    — no new source of truth; `tauri.conf.json` already carries the version,
    including the one stamped in by the release workflow)
- These are app-level, not repo-scoped: they read/write `config` directly,
  the same way the recent-repos commands do, without going through a
  per-repo `Worker` thread.
- `RepoClient` (`frontend/src/ipc/RepoClient.ts`) gains all three methods;
  `tauriRepoClient.ts` implements them over the new commands. No
  `vscodeRepoClient.ts` implementation yet — out of scope until the VSCode
  frontend exists — but the interface addition keeps both call sites
  consistent when it does.

### Frontend: modal + trigger

- New `ReleaseNotesModal` component, built on the Phase 5 design-token
  primitives (`Panel` etc.), renders one or more `ReleaseNotesEntry` items
  grouped by version, each section (Added/Changed/Fixed/Removed) as a
  labeled bullet list.
- On `App.tsx` mount: fetch `getAppVersion()` and `getLastSeenVersion()`.
  - If they differ (including `last_seen_version === null`, i.e. first-ever
    install), open the modal.
    - First-ever install: show only the latest version's entry (avoid
      dumping the entire history on a fresh install).
    - Upgrade (had a previous last-seen version): show all entries newer
      than last-seen, newest first.
  - On modal close (either path), call `setLastSeenVersion(currentVersion)`.
- On-demand access: new "?" icon button in the main toolbar opens a
  dropdown with a "Release Notes" item; selecting it opens the same modal
  unfiltered (full history, newest first) and does **not** touch
  last-seen state.
- Error handling: any failure fetching version info or loading
  `releaseNotes.json` is logged to console and swallowed — the modal simply
  doesn't auto-open. This is a nice-to-have surface, never a blocker for
  using the app.

### Testing

- Build script: unit test (Vitest, run in Node) against a fixture
  `CHANGELOG.md`, asserting JSON shape — `[Unreleased]` excluded, empty
  sections omitted, multiple versions parsed correctly.
- `crates/config`: extend existing temp-dir TOML tests with a round-trip
  test for `last_seen_version`.
- `commands.rs`: no new tests for the three pass-through commands, per this
  repo's convention (thin commands are covered by the logic they call); no
  DTO-drift risk here since there's no enum shared with the frontend.
- Frontend: `ReleaseNotesModal` test mocking `RepoClient`, covering
  first-install (latest only), upgrade (delta since last-seen), and
  on-demand (full history).
- No new E2E spec — this isn't a major feature-area flow per the existing
  e2e scope (one flow per major feature area); unit/component coverage is
  sufficient.

## Out of scope

- Native OS menu bar integration (Help > Release Notes) — the toolbar "?"
  button covers on-demand access without the platform-specific menu API
  work.
- Automating changelog generation from commits.
- A `vscodeRepoClient.ts` implementation of the new `RepoClient` methods.
