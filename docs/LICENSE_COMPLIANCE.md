# License compliance

Browsitory is MIT-licensed. The allowed licences are MIT, Apache-2.0, ISC, BSD-2-Clause,
BSD-3-Clause, MIT-0, Unicode-3.0, Zlib, BlueOak-1.0.0 and PSF-2.0 / Python-2.0. Anything else
needs a recorded owner decision: the libgit2 exception and the per-package exceptions below.

The tables below list the direct dependencies, verified with `cargo info <crate>` (Rust) and
`npm info <package> license` (JS) on 2026-08-12, against `crates/*/Cargo.toml`,
`frontend/package.json`, `extension/package.json`, `e2e/package.json` and
`extension/e2e/package.json`; dependencies added later carry their own verification date in
the Notes column. CI checks the full transitive trees automatically (see "Automated checks").

## Rust (`cargo info <crate>`)

| Crate | License | Notes |
|---|---|---|
| git2 | MIT OR Apache-2.0 (binding); vendored libgit2 is GPL-2.0-with-linking-exception | Deliberate exception — see below. Direct dep of `git-core`; dev-dependency of `tauri-app`. |
| keyring 4.1.6 | MIT OR Apache-2.0 | `repo-service` — cross-platform operating-system credential storage. Source: [crates.io](https://crates.io/crates/keyring/4.1.6); verified with `cargo info keyring@4.1.6` on 2026-08-15. |
| reqwest 0.13.5 | MIT OR Apache-2.0 | `repo-service` — blocking HTTP client (`default-features = false`, `["blocking", "rustls-no-provider", "json"]`) for the GitHub/Bitbucket pull-request adapters; blocking to match the synchronous per-repo worker thread, no tokio runtime elsewhere in this codebase. TLS via rustls with the ring provider (see the `rustls` row); certificates checked against the operating system's store through `rustls-platform-verifier` 0.7.1 (MIT OR Apache-2.0, already a transitive dependency). Source: [crates.io](https://crates.io/crates/reqwest/0.13.5); verified with `cargo info reqwest@0.13.5` on 2026-11-02. |
| rustls 0.23.46 | Apache-2.0 OR ISC OR MIT | `repo-service` — direct dependency (`default-features = false`, `["ring", "std"]`) only to install ring as rustls's process-default crypto provider before the forge HTTP client is built; already in the tree through reqwest and Tauri's updater, so it adds no crate. Source: [crates.io](https://crates.io/crates/rustls/0.23.46); verified with `cargo info rustls@0.23.46` on 2026-11-02. |
| thiserror | MIT OR Apache-2.0 | `git-core`; also `config` |
| libc 0.2.189 | MIT OR Apache-2.0 | `git-core`, unix targets only — `openat` with `O_DIRECTORY`/`O_NOFOLLOW`/`O_NONBLOCK`/`O_CLOEXEC`/`O_CREAT`/`O_EXCL`, `fstatat`/`fstat` (`AT_SYMLINK_NOFOLLOW`, `S_IF*`), `unlinkat` (`AT_REMOVEDIR`), `mkdirat`, `symlinkat`, `readlinkat` and `fchmod` (and their `errno` values) for `worktree_guard`'s work-tree walker (`worktree_guard/beneath_unix.rs`), which reads, writes and deletes work-tree files one component at a time through held folder descriptors, without following a symlink or hanging on a FIFO (and `mkfifo` in tests); `setsid` (hooks and signing programs start in a new session, `process.rs`) and `killpg` (timeout/cancel kill their process group). Already a transitive dependency. Source: [crates.io](https://crates.io/crates/libc/0.2.189); verified with `cargo info libc@0.2.189` on 2026-10-28. |
| unicode-normalization 0.1.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | `git-core` — NFD before case folding in `worktree_guard::fold_name`, so names equal after Unicode normalization (é as one or two code points) collide as on APFS/HFS+ (#166). The Unicode-3.0 part covers its Unicode Character Database tables. Pulls `tinyvec` (Zlib OR Apache-2.0 OR MIT), already a transitive dependency. Source: [crates.io](https://crates.io/crates/unicode-normalization/0.1.26); verified with `cargo info unicode-normalization@0.1.26` on 2026-10-31. |
| url | MIT OR Apache-2.0 | `git-core` and `repo-service` — structurally parses HTTP(S) remote URLs so embedded credentials are rejected and credential keychain keys normalize the default port without fragile string matching. |
| tempfile | MIT OR Apache-2.0 | dev-dependency of `tauri-app`, `repo-service` and `vscode-sidecar`; runtime dependency of `config` (atomic config-file writes) and `git-core` (signature temp files for the signer programs) |
| ctor 1.0.13 | Apache-2.0 OR MIT | `test-support` (dev-only crate; dev-dependency of `git-core`, `repo-service`, `vscode-sidecar`, `tauri-app`) — runs the hermetic-git test guard before `main`, so env vars are set before the test harness starts threads (#111). Never linked into the app or the sidecar. Already a transitive dependency. Source: [crates.io](https://crates.io/crates/ctor/1.0.13); verified with `cargo info ctor@1.0.13` on 2026-11-02 (pulls in `link-section` and `linktime-proc-macro`, both Apache-2.0 OR MIT). |
| tauri | Apache-2.0 OR MIT | `tauri-app` |
| tauri-build | Apache-2.0 OR MIT | build-dependency of `tauri-app` |
| tauri-plugin-dialog | Apache-2.0 OR MIT | `tauri-app` — native folder-picker dialog |
| tauri-plugin-opener 2.5.4 | Apache-2.0 OR MIT | `tauri-app` — opens a pull request's provider URL in the user's default external browser instead of navigating the app's own webview away. Source: [crates.io](https://crates.io/crates/tauri-plugin-opener/2.5.4); verified with `cargo info tauri-plugin-opener` on 2026-08-17. |
| serde | MIT OR Apache-2.0 | `tauri-app`, `repo-service` and `git-core`; also `config` |
| serde_json | MIT OR Apache-2.0 | `tauri-app`, `repo-service` and `git-core` |
| directories | MIT OR Apache-2.0 | `config` |
| toml | MIT OR Apache-2.0 | `config` |
| tauri-plugin-updater 2 | Apache-2.0 OR MIT | `tauri-app` — checks/downloads app updates for `UpdateBanner`. Source: [crates.io](https://crates.io/crates/tauri-plugin-updater); verified with `cargo info tauri-plugin-updater` on 2026-08-26. |
| tauri-plugin-process 2 | Apache-2.0 OR MIT | `tauri-app` — restarts the app to apply a downloaded update. Source: [crates.io](https://crates.io/crates/tauri-plugin-process); verified with `cargo info tauri-plugin-process` on 2026-08-26. |
| log 0.4 | MIT OR Apache-2.0 | `tauri-app` — logging facade used for the panic hook and any future backend log calls. Source: [crates.io](https://crates.io/crates/log); verified with `cargo info log` on 2026-08-28. |
| tauri-plugin-log 2 | Apache-2.0 OR MIT | `tauri-app` — rotated file logging (OS log dir) for failure diagnosis, backing `frontend`'s `@tauri-apps/plugin-log`. Source: [crates.io](https://crates.io/crates/tauri-plugin-log); verified with `cargo info tauri-plugin-log` on 2026-08-28. |

## JavaScript (`npm info <package> license`)

| Package | License | Notes |
|---|---|---|
| react | MIT | |
| react-dom | MIT | |
| lucide-react | ISC | Icon set adopted in Phase 5 for stage/unstage, merge-conflict, branch, stash, tag, worktree, submodule, PR-status, and theme-toggle iconography. |
| @tauri-apps/api | Apache-2.0 OR MIT | |
| vite | MIT | dev only |
| typescript | Apache-2.0 | dev only; shared by the frontend and VSCode extension packages. |
| vitest | MIT | dev only; shared by the frontend and VSCode extension packages. |
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
| @types/node | MIT | dev only; shared by the frontend and VSCode extension packages. |
| @types/react | MIT | dev only |
| @types/react-dom | MIT | dev only |
| @tauri-apps/plugin-process | MIT OR Apache-2.0 | frontend half of `tauri-plugin-process`, used by `UpdateBanner`. Source: `npm info @tauri-apps/plugin-process license`, verified 2026-08-26. |
| @tauri-apps/plugin-updater | MIT OR Apache-2.0 | frontend half of `tauri-plugin-updater`, used by `UpdateBanner`. Source: `npm info @tauri-apps/plugin-updater license`, verified 2026-08-26. |
| @tauri-apps/plugin-log | MIT OR Apache-2.0 | frontend half of `tauri-plugin-log`; `tauriRepoClient.ts` and `main.tsx` log IPC/uncaught failures into the same rotated file as the Rust backend. Source: `npm info @tauri-apps/plugin-log license`, verified 2026-08-28. |
| shiki 4.5.0 | MIT | `frontend` — syntax highlighting in diffs, using its JavaScript regex engine (no WebAssembly, so no CSP change) and CSS-variables theme. Languages load on demand. Source: [npm](https://www.npmjs.com/package/shiki/v/4.5.0); verified with `npm info shiki@4.5.0 license` on 2026-10-16. |

## JavaScript, `extension/` (`npm info <package> license`)

`extension/` is a separate pnpm package. Its dependencies are development tooling and VSCode
extension-host typings; none are loaded by the packaged extension itself.

| Package | License | Notes |
|---|---|---|
| @types/node | MIT | dev only; same package as the frontend row above. |
| @types/vscode 1.134.0 | MIT | dev only; VSCode extension-host API types. |
| @vscode/vsce 3.9.2 | MIT | dev only; packages target-specific VSIX artifacts, and `release.yml` runs it to publish them to the Visual Studio Marketplace. Source: `npm info @vscode/vsce@3.9.2 license`, verified 2026-08-31 (re-checked 2026-11-02). |
| typescript | Apache-2.0 | dev only; same package as the frontend row above. |
| vitest | MIT | dev only; same package as the frontend row above. |

## Build-only CI tools (not dependencies)

These tools run only inside a GitHub Actions job. They are not declared in any `package.json`
or `Cargo.toml`, are not linked into anything, and are not shipped. Running a tool places no
licence obligation on Browsitory, so they may be under a licence outside the allowed list.
Each is pinned to an exact version where it runs.

| Tool | License | Notes |
|---|---|---|
| cargo-deny 0.20.2 | MIT OR Apache-2.0 | `ci.yml`'s `licenses` job installs it with `taiki-e/install-action` and runs `cargo deny check licenses`. Version pinned in the job's `CARGO_DENY_VERSION` (Renovate updates it). Source: `cargo info cargo-deny@0.20.2`, verified 2026-11-03. |
| ovsx 1.3.0 | EPL-2.0 | `release.yml`'s `publish-vsix-open-vsx` job runs it as `npx --yes ovsx@1.3.0 publish` to upload the already-built `.vsix` files to Open VSX. EPL-2.0 is weak copyleft: it only covers changes to ovsx itself, which we neither make nor distribute. Never add it as a dependency. Source: `npm info ovsx@1.3.0 license`, verified 2026-11-02. |

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

## JavaScript, `extension/e2e/` (`npm info <package> license`)

`extension/e2e/` is a separate pnpm package from `extension/`, a standalone Electron E2E
harness (`@vscode/test-electron` + Mocha) that launches a real VSCode Extension Development Host
against the unpacked, dev-mode extension (`vscode.ExtensionMode.Test`), never a packaged
`.vsix`; none of its dependencies are shipped in the extension itself. The
webview panel is driven over a raw Chrome DevTools Protocol connection hand-rolled in
`support/webviewPage.ts` on Node's built-in `WebSocket`/`fetch` globals — no browser-automation
library is involved, so this table has no dependency row for one.

| Package | License | Notes |
|---|---|---|
| @types/mocha 10.0.10 | MIT | dev only; same package as `e2e/`'s row above. |
| @types/node 24.13.3 | MIT | dev only; same package as the frontend row above. |
| @types/vscode 1.134.0 | MIT | dev only; VSCode extension-host API types, for typechecking the `vscode.commands`/`vscode.workspace` calls in `specs/first-flow.spec.ts`. Same package already verified for `extension/`'s own row above — not re-verified here. |
| @vscode/test-electron 2.5.2 | MIT | dev only; downloads/launches a real VSCode Extension Development Host for the harness. Source: `npm info @vscode/test-electron@2.5.2 license`, verified 2026-09-01. |
| mocha 11.8.0 | MIT | dev only; test runner loaded by `suite/index.ts`. |
| typescript 6.0.3 | Apache-2.0 | dev only; same package as the frontend row above. |

## The one exception: libgit2 via `git2`

`git2` vendors libgit2, which is GPL-2.0-with-linking-exception, not MIT. The linking
exception explicitly permits linking libgit2 from software under a different license (like
Browsitory's MIT), so this is a conscious, documented choice, not an oversight — see
`docs/ARCHITECTURE.md`'s "Why git2 (libgit2 bindings), not gitoxide" for the rationale. No
other GPL/AGPL/SSPL dependency is acceptable without a new, equally explicit decision recorded
here. `deny.toml` scopes this exception to the `libgit2-sys` crate alone (see "Rust
exceptions").

Every VSCode extension package (`.vsix`) bundles the `vscode-sidecar` binary, which links
libgit2 statically. So the package ships `extension/THIRD_PARTY_NOTICES.md` next to `LICENSE`:
libgit2's full `COPYING` (GPL-2.0 text, the linking exception, and the licences of the code
libgit2 bundles), plus the licence texts of the other native libraries the sidecar links:
libssh2 (static), OpenSSL (static, Linux and macOS packages only) and zlib (static, or the
build machine's system zlib where `libz-sys` finds one).
When a `libgit2-sys`, `libssh2-sys`, `openssl-src` or `libz-sys` update changes a version or a
licence text, update that file in the same PR.

## Unicode-3.0

Unicode-3.0 (the Unicode License v3, OSI-approved) is a permissive notice license for the
Unicode Character Database and code derived from it; it allows use, modification and
redistribution with the copyright and permission notice kept. It is on the allowed list since
2026-10-31 (#166). Crates carrying it: `unicode-normalization` (direct, above) and the
transitive `icu_*` crates (`icu_normalizer`, `icu_properties`, `icu_collections`, …) that
`url` pulls in through `idna`.

## Zlib

Zlib (the zlib/libpng license, OSI-approved) is a short permissive license: it allows use,
modification and redistribution, including in binaries, with no copyleft and no notice
requirement for binary distribution; altered source must be marked as altered and the notice
kept in source distributions. It is on the allowed list since 2026-11-02 (user decision, Renovate
lockfile maintenance #279). Crates carrying it only: the transitive `zlib-rs` (0.6.8, verified with
`cargo info zlib-rs` on 2026-11-02), which `flate2` 1.1.10 uses as its default Rust backend
(`flate2` itself is MIT OR Apache-2.0); `flate2` comes in through Tauri's updater and zip
handling. Other crates offer Zlib as one option among MIT/Apache-2.0 (`tinyvec`,
`miniz_oxide`).

## BlueOak-1.0.0 and PSF-2.0 / Python-2.0

Both are permissive licences on the allowed list since 2026-11-03 (owner decision, #273).
BlueOak-1.0.0 (the Blue Oak Model License) allows use, modification and redistribution with
the licence or its notice kept; it also grants patent rights. PSF-2.0 (the Python Software
Foundation License) and Python-2.0 (the Python licence stack that contains it) allow the same
with the copyright notice kept. They appear only in the npm dev trees: BlueOak-1.0.0 in `glob`,
`minimatch`, `lru-cache`, `path-scurry` and related packages; PSF-2.0 / Python-2.0 in
`argparse` (through `js-yaml`).

## Rust exceptions

Recorded in `deny.toml`'s `[licenses] exceptions`, pinned by crate name (not version, so a
routine update doesn't break CI; a new crate under the same licence still fails).

| Crate | License | Why it's acceptable |
|---|---|---|
| libgit2-sys | GPL-2.0-only WITH GCC-exception-2.0 (vendored libgit2) | The one exception, see "The one exception" below. `libgit2-sys` declares only `MIT OR Apache-2.0` (its binding), so `deny.toml` clarifies the real licence from `libgit2/COPYING`, pinned by that file's hash. If the file changes, the clarification stops matching, the exception goes unused and the check fails, so someone re-reads the licence. `git2` vendors nothing and needs no entry. |
| option-ext 0.2.0 | MPL-2.0 | **Ships** in the desktop app and in the VSCode sidecar, through `directories` (our `config` crate) and `dirs` (Tauri). MPL-2.0 is file-level copyleft: we use it unmodified and say where its source is in `extension/THIRD_PARTY_NOTICES.md` (source: <https://crates.io/crates/option-ext/0.2.0>, <https://github.com/soc/option-ext>). The desktop app has no notices file; this row is its record. Owner decision 2026-11-03 (#273). |
| cssparser, cssparser-macros, selectors, dtoa-short | MPL-2.0 | Build time only on our targets: Tauri's build and code generation (`tauri-build`, `tauri-codegen`) through `dom_query`. Not linked into anything we ship. Owner decision 2026-11-03 (#273). |
| target-lexicon | Apache-2.0 WITH LLVM-exception | Build time only (`system-deps`, used by the Linux GTK bindings' build scripts). The LLVM exception only adds permissions to Apache-2.0. Owner decision 2026-11-03 (#273). |

## JavaScript dev-tree exceptions

Recorded in `EXCEPTIONS` and `CLARIFICATIONS` in `scripts/check-dependency-licenses.py`; keep
both in sync. None of these ship: they are build, test or packaging tools, or data those tools
read. An exception never applies to the production trees of `frontend/` and `extension/`.
"Versions" is a version prefix: `1` means any 1.x.y. Owner decision 2026-11-03 (#273).

| Package | Versions | License | Reason |
|---|---|---|---|
| lightningcss, lightningcss-* | 1 | MPL-2.0 | Vite's CSS minifier and its per-platform native binaries (`frontend/`, `extension/`'s vitest). A build tool; its output isn't covered. |
| caniuse-lite | 1 | CC-BY-4.0 | Browser support data for browserslist (`eslint-plugin-react-hooks` > `@babel/core`). |
| mdn-data | 2 | CC0-1.0 | CSS data for `css-tree` (`jsdom`, tests only). |
| spdx-license-ids | 3 | CC0-1.0 | SPDX id list for package metadata tools (`@vscode/vsce`, `@wdio/cli`). |
| spdx-exceptions | 2 | CC-BY-3.0 | SPDX exception list, same tools. |
| tslib | 2 | 0BSD | TypeScript runtime helpers inside `@vscode/vsce`'s and WebdriverIO's dependencies. |
| stream-buffers | 3 | Unlicense | `@wdio/local-runner` (`e2e/` only). |
| @promptbook/utils | 0 | CC-BY-4.0 | `@wdio/utils` > `locate-app` (`e2e/` only). |
| @azu/style-format | 1 | WTFPL | `@vscode/vsce` > secretlint formatter (packaging only). |
| binaryextensions, editions, istextorbinary, textextensions, version-range | 6, 6, 9, 6, 4 | Artistic-2.0 | `@vscode/vsce` > secretlint > `istextorbinary` (packaging only). |
| @vscode/vsce-sign, @vscode/vsce-sign-* | 2 | Microsoft licence terms (`SEE LICENSE IN LICENSE.txt`, reported as Unknown) | Required by Microsoft's own publishing tool `@vscode/vsce`. Used only to package and publish, locally and in CI; never redistributed in the VSIX or the app. |

| Clarified package | Versions | License | Reason |
|---|---|---|---|
| css-value | 0.0.1 | MIT | No licence field in its `package.json`; its `Readme.md` carries the MIT licence. Through `webdriverio` (`e2e/` only). |

## Automated checks

CI's `licenses` job (`.github/workflows/ci.yml`) checks every licence in the full trees, on
every pull request, every push to `main` and the weekly scheduled run:

- **Rust:** `cargo deny check licenses` with `deny.toml`. It covers every workspace crate and
  every transitive dependency for all the platforms we build for (Linux, macOS and Windows,
  x86_64 and aarch64), all features, including build and dev dependencies. Only the
  `licenses` check runs: advisories stay with `cargo audit` in the `audit` job
  (`.cargo/audit.toml`), and cargo-deny's `bans` and `sources` checks aren't used.
- **npm:** `scripts/check-dependency-licenses.py` (unit tests in
  `scripts/test_check_dependency_licenses.py`). For each of `frontend/`, `extension/`, `e2e/`
  and `extension/e2e/` it reads the installed tree with `pnpm licenses list --json`,
  production and dev. The production trees of `frontend/` and `extension/` must use allowed
  licences only; dev packages may also match the exceptions table above. pnpm installs only
  the current platform's native binaries, so the script looks up every lockfile package that
  isn't installed (esbuild, rolldown, lightningcss, `@tauri-apps/cli`, `@vscode/vsce-sign`
  for other platforms) in the npm registry by exact `name@version`. A published version can't
  change, so this is as reliable as installing every platform. Installing them instead
  (pnpm's `supportedArchitectures`, or `pnpm install --force`) would download every
  platform's native binaries, hundreds of MB, on every run, and would need a different
  setting for pnpm 9 and pnpm 11. Such a binary counts as production when a production
  package depends on it.
- An exception or clarification that no longer matches anything fails both checks, so the
  tables can't go stale.

`scripts/check-license-compliance.py` (the `audit` job) still runs: it fails if a direct
dependency has no row in the tables above. The tree checks verify licence values; this one
keeps the per-dependency notes complete.

Run them locally:

```bash
cargo install cargo-deny --locked --version 0.20.2   # once
cargo deny check licenses
# after `pnpm install` in frontend/, extension/, e2e/ and extension/e2e/ (see CLAUDE.md):
python3 scripts/check-dependency-licenses.py --summary
```

When a check fails, the new package's licence is outside the policy. Replace the dependency,
or ask the owner for a decision and record it as an exception (`deny.toml` or the script)
and in the tables above, in the same PR.

## Process

Before adding a new dependency: run `cargo info <crate>` or `npm info <package> license`,
confirm it's permissive, and add a row to the relevant table above in the same commit that
adds the dependency. CI's tree checks (above) then catch anything it pulls in transitively.
