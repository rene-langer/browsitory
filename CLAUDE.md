# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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

```bash
cd frontend
pnpm install
pnpm dev                                           # Vite dev server (used by `cargo tauri dev`)
pnpm build
pnpm lint
pnpm test -- --run
```

Run a single Rust test: `cargo test -p git-core --test status` or
`cargo test -p git-core -- reports_an_untracked_file_as_unstaged_new`.

```bash
# E2E (tauri-driver + WebdriverIO), from the repo root:
cargo install tauri-driver --locked               # once, if not already installed
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
cd e2e
pnpm install
pnpm test                                          # spawns/reaps tauri-driver itself; needs a display (xvfb-run on headless CI)
```

```bash
cd extension
pnpm install
pnpm test -- --run
pnpm run compile
```

```bash
# VSCode E2E (@vscode/test-electron), from the repo root:
cargo build --workspace
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-vscode-e2e-repo pnpm exec vite build --config vite.vscode.config.ts && cd ..
cd extension && pnpm install && pnpm run compile && cd ..
cd extension/e2e
pnpm install --ignore-workspace                    # NOT plain `pnpm install` — extension/e2e isn't
                                                    # listed in extension/pnpm-workspace.yaml's
                                                    # `packages:`, so a plain install silently
                                                    # resolves to extension/node_modules instead
pnpm test                                          # spawns the extension under vscode.ExtensionMode.Test; needs a display (xvfb-run on headless CI)
```

### Git hooks

One-time setup, per clone/worktree:

```bash
ln -sf "$(git rev-parse --show-toplevel)/scripts/hooks/pre-push" "$(git rev-parse --git-path hooks)/pre-push"
```

This wires up `scripts/check-changelog.py` as a `pre-push` hook: it blocks a
push that touches `crates/`, `frontend/src/`, or `e2e/` without a matching
`CHANGELOG.md` update. Bypass for one push with
`SKIP_CHANGELOG_CHECK=1 git push`.

## Project status

Second from-scratch rewrite (branch `feat/rust_from_scratch`). See
`docs/superpowers/specs/2026-08-11-browsitory-architecture-design.md` for the full rationale —
in short: a prior native Rust+egui pass (also on this branch, see git history) worked but
produced a UI that can't be reused as a VSCode webview, which is a stated future requirement.
This pass keeps the Rust git layer but replaces egui with Tauri + a React/TypeScript frontend
behind a `RepoClient` IPC interface, so a VSCode extension can implement the same interface
later without touching UI code.

Phase 0 was setup only: workspace scaffold, CI, `git-core::repo`/`status` with tests, and a
Tauri shell proving the IPC boundary end-to-end with a minimal status view.

Phase 1 (this pass) is complete: full repo view. Added `git-core::log` (commit history),
`git-core::diff` (line-level diffs for both the working tree and a given commit, plus a
`commit_files` helper), `git-core::stage` (whole-file stage/unstage), and `git-core::commit`
(commit creation) to the git layer; turned `crates/config` from a stub into a real recent-repos
registry backed by TOML; added 9 Tauri commands and a `tauri-plugin-dialog`-backed folder
picker; and built the unified frontend layout (`RepoPicker`, `HistoryList`, `DiffPane`,
`CommitBox`, composed in `App.tsx`) with basic keyboard navigation, retiring the old
`StatusView`. Also added Browsitory's first GUI E2E layer (`e2e/`, see "Testing conventions"
below) and a CI job for it.

Phases 2-4 are also complete: branch management, stash, merge with conflict resolution,
interactive rebase, blame viewer, and the multi-branch commit graph (Phase 2); push/pull/fetch
with transfer progress, multi-remote and tag push, and OS-keychain-backed credential handling
(Phase 3); worktrees, submodules, a reflog viewer, and forge (GitHub/Bitbucket) pull request
integration (Phase 4) — see `docs/ARCHITECTURE.md`'s Roadmap for the full breakdown.

Phase 5 (UI/UX polish over the full feature set, kept behind `RepoClient` so it works
unmodified in both the Tauri app and the future VSCode webview) is underway. Its
foundation sub-project is complete: design tokens with light/dark theming, four layout
primitives (`Panel`, `SplitView`, `Toolbar`, `ListRow`) under
`frontend/src/components/primitives/`, `lucide-react` iconography, and a reskin of the
core commit-review loop (`CommitBox`, `CommitGraph`, `DiffPane`/`DiffView`/`BlameView`).
Remaining: the rollout of that system to every other component, and a new app icon —
see `docs/superpowers/specs/2026-08-18-browsitory-phase5-design.md` and its plans.

