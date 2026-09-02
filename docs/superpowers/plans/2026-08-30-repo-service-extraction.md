# Repo-Service Extraction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract the transport-agnostic git service layer (per-repo worker threads, credential
storage, forge/PR API access) out of `tauri-app` into a new crate, `crates/repo-service`, with
zero behavior change — this is Phase 6 sub-phase (a) of shipping Browsitory as a VSCode
extension: both `tauri-app` and the future JSON-RPC sidecar will depend on this one crate
instead of re-implementing ~85 methods' dispatch logic twice.

**Architecture:** `crates/tauri-app/src/worker/` (the `Command` enum, `Worker::spawn`, the
per-repo owning thread, and all 12 `worker/*.rs` dispatch submodules), plus the two files it
already depends on via `crate::`-relative paths (`credentials.rs`, `pull_requests.rs`), move
into `crates/repo-service` as a single unit — their internal `crate::` references stay valid
unchanged because the whole subtree relocates together. `tauri-app`'s `commands/` module
becomes a strictly thinner Tauri adapter: it depends on `repo-service` as an external crate and
loses its own copies of this logic. No new logic is written; this is a pure move-and-rewire
refactor, verified at every task boundary by `cargo build --workspace` and
`cargo test --workspace` passing exactly as before.

**Tech Stack:** Rust, Cargo workspaces, git2 0.21, keyring 4.1.6, reqwest 0.12 (blocking).

**Spec:** `docs/superpowers/specs/2026-08-30-vscode-extension-design.md`

## Global Constraints

