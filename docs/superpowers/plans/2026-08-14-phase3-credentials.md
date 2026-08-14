# Credential Handling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** Authenticate HTTPS transfers with OS-keychain tokens and SSH transfers with the local SSH agent, without persisting or exposing secrets.

**Architecture:** A tauri-app credential service implements git_core::remote's injected CredentialProvider. Non-secret per-remote auth metadata lives in local Git config; keyring stores the HTTPS token; React manages only ephemeral form input.

**Tech Stack:** Rust 2021, git2 0.21 with https and ssh features, keyring 4.1.6 with v1, Tauri 2, React 18, TypeScript, Vitest.

**Spec:** docs/superpowers/specs/2026-08-14-phase3-remote-workflows-design.md

## Global Constraints

- Add only permissively licensed dependencies and record each one in docs/LICENSE_COMPLIANCE.md.
- Never write token/password/private-key material to Git config, URLs, logs, progress events, state, or command results.
- Store only auth mode and HTTPS username per remote in local Git config; rename/remove preserves or clears that metadata.
- HTTPS uses OS keychain; SSH uses Cred::ssh_key_from_agent only.
- Keep certificate and SSH host-key validation at libgit2 defaults.
- CI uses an in-memory CredentialStore; no real keychain or live credential is required.

---

### Task 1: Dependencies, license record, and credential storage service

**Files:**
- Modify: crates/git-core/Cargo.toml
- Modify: crates/tauri-app/Cargo.toml
- Modify: Cargo.lock
- Modify: docs/LICENSE_COMPLIANCE.md
- Create: crates/tauri-app/src/credentials.rs

**Interfaces:**
- Produces CredentialStore, KeyringCredentialStore, CredentialKey, HttpsCredential, and CredentialService.

- [ ] **Step 1: Write failing service tests**

~~~rust
#[test]
fn saves_reads_and_forgets_a_token_without_exposing_it_in_metadata() {
    let store = MemoryCredentialStore::default();
    let mut service = CredentialService::new(store);
    service.save_https("https://git.example.test/org/repo.git", "rene", "token-123").unwrap();

    assert_eq!(service.username_for_remote("origin").unwrap(), Some("rene".into()));
    assert_eq!(service.lookup_https("https://git.example.test/org/repo.git", Some("rene")).unwrap().token, "token-123");
    service.forget_https("https://git.example.test/org/repo.git", "rene").unwrap();
}
~~~

- [ ] **Step 2: Add dependencies and verify licensing before code**

Set git-core's dependency to git2 = { version = "0.21", features = ["https", "ssh"] }. Add keyring = { version = "4.1.6", features = ["v1"] } to tauri-app. Update Cargo.lock with cargo check. Record keyring's Apache-2.0 license, source, version, and use in docs/LICENSE_COMPLIANCE.md.

Run: cargo check --workspace
Expected: PASS before the service test compiles.

- [ ] **Step 3: Implement an injectable store**

~~~rust
pub trait CredentialStore {
    fn get(&self, key: &CredentialKey) -> Result<Option<String>, CredentialStoreError>;
    fn set(&self, key: &CredentialKey, token: &str) -> Result<(), CredentialStoreError>;
    fn delete(&self, key: &CredentialKey) -> Result<(), CredentialStoreError>;
}

pub struct CredentialKey { pub service: String, pub account: String }
pub struct CredentialService<S: CredentialStore> { store: S }
~~~

Use service name com.browsitory.git and account https://{host}:{port}/{username}, omitting only a URL's default port. KeyringCredentialStore wraps keyring::Entry. Keep MemoryCredentialStore inside the test module. Zero temporary token strings as far as Rust ownership permits by keeping them scoped to the callback; never derive Debug for credential types.

- [ ] **Step 4: Verify and commit**

Run: cargo test -p tauri-app credentials && cargo check --workspace && cargo fmt --all -- --check
Expected: PASS.

~~~bash
git add crates/git-core/Cargo.toml crates/tauri-app/Cargo.toml Cargo.lock crates/tauri-app/src/credentials.rs docs/LICENSE_COMPLIANCE.md
git commit -m "feat(tauri-app): add keychain credential storage"
~~~

### Task 2: Per-remote metadata and libgit2 credential provider

**Files:**
- Modify: crates/git-core/src/remote.rs
- Modify: crates/git-core/tests/remote.rs
- Modify: crates/tauri-app/src/worker.rs
- Modify: crates/tauri-app/src/credentials.rs

**Interfaces:**
- Consumes CredentialProvider from the transfer plan.
- Produces set_remote_auth_profile, clear_remote_auth_profile, and a Worker credential provider.

- [ ] **Step 1: Write failing real-repo and provider tests**

