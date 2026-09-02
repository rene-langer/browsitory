# VSCode Sidecar Remaining Methods Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish Phase 6 sub-phase (b) by wiring every remaining `RepoClient` method (all but
the five VSCode-native ones) into `crates/vscode-sidecar`'s JSON-RPC dispatch and
`frontend/src/ipc/vscodeRepoClient.ts`, reaching full `RepoClient` parity on the sidecar
transport.

**Architecture:** Mechanical repetition of the pattern
`docs/superpowers/plans/2026-08-30-vscode-sidecar-protocol-poc.md` already proved: each method
gets a `dispatch.rs` match arm, a params `Deserialize` struct (only when it takes more than the
already-defined `RepoPathParams`), a response DTO mirroring `crates/tauri-app/src/commands/mod.rs`'s
existing DTO byte-for-byte (same field names, same `rename_all = "camelCase"` usage, same
enum-tag shapes), and a black-box subprocess test in
`crates/vscode-sidecar/tests/protocol_roundtrip.rs`. `vscodeRepoClient.ts` gets each stub
replaced with a real `call<T>(...)`, plus a matching `vscodeRepoClient.test.ts` case. One
exception breaks the request/response mold: `subscribeTransferProgress` needs server-initiated
JSON-RPC *notifications* (no request id), because `fetch_remote`/`push_current_branch`/
`push_tags`/`pull_current_upstream` stream `TransferEvent`s from `Worker`'s own background
thread after (or, for fetch/push, *during*) the JSON-RPC response — Task 9 designs and wires
that separately from the plain request/response tasks around it.

**Tech Stack:** Rust, Cargo workspaces, git2 0.21 (dev-only), serde/serde_json; TypeScript,
Vitest, jsdom.

**Spec:** `docs/superpowers/specs/2026-08-30-vscode-extension-design.md`

## Global Constraints

- Out of scope: `pickRepoFolder`, `getAppVersion`, `getLastSeenVersion`, `setLastSeenVersion`,
  `openExternalUrl` stay `notImplemented(...)` in `vscodeRepoClient.ts` — the spec's
  "VSCode-native integrations" section implements these directly against VSCode APIs in
  sub-phase (c), never through the sidecar. No task in this plan touches them.
- No `RepoClient` method signature, DTO field name, or wire-format string changes for the
  *existing* Tauri transport. `tauriRepoClient.ts` and `tauri-app`'s commands are untouched.
- Every wire DTO this plan adds must serialize with the exact same JSON shape (field names,
  enum string/tag values) as the matching type in `frontend/src/ipc/RepoClient.ts` and the
  matching DTO already proven correct in `crates/tauri-app/src/commands/mod.rs` — that file is
  the source of truth this plan transcribes from for every task except Task 9's new
  notification shape (which has no Tauri-transport precedent to transcribe, since Tauri uses
  its own `AppHandle::emit`, not JSON-RPC).
- Single-threaded, one-request-at-a-time *dispatch* loop: no `Mutex` around dispatch, no worker
  thread spawned by the sidecar's own main loop beyond what `Worker::spawn` itself spawns —
  **except** Task 9's per-transfer-operation notification-relay thread, which is a deliberate,
  narrowly-scoped addition the transfer-progress design requires (see Task 9's own note on why
  this doesn't violate the spirit of the constraint: it only ever forwards a `Receiver` to
  stdout, it starts no git operation itself, and the dispatch loop remains single-threaded for
  every other method).
- No new licensed dependency beyond `config` (already recorded in `docs/LICENSE_COMPLIANCE.md`
  as a `tauri-app` dependency; this plan's Task 1 adds `vscode-sidecar` to that list of
  consumers, not a new row) — `git-core`, `repo-service`, `serde`, `serde_json` are already
  recorded too.
- Config-crate integration tests that exercise `config::*` (recent repos, workspaces, open
  repos, graph branch selection) must set `BROWSITORY_CONFIG_DIR` on the spawned sidecar
  subprocess's environment to a fresh `tempfile::TempDir`, exactly like
  `crates/config/tests/recent_repos.rs` isolates itself from the real user config file — a test
  that forgets this reads/writes the developer's real `~/.config/browsitory` (or platform
  equivalent).
- `cargo build --workspace`, `cargo test --workspace`, `cargo clippy --workspace --all-targets --
  -D warnings`, and `cargo fmt --all -- --check` must pass after the final task; `pnpm build`,
  `pnpm lint`, and `pnpm test -- --run` (from `frontend/`) likewise.
- Commit after each task (this repo's existing per-task-commit convention).

---

### Task 1: Recent repos, open repos, workspaces, graph branch selection

**Files:**
- Modify: `crates/vscode-sidecar/Cargo.toml` (add the `config` dependency)
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`

**Interfaces:**
- Consumes: `config::{add_recent_repo, list_recent_repos, list_open_repos, set_open_repos,
  scan_repos_in_root, list_workspaces, save_workspace, update_workspace, delete_workspace,
  get_graph_branch_selection, set_graph_branch_selection}`, `config::{OpenRepoEntry, Workspace}`
  (`crates/config/src/lib.rs`).
- Produces: `OpenRepoEntryDto`/`OpenRepoEntryInput`, `WorkspaceDto` — same camelCase shape as
  `crates/tauri-app/src/commands/mod.rs:171-214`'s pair.

Wires 10 methods: `listRecentRepos`, `listOpenRepos`, `persistOpenRepos`, `scanReposInRoot`,
`listWorkspaces`, `saveWorkspace`, `updateWorkspace`, `deleteWorkspace`,
`getGraphBranchSelection`, `setGraphBranchSelection`. Also fixes `open_repo`'s known gap (see
its doc comment in the current `dispatch.rs`): it now calls `config::add_recent_repo` on a
successful open, same as `tauri-app`'s `open_repo` command does.

- [ ] **Step 1: Add the `config` dependency**

Edit `crates/vscode-sidecar/Cargo.toml`'s `[dependencies]` section:

```toml
[dependencies]
repo-service = { path = "../repo-service" }
git-core = { path = "../git-core" }
config = { path = "../config" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Write the failing tests**

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs`:

```rust
struct ConfigDirGuard {
    _dir: tempfile::TempDir,
}

impl ConfigDirGuard {
    fn new() -> (Self, std::path::PathBuf) {
        let dir = tempfile::TempDir::new().expect("create config dir");
        let path = dir.path().to_path_buf();
        (Self { _dir: dir }, path)
    }
}

impl Sidecar {
    fn spawn_with_config_dir(config_dir: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_vscode-sidecar"))
            .env("BROWSITORY_CONFIG_DIR", config_dir)
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
}

#[test]
fn opening_a_repo_adds_it_to_recent_repos() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, _repo) = init_repo();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let recent = sidecar.call(2, "list_recent_repos", serde_json::json!({}));

    let paths = recent["result"].as_array().expect("recent repos array");
    assert!(paths.iter().any(|p| p == &repo_path));
}

#[test]
fn persist_and_list_open_repos_round_trip() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);

    let persist = sidecar.call(
        1,
        "persist_open_repos",
        serde_json::json!({
            "entries": [{"path": "/repos/a", "workspaceId": null}],
            "activePath": "/repos/a",
        }),
    );
    assert_eq!(persist["result"], serde_json::Value::Null);

    let listed = sidecar.call(2, "list_open_repos", serde_json::json!({}));
    assert_eq!(listed["result"]["entries"][0]["path"], "/repos/a");
    assert_eq!(listed["result"]["entries"][0]["workspaceId"], serde_json::Value::Null);
    assert_eq!(listed["result"]["activePath"], "/repos/a");
}

#[test]
fn scan_repos_in_root_finds_a_nested_repo() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let root = tempfile::TempDir::new().expect("create root dir");
    let (repo_dir, _repo) = init_repo();
    std::fs::rename(repo_dir.path(), root.path().join("nested")).expect("move repo into root");
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);

    let scanned = sidecar.call(
        1,
        "scan_repos_in_root",
        serde_json::json!({"root": root.path().to_str().unwrap()}),
    );

    let paths = scanned["result"].as_array().expect("scanned repos array");
    assert_eq!(paths.len(), 1);
}

#[test]
fn save_update_delete_workspace_round_trip() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);

    let saved = sidecar.call(
        1,
        "save_workspace",
        serde_json::json!({"name": "Suite", "root": "/repos/suite", "members": ["/repos/suite/api"]}),
    );
    let id = saved["result"].as_str().expect("workspace id").to_string();

    let listed = sidecar.call(2, "list_workspaces", serde_json::json!({}));
    assert_eq!(listed["result"][0]["id"], id);
    assert_eq!(listed["result"][0]["rootPath"], "/repos/suite");

    let updated = sidecar.call(
        3,
        "update_workspace",
        serde_json::json!({"id": id, "name": "Suite Renamed", "members": ["/repos/suite/web"]}),
    );
    assert_eq!(updated["result"], serde_json::Value::Null);
    let listed_after_update = sidecar.call(4, "list_workspaces", serde_json::json!({}));
    assert_eq!(listed_after_update["result"][0]["name"], "Suite Renamed");

    let deleted = sidecar.call(5, "delete_workspace", serde_json::json!({"id": id}));
    assert_eq!(deleted["result"], serde_json::Value::Null);
    let listed_after_delete = sidecar.call(6, "list_workspaces", serde_json::json!({}));
    assert_eq!(listed_after_delete["result"], serde_json::json!([]));
}

#[test]
fn graph_branch_selection_round_trips() {
    let (_guard, config_dir) = ConfigDirGuard::new();
    let (dir, _repo) = init_repo();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn_with_config_dir(&config_dir);

    let before = sidecar.call(
        1,
        "get_graph_branch_selection",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(before["result"], serde_json::Value::Null);

    let set = sidecar.call(
        2,
        "set_graph_branch_selection",
        serde_json::json!({"repoPath": repo_path, "selectedBranches": ["main", "feature"]}),
    );
    assert_eq!(set["result"], serde_json::Value::Null);

    let after = sidecar.call(
        3,
        "get_graph_branch_selection",
        serde_json::json!({"repoPath": repo_path}),
    );
    assert_eq!(after["result"], serde_json::json!(["main", "feature"]));
}
```

- [ ] **Step 3: Run the tests to see them fail**

Run: `cargo test -p vscode-sidecar`
Expected: FAIL — `list_recent_repos`/etc. report `unknown method`.

- [ ] **Step 4: Implement the handlers**

Add to `crates/vscode-sidecar/src/dispatch.rs`'s top imports:

```rust
use std::path::{Path, PathBuf};
```

Add to the `match` in `dispatch()`:

```rust
        "list_recent_repos" => list_recent_repos(),
        "list_open_repos" => list_open_repos_handler(),
        "persist_open_repos" => persist_open_repos(params),
        "scan_repos_in_root" => scan_repos_in_root(params),
        "list_workspaces" => list_workspaces_handler(),
        "save_workspace" => save_workspace(params),
        "update_workspace" => update_workspace(params),
        "delete_workspace" => delete_workspace(params),
        "get_graph_branch_selection" => get_graph_branch_selection(params),
        "set_graph_branch_selection" => set_graph_branch_selection(params),
```

Replace `open_repo`'s body (keep its signature) so it also records the repo as recent:

```rust
fn open_repo(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: OpenRepoParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    if let std::collections::hash_map::Entry::Vacant(entry) = repos.entry(params.path.clone()) {
        let worker = Worker::spawn(params.path.clone().into())?;
        entry.insert(worker);
    }
    let _ = config::add_recent_repo(Path::new(&params.path));
    Ok(Value::Null)
}
```

Add the new handler functions and DTOs anywhere below the existing code:

```rust
fn list_recent_repos() -> Result<Value, String> {
    let paths = config::list_recent_repos().map_err(|error| error.to_string())?;
    let paths: Vec<String> = paths
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    serde_json::to_value(paths).map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenRepoEntryDto {
    path: String,
    workspace_id: Option<String>,
}

impl From<config::OpenRepoEntry> for OpenRepoEntryDto {
    fn from(entry: config::OpenRepoEntry) -> Self {
        Self {
            path: entry.path.to_string_lossy().into_owned(),
            workspace_id: entry.workspace_id,
        }
    }
}

fn list_open_repos_handler() -> Result<Value, String> {
    let (entries, active) = config::list_open_repos().map_err(|error| error.to_string())?;
    let entries: Vec<OpenRepoEntryDto> = entries.into_iter().map(OpenRepoEntryDto::from).collect();
    let active = active.map(|p| p.to_string_lossy().into_owned());
    serde_json::to_value(serde_json::json!({ "entries": entries, "activePath": active }))
        .map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenRepoEntryInput {
    path: String,
    workspace_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistOpenReposParams {
    entries: Vec<OpenRepoEntryInput>,
    active_path: Option<String>,
}

fn persist_open_repos(params: Value) -> Result<Value, String> {
    let params: PersistOpenReposParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let entries: Vec<config::OpenRepoEntry> = params
        .entries
        .into_iter()
        .map(|entry| config::OpenRepoEntry {
            path: PathBuf::from(entry.path),
            workspace_id: entry.workspace_id,
        })
        .collect();
    config::set_open_repos(&entries, params.active_path.as_deref().map(Path::new))
        .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
struct ScanReposInRootParams {
    root: String,
}

fn scan_repos_in_root(params: Value) -> Result<Value, String> {
    let params: ScanReposInRootParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let paths = config::scan_repos_in_root(Path::new(&params.root)).map_err(|error| error.to_string())?;
    let paths: Vec<String> = paths
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    serde_json::to_value(paths).map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceDto {
    id: String,
    name: String,
    root_path: String,
    member_paths: Vec<String>,
}

impl From<config::Workspace> for WorkspaceDto {
    fn from(workspace: config::Workspace) -> Self {
        Self {
            id: workspace.id,
            name: workspace.name,
            root_path: workspace.root_path.to_string_lossy().into_owned(),
            member_paths: workspace
                .member_paths
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
        }
    }
}

fn list_workspaces_handler() -> Result<Value, String> {
    let workspaces = config::list_workspaces().map_err(|error| error.to_string())?;
    let workspaces: Vec<WorkspaceDto> = workspaces.into_iter().map(WorkspaceDto::from).collect();
    serde_json::to_value(workspaces).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
struct SaveWorkspaceParams {
    name: String,
    root: String,
    members: Vec<String>,
}

fn save_workspace(params: Value) -> Result<Value, String> {
    let params: SaveWorkspaceParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let members: Vec<PathBuf> = params.members.into_iter().map(PathBuf::from).collect();
    let id = config::save_workspace(&params.name, Path::new(&params.root), &members)
        .map_err(|error| error.to_string())?;
    Ok(Value::String(id))
}

#[derive(Deserialize)]
struct UpdateWorkspaceParams {
    id: String,
    name: String,
    members: Vec<String>,
}

fn update_workspace(params: Value) -> Result<Value, String> {
    let params: UpdateWorkspaceParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let members: Vec<PathBuf> = params.members.into_iter().map(PathBuf::from).collect();
    config::update_workspace(&params.id, &params.name, &members).map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
struct DeleteWorkspaceParams {
    id: String,
}

fn delete_workspace(params: Value) -> Result<Value, String> {
    let params: DeleteWorkspaceParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    config::delete_workspace(&params.id).map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

fn get_graph_branch_selection(params: Value) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let selection = config::get_graph_branch_selection(Path::new(&params.repo_path))
        .map_err(|error| error.to_string())?;
    serde_json::to_value(selection).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetGraphBranchSelectionParams {
    repo_path: String,
    selected_branches: Vec<String>,
}

fn set_graph_branch_selection(params: Value) -> Result<Value, String> {
    let params: SetGraphBranchSelectionParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    config::set_graph_branch_selection(Path::new(&params.repo_path), &params.selected_branches)
        .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}
```

- [ ] **Step 5: Run the tests to see them pass**

Run: `cargo test -p vscode-sidecar`
Expected: all pass (14 tests: the 9 from the POC plan plus this task's 5).

- [ ] **Step 6: Commit the Rust side**

```bash
git add crates/vscode-sidecar/Cargo.toml crates/vscode-sidecar/src/dispatch.rs crates/vscode-sidecar/tests/protocol_roundtrip.rs
git commit -m "feat(vscode-sidecar): wire recent repos, open repos, workspaces, graph branch selection"
```

- [ ] **Step 7: Wire the TypeScript client**

Edit `frontend/src/ipc/vscodeRepoClient.ts`'s import block to add `OpenRepoEntry` and
`Workspace`:

```typescript
import type { DiffHunk, GraphCommit, OpenRepoEntry, RepoClient, StatusEntry, Workspace } from "./RepoClient";
```

Replace these ten stub lines:

```typescript
  listRecentRepos: notImplemented("listRecentRepos"),
  listOpenRepos: notImplemented("listOpenRepos"),
  persistOpenRepos: notImplemented("persistOpenRepos"),
  scanReposInRoot: notImplemented("scanReposInRoot"),
  listWorkspaces: notImplemented("listWorkspaces"),
  saveWorkspace: notImplemented("saveWorkspace"),
  updateWorkspace: notImplemented("updateWorkspace"),
  deleteWorkspace: notImplemented("deleteWorkspace"),
  getGraphBranchSelection: notImplemented("getGraphBranchSelection"),
  setGraphBranchSelection: notImplemented("setGraphBranchSelection"),
```

with:

```typescript
  listRecentRepos: () => call<string[]>("list_recent_repos", {}),
  listOpenRepos: () =>
    call<{ entries: OpenRepoEntry[]; activePath: string | null }>("list_open_repos", {}),
  persistOpenRepos: (entries: OpenRepoEntry[], activePath: string | null) =>
    call<void>("persist_open_repos", { entries, activePath }),
  scanReposInRoot: (root: string) => call<string[]>("scan_repos_in_root", { root }),
  listWorkspaces: () => call<Workspace[]>("list_workspaces", {}),
  saveWorkspace: (name: string, root: string, members: string[]) =>
    call<string>("save_workspace", { name, root, members }),
  updateWorkspace: (id: string, name: string, members: string[]) =>
    call<void>("update_workspace", { id, name, members }),
  deleteWorkspace: (id: string) => call<void>("delete_workspace", { id }),
  getGraphBranchSelection: (repoPath: string) =>
    call<string[] | null>("get_graph_branch_selection", { repoPath }),
  setGraphBranchSelection: (repoPath: string, selectedBranches: string[]) =>
    call<void>("set_graph_branch_selection", { repoPath, selectedBranches }),
```

- [ ] **Step 8: Write the TypeScript tests**

Append inside the `describe("vscodeRepoClient", ...)` block in
`frontend/src/ipc/vscodeRepoClient.test.ts`, before the final `});`:

```typescript
  it("wires listRecentRepos", async () => {
    const promise = vscodeRepoClient.listRecentRepos();
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "list_recent_repos",
      params: {},
    });
    respond(1, ["/repos/a"]);
    await expect(promise).resolves.toEqual(["/repos/a"]);
  });

  it("wires listOpenRepos", async () => {
    const promise = vscodeRepoClient.listOpenRepos();
    respond(1, { entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" });
    await expect(promise).resolves.toEqual({
      entries: [{ path: "/repos/a", workspaceId: null }],
      activePath: "/repos/a",
    });
  });

  it("wires persistOpenRepos", async () => {
    const promise = vscodeRepoClient.persistOpenRepos(
      [{ path: "/repos/a", workspaceId: null }],
      "/repos/a",
    );
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "persist_open_repos",
      params: { entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" },
    });
    respond(1, null);
    await expect(promise).resolves.toBeNull();
  });

  it("wires scanReposInRoot", async () => {
    const promise = vscodeRepoClient.scanReposInRoot("/repos");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "scan_repos_in_root",
      params: { root: "/repos" },
    });
    respond(1, ["/repos/a"]);
    await expect(promise).resolves.toEqual(["/repos/a"]);
  });

  it("wires listWorkspaces", async () => {
    const promise = vscodeRepoClient.listWorkspaces();
    respond(1, [{ id: "w1", name: "Suite", rootPath: "/repos/suite", memberPaths: [] }]);
    await expect(promise).resolves.toEqual([
      { id: "w1", name: "Suite", rootPath: "/repos/suite", memberPaths: [] },
    ]);
  });

  it("wires saveWorkspace", async () => {
    const promise = vscodeRepoClient.saveWorkspace("Suite", "/repos/suite", ["/repos/suite/api"]);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "save_workspace",
      params: { name: "Suite", root: "/repos/suite", members: ["/repos/suite/api"] },
    });
    respond(1, "w1");
    await expect(promise).resolves.toBe("w1");
  });

  it("wires updateWorkspace", async () => {
    const promise = vscodeRepoClient.updateWorkspace("w1", "Renamed", ["/repos/suite/web"]);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "update_workspace",
      params: { id: "w1", name: "Renamed", members: ["/repos/suite/web"] },
    });
    respond(1, null);
    await expect(promise).resolves.toBeNull();
  });

  it("wires deleteWorkspace", async () => {
    const promise = vscodeRepoClient.deleteWorkspace("w1");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "delete_workspace",
      params: { id: "w1" },
    });
    respond(1, null);
    await expect(promise).resolves.toBeNull();
  });

  it("wires getGraphBranchSelection and setGraphBranchSelection", async () => {
    const getPromise = vscodeRepoClient.getGraphBranchSelection("/repo");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_graph_branch_selection",
      params: { repoPath: "/repo" },
    });
    respond(1, ["main"]);
    await expect(getPromise).resolves.toEqual(["main"]);

    const setPromise = vscodeRepoClient.setGraphBranchSelection("/repo", ["main", "feature"]);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "set_graph_branch_selection",
      params: { repoPath: "/repo", selectedBranches: ["main", "feature"] },
    });
    respond(2, null);
    await expect(setPromise).resolves.toBeNull();
  });