- Desktop-only VSCode extension target (spec's "Constraints / decisions") — not relevant to
  this particular plan's Rust-only scope, but repo-service's dependency set must stay
  desktop-compatible (no wasm/web restrictions to worry about here).
- The move must not change behavior: every existing test in `tauri-app` (`cargo test -p
  tauri-app`, soon to be split between `tauri-app` and `repo-service`) must still pass, and
  `cargo build --workspace` / `cargo test --workspace` must succeed after every task.
- Follow this repo's existing conventions: `crates/git-core` tests are real temp-dir repos, no
  mocks (`CLAUDE.md`'s Testing conventions) — the moved `worker/mod.rs` tests already follow
  this and must keep doing so unchanged.
- Commit after each task (this repo's existing per-task-commit convention; see recent commits
  on `main` for message style — this plan uses `refactor(...)` prefixes since no behavior
  changes).

---

### Task 1: Scaffold the `repo-service` crate

**Files:**
- Create: `crates/repo-service/Cargo.toml`
- Create: `crates/repo-service/src/lib.rs`
- Modify: `Cargo.toml:1-6` (workspace root — add the new member)

**Interfaces:**
- Produces: an empty, independently-buildable `repo-service` crate that Task 2 populates.

- [ ] **Step 1: Create the crate's `Cargo.toml`**

```toml
[package]
name = "repo-service"
version = "0.1.0"
edition = "2021"
license = "MIT"

[features]
# Mirrors tauri-app's own feature of the same name (see that crate's Cargo.toml): swaps
# Worker::spawn's credential store from KeyringCredentialStore to InMemoryCredentialStore and
# lets pull_requests.rs's forge API base URLs be redirected via env vars, so an E2E build never
# touches a real OS keychain or a live GitHub/Bitbucket account. tauri-app's own flag forwards
# to this one (see Task 2, Step 6).
forge-fixture-override = []

[dependencies]
git-core = { path = "../git-core" }
git2 = "0.21"
keyring = { version = "4.1.6", features = ["v1"] }
reqwest = { version = "0.12", default-features = false, features = ["blocking", "rustls-tls", "json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
url = "2.5"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 2: Create a placeholder `lib.rs`**

```rust
//! Transport-agnostic git service layer: per-repo worker threads, credential storage, and
//! forge (GitHub/Bitbucket) pull-request API access. Shared by every `RepoClient` transport —
//! the Tauri desktop app today, a JSON-RPC sidecar for the VSCode extension later. See
//! `docs/superpowers/specs/2026-08-30-vscode-extension-design.md`.
```

- [ ] **Step 3: Register the crate in the workspace**

Edit `Cargo.toml` (repo root):

```toml
[workspace]
resolver = "2"
members = [
    "crates/git-core",
    "crates/config",
    "crates/repo-service",
    "crates/tauri-app",
]
```

- [ ] **Step 4: Verify it builds**

Run: `cargo build -p repo-service`
Expected: succeeds (one crate, one doc-comment-only file).

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/repo-service
git commit -m "refactor(repo-service): scaffold empty crate"
```

---

### Task 2: Move `worker/`, `credentials.rs`, `pull_requests.rs` into `repo-service`

**Files:**
- Move: `crates/tauri-app/src/worker/` → `crates/repo-service/src/worker/` (directory, all 12
  files: `mod.rs`, `branch.rs`, `forge.rs`, `merge.rs`, `rebase.rs`, `reflog.rs`, `remote.rs`,
  `stash.rs`, `status.rs`, `submodule.rs`, `tag.rs`, `worktree.rs`)
- Move: `crates/tauri-app/src/credentials.rs` → `crates/repo-service/src/credentials.rs`
- Move: `crates/tauri-app/src/pull_requests.rs` → `crates/repo-service/src/pull_requests.rs`
- Modify: `crates/repo-service/src/lib.rs`
- Modify: `crates/repo-service/src/worker/mod.rs` (one visibility bump)
- Modify: `crates/tauri-app/Cargo.toml`
- Modify: `crates/tauri-app/src/main.rs`
- Modify: `crates/tauri-app/src/commands/mod.rs` (3 reference sites)

**Interfaces:**
- Consumes: nothing from Task 1 beyond the empty crate shell.
- Produces: `repo_service::worker::{Worker, WorkerHandle, TransferEvent}`,
  `repo_service::credentials::*`, `repo_service::pull_requests::*` — the public surface every
  later sub-phase (b)'s JSON-RPC sidecar, and `tauri-app`'s `commands/` module today, depend on.

None of the three moving items need their own internal `use` paths touched: `worker/mod.rs`,
`worker/forge.rs`, and `worker/remote.rs` reference `crate::credentials::...` and
`crate::pull_requests::...`, and those stay correct unchanged because `credentials.rs` and
`pull_requests.rs` move into the *same* new crate alongside them — `crate::` still resolves to
`repo-service` after the move for all three files at once. Only the boundary *between*
`tauri-app` and `repo-service` needs edits (Steps 5-8 below).

- [ ] **Step 1: Move the three files/directories with `git mv`**

```bash
git mv crates/tauri-app/src/worker crates/repo-service/src/worker
git mv crates/tauri-app/src/credentials.rs crates/repo-service/src/credentials.rs
git mv crates/tauri-app/src/pull_requests.rs crates/repo-service/src/pull_requests.rs
```

- [ ] **Step 2: Declare the moved modules in `repo-service`'s `lib.rs`**

Append to `crates/repo-service/src/lib.rs`:

```rust
pub mod credentials;
pub mod pull_requests;
pub mod worker;
```

- [ ] **Step 3: Bump `TransferEvent`'s visibility**

In `crates/repo-service/src/worker/mod.rs`, find:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TransferEvent {
```

Change to:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferEvent {
```

(It was `pub(crate)` because only `tauri-app`'s own `commands/mod.rs` used it, in the same
crate. `commands/mod.rs` now lives in a different crate from `TransferEvent`, so it needs full
`pub` visibility to stay reachable — Step 8 below is where `commands/mod.rs` starts importing
it as `repo_service::worker::TransferEvent`.)

- [ ] **Step 4: Verify `repo-service` builds and its own tests still pass standalone**

Run: `cargo test -p repo-service`
Expected: compiles and all of `worker/mod.rs`'s existing `#[cfg(test)] mod tests` (the
`FakeCredentialStore`/`FakeForgeApi`-backed tests, `get_status_reflects_an_untracked_file`,
etc.) plus any inline tests in `credentials.rs`/`pull_requests.rs` pass — same tests, same
assertions, just running under a different crate name. At this point `tauri-app` will *not*
yet build (its `mod worker;`/`mod credentials;`/`mod pull_requests;` declarations point at
files that no longer exist there) — that's expected and fixed by the remaining steps.

- [ ] **Step 5: Point `tauri-app` at the new crate**

In `crates/tauri-app/Cargo.toml`, add to `[dependencies]`:

```toml
repo-service = { path = "../repo-service" }
```

Remove these three now-unused dependencies from the same `[dependencies]` block (nothing left
in `tauri-app`'s own source references `keyring::`, `reqwest::`, or `url::` directly — only
`git2`, which `commands/mod.rs`'s own tests still use directly, must stay):

```toml
keyring = { version = "4.1.6", features = ["v1"] }
reqwest = { version = "0.12", default-features = false, features = ["blocking", "rustls-tls", "json"] }
url = "2.5"
```

In the same file's `[features]` block, change:

```toml
forge-fixture-override = []
```

to:

```toml
forge-fixture-override = ["repo-service/forge-fixture-override"]
```

- [ ] **Step 6: Remove the now-nonexistent module declarations from `main.rs`**

In `crates/tauri-app/src/main.rs`, change:

```rust
mod commands;
mod credentials;
mod pull_requests;
mod worker;
```

to:

```rust
mod commands;
```

- [ ] **Step 7: Fix `commands/mod.rs`'s three references to the old in-crate `worker` module**

Reference 1 — the top-of-file import. Change:

```rust
use crate::worker::{TransferEvent, Worker};
```

to:

```rust
use repo_service::worker::{TransferEvent, Worker};
```

Reference 2 — `worker_handle`'s return type. Change:

```rust
fn worker_handle(
    state: &State<AppState>,
    repo_path: &str,
) -> Result<crate::worker::WorkerHandle, String> {
```

to:

```rust
fn worker_handle(
    state: &State<AppState>,
    repo_path: &str,
) -> Result<repo_service::worker::WorkerHandle, String> {
```

Reference 3 — inside `#[cfg(test)] mod tests`, the module-level import. Change:

```rust
    use crate::worker::TransferEvent;
```

to:

```rust
    use repo_service::worker::TransferEvent;
```

- [ ] **Step 8: Fix the one inline `use` inside the `two_open_repos_have_independent_worker_state` test**

In the same test module, that test currently starts with:

```rust
    #[test]
    fn two_open_repos_have_independent_worker_state() {
        use crate::worker::Worker;
        use std::collections::HashMap;
```

Change the first `use` line to:

```rust
        use repo_service::worker::Worker;
```

- [ ] **Step 9: Verify the full workspace builds and every existing test still passes**

Run: `cargo build --workspace`
Expected: succeeds.

Run: `cargo test --workspace`
Expected: every test that passed before this task still passes — `worker/mod.rs`'s tests now
run under `repo-service`, `commands/mod.rs`'s tests (DTO serialization, wire-format pinning,
`two_open_repos_have_independent_worker_state`, etc.) still run under `tauri-app`, all green.

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml crates/repo-service crates/tauri-app
git commit -m "refactor(repo-service): move worker, credentials, pull_requests out of tauri-app"
```

---

### Task 3: Relocate the two portable wire-format pinning tests

**Files:**
- Modify: `crates/tauri-app/src/commands/mod.rs` (remove 2 tests + their helper functions)
- Modify: `crates/repo-service/src/lib.rs` (add the same 2 tests)

**Interfaces:**
- Consumes: `git_core::status::StatusKind`, `git_core::diff::DiffLineOrigin` — both already
  public in `git-core`, already a `repo-service` dependency from Task 1.
- Produces: nothing new for later tasks — this is a test-only relocation.

`commands/mod.rs`'s test module has six `*_wire_values_match_the_typescript_union` tests, but
only two of them test a git-core enum's raw `Debug` output with zero dependency on any
tauri-app-local DTO or helper function: `status_kind_wire_values_match_the_typescript_union`
(pins `format!("{:?}", StatusKind::...)`) and `diff_line_origin_wire_values_match_the_typescript_union`
(pins `format!("{:?}", DiffLineOrigin::...)`). These are the ones CLAUDE.md's Testing
conventions section means by "a contract no other test covers" — and they're exactly the kind
of check a future JSON-RPC sidecar adapter needs inherited for free, since it will serialize
the same git-core enums the same way. The other four
(`remote_auth_mode_wire_values_match_the_typescript_union`,
`forge_provider_wire_values_match_the_typescript_union`,
`pull_outcome_wire_values_match_the_typescript_union`,
`transfer_phase_wire_values_match_the_typescript_union`) assert on `RemoteAuthModeDto`,
`ForgeProviderDto`, `PullOutcomeDto`, and `transfer_event_payload`'s output respectively — all
`tauri-app`-local DTO/helper machinery — and stay exactly where they are.

- [ ] **Step 1: Remove the two tests and their helper functions from `commands/mod.rs`**

Delete this whole block (helper function + test) from the `#[cfg(test)] mod tests` block:

```rust
    /// The `kind` field of `StatusEntryDto` is produced by `format!("{:?}", kind)`, so the
    /// `Debug` output *is* the wire format. Its counterpart contract is the `StatusKind`
    /// union in `frontend/src/ipc/RepoClient.ts`
    /// (`"New" | "Modified" | "Deleted" | "Renamed" | "TypeChange"`) — these must stay in
    /// sync. The match below is exhaustive on purpose: adding a `StatusKind` variant breaks
    /// compilation here, which is the reminder to extend the TypeScript union too.
    fn expected_wire_value(kind: StatusKind) -> &'static str {
        match kind {
            StatusKind::New => "New",
            StatusKind::Modified => "Modified",
            StatusKind::Deleted => "Deleted",
            StatusKind::Renamed => "Renamed",
            StatusKind::TypeChange => "TypeChange",
            StatusKind::Conflicted => "Conflicted",
        }
    }

    #[test]
    fn status_kind_wire_values_match_the_typescript_union() {
        for kind in [
            StatusKind::New,
            StatusKind::Modified,
            StatusKind::Deleted,
            StatusKind::Renamed,
            StatusKind::TypeChange,
            StatusKind::Conflicted,
        ] {
            assert_eq!(format!("{:?}", kind), expected_wire_value(kind));
        }
    }

    /// `DiffLineDto::origin` is produced by `format!("{:?}", origin)`, so the `Debug`
    /// output *is* the wire format. Counterpart contract: the `DiffLineOrigin` union in
    /// `frontend/src/ipc/RepoClient.ts` (`"Add" | "Remove" | "Context"`) — these must stay
    /// in sync. Exhaustive on purpose: adding a `DiffLineOrigin` variant breaks compilation
    /// here, which is the reminder to extend the TypeScript union too.
    fn expected_diff_origin_wire_value(origin: DiffLineOrigin) -> &'static str {
        match origin {
            DiffLineOrigin::Add => "Add",
            DiffLineOrigin::Remove => "Remove",
            DiffLineOrigin::Context => "Context",
        }
    }

    #[test]
    fn diff_line_origin_wire_values_match_the_typescript_union() {
        for origin in [
            DiffLineOrigin::Add,
            DiffLineOrigin::Remove,
            DiffLineOrigin::Context,
        ] {
            assert_eq!(
                format!("{:?}", origin),
                expected_diff_origin_wire_value(origin)
            );
        }
    }
```

- [ ] **Step 2: Verify `tauri-app` still builds after the removal**

Run: `cargo build -p tauri-app --tests`
Expected: succeeds — `git_core::diff::DiffLineOrigin` and `git_core::status::StatusKind` stay
imported at the top of the test module (`DiffLineDto`/`StatusEntryDto` construction elsewhere
in the file still needs them), so no dangling `use` to clean up. If `cargo` reports either
import newly-unused, remove that one `use` line — check before assuming.

- [ ] **Step 3: Add the same two tests to `repo-service`**

Append to `crates/repo-service/src/lib.rs`:

```rust
#[cfg(test)]
mod wire_format_tests {
    use git_core::diff::DiffLineOrigin;
    use git_core::status::StatusKind;

    /// The `kind` field of the status wire DTO is produced by `format!("{:?}", kind)`, so the
    /// `Debug` output *is* the wire format. Its counterpart contract is the `StatusKind`
    /// union in `frontend/src/ipc/RepoClient.ts`
    /// (`"New" | "Modified" | "Deleted" | "Renamed" | "TypeChange"`) — these must stay in
    /// sync. The match below is exhaustive on purpose: adding a `StatusKind` variant breaks
    /// compilation here, which is the reminder to extend the TypeScript union too.
    fn expected_status_kind_wire_value(kind: StatusKind) -> &'static str {
        match kind {
            StatusKind::New => "New",
            StatusKind::Modified => "Modified",
            StatusKind::Deleted => "Deleted",
            StatusKind::Renamed => "Renamed",
            StatusKind::TypeChange => "TypeChange",
            StatusKind::Conflicted => "Conflicted",
        }
    }

    #[test]
    fn status_kind_wire_values_match_the_typescript_union() {
        for kind in [
            StatusKind::New,
            StatusKind::Modified,
            StatusKind::Deleted,
            StatusKind::Renamed,
            StatusKind::TypeChange,
            StatusKind::Conflicted,
        ] {
            assert_eq!(format!("{:?}", kind), expected_status_kind_wire_value(kind));
        }
    }

    /// A diff line's `origin` wire field is produced by `format!("{:?}", origin)`, so the
    /// `Debug` output *is* the wire format. Counterpart contract: the `DiffLineOrigin` union
    /// in `frontend/src/ipc/RepoClient.ts` (`"Add" | "Remove" | "Context"`) — these must stay
    /// in sync. Exhaustive on purpose: adding a `DiffLineOrigin` variant breaks compilation
    /// here, which is the reminder to extend the TypeScript union too.
    fn expected_diff_origin_wire_value(origin: DiffLineOrigin) -> &'static str {
        match origin {
            DiffLineOrigin::Add => "Add",
            DiffLineOrigin::Remove => "Remove",
            DiffLineOrigin::Context => "Context",
        }
    }

    #[test]
    fn diff_line_origin_wire_values_match_the_typescript_union() {
        for origin in [
            DiffLineOrigin::Add,
            DiffLineOrigin::Remove,
            DiffLineOrigin::Context,
        ] {
            assert_eq!(
                format!("{:?}", origin),
                expected_diff_origin_wire_value(origin)
            );
        }
    }
}
```

- [ ] **Step 4: Verify both crates' tests pass**

Run: `cargo test --workspace`
Expected: `repo_service::wire_format_tests::status_kind_wire_values_match_the_typescript_union`
and `...diff_line_origin_wire_values_match_the_typescript_union` pass under `repo-service`;
`tauri-app`'s test count drops by exactly these two, everything else unchanged and green.

- [ ] **Step 5: Commit**

```bash
git add crates/repo-service crates/tauri-app
git commit -m "refactor(repo-service): relocate git-core wire-format pinning tests"
```

---

### Task 4: Final verification pass

**Files:** none (verification only).

**Interfaces:** none — this task confirms Task 1-3's combined result meets every check this
repo's CI and `CLAUDE.md` require before the refactor is considered done.

- [ ] **Step 1: Full workspace build**

Run: `cargo build --workspace`
Expected: succeeds, no warnings about unused `repo-service` items (everything moved is either
used by `commands/mod.rs` or by `repo-service`'s own tests).

- [ ] **Step 2: Full workspace test suite**

Run: `cargo test --workspace`
Expected: all green — same total pass count as before this plan started, just redistributed
across two crates.

- [ ] **Step 3: Clippy, warnings as errors**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean. If the move surfaces a new lint (e.g. an import that's technically unused
after Task 3 trimmed a test), fix it directly rather than suppressing it.

- [ ] **Step 4: Format check**

Run: `cargo fmt --all -- --check`
Expected: clean. If `git mv`-relocated files pick up formatting drift from being re-parsed in
a new crate root (unlikely, but possible with doc-comment line-wrapping), run
`cargo fmt --all` and fold the diff into this task's commit.

- [ ] **Step 5: Confirm the E2E build path still works**

The `e2e/` suite drives the real built `tauri-app` binary — it must still build with the
`custom-protocol` feature now that `tauri-app` depends on `repo-service`:

Run: `cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override`
Expected: succeeds. This is also the first real exercise of the `forge-fixture-override`
feature forwarding added in Task 2 Step 5 — confirm the build doesn't warn about an unused or
unresolved feature flag.

- [ ] **Step 6: Update `docs/ARCHITECTURE.md`'s crate/package layout**

The crate list at the top of `docs/ARCHITECTURE.md` currently reads:

```
browsitory/
├── crates/
│   ├── git-core/    # git2-based service layer, UI-agnostic, unit-tested headlessly
│   ├── config/      # repo registry + preferences: recent-repos list, backed by TOML
│   └── tauri-app/    # Tauri commands + per-repo worker threads
└── frontend/          # React + TypeScript + Vite, the only crate/package that talks to a UI toolkit
```

Update it to:

```
browsitory/
├── crates/
│   ├── git-core/      # git2-based service layer, UI-agnostic, unit-tested headlessly
│   ├── config/        # repo registry + preferences: recent-repos list, backed by TOML
│   ├── repo-service/  # transport-agnostic worker threads, credentials, forge/PR API access
│   └── tauri-app/      # thin Tauri command adapter over repo-service
└── frontend/            # React + TypeScript + Vite, the only crate/package that talks to a UI toolkit
```

Also update the "Threading model" section's references to `crates/tauri-app/src/worker.rs`
(now `crates/repo-service/src/worker/mod.rs`) and `crates/tauri-app/src/commands.rs` (still
correct — the DTO/`AppState`/Tauri-command adapter layer stays there) to point at the new
locations. Read that section first to find the exact sentences before editing — don't
guess at phrasing that isn't there.

- [ ] **Step 7: Commit**

```bash
git add docs/ARCHITECTURE.md
git commit -m "docs(architecture): document the repo-service crate"
```