Test that setting origin's HTTPS username/auth mode writes only browsitory.remote.origin.auth-mode and browsitory.remote.origin.username in local config; test rename moves both keys and remove clears them. Test the provider returns Cred::userpass_plaintext only after its store returns a token and returns a stable MissingCredential error otherwise. Test SSH mode calls Cred::ssh_key_from_agent with the URL username or git.

- [ ] **Step 2: Implement metadata and callback resolution**

~~~rust
pub enum RemoteAuthMode { HttpsToken { username: String }, SshAgent }

pub fn set_remote_auth_profile(repo: &Repository, remote: &str, profile: RemoteAuthMode) -> Result<(), RemoteError>;
pub fn clear_remote_auth_profile(repo: &Repository, remote: &str) -> Result<(), RemoteError>;
~~~

The Worker owns CredentialService and supplies a provider closure for every fetch, pull, and push. It derives a keychain key from the URL and configured username. Reject an HTTPS callback without a profile as MissingCredential; for SSH use the agent and never query keyring. Update remote rename/remove operations so metadata follows the remote's name or is cleared after upstream confirmation.

- [ ] **Step 3: Verify and commit**

Run: cargo test -p git-core --test remote && cargo test -p tauri-app && cargo fmt --all -- --check
Expected: PASS.

~~~bash
git add crates/git-core/src/remote.rs crates/git-core/tests/remote.rs crates/tauri-app/src/worker.rs crates/tauri-app/src/credentials.rs
git commit -m "feat: authenticate remote transfers securely"
~~~

### Task 3: Credential commands and RemotePanel settings

**Files:**
- Modify: crates/tauri-app/src/commands.rs
- Modify: crates/tauri-app/src/main.rs
- Modify: frontend/src/ipc/RepoClient.ts
- Modify: frontend/src/ipc/tauriRepoClient.ts
- Modify: frontend/src/components/RemotePanel.tsx
- Modify: frontend/src/components/RemotePanel.test.tsx
- Modify: frontend/src/state/useAppState.ts
- Modify: frontend/src/state/useAppState.test.ts

**Interfaces:**
- Produces saveHttpsCredential(remoteName, username, token), forgetHttpsCredential(remoteName), and setRemoteAuthMode(remoteName, mode, username).

- [ ] **Step 1: Write failing UI tests**

~~~tsx
it("submits the token only to the save callback and clears the input", async () => {
  render(<RemotePanel onSaveHttpsCredential={save} />);
  await user.type(screen.getByLabelText("Access token"), "token-123");
  await user.click(screen.getByRole("button", { name: "Save HTTPS credential" }));
  expect(save).toHaveBeenCalledWith("origin", "rene", "token-123");
  expect(screen.getByLabelText("Access token")).toHaveValue("");
});
~~~

- [ ] **Step 2: Implement secret-safe commands and forms**

Use async Tauri commands that accept a token only for save; serialize no credential DTO. RepoClient returns void for save/forget and RemoteAuthMode metadata only for list/read. Render password inputs with autocomplete="off"; clear form state in finally after submission. Provide SSH agent mode without a token input. Map MissingCredential, KeychainFailure, and SshAgentFailure to remediation that does not echo the underlying secret-bearing URL.

- [ ] **Step 3: Verify and commit**

Run: cd frontend && pnpm test -- --run RemotePanel useAppState && pnpm lint && pnpm build
Expected: PASS.

~~~bash
git add crates/tauri-app/src/commands.rs crates/tauri-app/src/main.rs frontend/src/ipc frontend/src/components/RemotePanel.tsx frontend/src/components/RemotePanel.test.tsx frontend/src/state
git commit -m "feat(frontend): configure remote credentials"
~~~

### Task 4: Final regression and manual release acceptance

**Files:**
- Modify: e2e/specs/remote-transfer.spec.ts
- Modify: docs/ARCHITECTURE.md

**Interfaces:**
- Consumes all prior credential and transfer interfaces.
- Produces documented secure-operation acceptance evidence.

- [ ] **Step 1: Add non-secret E2E assertions**

Extend the local-bare-remote flow to select SSH-agent mode without a token field and assert a missing HTTPS credential produces Save an HTTPS token rather than the raw callback error. Do not connect to a live host in E2E.

- [ ] **Step 2: Run the complete automated suite**

Run: cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check && cd frontend && pnpm test -- --run && pnpm lint && pnpm build
Expected: PASS.

- [ ] **Step 3: Perform and record manual acceptance**

Against a disposable HTTPS repository, save a test token, fetch twice, forget it, and verify the next fetch requests a credential. Against a disposable SSH-agent repository, fetch and push with an agent-loaded key. Inspect .git/config, app errors, and visible progress to confirm no token appears. Add the exact manual procedure to docs/ARCHITECTURE.md's testing section.

- [ ] **Step 4: Commit**

~~~bash
git add e2e/specs/remote-transfer.spec.ts docs/ARCHITECTURE.md
git commit -m "test: document remote credential acceptance"
~~~