```

- [ ] **Step 9: Run**

Run: `cd frontend && pnpm test -- --run vscodeRepoClient`
Expected: all pass.

- [ ] **Step 10: Commit the TypeScript side**

```bash
git add frontend/src/ipc/vscodeRepoClient.ts frontend/src/ipc/vscodeRepoClient.test.ts
git commit -m "feat(frontend): wire vscodeRepoClient recent repos, open repos, workspaces, graph branch selection"
```

---

### Task 2: Staging and commit

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`
- Modify: `frontend/src/ipc/vscodeRepoClient.ts`
- Modify: `frontend/src/ipc/vscodeRepoClient.test.ts`

**Interfaces:**
- Consumes: `WorkerHandle::{get_commit_files, stage_file, unstage_file, stage_hunk, unstage_hunk,
  discard_hunk, commit}` (`crates/repo-service/src/worker/status.rs`, called the same way
  `crates/tauri-app/src/commands/status.rs:77-158` already calls them).
- Produces: no new DTOs — every one of these seven methods takes/returns only strings, bools,
  u32s, or unit.

Wires `getCommitFiles`, `stageFile`, `unstageFile`, `stageHunk`, `unstageHunk`, `discardHunk`,
`commit`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs`:

```rust
#[test]
fn stage_commit_and_get_commit_files_round_trip_through_the_sidecar() {
    let (dir, _repo) = init_repo();
    write_file(dir.path(), "new.txt", "hello");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let staged = sidecar.call(
        2,
        "stage_file",
        serde_json::json!({"repoPath": repo_path, "path": "new.txt"}),
    );
    assert_eq!(staged["result"], serde_json::Value::Null);

    let committed = sidecar.call(
        3,
        "commit",
        serde_json::json!({"repoPath": repo_path, "message": "add new.txt"}),
    );
    let commit_id = committed["result"].as_str().expect("commit id").to_string();
    assert_eq!(commit_id.len(), 40);

    let files = sidecar.call(
        4,
        "get_commit_files",
        serde_json::json!({"repoPath": repo_path, "commitId": commit_id}),
    );
    assert_eq!(files["result"], serde_json::json!(["new.txt"]));

    let unstaged = sidecar.call(
        5,
        "unstage_file",
        serde_json::json!({"repoPath": repo_path, "path": "new.txt"}),
    );
    assert_eq!(unstaged["result"], serde_json::Value::Null);
}

#[test]
fn stage_unstage_and_discard_hunk_round_trip_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\nline two\n");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "tracked.txt", "line one changed\nline two\n");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let diff = sidecar.call(
        2,
        "get_working_diff",
        serde_json::json!({"repoPath": repo_path, "path": "tracked.txt", "staged": false}),
    );
    let hunk = &diff["result"][0];
    let old_start = hunk["oldStart"].as_u64().unwrap();
    let new_start = hunk["newStart"].as_u64().unwrap();

    let staged = sidecar.call(
        3,
        "stage_hunk",
        serde_json::json!({"repoPath": repo_path, "path": "tracked.txt", "oldStart": old_start, "newStart": new_start}),
    );
    assert_eq!(staged["result"], serde_json::Value::Null);

    let status = sidecar.call(4, "get_status", serde_json::json!({"repoPath": repo_path}));
    assert!(status["result"][0]["staged"].as_bool().unwrap());

    let unstaged = sidecar.call(
        5,
        "unstage_hunk",
        serde_json::json!({"repoPath": repo_path, "path": "tracked.txt", "oldStart": old_start, "newStart": new_start}),
    );
    assert_eq!(unstaged["result"], serde_json::Value::Null);

    let discarded = sidecar.call(
        6,
        "discard_hunk",
        serde_json::json!({"repoPath": repo_path, "path": "tracked.txt", "oldStart": old_start, "newStart": new_start}),
    );
    assert_eq!(discarded["result"], serde_json::Value::Null);
    let on_disk = std::fs::read_to_string(dir.path().join("tracked.txt")).unwrap();
    assert_eq!(on_disk.replace("\r\n", "\n"), "line one\nline two\n");
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p vscode-sidecar`
Expected: FAIL — `stage_file`/etc. report `unknown method`.

- [ ] **Step 3: Implement the handlers**

Add to the `match` in `dispatch()`:

```rust
        "get_commit_files" => get_commit_files(params, repos),
        "stage_file" => stage_file(params, repos),
        "unstage_file" => unstage_file(params, repos),
        "stage_hunk" => stage_hunk(params, repos),
        "unstage_hunk" => unstage_hunk(params, repos),
        "discard_hunk" => discard_hunk(params, repos),
        "commit" => commit(params, repos),
```

Add the handler functions:

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetCommitFilesParams {
    repo_path: String,
    commit_id: String,
}

fn get_commit_files(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: GetCommitFilesParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let files = worker_handle(repos, &params.repo_path)?.get_commit_files(params.commit_id)?;
    serde_json::to_value(files).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoFilePathParams {
    repo_path: String,
    path: String,
}

fn stage_file(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoFilePathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.stage_file(params.path)?;
    Ok(Value::Null)
}

fn unstage_file(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoFilePathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.unstage_file(params.path)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HunkParams {
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
}

fn stage_hunk(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: HunkParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.stage_hunk(params.path, params.old_start, params.new_start)?;
    Ok(Value::Null)
}

fn unstage_hunk(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: HunkParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.unstage_hunk(params.path, params.old_start, params.new_start)?;
    Ok(Value::Null)
}

fn discard_hunk(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: HunkParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.discard_hunk(params.path, params.old_start, params.new_start)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitParams {
    repo_path: String,
    message: String,
}

fn commit(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: CommitParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let commit_id = worker_handle(repos, &params.repo_path)?.commit(params.message)?;
    Ok(Value::String(commit_id))
}
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p vscode-sidecar`
Expected: all pass (16 tests).

- [ ] **Step 5: Commit the Rust side**

```bash
git add crates/vscode-sidecar/src/dispatch.rs crates/vscode-sidecar/tests/protocol_roundtrip.rs
git commit -m "feat(vscode-sidecar): wire staging and commit"
```

- [ ] **Step 6: Wire the TypeScript client**

Replace these seven stub lines in `frontend/src/ipc/vscodeRepoClient.ts`:

```typescript
  getCommitFiles: notImplemented("getCommitFiles"),
  stageFile: notImplemented("stageFile"),
  unstageFile: notImplemented("unstageFile"),
  stageHunk: notImplemented("stageHunk"),
  unstageHunk: notImplemented("unstageHunk"),
  discardHunk: notImplemented("discardHunk"),
  commit: notImplemented("commit"),
```

with:

```typescript
  getCommitFiles: (repoPath: string, commitId: string) =>
    call<string[]>("get_commit_files", { repoPath, commitId }),
  stageFile: (repoPath: string, path: string) => call<void>("stage_file", { repoPath, path }),
  unstageFile: (repoPath: string, path: string) => call<void>("unstage_file", { repoPath, path }),
  stageHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    call<void>("stage_hunk", { repoPath, path, oldStart, newStart }),
  unstageHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    call<void>("unstage_hunk", { repoPath, path, oldStart, newStart }),
  discardHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    call<void>("discard_hunk", { repoPath, path, oldStart, newStart }),
  commit: (repoPath: string, message: string) => call<string>("commit", { repoPath, message }),
```

- [ ] **Step 7: Write the TypeScript tests**

Append inside `describe("vscodeRepoClient", ...)`:

```typescript
  it("wires getCommitFiles", async () => {
    const promise = vscodeRepoClient.getCommitFiles("/repo", "abc123");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_commit_files",
      params: { repoPath: "/repo", commitId: "abc123" },
    });
    respond(1, ["a.txt"]);
    await expect(promise).resolves.toEqual(["a.txt"]);
  });

  it("wires stageFile and unstageFile", async () => {
    const stagePromise = vscodeRepoClient.stageFile("/repo", "a.txt");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "stage_file",
      params: { repoPath: "/repo", path: "a.txt" },
    });
    respond(1, null);
    await expect(stagePromise).resolves.toBeNull();

    const unstagePromise = vscodeRepoClient.unstageFile("/repo", "a.txt");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "unstage_file",
      params: { repoPath: "/repo", path: "a.txt" },
    });
    respond(2, null);
    await expect(unstagePromise).resolves.toBeNull();
  });

  it("wires stageHunk, unstageHunk, and discardHunk", async () => {
    const stagePromise = vscodeRepoClient.stageHunk("/repo", "a.txt", 1, 1);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "stage_hunk",
      params: { repoPath: "/repo", path: "a.txt", oldStart: 1, newStart: 1 },
    });
    respond(1, null);
    await expect(stagePromise).resolves.toBeNull();

    const unstagePromise = vscodeRepoClient.unstageHunk("/repo", "a.txt", 1, 1);
    respond(2, null);
    await expect(unstagePromise).resolves.toBeNull();

    const discardPromise = vscodeRepoClient.discardHunk("/repo", "a.txt", 1, 1);
    respond(3, null);
    await expect(discardPromise).resolves.toBeNull();
  });

  it("wires commit", async () => {
    const promise = vscodeRepoClient.commit("/repo", "message");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "commit",
      params: { repoPath: "/repo", message: "message" },
    });
    respond(1, "abc123");
    await expect(promise).resolves.toBe("abc123");
  });
```

- [ ] **Step 8: Run**

Run: `cd frontend && pnpm test -- --run vscodeRepoClient`
Expected: all pass.

- [ ] **Step 9: Commit the TypeScript side**

```bash
git add frontend/src/ipc/vscodeRepoClient.ts frontend/src/ipc/vscodeRepoClient.test.ts
git commit -m "feat(frontend): wire vscodeRepoClient staging and commit"
```

---

### Task 3: Branches

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`
- Modify: `frontend/src/ipc/vscodeRepoClient.ts`
- Modify: `frontend/src/ipc/vscodeRepoClient.test.ts`

**Interfaces:**
- Consumes: `WorkerHandle::{list_branches, create_branch, switch_branch, delete_branch,
  rename_branch}` (`crates/tauri-app/src/commands/branch.rs`).
- Produces: `BranchInfoDto` — same shape as
  `crates/tauri-app/src/commands/mod.rs:138-143`'s `BranchInfoDto`.

Wires `listBranches`, `createBranch`, `switchBranch`, `deleteBranch`, `renameBranch`.

- [ ] **Step 1: Write the failing test**

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs`:

```rust
#[test]
fn branch_lifecycle_round_trips_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let created = sidecar.call(
        2,
        "create_branch",
        serde_json::json!({"repoPath": repo_path, "name": "feature", "startPoint": "HEAD"}),
    );
    assert_eq!(created["result"], serde_json::Value::Null);

    let branches = sidecar.call(3, "list_branches", serde_json::json!({"repoPath": repo_path}));
    let list = branches["result"].as_array().expect("branch list");
    assert_eq!(list.len(), 2);
    let feature = list.iter().find(|b| b["name"] == "feature").expect("feature branch");
    assert_eq!(feature["isCurrent"], true);

    let initial_name = list
        .iter()
        .find(|b| b["name"] != "feature")
        .expect("initial branch")["name"]
        .as_str()
        .unwrap()
        .to_string();
    sidecar.call(
        4,
        "switch_branch",
        serde_json::json!({"repoPath": repo_path, "name": initial_name}),
    );

    let renamed = sidecar.call(
        5,
        "rename_branch",
        serde_json::json!({"repoPath": repo_path, "oldName": "feature", "newName": "renamed"}),
    );
    assert_eq!(renamed["result"], serde_json::Value::Null);

    let deleted = sidecar.call(
        6,
        "delete_branch",
        serde_json::json!({"repoPath": repo_path, "name": "renamed", "force": true}),
    );
    assert_eq!(deleted["result"], serde_json::Value::Null);

    let final_branches = sidecar.call(7, "list_branches", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(final_branches["result"].as_array().unwrap().len(), 1);
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p vscode-sidecar`
Expected: FAIL — `create_branch` reports `unknown method`.

- [ ] **Step 3: Implement the handlers**

Add to the `match` in `dispatch()`:

```rust
        "list_branches" => list_branches(params, repos),
        "create_branch" => create_branch(params, repos),
        "switch_branch" => switch_branch(params, repos),
        "delete_branch" => delete_branch(params, repos),
        "rename_branch" => rename_branch(params, repos),
```

Add:

```rust
use git_core::branch::BranchInfo;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BranchInfoDto {
    name: String,
    is_current: bool,
}

impl From<BranchInfo> for BranchInfoDto {
    fn from(branch: BranchInfo) -> Self {
        Self {
            name: branch.name,
            is_current: branch.is_current,
        }
    }
}

fn list_branches(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let branches: Vec<BranchInfoDto> = worker_handle(repos, &params.repo_path)?
        .list_branches()?
        .into_iter()
        .map(BranchInfoDto::from)
        .collect();
    serde_json::to_value(branches).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateBranchParams {
    repo_path: String,
    name: String,
    start_point: String,
}

fn create_branch(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: CreateBranchParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.create_branch(params.name, params.start_point)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SwitchBranchParams {
    repo_path: String,
    name: String,
}

fn switch_branch(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: SwitchBranchParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.switch_branch(params.name)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteBranchParams {
    repo_path: String,
    name: String,
    force: bool,
}

fn delete_branch(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: DeleteBranchParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.delete_branch(params.name, params.force)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameBranchParams {
    repo_path: String,
    old_name: String,
    new_name: String,
}

fn rename_branch(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RenameBranchParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.rename_branch(params.old_name, params.new_name)?;
    Ok(Value::Null)
}
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p vscode-sidecar`
Expected: all pass (17 tests).

- [ ] **Step 5: Commit the Rust side**

```bash
git add crates/vscode-sidecar/src/dispatch.rs crates/vscode-sidecar/tests/protocol_roundtrip.rs
git commit -m "feat(vscode-sidecar): wire branches"
```

- [ ] **Step 6: Wire the TypeScript client**

Add `BranchInfo` to the import list in `frontend/src/ipc/vscodeRepoClient.ts`. Replace:

```typescript
  listBranches: notImplemented("listBranches"),
  createBranch: notImplemented("createBranch"),
  switchBranch: notImplemented("switchBranch"),
  deleteBranch: notImplemented("deleteBranch"),
  renameBranch: notImplemented("renameBranch"),
```

with:

```typescript
  listBranches: (repoPath: string) => call<BranchInfo[]>("list_branches", { repoPath }),
  createBranch: (repoPath: string, name: string, startPoint: string) =>
    call<void>("create_branch", { repoPath, name, startPoint }),
  switchBranch: (repoPath: string, name: string) => call<void>("switch_branch", { repoPath, name }),
  deleteBranch: (repoPath: string, name: string, force: boolean) =>
    call<void>("delete_branch", { repoPath, name, force }),
  renameBranch: (repoPath: string, oldName: string, newName: string) =>
    call<void>("rename_branch", { repoPath, oldName, newName }),
```

- [ ] **Step 7: Write the TypeScript tests**

Append inside `describe("vscodeRepoClient", ...)`:

```typescript
  it("wires listBranches", async () => {
    const promise = vscodeRepoClient.listBranches("/repo");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "list_branches",
      params: { repoPath: "/repo" },
    });
    respond(1, [{ name: "main", isCurrent: true }]);
    await expect(promise).resolves.toEqual([{ name: "main", isCurrent: true }]);
  });

  it("wires createBranch, switchBranch, renameBranch, and deleteBranch", async () => {
    const createPromise = vscodeRepoClient.createBranch("/repo", "feature", "HEAD");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "create_branch",
      params: { repoPath: "/repo", name: "feature", startPoint: "HEAD" },
    });
    respond(1, null);
    await expect(createPromise).resolves.toBeNull();

    const switchPromise = vscodeRepoClient.switchBranch("/repo", "feature");
    respond(2, null);
    await expect(switchPromise).resolves.toBeNull();

    const renamePromise = vscodeRepoClient.renameBranch("/repo", "feature", "renamed");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "rename_branch",
      params: { repoPath: "/repo", oldName: "feature", newName: "renamed" },
    });
    respond(3, null);
    await expect(renamePromise).resolves.toBeNull();

    const deletePromise = vscodeRepoClient.deleteBranch("/repo", "renamed", true);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 4,
      method: "delete_branch",
      params: { repoPath: "/repo", name: "renamed", force: true },
    });
    respond(4, null);
    await expect(deletePromise).resolves.toBeNull();
  });
```

- [ ] **Step 8: Run**

Run: `cd frontend && pnpm test -- --run vscodeRepoClient`
Expected: all pass.

- [ ] **Step 9: Commit the TypeScript side**

```bash
git add frontend/src/ipc/vscodeRepoClient.ts frontend/src/ipc/vscodeRepoClient.test.ts
git commit -m "feat(frontend): wire vscodeRepoClient branches"
```

---

### Task 4: Worktrees

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`
- Modify: `frontend/src/ipc/vscodeRepoClient.ts`
- Modify: `frontend/src/ipc/vscodeRepoClient.test.ts`

**Interfaces:**
- Consumes: `WorkerHandle::{list_worktrees, create_worktree, remove_worktree, prune_worktrees}`
  (`crates/tauri-app/src/commands/worktree.rs`).
- Produces: `WorktreeInfoDto` — same shape as
  `crates/tauri-app/src/commands/mod.rs:145-167`'s `WorktreeInfoDto`, including its empty-string-
  to-`null` `head` conversion.

Wires `listWorktrees`, `createWorktree`, `removeWorktree`, `pruneWorktrees`.

- [ ] **Step 1: Write the failing test**

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs`:

```rust
#[test]
fn worktree_lifecycle_round_trips_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let linked = dir.path().join("feature-tree");
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let created = sidecar.call(
        2,
        "create_worktree",
        serde_json::json!({
            "repoPath": repo_path,
            "name": "feature-tree",
            "path": linked.to_str().unwrap(),
            "branch": "feature",
            "startPoint": "HEAD",
        }),
    );
    assert_eq!(created["result"], serde_json::Value::Null);

    let listed = sidecar.call(3, "list_worktrees", serde_json::json!({"repoPath": repo_path}));
    let worktrees = listed["result"].as_array().expect("worktree list");
    assert!(worktrees.iter().any(|w| w["name"] == "feature-tree" && w["isMain"] == false));

    let removed = sidecar.call(
        4,
        "remove_worktree",
        serde_json::json!({"repoPath": repo_path, "name": "feature-tree"}),
    );
    assert_eq!(removed["result"], serde_json::Value::Null);
    assert!(!linked.exists());

    let pruned = sidecar.call(5, "prune_worktrees", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(pruned["result"], serde_json::Value::Null);
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p vscode-sidecar`
Expected: FAIL — `create_worktree` reports `unknown method`.

- [ ] **Step 3: Implement the handlers**

Add to the `match` in `dispatch()`:

```rust
        "list_worktrees" => list_worktrees(params, repos),
        "create_worktree" => create_worktree(params, repos),
        "remove_worktree" => remove_worktree(params, repos),
        "prune_worktrees" => prune_worktrees(params, repos),
```

Add:

```rust
use git_core::worktree::WorktreeInfo;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorktreeInfoDto {
    name: String,
    path: String,
    head: Option<String>,
    is_main: bool,
    is_locked: bool,
    is_prunable: bool,
}

impl From<WorktreeInfo> for WorktreeInfoDto {
    fn from(worktree: WorktreeInfo) -> Self {
        Self {
            name: worktree.name,
            path: worktree.path.to_string_lossy().into_owned(),
            head: (!worktree.head.is_empty()).then_some(worktree.head),
            is_main: worktree.is_main,
            is_locked: worktree.is_locked,
            is_prunable: worktree.is_prunable,
        }
    }
}

fn list_worktrees(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let worktrees: Vec<WorktreeInfoDto> = worker_handle(repos, &params.repo_path)?
        .list_worktrees()?
        .into_iter()
        .map(WorktreeInfoDto::from)
        .collect();
    serde_json::to_value(worktrees).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateWorktreeParams {
    repo_path: String,
    name: String,
    path: String,
    branch: String,
    start_point: Option<String>,
}

fn create_worktree(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: CreateWorktreeParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.create_worktree(
        params.name,
        PathBuf::from(params.path),
        params.branch,
        params.start_point,
    )?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorktreeNameParams {
    repo_path: String,
    name: String,
}

fn remove_worktree(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: WorktreeNameParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.remove_worktree(params.name)?;
    Ok(Value::Null)
}

fn prune_worktrees(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.prune_worktrees()?;
    Ok(Value::Null)
}
```

`PathBuf` is already imported by Task 1.

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p vscode-sidecar`
Expected: all pass (18 tests).

- [ ] **Step 5: Commit the Rust side**

```bash
git add crates/vscode-sidecar/src/dispatch.rs crates/vscode-sidecar/tests/protocol_roundtrip.rs
git commit -m "feat(vscode-sidecar): wire worktrees"
```

- [ ] **Step 6: Wire the TypeScript client**

Add `WorktreeInfo` to the import list. Replace:

```typescript
  listWorktrees: notImplemented("listWorktrees"),
  createWorktree: notImplemented("createWorktree"),
  removeWorktree: notImplemented("removeWorktree"),
  pruneWorktrees: notImplemented("pruneWorktrees"),
```

with:

```typescript
  listWorktrees: (repoPath: string) => call<WorktreeInfo[]>("list_worktrees", { repoPath }),
  createWorktree: (
    repoPath: string,
    name: string,
    path: string,
    branch: string,
    startPoint: string | null,
  ) => call<void>("create_worktree", { repoPath, name, path, branch, startPoint }),
  removeWorktree: (repoPath: string, name: string) => call<void>("remove_worktree", { repoPath, name }),
  pruneWorktrees: (repoPath: string) => call<void>("prune_worktrees", { repoPath }),
```

- [ ] **Step 7: Write the TypeScript tests**

Append inside `describe("vscodeRepoClient", ...)`:

```typescript
  it("wires listWorktrees", async () => {
    const promise = vscodeRepoClient.listWorktrees("/repo");
    respond(1, [
      { name: "main", path: "/repo", head: "refs/heads/main", isMain: true, isLocked: false, isPrunable: false },
    ]);
    await expect(promise).resolves.toEqual([
      { name: "main", path: "/repo", head: "refs/heads/main", isMain: true, isLocked: false, isPrunable: false },
    ]);
  });

  it("wires createWorktree, removeWorktree, and pruneWorktrees", async () => {
    const createPromise = vscodeRepoClient.createWorktree("/repo", "feature-tree", "/repo-feature", "feature", "HEAD");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "create_worktree",
      params: { repoPath: "/repo", name: "feature-tree", path: "/repo-feature", branch: "feature", startPoint: "HEAD" },
    });
    respond(1, null);
    await expect(createPromise).resolves.toBeNull();

    const removePromise = vscodeRepoClient.removeWorktree("/repo", "feature-tree");
    respond(2, null);
    await expect(removePromise).resolves.toBeNull();

    const prunePromise = vscodeRepoClient.pruneWorktrees("/repo");
    respond(3, null);
    await expect(prunePromise).resolves.toBeNull();
  });
```

- [ ] **Step 8: Run**

Run: `cd frontend && pnpm test -- --run vscodeRepoClient`
Expected: all pass.

- [ ] **Step 9: Commit the TypeScript side**

```bash
git add frontend/src/ipc/vscodeRepoClient.ts frontend/src/ipc/vscodeRepoClient.test.ts
git commit -m "feat(frontend): wire vscodeRepoClient worktrees"
```

---

### Task 5: Submodules

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`
- Modify: `frontend/src/ipc/vscodeRepoClient.ts`
- Modify: `frontend/src/ipc/vscodeRepoClient.test.ts`

**Interfaces:**
- Consumes: `WorkerHandle::{list_submodules, init_submodule, update_submodule}`
  (`crates/tauri-app/src/commands/submodule.rs`).
- Produces: `SubmoduleInfoDto` — same shape as
  `crates/tauri-app/src/commands/mod.rs:216-236`'s `SubmoduleInfoDto`.

Wires `listSubmodules`, `initSubmodule`, `updateSubmodule`.

