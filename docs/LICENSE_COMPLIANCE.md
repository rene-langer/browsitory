# License compliance

Browsitory is MIT-licensed. Every dependency below was verified permissive (MIT, Apache-2.0,
ISC, BSD, MIT-0) except the one documented exception. Verified with `cargo info <crate>` (Rust)
and `npm info <package> license` (JS) on 2026-08-12, against the direct dependencies declared
in `crates/*/Cargo.toml` and `frontend/package.json` as of Tasks 1-3 (not the full transitive
tree).

## Rust (`cargo info <crate>`)

| Crate | License | Notes |
|---|---|---|
| git2 | MIT OR Apache-2.0 (binding); vendored libgit2 is GPL-2.0-with-linking-exception | Deliberate exception — see below. Direct dep of `git-core`; dev-dependency of `tauri-app`. |
| thiserror | MIT OR Apache-2.0 | `git-core`; also `config` |
| tempfile | MIT OR Apache-2.0 | dev-dependency of `git-core`, `tauri-app`, and `config` |
| tauri | Apache-2.0 OR MIT | `tauri-app` |
| tauri-build | Apache-2.0 OR MIT | build-dependency of `tauri-app` |
| tauri-plugin-dialog | Apache-2.0 OR MIT | `tauri-app` — native folder-picker dialog |
| serde | MIT OR Apache-2.0 | `tauri-app`; also `config` |
| serde_json | MIT OR Apache-2.0 | `tauri-app` |
| directories | MIT OR Apache-2.0 | `config` |
| toml | MIT OR Apache-2.0 | `config` |

## JavaScript (`npm info <package> license`)

| Package | License | Notes |
|---|---|---|
| react | MIT | |
| react-dom | MIT | |
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
