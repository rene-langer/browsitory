# Browsitory Phase 0 Setup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the Browsitory Rust+Tauri+React workspace from an empty repo: buildable Cargo workspace, CI, the first tested `git-core` operations, a Tauri shell wired to the frontend through a `RepoClient` IPC abstraction, and the process docs (CLAUDE.md, ARCHITECTURE.md, task template, project skill) that govern everything built after this plan.

**Architecture:** See `docs/superpowers/specs/2026-08-11-browsitory-architecture-design.md`. Three Rust crates (`git-core`, `config`, `tauri-app`) plus a React/TypeScript `frontend/`, connected only through a `RepoClient` interface so the frontend can later be reused as a VSCode webview without a rewrite.

**Tech Stack:** Rust (stable), git2 0.21, thiserror, tempfile (dev), Tauri 2, React 18 + TypeScript + Vite, Vitest + Testing Library, pnpm.

## Global Constraints

- License: MIT-only dependencies, with one documented exception — git2's vendored libgit2 is GPL-2.0-with-linking-exception. No other GPL/AGPL/SSPL dependency is acceptable without a new explicit decision.
- TDD mandatory: `git-core`/`config` tests always use real temp-dir repos/files (`tempfile`), never a mocked `git2::Repository`. Frontend tests mock the `RepoClient` interface, never `@tauri-apps/api` directly.
- UI code (`frontend/src/components`, `frontend/src/state`) never imports a transport (`@tauri-apps/api`, `postMessage`) directly — only `frontend/src/ipc/*` does.
- `git-core` functions take `&git2::Repository` (or a path) as an explicit argument, never a singleton — see the spec's "git-core is dependency-injected on purpose" rationale, carried forward from the prior Rust pass.
- Commit messages use Conventional Commits prefixes (`feat:`, `fix:`, `docs:`, `test:`, `chore:`, `refactor:`).

---

## File Structure

```
browsitory/
├── Cargo.toml                          # workspace manifest (git-core, config, tauri-app)
├── rust-toolchain.toml
├── LICENSE                             # MIT
├── .github/workflows/ci.yml
├── crates/
│   ├── git-core/
│   │   ├── Cargo.toml
│   │   ├── src/lib.rs
│   │   ├── src/repo.rs                 # open()
│   │   ├── src/status.rs               # status(), StatusEntry, StatusKind
│   │   └── tests/
│   │       ├── common/mod.rs           # init_repo(), write_file()
│   │       ├── repo.rs
│   │       └── status.rs
│   ├── config/
│   │   ├── Cargo.toml
│   │   └── src/lib.rs                  # stub, implemented in a later phase
│   └── tauri-app/
│       ├── Cargo.toml
│       ├── tauri.conf.json
│       ├── build.rs
│       ├── src/main.rs
│       ├── src/worker.rs               # Worker: per-repo thread + command/reply channel
│       └── src/commands.rs             # open_repo, get_status Tauri commands + DTOs
├── frontend/
│   ├── package.json
│   ├── vite.config.ts
│   ├── tsconfig.json
│   ├── index.html
│   └── src/
│       ├── main.tsx
│       ├── App.tsx
│       ├── ipc/
│       │   ├── RepoClient.ts           # interface + StatusEntry type
│       │   └── tauriRepoClient.ts      # Tauri invoke()-based implementation
│       └── components/
│           ├── StatusView.tsx
│           └── StatusView.test.tsx
└── docs/
    ├── superpowers/specs/2026-08-11-browsitory-architecture-design.md   # already exists
    ├── ARCHITECTURE.md
    ├── LICENSE_COMPLIANCE.md
    └── TASK_TEMPLATE.md
```

`CLAUDE.md` is at the repo root (already exists as a stub from a prior session — Task 4 rewrites it).

---

### Task 1: Cargo + frontend workspace scaffold, LICENSE, CI

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `LICENSE`, `.github/workflows/ci.yml`
- Create: `crates/git-core/Cargo.toml` (deps only, no source yet — Task 2 adds `src/`)
- Create: `crates/config/Cargo.toml`, `crates/config/src/lib.rs`
- Create: `frontend/` (via `pnpm create vite`), then add `vitest`, `@testing-library/react`, `eslint` scripts

**Interfaces:**
- Produces: a workspace where `cargo build --workspace` and `cargo test --workspace` succeed with zero crates yet implementing real logic, and `pnpm install && pnpm build && pnpm lint` succeed in `frontend/`. Task 2 depends on `crates/git-core`'s `Cargo.toml` existing with `git2`/`thiserror`/`tempfile` already declared.

- [ ] **Step 1: Create the workspace manifest and toolchain pin**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = [
    "crates/git-core",
    "crates/config",
]
```

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "stable"
```

- [ ] **Step 2: Add the MIT LICENSE**

