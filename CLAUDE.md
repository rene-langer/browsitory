# CLAUDE.md

Guidance for Claude Code working in this repo.

## Project

Browsitory: a Git GUI. Rust backend (`git2`) shared across two frontends — a Tauri desktop app
and a VSCode extension — behind one `RepoClient` IPC interface. See
`docs/ARCHITECTURE.md` for the full design and `CHANGELOG.md` for what shipped when.

## Commands

```bash
cargo build --workspace                          # build all Rust crates
cargo test --workspace                            # run all Rust tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check                        # format check (CI)
cargo tauri dev                                    # run the desktop app (from crates/tauri-app)
scripts/build-dist.sh                              # build standalone distributable (Linux/macOS)
scripts\build-dist.ps1                             # build standalone distributable (Windows)
```

Single Rust test: `cargo test -p git-core --test status` or
`cargo test -p git-core -- reports_an_untracked_file_as_unstaged_new`.

```bash
cd frontend
pnpm install
pnpm dev                                           # Vite dev server (used by `cargo tauri dev`)
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

## Git hooks

One-time setup, per clone/worktree:

```bash
ln -sf "$(git rev-parse --show-toplevel)/scripts/hooks/pre-push" "$(git rev-parse --git-path hooks)/pre-push"
```

Wires up `scripts/check-changelog.py` as a `pre-push` hook: blocks a push touching `crates/`,
`frontend/src/`, or `e2e/` without a matching `CHANGELOG.md` update. Bypass once with
`SKIP_CHANGELOG_CHECK=1 git push`. A diff that touches only dependency manifests/lockfiles
(`Cargo.toml`, `Cargo.lock`, `package.json`, `pnpm-lock.yaml`, `pnpm-workspace.yaml`,
`rust-toolchain.toml`) needs no entry.

## Dependency updates

Renovate (`renovate.json`) opens dependency PRs before 6am on Mondays (Europe/Berlin), at most
three open at a time, labelled `maintainability`, and never automerges — merge by hand once CI
is green. Covered: the Cargo workspace, all four pnpm projects (`frontend/`, `extension/`,
`e2e/`, `extension/e2e/`, each with its own lockfile), GitHub Actions, and
`rust-toolchain.toml`. Minor/patch updates are grouped per ecosystem (Rust, JS, GitHub
Actions); Tauri crates and `@tauri-apps/*` packages share one group. Majors and
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
updates the pinned `CARGO_DENY_VERSION` in `ci.yml`. Advisories stay with `cargo audit`: when an
update fixes one, drop its ignore from `.cargo/audit.toml` in the same PR and close the
matching RUSTSEC issue.

## Versioning & releases

Semantic versioning. Pushing a `release/MAJOR.MINOR.PATCH` tag (no `v` prefix) triggers
`.github/workflows/release.yml`: builds standalone distributables for Linux/macOS/Windows
(`tauri-apps/tauri-action`, same build as `scripts/build-dist.*`) and publishes a draft GitHub
Release. The workflow stamps the version into `crates/tauri-app/tauri.conf.json` at build time.
`cargo test --workspace` and the frontend test suite must pass on the tagged commit first.

### Publishing the extension

The same tag builds one `.vsix` per platform (`build-vsix-release`, version stamped from the
tag) and attaches them to the draft release (`publish-vsix-release`). Only after that, and after
the desktop build, two jobs publish the same files:

- **Visual Studio Marketplace** (`publish-vsix-marketplace`): GitHub OIDC into Microsoft Entra
  ID, no stored token. `azure/login`, then `vsce publish --azure-credential`.
- **Open VSX** (`publish-vsix-open-vsx`): `OVSX_PAT`, a token for the Open VSX namespace
  `browsitory`. `ovsx` runs via `npx` only (EPL-2.0, build-only tool; see
  `docs/LICENSE_COMPLIANCE.md`).

Both jobs run in the GitHub environment `marketplace` (deployment tags restricted to
`release/*`), and all three secrets are **environment** secrets there: `AZURE_CLIENT_ID`,
`AZURE_TENANT_ID`, `OVSX_PAT`. Each job skips with a notice while its secret is empty.

**Pre-release switch:** `VSIX_PRE_RELEASE` at the top of `release.yml` (`"true"` now). Set it to
`"false"` to package and publish stable releases. Packaging reads it too, because `vsce publish
--pre-release` refuses a package that wasn't packaged as a pre-release. A stable version must be
higher than every pre-release version already published.

**Never re-tag a published version at a different commit.** Both registries refuse to replace a
published version. `--skip-duplicate` only makes re-running the *same* commit's release safe; a
moved tag would leave the registries on the old build. Bump the version instead.

Owner setup (once):

1. Marketplace: create publisher `browsitory` (<https://marketplace.visualstudio.com/manage>).
2. Entra ID: an app registration with a federated credential for GitHub Actions, repository
   `rene-langer/browsitory`, entity **Environment** `marketplace`
   (`repo:rene-langer/browsitory:environment:marketplace`). Store its client and tenant ids as
   `AZURE_CLIENT_ID` / `AZURE_TENANT_ID` in the `marketplace` environment.
3. Open VSX: create an account, sign the Eclipse publisher agreement, create the namespace
   (`npx ovsx@1.3.0 create-namespace browsitory -p <token>`), and store the token as `OVSX_PAT`
   in the `marketplace` environment.
4. First release: the Marketplace job prints a notice with the Entra app's Marketplace member
   id (`az rest -u https://app.vssps.visualstudio.com/_apis/profile/profiles/me --resource
   499b84ac-1321-427f-aa17-267ca6975798 --query id -o tsv`). Until that id is a member, the
   publish step fails. Add the id to publisher `browsitory` as **Contributor**, then re-run the
   failed job. Later releases need nothing.

## License policy

Permissive dependencies only: MIT, Apache-2.0, ISC, BSD-2-Clause, BSD-3-Clause, MIT-0,
Unicode-3.0, Zlib, BlueOak-1.0.0 and PSF-2.0 / Python-2.0. One deliberate exception,
`libgit2-sys` (vendored libgit2), plus per-package owner decisions for a few Rust crates and
dev-only npm packages; all are listed in `docs/LICENSE_COMPLIANCE.md`. Never add a licence or
an exception on your own: ask the owner. Before adding a dependency: verify its license
(`cargo info <crate>` / `npm info <package> license`) and add a row to
`docs/LICENSE_COMPLIANCE.md` in the same commit. CI's `licenses` job then checks the whole
tree it pulls in (`deny.toml`, `scripts/check-dependency-licenses.py`); keep their allow-lists
in sync with this one.

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

New implementation tasks follow `docs/TASK_TEMPLATE.md` and the `superpowers` plugin's
`test-driven-development` / `writing-plans` / `subagent-driven-development` / `executing-plans`
skills, plus `.claude/skills/browsitory-conventions` for rules those don't cover (real-repo
tests, the `RepoClient` transport-isolation rule, task-file naming). `CLAUDE.md` and
`docs/ARCHITECTURE.md` are authoritative if either ever disagrees with a skill.
