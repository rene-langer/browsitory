# VSCode Sidecar Protocol Proof-of-Concept Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prove the VSCode-extension sidecar architecture end-to-end — a new `crates/vscode-sidecar`
binary that speaks line-delimited JSON-RPC 2.0 over stdio, dispatching to `repo-service`'s
`Worker`, plus a `frontend/src/ipc/vscodeRepoClient.ts` that speaks the same protocol from the
webview side — wired for a handful of methods (`open_repo`/`close_repo` plus the status/log/diff
family: `get_status`, `get_commit_graph`, `get_working_diff`, `get_commit_diff`) before the
remaining ~79 `RepoClient` methods get mechanically wired in follow-up plans. This is Phase 6
sub-phase (b)'s first half, per the spec's own phrasing: "proven end-to-end on a handful of
methods ... before wiring all 85."

**Architecture:** `crates/vscode-sidecar` is a new binary crate (sibling to `tauri-app`) that
owns a `HashMap<String, Worker>` repo registry and a synchronous stdin-read / dispatch /
stdout-write loop — one JSON-RPC request per line in, one response per line out, no concurrency
(a deliberate simplification for this proof; see Global Constraints). It depends on
`repo-service` exactly like `tauri-app` does, and defines its own wire DTOs
(`StatusEntryDto`/`GraphCommitDto`/`DiffHunkDto`/`DiffLineDto`) with the same camelCase JSON
shape `tauri-app/src/commands/mod.rs` already produces, since both transports serialize for the
same `frontend/src/ipc/RepoClient.ts` consumer. `frontend/src/ipc/vscodeRepoClient.ts` implements
the full `RepoClient` interface: the six wired methods send a JSON-RPC request via
`acquireVsCodeApi().postMessage` and resolve/reject a pending-request map keyed by request id when
a matching `window.addEventListener("message", ...)` reply arrives; every other method is a typed
stub that rejects with "not implemented yet" until a later plan wires it. Every Rust task is
verified with a black-box integration test that spawns the actual built `vscode-sidecar` binary
and drives it over its real stdin/stdout, against real temp-dir repos — same "real repo, no
mocks" convention `crates/repo-service/src/worker/mod.rs`'s own tests already follow, and the same
black-box-subprocess spirit `e2e/` uses for `tauri-app`.

**Tech Stack:** Rust, Cargo workspaces, git2 0.21 (dev-only in this crate), serde/serde_json;
TypeScript, Vitest, jsdom.

**Spec:** `docs/superpowers/specs/2026-08-30-vscode-extension-design.md`

## Global Constraints

- Desktop VSCode only — the sidecar is a stdio subprocess, not a WASM/browser target (spec's
  "Constraints / decisions").
- Full `RepoClient` parity is the eventual target (~85 methods), but this plan proves the
  mechanism on a handful first — the spec's own ordering — and stops there; wiring the rest is a
  mechanical follow-up, not this plan's scope.
- No `RepoClient` method signature, DTO field name, or wire-format string changes for the
  *existing* Tauri transport. `tauriRepoClient.ts` and `tauri-app`'s commands are untouched.
- Every wire DTO this plan adds must serialize with the exact same JSON shape (field names,
  `StatusKind`/`DiffLineOrigin` string values) as the matching type in
  `frontend/src/ipc/RepoClient.ts` — the same contract `repo-service`'s
  `wire_format_tests` module pins for the enums; this plan's DTOs are new code repeating that
  same contract for a second transport, not something `wire_format_tests` itself covers (it pins
  raw enum `Debug` strings, not full struct shapes).
- Single-threaded, one-request-at-a-time dispatch in the sidecar for this plan — no `Mutex`, no
  worker threads spawned by the sidecar's own main loop beyond what `Worker::spawn` itself
  spawns. Concurrent/pipelined request handling, if ever needed, is out of scope here.
- No `vscode` npm package dependency added — `vscodeRepoClient.ts` declares
  `acquireVsCodeApi` as an ambient ad-hoc type instead, since the real `@types/vscode-webview`
  wiring belongs to sub-phase (c)'s `extension/` host work. No `frontend/eslint.config.js` change
  needed for this plan (it currently restricts only `@tauri-apps/*` imports; this plan adds no
  `vscode`-package import to restrict).
- No new licensed dependency: `git-core`, `repo-service`, `serde`, `serde_json` are already
  recorded in `docs/LICENSE_COMPLIANCE.md`; this plan's one new dev-only dependency, `git2`, is
  already recorded there too (as a `git-core`/`tauri-app`/`repo-service` dependency — this plan
  just adds `vscode-sidecar` to that list of consumers, not a new row).
- `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets --
  -D warnings`, and `cargo fmt --all -- --check` must pass after the final task; `pnpm build`,
  `pnpm lint`, and `pnpm test -- --run` (from `frontend/`) likewise.
