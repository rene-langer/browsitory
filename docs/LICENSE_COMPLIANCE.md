# License compliance

Browsitory is MIT-licensed. Every dependency below was verified permissive (MIT, Apache-2.0,
ISC, BSD, MIT-0) except the one documented exception. Verified with `cargo info <crate>` (Rust)
and `npm info <package> license` (JS) on 2026-08-12, against the direct dependencies declared
in `crates/*/Cargo.toml`, `frontend/package.json`, and `e2e/package.json` as of Phase 1's
completion (not the full transitive tree).

## Rust (`cargo info <crate>`)

| Crate | License | Notes |
|---|---|---|
| git2 | MIT OR Apache-2.0 (binding); vendored libgit2 is GPL-2.0-with-linking-exception | Deliberate exception — see below. Direct dep of `git-core`; dev-dependency of `tauri-app`. |
| keyring 4.1.6 | MIT OR Apache-2.0 | `tauri-app` — cross-platform operating-system credential storage. Source: [crates.io](https://crates.io/crates/keyring/4.1.6); verified with `cargo info keyring@4.1.6` on 2026-08-15. |
| reqwest 0.12.28 | MIT OR Apache-2.0 | `tauri-app` — blocking HTTP client (`default-features = false`, `["blocking", "rustls-tls", "json"]`) for the GitHub/Bitbucket pull-request adapters; blocking to match the synchronous per-repo worker thread, no tokio runtime elsewhere in this codebase. Source: [crates.io](https://crates.io/crates/reqwest/0.12.28); verified with `cargo info reqwest@0.12.28` on 2026-08-17. |
| thiserror | MIT OR Apache-2.0 | `git-core`; also `config` |
| url | MIT OR Apache-2.0 | `git-core` and `tauri-app` — structurally parses HTTP(S) remote URLs so embedded credentials are rejected and credential keychain keys normalize the default port without fragile string matching. |
| tempfile | MIT OR Apache-2.0 | dev-dependency of `git-core`, `tauri-app`, and `config` |
| tauri | Apache-2.0 OR MIT | `tauri-app` |
| tauri-build | Apache-2.0 OR MIT | build-dependency of `tauri-app` |
| tauri-plugin-dialog | Apache-2.0 OR MIT | `tauri-app` — native folder-picker dialog |
| tauri-plugin-opener 2.5.4 | Apache-2.0 OR MIT | `tauri-app` — opens a pull request's provider URL in the user's default external browser instead of navigating the app's own webview away. Source: [crates.io](https://crates.io/crates/tauri-plugin-opener/2.5.4); verified with `cargo info tauri-plugin-opener` on 2026-08-17. |
| serde | MIT OR Apache-2.0 | `tauri-app`; also `config` |
| serde_json | MIT OR Apache-2.0 | `tauri-app` |
| directories | MIT OR Apache-2.0 | `config` |
| toml | MIT OR Apache-2.0 | `config` |
| tauri-plugin-updater 2 | Apache-2.0 OR MIT | `tauri-app` — checks/downloads app updates for `UpdateBanner`. Source: [crates.io](https://crates.io/crates/tauri-plugin-updater); verified with `cargo info tauri-plugin-updater` on 2026-08-26. |
| tauri-plugin-process 2 | Apache-2.0 OR MIT | `tauri-app` — restarts the app to apply a downloaded update. Source: [crates.io](https://crates.io/crates/tauri-plugin-process); verified with `cargo info tauri-plugin-process` on 2026-08-26. |

## JavaScript (`npm info <package> license`)

| Package | License | Notes |
|---|---|---|
| react | MIT | |
| react-dom | MIT | |
| lucide-react | ISC | Icon set adopted in Phase 5 for stage/unstage, merge-conflict, branch, stash, tag, worktree, submodule, PR-status, and theme-toggle iconography. |
| @tauri-apps/api | Apache-2.0 OR MIT | |
| vite | MIT | dev only |
| typescript | Apache-2.0 | dev only |
| vitest | MIT | dev only |
| @testing-library/react | MIT | dev only |
| @testing-library/jest-dom | MIT | dev only |
| jsdom | MIT | dev only |
| @tauri-apps/cli | Apache-2.0 OR MIT | dev only |
| eslint | MIT | dev only |
| @eslint/js | MIT | dev only |
| eslint-plugin-react-hooks | MIT | dev only |
| eslint-plugin-react-refresh | MIT | dev only |
| typescript-eslint | MIT | dev only |
| globals | MIT | dev only |
| @vitejs/plugin-react | MIT | dev only |
| @types/node | MIT | dev only |
| @types/react | MIT | dev only |
| @types/react-dom | MIT | dev only |
| @tauri-apps/plugin-process | MIT OR Apache-2.0 | frontend half of `tauri-plugin-process`, used by `UpdateBanner`. Source: `npm info @tauri-apps/plugin-process license`, verified 2026-08-26. |
| @tauri-apps/plugin-updater | MIT OR Apache-2.0 | frontend half of `tauri-plugin-updater`, used by `UpdateBanner`. Source: `npm info @tauri-apps/plugin-updater license`, verified 2026-08-26. |

## JavaScript, `e2e/` (`npm info <package> license`)

`e2e/` is a separate pnpm package from `frontend/`; its dependencies aren't shipped in the app
and are dev-only test tooling, but are recorded here for completeness.

| Package | License | Notes |
|---|---|---|
| webdriverio | MIT | |
| @wdio/cli | MIT | |
| @wdio/globals | MIT | |
| @wdio/local-runner | MIT | |
| @wdio/mocha-framework | MIT | |
| @wdio/spec-reporter | MIT | |
| @wdio/types | MIT | |
| tsx | MIT | |
| typescript | Apache-2.0 | same package as `frontend/`'s row above |
| @types/node | MIT | same package as `frontend/`'s row above |
| @types/mocha | MIT | |

## The one exception: libgit2 via `git2`

`git2` vendors libgit2, which is GPL-2.0-with-linking-exception, not MIT. The linking
exception explicitly permits linking libgit2 from software under a different license (like
Browsitory's MIT), so this is a conscious, documented choice, not an oversight — see
`docs/ARCHITECTURE.md`'s "Why git2 (libgit2 bindings), not gitoxide" for the rationale. No
other GPL/AGPL/SSPL dependency is acceptable without a new, equally explicit decision recorded
here.

## Process

Before adding a new dependency: run `cargo info <crate>` or `npm info <package> license`,
confirm it's permissive, and add a row to the relevant table above in the same commit that
adds the dependency.

CI's `audit` job runs `scripts/check-license-compliance.py`, which fails the build if any
direct dependency in `crates/*/Cargo.toml`, `frontend/package.json`, or `e2e/package.json` has
no matching row here — it checks presence, not the license value itself, so the manual
`cargo info`/`npm info` step above still applies.