Phase 6 (shipping Browsitory as a VSCode extension) is complete: a `crates/vscode-sidecar`
binary that gives the extension host the same `repo-service` functionality the Tauri app has,
an `extension/` VSCode extension (host + `postMessage`-based webview bridge, following the same
`RepoClient` pattern the Tauri app uses so the existing frontend components are reused
unmodified), recoverable sidecar-process lifecycle handling, four target-specific VSIX
package/release artifacts, and a `@vscode/test-electron`-based E2E layer (`extension/e2e/`)
wired into CI — see `docs/ARCHITECTURE.md`'s Roadmap for the sub-phase (a-e) breakdown.

## Versioning & releases

Semantic versioning (`MAJOR.MINOR.PATCH`). Pushing a `release/MAJOR.MINOR.PATCH` tag (no `v`
prefix) triggers `.github/workflows/release.yml`, which builds standalone distributables for
Linux/macOS/Windows (via `tauri-apps/tauri-action`, same underlying build as
`scripts/build-dist.*`) and publishes them as a draft GitHub Release. The workflow stamps that
version into `crates/tauri-app/tauri.conf.json` at build time — no manual version bump needed
before tagging. `cargo test --workspace` and the frontend test suite must pass on the tagged
commit before the platform builds run.

## Architecture

See `docs/ARCHITECTURE.md` for the full crate/package layout, the `RepoClient` IPC boundary,
and the threading model. Summary: `crates/git-core` (git2, UI-agnostic, DI'd per function,
tested against real temp-dir repos) + `crates/config` (TOML-backed recent-repos registry) +
`crates/repo-service` (transport-agnostic worker threads, credentials, forge/PR API access) +
`crates/tauri-app` (a thin Tauri command adapter over `repo-service`) + `crates/vscode-sidecar`
(a sibling adapter over `repo-service` for the VSCode extension target) + `frontend/` (React/TS,
talks to the backend only through `frontend/src/ipc/RepoClient.ts`) + `extension/` (the VSCode
extension host and webview bridge, talking to `vscode-sidecar` over the same `RepoClient`
interface via a second `RepoClient` implementation).

Building `tauri-app` standalone (no dev server) requires the `custom-protocol` Cargo feature —
see the "Commands" section's E2E block and `crates/tauri-app/Cargo.toml`'s comment on it. Plain
`cargo build --workspace` always leaves the binary looking for the Vite dev server, regardless
of debug/release.

### git2 API gotchas