- Commit after each task (this repo's existing per-task-commit convention).

---

### Task 1: Scaffold `vscode-sidecar` — JSON-RPC types and a safe no-op dispatch loop

**Files:**
- Create: `crates/vscode-sidecar/Cargo.toml`
- Create: `crates/vscode-sidecar/src/protocol.rs`
- Create: `crates/vscode-sidecar/src/dispatch.rs`
- Create: `crates/vscode-sidecar/src/main.rs`
- Create: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`
- Modify: `Cargo.toml:1-7` (workspace root — add the new member)

**Interfaces:**
- Consumes: nothing from earlier tasks (first task).
- Produces: `protocol::{JsonRpcRequest, JsonRpcResponse, JsonRpcError}`, `dispatch::dispatch(method:
  &str, params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String>` — the
  signature every later task's method handlers plug into via `dispatch.rs`'s `match`.

This task proves the stdio transport (line framing, JSON-RPC envelope, one-request-per-line
dispatch loop, malformed-input resilience) in isolation, before any task wires real git-backed
methods to it — so a transport bug can't be confused with a `git-core`/`repo-service` bug later.

- [ ] **Step 1: Create the crate's `Cargo.toml`**

```toml
[package]
name = "vscode-sidecar"
version = "0.1.0"
edition = "2021"
license = "MIT"

[[bin]]
name = "vscode-sidecar"
path = "src/main.rs"

[dependencies]
repo-service = { path = "../repo-service" }
git-core = { path = "../git-core" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[dev-dependencies]
git2 = "0.21"
tempfile = "3"
```

- [ ] **Step 2: Register the crate in the workspace**

Edit `Cargo.toml` (repo root):

```toml
[workspace]
resolver = "2"
members = [
    "crates/git-core",
    "crates/config",
    "crates/repo-service",
    "crates/tauri-app",
    "crates/vscode-sidecar",
]
```

- [ ] **Step 3: Write the JSON-RPC envelope types**

```rust
// crates/vscode-sidecar/src/protocol.rs

//! Line-delimited JSON-RPC 2.0 over stdio — the wire protocol between the VSCode extension
//! host (eventually relaying from the webview via `postMessage`) and this sidecar process. One
//! JSON object per line on stdin (a request), one JSON object per line on stdout (a response).

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, message: String) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32000,
                message,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_deserializes_from_json_rpc_shape() {
        let json = r#"{"jsonrpc":"2.0","id":7,"method":"get_status","params":{"repoPath":"/tmp/x"}}"#;

        let request: JsonRpcRequest = serde_json::from_str(json).expect("parse request");

        assert_eq!(request.id, 7);
        assert_eq!(request.method, "get_status");
        assert_eq!(request.params["repoPath"], "/tmp/x");
    }

    #[test]
    fn ok_response_serializes_without_an_error_field() {
        let response = JsonRpcResponse::ok(3, serde_json::json!({"a": 1}));

        let serialized = serde_json::to_string(&response).expect("serialize response");

        assert_eq!(serialized, r#"{"jsonrpc":"2.0","id":3,"result":{"a":1}}"#);
    }

    #[test]
    fn err_response_serializes_without_a_result_field() {
        let response = JsonRpcResponse::err(4, "boom".to_string());

        let serialized = serde_json::to_string(&response).expect("serialize response");

        assert_eq!(
            serialized,
            r#"{"jsonrpc":"2.0","id":4,"error":{"code":-32000,"message":"boom"}}"#
        );
    }
}
```

- [ ] **Step 4: Write a no-op dispatch stub (real methods arrive in later tasks)**

```rust
// crates/vscode-sidecar/src/dispatch.rs

use std::collections::HashMap;

use repo_service::worker::Worker;
use serde_json::Value;

/// Routes one JSON-RPC method call to its handler. Every later task in this plan adds a `match`
/// arm here; until then, every method reports as unknown rather than hanging or panicking, so
/// this task's transport-only test can prove the stdio loop safely.
pub fn dispatch(
    method: &str,
    _params: Value,
    _repos: &mut HashMap<String, Worker>,
) -> Result<Value, String> {
    Err(format!("unknown method: {method}"))
}
```

- [ ] **Step 5: Write the stdin/stdout dispatch loop**

```rust
// crates/vscode-sidecar/src/main.rs

use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use repo_service::worker::Worker;

mod dispatch;
mod protocol;

use protocol::{JsonRpcRequest, JsonRpcResponse};

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut repos: HashMap<String, Worker> = HashMap::new();

    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(error) => {
                eprintln!("vscode-sidecar: dropping malformed request: {error}");
                continue;
            }
        };

        let response = match dispatch::dispatch(&request.method, request.params, &mut repos) {
            Ok(result) => JsonRpcResponse::ok(request.id, result),
            Err(message) => JsonRpcResponse::err(request.id, message),
        };

        let serialized = serde_json::to_string(&response).expect("response always serializes");
        writeln!(stdout, "{serialized}").expect("stdout write failed");
        stdout.flush().expect("stdout flush failed");
    }
}
```

- [ ] **Step 6: Write the black-box transport test**

```rust
// crates/vscode-sidecar/tests/protocol_roundtrip.rs

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};