- [ ] **Step 1: Write the failing test**

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs`:

```rust
#[test]
fn submodule_lifecycle_round_trips_through_the_sidecar() {
    let parent_dir = tempfile::TempDir::new().expect("create parent dir");
    let child_dir = tempfile::TempDir::new().expect("create child dir");
    let child = git2::Repository::init(child_dir.path()).expect("init child repo");
    {
        let mut config = child.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    write_file(child_dir.path(), "child.txt", "hello");
    commit_all(&child, "child commit");

    let parent = git2::Repository::init(parent_dir.path()).expect("init parent repo");
    {
        let mut config = parent.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    let mut submodule = parent
        .submodule(child_dir.path().to_str().unwrap(), std::path::Path::new("deps/child"), true)
        .expect("configure submodule");
    submodule.clone(None).expect("clone submodule");
    submodule.add_to_index(true).expect("stage submodule");
    submodule.add_finalize().expect("finalize submodule");
    drop(submodule);
    commit_all(&parent, "add submodule");
    drop(parent);

    let checkout_dir = tempfile::TempDir::new().expect("create checkout dir");
    git2::Repository::clone(parent_dir.path().to_str().unwrap(), checkout_dir.path())
        .expect("clone parent");
    let repo_path = checkout_dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let before = sidecar.call(2, "list_submodules", serde_json::json!({"repoPath": repo_path}));
    let list = before["result"].as_array().expect("submodule list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["path"], "deps/child");
    assert_eq!(list[0]["initialized"], false);

    let inited = sidecar.call(
        3,
        "init_submodule",
        serde_json::json!({"repoPath": repo_path, "path": "deps/child"}),
    );
    assert_eq!(inited["result"], serde_json::Value::Null);

    let updated = sidecar.call(
        4,
        "update_submodule",
        serde_json::json!({"repoPath": repo_path, "path": "deps/child", "recursive": false}),
    );
    assert_eq!(updated["result"], serde_json::Value::Null);

    let after = sidecar.call(5, "list_submodules", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(after["result"][0]["initialized"], true);
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p vscode-sidecar`
Expected: FAIL — `list_submodules` reports `unknown method`.

- [ ] **Step 3: Implement the handlers**

Add to the `match` in `dispatch()`:

```rust
        "list_submodules" => list_submodules(params, repos),
        "init_submodule" => init_submodule(params, repos),
        "update_submodule" => update_submodule(params, repos),
```

Add:

```rust
use git_core::submodule::SubmoduleInfo;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SubmoduleInfoDto {
    path: String,
    url: Option<String>,
    gitlink_id: Option<String>,
    initialized: bool,
    head_id: Option<String>,
}

impl From<SubmoduleInfo> for SubmoduleInfoDto {
    fn from(submodule: SubmoduleInfo) -> Self {
        Self {
            path: submodule.path,
            url: submodule.url,
            gitlink_id: submodule.gitlink_id,
            initialized: submodule.initialized,
            head_id: submodule.head_id,
        }
    }
}

fn list_submodules(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let submodules: Vec<SubmoduleInfoDto> = worker_handle(repos, &params.repo_path)?
        .list_submodules()?
        .into_iter()
        .map(SubmoduleInfoDto::from)
        .collect();
    serde_json::to_value(submodules).map_err(|error| error.to_string())
}

fn init_submodule(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoFilePathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.init_submodule(params.path)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateSubmoduleParams {
    repo_path: String,
    path: String,
    recursive: bool,
}

fn update_submodule(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: UpdateSubmoduleParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.update_submodule(params.path, params.recursive)?;
    Ok(Value::Null)
}
```

`RepoFilePathParams` (with fields `repo_path`, `path`) already exists from Task 2.

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p vscode-sidecar`
Expected: all pass (19 tests).

- [ ] **Step 5: Commit the Rust side**

```bash
git add crates/vscode-sidecar/src/dispatch.rs crates/vscode-sidecar/tests/protocol_roundtrip.rs
git commit -m "feat(vscode-sidecar): wire submodules"
```

- [ ] **Step 6: Wire the TypeScript client**

Add `SubmoduleInfo` to the import list. Replace:

```typescript
  listSubmodules: notImplemented("listSubmodules"),
  initSubmodule: notImplemented("initSubmodule"),
  updateSubmodule: notImplemented("updateSubmodule"),
```

with:

```typescript
  listSubmodules: (repoPath: string) => call<SubmoduleInfo[]>("list_submodules", { repoPath }),
  initSubmodule: (repoPath: string, path: string) => call<void>("init_submodule", { repoPath, path }),
  updateSubmodule: (repoPath: string, path: string, recursive: boolean) =>
    call<void>("update_submodule", { repoPath, path, recursive }),
```

- [ ] **Step 7: Write the TypeScript test**

Append inside `describe("vscodeRepoClient", ...)`:

```typescript
  it("wires listSubmodules, initSubmodule, and updateSubmodule", async () => {
    const listPromise = vscodeRepoClient.listSubmodules("/repo");
    respond(1, [{ path: "deps/child", url: null, gitlinkId: null, initialized: false, headId: null }]);
    await expect(listPromise).resolves.toEqual([
      { path: "deps/child", url: null, gitlinkId: null, initialized: false, headId: null },
    ]);

    const initPromise = vscodeRepoClient.initSubmodule("/repo", "deps/child");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "init_submodule",
      params: { repoPath: "/repo", path: "deps/child" },
    });
    respond(2, null);
    await expect(initPromise).resolves.toBeNull();

    const updatePromise = vscodeRepoClient.updateSubmodule("/repo", "deps/child", true);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "update_submodule",
      params: { repoPath: "/repo", path: "deps/child", recursive: true },
    });
    respond(3, null);
    await expect(updatePromise).resolves.toBeNull();
  });
```

- [ ] **Step 8: Run**

Run: `cd frontend && pnpm test -- --run vscodeRepoClient`
Expected: all pass.

- [ ] **Step 9: Commit the TypeScript side**

```bash
git add frontend/src/ipc/vscodeRepoClient.ts frontend/src/ipc/vscodeRepoClient.test.ts
git commit -m "feat(frontend): wire vscodeRepoClient submodules"
```

---

### Task 6: Reflog

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`
- Modify: `frontend/src/ipc/vscodeRepoClient.ts`
- Modify: `frontend/src/ipc/vscodeRepoClient.test.ts`

**Interfaces:**
- Consumes: `WorkerHandle::{list_reflog_refs, get_reflog, restore_reflog_entry}`
  (`crates/tauri-app/src/commands/reflog.rs`).
- Produces: `ReflogEntryDto` — same shape as
  `crates/tauri-app/src/commands/mod.rs:238-264`'s `ReflogEntryDto`.

Wires `listReflogRefs`, `getReflog`, `restoreReflogEntry`.

- [ ] **Step 1: Write the failing test**

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs`:

```rust
#[test]
fn reflog_lists_and_restores_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "first commit");
    write_file(dir.path(), "file.txt", "v2");
    commit_all(&repo, "second commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let refs = sidecar.call(2, "list_reflog_refs", serde_json::json!({"repoPath": repo_path}));
    let refs_list = refs["result"].as_array().expect("reflog refs");
    assert!(refs_list.iter().any(|r| r == "HEAD"));

    let entries = sidecar.call(
        3,
        "get_reflog",
        serde_json::json!({"repoPath": repo_path, "reference": "HEAD"}),
    );
    let entries_list = entries["result"].as_array().expect("reflog entries");
    assert_eq!(entries_list[0]["summary"], "second commit");
    let first_commit_id = entries_list[1]["newId"].as_str().unwrap().to_string();

    let restored = sidecar.call(
        4,
        "restore_reflog_entry",
        serde_json::json!({"repoPath": repo_path, "reference": "HEAD", "newId": first_commit_id}),
    );
    assert_eq!(restored["result"], serde_json::Value::Null);
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p vscode-sidecar`
Expected: FAIL — `list_reflog_refs` reports `unknown method`.

- [ ] **Step 3: Implement the handlers**

Add to the `match` in `dispatch()`:

```rust
        "list_reflog_refs" => list_reflog_refs(params, repos),
        "get_reflog" => get_reflog(params, repos),
        "restore_reflog_entry" => restore_reflog_entry(params, repos),
```

Add:

```rust
use git_core::reflog::ReflogEntry;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReflogEntryDto {
    reference: String,
    old_id: String,
    new_id: String,
    committer_name: String,
    committer_email: String,
    timestamp: i64,
    message: String,
    summary: Option<String>,
}

impl From<ReflogEntry> for ReflogEntryDto {
    fn from(entry: ReflogEntry) -> Self {
        Self {
            reference: entry.reference,
            old_id: entry.old_id,
            new_id: entry.new_id,
            committer_name: entry.committer_name,
            committer_email: entry.committer_email,
            timestamp: entry.timestamp,
            message: entry.message,
            summary: entry.summary,
        }
    }
}

fn list_reflog_refs(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let refs = worker_handle(repos, &params.repo_path)?.list_reflog_refs()?;
    serde_json::to_value(refs).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetReflogParams {
    repo_path: String,
    reference: String,
}

fn get_reflog(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: GetReflogParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let entries: Vec<ReflogEntryDto> = worker_handle(repos, &params.repo_path)?
        .get_reflog(params.reference)?
        .into_iter()
        .map(ReflogEntryDto::from)
        .collect();
    serde_json::to_value(entries).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreReflogEntryParams {
    repo_path: String,
    reference: String,
    new_id: String,
}

fn restore_reflog_entry(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RestoreReflogEntryParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.restore_reflog_entry(params.reference, params.new_id)?;
    Ok(Value::Null)
}
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p vscode-sidecar`
Expected: all pass (20 tests).

- [ ] **Step 5: Commit the Rust side**

```bash
git add crates/vscode-sidecar/src/dispatch.rs crates/vscode-sidecar/tests/protocol_roundtrip.rs
git commit -m "feat(vscode-sidecar): wire reflog"
```

- [ ] **Step 6: Wire the TypeScript client**

Add `ReflogEntry` to the import list. Replace:

```typescript
  listReflogRefs: notImplemented("listReflogRefs"),
  getReflog: notImplemented("getReflog"),
  restoreReflogEntry: notImplemented("restoreReflogEntry"),
```

with:

```typescript
  listReflogRefs: (repoPath: string) => call<string[]>("list_reflog_refs", { repoPath }),
  getReflog: (repoPath: string, reference: string) =>
    call<ReflogEntry[]>("get_reflog", { repoPath, reference }),
  restoreReflogEntry: (repoPath: string, reference: string, newId: string) =>
    call<void>("restore_reflog_entry", { repoPath, reference, newId }),
```

- [ ] **Step 7: Write the TypeScript test**

Append inside `describe("vscodeRepoClient", ...)`:

```typescript
  it("wires listReflogRefs, getReflog, and restoreReflogEntry", async () => {
    const refsPromise = vscodeRepoClient.listReflogRefs("/repo");
    respond(1, ["HEAD"]);
    await expect(refsPromise).resolves.toEqual(["HEAD"]);

    const reflogPromise = vscodeRepoClient.getReflog("/repo", "HEAD");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "get_reflog",
      params: { repoPath: "/repo", reference: "HEAD" },
    });
    respond(2, [
      {
        reference: "HEAD",
        oldId: "1111111",
        newId: "2222222",
        committerName: "Test User",
        committerEmail: "test@example.com",
        timestamp: 1_725_000_000,
        message: "commit: second commit",
        summary: "second commit",
      },
    ]);
    await expect(reflogPromise).resolves.toHaveLength(1);

    const restorePromise = vscodeRepoClient.restoreReflogEntry("/repo", "HEAD", "1111111");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "restore_reflog_entry",
      params: { repoPath: "/repo", reference: "HEAD", newId: "1111111" },
    });
    respond(3, null);
    await expect(restorePromise).resolves.toBeNull();
  });
```

- [ ] **Step 8: Run**

Run: `cd frontend && pnpm test -- --run vscodeRepoClient`
Expected: all pass.

- [ ] **Step 9: Commit the TypeScript side**

```bash
git add frontend/src/ipc/vscodeRepoClient.ts frontend/src/ipc/vscodeRepoClient.test.ts
git commit -m "feat(frontend): wire vscodeRepoClient reflog"
```

---

### Task 7: Remotes and credentials

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`
- Modify: `frontend/src/ipc/vscodeRepoClient.ts`
- Modify: `frontend/src/ipc/vscodeRepoClient.test.ts`

**Interfaces:**
- Consumes: `WorkerHandle::{list_remotes, get_remote_auth_mode, list_remote_branches,
  get_current_upstream, get_remote_upstreams, add_remote, rename_remote, update_remote_urls,
  remove_remote, save_https_credential, forget_https_credential, set_remote_auth_mode,
  set_current_upstream, clear_current_upstream}` (`crates/repo-service/src/worker/remote.rs`,
  called the same way `crates/tauri-app/src/commands/remote.rs` already calls them).
- Produces: `RemoteInfoDto`/`RemoteAuthModeDto`/`UpstreamInfoDto` — same shape as
  `crates/tauri-app/src/commands/mod.rs:266-343,367-375`.

Wires `listRemotes`, `listRemoteBranches`, `getCurrentUpstream`, `getRemoteUpstreams`,
`addRemote`, `renameRemote`, `updateRemoteUrls`, `removeRemote`, `saveHttpsCredential`,
`forgetHttpsCredential`, `setRemoteAuthMode`, `setCurrentUpstream`, `clearCurrentUpstream`. This
task uses in-memory-credential-store test doubles the way `crates/repo-service`'s own tests do —
it does not touch the real OS keychain (see Step 1's use of the `forge-fixture-override`
feature, same one `e2e/` builds with).

- [ ] **Step 1: Write the failing tests**

`save_https_credential`/`forget_https_credential` go through `repo-service`'s
`KeyringCredentialStore` in a normal build, which would hit the real OS keychain/D-Bus secrets
service in a test run. `crates/tauri-app`'s own E2E build avoids this with the
`forge-fixture-override` Cargo feature (`crates/repo-service/src/worker/mod.rs:391-403`), which
swaps in an in-memory store. Add the same feature to `vscode-sidecar` and build the test binary
with it.

Edit `crates/vscode-sidecar/Cargo.toml` — add a `[features]` section (after `[dependencies]`):

```toml
[features]
forge-fixture-override = ["repo-service/forge-fixture-override"]
```

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs`:

```rust
#[test]
fn remote_lifecycle_round_trips_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    repo.remote("origin", "https://example.com/owner/repo.git").unwrap();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let remotes = sidecar.call(2, "list_remotes", serde_json::json!({"repoPath": repo_path}));
    let list = remotes["result"].as_array().expect("remote list");
    assert_eq!(list[0]["name"], "origin");
    assert_eq!(list[0]["fetchUrl"], "https://example.com/owner/repo.git");
    assert_eq!(list[0]["authMode"], serde_json::Value::Null);

    let renamed = sidecar.call(
        3,
        "rename_remote",
        serde_json::json!({"repoPath": repo_path, "oldName": "origin", "newName": "upstream"}),
    );
    assert_eq!(renamed["result"], serde_json::Value::Null);

    let updated = sidecar.call(
        4,
        "update_remote_urls",
        serde_json::json!({"repoPath": repo_path, "name": "upstream", "fetchUrl": "https://example.com/owner/repo2.git", "pushUrl": null}),
    );
    assert_eq!(updated["result"], serde_json::Value::Null);

    let auth_set = sidecar.call(
        5,
        "set_remote_auth_mode",
        serde_json::json!({"repoPath": repo_path, "remoteName": "upstream", "mode": "HttpsToken", "username": "alice"}),
    );
    assert_eq!(auth_set["result"], serde_json::Value::Null);

    let remotes_after = sidecar.call(6, "list_remotes", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(remotes_after["result"][0]["authMode"], "HttpsToken");
    assert_eq!(remotes_after["result"][0]["authUsername"], "alice");

    let removed = sidecar.call(
        7,
        "remove_remote",
        serde_json::json!({"repoPath": repo_path, "name": "upstream", "clearUpstreams": true}),
    );
    assert_eq!(removed["result"], serde_json::Value::Null);
    let remotes_final = sidecar.call(8, "list_remotes", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(remotes_final["result"], serde_json::json!([]));
}

#[test]
fn add_remote_https_credential_and_current_upstream_round_trip_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let added = sidecar.call(
        2,
        "add_remote",
        serde_json::json!({"repoPath": repo_path, "name": "origin", "fetchUrl": "https://example.com/owner/repo.git", "pushUrl": null}),
    );
    assert_eq!(added["result"], serde_json::Value::Null);

    let saved = sidecar.call(
        3,
        "save_https_credential",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin", "username": "alice", "token": "secret"}),
    );
    assert_eq!(saved["result"], serde_json::Value::Null);

    let forgotten = sidecar.call(
        4,
        "forget_https_credential",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin"}),
    );
    assert_eq!(forgotten["result"], serde_json::Value::Null);

    let no_upstream = sidecar.call(5, "get_current_upstream", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(no_upstream["result"], serde_json::Value::Null);

    let set = sidecar.call(
        6,
        "set_current_upstream",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin", "remoteBranch": "main"}),
    );
    assert_eq!(set["result"], serde_json::Value::Null);

    let upstream = sidecar.call(7, "get_current_upstream", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(upstream["result"]["remoteName"], "origin");
    assert_eq!(upstream["result"]["remoteBranch"], "main");

    let upstreams = sidecar.call(
        8,
        "get_remote_upstreams",
        serde_json::json!({"repoPath": repo_path, "name": "origin"}),
    );
    assert_eq!(upstreams["result"].as_array().unwrap().len(), 1);

    let cleared = sidecar.call(9, "clear_current_upstream", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(cleared["result"], serde_json::Value::Null);
    let after_clear = sidecar.call(10, "get_current_upstream", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(after_clear["result"], serde_json::Value::Null);

    let branches = sidecar.call(
        11,
        "list_remote_branches",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin"}),
    );
    assert_eq!(branches["result"], serde_json::json!([]));
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p vscode-sidecar --features forge-fixture-override`
Expected: FAIL — `list_remotes` reports `unknown method`.

- [ ] **Step 3: Implement the handlers**

Add to the `match` in `dispatch()`:

```rust
        "list_remotes" => list_remotes(params, repos),
        "list_remote_branches" => list_remote_branches(params, repos),
        "get_current_upstream" => get_current_upstream(params, repos),
        "get_remote_upstreams" => get_remote_upstreams(params, repos),
        "add_remote" => add_remote(params, repos),
        "rename_remote" => rename_remote(params, repos),
        "update_remote_urls" => update_remote_urls(params, repos),
        "remove_remote" => remove_remote(params, repos),
        "save_https_credential" => save_https_credential(params, repos),
        "forget_https_credential" => forget_https_credential(params, repos),
        "set_remote_auth_mode" => set_remote_auth_mode(params, repos),
        "set_current_upstream" => set_current_upstream(params, repos),
        "clear_current_upstream" => clear_current_upstream(params, repos),
```

Add:

```rust
#[derive(Clone, Serialize, Deserialize)]
enum RemoteAuthModeDto {
    HttpsToken,
    SshAgent,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteInfoDto {
    name: String,
    fetch_url: String,
    push_url: Option<String>,
    auth_mode: Option<RemoteAuthModeDto>,
    auth_username: Option<String>,
}

impl From<(git_core::remote::RemoteInfo, Option<git_core::remote::RemoteAuthMode>)> for RemoteInfoDto {
    fn from(
        (remote, profile): (git_core::remote::RemoteInfo, Option<git_core::remote::RemoteAuthMode>),
    ) -> Self {
        let (auth_mode, auth_username) = match profile {
            Some(git_core::remote::RemoteAuthMode::HttpsToken { username }) => {
                (Some(RemoteAuthModeDto::HttpsToken), Some(username))
            }
            Some(git_core::remote::RemoteAuthMode::SshAgent) => (Some(RemoteAuthModeDto::SshAgent), None),
            None => (None, None),
        };
        Self {
            name: remote.name,
            fetch_url: remote.fetch_url,
            push_url: remote.push_url,
            auth_mode,
            auth_username,
        }
    }
}

fn list_remotes(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let worker = worker_handle(repos, &params.repo_path)?;
    let remotes: Result<Vec<RemoteInfoDto>, String> = worker
        .list_remotes()?
        .into_iter()
        .map(|remote| {
            let profile = worker.get_remote_auth_mode(remote.name.clone())?;
            Ok(RemoteInfoDto::from((remote, profile)))
        })
        .collect();
    serde_json::to_value(remotes?).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteNameParams {
    repo_path: String,
    remote_name: String,
}

fn list_remote_branches(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RemoteNameParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let branches = worker_handle(repos, &params.repo_path)?.list_remote_branches(params.remote_name)?;
    serde_json::to_value(branches).map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpstreamInfoDto {
    local_branch: String,
    remote_name: String,
    remote_branch: String,
}

impl From<git_core::remote::UpstreamInfo> for UpstreamInfoDto {
    fn from(upstream: git_core::remote::UpstreamInfo) -> Self {
        Self {
            local_branch: upstream.local_branch,
            remote_name: upstream.remote_name,
            remote_branch: upstream.remote_branch,
        }
    }
}

fn get_current_upstream(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let upstream = worker_handle(repos, &params.repo_path)?
        .get_current_upstream()?
        .map(UpstreamInfoDto::from);
    serde_json::to_value(upstream).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetRemoteUpstreamsParams {
    repo_path: String,
    name: String,
}

fn get_remote_upstreams(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: GetRemoteUpstreamsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let upstreams: Vec<UpstreamInfoDto> = worker_handle(repos, &params.repo_path)?
        .get_remote_upstreams(params.name)?
        .into_iter()
        .map(UpstreamInfoDto::from)
        .collect();
    serde_json::to_value(upstreams).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddRemoteParams {
    repo_path: String,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
}

fn add_remote(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: AddRemoteParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.add_remote(params.name, params.fetch_url, params.push_url)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameRemoteParams {
    repo_path: String,
    old_name: String,
    new_name: String,
}

fn rename_remote(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RenameRemoteParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.rename_remote(params.old_name, params.new_name)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateRemoteUrlsParams {
    repo_path: String,
    name: String,
    fetch_url: String,
    push_url: Option<String>,
}

fn update_remote_urls(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: UpdateRemoteUrlsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .update_remote_urls(params.name, params.fetch_url, params.push_url)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoveRemoteParams {
    repo_path: String,
    name: String,
    clear_upstreams: bool,
}

fn remove_remote(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RemoveRemoteParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.remove_remote(params.name, params.clear_upstreams)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveHttpsCredentialParams {
    repo_path: String,
    remote_name: String,
    username: String,
    token: String,
}

fn save_https_credential(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: SaveHttpsCredentialParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.save_https_credential(
        params.remote_name,
        params.username,
        params.token,
    )?;
    Ok(Value::Null)
}

fn forget_https_credential(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RemoteNameParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.forget_https_credential(params.remote_name)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetRemoteAuthModeParams {
    repo_path: String,
    remote_name: String,
    mode: RemoteAuthModeDto,
    username: Option<String>,
}

fn set_remote_auth_mode(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: SetRemoteAuthModeParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let mode = match params.mode {
        RemoteAuthModeDto::HttpsToken => git_core::remote::RemoteAuthMode::HttpsToken {
            username: params
                .username
                .filter(|username| !username.trim().is_empty())
                .ok_or_else(|| "HTTPS username is required".to_string())?,
        },
        RemoteAuthModeDto::SshAgent => git_core::remote::RemoteAuthMode::SshAgent,
    };
    worker_handle(repos, &params.repo_path)?.set_remote_auth_mode(params.remote_name, mode)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetCurrentUpstreamParams {
    repo_path: String,
    remote_name: String,
    remote_branch: String,
}

fn set_current_upstream(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: SetCurrentUpstreamParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .set_current_upstream(params.remote_name, params.remote_branch)?;
    Ok(Value::Null)
}

fn clear_current_upstream(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.clear_current_upstream()?;
    Ok(Value::Null)
}
```

- [ ] **Step 4: Run to see them pass**

Run: `cargo test -p vscode-sidecar --features forge-fixture-override`
Expected: all pass (22 tests). Note: from this task on, always pass
`--features forge-fixture-override` when running `vscode-sidecar`'s tests locally, mirroring
how `crates/tauri-app`'s own tests are run — the workspace-wide `cargo test --workspace` in
Task 14 enables it explicitly (see that task).

- [ ] **Step 5: Commit the Rust side**

```bash
git add crates/vscode-sidecar/Cargo.toml crates/vscode-sidecar/src/dispatch.rs crates/vscode-sidecar/tests/protocol_roundtrip.rs
git commit -m "feat(vscode-sidecar): wire remotes and credentials"
```

- [ ] **Step 6: Wire the TypeScript client**

Add `RemoteInfo`, `RemoteAuthMode`, `UpstreamInfo` to the import list. Replace:

```typescript
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
```

with (mirroring `tauriRepoClient.ts:118-141`'s call shapes, including that `addRemote`/
`updateRemoteUrls` validate URLs client-side first via `validateRemoteUrls`, exported from
`tauriRepoClient.ts` — import it from there rather than duplicating it):

```typescript
  listRemotes: (repoPath: string) => call<RemoteInfo[]>("list_remotes", { repoPath }),
  listRemoteBranches: (repoPath: string, remoteName: string) =>
    call<string[]>("list_remote_branches", { repoPath, remoteName }),
  getCurrentUpstream: (repoPath: string) =>
    call<UpstreamInfo | null>("get_current_upstream", { repoPath }),
  getRemoteUpstreams: (repoPath: string, name: string) =>
    call<UpstreamInfo[]>("get_remote_upstreams", { repoPath, name }),
  addRemote: (repoPath: string, name: string, fetchUrl: string, pushUrl: string | null) => {
    validateRemoteUrls(fetchUrl, pushUrl);
    return call<void>("add_remote", { repoPath, name, fetchUrl, pushUrl });
  },
  renameRemote: (repoPath: string, oldName: string, newName: string) =>
    call<void>("rename_remote", { repoPath, oldName, newName }),
  updateRemoteUrls: (repoPath: string, name: string, fetchUrl: string, pushUrl: string | null) => {
    validateRemoteUrls(fetchUrl, pushUrl);
    return call<void>("update_remote_urls", { repoPath, name, fetchUrl, pushUrl });
  },
  removeRemote: (repoPath: string, name: string, clearUpstreams: boolean) =>
    call<void>("remove_remote", { repoPath, name, clearUpstreams }),
  saveHttpsCredential: (repoPath: string, remoteName: string, username: string, token: string) =>
    call<void>("save_https_credential", { repoPath, remoteName, username, token }),
  forgetHttpsCredential: (repoPath: string, remoteName: string) =>
    call<void>("forget_https_credential", { repoPath, remoteName }),
  setRemoteAuthMode: (repoPath: string, remoteName: string, mode: RemoteAuthMode, username: string | null) =>
    call<void>("set_remote_auth_mode", { repoPath, remoteName, mode, username }),
  setCurrentUpstream: (repoPath: string, remoteName: string, remoteBranch: string) =>
    call<void>("set_current_upstream", { repoPath, remoteName, remoteBranch }),
  clearCurrentUpstream: (repoPath: string) => call<void>("clear_current_upstream", { repoPath }),
```

Add the import at the top of the file:

```typescript
import { validateRemoteUrls } from "./tauriRepoClient";
```

- [ ] **Step 7: Write the TypeScript tests**

Append inside `describe("vscodeRepoClient", ...)`:

```typescript
  it("wires listRemotes, listRemoteBranches, and upstream methods", async () => {
    const remotesPromise = vscodeRepoClient.listRemotes("/repo");
    respond(1, [{ name: "origin", fetchUrl: "https://example.com/r.git", pushUrl: null, authMode: null, authUsername: null }]);
    await expect(remotesPromise).resolves.toHaveLength(1);

    const branchesPromise = vscodeRepoClient.listRemoteBranches("/repo", "origin");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "list_remote_branches",
      params: { repoPath: "/repo", remoteName: "origin" },
    });
    respond(2, ["main"]);
    await expect(branchesPromise).resolves.toEqual(["main"]);

    const currentPromise = vscodeRepoClient.getCurrentUpstream("/repo");
    respond(3, null);
    await expect(currentPromise).resolves.toBeNull();

    const upstreamsPromise = vscodeRepoClient.getRemoteUpstreams("/repo", "origin");
    respond(4, []);
    await expect(upstreamsPromise).resolves.toEqual([]);
  });

  it("wires addRemote and rejects embedded credentials before posting", async () => {
    const promise = vscodeRepoClient.addRemote("/repo", "origin", "https://example.com/r.git", null);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "add_remote",
      params: { repoPath: "/repo", name: "origin", fetchUrl: "https://example.com/r.git", pushUrl: null },
    });
    respond(1, null);
    await expect(promise).resolves.toBeNull();

    expect(() =>
      vscodeRepoClient.addRemote("/repo", "origin", "https://alice:secret@example.com/r.git", null),
    ).toThrow("Remote URLs must not contain embedded credentials");
  });

  it("wires renameRemote, updateRemoteUrls, and removeRemote", async () => {
    const renamePromise = vscodeRepoClient.renameRemote("/repo", "origin", "upstream");
    respond(1, null);
    await expect(renamePromise).resolves.toBeNull();

    const updatePromise = vscodeRepoClient.updateRemoteUrls("/repo", "upstream", "https://example.com/r2.git", null);
    respond(2, null);
    await expect(updatePromise).resolves.toBeNull();

    const removePromise = vscodeRepoClient.removeRemote("/repo", "upstream", true);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "remove_remote",
      params: { repoPath: "/repo", name: "upstream", clearUpstreams: true },
    });
    respond(3, null);
    await expect(removePromise).resolves.toBeNull();
  });

  it("wires https credential and auth mode methods", async () => {
    const savePromise = vscodeRepoClient.saveHttpsCredential("/repo", "origin", "alice", "secret");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "save_https_credential",
      params: { repoPath: "/repo", remoteName: "origin", username: "alice", token: "secret" },
    });
    respond(1, null);
    await expect(savePromise).resolves.toBeNull();

    const forgetPromise = vscodeRepoClient.forgetHttpsCredential("/repo", "origin");
    respond(2, null);
    await expect(forgetPromise).resolves.toBeNull();

    const authPromise = vscodeRepoClient.setRemoteAuthMode("/repo", "origin", "HttpsToken", "alice");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "set_remote_auth_mode",
      params: { repoPath: "/repo", remoteName: "origin", mode: "HttpsToken", username: "alice" },
    });
    respond(3, null);
    await expect(authPromise).resolves.toBeNull();
  });

  it("wires setCurrentUpstream and clearCurrentUpstream", async () => {
    const setPromise = vscodeRepoClient.setCurrentUpstream("/repo", "origin", "main");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "set_current_upstream",
      params: { repoPath: "/repo", remoteName: "origin", remoteBranch: "main" },
    });
    respond(1, null);
    await expect(setPromise).resolves.toBeNull();

    const clearPromise = vscodeRepoClient.clearCurrentUpstream("/repo");
    respond(2, null);
    await expect(clearPromise).resolves.toBeNull();
  });
```

- [ ] **Step 8: Run**

Run: `cd frontend && pnpm test -- --run vscodeRepoClient`
Expected: all pass.

- [ ] **Step 9: Commit the TypeScript side**

```bash
git add frontend/src/ipc/vscodeRepoClient.ts frontend/src/ipc/vscodeRepoClient.test.ts
git commit -m "feat(frontend): wire vscodeRepoClient remotes and credentials"
```

---

### Task 8: Tags

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`
- Modify: `frontend/src/ipc/vscodeRepoClient.ts`
- Modify: `frontend/src/ipc/vscodeRepoClient.test.ts`

**Interfaces:**
- Consumes: `WorkerHandle::{list_tags, create_tag, delete_tag}`
  (`crates/tauri-app/src/commands/tag.rs`).
- Produces: `TagInfoDto` — same shape as
  `crates/tauri-app/src/commands/mod.rs:282-304`'s `TagInfoDto`.

Wires `listTags`, `createTag`, `deleteTag`.

- [ ] **Step 1: Write the failing test**

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs`:

```rust
#[test]
fn tag_lifecycle_round_trips_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let created = sidecar.call(
        2,
        "create_tag",
        serde_json::json!({"repoPath": repo_path, "name": "v1.0.0", "message": "first release"}),
    );
    assert_eq!(created["result"], serde_json::Value::Null);

    let tags = sidecar.call(3, "list_tags", serde_json::json!({"repoPath": repo_path}));
    let list = tags["result"].as_array().expect("tag list");
    assert_eq!(list[0]["name"], "v1.0.0");
    assert_eq!(list[0]["annotated"], true);
    assert_eq!(list[0]["message"], "first release");

    let deleted = sidecar.call(
        4,
        "delete_tag",
        serde_json::json!({"repoPath": repo_path, "name": "v1.0.0"}),
    );
    assert_eq!(deleted["result"], serde_json::Value::Null);
    let tags_after = sidecar.call(5, "list_tags", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(tags_after["result"], serde_json::json!([]));
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p vscode-sidecar --features forge-fixture-override`
Expected: FAIL — `create_tag` reports `unknown method`.

- [ ] **Step 3: Implement the handlers**

Add to the `match` in `dispatch()`:

```rust
        "list_tags" => list_tags(params, repos),
        "create_tag" => create_tag(params, repos),
        "delete_tag" => delete_tag(params, repos),
```

Add:

```rust
use git_core::remote::TagInfo;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TagInfoDto {
    name: String,
    target_id: String,
    annotated: bool,
    message: Option<String>,
    tagger_name: Option<String>,
    timestamp: Option<i64>,
}

impl From<TagInfo> for TagInfoDto {
    fn from(tag: TagInfo) -> Self {
        Self {
            name: tag.name,
            target_id: tag.target_id,
            annotated: tag.annotated,
            message: tag.message,
            tagger_name: tag.tagger_name,
            timestamp: tag.timestamp,
        }
    }
}

fn list_tags(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let tags: Vec<TagInfoDto> = worker_handle(repos, &params.repo_path)?
        .list_tags()?
        .into_iter()
        .map(TagInfoDto::from)
        .collect();
    serde_json::to_value(tags).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateTagParams {
    repo_path: String,
    name: String,
    message: Option<String>,
}

fn create_tag(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: CreateTagParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.create_tag(params.name, params.message)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteTagParams {
    repo_path: String,
    name: String,
}

fn delete_tag(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: DeleteTagParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.delete_tag(params.name)?;
    Ok(Value::Null)
}
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p vscode-sidecar --features forge-fixture-override`
Expected: all pass (23 tests).

- [ ] **Step 5: Commit the Rust side**

```bash
git add crates/vscode-sidecar/src/dispatch.rs crates/vscode-sidecar/tests/protocol_roundtrip.rs
git commit -m "feat(vscode-sidecar): wire tags"
```

- [ ] **Step 6: Wire the TypeScript client**

Add `TagInfo` to the import list. Replace:

```typescript
  listTags: notImplemented("listTags"),
  createTag: notImplemented("createTag"),
  deleteTag: notImplemented("deleteTag"),
```

with:

```typescript
  listTags: (repoPath: string) => call<TagInfo[]>("list_tags", { repoPath }),
  createTag: (repoPath: string, name: string, message: string | null) =>
    call<void>("create_tag", { repoPath, name, message }),
  deleteTag: (repoPath: string, name: string) => call<void>("delete_tag", { repoPath, name }),
```

- [ ] **Step 7: Write the TypeScript test**

Append inside `describe("vscodeRepoClient", ...)`:

```typescript
  it("wires listTags, createTag, and deleteTag", async () => {
    const createPromise = vscodeRepoClient.createTag("/repo", "v1.0.0", "first release");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "create_tag",
      params: { repoPath: "/repo", name: "v1.0.0", message: "first release" },
    });
    respond(1, null);
    await expect(createPromise).resolves.toBeNull();

    const listPromise = vscodeRepoClient.listTags("/repo");
    respond(2, [
      { name: "v1.0.0", targetId: "abc", annotated: true, message: "first release", taggerName: "Test User", timestamp: 1_725_000_000 },
    ]);
    await expect(listPromise).resolves.toHaveLength(1);

    const deletePromise = vscodeRepoClient.deleteTag("/repo", "v1.0.0");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "delete_tag",
      params: { repoPath: "/repo", name: "v1.0.0" },
    });
    respond(3, null);
    await expect(deletePromise).resolves.toBeNull();
  });
```

- [ ] **Step 8: Run**

Run: `cd frontend && pnpm test -- --run vscodeRepoClient`
Expected: all pass.

- [ ] **Step 9: Commit the TypeScript side**

```bash
git add frontend/src/ipc/vscodeRepoClient.ts frontend/src/ipc/vscodeRepoClient.test.ts
git commit -m "feat(frontend): wire vscodeRepoClient tags"
```

---

### Task 9: Transfer (fetch/push/pull) and progress notifications

**Files:**
- Modify: `crates/vscode-sidecar/src/main.rs`
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`
- Modify: `frontend/src/ipc/vscodeRepoClient.ts`
- Modify: `frontend/src/ipc/vscodeRepoClient.test.ts`

**Interfaces:**
- Consumes: `WorkerHandle::{fetch_remote, push_current_branch, push_tags,
  pull_current_upstream}` (`crates/repo-service/src/worker/remote.rs:332-403`), each taking a
  `std::sync::mpsc::Sender<TransferEvent>` and (for fetch/push) replying with the operation id
  *before* the transfer finishes — `TransferEvent`s keep arriving on `Worker`'s own background
  thread (spawned inside `Worker::spawn`, not by the sidecar) after the JSON-RPC response for
  fetch/push has already gone out, and (for pull) arrive while the JSON-RPC response is still
  pending, since `pull_current_upstream` blocks until the whole pull finishes.
- Produces: `TransferProgressDto` (new — no direct Tauri-transport DTO precedent, since Tauri
  uses `AppHandle::emit`, not JSON-RPC notifications, to carry the equivalent
  `crates/tauri-app/src/commands/mod.rs:47-58`'s `TransferProgressDto`; this task's version
  carries the same fields, camelCase, with the same sideband-message redaction), `PullOutcomeDto`
  — same tagged shape as `crates/tauri-app/src/commands/mod.rs:345-365`'s `PullOutcomeDto`.

**Why this needs its own design, not just another mechanical transcription:** every other task
in this plan copies a synchronous request → `WorkerHandle` call → response pattern. Transfer
operations don't fit that pattern on the wire: `RepoClient.subscribeTransferProgress` is a
long-lived push subscription, not a call-and-reply. The spec's answer is JSON-RPC
*notifications* — objects with a `method` and `params` but no `id`, which
`vscodeRepoClient.ts`'s existing `isJsonRpcResponse` guard already ignores (`"id" in value` is
part of that guard) — so the wire distinction between a response and a notification is simply
"has an `id`" vs. "doesn't." The one piece of new machinery this requires: something has to
write those notification lines to the *same* stdout the main dispatch loop writes responses to,
without interleaving partial lines — hence sharing an `Arc<Mutex<io::Stdout>>` between the main
loop and a small per-transfer-operation relay thread. That relay thread is the one deliberate
exception to this plan's "no new threads" constraint (see the Global Constraints section) — it
does no git work itself, it only drains a `Receiver<TransferEvent>` that `Worker`'s own
already-existing background thread feeds, and it exits on its own once that channel's senders
are all dropped (which happens inside `repo-service` once the transfer operation completes, the
same point where `Worker`'s thread would otherwise finish emitting events to `AppHandle::emit`
in the Tauri transport).

- [ ] **Step 1: Write the failing tests**

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs`:

```rust
fn local_and_bare_remote() -> (tempfile::TempDir, tempfile::TempDir, tempfile::TempDir) {
    let (source_dir, source) = init_repo();
    write_file(source_dir.path(), "README.md", "initial commit\n");
    commit_all(&source, "initial commit");

    let remote_dir = tempfile::TempDir::new().expect("create bare remote dir");
    let remote = git2::Repository::init_bare(remote_dir.path()).expect("init bare remote");
    let branch = source.head().unwrap().shorthand().unwrap().to_string();
    let branch_ref = format!("refs/heads/{branch}");
    source
        .remote("origin", remote_dir.path().to_str().unwrap())
        .expect("add source remote");
    source
        .find_remote("origin")
        .unwrap()
        .push(&[format!("{branch_ref}:{branch_ref}")], None)
        .expect("push source commit");
    drop(remote);

    let (local_dir, local) = init_repo();
    local
        .remote("origin", remote_dir.path().to_str().unwrap())
        .expect("add local remote");

    (source_dir, remote_dir, local_dir)
}

impl Sidecar {
    fn read_line(&mut self) -> serde_json::Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).expect("read line");
        serde_json::from_str(&line).expect("parse line")
    }

    /// Like `call`, but also collects every JSON-RPC *notification* (no `id`) that arrives
    /// before the response, and — when `wait_for_terminal_notification` is set — keeps reading
    /// past the response too, until a `transferProgress` notification with a terminal phase
    /// (`Completed`/`Failed`) shows up. Real terminal-phase notifications for `fetch`/`push` can
    /// arrive strictly after the response, since those two reply with the operation id before
    /// the transfer finishes.
    fn call_and_collect_notifications(
        &mut self,
        id: u64,
        method: &str,
        params: serde_json::Value,
        wait_for_terminal_notification: bool,
    ) -> (serde_json::Value, Vec<serde_json::Value>) {
        let request =
            serde_json::json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        writeln!(self.stdin, "{request}").expect("write request");
        self.stdin.flush().expect("flush request");

        let mut notifications = Vec::new();
        let mut response = None;
        let is_terminal = |line: &serde_json::Value| {
            matches!(
                line["params"]["phase"].as_str(),
                Some("Completed") | Some("Failed")
            )
        };
        loop {
            let line = self.read_line();
            if line.get("id").is_some() {
                response = Some(line);
                if !wait_for_terminal_notification {
                    break;
                }
            } else {
                let terminal = is_terminal(&line);
                notifications.push(line);
                if terminal && response.is_some() {
                    break;
                }
            }
        }
        (response.expect("response line"), notifications)
    }
}

#[test]
fn fetch_remote_streams_transfer_progress_notifications_through_the_sidecar() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let repo_path = local_dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let (response, notifications) = sidecar.call_and_collect_notifications(
        2,
        "fetch_remote",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin"}),
        true,
    );

    let operation_id = response["result"].as_str().expect("operation id").to_string();
    assert!(!operation_id.is_empty());
    assert!(notifications
        .iter()
        .all(|n| n["method"] == "transferProgress" && n["params"]["operationId"] == operation_id));
    assert!(notifications
        .iter()
        .any(|n| n["params"]["phase"] == "Starting"));
    assert!(notifications
        .iter()
        .any(|n| n["params"]["phase"] == "Completed"));
    // Sideband/message text is never forwarded, mirroring `crates/tauri-app/src/commands/mod.rs`'s
    // own redaction — see its `transfer_event_bridge_redacts_sideband_and_failure_messages` test.
    assert!(notifications.iter().all(|n| n["params"]["message"].is_null()));
}

#[test]
fn push_current_branch_and_push_tags_stream_notifications_through_the_sidecar() {
    let (local_dir, repo) = init_repo();
    write_file(local_dir.path(), "README.md", "initial commit\n");
    commit_all(&repo, "initial commit");
    repo.create_tag_lightweight("v1.0.0", &repo.head().unwrap().peel(git2::ObjectType::Commit).unwrap(), false)
        .expect("create tag");
    let remote_dir = tempfile::TempDir::new().expect("create bare remote dir");
    git2::Repository::init_bare(remote_dir.path()).expect("init bare remote");
    repo.remote("origin", remote_dir.path().to_str().unwrap()).expect("add origin");
    drop(repo);
    let repo_path = local_dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let (push_response, push_notifications) = sidecar.call_and_collect_notifications(
        2,
        "push_current_branch",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin"}),
        true,
    );
    assert!(push_response["result"].as_str().unwrap().starts_with("push-"));
    assert!(push_notifications
        .iter()
        .any(|n| n["params"]["phase"] == "Completed" && n["params"]["operation"] == "PushBranch"));

    let (tags_response, tags_notifications) = sidecar.call_and_collect_notifications(
        3,
        "push_tags",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin", "names": ["v1.0.0"]}),
        true,
    );
    assert!(tags_response["result"].as_str().unwrap().starts_with("push-"));
    assert!(tags_notifications
        .iter()
        .any(|n| n["params"]["phase"] == "Completed" && n["params"]["operation"] == "PushTags"));
}

#[test]
fn pull_current_upstream_streams_notifications_and_returns_an_outcome() {
    let (_source_dir, _remote_dir, local_dir) = local_and_bare_remote();
    let repo_path = local_dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));
    sidecar.call(
        2,
        "set_current_upstream",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin", "remoteBranch": "main"}),
    );

    let (response, notifications) = sidecar.call_and_collect_notifications(
        3,
        "pull_current_upstream",
        serde_json::json!({"repoPath": repo_path}),
        false,
    );

    assert_eq!(response["result"]["kind"], "FastForwarded");
    assert!(notifications
        .iter()
        .any(|n| n["params"]["phase"] == "Starting" && n["params"]["operation"] == "Pull"));
}
```

Note: `set_current_upstream` in the third test assumes `origin`'s default branch is named
`main`; if the local git installation's `init.defaultBranch` differs, adjust the `remoteBranch`
argument to match — the same assumption `crates/repo-service/src/worker/mod.rs`'s own
`remote_and_current_upstream_round_trip_through_the_worker` test avoids by reading the branch
name back from `repo.head()` rather than hardcoding it; prefer that pattern if this test proves
flaky in CI.

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p vscode-sidecar --features forge-fixture-override`
Expected: FAIL — `fetch_remote` reports `unknown method`.

- [ ] **Step 3: Share stdout between the main loop and the notification relay**

Replace `crates/vscode-sidecar/src/main.rs` in full:

```rust
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};

use repo_service::worker::Worker;

mod dispatch;
mod protocol;

use protocol::{JsonRpcRequest, JsonRpcResponse};

fn main() {
    let stdin = io::stdin();
    let stdout = Arc::new(Mutex::new(io::stdout()));
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

        let response = match dispatch::dispatch(&request.method, request.params, &mut repos, &stdout) {
            Ok(result) => JsonRpcResponse::ok(request.id, result),
            Err(message) => JsonRpcResponse::err(request.id, message),
        };

        let serialized = serde_json::to_string(&response).expect("response always serializes");
        let mut out = stdout.lock().unwrap_or_else(|e| e.into_inner());
        writeln!(out, "{serialized}").expect("stdout write failed");
        out.flush().expect("stdout flush failed");
    }
}
```

- [ ] **Step 4: Thread `stdout` through `dispatch()`**

Change `dispatch.rs`'s top-level `dispatch` function signature and every match arm's call site
to pass `stdout` through — only the four transfer handlers actually use it, but the parameter
travels through the `match` uniformly:

```rust
use std::sync::{Arc, Mutex};

pub fn dispatch(
    method: &str,
    params: Value,
    repos: &mut HashMap<String, Worker>,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    match method {
        "open_repo" => open_repo(params, repos),
        "close_repo" => close_repo(params, repos),
        "get_status" => get_status(params, repos),
        "get_commit_graph" => get_commit_graph(params, repos),
        "get_working_diff" => get_working_diff(params, repos),
        "get_commit_diff" => get_commit_diff(params, repos),
        "list_recent_repos" => list_recent_repos(),
        "list_open_repos" => list_open_repos_handler(),
        "persist_open_repos" => persist_open_repos(params),
        "scan_repos_in_root" => scan_repos_in_root(params),
        "list_workspaces" => list_workspaces_handler(),
        "save_workspace" => save_workspace(params),
        "update_workspace" => update_workspace(params),
        "delete_workspace" => delete_workspace(params),
        "get_graph_branch_selection" => get_graph_branch_selection(params),
        "set_graph_branch_selection" => set_graph_branch_selection(params),
        "get_commit_files" => get_commit_files(params, repos),
        "stage_file" => stage_file(params, repos),
        "unstage_file" => unstage_file(params, repos),
        "stage_hunk" => stage_hunk(params, repos),
        "unstage_hunk" => unstage_hunk(params, repos),
        "discard_hunk" => discard_hunk(params, repos),
        "commit" => commit(params, repos),
        "list_branches" => list_branches(params, repos),
        "create_branch" => create_branch(params, repos),
        "switch_branch" => switch_branch(params, repos),
        "delete_branch" => delete_branch(params, repos),
        "rename_branch" => rename_branch(params, repos),
        "list_worktrees" => list_worktrees(params, repos),
        "create_worktree" => create_worktree(params, repos),
        "remove_worktree" => remove_worktree(params, repos),
        "prune_worktrees" => prune_worktrees(params, repos),
        "list_submodules" => list_submodules(params, repos),
        "init_submodule" => init_submodule(params, repos),
        "update_submodule" => update_submodule(params, repos),
        "list_reflog_refs" => list_reflog_refs(params, repos),
        "get_reflog" => get_reflog(params, repos),
        "restore_reflog_entry" => restore_reflog_entry(params, repos),
        "list_remotes" => list_remotes(params, repos),
        "list_remote_branches" => list_remote_branches(params, repos),
        "get_current_upstream" => get_current_upstream(params, repos),
        "get_remote_upstreams" => get_remote_upstreams(params, repos),
        "add_remote" => add_remote(params, repos),
        "rename_remote" => rename_remote(params, repos),
        "update_remote_urls" => update_remote_urls(params, repos),
        "remove_remote" => remove_remote(params, repos),
        "save_https_credential" => save_https_credential(params, repos),
        "forget_https_credential" => forget_https_credential(params, repos),
        "set_remote_auth_mode" => set_remote_auth_mode(params, repos),
        "set_current_upstream" => set_current_upstream(params, repos),
        "clear_current_upstream" => clear_current_upstream(params, repos),
        "list_tags" => list_tags(params, repos),
        "create_tag" => create_tag(params, repos),
        "delete_tag" => delete_tag(params, repos),
        "fetch_remote" => fetch_remote(params, repos, stdout),
        "push_current_branch" => push_current_branch(params, repos, stdout),
        "push_tags" => push_tags(params, repos, stdout),
        "pull_current_upstream" => pull_current_upstream(params, repos, stdout),
        other => Err(format!("unknown method: {other}")),
    }
}
```

(This replaces the existing `match` in full — list every prior task's arm exactly as already
present, since this is a full-function replacement, not a diff.)

- [ ] **Step 5: Implement the transfer handlers and the notification relay**

Add to `dispatch.rs`:

```rust
use std::io::Write as _;
use std::sync::mpsc;

use repo_service::worker::TransferEvent;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransferProgressDto {
    operation_id: String,
    operation: String,
    phase: String,
    error_kind: Option<String>,
    current: usize,
    total: usize,
    received_bytes: usize,
    message: Option<String>,
}

fn transfer_progress_dto(event: TransferEvent) -> TransferProgressDto {
    match event {
        TransferEvent::Started {
            operation_id,
            operation,
        } => TransferProgressDto {
            operation_id,
            operation: format!("{operation:?}"),
            phase: "Starting".to_string(),
            error_kind: None,
            current: 0,
            total: 0,
            received_bytes: 0,
            message: None,
        },
        TransferEvent::Progress(progress) => TransferProgressDto {
            operation_id: progress.operation_id,
            operation: format!("{:?}", progress.operation),
            phase: format!("{:?}", progress.phase),
            error_kind: None,
            current: progress.current,
            total: progress.total,
            received_bytes: progress.received_bytes,
            // Sideband and reference-update text comes from the remote — never safe to forward
            // over IPC, even when it looks like ordinary progress output. Same redaction
            // `crates/tauri-app/src/commands/mod.rs`'s `TransferProgressDto::from` applies.
            message: None,
        },
        TransferEvent::Completed {
            operation_id,
            operation,
            error,
        } => {
            let failed = error.is_some();
            TransferProgressDto {
                operation_id,
                operation: format!("{operation:?}"),
                phase: if failed { "Failed" } else { "Completed" }.to_string(),
                error_kind: error.map(|kind| format!("{kind:?}")),
                current: 0,
                total: 0,
                received_bytes: 0,
                message: None,
            }
        }
    }
}

/// Drains `event_rx` on a dedicated thread, writing each event as a `transferProgress` JSON-RPC
/// notification (no `id`) to the shared `stdout`. Exits once every `Sender<TransferEvent>` clone
/// held inside `repo-service` for this operation is dropped — see this task's own note on why a
/// per-operation thread here doesn't violate the "no new threads" constraint elsewhere in this
/// plan.
fn spawn_progress_relay(
    event_rx: mpsc::Receiver<TransferEvent>,
    stdout: Arc<Mutex<std::io::Stdout>>,
) {
    std::thread::spawn(move || {
        for event in event_rx {
            let notification = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "transferProgress",
                "params": transfer_progress_dto(event),
            });
            let Ok(line) = serde_json::to_string(&notification) else {
                continue;
            };
            let mut out = stdout.lock().unwrap_or_else(|e| e.into_inner());
            let _ = writeln!(out, "{line}");
            let _ = out.flush();
        }
    });
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchRemoteParams {
    repo_path: String,
    remote_name: String,
}

fn fetch_remote(
    params: Value,
    repos: &mut HashMap<String, Worker>,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: FetchRemoteParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id =
        worker_handle(repos, &params.repo_path)?.fetch_remote(params.remote_name, event_tx)?;
    Ok(Value::String(operation_id))
}

fn push_current_branch(
    params: Value,
    repos: &mut HashMap<String, Worker>,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: RemoteNameParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?
        .push_current_branch(params.remote_name, event_tx)?;
    Ok(Value::String(operation_id))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PushTagsParams {
    repo_path: String,
    remote_name: String,
    names: Vec<String>,
}

fn push_tags(
    params: Value,
    repos: &mut HashMap<String, Worker>,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: PushTagsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let operation_id = worker_handle(repos, &params.repo_path)?
        .push_tags(params.remote_name, params.names, event_tx)?;
    Ok(Value::String(operation_id))
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all_fields = "camelCase")]
enum PullOutcomeDto {
    UpToDate,
    FastForwarded { upstream_ref: String },
    Diverged { upstream_ref: String },
}

impl From<git_core::remote::PullOutcome> for PullOutcomeDto {
    fn from(outcome: git_core::remote::PullOutcome) -> Self {
        match outcome {
            git_core::remote::PullOutcome::UpToDate => PullOutcomeDto::UpToDate,
            git_core::remote::PullOutcome::FastForwarded { upstream_ref } => {
                PullOutcomeDto::FastForwarded { upstream_ref }
            }
            git_core::remote::PullOutcome::Diverged { upstream_ref } => {
                PullOutcomeDto::Diverged { upstream_ref }
            }
        }
    }
}

fn pull_current_upstream(
    params: Value,
    repos: &mut HashMap<String, Worker>,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let (event_tx, event_rx) = mpsc::channel();
    spawn_progress_relay(event_rx, Arc::clone(stdout));
    let outcome = worker_handle(repos, &params.repo_path)?.pull_current_upstream(event_tx)?;
    serde_json::to_value(PullOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}
```

`RemoteNameParams` (fields `repo_path`, `remote_name`) already exists from Task 7.

- [ ] **Step 6: Run to see them pass**

Run: `cargo test -p vscode-sidecar --features forge-fixture-override`
Expected: all pass (26 tests). These three tests are the first in this plan to exercise real
network-free git transport (a bare local remote) — if any hangs, check first that
`spawn_progress_relay`'s thread is actually being joined implicitly by process exit (it is not
explicitly joined anywhere, by design: `Sidecar::drop` kills the child process, which tears down
every thread in it, so no explicit join is needed in either the sidecar itself or its tests).

- [ ] **Step 7: Commit the Rust side**

```bash
git add crates/vscode-sidecar/src/main.rs crates/vscode-sidecar/src/dispatch.rs crates/vscode-sidecar/tests/protocol_roundtrip.rs
git commit -m "feat(vscode-sidecar): wire fetch/push/pull with transfer-progress notifications"
```

- [ ] **Step 8: Wire the TypeScript client**

`vscodeRepoClient.ts` needs to distinguish a JSON-RPC *notification* (`{jsonrpc, method,
params}`, no `id`) from the existing response shapes, and dispatch `transferProgress`
notifications to whichever listener(s) `subscribeTransferProgress` registered. Replace the
whole file's content with:

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
import { validateRemoteUrls } from "./tauriRepoClient";

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

interface JsonRpcNotification {
  jsonrpc: "2.0";
  method: string;
  params: unknown;
}

function isJsonRpcNotification(value: unknown): value is JsonRpcNotification {
  return (
    typeof value === "object" &&
    value !== null &&
    "jsonrpc" in value &&
    "method" in value &&
    !("id" in value)
  );
}

let vscode: VsCodeWebviewApi | undefined;
let listenerRegistered = false;
let nextRequestId = 1;
const pending = new Map<
  number,
  { resolve: (value: unknown) => void; reject: (error: Error) => void }
>();
const transferProgressListeners = new Set<(progress: TransferProgress) => void>();

function ensureInitialized(): VsCodeWebviewApi {
  if (!vscode) {
    vscode = acquireVsCodeApi();
  }
  if (!listenerRegistered) {
    listenerRegistered = true;
    window.addEventListener("message", (event: MessageEvent) => {
      const message = event.data;
      if (isJsonRpcResponse(message)) {
        const waiting = pending.get(message.id);
        if (!waiting) return;
        pending.delete(message.id);
        if ("error" in message) {
          waiting.reject(new Error(message.error.message));
        } else {
          waiting.resolve(message.result);
        }
        return;
      }
      if (isJsonRpcNotification(message) && message.method === "transferProgress") {
        const progress = message.params as TransferProgress;
        for (const listener of transferProgressListeners) listener(progress);
      }
    });
  }
  return vscode;
}

function call<T>(method: string, params: Record<string, unknown>): Promise<T> {
  const api = ensureInitialized();
  const id = nextRequestId++;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (value: unknown) => void, reject });
    api.postMessage({ jsonrpc: "2.0", id, method, params });
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
  listRecentRepos: () => call<string[]>("list_recent_repos", {}),
  getAppVersion: notImplemented("getAppVersion"),
  getLastSeenVersion: notImplemented("getLastSeenVersion"),
  setLastSeenVersion: notImplemented("setLastSeenVersion"),
  listOpenRepos: () =>
    call<{ entries: OpenRepoEntry[]; activePath: string | null }>("list_open_repos", {}),
  persistOpenRepos: (entries: OpenRepoEntry[], activePath: string | null) =>
    call<void>("persist_open_repos", { entries, activePath }),
  scanReposInRoot: (root: string) => call<string[]>("scan_repos_in_root", { root }),
  listWorkspaces: () => call<Workspace[]>("list_workspaces", {}),
  saveWorkspace: (name: string, root: string, members: string[]) =>
    call<string>("save_workspace", { name, root, members }),
  updateWorkspace: (id: string, name: string, members: string[]) =>
    call<void>("update_workspace", { id, name, members }),
  deleteWorkspace: (id: string) => call<void>("delete_workspace", { id }),
  getGraphBranchSelection: (repoPath: string) =>
    call<string[] | null>("get_graph_branch_selection", { repoPath }),
  setGraphBranchSelection: (repoPath: string, selectedBranches: string[]) =>
    call<void>("set_graph_branch_selection", { repoPath, selectedBranches }),
  getCommitFiles: (repoPath: string, commitId: string) =>
    call<string[]>("get_commit_files", { repoPath, commitId }),
  stageFile: (repoPath: string, path: string) => call<void>("stage_file", { repoPath, path }),
  unstageFile: (repoPath: string, path: string) => call<void>("unstage_file", { repoPath, path }),
  stageHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    call<void>("stage_hunk", { repoPath, path, oldStart, newStart }),
  unstageHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    call<void>("unstage_hunk", { repoPath, path, oldStart, newStart }),
  discardHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    call<void>("discard_hunk", { repoPath, path, oldStart, newStart }),
  commit: (repoPath: string, message: string) => call<string>("commit", { repoPath, message }),
  listBranches: (repoPath: string) => call<BranchInfo[]>("list_branches", { repoPath }),
  createBranch: (repoPath: string, name: string, startPoint: string) =>
    call<void>("create_branch", { repoPath, name, startPoint }),
  switchBranch: (repoPath: string, name: string) => call<void>("switch_branch", { repoPath, name }),
  deleteBranch: (repoPath: string, name: string, force: boolean) =>
    call<void>("delete_branch", { repoPath, name, force }),
  renameBranch: (repoPath: string, oldName: string, newName: string) =>
    call<void>("rename_branch", { repoPath, oldName, newName }),
  listWorktrees: (repoPath: string) => call<WorktreeInfo[]>("list_worktrees", { repoPath }),
  createWorktree: (
    repoPath: string,
    name: string,
    path: string,
    branch: string,
    startPoint: string | null,
  ) => call<void>("create_worktree", { repoPath, name, path, branch, startPoint }),
  removeWorktree: (repoPath: string, name: string) => call<void>("remove_worktree", { repoPath, name }),
  pruneWorktrees: (repoPath: string) => call<void>("prune_worktrees", { repoPath }),
  listSubmodules: (repoPath: string) => call<SubmoduleInfo[]>("list_submodules", { repoPath }),
  initSubmodule: (repoPath: string, path: string) => call<void>("init_submodule", { repoPath, path }),
  updateSubmodule: (repoPath: string, path: string, recursive: boolean) =>
    call<void>("update_submodule", { repoPath, path, recursive }),
  listReflogRefs: (repoPath: string) => call<string[]>("list_reflog_refs", { repoPath }),
  getReflog: (repoPath: string, reference: string) =>
    call<ReflogEntry[]>("get_reflog", { repoPath, reference }),
  restoreReflogEntry: (repoPath: string, reference: string, newId: string) =>
    call<void>("restore_reflog_entry", { repoPath, reference, newId }),
  listRemotes: (repoPath: string) => call<RemoteInfo[]>("list_remotes", { repoPath }),
  listRemoteBranches: (repoPath: string, remoteName: string) =>
    call<string[]>("list_remote_branches", { repoPath, remoteName }),
  getCurrentUpstream: (repoPath: string) =>
    call<UpstreamInfo | null>("get_current_upstream", { repoPath }),
  getRemoteUpstreams: (repoPath: string, name: string) =>
    call<UpstreamInfo[]>("get_remote_upstreams", { repoPath, name }),
  addRemote: (repoPath: string, name: string, fetchUrl: string, pushUrl: string | null) => {
    validateRemoteUrls(fetchUrl, pushUrl);
    return call<void>("add_remote", { repoPath, name, fetchUrl, pushUrl });
  },
  renameRemote: (repoPath: string, oldName: string, newName: string) =>
    call<void>("rename_remote", { repoPath, oldName, newName }),
  updateRemoteUrls: (repoPath: string, name: string, fetchUrl: string, pushUrl: string | null) => {
    validateRemoteUrls(fetchUrl, pushUrl);
    return call<void>("update_remote_urls", { repoPath, name, fetchUrl, pushUrl });
  },
  removeRemote: (repoPath: string, name: string, clearUpstreams: boolean) =>
    call<void>("remove_remote", { repoPath, name, clearUpstreams }),
  saveHttpsCredential: (repoPath: string, remoteName: string, username: string, token: string) =>
    call<void>("save_https_credential", { repoPath, remoteName, username, token }),
  forgetHttpsCredential: (repoPath: string, remoteName: string) =>
    call<void>("forget_https_credential", { repoPath, remoteName }),
  setRemoteAuthMode: (repoPath: string, remoteName: string, mode: RemoteAuthMode, username: string | null) =>
    call<void>("set_remote_auth_mode", { repoPath, remoteName, mode, username }),
  setCurrentUpstream: (repoPath: string, remoteName: string, remoteBranch: string) =>
    call<void>("set_current_upstream", { repoPath, remoteName, remoteBranch }),
  clearCurrentUpstream: (repoPath: string) => call<void>("clear_current_upstream", { repoPath }),
  listTags: (repoPath: string) => call<TagInfo[]>("list_tags", { repoPath }),
  createTag: (repoPath: string, name: string, message: string | null) =>
    call<void>("create_tag", { repoPath, name, message }),
  deleteTag: (repoPath: string, name: string) => call<void>("delete_tag", { repoPath, name }),
  fetchRemote: (repoPath: string, remoteName: string) =>
    call<string>("fetch_remote", { repoPath, remoteName }),
  pushCurrentBranch: (repoPath: string, remoteName: string) =>
    call<string>("push_current_branch", { repoPath, remoteName }),
  pushTags: (repoPath: string, remoteName: string, names: string[]) =>
    call<string>("push_tags", { repoPath, remoteName, names }),
  pullCurrentUpstream: (repoPath: string) =>
    call<PullOutcome>("pull_current_upstream", { repoPath }),
  subscribeTransferProgress: (listener: (progress: TransferProgress) => void) => {
    ensureInitialized();
    transferProgressListeners.add(listener);
    return () => {
      transferProgressListeners.delete(listener);
    };
  },
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

This is a full-file replacement reflecting every prior task's wiring (Tasks 1-8) plus this
task's five transfer methods — from here on, later tasks in this plan go back to small
stub-replacement diffs against this new baseline.

- [ ] **Step 9: Write the TypeScript tests**

Append inside `describe("vscodeRepoClient", ...)`:

```typescript
  function notify(method: string, params: unknown) {
    window.dispatchEvent(new MessageEvent("message", { data: { jsonrpc: "2.0", method, params } }));
  }

  it("wires fetchRemote, pushCurrentBranch, and pushTags", async () => {
    const fetchPromise = vscodeRepoClient.fetchRemote("/repo", "origin");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "fetch_remote",
      params: { repoPath: "/repo", remoteName: "origin" },
    });
    respond(1, "fetch-1");
    await expect(fetchPromise).resolves.toBe("fetch-1");

    const pushPromise = vscodeRepoClient.pushCurrentBranch("/repo", "origin");
    respond(2, "push-1");
    await expect(pushPromise).resolves.toBe("push-1");

    const tagsPromise = vscodeRepoClient.pushTags("/repo", "origin", ["v1.0.0"]);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "push_tags",
      params: { repoPath: "/repo", remoteName: "origin", names: ["v1.0.0"] },
    });
    respond(3, "push-2");
    await expect(tagsPromise).resolves.toBe("push-2");
  });

  it("wires pullCurrentUpstream", async () => {
    const promise = vscodeRepoClient.pullCurrentUpstream("/repo");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "pull_current_upstream",
      params: { repoPath: "/repo" },
    });
    respond(1, { kind: "FastForwarded", upstreamRef: "refs/remotes/origin/main" });
    await expect(promise).resolves.toEqual({ kind: "FastForwarded", upstreamRef: "refs/remotes/origin/main" });
  });

  it("delivers transferProgress notifications to subscribed listeners and stops after unsubscribe", () => {
    const received: unknown[] = [];
    const unsubscribe = vscodeRepoClient.subscribeTransferProgress((progress) => received.push(progress));

    notify("transferProgress", { operationId: "fetch-1", operation: "Fetch", phase: "Starting", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null });
    expect(received).toHaveLength(1);

    unsubscribe();
    notify("transferProgress", { operationId: "fetch-1", operation: "Fetch", phase: "Completed", errorKind: null, current: 0, total: 0, receivedBytes: 0, message: null });
    expect(received).toHaveLength(1);
  });

  it("ignores notifications for methods it doesn't recognize", () => {
    const received: unknown[] = [];
    vscodeRepoClient.subscribeTransferProgress((progress) => received.push(progress));

    notify("somethingElse", { anything: true });

    expect(received).toHaveLength(0);
  });
