# Branch Management Design

Status: Approved

## Context

Phase 1 (see `docs/superpowers/specs/2026-08-12-browsitory-phase1-design.md`) shipped a full
repo view — commit history, diff, staging, commit — for the current branch's HEAD only. Phase 2,
per `docs/ARCHITECTURE.md`'s roadmap, adds branch management, stash, merge with conflict
resolution, interactive rebase, a blame viewer, and a multi-branch commit graph. Those six are
independent subsystems, each with its own `git-core` module and frontend surface; this spec
covers only the first of them, branch management — the foundation the others build UI context on.

**Goals:**
- List local branches, showing which one is current.
- Switch branches from a fast, keyboard-reachable UI (current branch
  name visible near the history header, click opens a searchable switcher).
- Create a branch from HEAD or from any commit selected in `HistoryList`, auto-switching to it.
- Rename a branch.
- Delete a branch, blocked by default if it has unmerged commits, with an explicit
  confirm-and-force retry path.

**Non-goals (explicitly deferred):**
- Remote-tracking branches (listing `origin/*`, push/pull) — Phase 3.
- Stash integration for dirty-tree switches — stash is its own Phase 2 subsystem; this design
  does not attempt to fold stash-on-switch in.
- Merge, rebase, cherry-pick, or any branch-graph rendering — later Phase 2 subsystems.
- A generic "force everything" toggle — delete's force path is a single scoped retry, not a
  reusable UI pattern.

## Architecture

### `git-core` addition: `branch.rs`

Same shape as `status.rs`/`log.rs`/`diff.rs`: free functions taking `&git2::Repository`, a
`thiserror` `BranchError` enum, tested against real temp-dir repos (`crates/git-core/tests/branch.rs`).

```rust
pub struct BranchInfo {
    pub name: String,
    pub is_current: bool,
}

pub fn list_branches(repo: &Repository) -> Result<Vec<BranchInfo>, BranchError>;

// start_point: "HEAD" or a commit id (full or short hex).
pub fn create_branch(repo: &Repository, name: &str, start_point: &str) -> Result<(), BranchError>;

pub fn switch_branch(repo: &Repository, name: &str) -> Result<(), BranchError>;

pub fn delete_branch(repo: &Repository, name: &str, force: bool) -> Result<(), BranchError>;

pub fn rename_branch(repo: &Repository, old_name: &str, new_name: &str) -> Result<(), BranchError>;
```

- `list_branches`: iterates `Repository::branches(Some(BranchType::Local))`; `is_current` via
  `Branch::is_head()`.
- `create_branch`: resolves `start_point` to a commit (`"HEAD"` via `repo.head()`, otherwise
  `Repository::revparse_single`), creates the branch via `Repository::branch()`, then calls
  `switch_branch` — creation always auto-switches (confirmed default; no separate flag). If the
  auto-switch step fails (dirty-tree conflict), the branch ref is *not* rolled back — it stays
  created, matching `git checkout -b`'s own behavior — and the `BranchError::Checkout` error
  propagates so the frontend can tell the user the branch exists but they're still on the old one.
- `switch_branch`: `Repository::set_head()` + `checkout_head()` with the default (safe)
  `CheckoutBuilder` — libgit2's safe checkout natively refuses to overwrite modified/untracked
  files that differ between the current and target trees. That native error is wrapped into
  `BranchError::Checkout` and propagated as-is; no separate dirty-tree pre-check.
- `delete_branch`: when `force` is `false`, checks whether the branch tip is an ancestor of HEAD
  via `Repository::graph_descendant_of(head_oid, branch_oid)`; if not, returns
  `BranchError::NotMerged` without deleting. When `force` is `true`, deletes unconditionally
  (`Branch::delete()`).
- `rename_branch`: `Branch::rename()`.

### `tauri-app`: `Worker`/`Command` and Tauri commands

`worker.rs`'s `Command` enum gains five variants, matching the existing shape (`{ args, reply:
Sender<Result<T, String>> }`), matched in the worker thread loop, delegating to
`git_core::branch::*` with `.map_err(|e| e.to_string())`:

```rust
ListBranches { reply: Sender<Result<Vec<BranchInfo>, String>> },
CreateBranch { name: String, start_point: String, reply: Sender<Result<(), String>> },
SwitchBranch { name: String, reply: Sender<Result<(), String>> },
DeleteBranch { name: String, force: bool, reply: Sender<Result<(), String>> },
RenameBranch { old_name: String, new_name: String, reply: Sender<Result<(), String>> },
```

`commands.rs` gets five thin `async fn` Tauri commands, same pass-through pattern as existing
commands (no dedicated tests, per the testing convention — pass-through commands are covered by
the `git-core`/`Worker` tests they call).

### `RepoClient` / frontend IPC

`frontend/src/ipc/RepoClient.ts` gains:

```ts
export interface BranchInfo {
  name: string;
  isCurrent: boolean;
}

listBranches(): Promise<BranchInfo[]>;
createBranch(name: string, startPoint: string): Promise<void>;
switchBranch(name: string): Promise<void>;
deleteBranch(name: string, force: boolean): Promise<void>;
renameBranch(oldName: string, newName: string): Promise<void>;
```

`tauriRepoClient.ts` implements each as a thin `invoke()` call, matching the existing 11 methods'
pattern exactly.

### Frontend state and components

`useAppState.ts` gains `branches: BranchInfo[]` and derives the current branch name from it;
refetched after `openRepo` and after any of the five new mutations succeed, matching the
existing refetch-after-mutation pattern staging already uses.

Two component changes:

- **`BranchSwitcher.tsx`** (new): a popover/dropdown near `HistoryList`'s header showing the
  current branch name; opening it shows a searchable list of `branches`. Selecting one calls
  `switchBranch` + refetch. A "New Branch…" entry opens a small inline create form (name input,
  `startPoint` defaulting to `"HEAD"`). Each branch row gets small rename/delete affordances;
  delete calls `deleteBranch(name, false)` first, and on a `BranchError::NotMerged` rejection
  shows an inline confirm that retries with `force: true`.
- **`HistoryList.tsx`** (modified): a context-menu (right-click) entry per commit row, "Branch
  from here", opening the same create form with `startPoint` pre-filled to that commit's id.

### Error handling

No new error-plumbing: branch errors flow through the same `Result<T, String>` → rejected
promise → component-level catch → inline error display already established for `openRepo`
(`bde37f8`).

### Testing

- `crates/git-core/tests/branch.rs`: create/list/switch/delete/rename against real temp-dir
  repos; a test asserting `switch_branch` fails when the target checkout would overwrite a
  conflicting modified file; a test asserting `delete_branch(force: false)` fails on an unmerged
  branch and `delete_branch(force: true)` succeeds on the same branch.
- `BranchSwitcher.test.tsx` (new) / `HistoryList.test.tsx` (extended): mock `RepoClient`, assert
  switch/create/delete/rename call the right method with the right arguments and that the UI
  reflects the resulting state.
- One new E2E flow (`e2e/specs/`): open repo → create branch → switch → see it reflected in
  history.
