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
cd e2e && pnpm install && pnpm test               # spawns/reaps tauri-driver itself; needs a display (xvfb-run on headless CI)
```

### E2E: VSCode extension (@vscode/test-electron), from repo root

```bash
cargo build --workspace
cd frontend && pnpm run generate:release-notes && VITE_E2E_REPO_PATH=/tmp/browsitory-vscode-e2e-repo pnpm exec vite build --config vite.vscode.config.ts && cd ..
cd extension && pnpm install && pnpm run compile && cd ..
cd extension/e2e
pnpm install --ignore-workspace                    # NOT plain `pnpm install` — extension/e2e isn't
                                                    # listed in extension/pnpm-workspace.yaml's
                                                    # `packages:`, so a plain install silently
                                                    # resolves to extension/node_modules instead
pnpm test                                          # spawns the extension under vscode.ExtensionMode.Test; needs a display (xvfb-run on headless CI)
```

## Git hooks

One-time setup, per clone/worktree:

```bash
ln -sf "$(git rev-parse --show-toplevel)/scripts/hooks/pre-push" "$(git rev-parse --git-path hooks)/pre-push"
```

Wires up `scripts/check-changelog.py` as a `pre-push` hook: blocks a push touching `crates/`,
`frontend/src/`, or `e2e/` without a matching `CHANGELOG.md` update. Bypass once with
`SKIP_CHANGELOG_CHECK=1 git push`.

## Versioning & releases

Semantic versioning. Pushing a `release/MAJOR.MINOR.PATCH` tag (no `v` prefix) triggers
`.github/workflows/release.yml`: builds standalone distributables for Linux/macOS/Windows
(`tauri-apps/tauri-action`, same build as `scripts/build-dist.*`) and publishes a draft GitHub
Release. The workflow stamps the version into `crates/tauri-app/tauri.conf.json` at build time.
`cargo test --workspace` and the frontend test suite must pass on the tagged commit first.

## License policy

Permissive dependencies only (MIT, Apache-2.0, ISC, BSD, MIT-0), with one deliberate exception
(`git2`/libgit2 — see `docs/LICENSE_COMPLIANCE.md`). Before adding a dependency: verify its
license (`cargo info <crate>` / `npm info <package> license`) and add a row to
`docs/LICENSE_COMPLIANCE.md` in the same commit.

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