struct Sidecar {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl Sidecar {
    fn spawn() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_vscode-sidecar"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("spawn vscode-sidecar");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn call(&mut self, id: u64, method: &str, params: serde_json::Value) -> serde_json::Value {
        let request =
            serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(self.stdin, "{request}").expect("write request");
        self.stdin.flush().expect("flush request");
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read response");
        serde_json::from_str(&line).expect("parse response")
    }
}

impl Drop for Sidecar {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn unknown_method_returns_a_json_rpc_error_not_a_hang() {
    let mut sidecar = Sidecar::spawn();

    let response = sidecar.call(1, "does_not_exist", serde_json::json!({}));

    assert_eq!(response["error"]["message"], "unknown method: does_not_exist");
    assert!(response.get("result").is_none());
}

#[test]
fn malformed_request_is_dropped_without_killing_the_sidecar() {
    let mut sidecar = Sidecar::spawn();

    writeln!(sidecar.stdin, "not json").expect("write malformed line");
    sidecar.stdin.flush().expect("flush malformed line");

    let response = sidecar.call(1, "does_not_exist", serde_json::json!({}));
    assert_eq!(response["error"]["message"], "unknown method: does_not_exist");
}
```

- [ ] **Step 7: Build and run**

Run: `cargo test -p vscode-sidecar`
Expected: all 5 tests pass (3 in `protocol.rs`, 2 in `tests/protocol_roundtrip.rs`).

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml crates/vscode-sidecar
git commit -m "feat(vscode-sidecar): scaffold JSON-RPC transport over stdio"
```

---

### Task 2: Wire `open_repo`, `close_repo`, `get_status`

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`

**Interfaces:**
- Consumes: `dispatch::dispatch` (Task 1), `repo_service::worker::{Worker, WorkerHandle}`'s
  `get_status(&self) -> Result<Vec<StatusEntry>, String>` (`crates/repo-service/src/worker/status.rs:126`).
- Produces: `StatusEntryDto` (this task's own copy of the wire shape — camelCase-free since its
  fields are already single words, matching `crates/tauri-app/src/commands/mod.rs:132-136`'s
  `StatusEntryDto` exactly) — later tasks in this plan follow the same per-method DTO pattern.

- [ ] **Step 1: Replace `dispatch.rs`'s stub with the first three real methods**

```rust
// crates/vscode-sidecar/src/dispatch.rs

use std::collections::HashMap;

use git_core::status::StatusEntry;
use repo_service::worker::Worker;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn dispatch(
    method: &str,
    params: Value,
    repos: &mut HashMap<String, Worker>,
) -> Result<Value, String> {
    match method {
        "open_repo" => open_repo(params, repos),
        "close_repo" => close_repo(params, repos),
        "get_status" => get_status(params, repos),
        other => Err(format!("unknown method: {other}")),
    }
}

fn worker_handle(
    repos: &HashMap<String, Worker>,
    repo_path: &str,
) -> Result<repo_service::worker::WorkerHandle, String> {
    repos
        .get(repo_path)
        .map(Worker::handle)
        .ok_or_else(|| format!("repo not open: {repo_path}"))
}

#[derive(Deserialize)]
struct OpenRepoParams {
    path: String,
}

fn open_repo(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: OpenRepoParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    if !repos.contains_key(&params.path) {
        let worker = Worker::spawn(params.path.clone().into())?;
        repos.insert(params.path, worker);
    }
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoPathParams {
    repo_path: String,
}

fn close_repo(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    repos.remove(&params.repo_path);
    Ok(Value::Null)
}

#[derive(Serialize)]
struct StatusEntryDto {
    path: String,
    staged: bool,
    kind: String,
}

impl From<StatusEntry> for StatusEntryDto {
    fn from(entry: StatusEntry) -> Self {
        Self {
            path: entry.path,
            staged: entry.staged,
            kind: format!("{:?}", entry.kind),
        }
    }
}

fn get_status(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let entries: Vec<StatusEntryDto> = worker_handle(repos, &params.repo_path)?
        .get_status()?
        .into_iter()
        .map(StatusEntryDto::from)
        .collect();
    serde_json::to_value(entries).map_err(|error| error.to_string())
}
```

- [ ] **Step 2: Add the open/status/close round trip and the unopened-repo error test**

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs` (keep the existing two tests and
`Sidecar` struct above these):

```rust
fn write_file(dir: &std::path::Path, relative_path: &str, contents: &str) {
    std::fs::write(dir.join(relative_path), contents).expect("write file");
}

fn init_repo() -> (tempfile::TempDir, git2::Repository) {
    let dir = tempfile::TempDir::new().expect("create temp dir");
    let repo = git2::Repository::init(dir.path()).expect("init repo");
    let mut config = repo.config().expect("repo config");
    config.set_str("user.name", "Test User").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();
    (dir, repo)
}

#[test]
fn open_status_close_round_trip_through_the_sidecar() {
    let (dir, _repo) = init_repo();
    write_file(dir.path(), "untracked.txt", "hello");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();

    let open = sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));
    assert_eq!(open["result"], serde_json::Value::Null);

    let status = sidecar.call(2, "get_status", serde_json::json!({"repoPath": repo_path}));
    let entries = status["result"].as_array().expect("status result array");
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["path"], "untracked.txt");
    assert_eq!(entries[0]["kind"], "New");

    let close = sidecar.call(3, "close_repo", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(close["result"], serde_json::Value::Null);
}

#[test]
fn get_status_on_an_unopened_repo_returns_a_json_rpc_error() {
    let mut sidecar = Sidecar::spawn();

    let status = sidecar.call(1, "get_status", serde_json::json!({"repoPath": "/no/such/repo"}));

    assert!(status["error"]["message"]
        .as_str()
        .unwrap()
        .contains("repo not open"));
    assert!(status.get("result").is_none());
}
```

- [ ] **Step 3: Build and run**

Run: `cargo test -p vscode-sidecar`
Expected: all 7 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/vscode-sidecar
git commit -m "feat(vscode-sidecar): wire open_repo, close_repo, get_status"
```

---

### Task 3: Wire `get_commit_graph`

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`

**Interfaces:**
- Consumes: `WorkerHandle::get_commit_graph(&self, limit: usize, selected_branches:
  Option<Vec<String>>) -> Result<Vec<GraphCommit>, String>` (`crates/repo-service/src/worker/status.rs:135-150`).
- Produces: `GraphCommitDto` — same camelCase shape as
  `crates/tauri-app/src/commands/mod.rs:428-439`'s `GraphCommitDto`.

- [ ] **Step 1: Add the `get_commit_graph` handler to `dispatch.rs`**

Add to the `match` in `dispatch()`:

```rust
        "get_commit_graph" => get_commit_graph(params, repos),
```

Add the imports, params struct, DTO, and handler function:

```rust
use git_core::graph::GraphCommit;
```

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetCommitGraphParams {
    repo_path: String,
    limit: usize,
    selected_branches: Option<Vec<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphCommitDto {
    id: String,
    short_id: String,
    summary: String,
    author_name: String,
    author_email: String,
    timestamp: i64,
    parent_ids: Vec<String>,
    branch_refs: Vec<String>,
}

impl From<GraphCommit> for GraphCommitDto {
    fn from(c: GraphCommit) -> Self {
        Self {
            id: c.id,
            short_id: c.short_id,
            summary: c.summary,
            author_name: c.author_name,
            author_email: c.author_email,
            timestamp: c.timestamp,
            parent_ids: c.parent_ids,
            branch_refs: c.branch_refs,
        }
    }
}

fn get_commit_graph(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: GetCommitGraphParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let commits: Vec<GraphCommitDto> = worker_handle(repos, &params.repo_path)?
        .get_commit_graph(params.limit, params.selected_branches)?
        .into_iter()
        .map(GraphCommitDto::from)
        .collect();
    serde_json::to_value(commits).map_err(|error| error.to_string())
}
```

- [ ] **Step 2: Add the commit-graph test**

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs`:

```rust
fn commit_all(repo: &git2::Repository, message: &str) {
    let mut index = repo.index().expect("open index");
    index
        .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
        .expect("stage all");
    index.write().expect("write index");
    let tree_id = index.write_tree().expect("write tree");
    let tree = repo.find_tree(tree_id).expect("find tree");
    let signature = repo.signature().expect("signature");
    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parents,
    )
    .expect("commit");
}

#[test]
fn commit_graph_reflects_a_commit_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "hello");
    commit_all(&repo, "initial commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let graph = sidecar.call(
        2,
        "get_commit_graph",
        serde_json::json!({"repoPath": repo_path, "limit": 10, "selectedBranches": null}),
    );

    let commits = graph["result"].as_array().expect("commit graph result array");
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0]["summary"], "initial commit");
}
```

- [ ] **Step 3: Build and run**

Run: `cargo test -p vscode-sidecar`
Expected: all 8 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/vscode-sidecar
git commit -m "feat(vscode-sidecar): wire get_commit_graph"
```

---

### Task 4: Wire `get_working_diff` and `get_commit_diff`

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`

**Interfaces:**
- Consumes: `WorkerHandle::get_working_diff(&self, path: String, staged: bool) ->
  Result<Vec<DiffHunk>, String>` and `WorkerHandle::get_commit_diff(&self, commit_id: String, path:
  String) -> Result<Vec<DiffHunk>, String>` (`crates/repo-service/src/worker/status.rs:152-180`).
- Produces: `DiffHunkDto`/`DiffLineDto` — same shape as
  `crates/tauri-app/src/commands/mod.rs:610-624`'s pair.

- [ ] **Step 1: Add both handlers to `dispatch.rs`**

Add to the `match` in `dispatch()`:

```rust
        "get_working_diff" => get_working_diff(params, repos),
        "get_commit_diff" => get_commit_diff(params, repos),
```

Add the import, DTOs, params structs, and handler functions:

```rust
use git_core::diff::DiffHunk;
```

```rust
#[derive(Serialize)]
struct DiffLineDto {
    origin: String,
    content: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiffHunkDto {
    old_start: u32,
    old_lines: u32,
    new_start: u32,
    new_lines: u32,
    lines: Vec<DiffLineDto>,
}

impl From<DiffHunk> for DiffHunkDto {
    fn from(hunk: DiffHunk) -> Self {
        Self {
            old_start: hunk.old_start,
            old_lines: hunk.old_lines,
            new_start: hunk.new_start,
            new_lines: hunk.new_lines,
            lines: hunk
                .lines
                .into_iter()
                .map(|line| DiffLineDto {
                    origin: format!("{:?}", line.origin),
                    content: line.content,
                })
                .collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetWorkingDiffParams {
    repo_path: String,
    path: String,
    staged: bool,
}

fn get_working_diff(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: GetWorkingDiffParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_working_diff(params.path, params.staged)?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetCommitDiffParams {
    repo_path: String,
    commit_id: String,
    path: String,
}

fn get_commit_diff(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: GetCommitDiffParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let hunks: Vec<DiffHunkDto> = worker_handle(repos, &params.repo_path)?
        .get_commit_diff(params.commit_id, params.path)?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect();
    serde_json::to_value(hunks).map_err(|error| error.to_string())
}
```

- [ ] **Step 2: Add the diff round-trip test**

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs`:

```rust
#[test]
fn working_and_commit_diff_round_trip_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\nline two\n");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "tracked.txt", "line one changed\nline two\n");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let working = sidecar.call(
        2,
        "get_working_diff",
        serde_json::json!({"repoPath": repo_path, "path": "tracked.txt", "staged": false}),
    );
    let hunks = working["result"].as_array().expect("working diff result array");
    assert_eq!(hunks.len(), 1);

    let graph = sidecar.call(
        3,
        "get_commit_graph",
        serde_json::json!({"repoPath": repo_path, "limit": 1, "selectedBranches": null}),
    );
    let head_id = graph["result"][0]["id"]
        .as_str()
        .expect("head commit id")
        .to_string();

    let commit_diff = sidecar.call(
        4,
        "get_commit_diff",
        serde_json::json!({"repoPath": repo_path, "commitId": head_id, "path": "tracked.txt"}),
    );
    let commit_hunks = commit_diff["result"]
        .as_array()
        .expect("commit diff result array");
    assert_eq!(commit_hunks.len(), 1);
}
```

- [ ] **Step 3: Build and run**

Run: `cargo test -p vscode-sidecar`
Expected: all 9 tests pass.

- [ ] **Step 4: Commit**

```bash
git add crates/vscode-sidecar
git commit -m "feat(vscode-sidecar): wire get_working_diff and get_commit_diff"
```

---

### Task 5: `frontend/src/ipc/vscodeRepoClient.ts`

**Files:**
- Create: `frontend/src/ipc/vscodeRepoClient.ts`
- Create: `frontend/src/ipc/vscodeRepoClient.test.ts`

**Interfaces:**
- Consumes: `RepoClient` (`frontend/src/ipc/RepoClient.ts:217-306`), the JSON-RPC wire shape Tasks
  1-4 defined (`{jsonrpc, id, method, params}` request; `{jsonrpc, id, result}` or `{jsonrpc, id,
  error: {code, message}}` response).
- Produces: `vscodeRepoClient: RepoClient` — a second `RepoClient` implementation, sibling to
  `tauriRepoClient` (`frontend/src/ipc/tauriRepoClient.ts:48`), not yet imported by any component
  (that wiring is sub-phase (c)'s `extension/` host work).

- [ ] **Step 1: Write `vscodeRepoClient.ts`**

```typescript
import type {
  BranchInfo,
  ConflictSegment,
  CreatePullRequest,
  DiffHunk,
  FileConflictChoice,
  ForgeProvider,
  ForgeRepository,
  GraphCommit,
  MergeOutcome,
  OpenRepoEntry,
  PullOutcome,
  PullRequest,
  PullRequestList,
  RebasePlanCommit,
  RebasePlanEntry,
  RebaseStepResult,
  ReflogEntry,
  RemoteAuthMode,
  RemoteInfo,
  RepoClient,
  StashEntry,
  StatusEntry,
  SubmoduleInfo,
  TagInfo,
  TransferProgress,
  UpstreamInfo,
  Workspace,
  WorktreeInfo,
} from "./RepoClient";

// No `vscode` npm package dependency here on purpose — the real extension host's webview API
// typings arrive with sub-phase (c)'s `extension/` package. This ambient declaration is the
// minimal shape this file actually calls.
interface VsCodeWebviewApi {
  postMessage(message: unknown): void;
}

declare function acquireVsCodeApi(): VsCodeWebviewApi;

interface JsonRpcSuccess {
  jsonrpc: "2.0";
  id: number;
  result: unknown;
}

interface JsonRpcFailure {
  jsonrpc: "2.0";
  id: number;
  error: { code: number; message: string };
}

type JsonRpcResponse = JsonRpcSuccess | JsonRpcFailure;

function isJsonRpcResponse(value: unknown): value is JsonRpcResponse {
  return (
    typeof value === "object" &&
    value !== null &&
    "jsonrpc" in value &&
    "id" in value &&
    ("result" in value || "error" in value)
  );
}

const vscode = acquireVsCodeApi();
let nextRequestId = 1;
const pending = new Map<
  number,
  { resolve: (value: unknown) => void; reject: (error: Error) => void }
>();

window.addEventListener("message", (event: MessageEvent) => {
  const message = event.data;
  if (!isJsonRpcResponse(message)) return;
  const waiting = pending.get(message.id);
  if (!waiting) return;
  pending.delete(message.id);
  if ("error" in message) {
    waiting.reject(new Error(message.error.message));
  } else {
    waiting.resolve(message.result);
  }
});

function call<T>(method: string, params: Record<string, unknown>): Promise<T> {
  const id = nextRequestId++;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
    vscode.postMessage({ jsonrpc: "2.0", id, method, params });
  });
}

