# AGENTS.md

Guidance for coding agents (Claude Code, Codex and others) working in this repo. Claude Code
loads it through `CLAUDE.md`'s `@AGENTS.md` import.

## Project

Browsitory: a Git GUI. Rust backend (`git2`) shared across two frontends — a Tauri desktop app
and a VSCode extension — behind one `RepoClient` IPC interface. See
`docs/ARCHITECTURE.md` for the full design and `CHANGELOG.md` for what shipped when.
`docs/README.md` indexes every document; the user guide is `docs/guide/` (one page per topic,
published with Zensical, see "User guide site" below) and `CONTRIBUTING.md` is the human-facing
build guide.

## Commands

```bash
cargo build --workspace                          # build all Rust crates
cargo test --workspace                            # run all Rust tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check                        # format check (CI)
node ../../frontend/node_modules/@tauri-apps/cli/tauri.js dev  # run the desktop app (from
                                                   # crates/tauri-app, after `pnpm install` in
                                                   # frontend/): the Tauri CLI the lockfile pins
scripts/build-dist.sh                              # build standalone distributable (Linux/macOS)
scripts\build-dist.ps1                             # build standalone distributable (Windows)
```

Single Rust test: `cargo test -p git-core --test status` or
`cargo test -p git-core -- reports_an_untracked_file_as_unstaged_new`.

```bash
cd frontend
pnpm install
pnpm dev                                           # Vite dev server (used by `tauri dev`)
pnpm build
pnpm lint
pnpm test -- --run
```

```bash
cd extension
pnpm install
pnpm test -- --run
pnpm run compile
```

### E2E: Tauri app (tauri-driver + WebdriverIO), from repo root

```bash
cargo install tauri-driver --locked               # once, if not already installed
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
cd e2e && pnpm install && pnpm test               # spawns/reaps tauri-driver itself; needs a display (headless: dbus-run-session -- xvfb-run -a pnpm test)
```

### E2E: VSCode extension (@vscode/test-electron), from repo root

```bash
cargo build --workspace
cd frontend && pnpm run generate:release-notes && VITE_E2E_REPO_PATH=/tmp/browsitory-vscode-e2e-repo pnpm exec vite build --config vite.vscode.config.ts && cd ..
cd extension && pnpm install && pnpm run compile && cd ..
cd extension/e2e
pnpm install --ignore-workspace                    # extension/e2e isn't a member of
                                                    # extension/pnpm-workspace.yaml; its own
                                                    # pnpm-workspace.yaml stops pnpm walking up
                                                    # to extension/, --ignore-workspace too
pnpm test                                          # spawns the extension under vscode.ExtensionMode.Test; needs a display (xvfb-run on headless CI)
```

### User guide site (Zensical), from repo root

```bash
python3 -m venv .venv && .venv/bin/pip install --require-hashes -r docs/requirements.txt
python3 scripts/check-docs-nav.py                 # nav vs pages, absolute links, missing images
.venv/bin/zensical build --strict                 # links to missing pages or anchors
```

Both are what `.github/workflows/docs.yml` runs.

Zensical reads `mkdocs.yml` (`theme.variant: classic` keeps the Material look). The pages are
`docs/guide/*.md`, plain Markdown with relative links (also read on GitHub); a new page needs a
`nav` entry in `mkdocs.yml`. Screenshots in `docs/guide/assets/` come from
`e2e/screenshots/capture.sh` (see `e2e/screenshots/README.md`); don't edit them by hand.

## Git hooks

One-time setup, per clone/worktree:

```bash
ln -sf "$(git rev-parse --show-toplevel)/scripts/hooks/pre-push" "$(git rev-parse --git-path hooks)/pre-push"
```

