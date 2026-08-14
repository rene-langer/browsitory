# Remote and Upstream Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** Let users manage named Git remotes and the current local branch's upstream through the existing IPC boundary.

**Architecture:** Add a UI-agnostic git_core::remote module for remote configuration and upstream discovery. Thread it through Worker, Tauri DTO commands, RepoClient, useAppState, and a focused RemotePanel component.

**Tech Stack:** Rust 2021, git2 0.21, thiserror, Tauri 2, React 18, TypeScript, Vitest, Testing Library, WebdriverIO.

**Spec:** docs/superpowers/specs/2026-08-14-phase3-remote-workflows-design.md

## Global Constraints

- Tests in git-core and tauri-app use real temporary Git repositories; never mock git2::Repository.
- Components and state use only RepoClient; only frontend/src/ipc/tauriRepoClient.ts may import Tauri APIs.
- Remotes have a fetch URL and optional distinct push URL.
- Removing a remote must require explicit clearing of every affected local upstream.
- No credential secret is stored, returned, or rendered in this plan.
- Run cargo fmt, the focused tests, pnpm lint, and relevant E2E before each task commit.

---

### Task 1: git-core remote and upstream service

**Files:**
- Create: crates/git-core/src/remote.rs
- Modify: crates/git-core/src/lib.rs
- Create: crates/git-core/tests/remote.rs

**Interfaces:**
- Produces RemoteInfo, UpstreamInfo, RemoteError, and all functions consumed by the Worker task.

- [ ] **Step 1: Write the failing integration tests**

~~~rust
#[test]
fn remote_crud_and_upstream_round_trip() {
    let (_dir, repo) = common::repo_with_initial_commit();
    add_remote(&repo, "origin", "file:///tmp/origin.git", None).unwrap();
    set_current_upstream(&repo, "origin", "main").unwrap();

    assert_eq!(list_remotes(&repo).unwrap()[0].name, "origin");
    assert_eq!(current_upstream(&repo).unwrap().unwrap().remote_name, "origin");
    assert!(matches!(remove_remote(&repo, "origin"), Err(RemoteError::RemoteInUse { .. })));

    clear_current_upstream(&repo).unwrap();
    remove_remote(&repo, "origin").unwrap();
}
~~~

- [ ] **Step 2: Run the test to verify it fails**

Run: cargo test -p git-core --test remote
Expected: FAIL because git_core::remote does not exist.

- [ ] **Step 3: Implement the smallest public service**

~~~rust
pub struct RemoteInfo {
    pub name: String,
    pub fetch_url: String,
    pub push_url: Option<String>,
}

pub struct UpstreamInfo {
    pub local_branch: String,
    pub remote_name: String,
    pub remote_branch: String,
}

pub fn list_remotes(repo: &Repository) -> Result<Vec<RemoteInfo>, RemoteError>;
pub fn add_remote(repo: &Repository, name: &str, fetch_url: &str, push_url: Option<&str>) -> Result<(), RemoteError>;
pub fn rename_remote(repo: &Repository, old_name: &str, new_name: &str) -> Result<(), RemoteError>;
pub fn update_remote_urls(repo: &Repository, name: &str, fetch_url: &str, push_url: Option<&str>) -> Result<(), RemoteError>;
pub fn current_upstream(repo: &Repository) -> Result<Option<UpstreamInfo>, RemoteError>;
pub fn set_current_upstream(repo: &Repository, remote_name: &str, remote_branch: &str) -> Result<(), RemoteError>;
pub fn clear_current_upstream(repo: &Repository) -> Result<(), RemoteError>;
pub fn remove_remote(repo: &Repository, name: &str) -> Result<(), RemoteError>;
~~~

Use Repository::remotes, find_remote, remote_rename, remote_set_url, remote_set_pushurl, and Branch::set_upstream. Before removal, enumerate local branches whose upstream remote matches name and return RemoteInUse with their names.

- [ ] **Step 4: Run focused verification**

Run: cargo test -p git-core --test remote && cargo fmt --all -- --check
Expected: PASS.

- [ ] **Step 5: Commit**

~~~bash
git add crates/git-core/src/lib.rs crates/git-core/src/remote.rs crates/git-core/tests/remote.rs
git commit -m "feat(git-core): add remote management"
~~~

### Task 2: Worker, Tauri commands, and RepoClient contract

