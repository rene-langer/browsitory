# Auto-Update Design

Date: 2026-08-24

## Goal

Browsitory now publishes standalone distributables via GitHub Releases
(`.github/workflows/release.yml`, tag `release/MAJOR.MINOR.PATCH`). Add
VSCode-style auto-update: the app checks for a newer release in the background,
downloads it automatically, and prompts the user to restart to apply it.

## Approach

Use Tauri's official `tauri-plugin-updater` against a static `latest.json`
manifest published as a GitHub Release asset — no custom update server needed.
`tauri-apps/tauri-action` (already used in `release.yml`) generates that
manifest and signs every platform artifact automatically once signing secrets
are present and `plugins.updater` is configured in `tauri.conf.json`.

## Signing

Updates must be cryptographically signed so the app only installs artifacts
built by this project's CI, not an arbitrary file placed at the update URL.

- Generate a minisign keypair once: `cargo tauri signer generate -w
  ~/.tauri/browsitory.key` (or equivalent).
- Public key → `tauri.conf.json`'s `plugins.updater.pubkey`. Checked into the
  repo; it's not a secret.
- Private key contents + its password → GitHub Actions repository secrets
  `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Never
  committed.

## Release workflow changes

`.github/workflows/release.yml`'s `build-release` job:

- Add `TAURI_SIGNING_PRIVATE_KEY` / `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to the
  `tauri-apps/tauri-action` step's `env`.
- No other change needed — `tauri-action` detects `plugins.updater` in
  `tauri.conf.json` and emits a signed `latest.json` alongside the platform
  bundles as a release asset automatically.
- `releaseDraft: true` is unchanged. A draft release's assets are not visible
  at the `.../releases/latest/download/...` URL the updater polls, so the
  update only becomes visible to users once the release is manually published
  on GitHub — this is the intended review gate, not a bug to fix.

## Backend (`crates/tauri-app`)

- Add dependencies: `tauri-plugin-updater = "2"`, `tauri-plugin-process = "2"`
  (the latter provides the `relaunch()` used to apply an update).
- Register both plugins in `main.rs` alongside the existing
  `tauri-plugin-dialog`/`tauri-plugin-opener` registrations.
- `tauri.conf.json`: add a `plugins.updater` block:
  ```json
  "plugins": {
    "updater": {
      "pubkey": "<generated public key>",
      "endpoints": [
        "https://github.com/<owner>/<repo>/releases/latest/download/latest.json"
      ]
    }
  }
  ```
- No new Tauri command is needed. The updater and process plugins expose their
  own JS bindings (`@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`);
  the frontend calls those directly, the same way `tauriRepoClient.ts` is the
  sole place that imports `@tauri-apps/api` today.

## Frontend

New `frontend/src/components/UpdateBanner.tsx` (or similar), mounted once near
the app root:

- On mount: call `check()` from `@tauri-apps/plugin-updater`.
- Re-check on a timer every 6 hours while the app stays open (covers
  long-running sessions).
- If `check()` finds an update: call `update.downloadAndInstall()`
  immediately (no user prompt before download — download is automatic per
  design decision).
- Once downloaded: show a persistent, dismissable-but-reappearing banner —
  "Update v{version} ready — Restart to update" — with a button that calls
  `relaunch()` from `@tauri-apps/plugin-process`.
- Any error from `check()` or `downloadAndInstall()` (offline, no release
  published yet, network failure) is caught and swallowed — logged to console,
  no user-facing error. A failed background check is not actionable by the
  user and shouldn't interrupt them.
- This module talks to Tauri plugins directly and is **not** routed through
  `RepoClient` — it's app-lifecycle concerned, not repo-data, so it sits outside
  that interface's scope the same way window-chrome or menu code would.

## Testing

- Frontend: unit test `UpdateBanner` with `@tauri-apps/plugin-updater` and
  `@tauri-apps/plugin-process` mocked (`vi.mock`), covering: no update found →
  no banner; update found → auto-download called → banner shown after
  download resolves; restart button → `relaunch()` called; check/download
  rejection → banner stays hidden, no thrown error.
- No new Rust tests — this phase is plugin configuration and wiring, not new
  `git-core`/`Worker` logic.
- No E2E coverage — exercising a real update requires a second signed,
  published GitHub release to update *to*, which isn't reproducible in CI.
  Manual verification: publish a real `release/x.y.z`, run an older installed
  build, confirm the banner appears and restart applies it.

## Out of scope

- In-app changelog/release-notes display in the banner (VSCode shows this;
  can be added later by reading `update.body` from the `check()` result — the
  plugin already surfaces it).
- User-configurable update channel (stable-only for now; no beta channel).
- Silent/forced updates with no user action — this design always requires an
  explicit restart click.