- `StatusEntry::path()`, `Signature::name()`/`email()`, and `Reference::shorthand()` return
  `Result<&str, Error>` — never `Option`, not even nested. Handle with `let Ok(x) = ... else {
  continue };` in a loop, or `.ok().unwrap_or_default()` otherwise (no `.flatten()` — there's no
  `Option` layer to flatten; it won't compile). See `crates/git-core/src/status.rs`,
  `crates/git-core/src/log.rs`.
- `Commit::summary()` is the one in this family that's actually `Result<Option<&str>, Error>` —
  `Ok(None)` means no summary, not an error. This is the one that wants
  `.ok().flatten().unwrap_or_default()`. See `crates/git-core/src/log.rs`.
- All of the above verified against the vendored `git2` 0.21 source — don't assume a shared
  shape across "similar-sounding" accessors; check the actual signature, since this crate mixes
  both shapes and an incorrect assumption in either direction fails to compile with an
  unhelpful-looking error at the call site (a plain-`Result` accessor rejects `.flatten()`,
  which is where this note came from).
- `StringArray::iter()` (from `Repository::remotes()`) yields `Result<Option<&str>, Error>`
  per slot — needs `.iter().flatten().flatten()`, not a single `.flatten()`, once remote
  support is added.

### Threading model

`git2::Repository` **is** `Send` but is **not** `Sync`. It can be moved into one thread and
owned there (that's why `Worker::spawn`'s `thread::spawn(move || …)` compiles), but a
`&Repository` can never be shared across threads. Tauri managed state requires `Send + Sync`,
so a `Repository` can't be `State` directly, and putting it behind `State<Mutex<Repository>>`
would serialize every command on one lock held across blocking git work. The response to
`!Sync` is therefore message-passing to a single owning thread:
`crates/repo-service/src/worker/mod.rs`'s `Worker::spawn` opens the repository on a dedicated
thread and owns it for that thread's lifetime; Tauri commands (`crates/tauri-app/src/commands.rs`)
send `Command`s over an `mpsc` channel and get replies over a per-call reply channel. UI code
never touches `git-core` directly — only through `RepoClient` → a Tauri command → the worker
thread.

### `RepoClient`: why it exists

`frontend/src/ipc/RepoClient.ts` is the only interface `frontend/src/components` and
`frontend/src/state` are allowed to depend on for backend calls.
`frontend/src/ipc/tauriRepoClient.ts` is the only file that imports `@tauri-apps/api`; a
`no-restricted-imports` override in `frontend/eslint.config.js` fails `pnpm lint` if any file
under `src/components/` or `src/state/` imports `@tauri-apps/*` directly. When a
VSCode extension frontend is built later, it gets a second implementation
(`frontend/src/ipc/vscodeRepoClient.ts`, over `postMessage`) behind the same interface — no
changes to any component.

## License policy

Permissive dependencies only (MIT, Apache-2.0, ISC, BSD, MIT-0) with **one explicit, deliberate
exception**: `git2` links against libgit2 (via vendored build), which is
GPL-2.0-with-linking-exception — not MIT, but the linking exception explicitly permits linking
from differently-licensed code. Verify new dependencies (`cargo info <crate>` / `npm info
<package>`) before adding them and record them in `docs/LICENSE_COMPLIANCE.md`.

## Testing conventions

- `git-core` tests live in `crates/git-core/tests/*.rs` (one file per module) plus a shared
  `tests/common/mod.rs` helper. They use real repos via `git2::Repository::init`/`TempDir`,
  never a mocked `Repository`.
- `repo-service` tests live inline (`#[cfg(test)] mod tests`) next to the code they test (see
  `worker/mod.rs`), against real temp-dir repos, and also own the DTO wire-format pinning:
  `crates/repo-service/src/lib.rs`'s `wire_format_tests` module asserts the `StatusKind` and
  `DiffLineOrigin` strings it emits match the matching unions in
  `frontend/src/ipc/RepoClient.ts`, since nothing else catches that drift.
- `tauri-app` tests live inline (`#[cfg(test)] mod tests`) next to the code they test (see
  `commands/mod.rs`). It's now a thin adapter over `repo-service`, so pass-through Tauri
  commands don't need their own tests — the `git-core`/`repo-service` logic they call already
  is tested. What remains is its own DTO serialization (e.g. camelCase field names), covered by
  `commands/mod.rs`'s test module.
- `frontend` tests mock `RepoClient` (a real interface seam), never `@tauri-apps/api`.
- `e2e/` holds `tauri-driver` + WebdriverIO specs (`e2e/specs/*.spec.ts`) that drive the real
  built `tauri-app` binary as a black box, one flow per major feature area (currently: open
  repo → stage a file → commit → see it in history). Run separately from `cargo test`/`pnpm
  test` — it needs a debug build with the `custom-protocol` feature and a frontend build with
  `VITE_E2E_REPO_PATH` baked in first; see the "Commands" section above for the exact sequence
  (mirrors `.github/workflows/ci.yml`'s `e2e` job, the source of truth if this drifts).

## Task workflow

This repo uses the `superpowers` plugin's `test-driven-development`, `writing-plans`,
`subagent-driven-development`, and `executing-plans` skills for all implementation work, plus
the project-local `.claude/skills/browsitory-conventions` skill for the Browsitory-specific
conventions those global skills don't cover (real temp-dir repos in tests, the `RepoClient`
transport-isolation rule, task-file naming). That skill is a pointer into this file and
`docs/ARCHITECTURE.md`, which stay authoritative if the two ever disagree. New implementation
tasks (Phase 1 onward) follow `docs/TASK_TEMPLATE.md`.

<!-- rtk-instructions v2 -->
# RTK (Rust Token Killer) - Token-Optimized Commands

## Golden Rule

**Always prefix commands with `rtk`**. If RTK has a dedicated filter, it uses it. If not, it passes through unchanged. This means RTK is always safe to use.

**Important**: Even in command chains with `&&`, use `rtk`:
```bash
# ❌ Wrong
git add . && git commit -m "msg" && git push

# ✅ Correct
rtk git add . && rtk git commit -m "msg" && rtk git push
```

## RTK Commands by Workflow

### Build & Compile (80-90% savings)
```bash
rtk cargo build         # Cargo build output
rtk cargo check         # Cargo check output
rtk cargo clippy        # Clippy warnings grouped by file (80%)
rtk tsc                 # TypeScript errors grouped by file/code (83%)
rtk lint                # ESLint/Biome violations grouped (84%)
rtk prettier --check    # Files needing format only (70%)
rtk next build          # Next.js build with route metrics (87%)
```

### Test (60-99% savings)
```bash
rtk cargo test          # Cargo test failures only (90%)
rtk go test             # Go test failures only (90%)
rtk jest                # Jest failures only (99.5%)
rtk vitest              # Vitest failures only (99.5%)
rtk playwright test     # Playwright failures only (94%)
rtk pytest              # Python test failures only (90%)
rtk rake test           # Ruby test failures only (90%)
rtk rspec               # RSpec test failures only (60%)
rtk test <cmd>          # Generic test wrapper - failures only
```

### Git (59-80% savings)
```bash
rtk git status          # Compact status
rtk git log             # Compact log (works with all git flags)
rtk git diff            # Compact diff (80%)
rtk git show            # Compact show (80%)
rtk git add             # Ultra-compact confirmations (59%)
rtk git commit          # Ultra-compact confirmations (59%)
rtk git push            # Ultra-compact confirmations
rtk git pull            # Ultra-compact confirmations
rtk git branch          # Compact branch list
rtk git fetch           # Compact fetch
rtk git stash           # Compact stash
rtk git worktree        # Compact worktree
```

Note: Git passthrough works for ALL subcommands, even those not explicitly listed.

### GitHub (26-87% savings)
```bash
rtk gh pr view <num>    # Compact PR view (87%)
rtk gh pr checks        # Compact PR checks (79%)
rtk gh run list         # Compact workflow runs (82%)
rtk gh issue list       # Compact issue list (80%)
rtk gh api              # Compact API responses (26%)
```

### JavaScript/TypeScript Tooling (70-90% savings)
```bash
rtk pnpm list           # Compact dependency tree (70%)
rtk pnpm outdated       # Compact outdated packages (80%)
rtk pnpm install        # Compact install output (90%)
rtk npm run <script>    # Compact npm script output
rtk npx <cmd>           # Compact npx command output
rtk prisma              # Prisma without ASCII art (88%)
rtk uv run <cmd>        # Compact uv project command output
```

### Files & Search (60-75% savings)
```bash
rtk ls <path>           # Tree format, compact (65%)
rtk read <file>         # Code reading with filtering (60%)
rtk grep <pattern>      # Search grouped by file (75%). Format flags (-c, -l, -L, -o, -Z) run raw.
rtk find <pattern>      # Find grouped by directory (70%)
```

### Analysis & Debug (70-90% savings)
```bash
rtk err <cmd>           # Filter errors only from any command
rtk log <file>          # Deduplicated logs with counts
rtk json <file>         # JSON structure without values
rtk deps                # Dependency overview
rtk env                 # Environment variables compact
rtk summary <cmd>       # Smart summary of command output
rtk diff                # Ultra-compact diffs
```

### Infrastructure (85% savings)
```bash
rtk docker ps           # Compact container list
rtk docker images       # Compact image list
rtk docker logs <c>     # Deduplicated logs
rtk kubectl get         # Compact resource list
rtk kubectl logs        # Deduplicated pod logs
```

### Network (65-70% savings)
```bash
rtk curl <url>          # Compact HTTP responses (70%)
rtk wget <url>          # Compact download output (65%)
```

### Meta Commands
```bash
rtk gain                # View token savings statistics
rtk gain --history      # View command history with savings
rtk discover            # Analyze Claude Code sessions for missed RTK usage
rtk proxy <cmd>         # Run command without filtering (for debugging)
rtk init                # Add RTK instructions to CLAUDE.md
rtk init --global       # Add RTK to ~/.claude/CLAUDE.md
```

## Token Savings Overview

| Category | Commands | Typical Savings |
|----------|----------|-----------------|
| Tests | vitest, playwright, cargo test | 90-99% |
| Build | next, tsc, lint, prettier | 70-87% |
| Git | status, log, diff, add, commit | 59-80% |
| GitHub | gh pr, gh run, gh issue | 26-87% |
| Package Managers | pnpm, npm, npx | 70-90% |
| Files | ls, read, grep, find | 60-75% |
| Infrastructure | docker, kubectl | 85% |
| Network | curl, wget | 65-70% |

Overall average: **60-90% token reduction** on common development operations.
<!-- /rtk-instructions -->

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