function notImplemented(method: string) {
  return (): Promise<never> =>
    Promise.reject(new Error(`vscodeRepoClient: ${method} is not implemented yet`));
}

export const vscodeRepoClient: RepoClient = {
  openRepo: (path: string) => call<void>("open_repo", { path }),
  closeRepo: (repoPath: string) => call<void>("close_repo", { repoPath }),
  getStatus: (repoPath: string) => call<StatusEntry[]>("get_status", { repoPath }),
  getCommitGraph: (repoPath: string, limit: number, selectedBranches: string[] | null) =>
    call<GraphCommit[]>("get_commit_graph", { repoPath, limit, selectedBranches }),
  getWorkingDiff: (repoPath: string, path: string, staged: boolean) =>
    call<DiffHunk[]>("get_working_diff", { repoPath, path, staged }),
  getCommitDiff: (repoPath: string, commitId: string, path: string) =>
    call<DiffHunk[]>("get_commit_diff", { repoPath, commitId, path }),

  pickRepoFolder: notImplemented("pickRepoFolder"),
  listRecentRepos: notImplemented("listRecentRepos"),
  getAppVersion: notImplemented("getAppVersion"),
  getLastSeenVersion: notImplemented("getLastSeenVersion"),
  setLastSeenVersion: notImplemented("setLastSeenVersion"),
  listOpenRepos: notImplemented("listOpenRepos"),
  persistOpenRepos: notImplemented("persistOpenRepos"),
  scanReposInRoot: notImplemented("scanReposInRoot"),
  listWorkspaces: notImplemented("listWorkspaces"),
  saveWorkspace: notImplemented("saveWorkspace"),
  updateWorkspace: notImplemented("updateWorkspace"),
  deleteWorkspace: notImplemented("deleteWorkspace"),
  getGraphBranchSelection: notImplemented("getGraphBranchSelection"),
  setGraphBranchSelection: notImplemented("setGraphBranchSelection"),
  getCommitFiles: notImplemented("getCommitFiles"),
  stageFile: notImplemented("stageFile"),
  unstageFile: notImplemented("unstageFile"),
  stageHunk: notImplemented("stageHunk"),
  unstageHunk: notImplemented("unstageHunk"),
  discardHunk: notImplemented("discardHunk"),
  commit: notImplemented("commit"),
  listBranches: notImplemented("listBranches"),
  createBranch: notImplemented("createBranch"),
  switchBranch: notImplemented("switchBranch"),
  deleteBranch: notImplemented("deleteBranch"),
  renameBranch: notImplemented("renameBranch"),
  listWorktrees: notImplemented("listWorktrees"),
  createWorktree: notImplemented("createWorktree"),
  removeWorktree: notImplemented("removeWorktree"),
  pruneWorktrees: notImplemented("pruneWorktrees"),
  listSubmodules: notImplemented("listSubmodules"),
  initSubmodule: notImplemented("initSubmodule"),
  updateSubmodule: notImplemented("updateSubmodule"),
  listReflogRefs: notImplemented("listReflogRefs"),
  getReflog: notImplemented("getReflog"),
  restoreReflogEntry: notImplemented("restoreReflogEntry"),
  listRemotes: notImplemented("listRemotes"),
  listRemoteBranches: notImplemented("listRemoteBranches"),
  getCurrentUpstream: notImplemented("getCurrentUpstream"),
  getRemoteUpstreams: notImplemented("getRemoteUpstreams"),
  addRemote: notImplemented("addRemote"),
  renameRemote: notImplemented("renameRemote"),
  updateRemoteUrls: notImplemented("updateRemoteUrls"),
  removeRemote: notImplemented("removeRemote"),
  saveHttpsCredential: notImplemented("saveHttpsCredential"),
  forgetHttpsCredential: notImplemented("forgetHttpsCredential"),
  setRemoteAuthMode: notImplemented("setRemoteAuthMode"),
  setCurrentUpstream: notImplemented("setCurrentUpstream"),
  clearCurrentUpstream: notImplemented("clearCurrentUpstream"),
  listTags: notImplemented("listTags"),
  createTag: notImplemented("createTag"),
  deleteTag: notImplemented("deleteTag"),
  fetchRemote: notImplemented("fetchRemote"),
  pushCurrentBranch: notImplemented("pushCurrentBranch"),
  pushTags: notImplemented("pushTags"),
  pullCurrentUpstream: notImplemented("pullCurrentUpstream"),
  subscribeTransferProgress: () => () => {},
  listStashes: notImplemented("listStashes"),
  saveStash: notImplemented("saveStash"),
  applyStash: notImplemented("applyStash"),
  dropStash: notImplemented("dropStash"),
  getBlame: notImplemented("getBlame"),
  mergeBranch: notImplemented("mergeBranch"),
  getConflictHunks: notImplemented("getConflictHunks"),
  resolveConflict: notImplemented("resolveConflict"),
  abortMerge: notImplemented("abortMerge"),
  getMergeMessage: notImplemented("getMergeMessage"),
  resolveAddDeleteConflict: notImplemented("resolveAddDeleteConflict"),
  commitsSince: notImplemented("commitsSince"),
  startRebase: notImplemented("startRebase"),
  rebaseContinue: notImplemented("rebaseContinue"),
  abortRebase: notImplemented("abortRebase"),
  getRebaseProgress: notImplemented("getRebaseProgress"),
  detectForgeRepository: notImplemented("detectForgeRepository"),
  saveForgeToken: notImplemented("saveForgeToken"),
  forgetForgeToken: notImplemented("forgetForgeToken"),
  listPullRequests: notImplemented("listPullRequests"),
  createPullRequest: notImplemented("createPullRequest"),
  openExternalUrl: notImplemented("openExternalUrl"),
};
```

Unused-import note: `BranchInfo`, `ConflictSegment`, `CreatePullRequest`, `ForgeProvider`,
`ForgeRepository`, `MergeOutcome`, `OpenRepoEntry`, `PullOutcome`, `PullRequest`,
`PullRequestList`, `RebasePlanCommit`, `RebasePlanEntry`, `RebaseStepResult`, `ReflogEntry`,
`RemoteAuthMode`, `RemoteInfo`, `StashEntry`, `SubmoduleInfo`, `TagInfo`, `TransferProgress`,
`UpstreamInfo`, `Workspace`, `WorktreeInfo`, and `FileConflictChoice` are imported only as
`type` and never referenced in an expression — `notImplemented`'s stub functions don't name
their parameter types. Remove any the TypeScript compiler flags as unused when Step 3 runs
`tsc`; keep only the ones the wired methods' own signatures need
(`StatusEntry`/`GraphCommit`/`DiffHunk`/`RepoClient`).

- [ ] **Step 2: Write the test**

```typescript
// frontend/src/ipc/vscodeRepoClient.test.ts

