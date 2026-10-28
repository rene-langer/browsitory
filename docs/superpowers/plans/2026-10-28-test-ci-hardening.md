# Test & CI Hardening Plan

**Goal:** Close #111 (Rust tests leak the developer's / CI runner's global git config), #103
(E2E specs compare a `Promise` to a number; `e2e/` and `extension/e2e/` never type-checked in
CI), #145 (stale-element flake in `not.toBeExisting()`), #153 (flaky
`signing_ssh::unlisted_ssh_signer_is_good_untrusted`), plus a sweep of frontend App-level tests
that fire keys before the active workspace's listener mounts (#135 pattern).

**Branch:** `chore/test-ci-hardening` (pushed). No PR from this task.

**No user-visible change.** CHANGELOG gets one short "Internal" entry only because
`scripts/check-changelog.py` requires one for any `crates/` or `e2e/` change.

---

## Task 1 — Hermetic git config for every Rust test binary (#111)

- New dev-only workspace crate `crates/test-support` (`publish = false`), dev-dependency of
  `git-core`, `repo-service`, `vscode-sidecar`, `tauri-app`. Never a runtime dependency, so
  `cargo run` / the app / the sidecar are unaffected (rejected: `.cargo/config.toml` `[env]`,
  which would also change `cargo run` and the real app).
- `test_support::hermetic_git!()` expands, inside each test binary, to a `#[ctor]` constructor
  calling the `Once`-guarded `test_support::install()` **before `main`**, i.e. before the test
  harness starts threads — the only point where setting env vars is sound. It:
  - points libgit2's Global/XDG/System/ProgramData search paths at an always-empty directory;
  - sets `HOME`, `XDG_CONFIG_HOME`, `GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=<empty file>`
    so spawned `git`, hook scripts and the spawned sidecar binary are hermetic too.
- Directories live at `target/<profile>/browsitory-hermetic-git/` (next to the test binaries)
  and are never deleted during a run — no Windows "'.gitconfig' is locked" race.
- Placement: `crates/git-core/tests/common/mod.rs`, the two git-core test files without
  `common`, `vscode-sidecar/tests/protocol_roundtrip.rs`, and `#[cfg(test)]` at each crate root.
- Remove `stage_commit.rs`'s racy `ConfigSearchPathOverride` and `tools.rs`'s `isolate()` —
  both subsumed. Tests needing a global config build their own `git2::Config` with
  `add_file(…, ConfigLevel::Global, …)` (already the case in `tools.rs`) or pass `HOME` to a
  child (already the case in the sidecar tool-settings test).
- `ctor` 0.8 (MIT OR Apache-2.0, already in `Cargo.lock` via tauri) → row in
  `docs/LICENSE_COMPLIANCE.md`.

**TDD:** `crates/test-support/tests/hermetic.rs` — `Config::open_default()` lists no entries
and `git config --list --show-scope` shows no global/system entries (fails without the guard
under a hostile `HOME`). Plus: the whole workspace fails under a hostile global config before
(`pick.rs` ×15 on `init.defaultBranch=trunk`, a `repo-service` push test) and passes after.

**Verify:** `cargo test --workspace` normally and with `HOME`/`XDG_CONFIG_HOME`/
`GIT_CONFIG_GLOBAL` pointing at a config setting `core.autocrlf=true`, `fetch.prune=true`,
`merge.ff=only`, `commit.gpgSign=true`, `core.hooksPath=/nonexistent`,
`init.defaultBranch=trunk`, `pull.rebase=true`, `core.ignorecase=true` (files under
`target/`, never the real `~/.gitconfig`).

## Task 2 — #153 signing flake

Reproduce: loop the test alone (200×) and the whole `signing_ssh` binary 16-way parallel.
Fix the root cause (see commit message / report), not a retry.

## Task 3 — E2E type-checking (#103)

- Add the missing `await`s in `e2e/specs/hunk-staging.spec.ts:47`, `multi-repo.spec.ts:23,57`.
- `e2e` and `extension/e2e` `tsc --noEmit` clean; add a `typecheck` step to the existing `e2e`
  and `e2e-vscode` CI jobs (after their installs; no new job).

## Task 4 — Stale-safe absence waits (#145)

Replace `expect(el).not.toBeExisting()` after a row-removing action with
`el.waitForExist({ reverse: true })` in `line-staging.spec.ts:57` and every other spec using
the same pattern.

## Task 5 — Frontend key-before-listener sweep (#135 pattern)

Grep App-level tests for `fireEvent.keyDown(window|document…)`; where a test fires a key right
after rendering `App` without waiting for `[data-active-repo="true"]`, add that wait. Leave
already-safe tests alone.

## Gate

`cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test
--workspace` (normal + hostile), frontend `lint` / `test --run` ×3 / `build`, extension `test
--run` + `compile`, `e2e` + `extension/e2e` typecheck, Tauri + VSCode E2E full suites under
`xvfb-run -a` with `XDG_*` in `target/`, `python3 scripts/check-changelog.py`.