```

- [ ] **Step 10: Run**

Run: `cd frontend && pnpm test -- --run vscodeRepoClient`
Expected: all pass.

- [ ] **Step 11: Commit the TypeScript side**

```bash
git add frontend/src/ipc/vscodeRepoClient.ts frontend/src/ipc/vscodeRepoClient.test.ts
git commit -m "feat(frontend): wire vscodeRepoClient transfer progress notifications"
```

---

### Task 10: Stash

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`
- Modify: `frontend/src/ipc/vscodeRepoClient.ts`
- Modify: `frontend/src/ipc/vscodeRepoClient.test.ts`

**Interfaces:**
- Consumes: `WorkerHandle::{list_stashes, save_stash, apply_stash, drop_stash}`
  (`crates/tauri-app/src/commands/stash.rs`).
- Produces: `StashEntryDto` — same shape as
  `crates/tauri-app/src/commands/mod.rs:386-402`'s `StashEntryDto`.

Wires `listStashes`, `saveStash`, `applyStash`, `dropStash`.

- [ ] **Step 1: Write the failing test**

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs`:

```rust
#[test]
fn stash_lifecycle_round_trips_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "file.txt", "v2");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let saved = sidecar.call(2, "save_stash", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(saved["result"], serde_json::Value::Null);

    let stashes = sidecar.call(3, "list_stashes", serde_json::json!({"repoPath": repo_path}));
    let list = stashes["result"].as_array().expect("stash list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["index"], 0);

    let applied = sidecar.call(4, "apply_stash", serde_json::json!({"repoPath": repo_path, "index": 0}));
    assert_eq!(applied["result"], serde_json::Value::Null);
    let on_disk = std::fs::read_to_string(dir.path().join("file.txt")).unwrap();
    assert_eq!(on_disk, "v2");

    let dropped = sidecar.call(5, "drop_stash", serde_json::json!({"repoPath": repo_path, "index": 0}));
    assert_eq!(dropped["result"], serde_json::Value::Null);
    let stashes_after = sidecar.call(6, "list_stashes", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(stashes_after["result"], serde_json::json!([]));
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p vscode-sidecar --features forge-fixture-override`
Expected: FAIL — `save_stash` reports `unknown method`.

- [ ] **Step 3: Implement the handlers**

Add to the `match` in `dispatch()`:

```rust
        "list_stashes" => list_stashes(params, repos),
        "save_stash" => save_stash(params, repos),
        "apply_stash" => apply_stash(params, repos),
        "drop_stash" => drop_stash(params, repos),
```

Add:

```rust
use git_core::stash::StashEntry;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StashEntryDto {
    index: usize,
    message: String,
    commit_id: String,
}

impl From<StashEntry> for StashEntryDto {
    fn from(entry: StashEntry) -> Self {
        Self {
            index: entry.index,
            message: entry.message,
            commit_id: entry.commit_id,
        }
    }
}

fn list_stashes(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let stashes: Vec<StashEntryDto> = worker_handle(repos, &params.repo_path)?
        .list_stashes()?
        .into_iter()
        .map(StashEntryDto::from)
        .collect();
    serde_json::to_value(stashes).map_err(|error| error.to_string())
}

fn save_stash(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.save_stash()?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StashIndexParams {
    repo_path: String,
    index: usize,
}

fn apply_stash(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: StashIndexParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.apply_stash(params.index)?;
    Ok(Value::Null)
}

fn drop_stash(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: StashIndexParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.drop_stash(params.index)?;
    Ok(Value::Null)
}
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p vscode-sidecar --features forge-fixture-override`
Expected: all pass (27 tests).

- [ ] **Step 5: Commit the Rust side**

```bash
git add crates/vscode-sidecar/src/dispatch.rs crates/vscode-sidecar/tests/protocol_roundtrip.rs
git commit -m "feat(vscode-sidecar): wire stash"
```

- [ ] **Step 6: Wire the TypeScript client**

Add `StashEntry` to the import list. Replace:

```typescript
  listStashes: notImplemented("listStashes"),
  saveStash: notImplemented("saveStash"),
  applyStash: notImplemented("applyStash"),
  dropStash: notImplemented("dropStash"),
```

with:

```typescript
  listStashes: (repoPath: string) => call<StashEntry[]>("list_stashes", { repoPath }),
  saveStash: (repoPath: string) => call<void>("save_stash", { repoPath }),
  applyStash: (repoPath: string, index: number) => call<void>("apply_stash", { repoPath, index }),
  dropStash: (repoPath: string, index: number) => call<void>("drop_stash", { repoPath, index }),
```

- [ ] **Step 7: Write the TypeScript test**

Append inside `describe("vscodeRepoClient", ...)`:

```typescript
  it("wires listStashes, saveStash, applyStash, and dropStash", async () => {
    const savePromise = vscodeRepoClient.saveStash("/repo");
    respond(1, null);
    await expect(savePromise).resolves.toBeNull();

    const listPromise = vscodeRepoClient.listStashes("/repo");
    respond(2, [{ index: 0, message: "WIP", commitId: "abc123" }]);
    await expect(listPromise).resolves.toHaveLength(1);

    const applyPromise = vscodeRepoClient.applyStash("/repo", 0);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "apply_stash",
      params: { repoPath: "/repo", index: 0 },
    });
    respond(3, null);
    await expect(applyPromise).resolves.toBeNull();

    const dropPromise = vscodeRepoClient.dropStash("/repo", 0);
    respond(4, null);
    await expect(dropPromise).resolves.toBeNull();
  });
```

- [ ] **Step 8: Run**

Run: `cd frontend && pnpm test -- --run vscodeRepoClient`
Expected: all pass.

- [ ] **Step 9: Commit the TypeScript side**

```bash
git add frontend/src/ipc/vscodeRepoClient.ts frontend/src/ipc/vscodeRepoClient.test.ts
git commit -m "feat(frontend): wire vscodeRepoClient stash"
```

---

### Task 11: Blame, merge, and conflict resolution

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`
- Modify: `frontend/src/ipc/vscodeRepoClient.ts`
- Modify: `frontend/src/ipc/vscodeRepoClient.test.ts`

**Interfaces:**
- Consumes: `WorkerHandle::{get_blame, start_merge, get_conflict_hunks, resolve_conflict,
  abort_merge, get_merge_message, resolve_add_delete_conflict}`
  (`crates/tauri-app/src/commands/status.rs:86-98`, `crates/tauri-app/src/commands/merge.rs`).
- Produces: `BlameLineDto`, `MergeOutcomeDto`, `ConflictSegmentDto`, `FileConflictChoiceDto` —
  same shapes as `crates/tauri-app/src/commands/mod.rs:404-513`.

Wires `getBlame`, `mergeBranch` (JSON-RPC method `start_merge`, matching
`tauriRepoClient.ts:186-187`'s own naming), `getConflictHunks`, `resolveConflict`, `abortMerge`,
`getMergeMessage`, `resolveAddDeleteConflict`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs`:

```rust
#[test]
fn get_blame_round_trips_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "hello\n");
    commit_all(&repo, "initial commit");
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let blame = sidecar.call(
        2,
        "get_blame",
        serde_json::json!({"repoPath": repo_path, "commitId": "HEAD", "path": "file.txt"}),
    );
    let lines = blame["result"].as_array().expect("blame lines");
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0]["content"], "hello");
}

