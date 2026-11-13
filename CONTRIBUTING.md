# Contributing to Browsitory

Thanks for helping. This page shows how to build and run Browsitory from source. The full list
of build, test and E2E commands, and the rules every change follows, are in
[AGENTS.md](AGENTS.md); it is written for coding agents, but it is the reference for people
too.

## How it fits together

Browsitory has a Rust backend (`git2`, in `crates/`) and one React frontend (`frontend/`) shared
by two hosts: a Tauri desktop app (`crates/tauri-app`) and a VSCode extension (`extension/`).
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) describes the design.

## Run the desktop app from source

You need Rust, Node.js 22 or newer (CI uses 24; packaging the extension with `vsce` needs 22),
[pnpm](https://pnpm.io) 11 or newer (it switches to the version each `package.json` pins) and
the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your system. The Tauri CLI
comes with the frontend's dependencies (`@tauri-apps/cli`, locked in `frontend/pnpm-lock.yaml`),
the same version the release build uses; run it with `node` from `crates/tauri-app`, where it
finds `tauri.conf.json`.

```bash
git clone https://github.com/rene-langer/browsitory.git
cd browsitory
cd frontend && pnpm install && cd ../crates/tauri-app
node ../../frontend/node_modules/@tauri-apps/cli/tauri.js dev
```

## Build a distributable

```bash
scripts/build-dist.sh      # Linux/macOS: .deb/.rpm/.AppImage or .dmg
scripts\build-dist.ps1     # Windows: .msi/.exe
```

Bundles land in `target/release/bundle/`, with a copy of `THIRD_PARTY_NOTICES.md`.

## Tests and checks

The short version (details and the E2E suites: [AGENTS.md](AGENTS.md#commands)):

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cd frontend && pnpm test -- --run && pnpm lint
```

Code under `#[cfg(windows)]` (in `crates/git-core`) only compiles for a Windows target. On Linux
or macOS, `scripts/check-windows.sh` runs clippy on it for `x86_64-pc-windows-gnu`; it needs
`rustup target add x86_64-pc-windows-gnu` and nothing else (no MinGW: a stub stands in for the C
compiler, as nothing is linked). Its tests run only on CI's `windows-latest` job.

## Pull requests

- Every pull request that changes the app adds a changelog fragment in `changelog.d/` instead
  of editing `CHANGELOG.md` (see [changelog.d/README.md](changelog.d/README.md)).
- New dependencies must use a licence from the policy in [AGENTS.md](AGENTS.md#license-policy)
  and get a row in [docs/LICENSE_COMPLIANCE.md](docs/LICENSE_COMPLIANCE.md).
- User-facing text follows [docs/CONTENT_GUIDELINES.md](docs/CONTENT_GUIDELINES.md).
- Pull requests are squash-merged (the only merge method allowed), and GitHub deletes the
  branch on merge. Because the branch's own commits never reach `main`, `git branch --merged`
  and `git fetch --prune` don't clean up your local copies;
  `scripts/prune-merged-branches.sh` lists them, and `--delete` removes them.

## Documentation

- The user guide lives in [docs/guide/](docs/guide/README.md), one Markdown page per topic. It
  is published to <https://rene-langer.github.io/browsitory/> with
  [Zensical](https://zensical.org/), which reads `mkdocs.yml`, by `.github/workflows/docs.yml`. Build it locally:

  ```bash
  python3 -m venv .venv
  .venv/bin/pip install --require-hashes -r docs/requirements.txt
  .venv/bin/zensical serve      # live preview
  .venv/bin/zensical build --strict && python3 scripts/check-docs-nav.py   # what CI checks
  ```

- The screenshots in `docs/guide/assets/` come from a script; see
  [e2e/screenshots/README.md](e2e/screenshots/README.md).
- [docs/README.md](docs/README.md) lists every document in `docs/`.
- Releases: [docs/RELEASING.md](docs/RELEASING.md).
