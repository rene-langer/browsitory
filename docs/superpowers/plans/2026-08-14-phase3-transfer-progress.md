# Transfer Progress and Fetch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** Fetch a selected remote while streaming authenticated transfer progress to a transport-neutral frontend panel.

**Architecture:** Extend git_core::remote with callback-injected fetch and owned progress records. The Worker publishes records as Tauri events; RepoClient normalizes event subscription; useAppState owns one active operation.

**Tech Stack:** Rust 2021, git2 0.21, Tauri 2 event API, React 18, TypeScript, Vitest, WebdriverIO.

**Spec:** docs/superpowers/specs/2026-08-14-phase3-remote-workflows-design.md

## Global Constraints

- Preserve one repository-owning Worker thread; libgit2 callbacks run synchronously there.
- Progress and errors must contain no token, password, private key, or URL userinfo.
- Use owned Send transfer records; never send git2::Progress or git2::Cred across threads.
- RepoClient remains the only frontend transport seam.
- Fetch works with a dirty worktree; no live network accounts are used in CI.

---

### Task 1: git-core transfer contract and fetch

**Files:**
- Modify: crates/git-core/src/remote.rs
- Modify: crates/git-core/tests/remote.rs

**Interfaces:**
- Produces TransferOperation, TransferPhase, TransferProgress, TransferReporter, CredentialProvider, and fetch_remote.

- [ ] **Step 1: Write a failing local-bare-remote fetch test**

~~~rust
#[test]
fn fetch_updates_tracking_ref_and_reports_owned_progress() {
    let (local, remote) = common::local_and_bare_remote();
    let mut events = VecReporter::default();
    fetch_remote(&local, "origin", &mut NoCredentials, &mut events).unwrap();

    assert!(local.find_reference("refs/remotes/origin/main").is_ok());
    assert!(events.events.iter().any(|event| event.phase == TransferPhase::Receiving));
}
~~~

- [ ] **Step 2: Run the test**

Run: cargo test -p git-core --test remote fetch_updates
Expected: FAIL because fetch_remote and transfer types do not exist.

- [ ] **Step 3: Implement callback-safe fetch**

~~~rust
pub struct TransferProgress {
    pub operation_id: String,
    pub operation: TransferOperation,
    pub phase: TransferPhase,
    pub current: usize,
    pub total: usize,
    pub received_bytes: usize,
    pub message: Option<String>,
}

pub trait TransferReporter { fn report(&mut self, event: TransferProgress); }
pub trait CredentialProvider {
    fn credential(&mut self, url: &str, username: Option<&str>, allowed: CredentialType) -> Result<Cred, git2::Error>;
}
pub fn fetch_remote(
    repo: &Repository, remote_name: &str, operation_id: String,
    credentials: &mut dyn CredentialProvider, reporter: &mut dyn TransferReporter,
) -> Result<(), RemoteError>;
~~~

Configure RemoteCallbacks::credentials, transfer_progress, sideband_progress, and update_tips; copy every callback value into TransferProgress before reporting. Keep certificate validation at libgit2 defaults.

- [ ] **Step 4: Verify and commit**

Run: cargo test -p git-core --test remote && cargo fmt --all -- --check
Expected: PASS.

~~~bash
git add crates/git-core/src/remote.rs crates/git-core/tests/remote.rs
git commit -m "feat(git-core): fetch remotes with progress"
~~~

### Task 2: Worker event bridge and client subscription

**Files:**
- Modify: crates/tauri-app/src/worker.rs
- Modify: crates/tauri-app/src/commands.rs
- Modify: crates/tauri-app/src/main.rs
- Modify: frontend/src/ipc/RepoClient.ts
- Modify: frontend/src/ipc/tauriRepoClient.ts

**Interfaces:**
- Consumes fetch_remote and TransferProgress from Task 1.
- Produces fetchRemote(remoteName: string): Promise<string> and subscribeTransferProgress(listener): () => void.

- [ ] **Step 1: Add failing transport tests**

Assert a WorkerHandle fetch emits ordered start/progress/completed records through an injected Sender. Add a tauriRepoClient test that converts a transfer-progress event into the TypeScript TransferProgress shape and unregisters its listener.

- [ ] **Step 2: Run the tests**

Run: cargo test -p tauri-app transfer && cd frontend && pnpm test -- --run tauriRepoClient
Expected: FAIL until event DTOs and subscription exist.

- [ ] **Step 3: Implement the event bridge**

~~~rust
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgressDto { pub operation_id: String, pub phase: String, pub current: usize, pub total: usize, pub received_bytes: usize, pub message: Option<String> }

#[tauri::command]
pub async fn fetch_remote(remote_name: String, app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    worker_handle(&state)?.fetch_remote(remote_name, app)
}
~~~

Emit transfer-progress and transfer-complete events with the opaque operation ID. The command reply returns that ID immediately after the worker begins; completion remains event-driven. Implement subscribeTransferProgress with Tauri listen and return its unlisten function.

- [ ] **Step 4: Verify and commit**

Run: cargo test -p tauri-app && cd frontend && pnpm test -- --run tauriRepoClient && pnpm lint
Expected: PASS.

~~~bash
git add crates/tauri-app/src frontend/src/ipc
git commit -m "feat(ipc): stream remote transfer progress"
~~~

### Task 3: TransferPanel, state, and Fetch control

**Files:**
- Create: frontend/src/components/TransferPanel.tsx
- Create: frontend/src/components/TransferPanel.test.tsx
- Modify: frontend/src/components/RemotePanel.tsx
- Modify: frontend/src/state/useAppState.ts
- Modify: frontend/src/state/useAppState.test.ts
- Modify: frontend/src/App.tsx
- Modify: e2e/specs/remote-transfer.spec.ts

**Interfaces:**
- Consumes Task 2 subscription and fetchRemote.
- Produces state.transfer: TransferProgress | null and an accessible Fetch button.

- [ ] **Step 1: Write failing UI tests**

~~~tsx
it("disables Fetch during an active operation and renders byte progress", () => {
  render(<TransferPanel progress={{ operationId: "op-1", phase: "Receiving", current: 2, total: 4, receivedBytes: 1024, message: null }} />);
  expect(screen.getByText("2 / 4 objects")).toBeInTheDocument();
  expect(screen.getByText("1.0 KB received")).toBeInTheDocument();
});
~~~

- [ ] **Step 2: Implement subscription lifecycle**

Subscribe after a repository opens; unsubscribe on repository replacement and component unmount. Store the matching active operation only, call refresh on a matching completed event, and clear pending only then. RemotePanel's Fetch uses the selected remote and is disabled by pending, mergeMessage, rebaseProgress, or an active transfer.

- [ ] **Step 3: Verify and commit**

Run: cd frontend && pnpm test -- --run TransferPanel useAppState && pnpm lint && pnpm build
Expected: PASS.

~~~bash
git add frontend/src/components frontend/src/state frontend/src/App.tsx e2e/specs/remote-transfer.spec.ts
git commit -m "feat(frontend): show fetch transfer progress"
~~~