#[test]
fn merge_conflict_lifecycle_round_trips_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "base\n");
    commit_all(&repo, "base commit");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &base, false).unwrap();

    write_file(dir.path(), "file.txt", "main change\n");
    commit_all(&repo, "main change");

    let feature_ref = repo.find_branch("feature", git2::BranchType::Local).unwrap();
    repo.set_head(feature_ref.get().name().unwrap()).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
    write_file(dir.path(), "file.txt", "feature change\n");
    commit_all(&repo, "feature change");

    let main_branch = repo
        .branches(Some(git2::BranchType::Local))
        .unwrap()
        .find_map(|b| {
            let (branch, _) = b.unwrap();
            let name = branch.name().unwrap().unwrap().to_string();
            (name != "feature").then_some(name)
        })
        .unwrap();
    repo.set_head(&format!("refs/heads/{main_branch}")).unwrap();
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
    drop(repo);

    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let merged = sidecar.call(
        2,
        "start_merge",
        serde_json::json!({"repoPath": repo_path, "branchName": "feature"}),
    );
    assert_eq!(merged["result"]["kind"], "Conflicted");
    let conflicted_files = merged["result"]["files"].as_array().unwrap();
    assert!(conflicted_files.iter().any(|f| f == "file.txt"));

    let hunks = sidecar.call(
        3,
        "get_conflict_hunks",
        serde_json::json!({"repoPath": repo_path, "path": "file.txt"}),
    );
    assert!(hunks["result"].as_array().unwrap().iter().any(|h| h["kind"] == "Conflict"));

    let message = sidecar.call(4, "get_merge_message", serde_json::json!({"repoPath": repo_path}));
    assert!(message["result"].as_str().unwrap().contains("feature"));

    let resolved = sidecar.call(
        5,
        "resolve_conflict",
        serde_json::json!({"repoPath": repo_path, "path": "file.txt", "resolvedContent": "resolved\n"}),
    );
    assert_eq!(resolved["result"], serde_json::Value::Null);

    let aborted = sidecar.call(6, "abort_merge", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(aborted["result"], serde_json::Value::Null);
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p vscode-sidecar --features forge-fixture-override`
Expected: FAIL — `get_blame` reports `unknown method`.

- [ ] **Step 3: Implement the handlers**

Add to the `match` in `dispatch()`:

```rust
        "get_blame" => get_blame(params, repos),
        "start_merge" => start_merge(params, repos),
        "get_conflict_hunks" => get_conflict_hunks(params, repos),
        "resolve_conflict" => resolve_conflict(params, repos),
        "abort_merge" => abort_merge(params, repos),
        "get_merge_message" => get_merge_message(params, repos),
        "resolve_add_delete_conflict" => resolve_add_delete_conflict(params, repos),
```

Add:

```rust
use git_core::blame::BlameLine;
use git_core::merge::{ConflictSegment, FileConflictChoice, MergeOutcome};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BlameLineDto {
    line_number: usize,
    content: String,
    commit_id: String,
    short_id: String,
    author_name: String,
    timestamp: i64,
}

impl From<BlameLine> for BlameLineDto {
    fn from(line: BlameLine) -> Self {
        Self {
            line_number: line.line_number,
            content: line.content,
            commit_id: line.commit_id,
            short_id: line.short_id,
            author_name: line.author_name,
            timestamp: line.timestamp,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetBlameParams {
    repo_path: String,
    commit_id: String,
    path: String,
}

fn get_blame(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: GetBlameParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let lines: Vec<BlameLineDto> = worker_handle(repos, &params.repo_path)?
        .get_blame(params.commit_id, params.path)?
        .into_iter()
        .map(BlameLineDto::from)
        .collect();
    serde_json::to_value(lines).map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum MergeOutcomeDto {
    UpToDate,
    FastForwarded,
    Merged,
    Conflicted { files: Vec<String> },
}

impl From<MergeOutcome> for MergeOutcomeDto {
    fn from(outcome: MergeOutcome) -> Self {
        match outcome {
            MergeOutcome::UpToDate => MergeOutcomeDto::UpToDate,
            MergeOutcome::FastForwarded => MergeOutcomeDto::FastForwarded,
            MergeOutcome::Merged => MergeOutcomeDto::Merged,
            MergeOutcome::Conflicted { files } => MergeOutcomeDto::Conflicted { files },
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartMergeParams {
    repo_path: String,
    branch_name: String,
}

fn start_merge(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: StartMergeParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let outcome = worker_handle(repos, &params.repo_path)?.start_merge(params.branch_name)?;
    serde_json::to_value(MergeOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum ConflictSegmentDto {
    Clean { content: String },
    Conflict { ours: String, theirs: String },
}

impl From<ConflictSegment> for ConflictSegmentDto {
    fn from(segment: ConflictSegment) -> Self {
        match segment {
            ConflictSegment::Clean { content } => ConflictSegmentDto::Clean { content },
            ConflictSegment::Conflict { ours, theirs } => ConflictSegmentDto::Conflict { ours, theirs },
        }
    }
}

fn get_conflict_hunks(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoFilePathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let segments: Vec<ConflictSegmentDto> = worker_handle(repos, &params.repo_path)?
        .get_conflict_hunks(params.path)?
        .into_iter()
        .map(ConflictSegmentDto::from)
        .collect();
    serde_json::to_value(segments).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveConflictParams {
    repo_path: String,
    path: String,
    resolved_content: String,
}

fn resolve_conflict(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: ResolveConflictParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.resolve_conflict(params.path, params.resolved_content)?;
    Ok(Value::Null)
}

fn abort_merge(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.abort_merge()?;
    Ok(Value::Null)
}

fn get_merge_message(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let message = worker_handle(repos, &params.repo_path)?.get_merge_message()?;
    serde_json::to_value(message).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
enum FileConflictChoiceDto {
    Ours,
    Theirs,
    Delete,
}

impl From<FileConflictChoiceDto> for FileConflictChoice {
    fn from(dto: FileConflictChoiceDto) -> Self {
        match dto {
            FileConflictChoiceDto::Ours => FileConflictChoice::Ours,
            FileConflictChoiceDto::Theirs => FileConflictChoice::Theirs,
            FileConflictChoiceDto::Delete => FileConflictChoice::Delete,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResolveAddDeleteConflictParams {
    repo_path: String,
    path: String,
    choice: FileConflictChoiceDto,
}

fn resolve_add_delete_conflict(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: ResolveAddDeleteConflictParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .resolve_add_delete_conflict(params.path, params.choice.into())?;
    Ok(Value::Null)
}
```

- [ ] **Step 4: Run to see them pass**

Run: `cargo test -p vscode-sidecar --features forge-fixture-override`
Expected: all pass (29 tests).

- [ ] **Step 5: Commit the Rust side**

```bash
git add crates/vscode-sidecar/src/dispatch.rs crates/vscode-sidecar/tests/protocol_roundtrip.rs
git commit -m "feat(vscode-sidecar): wire blame, merge, and conflict resolution"
```

- [ ] **Step 6: Wire the TypeScript client**

Add `MergeOutcome`, `ConflictSegment`, `FileConflictChoice` to the import list. Replace:

```typescript
  getBlame: notImplemented("getBlame"),
  mergeBranch: notImplemented("mergeBranch"),
  getConflictHunks: notImplemented("getConflictHunks"),
  resolveConflict: notImplemented("resolveConflict"),
  abortMerge: notImplemented("abortMerge"),
  getMergeMessage: notImplemented("getMergeMessage"),
  resolveAddDeleteConflict: notImplemented("resolveAddDeleteConflict"),
```

with:

```typescript
  getBlame: (repoPath: string, commitId: string, path: string) =>
    call<BlameLine[]>("get_blame", { repoPath, commitId, path }),
  mergeBranch: (repoPath: string, branchName: string) =>
    call<MergeOutcome>("start_merge", { repoPath, branchName }),
  getConflictHunks: (repoPath: string, path: string) =>
    call<ConflictSegment[]>("get_conflict_hunks", { repoPath, path }),
  resolveConflict: (repoPath: string, path: string, resolvedContent: string) =>
    call<void>("resolve_conflict", { repoPath, path, resolvedContent }),
  abortMerge: (repoPath: string) => call<void>("abort_merge", { repoPath }),
  getMergeMessage: (repoPath: string) => call<string | null>("get_merge_message", { repoPath }),
  resolveAddDeleteConflict: (repoPath: string, path: string, choice: FileConflictChoice) =>
    call<void>("resolve_add_delete_conflict", { repoPath, path, choice }),
```

Also add `BlameLine` to the import list (already listed in `RepoClient.ts`'s exports but not
yet imported into this file).

- [ ] **Step 7: Write the TypeScript tests**

Append inside `describe("vscodeRepoClient", ...)`:

```typescript
  it("wires getBlame", async () => {
    const promise = vscodeRepoClient.getBlame("/repo", "HEAD", "file.txt");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_blame",
      params: { repoPath: "/repo", commitId: "HEAD", path: "file.txt" },
    });
    respond(1, [{ lineNumber: 1, content: "hello", commitId: "abc", shortId: "a", authorName: "Test", timestamp: 0 }]);
    await expect(promise).resolves.toHaveLength(1);
  });

  it("wires mergeBranch onto start_merge", async () => {
    const promise = vscodeRepoClient.mergeBranch("/repo", "feature");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "start_merge",
      params: { repoPath: "/repo", branchName: "feature" },
    });
    respond(1, { kind: "Conflicted", files: ["file.txt"] });
    await expect(promise).resolves.toEqual({ kind: "Conflicted", files: ["file.txt"] });
  });

  it("wires getConflictHunks, resolveConflict, abortMerge, getMergeMessage, and resolveAddDeleteConflict", async () => {
    const hunksPromise = vscodeRepoClient.getConflictHunks("/repo", "file.txt");
    respond(1, [{ kind: "Conflict", ours: "a", theirs: "b" }]);
    await expect(hunksPromise).resolves.toEqual([{ kind: "Conflict", ours: "a", theirs: "b" }]);

    const resolvePromise = vscodeRepoClient.resolveConflict("/repo", "file.txt", "resolved");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "resolve_conflict",
      params: { repoPath: "/repo", path: "file.txt", resolvedContent: "resolved" },
    });
    respond(2, null);
    await expect(resolvePromise).resolves.toBeNull();

    const abortPromise = vscodeRepoClient.abortMerge("/repo");
    respond(3, null);
    await expect(abortPromise).resolves.toBeNull();

    const messagePromise = vscodeRepoClient.getMergeMessage("/repo");
    respond(4, "Merge branch 'feature'");
    await expect(messagePromise).resolves.toBe("Merge branch 'feature'");

    const resolveAddDeletePromise = vscodeRepoClient.resolveAddDeleteConflict("/repo", "file.txt", "Ours");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 5,
      method: "resolve_add_delete_conflict",
      params: { repoPath: "/repo", path: "file.txt", choice: "Ours" },
    });
    respond(5, null);
    await expect(resolveAddDeletePromise).resolves.toBeNull();
  });
```

- [ ] **Step 8: Run**

Run: `cd frontend && pnpm test -- --run vscodeRepoClient`
Expected: all pass.

- [ ] **Step 9: Commit the TypeScript side**

```bash
git add frontend/src/ipc/vscodeRepoClient.ts frontend/src/ipc/vscodeRepoClient.test.ts
git commit -m "feat(frontend): wire vscodeRepoClient blame, merge, and conflict resolution"
```

---

### Task 12: Rebase

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`
- Modify: `frontend/src/ipc/vscodeRepoClient.ts`
- Modify: `frontend/src/ipc/vscodeRepoClient.test.ts`

**Interfaces:**
- Consumes: `WorkerHandle::{commits_since, start_rebase, rebase_continue, abort_rebase,
  get_rebase_progress}` (`crates/tauri-app/src/commands/rebase.rs`).
- Produces: `RebasePlanCommitDto`, `RebaseActionDto`, `RebasePlanEntryDto`,
  `RebaseStepResultDto`, `RebaseProgressDto` — same shapes as
  `crates/tauri-app/src/commands/mod.rs:515-608`.

Wires `commitsSince`, `startRebase`, `rebaseContinue`, `abortRebase`, `getRebaseProgress`.

- [ ] **Step 1: Write the failing test**

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs`:

```rust
#[test]
fn rebase_lifecycle_round_trips_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "first commit");
    let base = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &base, false).unwrap();
    let feature_ref = repo.find_branch("feature", git2::BranchType::Local).unwrap();
    repo.set_head(feature_ref.get().name().unwrap()).unwrap();
    write_file(dir.path(), "other.txt", "v1");
    commit_all(&repo, "second commit");
    let second = repo.head().unwrap().peel_to_commit().unwrap().id().to_string();
    drop(repo);

    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let since = sidecar.call(
        2,
        "commits_since",
        serde_json::json!({"repoPath": repo_path, "onto": "HEAD~1"}),
    );
    let commits = since["result"].as_array().expect("commits since");
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0]["id"], second);

    let started = sidecar.call(
        3,
        "start_rebase",
        serde_json::json!({
            "repoPath": repo_path,
            "onto": "HEAD~1",
            "plan": [{"commitId": second, "action": {"kind": "Pick"}, "combinedMessage": null}],
        }),
    );
    assert_eq!(started["result"]["kind"], "Done");

    let progress = sidecar.call(4, "get_rebase_progress", serde_json::json!({"repoPath": repo_path}));
    assert_eq!(progress["result"], serde_json::Value::Null);
}
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p vscode-sidecar --features forge-fixture-override`
Expected: FAIL — `commits_since` reports `unknown method`.

- [ ] **Step 3: Implement the handlers**

Add to the `match` in `dispatch()`:

```rust
        "commits_since" => commits_since(params, repos),
        "start_rebase" => start_rebase(params, repos),
        "rebase_continue" => rebase_continue(params, repos),
        "abort_rebase" => abort_rebase(params, repos),
        "get_rebase_progress" => get_rebase_progress(params, repos),
```

Add:

```rust
use git_core::rebase::{RebaseAction, RebasePlanCommit, RebasePlanEntry, RebaseStepResult};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RebasePlanCommitDto {
    id: String,
    short_id: String,
    summary: String,
    author_name: String,
    timestamp: i64,
}

impl From<RebasePlanCommit> for RebasePlanCommitDto {
    fn from(c: RebasePlanCommit) -> Self {
        Self {
            id: c.id,
            short_id: c.short_id,
            summary: c.summary,
            author_name: c.author_name,
            timestamp: c.timestamp,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitsSinceParams {
    repo_path: String,
    onto: String,
}

fn commits_since(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: CommitsSinceParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let commits: Vec<RebasePlanCommitDto> = worker_handle(repos, &params.repo_path)?
        .commits_since(params.onto)?
        .into_iter()
        .map(RebasePlanCommitDto::from)
        .collect();
    serde_json::to_value(commits).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(tag = "kind")]
enum RebaseActionDto {
    Pick,
    Reword { message: String },
    Edit,
    Squash,
    Fixup,
    Drop,
}

impl From<RebaseActionDto> for RebaseAction {
    fn from(dto: RebaseActionDto) -> Self {
        match dto {
            RebaseActionDto::Pick => RebaseAction::Pick,
            RebaseActionDto::Reword { message } => RebaseAction::Reword { message },
            RebaseActionDto::Edit => RebaseAction::Edit,
            RebaseActionDto::Squash => RebaseAction::Squash,
            RebaseActionDto::Fixup => RebaseAction::Fixup,
            RebaseActionDto::Drop => RebaseAction::Drop,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RebasePlanEntryDto {
    commit_id: String,
    action: RebaseActionDto,
    combined_message: Option<String>,
}

impl From<RebasePlanEntryDto> for RebasePlanEntry {
    fn from(dto: RebasePlanEntryDto) -> Self {
        RebasePlanEntry {
            commit_id: dto.commit_id,
            action: dto.action.into(),
            combined_message: dto.combined_message,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum RebaseStepResultDto {
    Conflicted { files: Vec<String> },
    PausedForEdit,
    Advanced,
    Done,
}

impl From<RebaseStepResult> for RebaseStepResultDto {
    fn from(result: RebaseStepResult) -> Self {
        match result {
            RebaseStepResult::Conflicted { files } => RebaseStepResultDto::Conflicted { files },
            RebaseStepResult::PausedForEdit => RebaseStepResultDto::PausedForEdit,
            RebaseStepResult::Advanced => RebaseStepResultDto::Advanced,
            RebaseStepResult::Done => RebaseStepResultDto::Done,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartRebaseParams {
    repo_path: String,
    onto: String,
    plan: Vec<RebasePlanEntryDto>,
}

fn start_rebase(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: StartRebaseParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let result = worker_handle(repos, &params.repo_path)?
        .start_rebase(params.onto, params.plan.into_iter().map(Into::into).collect())?;
    serde_json::to_value(RebaseStepResultDto::from(result)).map_err(|error| error.to_string())
}

fn rebase_continue(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let result = worker_handle(repos, &params.repo_path)?.rebase_continue()?;
    serde_json::to_value(RebaseStepResultDto::from(result)).map_err(|error| error.to_string())
}

fn abort_rebase(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.abort_rebase()?;
    Ok(Value::Null)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RebaseProgressDto {
    current_step: usize,
    total_steps: usize,
}

fn get_rebase_progress(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let progress = worker_handle(repos, &params.repo_path)?
        .get_rebase_progress()?
        .map(|(current_step, total_steps)| RebaseProgressDto {
            current_step,
            total_steps,
        });
    serde_json::to_value(progress).map_err(|error| error.to_string())
}
```

- [ ] **Step 4: Run to see it pass**

Run: `cargo test -p vscode-sidecar --features forge-fixture-override`
Expected: all pass (30 tests).

- [ ] **Step 5: Commit the Rust side**

```bash
git add crates/vscode-sidecar/src/dispatch.rs crates/vscode-sidecar/tests/protocol_roundtrip.rs
git commit -m "feat(vscode-sidecar): wire rebase"
```

- [ ] **Step 6: Wire the TypeScript client**

Add `RebasePlanCommit`, `RebasePlanEntry`, `RebaseStepResult` to the import list. Replace:

```typescript
  commitsSince: notImplemented("commitsSince"),
  startRebase: notImplemented("startRebase"),
  rebaseContinue: notImplemented("rebaseContinue"),
  abortRebase: notImplemented("abortRebase"),
  getRebaseProgress: notImplemented("getRebaseProgress"),
```

with:

```typescript
  commitsSince: (repoPath: string, onto: string) =>
    call<RebasePlanCommit[]>("commits_since", { repoPath, onto }),
  startRebase: (repoPath: string, onto: string, plan: RebasePlanEntry[]) =>
    call<RebaseStepResult>("start_rebase", { repoPath, onto, plan }),
  rebaseContinue: (repoPath: string) => call<RebaseStepResult>("rebase_continue", { repoPath }),
  abortRebase: (repoPath: string) => call<void>("abort_rebase", { repoPath }),
  getRebaseProgress: (repoPath: string) =>
    call<{ currentStep: number; totalSteps: number } | null>("get_rebase_progress", { repoPath }),
```

- [ ] **Step 7: Write the TypeScript test**

Append inside `describe("vscodeRepoClient", ...)`:

```typescript
  it("wires commitsSince, startRebase, rebaseContinue, abortRebase, and getRebaseProgress", async () => {
    const sincePromise = vscodeRepoClient.commitsSince("/repo", "main");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "commits_since",
      params: { repoPath: "/repo", onto: "main" },
    });
    respond(1, [{ id: "abc", shortId: "a", summary: "s", authorName: "Test", timestamp: 0 }]);
    await expect(sincePromise).resolves.toHaveLength(1);

    const plan = [{ commitId: "abc", action: { kind: "Pick" as const }, combinedMessage: null }];
    const startPromise = vscodeRepoClient.startRebase("/repo", "main", plan);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "start_rebase",
      params: { repoPath: "/repo", onto: "main", plan },
    });
    respond(2, { kind: "Done" });
    await expect(startPromise).resolves.toEqual({ kind: "Done" });

    const continuePromise = vscodeRepoClient.rebaseContinue("/repo");
    respond(3, { kind: "Advanced" });
    await expect(continuePromise).resolves.toEqual({ kind: "Advanced" });

    const abortPromise = vscodeRepoClient.abortRebase("/repo");
    respond(4, null);
    await expect(abortPromise).resolves.toBeNull();

    const progressPromise = vscodeRepoClient.getRebaseProgress("/repo");
    respond(5, { currentStep: 1, totalSteps: 3 });
    await expect(progressPromise).resolves.toEqual({ currentStep: 1, totalSteps: 3 });
  });
```

- [ ] **Step 8: Run**

Run: `cd frontend && pnpm test -- --run vscodeRepoClient`
Expected: all pass.

- [ ] **Step 9: Commit the TypeScript side**

```bash
git add frontend/src/ipc/vscodeRepoClient.ts frontend/src/ipc/vscodeRepoClient.test.ts
git commit -m "feat(frontend): wire vscodeRepoClient rebase"
```

---

### Task 13: Forge repository detection and pull requests

**Files:**
- Modify: `crates/vscode-sidecar/src/dispatch.rs`
- Modify: `crates/vscode-sidecar/tests/protocol_roundtrip.rs`
- Modify: `frontend/src/ipc/vscodeRepoClient.ts`
- Modify: `frontend/src/ipc/vscodeRepoClient.test.ts`

**Interfaces:**
- Consumes: `WorkerHandle::{detect_forge_repository, save_forge_token, forget_forge_token,
  list_pull_requests, create_pull_request}` (`crates/tauri-app/src/commands/forge.rs`).
- Produces: `ForgeProviderDto`, `ForgeRepositoryDto`, `PullRequestDto`, `PullRequestListDto`,
  `CreatePullRequestDto` — same shapes as `crates/tauri-app/src/commands/mod.rs:645-763`.

Wires `detectForgeRepository`, `saveForgeToken`, `forgetForgeToken`, `listPullRequests`,
`createPullRequest`. **Testing note:** `detect_forge_repository` is local remote-URL parsing
(no network) and `save_forge_token`/`forget_forge_token` only touch the credential store (via
the same `forge-fixture-override` in-memory store Task 7 already builds with) — all three get a
real black-box subprocess test. `list_pull_requests`/`create_pull_request` call a real
GitHub/Bitbucket HTTP API through `ReqwestForgeApi`, which — unlike the credential store — has
no swap-in-memory-at-runtime seam reachable from outside the sidecar's own compiled binary (only
`crates/repo-service/src/worker/mod.rs`'s own in-process unit tests inject a `FakeForgeApi`, via
`Worker::spawn_with`, which nothing outside that crate can call). This plan wires both methods
fully and gives `list_pull_requests` an error-path test (a remote that isn't a recognized forge
repository, which fails before any HTTP call), but leaves success-path coverage for both to a
later `extension/e2e/` flow (per the spec's own "Testing" section) — the same gap
`crates/tauri-app` already has today, since it has no HTTP-hitting test for these two commands
either.

- [ ] **Step 1: Write the failing tests**

Append to `crates/vscode-sidecar/tests/protocol_roundtrip.rs`:

```rust
#[test]
fn detect_forge_repository_and_forge_token_lifecycle_round_trip_through_the_sidecar() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    repo.remote("origin", "https://github.com/acme/widget.git").unwrap();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let detected = sidecar.call(2, "detect_forge_repository", serde_json::json!({"repoPath": repo_path}));
    let repos = detected["result"].as_array().expect("forge repositories");
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0]["provider"], "GitHub");
    assert_eq!(repos[0]["owner"], "acme");
    assert_eq!(repos[0]["name"], "widget");

    let saved = sidecar.call(
        3,
        "save_forge_token",
        serde_json::json!({"repoPath": repo_path, "provider": "GitHub", "account": "alice", "token": "secret"}),
    );
    assert_eq!(saved["result"], serde_json::Value::Null);

    let forgotten = sidecar.call(
        4,
        "forget_forge_token",
        serde_json::json!({"repoPath": repo_path, "provider": "GitHub", "account": "alice"}),
    );
    assert_eq!(forgotten["result"], serde_json::Value::Null);
}

#[test]
fn list_pull_requests_on_a_non_forge_remote_fails_before_any_http_call() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "file.txt", "v1");
    commit_all(&repo, "initial commit");
    repo.remote("origin", "https://example.com/not-a-forge/repo.git").unwrap();
    let repo_path = dir.path().to_str().unwrap().to_string();
    let mut sidecar = Sidecar::spawn();
    sidecar.call(1, "open_repo", serde_json::json!({"path": repo_path}));

    let listed = sidecar.call(
        2,
        "list_pull_requests",
        serde_json::json!({"repoPath": repo_path, "remoteName": "origin", "account": "alice"}),
    );

    assert!(listed.get("result").is_none());
    assert!(listed["error"]["message"].as_str().is_some());
}
```

- [ ] **Step 2: Run to see them fail**

Run: `cargo test -p vscode-sidecar --features forge-fixture-override`
Expected: FAIL — `detect_forge_repository` reports `unknown method`.

- [ ] **Step 3: Implement the handlers**

Add to the `match` in `dispatch()`:

```rust
        "detect_forge_repository" => detect_forge_repository(params, repos),
        "save_forge_token" => save_forge_token(params, repos),
        "forget_forge_token" => forget_forge_token(params, repos),
        "list_pull_requests" => list_pull_requests(params, repos),
        "create_pull_request" => create_pull_request(params, repos),
```

Add:

```rust
use git_core::forge::{ForgeProvider, ForgeRepository};

#[derive(Clone, Copy, Serialize, Deserialize)]
enum ForgeProviderDto {
    GitHub,
    Bitbucket,
}

impl From<ForgeProvider> for ForgeProviderDto {
    fn from(provider: ForgeProvider) -> Self {
        match provider {
            ForgeProvider::GitHub => ForgeProviderDto::GitHub,
            ForgeProvider::Bitbucket => ForgeProviderDto::Bitbucket,
        }
    }
}

impl From<ForgeProviderDto> for ForgeProvider {
    fn from(dto: ForgeProviderDto) -> Self {
        match dto {
            ForgeProviderDto::GitHub => ForgeProvider::GitHub,
            ForgeProviderDto::Bitbucket => ForgeProvider::Bitbucket,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ForgeRepositoryDto {
    provider: ForgeProviderDto,
    host: String,
    owner: String,
    name: String,
    remote_name: String,
}

impl From<ForgeRepository> for ForgeRepositoryDto {
    fn from(repository: ForgeRepository) -> Self {
        Self {
            provider: repository.provider.into(),
            host: repository.host,
            owner: repository.owner,
            name: repository.name,
            remote_name: repository.remote_name,
        }
    }
}

fn detect_forge_repository(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let repositories: Vec<ForgeRepositoryDto> = worker_handle(repos, &params.repo_path)?
        .detect_forge_repository()?
        .into_iter()
        .map(ForgeRepositoryDto::from)
        .collect();
    serde_json::to_value(repositories).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ForgeTokenParams {
    repo_path: String,
    provider: ForgeProviderDto,
    account: String,
    token: String,
}

fn save_forge_token(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: ForgeTokenParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?
        .save_forge_token(params.provider.into(), params.account, params.token)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ForgetForgeTokenParams {
    repo_path: String,
    provider: ForgeProviderDto,
    account: String,
}

fn forget_forge_token(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: ForgetForgeTokenParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    worker_handle(repos, &params.repo_path)?.forget_forge_token(params.provider.into(), params.account)?;
    Ok(Value::Null)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PullRequestDto {
    id: String,
    number: u64,
    title: String,
    url: String,
    author: String,
    source_branch: String,
    target_branch: String,
    state: String,
}

impl From<repo_service::pull_requests::PullRequest> for PullRequestDto {
    fn from(pull_request: repo_service::pull_requests::PullRequest) -> Self {
        Self {
            id: pull_request.id,
            number: pull_request.number,
            title: pull_request.title,
            url: pull_request.url,
            author: pull_request.author,
            source_branch: pull_request.source_branch,
            target_branch: pull_request.target_branch,
            state: pull_request.state,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PullRequestListDto {
    pull_requests: Vec<PullRequestDto>,
    truncated: bool,
}

impl From<repo_service::pull_requests::PullRequestList> for PullRequestListDto {
    fn from(list: repo_service::pull_requests::PullRequestList) -> Self {
        Self {
            pull_requests: list.pull_requests.into_iter().map(PullRequestDto::from).collect(),
            truncated: list.truncated,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListPullRequestsParams {
    repo_path: String,
    remote_name: String,
    account: String,
}

fn list_pull_requests(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: ListPullRequestsParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let list = worker_handle(repos, &params.repo_path)?
        .list_pull_requests(params.remote_name, params.account)?;
    serde_json::to_value(PullRequestListDto::from(list)).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatePullRequestInputDto {
    title: String,
    description: Option<String>,
    source_branch: String,
    target_branch: String,
}

impl From<CreatePullRequestInputDto> for repo_service::pull_requests::CreatePullRequest {
    fn from(dto: CreatePullRequestInputDto) -> Self {
        Self {
            title: dto.title,
            description: dto.description,
            source_branch: dto.source_branch,
            target_branch: dto.target_branch,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatePullRequestParams {
    repo_path: String,
    remote_name: String,
    account: String,
    pull_request: CreatePullRequestInputDto,
}

fn create_pull_request(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: CreatePullRequestParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let created = worker_handle(repos, &params.repo_path)?.create_pull_request(
        params.remote_name,
        params.account,
        params.pull_request.into(),
    )?;
    serde_json::to_value(PullRequestDto::from(created)).map_err(|error| error.to_string())
}
```

- [ ] **Step 4: Run to see them pass**

Run: `cargo test -p vscode-sidecar --features forge-fixture-override`
Expected: all pass (32 tests).

- [ ] **Step 5: Commit the Rust side**

```bash
git add crates/vscode-sidecar/src/dispatch.rs crates/vscode-sidecar/tests/protocol_roundtrip.rs
git commit -m "feat(vscode-sidecar): wire forge repository detection and pull requests"
```

- [ ] **Step 6: Wire the TypeScript client**

Add `ForgeProvider`, `ForgeRepository`, `PullRequest`, `PullRequestList`, `CreatePullRequest` to
the import list. Replace:

```typescript
  detectForgeRepository: notImplemented("detectForgeRepository"),
  saveForgeToken: notImplemented("saveForgeToken"),
  forgetForgeToken: notImplemented("forgetForgeToken"),
  listPullRequests: notImplemented("listPullRequests"),
  createPullRequest: notImplemented("createPullRequest"),
```

with:

```typescript
  detectForgeRepository: (repoPath: string) =>
    call<ForgeRepository[]>("detect_forge_repository", { repoPath }),
  saveForgeToken: (repoPath: string, provider: ForgeProvider, account: string, token: string) =>
    call<void>("save_forge_token", { repoPath, provider, account, token }),
  forgetForgeToken: (repoPath: string, provider: ForgeProvider, account: string) =>
    call<void>("forget_forge_token", { repoPath, provider, account }),
  listPullRequests: (repoPath: string, remoteName: string, account: string) =>
    call<PullRequestList>("list_pull_requests", { repoPath, remoteName, account }),
  createPullRequest: (repoPath: string, remoteName: string, account: string, pullRequest: CreatePullRequest) =>
    call<PullRequest>("create_pull_request", { repoPath, remoteName, account, pullRequest }),
```

- [ ] **Step 7: Write the TypeScript test**

Append inside `describe("vscodeRepoClient", ...)`:

```typescript
  it("wires detectForgeRepository, saveForgeToken, and forgetForgeToken", async () => {
    const detectPromise = vscodeRepoClient.detectForgeRepository("/repo");
    respond(1, [{ provider: "GitHub", host: "github.com", owner: "acme", name: "widget", remoteName: "origin" }]);
    await expect(detectPromise).resolves.toHaveLength(1);

    const savePromise = vscodeRepoClient.saveForgeToken("/repo", "GitHub", "alice", "secret");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "save_forge_token",
      params: { repoPath: "/repo", provider: "GitHub", account: "alice", token: "secret" },
    });
    respond(2, null);
    await expect(savePromise).resolves.toBeNull();

    const forgetPromise = vscodeRepoClient.forgetForgeToken("/repo", "GitHub", "alice");
    respond(3, null);
    await expect(forgetPromise).resolves.toBeNull();
  });

  it("wires listPullRequests and createPullRequest", async () => {
    const listPromise = vscodeRepoClient.listPullRequests("/repo", "origin", "alice");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "list_pull_requests",
      params: { repoPath: "/repo", remoteName: "origin", account: "alice" },
    });
    respond(1, { pullRequests: [], truncated: false });
    await expect(listPromise).resolves.toEqual({ pullRequests: [], truncated: false });

    const createPromise = vscodeRepoClient.createPullRequest("/repo", "origin", "alice", {
      title: "Add feature",
      description: null,
      sourceBranch: "feature",
      targetBranch: "main",
    });
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "create_pull_request",
      params: {
        repoPath: "/repo",
        remoteName: "origin",
        account: "alice",
        pullRequest: { title: "Add feature", description: null, sourceBranch: "feature", targetBranch: "main" },
      },
    });
    respond(2, {
      id: "1",
      number: 7,
      title: "Add feature",
      url: "https://github.com/acme/widget/pull/7",
      author: "alice",
      sourceBranch: "feature",
      targetBranch: "main",
      state: "open",
    });
    await expect(createPromise).resolves.toMatchObject({ number: 7 });
  });
```

- [ ] **Step 8: Run**

Run: `cd frontend && pnpm test -- --run vscodeRepoClient`
Expected: all pass.

- [ ] **Step 9: Commit the TypeScript side**

```bash
git add frontend/src/ipc/vscodeRepoClient.ts frontend/src/ipc/vscodeRepoClient.test.ts
git commit -m "feat(frontend): wire vscodeRepoClient forge repository detection and pull requests"
```

---

### Task 14: Final verification pass

**Files:** `docs/ARCHITECTURE.md` (modified); everything else verification-only.

**Interfaces:** none — confirms Tasks 1-13's combined result reaches full `RepoClient` parity
on the sidecar transport and meets every check this repo's CI and `CLAUDE.md` require.

- [ ] **Step 1: Full workspace build and test**

Run: `cargo build --workspace --features vscode-sidecar/forge-fixture-override`
Expected: succeeds.

Run: `cargo test --workspace --features vscode-sidecar/forge-fixture-override`
Expected: all green, including `vscode-sidecar`'s full test suite (32 tests across this plan's
13 Rust tasks plus the POC plan's 9); every previously-passing test elsewhere unchanged.

- [ ] **Step 2: Clippy and format**

Run: `cargo clippy --workspace --all-targets --features vscode-sidecar/forge-fixture-override -- -D warnings`
Expected: clean.

Run: `cargo fmt --all -- --check`
Expected: clean. If not, run `cargo fmt --all` and fold the diff into this task's commit.

- [ ] **Step 3: Frontend build, lint, test**

Run: `cd frontend && pnpm build`
Expected: succeeds (`tsc -b` catches any leftover type error against `vscodeRepoClient.ts`'s
now-fully-wired `RepoClient` object).

Run: `cd frontend && pnpm lint`
Expected: clean.

Run: `cd frontend && pnpm test -- --run`
Expected: all green.

- [ ] **Step 4: Confirm only the five VSCode-native methods remain stubbed**

Run: `grep -c 'notImplemented("' frontend/src/ipc/vscodeRepoClient.ts`
Expected: `5` — `pickRepoFolder`, `getAppVersion`, `getLastSeenVersion`, `setLastSeenVersion`,
`openExternalUrl` (the quote-anchored pattern excludes the `notImplemented` helper's own
definition line, which would otherwise inflate the count by one). Any other count means a
method from this plan's scope was missed; cross-check against `frontend/src/ipc/RepoClient.ts`'s
`RepoClient` interface method-by-method.

- [ ] **Step 5: Update `docs/ARCHITECTURE.md`'s `vscode-sidecar` paragraph**

The paragraph currently reads (added by the POC plan's Task 6):

```markdown
`vscode-sidecar` is `tauri-app`'s sibling for the VSCode extension target (Phase 6, spec
`docs/superpowers/specs/2026-08-30-vscode-extension-design.md`): a standalone binary speaking
line-delimited JSON-RPC 2.0 over stdio instead of Tauri's IPC. As of this writing it wires
`open_repo`/`close_repo` plus the status/log/diff method family
(`crates/vscode-sidecar/src/dispatch.rs`); the remaining ~79 `RepoClient` methods, and the
`extension/` host that will actually spawn this process from VSCode, are follow-up work.
```

Replace it with:

```markdown
`vscode-sidecar` is `tauri-app`'s sibling for the VSCode extension target (Phase 6, spec
`docs/superpowers/specs/2026-08-30-vscode-extension-design.md`): a standalone binary speaking
line-delimited JSON-RPC 2.0 over stdio instead of Tauri's IPC. It now wires every `RepoClient`
method except the five VSCode-native ones (`pickRepoFolder`, `getAppVersion`,
`getLastSeenVersion`, `setLastSeenVersion`, `openExternalUrl`), which sub-phase (c)'s
`extension/` host implements directly against VSCode APIs instead of round-tripping through the
sidecar — see the spec's "VSCode-native integrations" section. Transfer progress
(`subscribeTransferProgress`) rides the same JSON-RPC connection as everything else, as
server-initiated notifications (`crates/vscode-sidecar/src/dispatch.rs`'s
`spawn_progress_relay`) rather than request/response calls. Phase 6 sub-phase (b) is complete;
the `extension/` host that will actually spawn this process from VSCode, package it per
platform, and add the `@vscode/test-electron` E2E layer are sub-phases (c)-(e), still follow-up
work.
```

- [ ] **Step 6: Commit**

```bash
git add docs/ARCHITECTURE.md
git commit -m "docs(architecture): document full RepoClient parity on the vscode-sidecar transport"
```