import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RepoClient } from "./RepoClient";

describe("vscodeRepoClient", () => {
  let postMessage: ReturnType<typeof vi.fn>;
  let vscodeRepoClient: RepoClient;

  beforeEach(async () => {
    vi.resetModules();
    postMessage = vi.fn();
    (
      globalThis as unknown as { acquireVsCodeApi: () => { postMessage: typeof postMessage } }
    ).acquireVsCodeApi = () => ({ postMessage });
    ({ vscodeRepoClient } = await import("./vscodeRepoClient"));
  });

  function respond(id: number, result: unknown) {
    window.dispatchEvent(new MessageEvent("message", { data: { jsonrpc: "2.0", id, result } }));
  }

  function respondWithError(id: number, message: string) {
    window.dispatchEvent(
      new MessageEvent("message", {
        data: { jsonrpc: "2.0", id, error: { code: -32000, message } },
      }),
    );
  }

  it("posts a JSON-RPC request and resolves on a matching reply", async () => {
    const promise = vscodeRepoClient.getStatus("/repo");

    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_status",
      params: { repoPath: "/repo" },
    });
    respond(1, [{ path: "a.txt", staged: false, kind: "New" }]);

    await expect(promise).resolves.toEqual([{ path: "a.txt", staged: false, kind: "New" }]);
  });

  it("rejects when the sidecar returns a JSON-RPC error", async () => {
    const promise = vscodeRepoClient.getStatus("/missing");

    respondWithError(1, "repo not open: /missing");

    await expect(promise).rejects.toThrow("repo not open: /missing");
  });

  it("correlates concurrent requests by id", async () => {
    const openPromise = vscodeRepoClient.openRepo("/repo");
    const statusPromise = vscodeRepoClient.getStatus("/repo");

    respond(2, [{ path: "b.txt", staged: true, kind: "Modified" }]);
    respond(1, null);

    await expect(openPromise).resolves.toBeUndefined();
    await expect(statusPromise).resolves.toEqual([
      { path: "b.txt", staged: true, kind: "Modified" },
    ]);
  });

  it("rejects unwired methods without touching postMessage", async () => {
    await expect(vscodeRepoClient.listBranches("/repo")).rejects.toThrow(
      "listBranches is not implemented yet",
    );
    expect(postMessage).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 3: Run**

Run: `cd frontend && pnpm exec tsc -b --noEmit` (type-check only, catches the unused-import note
from Step 1) then `pnpm test -- --run vscodeRepoClient`
Expected: `tsc` reports no errors after any unused imports are trimmed; all 4 tests pass.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/ipc/vscodeRepoClient.ts frontend/src/ipc/vscodeRepoClient.test.ts
git commit -m "feat(frontend): add vscodeRepoClient JSON-RPC transport for status/log/diff"
```

---

### Task 6: Final verification pass

**Files:** `docs/ARCHITECTURE.md` (modified); everything else verification-only.

**Interfaces:** none — confirms Tasks 1-5's combined result meets every check this repo's CI and
`CLAUDE.md` require.

- [ ] **Step 1: Full workspace build and test**

Run: `cargo build --workspace`
Expected: succeeds.

Run: `cargo test --workspace`
Expected: all green, including `vscode-sidecar`'s 9 tests; every previously-passing test
elsewhere unchanged.

- [ ] **Step 2: Clippy and format**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

Run: `cargo fmt --all -- --check`
Expected: clean. If not, run `cargo fmt --all` and fold the diff into this task's commit.

- [ ] **Step 3: Frontend build, lint, test**

Run: `cd frontend && pnpm build`
Expected: succeeds (`tsc -b` catches any remaining type error from Task 5's stub object).

Run: `cd frontend && pnpm lint`
Expected: clean.

Run: `cd frontend && pnpm test -- --run`
Expected: all green.

- [ ] **Step 4: Update `docs/ARCHITECTURE.md`'s crate/package layout**

The crate list currently reads:

```
browsitory/
├── crates/
│   ├── git-core/      # git2-based service layer, UI-agnostic, unit-tested headlessly
│   ├── config/        # repo registry + preferences: recent-repos list, backed by TOML
│   ├── repo-service/  # transport-agnostic worker threads, credentials, forge/PR API access
│   └── tauri-app/      # thin Tauri command adapter over repo-service
└── frontend/            # React + TypeScript + Vite, the only crate/package that talks to a UI toolkit
```

Update it to:

```
browsitory/
├── crates/
│   ├── git-core/        # git2-based service layer, UI-agnostic, unit-tested headlessly
│   ├── config/          # repo registry + preferences: recent-repos list, backed by TOML
│   ├── repo-service/    # transport-agnostic worker threads, credentials, forge/PR API access
│   ├── tauri-app/        # thin Tauri command adapter over repo-service
│   └── vscode-sidecar/  # JSON-RPC-over-stdio adapter over repo-service, for the VSCode extension
└── frontend/              # React + TypeScript + Vite, the only crate/package that talks to a UI toolkit
```

Directly below that block, add a short paragraph (matching this file's existing prose style):

```markdown
`vscode-sidecar` is `tauri-app`'s sibling for the VSCode extension target (Phase 6, spec
`docs/superpowers/specs/2026-08-30-vscode-extension-design.md`): a standalone binary speaking
line-delimited JSON-RPC 2.0 over stdio instead of Tauri's IPC. As of this writing it wires
`open_repo`/`close_repo` plus the status/log/diff method family
(`crates/vscode-sidecar/src/dispatch.rs`); the remaining ~79 `RepoClient` methods, and the
`extension/` host that will actually spawn this process from VSCode, are follow-up work.
```

- [ ] **Step 5: Commit**

```bash
git add docs/ARCHITECTURE.md
git commit -m "docs(architecture): document the vscode-sidecar crate"
```