**Files:**
- Modify: crates/tauri-app/src/worker.rs
- Modify: crates/tauri-app/src/commands.rs
- Modify: crates/tauri-app/src/main.rs
- Modify: frontend/src/ipc/RepoClient.ts
- Modify: frontend/src/ipc/tauriRepoClient.ts
- Modify: frontend/src/state/useAppState.test.ts

**Interfaces:**
- Consumes RemoteInfo and UpstreamInfo from Task 1.
- Produces listRemotes, getCurrentUpstream, addRemote, renameRemote, updateRemoteUrls, removeRemote, setCurrentUpstream, and clearCurrentUpstream on RepoClient.

- [ ] **Step 1: Add failing contract tests**

Add a Worker test that creates origin, sets main's upstream, and reads both values through WorkerHandle. Add a useAppState fixture whose RepoClient requires the eight new methods, so omitted transport methods are a TypeScript failure.

- [ ] **Step 2: Run the focused tests**

Run: cargo test -p tauri-app remote && cd frontend && pnpm test -- --run useAppState
Expected: FAIL to compile until the Worker and client methods exist.

- [ ] **Step 3: Add the one-to-one transport mapping**

~~~rust
// worker.rs
ListRemotes { reply: Sender<Result<Vec<RemoteInfo>, String>> },
SetCurrentUpstream { remote_name: String, remote_branch: String, reply: Sender<Result<(), String>> },

// commands.rs
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfoDto { pub name: String, pub fetch_url: String, pub push_url: Option<String> }
~~~

Use async Tauri commands for every worker round trip. Register every command in main.rs. In RepoClient.ts use camelCase DTO properties and Promise-returning methods; tauriRepoClient.ts invokes the snake_case command names.

- [ ] **Step 4: Run focused verification**

Run: cargo test -p tauri-app && cd frontend && pnpm test -- --run useAppState && pnpm lint
Expected: PASS.

- [ ] **Step 5: Commit**

~~~bash
git add crates/tauri-app/src frontend/src/ipc frontend/src/state/useAppState.test.ts
git commit -m "feat(ipc): expose remote management"
~~~

### Task 3: RemotePanel and application state

**Files:**
- Create: frontend/src/components/RemotePanel.tsx
- Create: frontend/src/components/RemotePanel.test.tsx
- Modify: frontend/src/state/useAppState.ts
- Modify: frontend/src/App.tsx
- Modify: frontend/src/index.css
- Modify: e2e/specs/remote-management.spec.ts

**Interfaces:**
- Consumes the Task 2 RepoClient methods.
- Produces the in-app remote CRUD and upstream-management UI.

- [ ] **Step 1: Write failing UI and E2E tests**

~~~tsx
it("requires clearing the upstream before removing its remote", async () => {
  render(<RemotePanel remotes={[origin]} upstream={upstream} onRemoveRemote={remove} onClearUpstream={clear} />);
  await user.click(screen.getByRole("button", { name: "Remove origin" }));
  expect(screen.getByText(/clear main's upstream/i)).toBeInTheDocument();
  expect(remove).not.toHaveBeenCalled();
});
~~~

The E2E flow creates a local bare remote, adds it as origin, sets main to track origin/main, confirms removal is blocked, clears upstream, then removes origin.

- [ ] **Step 2: Run the focused frontend test**

Run: cd frontend && pnpm test -- --run RemotePanel
Expected: FAIL because RemotePanel does not exist.

- [ ] **Step 3: Implement state refresh and the panel**

Extend AppState with remotes: RemoteInfo[] and upstream: UpstreamInfo | null. Fetch both in refresh's Promise.all. Add runMutation wrappers for every Task 2 mutation. Render RemotePanel below BranchSwitcher with accessible labelled forms for remote name, fetch URL, optional push URL, and upstream branch. Use a confirmation state instead of window.confirm so it is testable.

- [ ] **Step 4: Run verification**

Run: cd frontend && pnpm test -- --run RemotePanel useAppState && pnpm lint && pnpm build
Expected: PASS.

- [ ] **Step 5: Commit**

~~~bash
git add frontend/src/components frontend/src/state/useAppState.ts frontend/src/App.tsx frontend/src/index.css e2e/specs/remote-management.spec.ts
git commit -m "feat(frontend): manage remotes and upstreams"
~~~
