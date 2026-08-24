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

`frontend/eslint.config.js` bars any file under `src/components/**` or
`src/state/**` from importing `@tauri-apps/*` directly (`no-restricted-imports`,
enforced today to keep those directories on `RepoClient`). This feature is
intentionally *not* routed through `RepoClient` (it's app-lifecycle, not
repo-data — see rationale below), so it needs its own thin wrapper module,
the same shape `tauriRepoClient.ts` already gives `@tauri-apps/api`:

- **`frontend/src/ipc/updater.ts`** (new, not part of the `RepoClient`
  interface — just a plain module living alongside it): exports
  `checkForUpdate()`, `downloadAndInstallUpdate(update)`, and `relaunchApp()`,
  each a thin wrapper around `@tauri-apps/plugin-updater`'s `check()` /
  `update.downloadAndInstall()` and `@tauri-apps/plugin-process`'s
  `relaunch()`. This is the only file that imports those two plugins.
- **`frontend/src/components/UpdateBanner.tsx`** (new), mounted once near the
  app root, imports only `../ipc/updater`:
  - On mount: call `checkForUpdate()`.
  - Re-check on a timer every 6 hours while the app stays open (covers
    long-running sessions).
  - If an update is found: call `downloadAndInstallUpdate()` immediately (no
    user prompt before download — download is automatic per design decision).
  - Once downloaded: show a persistent banner — "Update v{version} ready —
    Restart to update" — with a button that calls `relaunchApp()`.
  - Any error from either call (offline, no release published yet, network
    failure) is caught and swallowed — logged to console, no user-facing
    error. A failed background check is not actionable by the user and
    shouldn't interrupt them.

Why not through `RepoClient`: that interface exists so `src/components` and
`src/state` work unmodified against a future VSCode-webview backend (see
`docs/ARCHITECTURE.md`'s "`RepoClient`: why it exists"). A VSCode extension
would use VSCode's own built-in auto-update, not this plugin — so putting
updater calls behind `RepoClient` would model a capability that doesn't
port. `ipc/updater.ts` is therefore a Tauri-only module the desktop app's
`UpdateBanner` depends on directly; a future webview build simply wouldn't
render that component.

## Testing

- Frontend: `frontend/src/ipc/updater.test.ts` mocks `@tauri-apps/plugin-updater`
  and `@tauri-apps/plugin-process` directly (`vi.mock`, same pattern as
  `tauriRepoClient.test.ts`), verifying each wrapper calls through correctly.
  `frontend/src/components/UpdateBanner.test.tsx` mocks `../ipc/updater`
  instead (component-level tests mock the seam they depend on, not the Tauri
  plugin two layers down), covering: no update found → no banner; update
  found → `downloadAndInstallUpdate` called → banner shown after it resolves;
  restart button → `relaunchApp` called; rejection from either call → banner
  stays hidden, no thrown error.
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
