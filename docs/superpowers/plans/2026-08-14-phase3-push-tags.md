# Push and Tag Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (- [ ]) syntax for tracking.

**Goal:** Push the current branch and selected local tags safely, while allowing users to create and delete local lightweight or annotated tags.

**Architecture:** git_core::remote builds explicit non-force refspecs and uses the shared transfer callbacks. Worker, IPC, state, and RemotePanel expose branch/tag mutations and refresh after matching transfer completion.

**Tech Stack:** Rust 2021, git2 0.21, Tauri 2, React 18, TypeScript, Vitest, WebdriverIO.

**Spec:** docs/superpowers/specs/2026-08-14-phase3-remote-workflows-design.md

## Global Constraints

- Push only non-force refspecs; never delete or force-update a remote ref.
- Tag deletion is local-only and requires frontend confirmation.
- Tags are listed from local refs and carry annotated-tag metadata when available.
- Use the shared credential/progress callbacks and transfer operation IDs.
- All Git tests use a real temporary bare remote.

---

### Task 1: Local tags and safe push primitives in git-core

**Files:**
- Modify: crates/git-core/src/remote.rs
- Modify: crates/git-core/tests/remote.rs

**Interfaces:**
- Produces TagInfo, list_tags, create_tag, delete_tag, push_current_branch, and push_tags.

- [ ] **Step 1: Write failing integration tests**

~~~rust
#[test]
fn creates_lists_and_deletes_lightweight_and_annotated_tags() {
    let (_dir, repo) = common::repo_with_initial_commit();
    create_tag(&repo, "v1.0.0", None, None).unwrap();
    create_tag(&repo, "release-note", Some("ship it"), Some(signature())).unwrap();

    assert!(list_tags(&repo).unwrap().iter().any(|tag| tag.name == "release-note" && tag.annotated));
    delete_tag(&repo, "v1.0.0").unwrap();
}

#[test]
fn pushes_branch_and_selected_tag_without_force() {
    let fixture = common::local_and_bare_remote();
    push_current_branch(&fixture.local, "origin", &mut auth, &mut reporter).unwrap();
    push_tags(&fixture.local, "origin", &["v1.0.0".into()], &mut auth, &mut reporter).unwrap();
}
~~~

- [ ] **Step 2: Run the tests**

Run: cargo test -p git-core --test remote tag push
Expected: FAIL until the tag and push APIs exist.

- [ ] **Step 3: Implement explicit refspec generation**

~~~rust
pub struct TagInfo {
    pub name: String,
    pub target_id: String,
    pub annotated: bool,
    pub message: Option<String>,
    pub tagger_name: Option<String>,
    pub timestamp: Option<i64>,
}

pub fn create_tag(repo: &Repository, name: &str, message: Option<&str>) -> Result<(), RemoteError>;
pub fn delete_tag(repo: &Repository, name: &str) -> Result<(), RemoteError>;
pub fn push_current_branch(repo: &Repository, remote_name: &str, auth: &mut dyn CredentialProvider, reporter: &mut dyn TransferReporter) -> Result<(), RemoteError>;
pub fn push_tags(repo: &Repository, remote_name: &str, names: &[String], auth: &mut dyn CredentialProvider, reporter: &mut dyn TransferReporter) -> Result<(), RemoteError>;
~~~

Validate names through git2 before writing refs. Build branch refspec refs/heads/{branch}:refs/heads/{branch}; build tag refspec refs/tags/{name}:refs/tags/{name}; reject names beginning with plus and pass no force option. Use PushOptions with RemoteCallbacks, including push_transfer_progress and push_update_reference.

- [ ] **Step 4: Verify and commit**

Run: cargo test -p git-core --test remote && cargo fmt --all -- --check
Expected: PASS.

~~~bash
git add crates/git-core/src/remote.rs crates/git-core/tests/remote.rs
git commit -m "feat(git-core): push branches and tags"
~~~

### Task 2: Worker and transport contract

**Files:**
- Modify: crates/tauri-app/src/worker.rs
- Modify: crates/tauri-app/src/commands.rs
- Modify: crates/tauri-app/src/main.rs
- Modify: frontend/src/ipc/RepoClient.ts
- Modify: frontend/src/ipc/tauriRepoClient.ts
- Modify: frontend/src/state/useAppState.ts
- Modify: frontend/src/state/useAppState.test.ts

**Interfaces:**
- Consumes Task 1 operations.
- Produces listTags, createTag, deleteTag, pushCurrentBranch, and pushTags on RepoClient.

- [ ] **Step 1: Add failing contract tests**

Assert a Worker pushes a real branch and returns tag DTOs. Extend the frontend mock client fixture with every new method and write a state test that refreshes tags after create/delete and starts an event-tracked operation for push.

- [ ] **Step 2: Implement DTOs and mutation wrappers**

~~~ts
export interface TagInfo {
  name: string;
  targetId: string;
  annotated: boolean;
  message: string | null;
  taggerName: string | null;
  timestamp: number | null;
}

createTag(name: string, message: string | null): Promise<void>;
deleteTag(name: string): Promise<void>;
pushCurrentBranch(remoteName: string): Promise<string>;
pushTags(remoteName: string, names: string[]): Promise<string>;
~~~

Return operation IDs for both push commands and use the existing transfer subscription to complete/refresh them. Add tags: TagInfo[] to AppState and fetch them in refresh.

- [ ] **Step 3: Verify and commit**

Run: cargo test -p tauri-app && cd frontend && pnpm test -- --run useAppState && pnpm lint
Expected: PASS.

~~~bash
git add crates/tauri-app/src frontend/src/ipc frontend/src/state
git commit -m "feat(ipc): expose push and tag operations"
~~~

### Task 3: Tag and Push UI with E2E

**Files:**
- Create: frontend/src/components/TagPanel.tsx
- Create: frontend/src/components/TagPanel.test.tsx
- Modify: frontend/src/components/RemotePanel.tsx
- Modify: frontend/src/App.tsx
- Modify: frontend/src/index.css
- Modify: e2e/specs/remote-transfer.spec.ts

**Interfaces:**
- Consumes tags and Task 2 mutation callbacks.
- Produces tag form, local-delete confirmation, selected/all tag push, and branch Push control.

- [ ] **Step 1: Write failing component tests**

~~~tsx
it("requires confirmation before deleting a local tag", async () => {
  render(<TagPanel tags={[tag]} onDelete={deleteTag} />);
  await user.click(screen.getByRole("button", { name: "Delete v1.0.0" }));
  expect(screen.getByRole("dialog", { name: "Delete local tag v1.0.0" })).toBeInTheDocument();
});
~~~

- [ ] **Step 2: Implement focused controls**

Provide radio buttons for lightweight and annotated tag creation; require a message for annotated tags. Show a checkbox per tag and a Push selected tags button; an empty selected list means push all local tags only when the user chooses the clearly-labelled Push all tags action. Place current-branch Push in RemotePanel and disable every push control when an operation, merge, or rebase is active.

- [ ] **Step 3: Verify and commit**

Run: cd frontend && pnpm test -- --run TagPanel RemotePanel && pnpm lint && pnpm build
Expected: PASS.

~~~bash
git add frontend/src/components frontend/src/App.tsx frontend/src/index.css e2e/specs/remote-transfer.spec.ts
git commit -m "feat(frontend): push branches and manage tags"
~~~