`LICENSE` (fill `<year>`/`<name>` with the current year and the repo owner's name):
```
MIT License

Copyright (c) <year> <name>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 3: Scaffold `crates/git-core` (manifest only)**

`crates/git-core/Cargo.toml`:
```toml
[package]
name = "git-core"
version = "0.1.0"
edition = "2021"
license = "MIT"

[dependencies]
git2 = "0.21"
thiserror = "1.0"

[dev-dependencies]
tempfile = "3"
```

Also create an empty `crates/git-core/src/lib.rs` containing only `// implemented in Task 2` so `cargo build --workspace` succeeds — Task 2 replaces this file's contents.

- [ ] **Step 4: Scaffold `crates/config` (stub crate)**

`crates/config/Cargo.toml`:
```toml
[package]
name = "config"
version = "0.1.0"
edition = "2021"
license = "MIT"
```

`crates/config/src/lib.rs`:
```rust
// Repo registry and user preferences, implemented in a later phase.
```

- [ ] **Step 5: Verify the Rust workspace builds**

Run: `cargo build --workspace`
Expected: succeeds, builds `git-core` and `config` with no warnings.

- [ ] **Step 6: Scaffold the frontend with Vite**

Run: `pnpm create vite@latest frontend -- --template react-ts`

Then, inside `frontend/`, add test tooling:

Run: `pnpm add -D vitest @testing-library/react @testing-library/jest-dom jsdom`

Add to `frontend/package.json`'s `"scripts"`:
```json
"test": "vitest",
"lint": "eslint ."
```

Add to `frontend/vite.config.ts` (merge into the existing Vite config generated by the template):
```ts
/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    globals: true,
  },
});
```

- [ ] **Step 7: Verify the frontend builds, lints, and tests run**

Run (inside `frontend/`): `pnpm install && pnpm build && pnpm lint && pnpm test -- --run`
Expected: all four succeed (zero tests is fine — no `.test.tsx` files exist yet).

- [ ] **Step 8: Add the CI workflow**

`.github/workflows/ci.yml`:
```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  rust:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - name: Format check
        run: cargo fmt --all -- --check
      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: Test
        run: cargo test --workspace

  frontend:
    runs-on: ubuntu-latest
    defaults:
      run:
        working-directory: frontend
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: pnpm
          cache-dependency-path: frontend/pnpm-lock.yaml
      - run: pnpm install --frozen-lockfile
      - run: pnpm lint
      - run: pnpm build
      - run: pnpm test -- --run
```

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml rust-toolchain.toml LICENSE .github/workflows/ci.yml \
        crates/git-core/Cargo.toml crates/git-core/src/lib.rs \
        crates/config frontend
git commit -m "chore: scaffold Cargo workspace, frontend, LICENSE, and CI"
```

---

### Task 2: `git-core::repo` + `git-core::status` (TDD)

**Files:**
- Create: `crates/git-core/tests/common/mod.rs`
- Create: `crates/git-core/tests/repo.rs`
- Create: `crates/git-core/tests/status.rs`
- Modify: `crates/git-core/src/lib.rs`
- Create: `crates/git-core/src/repo.rs`
- Create: `crates/git-core/src/status.rs`

**Interfaces:**
- Consumes: `crates/git-core/Cargo.toml` from Task 1 (git2, thiserror, tempfile already declared).
- Produces: `git_core::repo::open(path: &Path) -> Result<git2::Repository, RepoError>` and `git_core::status::status(repo: &git2::Repository) -> Result<Vec<StatusEntry>, StatusError>`, where `StatusEntry { path: String, staged: bool, kind: StatusKind }` and `StatusKind { New, Modified, Deleted, Renamed, TypeChange }`. Task 3's `worker.rs` calls both.

- [ ] **Step 1: Write the shared test helper**

`crates/git-core/tests/common/mod.rs`:
```rust
use std::path::Path;

use git2::Repository;
use tempfile::TempDir;

pub fn init_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().expect("create temp dir");
    let repo = Repository::init(dir.path()).expect("init repo");
    {
        let mut config = repo.config().expect("repo config");
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    (dir, repo)
}

pub fn write_file(dir: &Path, relative_path: &str, contents: &str) {
    let full_path = dir.join(relative_path);
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent).expect("create parent dirs");
    }
    std::fs::write(full_path, contents).expect("write file");
}
```

- [ ] **Step 2: Write the failing `repo::open` tests**

`crates/git-core/tests/repo.rs`:
```rust
mod common;

use common::init_repo;

#[test]
fn opens_an_existing_repository() {
    let (dir, _repo) = init_repo();

    let result = git_core::repo::open(dir.path());

    assert!(result.is_ok());
}

#[test]
fn discovers_repository_from_a_subdirectory() {
    let (dir, _repo) = init_repo();
    let subdir = dir.path().join("nested/sub/dir");
    std::fs::create_dir_all(&subdir).unwrap();

    let result = git_core::repo::open(&subdir);

    assert!(result.is_ok());
}