Wires up `scripts/check-changelog.py` as a `pre-push` hook: blocks a push touching `crates/`,
`frontend/src/`, or `e2e/` without a changelog entry (see below). The hook diffs against
`origin/main` if your clone has it, else `main`; it never fetches, so `git fetch` now and then
keeps it accurate. Bypass once with `SKIP_CHANGELOG_CHECK=1 git push`. A diff that touches only dependency manifests/lockfiles
(`Cargo.toml`, `Cargo.lock`, `package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml`,
`rust-toolchain.toml`) needs no entry. CI's `changelog` job runs the same check (the
`skip-changelog` PR label bypasses it).

## Changelog entries

Don't edit `CHANGELOG.md` in a pull request; add a fragment instead, so parallel PRs don't
conflict. One file per PR: `changelog.d/<issue>-<short-slug>.md` (lowest issue number if
several; without an issue, a descriptive slug alone). It holds exactly the section you would
have put at the top of `## [Unreleased]`: a `### Heading` line, an optional intro, then
`- Fixed:`/`Added:`/`Changed:` bullets with issue refs, wrapped at about 100 characters. No
`## [Unreleased]` header. Format and example: `changelog.d/README.md`. Before a release,
`scripts/fold-changelog.py` folds all fragments into `CHANGELOG.md` (see `docs/RELEASING.md`).

## Dependency updates

Renovate (`renovate.json`) opens dependency PRs before 6am on Mondays (Europe/Berlin), at most
three open at a time, labelled `maintainability`, and never automerges — merge by hand once CI
is green. Covered: the Cargo workspace, all four pnpm projects (`frontend/`, `extension/`,
`e2e/`, `extension/e2e/`, each with its own lockfile), GitHub Actions,
`rust-toolchain.toml`, and the user guide site's Python tools (`docs/requirements.in`, locked
with hashes in `docs/requirements.txt` by Renovate's `pip-compile` manager). Minor/patch updates
are grouped per ecosystem (Rust, JS, Python docs tools, GitHub Actions); Tauri crates and `@tauri-apps/*` packages share one group. Majors and
`git2`/`libgit2-sys` wait for approval on the Dependency Dashboard issue (libgit2 bumps change
behaviour the tests and `docs/ARCHITECTURE.md` characterize). Security fixes skip the
schedule; lockfiles get a monthly maintenance PR.

The license policy and the CI `audit` and `licenses` jobs still apply to these PRs. The
`licenses` job checks every licence in the full Rust and npm trees, transitive and dev
included (`cargo deny check licenses` with `deny.toml`; `scripts/check-dependency-licenses.py`
for all four pnpm projects). If an update pulls in a licence outside the policy, it fails:
replace the dependency or ask the owner, then record the decision as an exception in
`deny.toml` or the script and in `docs/LICENSE_COMPLIANCE.md`, in the same PR. A stale
exception fails too; remove it. `scripts/check-license-compliance.py` (`audit` job) still
requires a `docs/LICENSE_COMPLIANCE.md` row for every *direct* dependency. Renovate also
updates the pinned `CARGO_DENY_VERSION` in `ci.yml`; cargo-deny is 0.x, so its minor bumps
wait for approval on the Dependency Dashboard like other 0.x dependencies. Advisories stay with `cargo audit`: when an
update fixes one, drop its ignore from `.cargo/audit.toml` in the same PR and close the
matching RUSTSEC issue.