#[test]
fn fails_on_a_non_repository_path() {
    let dir = tempfile::TempDir::new().unwrap();

    let result = git_core::repo::open(dir.path());

    assert!(result.is_err());
}
```

- [ ] **Step 3: Run the tests to verify they fail to compile**

Run: `cargo test -p git-core --test repo`
Expected: FAIL — `error[E0433]: failed to resolve: use of undeclared crate or module 'repo'` (the module doesn't exist yet).

- [ ] **Step 4: Implement `repo::open`**

`crates/git-core/src/lib.rs`:
```rust
pub mod repo;
pub mod status;
```

`crates/git-core/src/repo.rs`:
```rust
use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RepoError {
    #[error("failed to open repository: {0}")]
    Open(#[from] git2::Error),
}

pub fn open(path: &Path) -> Result<git2::Repository, RepoError> {
    Ok(git2::Repository::discover(path)?)
}
```

Also create a placeholder `crates/git-core/src/status.rs` containing `// implemented in Step 6` so `lib.rs`'s `pub mod status;` compiles.

- [ ] **Step 5: Run the tests to verify `repo::open` passes**

Run: `cargo test -p git-core --test repo`
Expected: PASS — 3 passed.

- [ ] **Step 6: Write the failing `status::status` tests**

`crates/git-core/tests/status.rs`:
```rust
mod common;

use common::{init_repo, write_file};
use git_core::status::StatusKind;

#[test]
fn reports_an_untracked_file_as_unstaged_new() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "untracked.txt", "hello");

    let entries = git_core::status::status(&repo).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "untracked.txt");
    assert!(!entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::New);
}

#[test]
fn reports_a_staged_new_file_as_staged_new() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "staged.txt", "hello");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("staged.txt")).unwrap();
    index.write().unwrap();

    let entries = git_core::status::status(&repo).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "staged.txt");
    assert!(entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::New);
}

#[test]
fn reports_a_modified_tracked_file_as_unstaged_modified() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "hello");
    let mut index = repo.index().unwrap();
    index.add_path(std::path::Path::new("tracked.txt")).unwrap();
    let tree_id = index.write_tree().unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let sig = repo.signature().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
        .unwrap();
    write_file(dir.path(), "tracked.txt", "changed");

    let entries = git_core::status::status(&repo).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "tracked.txt");
    assert!(!entries[0].staged);
    assert_eq!(entries[0].kind, StatusKind::Modified);
}

#[test]
fn reports_a_clean_repository_as_empty() {
    let (_dir, repo) = init_repo();

    let entries = git_core::status::status(&repo).unwrap();

    assert!(entries.is_empty());
}
```

- [ ] **Step 7: Run the tests to verify they fail**

Run: `cargo test -p git-core --test status`
Expected: FAIL — `error[E0425]: cannot find function 'status' in module 'git_core::status'` (only the placeholder from Step 4 exists).

- [ ] **Step 8: Implement `status::status`**

`crates/git-core/src/status.rs`:
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StatusError {
    #[error("failed to read repository status: {0}")]
    Read(#[from] git2::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    New,
    Modified,
    Deleted,
    Renamed,
    TypeChange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    pub path: String,
    pub staged: bool,
    pub kind: StatusKind,
}

pub fn status(repo: &git2::Repository) -> Result<Vec<StatusEntry>, StatusError> {
    let statuses = repo.statuses(None)?;
    let mut entries = Vec::new();

    for entry in statuses.iter() {
        let Ok(path) = entry.path() else { continue };
        let flags = entry.status();

        if let Some(kind) = staged_kind(flags) {
            entries.push(StatusEntry {
                path: path.to_string(),
                staged: true,
                kind,
            });
        }
        if let Some(kind) = unstaged_kind(flags) {
            entries.push(StatusEntry {
                path: path.to_string(),
                staged: false,
                kind,
            });
        }
    }

    Ok(entries)
}

fn staged_kind(flags: git2::Status) -> Option<StatusKind> {
    if flags.is_index_new() {
        Some(StatusKind::New)
    } else if flags.is_index_modified() {
        Some(StatusKind::Modified)
    } else if flags.is_index_deleted() {
        Some(StatusKind::Deleted)
    } else if flags.is_index_renamed() {
        Some(StatusKind::Renamed)
    } else if flags.is_index_typechange() {
        Some(StatusKind::TypeChange)
    } else {
        None
    }
}

fn unstaged_kind(flags: git2::Status) -> Option<StatusKind> {
    if flags.is_wt_new() {
        Some(StatusKind::New)
    } else if flags.is_wt_modified() {
        Some(StatusKind::Modified)
    } else if flags.is_wt_deleted() {
        Some(StatusKind::Deleted)
    } else if flags.is_wt_renamed() {
        Some(StatusKind::Renamed)
    } else if flags.is_wt_typechange() {
        Some(StatusKind::TypeChange)
    } else {
        None
    }
}
```

- [ ] **Step 9: Run the tests to verify they pass**

Run: `cargo test -p git-core --test status`
Expected: PASS — 4 passed.

- [ ] **Step 10: Run the full workspace test suite and lints**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: all pass with zero warnings.

- [ ] **Step 11: Commit**

```bash
git add crates/git-core
git commit -m "feat: add git-core repo::open and status::status with tests"
```

---

### Task 3: Tauri shell + `RepoClient` interface + minimal status view, wired end-to-end

**Files:**
- Create: `crates/tauri-app/Cargo.toml`, `crates/tauri-app/build.rs`, `crates/tauri-app/tauri.conf.json`
- Create: `crates/tauri-app/src/main.rs`, `crates/tauri-app/src/worker.rs`, `crates/tauri-app/src/commands.rs`
- Modify: `Cargo.toml` (add `crates/tauri-app` to workspace members)
- Create: `frontend/src/ipc/RepoClient.ts`, `frontend/src/ipc/tauriRepoClient.ts`
- Create: `frontend/src/components/StatusView.tsx`, `frontend/src/components/StatusView.test.tsx`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/package.json` (add `@tauri-apps/api`, `@tauri-apps/cli`)

**Interfaces:**
- Consumes: `git_core::repo::open`, `git_core::status::status`, `git_core::status::StatusEntry`/`StatusKind` from Task 2.
- Produces: Tauri commands `open_repo(path: String) -> Result<(), String>` and `get_status() -> Result<Vec<StatusEntryDto>, String>`; frontend `RepoClient { openRepo(path: string): Promise<void>; getStatus(): Promise<StatusEntry[]> }`. Later phases add more `RepoClient` methods and more `components/`.

- [ ] **Step 1: Add the workspace member and install the Tauri CLI**

Add `"crates/tauri-app"` to the `members` array in the root `Cargo.toml` (from Task 1).

Run: `cargo install tauri-cli --version "^2.0" --locked`

- [ ] **Step 2: Scaffold the Tauri crate**

`crates/tauri-app/Cargo.toml`:
```toml
[package]
name = "tauri-app"
version = "0.1.0"
edition = "2021"
license = "MIT"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
git-core = { path = "../git-core" }

[dev-dependencies]
git2 = "0.21"
tempfile = "3"
```

`crates/tauri-app/build.rs`:
```rust
fn main() {
    tauri_build::build()
}
```

`crates/tauri-app/tauri.conf.json`:
```json
{
  "productName": "Browsitory",
  "version": "0.1.0",
  "identifier": "com.browsitory.app",
  "build": {
    "beforeDevCommand": "pnpm --dir ../../frontend dev",
    "beforeBuildCommand": "pnpm --dir ../../frontend build",
    "devUrl": "http://localhost:5173",
    "frontendDist": "../../frontend/dist"
  },
  "app": {
    "windows": [
      {
        "title": "Browsitory",
        "width": 1200,
        "height": 800
      }
    ]
  },
  "bundle": {
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

Run `cargo tauri icons` (from `crates/tauri-app/`) against any square PNG to generate the `icons/` directory referenced by `tauri.conf.json`'s `bundle.icon` list above — any placeholder square image works for local development; replace with a real app icon before release.

- [ ] **Step 3: Write the failing `Worker` test**

`crates/tauri-app/src/worker.rs` (test module only for now — no `Worker` impl yet):
```rust
#[cfg(test)]
mod tests {
    use std::path::Path;

    use git2::Repository;
    use tempfile::TempDir;

    use super::Worker;

    fn init_repo() -> (TempDir, Repository) {
        let dir = TempDir::new().expect("create temp dir");
        let repo = Repository::init(dir.path()).expect("init repo");
        {
            let mut config = repo.config().expect("repo config");
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
        }
        (dir, repo)
    }

    fn write_file(dir: &Path, relative_path: &str, contents: &str) {
        std::fs::write(dir.join(relative_path), contents).expect("write file");
    }

    #[test]
    fn get_status_reflects_an_untracked_file() {
        let (dir, _repo) = init_repo();
        write_file(dir.path(), "untracked.txt", "hello");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let entries = worker.get_status().unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "untracked.txt");
    }

    #[test]
    fn spawn_fails_on_a_non_repository_path() {
        let dir = TempDir::new().unwrap();

        let result = Worker::spawn(dir.path().to_path_buf());

        assert!(result.is_err());
    }
}
```

- [ ] **Step 4: Run the test to verify it fails to compile**

Run: `cargo test -p tauri-app worker::tests`
Expected: FAIL — `error[E0432]: unresolved import 'super::Worker'` (nothing above the test module yet).

- [ ] **Step 5: Implement `Worker`**

Prepend this to `crates/tauri-app/src/worker.rs`, above the `#[cfg(test)]` module already there:
```rust
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::thread;

use git_core::status::StatusEntry;

enum Command {
    GetStatus { reply: Sender<Result<Vec<StatusEntry>, String>> },
}

pub struct Worker {
    tx: Sender<Command>,
}

impl Worker {
    pub fn spawn(path: PathBuf) -> Result<Self, String> {
        let repo = git_core::repo::open(&path).map_err(|e| e.to_string())?;
        let (tx, rx) = mpsc::channel::<Command>();

        thread::spawn(move || {
            let repo = repo;
            for command in rx {
                match command {
                    Command::GetStatus { reply } => {
                        let result = git_core::status::status(&repo).map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                }
            }
        });

        Ok(Worker { tx })
    }

    pub fn get_status(&self) -> Result<Vec<StatusEntry>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetStatus { reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cargo test -p tauri-app worker::tests`
Expected: PASS — 2 passed.

- [ ] **Step 7: Add the Tauri commands and main entrypoint**

`crates/tauri-app/src/commands.rs`:
```rust
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

use crate::worker::Worker;

#[derive(Serialize)]
pub struct StatusEntryDto {
    pub path: String,
    pub staged: bool,
    pub kind: String,
}

#[derive(Default)]
pub struct AppState {
    pub worker: Mutex<Option<Worker>>,
}

#[tauri::command]
pub fn open_repo(path: String, state: State<AppState>) -> Result<(), String> {
    let worker = Worker::spawn(PathBuf::from(path))?;
    *state.worker.lock().unwrap() = Some(worker);
    Ok(())
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> Result<Vec<StatusEntryDto>, String> {
    let guard = state.worker.lock().unwrap();
    let worker = guard.as_ref().ok_or_else(|| "no repo open".to_string())?;
    let entries = worker.get_status()?;
    Ok(entries
        .into_iter()
        .map(|e| StatusEntryDto {
            path: e.path,
            staged: e.staged,
            kind: format!("{:?}", e.kind),
        })
        .collect())
}
```

`Worker` needs `Default` derivable state, so add `impl Default for AppState` is already covered by `#[derive(Default)]` since `Mutex<Option<Worker>>` implements `Default`.

`crates/tauri-app/src/main.rs`:
```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod worker;

use commands::{get_status, open_repo, AppState};

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![open_repo, get_status])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 8: Verify the Rust side builds**

Run: `cargo build --workspace`
Expected: succeeds.

- [ ] **Step 9: Add the `RepoClient` interface**

`frontend/src/ipc/RepoClient.ts`:
```ts
export type StatusKind = "New" | "Modified" | "Deleted" | "Renamed" | "TypeChange";

export interface StatusEntry {
  path: string;
  staged: boolean;
  kind: StatusKind;
}

export interface RepoClient {
  openRepo(path: string): Promise<void>;
  getStatus(): Promise<StatusEntry[]>;
}
```

Run: `pnpm add @tauri-apps/api` (inside `frontend/`)

`frontend/src/ipc/tauriRepoClient.ts`:
```ts
import { invoke } from "@tauri-apps/api/core";
import type { RepoClient, StatusEntry } from "./RepoClient";

export const tauriRepoClient: RepoClient = {
  openRepo: (path: string) => invoke("open_repo", { path }),
  getStatus: () => invoke<StatusEntry[]>("get_status"),
};
```

- [ ] **Step 10: Write the failing `StatusView` test**

`frontend/src/components/StatusView.test.tsx`:
```tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { RepoClient, StatusEntry } from "../ipc/RepoClient";
import { StatusView } from "./StatusView";

function fakeClient(entries: StatusEntry[]): RepoClient {
  return {
    openRepo: async () => {},
    getStatus: async () => entries,
  };
}

describe("StatusView", () => {
  it("renders each status entry's path", async () => {
    const client = fakeClient([
      { path: "src/main.rs", staged: false, kind: "Modified" },
      { path: "README.md", staged: true, kind: "New" },
    ]);

    render(<StatusView client={client} />);

    expect(await screen.findByText("src/main.rs")).toBeInTheDocument();
    expect(await screen.findByText("README.md")).toBeInTheDocument();
  });

  it("renders nothing extra when there are no changes", async () => {
    const client = fakeClient([]);

    render(<StatusView client={client} />);

    expect(await screen.findByText("No changes")).toBeInTheDocument();
  });
});
```

- [ ] **Step 11: Run the test to verify it fails**

Run: `pnpm test -- --run` (inside `frontend/`)
Expected: FAIL — cannot find module `./StatusView` (doesn't exist yet).

- [ ] **Step 12: Implement `StatusView`**

`frontend/src/components/StatusView.tsx`:
```tsx
import { useEffect, useState } from "react";
import type { RepoClient, StatusEntry } from "../ipc/RepoClient";

export function StatusView({ client }: { client: RepoClient }) {
  const [entries, setEntries] = useState<StatusEntry[]>([]);

  useEffect(() => {
    client.getStatus().then(setEntries);
  }, [client]);

  if (entries.length === 0) {
    return <p>No changes</p>;
  }

  return (
    <ul>
      {entries.map((entry) => (
        <li key={`${entry.staged}:${entry.path}`}>
          {entry.staged ? "●" : "○"} {entry.path} ({entry.kind})
        </li>
      ))}
    </ul>
  );
}
```

- [ ] **Step 13: Run the test to verify it passes**

Run: `pnpm test -- --run` (inside `frontend/`)
Expected: PASS — 2 passed.

- [ ] **Step 14: Wire `StatusView` into `App.tsx`**

`frontend/src/App.tsx`:
```tsx
import { tauriRepoClient } from "./ipc/tauriRepoClient";
import { StatusView } from "./components/StatusView";

export default function App() {
  return (
    <main>
      <h1>Browsitory</h1>
      <StatusView client={tauriRepoClient} />
    </main>
  );
}
```

- [ ] **Step 15: Verify the full stack builds**

Run: `cargo build --workspace && (cd frontend && pnpm build)`
Expected: both succeed.

- [ ] **Step 16: Commit**

```bash
git add Cargo.toml crates/tauri-app frontend
git commit -m "feat: add Tauri shell, RepoClient IPC boundary, and minimal status view"
```

---

### Task 4: `CLAUDE.md` and `docs/ARCHITECTURE.md`

**Files:**
- Modify: `CLAUDE.md`
- Create: `docs/ARCHITECTURE.md`
- Create: `docs/LICENSE_COMPLIANCE.md`

**Interfaces:**
- Consumes: the final crate/package layout, commands, and full dependency list from Tasks 1-3.
- Produces: nothing consumed by later tasks in this plan — these are the documents future sessions read first.

- [ ] **Step 1: Rewrite `CLAUDE.md`**

`CLAUDE.md`:
```markdown
# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build --workspace                          # build all Rust crates
cargo test --workspace                            # run all Rust tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check                        # format check (CI)
cargo tauri dev                                    # run the desktop app (from crates/tauri-app)
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

## Project status

Second from-scratch rewrite (branch `feat/rust_from_scratch`). See
`docs/superpowers/specs/2026-08-11-browsitory-architecture-design.md` for the full rationale —
in short: a prior native Rust+egui pass (also on this branch, see git history) worked but
produced a UI that can't be reused as a VSCode webview, which is a stated future requirement.
This pass keeps the Rust git layer but replaces egui with Tauri + a React/TypeScript frontend
behind a `RepoClient` IPC interface, so a VSCode extension can implement the same interface
later without touching UI code.

Phase 0 (this pass) is setup only: workspace scaffold, CI, `git-core::repo`/`status` with
tests, and a Tauri shell proving the IPC boundary end-to-end with a minimal status view.
Phases 1-4 (see `docs/ARCHITECTURE.md`) are not started.

## Architecture

See `docs/ARCHITECTURE.md` for the full crate/package layout, the `RepoClient` IPC boundary,
and the threading model. Summary: `crates/git-core` (git2, UI-agnostic, DI'd per function,
tested against real temp-dir repos) + `crates/config` (TOML registry/prefs, stub so far) +
`crates/tauri-app` (Tauri commands, one worker thread per open repo) + `frontend/` (React/TS,
talks to the backend only through `frontend/src/ipc/RepoClient.ts`).

### git2 API gotchas

- `StatusEntry::path()` (and `Signature::name()`/`email()`, `Reference::shorthand()`,
  `Commit::summary()`) return `Result<&str, Error>` or `Result<Option<&str>, Error>`, not a
  bare `Option`/`&str` — verified against the vendored `git2` 0.21 source. Handle with
  `let Ok(x) = ... else { continue };` in a loop, or `.ok().flatten().unwrap_or_default()`
  otherwise. See `crates/git-core/src/status.rs`.
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
`crates/tauri-app/src/worker.rs`'s `Worker::spawn` opens the repository on a dedicated thread
and owns it for that thread's lifetime; Tauri commands (`crates/tauri-app/src/commands.rs`)
send `Command`s over an `mpsc` channel and get replies over a per-call reply channel. UI code
never touches `git-core` directly — only through `RepoClient` → a Tauri command → the worker
thread.

### `RepoClient`: why it exists

`frontend/src/ipc/RepoClient.ts` is the only interface `frontend/src/components` and
`frontend/src/state` are allowed to depend on for backend calls.
`frontend/src/ipc/tauriRepoClient.ts` is the only file that imports `@tauri-apps/api`. When a
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
- `tauri-app` tests live inline (`#[cfg(test)] mod tests`) next to the code they test (see
  `worker.rs`), also against real temp-dir repos. Thin pass-through Tauri commands
  (`commands.rs`) don't need their own tests — the `git-core`/`Worker` logic they call already
  is tested.
- `frontend` tests mock `RepoClient` (a real interface seam), never `@tauri-apps/api`.

## Task workflow

This repo uses the `superpowers` plugin's `test-driven-development`, `writing-plans`,
`subagent-driven-development`, and `executing-plans` skills for all implementation work, plus
one project-local skill (`.claude/skills/browsitory-conventions/`) for the conventions above
that aren't already covered by those global skills. New implementation tasks follow
`docs/TASK_TEMPLATE.md`.
```

- [ ] **Step 2: Write `docs/ARCHITECTURE.md`**

`docs/ARCHITECTURE.md`:
```markdown
# Architecture

## Crate/package layout

```
browsitory/
├── crates/
│   ├── git-core/    # git2-based service layer, UI-agnostic, unit-tested headlessly
│   ├── config/      # repo registry + preferences (TOML), stub until Phase 1
│   └── tauri-app/    # Tauri commands + per-repo worker threads
└── frontend/          # React + TypeScript + Vite, the only crate/package that talks to a UI toolkit
```

## Why Tauri + a web frontend, not egui again

A prior pass (see git history on this branch before 2026-08-11) used `egui` for a
single-language, no-webview native UI. It worked, but `egui`'s immediate-mode canvas can't be
embedded in a VSCode webview later — a stated product requirement ("frontend shall be either
the standalone app or a vscode integration"). Tauri packages a plain web frontend as a
standalone app today, and that same frontend becomes the VSCode webview later by adding one
more `RepoClient` implementation — no UI rewrite.

## Why git2 (libgit2 bindings), not gitoxide

`git2` is mature and complete: native blame, native interactive rebase primitives, native
merge conflict handling, native stash/cherry-pick, native remote transports. The pure-Rust
alternative (`gitoxide`/`gix`) is a closer license fit (no GPL exception needed at all), but
its write-side operations (merge, rebase) are less mature as of this writing. `git2` is the
pragmatic choice; the libgit2 license deviation is documented, not silently accepted — see
`CLAUDE.md`'s License policy section.

## The `RepoClient` IPC boundary

`frontend/src/ipc/RepoClient.ts` defines the interface every UI component depends on:

```ts
export interface RepoClient {
  openRepo(path: string): Promise<void>;
  getStatus(): Promise<StatusEntry[]>;
  // grows with each feature phase
}
```

`frontend/src/ipc/tauriRepoClient.ts` implements it over `@tauri-apps/api`'s `invoke()`. This
is the *only* file allowed to import `@tauri-apps/api` — every other frontend file receives a
`RepoClient` as a prop/context value, so it can't accidentally couple to Tauri. A future VSCode
extension implements the same interface over `postMessage` in a sibling file
(`vscodeRepoClient.ts`); no component changes.

## Threading model

`git2::Repository` **is** `Send` (libgit2 handles can be moved between threads, and `git2`
carries an `unsafe impl Send` to say so) but it is **not** `Sync`: a `&Repository` must never
be used from two threads at once. So the handle can be *given* to exactly one thread, but not
*shared*. That rules out the obvious Tauri shape — managed state requires `Send + Sync`, so a
bare `Repository` can't be `State` at all, and `State<Mutex<Repository>>` (which does satisfy
`Sync`) would funnel every concurrent command invocation through a single lock held for the
duration of blocking git work.

The `!Sync` constraint is what message-passing answers. Each opened repository gets one
dedicated OS thread (`crates/tauri-app/src/worker.rs`'s `Worker::spawn`) that opens its own
`Repository` handle and owns it exclusively for the thread's lifetime — the handle is moved in
once and never shared by reference. Tauri commands (`crates/tauri-app/src/commands.rs`) send a
`Command` enum value over a `std::sync::mpsc` channel to that thread and receive the result
over a per-call reply channel; only owned, `Send` command/reply values cross the boundary.
Commands clone the channel `Sender` out of the state mutex and drop the guard before blocking
on a reply, so one slow repository operation can't serialize unrelated commands. One worker
thread per open repo also means multiple repos never contend on a shared handle.

## Error handling

`git-core` functions return typed errors (`thiserror` enums per module: `RepoError`,
`StatusError`, ...). `Worker`/Tauri commands map these to `Result<T, String>` crossing the IPC
boundary (Tauri serializes `Err` as a rejected JS promise). `RepoClient` methods return
`Promise<T>` that reject with that message — no error is swallowed at the boundary.

## Testing strategy

- `git-core`/`config`: `cargo test`, real temp-dir repos/files, no mocks.
- `tauri-app`: inline unit tests for logic that isn't thin delegation (see `worker.rs`'s tests,
  which spawn a real worker thread against a real temp-dir repo). Pass-through Tauri commands
  don't get separate tests.
- `frontend`: Vitest + Testing Library, mocking `RepoClient` (a real interface seam).
- E2E (added from Phase 1 onward, not in Phase 0): Playwright against the `cargo tauri dev`
  build, one flow per major feature area, added where a flow spans backend+frontend in a way
  unit tests can't catch.

## Roadmap

- **Phase 0** (this pass): workspace scaffold, `git-core::repo`/`status`, Tauri shell + minimal
  status view proving the IPC boundary.
- **Phase 1**: full repo view — commit history, diff viewer, stage/unstage, commit.
- **Phase 2**: branch management, stash, merge with conflict resolution, interactive rebase,
  blame viewer, multi-branch commit graph.
- **Phase 3**: push/pull/fetch with progress, multi-remote, tag push, credential handling.
- **Phase 4**: worktrees, submodules, reflog viewer, PR integration.
```

- [ ] **Step 3: Write `docs/LICENSE_COMPLIANCE.md`**

Audit every dependency added in Tasks 1-3 (`cargo info <crate>` for Rust, `npm info <package>
license` for JS) and record it here — this is the "record it in `docs/LICENSE_COMPLIANCE.md`"
step the spec's License policy calls for.

`docs/LICENSE_COMPLIANCE.md`:
```markdown
# License compliance

Browsitory is MIT-licensed. Every dependency below was verified permissive (MIT, Apache-2.0,
ISC, BSD, MIT-0) except the one documented exception.

## Rust (`cargo info <crate>`)

| Crate | License | Notes |
|---|---|---|
| git2 | MIT/Apache-2.0 (binding); vendored libgit2 is GPL-2.0-with-linking-exception | Deliberate exception — see below. |
| thiserror | MIT/Apache-2.0 | |
| tempfile | MIT/Apache-2.0 | dev-dependency only |
| tauri | MIT/Apache-2.0 | |
| tauri-build | MIT/Apache-2.0 | build-dependency |
| serde | MIT/Apache-2.0 | |
| serde_json | MIT/Apache-2.0 | |

## JavaScript (`npm info <package> license`)

| Package | License | Notes |
|---|---|---|
| react / react-dom | MIT | |
| vite | MIT | |
| typescript | Apache-2.0 | |
| vitest | MIT | |
| @testing-library/react, @testing-library/jest-dom | MIT | dev only |
| jsdom | MIT | dev only |
| @tauri-apps/api | MIT/Apache-2.0 | |
| @tauri-apps/cli | MIT/Apache-2.0 | dev only |
| eslint | MIT | dev only |

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
```

- [ ] **Step 4: Verify the documented commands actually work**

Run each command listed in `CLAUDE.md`'s "Commands" section (`cargo build --workspace`,
`cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt
--all -- --check`, and inside `frontend/`: `pnpm build`, `pnpm lint`, `pnpm test -- --run`).
Expected: every one succeeds as documented. Fix any command in the doc that doesn't match
reality before committing.

- [ ] **Step 5: Commit**

```bash
git add CLAUDE.md docs/ARCHITECTURE.md docs/LICENSE_COMPLIANCE.md
git commit -m "docs: rewrite CLAUDE.md, add ARCHITECTURE.md and LICENSE_COMPLIANCE.md for the Tauri rebuild"
```

---

### Task 5: Task template

**Files:**
- Create: `docs/TASK_TEMPLATE.md`

**Interfaces:**
- Consumes: nothing.
- Produces: the template every Phase 1+ task file follows. Referenced by `CLAUDE.md` (Task 4)
  and the project-local skill (Task 6).

- [ ] **Step 1: Write the template**

`docs/TASK_TEMPLATE.md`:
```markdown
# Task template

Every implementation task in Browsitory (Phase 1 onward) follows this shape, one file per
task, named `docs/tasks/phase-<N>/<workstream>-<id>-<slug>.md` (e.g.
`docs/tasks/phase-1/a-01-diff-viewer.md`). Tasks within the same phase that touch disjoint
files are grouped into parallel workstreams (A, B, C, ...) per
`superpowers:subagent-driven-development`; tasks within a workstream are sequential.

```markdown
# Task <phase>.<workstream>.<id>: <title>

## Goal
One paragraph: what this task adds and why.

## Depends on
List of task IDs that must land first, or "none".

## TDD requirement
Which test file(s) get written first, and what they must assert before any implementation
code is written. Real assertions, not "add appropriate tests."

## Acceptance criteria
Checklist of observable outcomes (tests passing, a command working end-to-end, etc).

## Out of scope
What this task deliberately does not do — keeps tasks isolated and reviewable independently.
```

## Example

```markdown
# Task 1.A.02: Diff viewer for a single file

## Goal
Add `git_core::diff::file_diff(repo, path) -> Result<Vec<DiffHunk>, DiffError>` and a
`DiffView` frontend component that renders it, so a user can see line-level changes for a
selected file in the status list.

## Depends on
Task 1.A.01 (git-core::status, already landed).

## TDD requirement
`crates/git-core/tests/diff.rs`: a test that modifies a tracked file and asserts
`file_diff` returns one hunk containing both the removed and added line. A test that diffs an
untracked file against an empty tree and asserts every line is an addition.
`frontend/src/components/DiffView.test.tsx`: a test that renders a `DiffView` given a fixed
list of `DiffHunk`s (via a mocked `RepoClient`) and asserts added/removed lines get distinct
CSS classes.

## Acceptance criteria
- [ ] `cargo test -p git-core --test diff` passes.
- [ ] `pnpm test -- --run` passes for `DiffView.test.tsx`.
- [ ] Selecting a file in `StatusView` shows its diff in `cargo tauri dev`.

## Out of scope
Word-level diff highlighting (later task). Binary file diffs (later task).
```
```

- [ ] **Step 2: Commit**

```bash
git add docs/TASK_TEMPLATE.md
git commit -m "docs: add the Phase 1+ task template"
```

---

### Task 6: Project-local Claude skill for Browsitory conventions

**Files:**
- Create: `.claude/skills/browsitory-conventions/SKILL.md` (exact path decided by the
  `writing-skills` skill's own conventions — this is the expected default)

**Interfaces:**
- Consumes: the conventions already written into `CLAUDE.md` (Task 4) and `docs/TASK_TEMPLATE.md`
  (Task 5) — this task packages them as a skill, it doesn't invent new ones.
- Produces: nothing consumed by other tasks in this plan; this is the last task.

- [ ] **Step 1: Invoke `superpowers:writing-skills`**

Invoke the `superpowers:writing-skills` skill to scaffold a new project-local skill named
`browsitory-conventions`, and follow that skill's process for structure/frontmatter/testing.
Give it this content brief (the skill's process will format it correctly):

- **Trigger:** any task that adds or modifies code in this repo (`git-core`, `config`,
  `tauri-app`, or `frontend`).
- **Rule 1:** `git-core`/`tauri-app` tests always use real temp-dir repos (`tempfile` +
  `git2::Repository::init`), never a mocked `git2::Repository`. `frontend` tests always mock
  the `RepoClient` interface, never `@tauri-apps/api` or `postMessage` directly.
- **Rule 2:** `frontend/src/components` and `frontend/src/state` never import
  `@tauri-apps/api` or any transport directly — only files under `frontend/src/ipc/` do.
- **Rule 3:** new implementation tasks follow `docs/TASK_TEMPLATE.md`'s shape and file naming
  (`docs/tasks/phase-<N>/<workstream>-<id>-<slug>.md`).
- Point back to `CLAUDE.md` and `docs/ARCHITECTURE.md` as the source of truth this skill
  summarizes — the skill should stay short and defer detail to those docs rather than
  duplicating them at length.

- [ ] **Step 2: Verify the skill was created correctly**

Confirm the skill file exists, has valid frontmatter (name + description), and states all
three rules above without contradicting `CLAUDE.md`. If `writing-skills` includes a
self-test/lint step, run it.

- [ ] **Step 3: Commit**

```bash
git add .claude/skills/browsitory-conventions
git commit -m "chore: add project-local browsitory-conventions Claude skill"
```

---

## Plan-level verification

After all six tasks: `cargo build --workspace && cargo test --workspace && cargo clippy
--workspace --all-targets -- -D warnings && cargo fmt --all -- --check`, and inside
`frontend/`: `pnpm build && pnpm lint && pnpm test -- --run`. Then `cargo tauri dev` (from
`crates/tauri-app/`) should open a window, and calling `openRepo` (e.g. temporarily hardcode a
path in `App.tsx` for this manual check, then revert) should populate `StatusView` with real
status entries from that repo.