Any change to the shipped Rust or npm trees also changes the generated
`THIRD_PARTY_NOTICES.md`: CI's `licenses` job fails until it is regenerated with
`python3 scripts/generate-third-party-notices.py` (needs cargo-about and `pnpm install` in
`frontend/` and `extension/`) and committed on the PR branch. On Renovate PRs a workflow does
this (`.github/workflows/renovate-notices.yml`, #325), but only when Renovate itself pushed
or opened the PR, not when someone else pushes to a Renovate branch. A read-only job
regenerates the file; a second job, which runs no package code, commits it to the Renovate
branch as `github-actions[bot]` and starts `ci.yml` on that branch with `workflow_dispatch`,
because a push made with `GITHUB_TOKEN` starts no workflow run. That run's checks show on the
PR. `renovate.json` lists the bot in `gitIgnoredAuthors`, so Renovate keeps rebasing the
branch. A rebase drops the bot's commit; the next `pull_request` run then regenerates once
more (one cycle per Renovate update, no loop). If that workflow fails, regenerate by hand as
above. Keep `ci.yml` working on `workflow_dispatch`: a job that reads pull request context
needs a fallback for it. A dispatched run has no labels, so `changelog` ignores
`skip-changelog` there (Renovate PRs don't need it).

## Versioning & releases

Semantic versioning; a `release/MAJOR.MINOR.PATCH` tag (no `v` prefix) triggers
`.github/workflows/release.yml`. Full process, extension publishing and owner setup:
`docs/RELEASING.md`. Two rules agents must not break:

- **Never re-tag a published version at a different commit.** The extension registries refuse
  to replace a published version; bump the version instead.
- Don't change `VSIX_PRE_RELEASE` in `release.yml` or release secrets/environments without the
  owner asking for it.

## License policy

Permissive dependencies only: MIT, Apache-2.0, ISC, BSD-2-Clause, BSD-3-Clause, MIT-0,
Unicode-3.0, Zlib, BlueOak-1.0.0, 0BSD and PSF-2.0 / Python-2.0. One deliberate exception,
`libgit2-sys` (vendored libgit2), plus per-package owner decisions for a few Rust crates and
dev-only npm packages; all are listed in `docs/LICENSE_COMPLIANCE.md`. Never add a licence or
an exception on your own: ask the owner. Before adding a dependency: verify its license
(`cargo info <crate>` / `npm info <package> license`) and add a row to
`docs/LICENSE_COMPLIANCE.md` in the same commit. CI's `licenses` job then checks the whole
tree it pulls in (`deny.toml`, `scripts/check-dependency-licenses.py`); keep their allow-lists
(and `about.toml`'s) in sync with this one. The shipped licence texts live in the generated
`THIRD_PARTY_NOTICES.md`; never edit it by hand (see `docs/LICENSE_COMPLIANCE.md`).

## Testing conventions

- `git-core`/`repo-service`: real temp-dir repos via `git2::Repository::init`/`TempDir`, never a
  mocked `Repository`.
- `frontend`: Vitest + Testing Library, mocking `RepoClient` (a real interface seam) — never
  `@tauri-apps/api` or `postMessage` directly.
- `tauri-app`: inline tests cover only its own DTO serialization; pass-through commands rely on
  `git-core`/`repo-service` coverage.
- `e2e/` (Tauri) and `extension/e2e/` (VSCode) are black-box, one flow per major feature area.
  Build steps are order-sensitive — see the "Commands" section above, not a paraphrase of it.

Full rationale and the wire-format/DTO contract details: `docs/ARCHITECTURE.md`'s "Testing
strategy".

## Task workflow

Larger implementation work: a design spec in `docs/superpowers/specs/` and a plan in
`docs/superpowers/plans/`, both named `YYYY-MM-DD-<slug>[-design].md`, written and executed with
the `superpowers` plugin's `writing-plans`, `subagent-driven-development` / `executing-plans`
and `test-driven-development` skills. Those files are a record of past decisions, not current
truth: if one disagrees with the code, `AGENTS.md` or `docs/ARCHITECTURE.md`, the latter win.
`.claude/skills/browsitory-conventions` lists the Browsitory rules generic skills don't cover.

Findings outside the task: a bug, gap or other problem you notice while working on something
else and don't fix in the same change must become a GitHub issue (`gh issue create`). Check for
an existing issue first (`gh issue list --search ...`) and comment there instead of filing a
duplicate. The issue states what you found, where (`path:line`), how to reproduce it if known,
and which task surfaced it. Mention the issue number in your final report.
