# SM Parity Batch 3b (Undo Stack) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ctrl/Cmd+Z / Ctrl/Cmd+Shift+Z undo and redo the last local Git operation (refs, index, and discarded working-tree content) in both the Tauri app and the VSCode extension.

**Architecture:** `git-core::undo` captures a `RepoSnapshot` (HEAD, local refs, index tree, optional "worktree tree") and restores one snapshot over another after a strict check of the components that differ. `repo-service`'s worker wraps each recordable command in `UndoRecorder::begin`/`finish`, persists the stacks to `<gitdir>/browsitory/undo.json`, and pins snapshot objects with a hidden anchor ref. Tauri and the VSCode sidecar expose `get_undo_state` / `undo_last` / `redo_last`; the frontend keeps `undoState` in `AppState`, adds toolbar buttons, shortcuts, palette commands and toasts with an inverse action.

**Tech Stack:** Rust (`git2` 0.21, `thiserror`, `serde`/`serde_json` in repo-service, `tauri` 2), React + TypeScript, Vitest + Testing Library, WebdriverIO (tauri-driver), `@vscode/test-electron`.

**Spec:** `docs/superpowers/specs/2026-10-18-sm-parity-batch3b-design.md`

## Global Constraints

- Both frontends: Tauri commands **and** `vscode-sidecar` methods; `RepoClient` gets `getUndoState`, `undo`, `redo` in both clients.
- No new dependencies (Rust or npm). `git-core` stays serde-free; `repo-service` already has `serde`/`serde_json`.
- Recordable commands and their scope are exactly the spec's table (`RefsAndIndex`: StageFile, UnstageFile, StageHunk, UnstageHunk, StageLines, UnstageLines, Commit, AmendCommit, CreateBranch, DeleteBranch, RenameBranch, CreateTag, DeleteTag; `Full`: DiscardHunk, DiscardLines, DiscardFile, DiscardAllUnstaged, IgnorePath, SwitchBranch, CheckoutCommit, ResetToCommit, CherryPick, RevertCommit, StartMerge, StartRebase, PullCurrentUpstream, RestoreReflogEntry).
- Undo cap: 50 entries (oldest dropped). Labels max 80 chars, paths middle-ellipsized with "…", commit summaries end-truncated with "…".
- Storage: `<gitdir>/browsitory/undo.json` (`repo.path()`), `{ "version": 1, "undo": [...], "redo": [...] }`, oldest first, atomic write (temp + rename). Anchor ref `refs/browsitory/undo/<worktree-key>`; non-commit objects pinned by `refs/browsitory/undo/<worktree-key>-keep/<n>`. `<worktree-key>` = `main` or the linked worktree's gitdir name.
- Anchor commit author/committer: `Browsitory <browsitory@localhost>`, message `browsitory undo anchor`.
- Reflog messages: `browsitory: undo <label>` / `browsitory: redo <label>`.
- Copy (exact): toast "Undid ⟨label⟩" with action **Redo**; toast "Redid ⟨label⟩" with action **Undo**; error banner "Can't undo ⟨label⟩: the repository changed since. Use the reflog to recover." / "Can't redo ⟨label⟩: the repository changed since. Use the reflog to recover."; "Finish or abort the merge or rebase first."; "Undo failed: ⟨message⟩" / "Redo failed: ⟨message⟩"; tooltips "Undo ⟨label⟩" / "Redo ⟨label⟩" / "Nothing to undo" / "Nothing to redo"; palette "Undo ⟨label⟩" / "Redo ⟨label⟩".
- Shortcuts: `Mod+Z` undo; `Mod+Shift+Z` redo; `Ctrl+Y` redo (non-Mac). Ignored when focus is in `INPUT`/`TEXTAREA`/`SELECT`/contenteditable.
- Tests: `git-core`/`repo-service` use real temp-dir repos (no mocked `Repository`); frontend mocks `RepoClient` only.
- Every push touching `crates/`, `frontend/src/` or `e2e/` needs a `CHANGELOG.md` update (pre-push hook) — the changelog lands in Task 9; push only after Task 9.

## Review Focus

1. **Undo must never overwrite user edits it doesn't know about.** A touched file edited after the operation (e.g. discard, then the user retypes the file) must refuse with `RepositoryChanged` and write nothing. Pinned in Task 2 (`refuses_when_a_touched_file_was_edited_and_writes_nothing`).
2. **Unrelated changes must not block undo** — another branch moved, an untouched file edited. Pinned in Task 2 (`unrelated_branch_and_file_changes_do_not_block_undo`).
3. **Operations performed while a merge/rebase is paused must not become separate entries**, and an aborted merge must leave no entry. Pinned in Task 5 (`stage_during_a_conflicted_merge_is_not_recorded_and_abort_drops_pending`).
4. **A deleted branch's commits must survive `git gc --prune=now`** while its delete is undoable. Pinned in Task 4 (`anchor_keeps_a_deleted_branch_commit_alive_across_gc`).
5. **Ctrl/Cmd+Z inside the commit message box must stay native text undo.** Pinned in Task 8 (`ignores Mod+Z while typing in the commit message`).

---

## File Structure

| File | Responsibility |
|---|---|
| `crates/git-core/src/undo.rs` (new) | `RepoSnapshot`, `capture`, `restore`, worktree tree build/verify/write |
| `crates/git-core/tests/undo.rs` (new) | capture/restore tests on real repos |
| `crates/repo-service/src/worker/undo_store.rs` (new) | `StoredSnapshot`/`UndoEntry` serde mirror, JSON persistence, anchor ref |
| `crates/repo-service/src/worker/undo.rs` (new) | `UndoRecorder`, `UndoPlan`, `plan_for(&Command)`, labels, `WorkerHandle` undo methods |
| `crates/repo-service/src/worker/mod.rs` | `Command::{GetUndoState, Undo, Redo}`, recorder wired into the loop |
| `crates/tauri-app/src/commands/undo.rs` (new) + `mod.rs` + `main.rs` | Tauri commands + DTOs |
| `crates/vscode-sidecar/src/dispatch.rs` | sidecar methods + DTOs |
| `frontend/src/ipc/RepoClient.ts`, `tauriRepoClient.ts`, `vscodeRepoClient.ts` | client methods + types |
| `frontend/src/state/useUndoActions.ts` (new), `useAppState.ts` | `undoState`, `undo()`/`redo()` |
| `frontend/src/state/useToasts.ts`, `components/primitives/ToastRegion.tsx` | toast action button |
| `frontend/src/components/UndoRedoButtons.tsx` (new), `SyncBar.tsx` | toolbar buttons |
| `frontend/src/App.tsx`, `lib/commands.ts`, `lib/shortcuts.ts` | shortcuts, palette, toasts, focus refresh |
| `e2e/specs/undo.spec.ts`, `extension/e2e/src/specs/undo.spec.ts` (new) | E2E |
| `CHANGELOG.md`, `docs/ARCHITECTURE.md`, `docs/USER_GUIDE.md` | docs |

---

### Task 1: `git-core::undo` — snapshot types and `capture`

**Files:**
- Create: `crates/git-core/src/undo.rs`
- Modify: `crates/git-core/src/lib.rs` (add `pub mod undo;` in alphabetical order, after `submodule`)
- Test: `crates/git-core/tests/undo.rs`

**Interfaces:**
- Produces:
  ```rust
  pub enum HeadState { Branch(String), Detached(String) }
  pub struct RepoSnapshot { pub head: HeadState, pub refs: BTreeMap<String, String>, pub index_tree: String, pub worktree_tree: Option<String> }
  pub enum SnapshotScope { RefsAndIndex, Full }
  pub enum UndoError { RepositoryChanged, OperationInProgress, ConflictedIndex, Bare, Git(git2::Error), Io(std::io::Error) }
  pub fn capture(repo: &Repository, scope: SnapshotScope) -> Result<RepoSnapshot, UndoError>
  ```
  All four types derive `Debug, Clone, PartialEq, Eq` (`UndoError`: `Debug` + `thiserror::Error` only).

- [ ] **Step 1: Write the failing tests**

`crates/git-core/tests/undo.rs`:

```rust
use std::fs;
use std::path::Path;

use git2::Repository;
use git_core::undo::{capture, HeadState, SnapshotScope, UndoError};
use tempfile::TempDir;

fn init_repo() -> (TempDir, Repository) {
    let dir = TempDir::new().unwrap();
    let repo = Repository::init(dir.path()).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test User").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
        config.set_bool("core.autocrlf", false).unwrap();
    }
    (dir, repo)
}

fn write(repo: &Repository, path: &str, contents: &str) {
    let full = repo.workdir().unwrap().join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, contents).unwrap();
}

fn read(repo: &Repository, path: &str) -> Option<String> {
    fs::read_to_string(repo.workdir().unwrap().join(path)).ok()
}

fn stage(repo: &Repository, path: &str) {
    let mut index = repo.index().unwrap();
    index.add_path(Path::new(path)).unwrap();
    index.write().unwrap();
}

fn commit(repo: &Repository, message: &str) -> git2::Oid {
    let mut index = repo.index().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = repo.signature().unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents).unwrap()
}

fn seeded() -> (TempDir, Repository) {
    let (dir, repo) = init_repo();
    write(&repo, "a.txt", "one\n");
    stage(&repo, "a.txt");
    commit(&repo, "initial");
    (dir, repo)
}

#[test]
fn capture_is_stable_when_nothing_changes() {
    let (_dir, repo) = seeded();
    write(&repo, "dirty.txt", "untracked\n");
    let first = capture(&repo, SnapshotScope::Full).unwrap();
    let second = capture(&repo, SnapshotScope::Full).unwrap();
    assert_eq!(first, second);
}

#[test]
fn refs_and_index_scope_has_no_worktree_tree() {
    let (_dir, repo) = seeded();
    let snapshot = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    assert_eq!(snapshot.worktree_tree, None);
    assert!(matches!(snapshot.head, HeadState::Branch(ref name) if name.starts_with("refs/heads/")));
    assert_eq!(snapshot.refs.len(), 1);
}

#[test]
fn staging_changes_only_the_index_tree() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    stage(&repo, "a.txt");
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    assert_ne!(before.index_tree, after.index_tree);
    assert_eq!(before.worktree_tree, after.worktree_tree);
    assert_eq!(before.refs, after.refs);
}

#[test]
fn discarding_a_change_changes_the_worktree_tree() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "edited\n");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "a.txt", "one\n");
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    assert_ne!(before.worktree_tree, after.worktree_tree);
    assert_eq!(before.index_tree, after.index_tree);
}

#[test]
fn untracked_and_deleted_files_are_part_of_the_worktree_tree() {
    let (_dir, repo) = seeded();
    let clean = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "nested/new.txt", "new\n");
    let with_untracked = capture(&repo, SnapshotScope::Full).unwrap();
    assert_ne!(clean.worktree_tree, with_untracked.worktree_tree);
    fs::remove_file(repo.workdir().unwrap().join("nested/new.txt")).unwrap();
    fs::remove_file(repo.workdir().unwrap().join("a.txt")).unwrap();
    let with_deletion = capture(&repo, SnapshotScope::Full).unwrap();
    assert_ne!(clean.worktree_tree, with_deletion.worktree_tree);
    assert_eq!(read(&repo, "a.txt"), None);
}

#[test]
fn ignored_files_are_not_snapshotted() {
    let (_dir, repo) = seeded();
    write(&repo, ".gitignore", "*.log\n");
    stage(&repo, ".gitignore");
    commit(&repo, "ignore logs");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "build.log", "noise\n");
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    assert_eq!(before, after);
}

#[test]
fn unborn_head_is_captured_as_a_branch() {
    let (_dir, repo) = init_repo();
    write(&repo, "a.txt", "one\n");
    let snapshot = capture(&repo, SnapshotScope::Full).unwrap();
    assert!(matches!(snapshot.head, HeadState::Branch(_)));
    assert!(snapshot.refs.is_empty());
    assert!(snapshot.worktree_tree.is_some());
}

#[test]
fn detached_head_is_captured_by_oid() {
    let (_dir, repo) = seeded();
    let oid = repo.head().unwrap().target().unwrap();
    repo.set_head_detached(oid).unwrap();
    let snapshot = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    assert_eq!(snapshot.head, HeadState::Detached(oid.to_string()));
}

#[test]
fn a_conflicted_index_refuses_to_capture() {
    let (_dir, repo) = seeded();
    let mut index = repo.index().unwrap();
    let id = repo.blob(b"x\n").unwrap();
    for stage_number in [1u16, 2, 3] {
        index
            .add(&git2::IndexEntry {
                ctime: git2::IndexTime::new(0, 0),
                mtime: git2::IndexTime::new(0, 0),
                dev: 0,
                ino: 0,
                mode: 0o100644,
                uid: 0,
                gid: 0,
                file_size: 0,
                id,
                flags: (stage_number << 12) | "c.txt".len() as u16,
                flags_extended: 0,
                path: b"c.txt".to_vec(),
            })
            .unwrap();
    }
    index.write().unwrap();
    assert!(matches!(
        capture(&repo, SnapshotScope::RefsAndIndex),
        Err(UndoError::ConflictedIndex)
    ));
}

#[cfg(unix)]
#[test]
fn executable_bit_and_symlinks_change_the_worktree_tree() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, repo) = seeded();
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    let full = repo.workdir().unwrap().join("a.txt");
    fs::set_permissions(&full, fs::Permissions::from_mode(0o755)).unwrap();
    let exec = capture(&repo, SnapshotScope::Full).unwrap();
    assert_ne!(before.worktree_tree, exec.worktree_tree);
    std::os::unix::fs::symlink("a.txt", repo.workdir().unwrap().join("link")).unwrap();
    let linked = capture(&repo, SnapshotScope::Full).unwrap();
    assert_ne!(exec.worktree_tree, linked.worktree_tree);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test undo`
Expected: FAIL — `unresolved import git_core::undo`.

- [ ] **Step 3: Implement `capture`**

`crates/git-core/src/undo.rs`:

```rust
//! Repository snapshots for the undo stack: capture the state an operation changes, and restore
//! one snapshot over another after checking nothing else touched the same state. See
//! `docs/superpowers/specs/2026-10-18-sm-parity-batch3b-design.md`.
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use git2::{
    Index, IndexEntry, IndexTime, ObjectType, Oid, Repository, RepositoryState, Status,
    StatusOptions, TreeWalkMode, TreeWalkResult,
};
use thiserror::Error;

const MODE_FILE: u32 = 0o100644;
const MODE_EXEC: u32 = 0o100755;
const MODE_LINK: u32 = 0o120000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadState {
    /// Full ref name, e.g. `refs/heads/main`. May be unborn.
    Branch(String),
    /// Commit oid.
    Detached(String),
}

// git-core stays serde-free; repo-service persists a mirror type (`StoredSnapshot`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSnapshot {
    pub head: HeadState,
    /// `refs/heads/*` and `refs/tags/*` -> oid.
    pub refs: BTreeMap<String, String>,
    pub index_tree: String,
    /// HEAD tree with every modified/deleted/untracked (not ignored) file overlaid. `None` for
    /// `SnapshotScope::RefsAndIndex`.
    pub worktree_tree: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotScope {
    RefsAndIndex,
    Full,
}

#[derive(Debug, Error)]
pub enum UndoError {
    #[error("the repository changed since this operation")]
    RepositoryChanged,
    #[error("a merge, rebase, cherry-pick or revert is in progress")]
    OperationInProgress,
    #[error("the index has conflicts")]
    ConflictedIndex,
    #[error("the repository has no working directory")]
    Bare,
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

pub fn capture(repo: &Repository, scope: SnapshotScope) -> Result<RepoSnapshot, UndoError> {
    let mut index = repo.index()?;
    index.read(false)?;
    if index.has_conflicts() {
        return Err(UndoError::ConflictedIndex);
    }
    let index_tree = index.write_tree()?.to_string();
    let worktree_tree = match scope {
        SnapshotScope::RefsAndIndex => None,
        SnapshotScope::Full => Some(build_worktree_tree(repo)?.to_string()),
    };
    Ok(RepoSnapshot {
        head: head_state(repo)?,
        refs: local_refs(repo)?,
        index_tree,
        worktree_tree,
    })
}

fn head_state(repo: &Repository) -> Result<HeadState, UndoError> {
    let head = repo.find_reference("HEAD")?;
    if let Some(target) = head.symbolic_target()? {
        return Ok(HeadState::Branch(target.to_string()));
    }
    let oid = head
        .target()
        .ok_or_else(|| git2::Error::from_str("HEAD has no target"))?;
    Ok(HeadState::Detached(oid.to_string()))
}

fn local_refs(repo: &Repository) -> Result<BTreeMap<String, String>, UndoError> {
    let mut refs = BTreeMap::new();
    for reference in repo.references()? {
        let reference = reference?;
        let Ok(name) = reference.name() else { continue };
        let Some(target) = reference.target() else { continue };
        if name.starts_with("refs/heads/") || name.starts_with("refs/tags/") {
            refs.insert(name.to_string(), target.to_string());
        }
    }
    Ok(refs)
}

struct WorktreeConfig {
    filemode: bool,
    symlinks: bool,
}

fn worktree_config(repo: &Repository) -> Result<WorktreeConfig, UndoError> {
    let config = repo.config()?;
    Ok(WorktreeConfig {
        filemode: config.get_bool("core.filemode").unwrap_or(cfg!(unix)),
        symlinks: config.get_bool("core.symlinks").unwrap_or(cfg!(unix)),
    })
}

fn build_worktree_tree(repo: &Repository) -> Result<Oid, UndoError> {
    let workdir = repo.workdir().ok_or(UndoError::Bare)?;
    let config = worktree_config(repo)?;
    let mut overlay = Index::new()?;
    if let Ok(head) = repo.head() {
        overlay.read_tree(&head.peel_to_tree()?)?;
    }
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .exclude_submodules(true);
    for entry in repo.statuses(Some(&mut options))?.iter() {
        if entry.status().contains(Status::IGNORED) {
            continue;
        }
        let path = entry.path()?.to_string();
        // An untracked nested repository is reported as "dir/"; it is not ours to snapshot.
        if path.ends_with('/') {
            continue;
        }
        let previous_mode = overlay.get_path(Path::new(&path), 0).map(|e| e.mode);
        match read_worktree_file(&workdir.join(&path), &config, previous_mode)? {
            Some((bytes, mode)) => {
                let id = repo.blob(&bytes)?;
                overlay.add(&index_entry(&path, id, mode))?;
            }
            None => {
                let _ = overlay.remove_path(Path::new(&path));
            }
        }
    }
    Ok(overlay.write_tree_to(repo)?)
}

/// Raw bytes (no clean filters) and git mode of a worktree path; `None` when absent or a
/// directory. `previous_mode` keeps the exec bit when `core.filemode` is off and the symlink
/// mode when `core.symlinks` is off, matching what git itself records.
fn read_worktree_file(
    full: &Path,
    config: &WorktreeConfig,
    previous_mode: Option<u32>,
) -> io::Result<Option<(Vec<u8>, u32)>> {
    let meta = match fs::symlink_metadata(full) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    if meta.file_type().is_symlink() {
        return Ok(Some((link_target_bytes(&fs::read_link(full)?), MODE_LINK)));
    }
    if meta.is_dir() {
        return Ok(None);
    }
    let bytes = fs::read(full)?;
    let mode = if !config.symlinks && previous_mode == Some(MODE_LINK) {
        MODE_LINK
    } else if config.filemode {
        if is_executable(&meta) {
            MODE_EXEC
        } else {
            MODE_FILE
        }
    } else if previous_mode == Some(MODE_EXEC) {
        MODE_EXEC
    } else {
        MODE_FILE
    };
    Ok(Some((bytes, mode)))
}

#[cfg(unix)]
fn is_executable(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn link_target_bytes(target: &Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    target.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn link_target_bytes(target: &Path) -> Vec<u8> {
    target.to_string_lossy().replace('\\', "/").into_bytes()
}

fn index_entry(path: &str, id: Oid, mode: u32) -> IndexEntry {
    IndexEntry {
        ctime: IndexTime::new(0, 0),
        mtime: IndexTime::new(0, 0),
        dev: 0,
        ino: 0,
        mode,
        uid: 0,
        gid: 0,
        file_size: 0,
        id,
        flags: path.len().min(0xfff) as u16,
        flags_extended: 0,
        path: path.as_bytes().to_vec(),
    }
}
```

In this task, import only what compiles warning-free: drop `ObjectType`, `RepositoryState`, `TreeWalkMode`, `TreeWalkResult` from the `git2` import above. Task 2 adds them back.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-core --test undo && cargo clippy -p git-core --all-targets -- -D warnings`
Expected: all PASS, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/undo.rs crates/git-core/src/lib.rs crates/git-core/tests/undo.rs
git commit -m "feat(git-core): capture repository snapshots for undo"
```

---

### Task 2: `git-core::undo::restore` with strict verification

**Files:**
- Modify: `crates/git-core/src/undo.rs`
- Test: `crates/git-core/tests/undo.rs`

**Interfaces:**
- Consumes: Task 1 types.
- Produces: `pub fn restore(repo: &Repository, expected: &RepoSnapshot, target: &RepoSnapshot, reflog_message: &str) -> Result<(), UndoError>`

- [ ] **Step 1: Write the failing tests** (append to `crates/git-core/tests/undo.rs`; extend the `use` line to `use git_core::undo::{capture, restore, HeadState, RepoSnapshot, SnapshotScope, UndoError};`)

```rust
/// Runs `op`, then checks undo (after -> before) and redo (before -> after) both round-trip.
fn assert_round_trip(repo: &Repository, scope: SnapshotScope, op: impl FnOnce(&Repository)) -> (RepoSnapshot, RepoSnapshot) {
    let before = capture(repo, scope).unwrap();
    op(repo);
    let after = capture(repo, scope).unwrap();
    assert_ne!(before, after, "operation changed nothing");
    restore(repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(capture(repo, scope).unwrap(), before, "undo did not restore");
    restore(repo, &before, &after, "browsitory: redo test").unwrap();
    assert_eq!(capture(repo, scope).unwrap(), after, "redo did not restore");
    (before, after)
}

#[test]
fn undoes_and_redoes_staging() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    assert_round_trip(&repo, SnapshotScope::RefsAndIndex, |repo| stage(repo, "a.txt"));
}

#[test]
fn undoes_a_discard_of_a_modified_file() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "precious edit\n");
    assert_round_trip(&repo, SnapshotScope::Full, |repo| {
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.force().path("a.txt");
        repo.checkout_head(Some(&mut checkout)).unwrap();
    });
    // assert_round_trip ends after redo, so the discard is applied again.
    assert_eq!(read(&repo, "a.txt").as_deref(), Some("one\n"));
}

#[test]
fn undoes_a_discard_of_an_untracked_file_in_a_new_directory() {
    let (_dir, repo) = seeded();
    write(&repo, "new/dir/file.txt", "keep me\n");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    fs::remove_dir_all(repo.workdir().unwrap().join("new")).unwrap();
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(read(&repo, "new/dir/file.txt").as_deref(), Some("keep me\n"));
    restore(&repo, &before, &after, "browsitory: redo test").unwrap();
    assert!(!repo.workdir().unwrap().join("new").exists(), "empty dirs are removed");
}

#[test]
fn undoes_a_commit_leaving_changes_staged() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    stage(&repo, "a.txt");
    let first = repo.head().unwrap().target().unwrap();
    let (before, after) = assert_round_trip(&repo, SnapshotScope::RefsAndIndex, |repo| {
        commit(repo, "second");
    });
    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(repo.head().unwrap().target().unwrap(), first);
    let statuses = repo.statuses(None).unwrap();
    assert!(statuses.iter().any(|e| e.status().contains(git2::Status::INDEX_MODIFIED)));
}

#[test]
fn undoes_the_first_commit_back_to_an_unborn_branch() {
    let (_dir, repo) = init_repo();
    write(&repo, "a.txt", "one\n");
    stage(&repo, "a.txt");
    assert_round_trip(&repo, SnapshotScope::RefsAndIndex, |repo| {
        commit(repo, "initial");
    });
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    assert_eq!(before.refs.len(), 1);
}

#[test]
fn undoes_a_hard_reset() {
    let (_dir, repo) = seeded();
    let first = repo.head().unwrap().target().unwrap();
    write(&repo, "a.txt", "two\n");
    stage(&repo, "a.txt");
    commit(&repo, "second");
    write(&repo, "a.txt", "uncommitted\n");
    write(&repo, "b.txt", "untracked survives\n");
    assert_round_trip(&repo, SnapshotScope::Full, |repo| {
        let target = repo.find_object(first, None).unwrap();
        repo.reset(&target, git2::ResetType::Hard, None).unwrap();
    });
    let after_redo = read(&repo, "a.txt");
    assert_eq!(after_redo.as_deref(), Some("one\n"));
}

#[test]
fn undoes_deleting_a_branch_and_an_annotated_tag() {
    let (_dir, repo) = seeded();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("feature", &head, false).unwrap();
    let sig = repo.signature().unwrap();
    repo.tag("v1", head.as_object(), &sig, "release", false).unwrap();
    assert_round_trip(&repo, SnapshotScope::RefsAndIndex, |repo| {
        repo.find_branch("feature", git2::BranchType::Local).unwrap().delete().unwrap();
        repo.find_reference("refs/tags/v1").unwrap().delete().unwrap();
    });
    assert!(repo.find_reference("refs/tags/v1").is_err(), "redo deleted it again");
}

#[test]
fn undoes_checking_out_a_commit() {
    let (_dir, repo) = seeded();
    let first = repo.head().unwrap().target().unwrap();
    write(&repo, "a.txt", "two\n");
    stage(&repo, "a.txt");
    commit(&repo, "second");
    let (before, after) = assert_round_trip(&repo, SnapshotScope::Full, |repo| {
        let target = repo.find_object(first, None).unwrap();
        repo.checkout_tree(&target, None).unwrap();
        repo.set_head_detached(first).unwrap();
    });
    assert!(matches!(before.head, HeadState::Branch(_)));
    assert_eq!(after.head, HeadState::Detached(first.to_string()));
}

#[test]
fn refuses_when_a_touched_file_was_edited_and_writes_nothing() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "precious edit\n");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "a.txt", "one\n"); // the discard
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "a.txt", "typed again after the discard\n");
    let result = restore(&repo, &after, &before, "browsitory: undo test");
    assert!(matches!(result, Err(UndoError::RepositoryChanged)));
    assert_eq!(read(&repo, "a.txt").as_deref(), Some("typed again after the discard\n"));
}

#[test]
fn refuses_when_a_touched_ref_moved() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    stage(&repo, "a.txt");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    commit(&repo, "second");
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    write(&repo, "a.txt", "three\n");
    stage(&repo, "a.txt");
    commit(&repo, "third, outside the app");
    let result = restore(&repo, &after, &before, "browsitory: undo test");
    assert!(matches!(result, Err(UndoError::RepositoryChanged)));
}

#[test]
fn refuses_when_the_index_changed_for_an_index_entry() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    write(&repo, "b.txt", "b\n");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    stage(&repo, "a.txt");
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    stage(&repo, "b.txt");
    let result = restore(&repo, &after, &before, "browsitory: undo test");
    assert!(matches!(result, Err(UndoError::RepositoryChanged)));
}

#[test]
fn unrelated_branch_and_file_changes_do_not_block_undo() {
    let (_dir, repo) = seeded();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("other", &head, false).unwrap();
    write(&repo, "a.txt", "edit\n");
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    write(&repo, "a.txt", "one\n");
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    // Unrelated: another branch moves, another file is edited.
    let tree = head.tree().unwrap();
    let sig = repo.signature().unwrap();
    let moved = repo.commit(None, &sig, &sig, "elsewhere", &tree, &[&head]).unwrap();
    repo.reference("refs/heads/other", moved, true, "move other").unwrap();
    write(&repo, "unrelated.txt", "new\n");
    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(read(&repo, "a.txt").as_deref(), Some("edit\n"));
    assert_eq!(read(&repo, "unrelated.txt").as_deref(), Some("new\n"));
}

#[test]
fn refuses_while_a_merge_is_in_progress() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    stage(&repo, "a.txt");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    let second = commit(&repo, "second");
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    fs::write(repo.path().join("MERGE_HEAD"), format!("{second}\n")).unwrap();
    let result = restore(&repo, &after, &before, "browsitory: undo test");
    assert!(matches!(result, Err(UndoError::OperationInProgress)));
}

#[test]
fn crlf_content_round_trips_byte_exact() {
    let (_dir, repo) = seeded();
    let full = repo.workdir().unwrap().join("a.txt");
    fs::write(&full, b"line one\r\nline two\r\n").unwrap();
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    fs::write(&full, b"one\n").unwrap();
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert_eq!(fs::read(&full).unwrap(), b"line one\r\nline two\r\n");
}

#[test]
fn writes_reflog_messages_for_moved_refs() {
    let (_dir, repo) = seeded();
    write(&repo, "a.txt", "two\n");
    stage(&repo, "a.txt");
    let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    commit(&repo, "second");
    let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
    restore(&repo, &after, &before, "browsitory: undo Commit “second”").unwrap();
    let branch = match &before.head {
        HeadState::Branch(name) => name.clone(),
        HeadState::Detached(_) => unreachable!(),
    };
    let reflog = repo.reflog(&branch).unwrap();
    assert_eq!(reflog.get(0).unwrap().message(), Some("browsitory: undo Commit “second”"));
}

#[cfg(unix)]
#[test]
fn restores_executable_bit_and_symlinks() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, repo) = seeded();
    let workdir = repo.workdir().unwrap().to_path_buf();
    fs::set_permissions(workdir.join("a.txt"), fs::Permissions::from_mode(0o755)).unwrap();
    std::os::unix::fs::symlink("a.txt", workdir.join("link")).unwrap();
    let before = capture(&repo, SnapshotScope::Full).unwrap();
    fs::remove_file(workdir.join("link")).unwrap();
    fs::set_permissions(workdir.join("a.txt"), fs::Permissions::from_mode(0o644)).unwrap();
    let after = capture(&repo, SnapshotScope::Full).unwrap();
    restore(&repo, &after, &before, "browsitory: undo test").unwrap();
    assert!(fs::symlink_metadata(workdir.join("link")).unwrap().file_type().is_symlink());
    assert_eq!(fs::read_link(workdir.join("link")).unwrap(), Path::new("a.txt"));
    assert_ne!(fs::metadata(workdir.join("a.txt")).unwrap().permissions().mode() & 0o111, 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test undo`
Expected: FAIL — `cannot find function restore`.

- [ ] **Step 3: Implement `restore`** (append to `crates/git-core/src/undo.rs`; add `std::collections::BTreeSet` to the `use` list together with `ObjectType`, `RepositoryState`, `TreeWalkMode`, `TreeWalkResult`)

```rust
pub fn restore(
    repo: &Repository,
    expected: &RepoSnapshot,
    target: &RepoSnapshot,
    reflog_message: &str,
) -> Result<(), UndoError> {
    if repo.state() != RepositoryState::Clean {
        return Err(UndoError::OperationInProgress);
    }

    // Only the components that differ between the two snapshots are checked and written, so
    // unrelated changes elsewhere never block an undo.
    let head_differs = expected.head != target.head;
    let changed_refs: Vec<&String> = expected
        .refs
        .keys()
        .chain(target.refs.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|name| expected.refs.get(*name) != target.refs.get(*name))
        .collect();
    let index_differs = expected.index_tree != target.index_tree;
    let worktree = match (&expected.worktree_tree, &target.worktree_tree) {
        (Some(from), Some(to)) if from != to => Some(WorktreeChange::new(repo, from, to)?),
        _ => None,
    };

    // 1. Verify, writing nothing.
    if head_differs && head_state(repo)? != expected.head {
        return Err(UndoError::RepositoryChanged);
    }
    for name in &changed_refs {
        let current = repo
            .find_reference(name)
            .ok()
            .and_then(|reference| reference.target())
            .map(|oid| oid.to_string());
        if current.as_ref() != expected.refs.get(*name) {
            return Err(UndoError::RepositoryChanged);
        }
    }
    if index_differs {
        let mut index = repo.index()?;
        index.read(false)?;
        if index.has_conflicts() || index.write_tree()?.to_string() != expected.index_tree {
            return Err(UndoError::RepositoryChanged);
        }
    }
    if let Some(change) = &worktree {
        change.verify(repo)?;
    }

    // 2. Write: worktree first (most likely to fail), then index, refs, HEAD.
    if let Some(change) = &worktree {
        change.apply(repo)?;
    }
    if index_differs {
        let mut index = repo.index()?;
        index.read_tree(&repo.find_tree(Oid::from_str(&target.index_tree)?)?)?;
        index.write()?;
    }
    for name in &changed_refs {
        match target.refs.get(*name) {
            Some(oid) => {
                repo.reference(name, Oid::from_str(oid)?, true, reflog_message)?;
            }
            None => repo.find_reference(name)?.delete()?,
        }
    }
    if head_differs {
        match &target.head {
            HeadState::Branch(name) => repo.set_head(name)?,
            HeadState::Detached(oid) => repo.set_head_detached(Oid::from_str(oid)?)?,
        }
    }
    Ok(())
}

type TreeFiles = BTreeMap<String, (Oid, u32)>;

struct WorktreeChange {
    from: TreeFiles,
    to: TreeFiles,
    paths: Vec<String>,
}

impl WorktreeChange {
    fn new(repo: &Repository, from: &str, to: &str) -> Result<Self, UndoError> {
        let from = flatten_tree(repo, from)?;
        let to = flatten_tree(repo, to)?;
        let paths = from
            .keys()
            .chain(to.keys())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .filter(|path| from.get(*path) != to.get(*path))
            .cloned()
            .collect();
        Ok(Self { from, to, paths })
    }

    fn verify(&self, repo: &Repository) -> Result<(), UndoError> {
        let workdir = repo.workdir().ok_or(UndoError::Bare)?;
        let config = worktree_config(repo)?;
        for path in &self.paths {
            let expected = self.from.get(path).copied();
            let previous_mode = expected.map(|(_, mode)| mode);
            let current = read_worktree_file(&workdir.join(path), &config, previous_mode)?
                .map(|(bytes, mode)| Ok::<_, git2::Error>((Oid::hash_object(ObjectType::Blob, &bytes)?, mode)))
                .transpose()?;
            if current != expected {
                return Err(UndoError::RepositoryChanged);
            }
        }
        Ok(())
    }

    fn apply(&self, repo: &Repository) -> Result<(), UndoError> {
        let workdir = repo.workdir().ok_or(UndoError::Bare)?;
        let config = worktree_config(repo)?;
        // Deletions first, so a file can replace a directory that only held deleted files.
        for path in self.paths.iter().filter(|path| !self.to.contains_key(*path)) {
            let full = workdir.join(path);
            remove_existing(&full)?;
            prune_empty_parents(workdir, &full);
        }
        for path in &self.paths {
            if let Some((id, mode)) = self.to.get(path) {
                write_worktree_file(repo, &workdir.join(path), *id, *mode, &config)?;
            }
        }
        Ok(())
    }
}

fn flatten_tree(repo: &Repository, id: &str) -> Result<TreeFiles, UndoError> {
    let tree = repo.find_tree(Oid::from_str(id)?)?;
    let mut files = TreeFiles::new();
    tree.walk(TreeWalkMode::PreOrder, |root, entry| {
        if entry.kind() == Some(ObjectType::Blob) {
            if let Ok(name) = entry.name() {
                files.insert(format!("{root}{name}"), (entry.id(), entry.filemode() as u32));
            }
        }
        TreeWalkResult::Ok
    })?;
    Ok(files)
}

fn remove_existing(full: &Path) -> io::Result<()> {
    match fs::symlink_metadata(full) {
        Ok(meta) if meta.is_dir() => fs::remove_dir(full),
        Ok(_) => fs::remove_file(full),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn prune_empty_parents(workdir: &Path, full: &Path) {
    let mut dir = full.parent();
    while let Some(current) = dir {
        if current == workdir || !current.starts_with(workdir) || fs::remove_dir(current).is_err() {
            break;
        }
        dir = current.parent();
    }
}

fn write_worktree_file(
    repo: &Repository,
    full: &Path,
    id: Oid,
    mode: u32,
    config: &WorktreeConfig,
) -> Result<(), UndoError> {
    let blob = repo.find_blob(id)?;
    remove_existing(full)?;
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent)?;
    }
    if mode == MODE_LINK && config.symlinks {
        create_symlink(blob.content(), full)?;
        return Ok(());
    }
    fs::write(full, blob.content())?;
    set_executable(full, mode == MODE_EXEC)?;
    Ok(())
}

#[cfg(unix)]
fn create_symlink(target: &[u8], full: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(target), full)
}

#[cfg(not(unix))]
fn create_symlink(target: &[u8], full: &Path) -> io::Result<()> {
    fs::write(full, target)
}

#[cfg(unix)]
fn set_executable(full: &Path, executable: bool) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(full)?.permissions();
    let mode = permissions.mode();
    permissions.set_mode(if executable { mode | 0o111 } else { mode & !0o111 });
    fs::set_permissions(full, permissions)
}

#[cfg(not(unix))]
fn set_executable(_full: &Path, _executable: bool) -> io::Result<()> {
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-core --test undo && cargo clippy -p git-core --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/undo.rs crates/git-core/tests/undo.rs
git commit -m "feat(git-core): restore snapshots with strict drift check"
```

---

### Task 3: Labels and recordable-command plans

**Files:**
- Create: `crates/repo-service/src/worker/undo.rs` (labels + `plan_for` only in this task)
- Modify: `crates/repo-service/src/worker/mod.rs` (add `mod undo;`; `mod undo_store;` comes in Task 4)

**Interfaces:**
- Consumes: `git_core::undo::SnapshotScope`, `Command` (in `worker/mod.rs`), `git_core::reset::ResetMode`.
- Produces:
  ```rust
  pub(super) struct UndoPlan { pub label: String, pub scope: SnapshotScope }
  pub(super) fn plan_for(command: &Command) -> Option<UndoPlan>
  fn ellipsize_middle(text: &str, max_chars: usize) -> String
  fn truncate_end(text: &str, max_chars: usize) -> String
  const MAX_LABEL_CHARS: usize = 80;
  ```

- [ ] **Step 1: Write the failing tests** (inline `#[cfg(test)] mod tests` in `worker/undo.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn label(command: Command) -> Option<(String, SnapshotScope)> {
        plan_for(&command).map(|plan| (plan.label, plan.scope))
    }

    #[test]
    fn labels_index_and_ref_commands() {
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::StageFile { path: "src/a.ts".into(), reply: tx }),
            Some(("Stage src/a.ts".into(), SnapshotScope::RefsAndIndex))
        );
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::Commit { message: "Fix parser\n\nbody".into(), reply: tx }),
            Some(("Commit “Fix parser”".into(), SnapshotScope::RefsAndIndex))
        );
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::RenameBranch { old_name: "a".into(), new_name: "b".into(), reply: tx }),
            Some(("Rename branch a to b".into(), SnapshotScope::RefsAndIndex))
        );
    }

    #[test]
    fn labels_worktree_commands_with_full_scope() {
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::DiscardFile { path: "a.txt".into(), reply: tx }),
            Some(("Discard changes to a.txt".into(), SnapshotScope::Full))
        );
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::ResetToCommit {
                commit_id: "1a2b3c4d5e6f".into(),
                mode: git_core::reset::ResetMode::Hard,
                reply: tx
            }),
            Some(("Reset to 1a2b3c4 (hard)".into(), SnapshotScope::Full))
        );
    }

    #[test]
    fn read_and_transfer_commands_are_not_recorded() {
        let (tx, _rx) = channel();
        assert_eq!(label(Command::GetStatus { reply: tx }), None);
        let (tx, _rx) = channel();
        assert_eq!(label(Command::SaveStash { reply: tx }), None);
    }

    #[test]
    fn long_paths_are_middle_ellipsized_to_80_chars() {
        let path = format!("{}/file.rs", "deep/".repeat(30));
        let (tx, _rx) = channel();
        let (text, _) = label(Command::StageFile { path, reply: tx }).unwrap();
        assert_eq!(text.chars().count(), 80);
        assert!(text.starts_with("Stage deep/"));
        assert!(text.ends_with("/file.rs"));
        assert!(text.contains('…'));
    }

    #[test]
    fn long_summaries_are_end_truncated() {
        let (tx, _rx) = channel();
        let (text, _) = label(Command::Commit { message: "x".repeat(200), reply: tx }).unwrap();
        assert_eq!(text.chars().count(), 80);
        assert!(text.ends_with("…”"));
    }

    #[test]
    fn empty_amend_message_has_a_fallback_label() {
        let (tx, _rx) = channel();
        assert_eq!(
            label(Command::AmendCommit { message: "  ".into(), reply: tx }).map(|(l, _)| l),
            Some("Amend last commit".into())
        );
    }
}
```

Check the exact field names of each `Command` variant in `worker/mod.rs` (lines ~60–470) before writing `plan_for`; the tests above use the real ones (`StageFile { path }`, `Commit { message }`, `RenameBranch { old_name, new_name }`, `DiscardFile { path }`, `ResetToCommit { commit_id, mode }`, `AmendCommit { message }`).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p repo-service undo::tests`
Expected: FAIL — `cannot find function plan_for`.

- [ ] **Step 3: Implement**

`crates/repo-service/src/worker/undo.rs` (top of file):

```rust
//! Undo stack: which worker commands are recorded, under which label, and the recorder that
//! snapshots around them. See `docs/superpowers/specs/2026-10-18-sm-parity-batch3b-design.md`.
use git_core::reset::ResetMode;
use git_core::undo::SnapshotScope;

use super::Command;

pub(super) const MAX_LABEL_CHARS: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UndoPlan {
    pub label: String,
    pub scope: SnapshotScope,
}

pub(super) fn plan_for(command: &Command) -> Option<UndoPlan> {
    use SnapshotScope::{Full, RefsAndIndex};
    let (label, scope) = match command {
        Command::StageFile { path, .. } => (with_path("Stage ", path, ""), RefsAndIndex),
        Command::UnstageFile { path, .. } => (with_path("Unstage ", path, ""), RefsAndIndex),
        Command::StageHunk { path, .. } => (with_path("Stage hunk in ", path, ""), RefsAndIndex),
        Command::UnstageHunk { path, .. } => {
            (with_path("Unstage hunk in ", path, ""), RefsAndIndex)
        }
        Command::StageLines { path, .. } => (with_path("Stage lines in ", path, ""), RefsAndIndex),
        Command::UnstageLines { path, .. } => {
            (with_path("Unstage lines in ", path, ""), RefsAndIndex)
        }
        Command::Commit { message, .. } => (quoted("Commit ", message), RefsAndIndex),
        Command::AmendCommit { message, .. } => {
            if message.trim().is_empty() {
                ("Amend last commit".to_string(), RefsAndIndex)
            } else {
                (quoted("Amend ", message), RefsAndIndex)
            }
        }
        Command::CreateBranch { name, .. } => (fit(format!("Create branch {name}")), RefsAndIndex),
        Command::DeleteBranch { name, .. } => (fit(format!("Delete branch {name}")), RefsAndIndex),
        Command::RenameBranch { old_name, new_name, .. } => {
            (fit(format!("Rename branch {old_name} to {new_name}")), RefsAndIndex)
        }
        Command::CreateTag { name, .. } => (fit(format!("Create tag {name}")), RefsAndIndex),
        Command::DeleteTag { name, .. } => (fit(format!("Delete tag {name}")), RefsAndIndex),
        Command::DiscardHunk { path, .. } => (with_path("Discard hunk in ", path, ""), Full),
        Command::DiscardLines { path, .. } => (with_path("Discard lines in ", path, ""), Full),
        Command::DiscardFile { path, .. } => (with_path("Discard changes to ", path, ""), Full),
        Command::DiscardAllUnstaged { .. } => ("Discard all unstaged changes".to_string(), Full),
        Command::IgnorePath { pattern, .. } => (with_path("Ignore ", pattern, ""), Full),
        Command::SwitchBranch { name, .. } => (fit(format!("Switch to {name}")), Full),
        Command::CheckoutCommit { commit_id, .. } => {
            (format!("Check out {}", short(commit_id)), Full)
        }
        Command::ResetToCommit { commit_id, mode, .. } => {
            let mode = match mode {
                ResetMode::Soft => "soft",
                ResetMode::Mixed => "mixed",
                ResetMode::Hard => "hard",
            };
            (format!("Reset to {} ({mode})", short(commit_id)), Full)
        }
        Command::CherryPick { commit_id, .. } => (format!("Cherry-pick {}", short(commit_id)), Full),
        Command::RevertCommit { commit_id, .. } => (format!("Revert {}", short(commit_id)), Full),
        Command::StartMerge { branch_name, .. } => (fit(format!("Merge {branch_name}")), Full),
        Command::StartRebase { onto, .. } => (fit(format!("Rebase onto {onto}")), Full),
        Command::PullCurrentUpstream { .. } => ("Pull".to_string(), Full),
        Command::RestoreReflogEntry { reference, new_id, .. } => {
            (fit(format!("Restore {reference} to {}", short(new_id))), Full)
        }
        _ => return None,
    };
    Some(UndoPlan { label, scope })
}

fn short(oid: &str) -> &str {
    &oid[..oid.len().min(7)]
}

fn fit(label: String) -> String {
    truncate_end(&label, MAX_LABEL_CHARS)
}

fn with_path(prefix: &str, path: &str, suffix: &str) -> String {
    let budget = MAX_LABEL_CHARS.saturating_sub(prefix.chars().count() + suffix.chars().count());
    format!("{prefix}{}{suffix}", ellipsize_middle(path, budget))
}

fn quoted(prefix: &str, message: &str) -> String {
    let summary = message.lines().next().unwrap_or("").trim();
    // prefix + “ + summary + ”
    let budget = MAX_LABEL_CHARS.saturating_sub(prefix.chars().count() + 2);
    format!("{prefix}“{}”", truncate_end(summary, budget))
}

fn truncate_end(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let kept: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{kept}…")
}

fn ellipsize_middle(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let head = keep / 2;
    let tail = keep - head;
    let start: String = text.chars().take(head).collect();
    let end: String = text.chars().skip(count - tail).collect();
    format!("{start}…{end}")
}
```

Add `mod undo;` to the module list in `worker/mod.rs` (alphabetical: after `tag`, before `worktree`). `ResetMode` must be `Copy`/matchable by reference — it is a plain enum in `git-core/src/reset.rs`.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p repo-service undo::tests && cargo clippy -p repo-service --all-targets -- -D warnings`
Expected: PASS. (`plan_for` is unused outside tests until Task 5 — add `#[cfg_attr(not(test), allow(dead_code))]` on `plan_for` and `UndoPlan` in this task and remove it in Task 5.)

- [ ] **Step 5: Commit**

```bash
git add crates/repo-service/src/worker/undo.rs crates/repo-service/src/worker/mod.rs
git commit -m "feat(repo-service): undo labels for recordable worker commands"
```

---

### Task 4: Persistent undo store and GC anchor

**Files:**
- Create: `crates/repo-service/src/worker/undo_store.rs`
- Modify: `crates/repo-service/src/worker/mod.rs` (add `mod undo_store;` after `mod undo;`)

**Interfaces:**
- Consumes: `git_core::undo::{RepoSnapshot, HeadState}`.
- Produces:
  ```rust
  pub(super) struct UndoEntry { pub label: String, pub before: RepoSnapshot, pub after: RepoSnapshot }
  #[derive(Default)] pub(super) struct UndoStacks { pub undo: Vec<UndoEntry>, pub redo: Vec<UndoEntry> }
  pub(super) struct UndoStore { .. }
  impl UndoStore {
      pub fn open(repo: &Repository) -> (Self, UndoStacks);
      pub fn reload_if_changed(&mut self, stacks: &mut UndoStacks);
      pub fn save(&mut self, repo: &Repository, stacks: &UndoStacks) -> Result<(), String>;
  }
  pub(super) const UNDO_LIMIT: usize = 50;
  ```

- [ ] **Step 1: Write the failing tests** (inline `#[cfg(test)] mod tests` in `undo_store.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use git_core::undo::{capture, SnapshotScope};
    use std::process::Command as Process;
    use tempfile::TempDir;

    fn repo_with_commit() -> (TempDir, Repository) {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        {
            let mut config = repo.config().unwrap();
            config.set_str("user.name", "Test User").unwrap();
            config.set_str("user.email", "test@example.com").unwrap();
        }
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("a.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[]).unwrap();
        drop(tree);
        (dir, repo)
    }

    fn entry(repo: &Repository, label: &str) -> UndoEntry {
        let snapshot = capture(repo, SnapshotScope::Full).unwrap();
        UndoEntry { label: label.into(), before: snapshot.clone(), after: snapshot }
    }

    #[test]
    fn saves_and_reloads_both_stacks() {
        let (_dir, repo) = repo_with_commit();
        let (mut store, mut stacks) = UndoStore::open(&repo);
        assert!(stacks.undo.is_empty() && stacks.redo.is_empty());
        stacks.undo.push(entry(&repo, "Stage a.txt"));
        stacks.redo.push(entry(&repo, "Commit “x”"));
        store.save(&repo, &stacks).unwrap();
        let (_, reloaded) = UndoStore::open(&repo);
        assert_eq!(reloaded.undo, stacks.undo);
        assert_eq!(reloaded.redo, stacks.redo);
        assert!(repo.find_reference("refs/browsitory/undo/main").is_ok());
    }

    #[test]
    fn a_corrupt_file_starts_empty() {
        let (_dir, repo) = repo_with_commit();
        std::fs::create_dir_all(repo.path().join("browsitory")).unwrap();
        std::fs::write(repo.path().join("browsitory/undo.json"), "{not json").unwrap();
        let (_, stacks) = UndoStore::open(&repo);
        assert!(stacks.undo.is_empty());
    }

    #[test]
    fn an_unknown_version_starts_empty() {
        let (_dir, repo) = repo_with_commit();
        std::fs::create_dir_all(repo.path().join("browsitory")).unwrap();
        std::fs::write(
            repo.path().join("browsitory/undo.json"),
            r#"{"version":99,"undo":[],"redo":[]}"#,
        )
        .unwrap();
        let (_, stacks) = UndoStore::open(&repo);
        assert!(stacks.undo.is_empty());
    }

    #[test]
    fn empty_stacks_remove_the_anchor_refs() {
        let (_dir, repo) = repo_with_commit();
        let (mut store, mut stacks) = UndoStore::open(&repo);
        stacks.undo.push(entry(&repo, "Stage a.txt"));
        store.save(&repo, &stacks).unwrap();
        stacks.undo.clear();
        store.save(&repo, &stacks).unwrap();
        assert!(repo.find_reference("refs/browsitory/undo/main").is_err());
    }

    #[test]
    fn reload_if_changed_picks_up_another_writer() {
        let (_dir, repo) = repo_with_commit();
        let (mut first, mut first_stacks) = UndoStore::open(&repo);
        let (mut second, mut second_stacks) = UndoStore::open(&repo);
        second_stacks.undo.push(entry(&repo, "Stage a.txt"));
        // Ensure a different mtime on coarse-grained filesystems.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        second.save(&repo, &second_stacks).unwrap();
        first.reload_if_changed(&mut first_stacks);
        assert_eq!(first_stacks.undo.len(), 1);
    }

    #[test]
    fn anchor_keeps_a_deleted_branch_commit_alive_across_gc() {
        if Process::new("git").arg("--version").output().is_err() {
            eprintln!("git CLI not found; skipping gc test");
            return;
        }
        let (dir, repo) = repo_with_commit();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        let sig = repo.signature().unwrap();
        let orphan = repo
            .commit(None, &sig, &sig, "only on feature", &head.tree().unwrap(), &[&head])
            .unwrap();
        repo.reference("refs/heads/feature", orphan, true, "create").unwrap();
        let tag_target = repo.find_object(orphan, None).unwrap();
        let tag = repo.tag("v1", &tag_target, &sig, "annotated", false).unwrap();
        let before = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
        repo.find_reference("refs/heads/feature").unwrap().delete().unwrap();
        repo.find_reference("refs/tags/v1").unwrap().delete().unwrap();
        let after = capture(&repo, SnapshotScope::RefsAndIndex).unwrap();
        let (mut store, mut stacks) = UndoStore::open(&repo);
        stacks.undo.push(UndoEntry { label: "Delete branch feature".into(), before, after });
        store.save(&repo, &stacks).unwrap();
        let status = Process::new("git")
            .args(["-c", "gc.reflogExpire=now", "-c", "gc.reflogExpireUnreachable=now"])
            .args(["reflog", "expire", "--all", "--expire=now"])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        let status = Process::new("git")
            .args(["gc", "--prune=now", "--quiet"])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        let reopened = Repository::open(dir.path()).unwrap();
        assert!(reopened.find_commit(orphan).is_ok(), "deleted branch commit was pruned");
        assert!(reopened.find_tag(tag).is_ok(), "deleted annotated tag object was pruned");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p repo-service undo_store::tests`
Expected: FAIL — `cannot find type UndoStore`.

- [ ] **Step 3: Implement**

`crates/repo-service/src/worker/undo_store.rs`:

```rust
//! Persists the undo/redo stacks to `<gitdir>/browsitory/undo.json` and keeps every object they
//! reference reachable from `refs/browsitory/undo/<worktree-key>` so `git gc` can't prune them.
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use git2::{ObjectType, Oid, Repository, Signature};
use git_core::undo::{HeadState, RepoSnapshot};
use serde::{Deserialize, Serialize};

pub(super) const UNDO_LIMIT: usize = 50;
const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct UndoEntry {
    pub label: String,
    pub before: RepoSnapshot,
    pub after: RepoSnapshot,
}

#[derive(Debug, Default)]
pub(super) struct UndoStacks {
    pub undo: Vec<UndoEntry>,
    pub redo: Vec<UndoEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
enum StoredHead {
    Branch(String),
    Detached(String),
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSnapshot {
    head: StoredHead,
    refs: BTreeMap<String, String>,
    index_tree: String,
    worktree_tree: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct StoredEntry {
    label: String,
    before: StoredSnapshot,
    after: StoredSnapshot,
}

#[derive(Serialize, Deserialize)]
struct StoredFile {
    version: u32,
    undo: Vec<StoredEntry>,
    redo: Vec<StoredEntry>,
}

impl From<&RepoSnapshot> for StoredSnapshot {
    fn from(snapshot: &RepoSnapshot) -> Self {
        Self {
            head: match &snapshot.head {
                HeadState::Branch(name) => StoredHead::Branch(name.clone()),
                HeadState::Detached(oid) => StoredHead::Detached(oid.clone()),
            },
            refs: snapshot.refs.clone(),
            index_tree: snapshot.index_tree.clone(),
            worktree_tree: snapshot.worktree_tree.clone(),
        }
    }
}

impl From<StoredSnapshot> for RepoSnapshot {
    fn from(stored: StoredSnapshot) -> Self {
        Self {
            head: match stored.head {
                StoredHead::Branch(name) => HeadState::Branch(name),
                StoredHead::Detached(oid) => HeadState::Detached(oid),
            },
            refs: stored.refs,
            index_tree: stored.index_tree,
            worktree_tree: stored.worktree_tree,
        }
    }
}

fn to_stored(entries: &[UndoEntry]) -> Vec<StoredEntry> {
    entries
        .iter()
        .map(|entry| StoredEntry {
            label: entry.label.clone(),
            before: (&entry.before).into(),
            after: (&entry.after).into(),
        })
        .collect()
}

fn from_stored(entries: Vec<StoredEntry>) -> Vec<UndoEntry> {
    entries
        .into_iter()
        .map(|entry| UndoEntry {
            label: entry.label,
            before: entry.before.into(),
            after: entry.after.into(),
        })
        .collect()
}

pub(super) struct UndoStore {
    file: PathBuf,
    anchor_ref: String,
    keep_prefix: String,
    mtime: Option<SystemTime>,
}

impl UndoStore {
    pub fn open(repo: &Repository) -> (Self, UndoStacks) {
        let key = worktree_key(repo);
        let mut store = Self {
            file: repo.path().join("browsitory").join("undo.json"),
            anchor_ref: format!("refs/browsitory/undo/{key}"),
            keep_prefix: format!("refs/browsitory/undo/{key}-keep/"),
            mtime: None,
        };
        let stacks = store.load();
        (store, stacks)
    }

    pub fn reload_if_changed(&mut self, stacks: &mut UndoStacks) {
        if modified(&self.file) != self.mtime {
            *stacks = self.load();
        }
    }

    pub fn save(&mut self, repo: &Repository, stacks: &UndoStacks) -> Result<(), String> {
        let file = StoredFile {
            version: FORMAT_VERSION,
            undo: to_stored(&stacks.undo),
            redo: to_stored(&stacks.redo),
        };
        let json = serde_json::to_vec_pretty(&file).map_err(|error| error.to_string())?;
        let dir = self.file.parent().expect("undo file has a parent");
        fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        let temp = dir.join("undo.json.tmp");
        fs::write(&temp, json).map_err(|error| error.to_string())?;
        fs::rename(&temp, &self.file).map_err(|error| error.to_string())?;
        self.mtime = modified(&self.file);
        self.write_anchor(repo, stacks).map_err(|error| error.to_string())
    }

    fn load(&mut self) -> UndoStacks {
        self.mtime = modified(&self.file);
        let Ok(bytes) = fs::read(&self.file) else {
            return UndoStacks::default();
        };
        match serde_json::from_slice::<StoredFile>(&bytes) {
            Ok(file) if file.version == FORMAT_VERSION => UndoStacks {
                undo: from_stored(file.undo),
                redo: from_stored(file.redo),
            },
            Ok(file) => {
                eprintln!("browsitory: ignoring undo history with unknown version {}", file.version);
                UndoStacks::default()
            }
            Err(error) => {
                eprintln!("browsitory: ignoring unreadable undo history: {error}");
                UndoStacks::default()
            }
        }
    }

    fn write_anchor(&self, repo: &Repository, stacks: &UndoStacks) -> Result<(), git2::Error> {
        for reference in repo.references_glob(&format!("{}*", self.keep_prefix))? {
            reference?.delete()?;
        }
        let entries: Vec<&UndoEntry> = stacks.undo.iter().chain(stacks.redo.iter()).collect();
        if entries.is_empty() {
            if let Ok(mut anchor) = repo.find_reference(&self.anchor_ref) {
                anchor.delete()?;
            }
            return Ok(());
        }

        let mut builder = repo.treebuilder(None)?;
        let mut referenced: BTreeSet<Oid> = BTreeSet::new();
        for (n, entry) in entries.iter().enumerate() {
            for (side, snapshot) in [("before", &entry.before), ("after", &entry.after)] {
                builder.insert(format!("{n}-{side}-index"), Oid::from_str(&snapshot.index_tree)?, 0o040000)?;
                if let Some(tree) = &snapshot.worktree_tree {
                    builder.insert(format!("{n}-{side}-worktree"), Oid::from_str(tree)?, 0o040000)?;
                }
                if let HeadState::Detached(oid) = &snapshot.head {
                    referenced.insert(Oid::from_str(oid)?);
                }
                for oid in snapshot.refs.values() {
                    referenced.insert(Oid::from_str(oid)?);
                }
            }
        }
        let tree = repo.find_tree(builder.write()?)?;

        let mut parents = Vec::new();
        let mut keep = 0;
        for oid in referenced {
            let Ok(object) = repo.find_object(oid, None) else { continue };
            if object.kind() == Some(ObjectType::Commit) {
                parents.push(object.peel_to_commit()?);
            } else {
                // A commit can't parent a tag object (or a tag pointing at a tree/blob); pin it
                // with its own ref instead.
                repo.reference(&format!("{}{keep}", self.keep_prefix), oid, true, "browsitory undo keep")?;
                keep += 1;
            }
        }
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
        let signature = Signature::now("Browsitory", "browsitory@localhost")?;
        let anchor = repo.commit(None, &signature, &signature, "browsitory undo anchor", &tree, &parent_refs)?;
        repo.reference(&self.anchor_ref, anchor, true, "browsitory undo anchor")?;
        Ok(())
    }
}

fn modified(file: &Path) -> Option<SystemTime> {
    fs::metadata(file).and_then(|meta| meta.modified()).ok()
}

/// `main` for the main worktree, else the linked worktree's gitdir name (`.git/worktrees/<name>`),
/// reduced to characters that are always valid in a ref name.
fn worktree_key(repo: &Repository) -> String {
    if !repo.is_worktree() {
        return "main".to_string();
    }
    let name = repo
        .path()
        .components()
        .next_back()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .unwrap_or_else(|| "worktree".to_string());
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' })
        .collect()
}
```

Notes for the implementer:
- `references_glob("refs/browsitory/undo/main-keep/*")`: libgit2's glob matches across `/`, which is what we want here (keep refs are flat anyway).
- Tree-entry names `0-before-index` etc. are unique per entry; duplicate tree oids under different names are fine.
- `peel_to_commit` on an `Object` of kind commit is cheap.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p repo-service undo_store::tests && cargo clippy -p repo-service --all-targets -- -D warnings`
Expected: PASS (gc test prints "skipping" if `git` is absent). Same dead-code allowance as Task 3 until Task 5 wires it.

- [ ] **Step 5: Commit**

```bash
git add crates/repo-service/src/worker/undo_store.rs crates/repo-service/src/worker/mod.rs
git commit -m "feat(repo-service): persist undo stacks and pin their objects"
```

---

### Task 5: `UndoRecorder` wired into the worker

**Files:**
- Modify: `crates/repo-service/src/worker/undo.rs` (recorder + `WorkerHandle` methods)
- Modify: `crates/repo-service/src/worker/mod.rs` (`Command` variants, loop wiring, `pub use`)
- Test: `crates/repo-service/src/worker/mod.rs` (`#[cfg(test)] mod tests`, existing helpers `init_repo`, `write_file`, `commit_all`, `local_and_bare_remote`)

**Interfaces:**
- Consumes: Task 3 `plan_for`, Task 4 `UndoStore`/`UndoStacks`/`UndoEntry`/`UNDO_LIMIT`, `git_core::undo::{capture, restore, UndoError}`.
- Produces (public, re-exported from `repo_service::worker`):
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)] pub struct UndoState { pub undo: Option<String>, pub redo: Option<String> }
  #[derive(Debug, Clone, PartialEq, Eq)] pub enum UndoOutcome { Done { label: String }, Nothing, RepositoryChanged { label: String }, OperationInProgress }
  impl WorkerHandle {
      pub fn get_undo_state(&self) -> Result<UndoState, String>;
      pub fn undo(&self) -> Result<UndoOutcome, String>;
      pub fn redo(&self) -> Result<UndoOutcome, String>;
  }
  ```
  New `Command` variants: `GetUndoState { reply: Sender<Result<UndoState, String>> }`, `Undo { reply: Sender<Result<UndoOutcome, String>> }`, `Redo { reply: Sender<Result<UndoOutcome, String>> }`.

- [ ] **Step 1: Write the failing tests** (add to `worker/mod.rs`'s test module; reuse its `init_repo`/`write_file`/`commit_all` helpers and the `Worker::spawn` path the other tests use)

```rust
    #[test]
    fn undo_and_redo_a_discard_round_trip_through_the_worker() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "one\n");
        commit_all(&repo, "initial");
        write_file(dir.path(), "a.txt", "precious\n");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();

        handle.discard_file("a.txt".into()).unwrap();
        assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "one\n");
        assert_eq!(
            handle.get_undo_state().unwrap(),
            UndoState { undo: Some("Discard changes to a.txt".into()), redo: None }
        );

        assert_eq!(
            handle.undo().unwrap(),
            UndoOutcome::Done { label: "Discard changes to a.txt".into() }
        );
        assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "precious\n");
        assert_eq!(
            handle.get_undo_state().unwrap(),
            UndoState { undo: None, redo: Some("Discard changes to a.txt".into()) }
        );

        assert_eq!(
            handle.redo().unwrap(),
            UndoOutcome::Done { label: "Discard changes to a.txt".into() }
        );
        assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "one\n");
    }

    #[test]
    fn failed_and_no_op_commands_record_nothing() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "one\n");
        commit_all(&repo, "initial");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        assert!(handle.delete_branch("missing".into(), false).is_err());
        handle.stage_file("a.txt".into()).unwrap(); // already clean: no change
        assert_eq!(handle.get_undo_state().unwrap(), UndoState { undo: None, redo: None });
        assert_eq!(handle.undo().unwrap(), UndoOutcome::Nothing);
    }

    #[test]
    fn a_new_command_clears_redo() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "one\n");
        commit_all(&repo, "initial");
        write_file(dir.path(), "a.txt", "two\n");
        write_file(dir.path(), "b.txt", "b\n");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.stage_file("a.txt".into()).unwrap();
        handle.undo().unwrap();
        handle.stage_file("b.txt".into()).unwrap();
        assert_eq!(
            handle.get_undo_state().unwrap(),
            UndoState { undo: Some("Stage b.txt".into()), redo: None }
        );
    }

    #[test]
    fn external_change_refuses_undo_and_keeps_the_entry() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "one\n");
        commit_all(&repo, "initial");
        write_file(dir.path(), "a.txt", "precious\n");
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.discard_file("a.txt".into()).unwrap();
        write_file(dir.path(), "a.txt", "typed later\n");
        assert_eq!(
            handle.undo().unwrap(),
            UndoOutcome::RepositoryChanged { label: "Discard changes to a.txt".into() }
        );
        assert_eq!(handle.get_undo_state().unwrap().undo, Some("Discard changes to a.txt".into()));
        assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "typed later\n");
    }

    #[test]
    fn stage_during_a_conflicted_merge_is_not_recorded_and_abort_drops_pending() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "base\n");
        commit_all(&repo, "base");
        let base_branch = repo.head().unwrap().shorthand().unwrap().to_string();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
        repo.set_head("refs/heads/feature").unwrap();
        write_file(dir.path(), "a.txt", "feature\n");
        commit_all(&repo, "feature change");
        repo.set_head(&format!("refs/heads/{base_branch}")).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
        write_file(dir.path(), "a.txt", "main\n");
        commit_all(&repo, "main change");

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        let outcome = handle.start_merge("feature".into()).unwrap();
        assert!(matches!(outcome, MergeOutcome::Conflicted { .. }));
        assert_eq!(handle.undo().unwrap(), UndoOutcome::OperationInProgress);
        handle.abort_merge().unwrap();
        assert_eq!(handle.get_undo_state().unwrap(), UndoState { undo: None, redo: None });
    }

    #[test]
    fn a_conflicted_merge_finished_by_commit_is_one_entry() {
        // Same fixture as above, then resolve and commit instead of aborting.
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "base\n");
        commit_all(&repo, "base");
        let base_branch = repo.head().unwrap().shorthand().unwrap().to_string();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.branch("feature", &head, false).unwrap();
        repo.set_head("refs/heads/feature").unwrap();
        write_file(dir.path(), "a.txt", "feature\n");
        commit_all(&repo, "feature change");
        repo.set_head(&format!("refs/heads/{base_branch}")).unwrap();
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).unwrap();
        write_file(dir.path(), "a.txt", "main\n");
        commit_all(&repo, "main change");
        let main_tip = repo.head().unwrap().target().unwrap();

        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        handle.start_merge("feature".into()).unwrap();
        handle.resolve_conflict("a.txt".into(), "resolved\n".into()).unwrap();
        handle.commit("Merge feature".into()).unwrap();
        assert_eq!(
            handle.get_undo_state().unwrap(),
            UndoState { undo: Some("Merge feature".into()), redo: None }
        );
        assert_eq!(handle.undo().unwrap(), UndoOutcome::Done { label: "Merge feature".into() });
        let reopened = git2::Repository::open(dir.path()).unwrap();
        assert_eq!(reopened.head().unwrap().target().unwrap(), main_tip);
        assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "main\n");
    }

    #[test]
    fn history_survives_a_worker_restart_and_is_capped() {
        let (dir, repo) = init_repo();
        write_file(dir.path(), "a.txt", "one\n");
        commit_all(&repo, "initial");
        {
            let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
            let handle = worker.handle();
            for n in 0..55 {
                handle.create_branch(format!("b{n}"), None).unwrap();
            }
        }
        let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
        let handle = worker.handle();
        assert_eq!(handle.get_undo_state().unwrap().undo, Some("Create branch b54".into()));
        let mut undone = 0;
        while let UndoOutcome::Done { .. } = handle.undo().unwrap() {
            undone += 1;
        }
        assert_eq!(undone, 50);
    }
```

Before writing these, check the exact `WorkerHandle` method names/signatures used (`discard_file`, `stage_file`, `delete_branch`, `create_branch`, `start_merge`, `resolve_conflict`, `abort_merge`, `commit`) in `worker/status.rs`, `worker/branch.rs`, `worker/merge.rs` and adjust argument shapes (e.g. `create_branch(name, start_point: Option<String>)`) to match; keep the assertions.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p repo-service undo_`
Expected: FAIL — `no method named get_undo_state`.

- [ ] **Step 3: Implement the recorder** (append to `worker/undo.rs`)

```rust
use std::sync::mpsc::Sender;

use git2::Repository;
use git_core::undo::{capture, restore, RepoSnapshot, UndoError};

use super::undo_store::{UndoEntry, UndoStacks, UndoStore, UNDO_LIMIT};
use super::WorkerHandle;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoState {
    pub undo: Option<String>,
    pub redo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoOutcome {
    Done { label: String },
    Nothing,
    RepositoryChanged { label: String },
    OperationInProgress,
}

pub(super) struct UndoRecorder {
    store: UndoStore,
    stacks: UndoStacks,
    active: Option<(UndoPlan, RepoSnapshot)>,
    /// An operation that stopped mid-way (conflicted merge/cherry-pick/revert, paused rebase):
    /// its label and pre-operation snapshot, recorded once nothing is in progress any more.
    pending: Option<(String, RepoSnapshot)>,
}

impl UndoRecorder {
    pub fn open(repo: &Repository) -> Self {
        let (store, stacks) = UndoStore::open(repo);
        Self { store, stacks, active: None, pending: None }
    }

    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn begin(&mut self, repo: &Repository, plan: UndoPlan, in_progress: bool) {
        if self.pending.is_some() || in_progress {
            return;
        }
        match capture(repo, plan.scope) {
            Ok(before) => self.active = Some((plan, before)),
            Err(error) => eprintln!("browsitory: not recording \"{}\" for undo: {error}", plan.label),
        }
    }

    pub fn finish(&mut self, repo: &Repository, in_progress: bool) {
        if let Some((plan, before)) = self.active.take() {
            if in_progress {
                self.pending = Some((plan.label, before));
                return;
            }
            self.record(repo, plan.label, plan.scope, before);
            return;
        }
        if !in_progress {
            if let Some((label, before)) = self.pending.take() {
                self.record(repo, label, SnapshotScope::Full, before);
            }
        }
    }

    fn record(&mut self, repo: &Repository, label: String, scope: SnapshotScope, before: RepoSnapshot) {
        let after = match capture(repo, scope) {
            Ok(after) => after,
            Err(error) => {
                eprintln!("browsitory: not recording \"{label}\" for undo: {error}");
                return;
            }
        };
        if after == before {
            return;
        }
        self.store.reload_if_changed(&mut self.stacks);
        self.stacks.undo.push(UndoEntry { label, before, after });
        self.stacks.redo.clear();
        let overflow = self.stacks.undo.len().saturating_sub(UNDO_LIMIT);
        self.stacks.undo.drain(..overflow);
        self.persist(repo);
    }

    pub fn state(&mut self) -> UndoState {
        self.store.reload_if_changed(&mut self.stacks);
        UndoState {
            undo: self.stacks.undo.last().map(|entry| entry.label.clone()),
            redo: self.stacks.redo.last().map(|entry| entry.label.clone()),
        }
    }

    pub fn undo(&mut self, repo: &Repository, in_progress: bool) -> Result<UndoOutcome, String> {
        self.step(repo, in_progress, Direction::Undo)
    }

    pub fn redo(&mut self, repo: &Repository, in_progress: bool) -> Result<UndoOutcome, String> {
        self.step(repo, in_progress, Direction::Redo)
    }

    fn step(&mut self, repo: &Repository, in_progress: bool, direction: Direction) -> Result<UndoOutcome, String> {
        self.store.reload_if_changed(&mut self.stacks);
        if in_progress || self.pending.is_some() {
            return Ok(UndoOutcome::OperationInProgress);
        }
        let (source, verb) = match direction {
            Direction::Undo => (&self.stacks.undo, "undo"),
            Direction::Redo => (&self.stacks.redo, "redo"),
        };
        let Some(entry) = source.last() else {
            return Ok(UndoOutcome::Nothing);
        };
        let (expected, target) = match direction {
            Direction::Undo => (&entry.after, &entry.before),
            Direction::Redo => (&entry.before, &entry.after),
        };
        let label = entry.label.clone();
        match restore(repo, expected, target, &format!("browsitory: {verb} {label}")) {
            Ok(()) => {}
            Err(UndoError::RepositoryChanged) => return Ok(UndoOutcome::RepositoryChanged { label }),
            Err(UndoError::OperationInProgress) => return Ok(UndoOutcome::OperationInProgress),
            Err(error) => return Err(error.to_string()),
        }
        let (from, to) = match direction {
            Direction::Undo => (&mut self.stacks.undo, &mut self.stacks.redo),
            Direction::Redo => (&mut self.stacks.redo, &mut self.stacks.undo),
        };
        let entry = from.pop().expect("entry checked above");
        to.push(entry);
        self.persist(repo);
        Ok(UndoOutcome::Done { label })
    }

    fn persist(&mut self, repo: &Repository) {
        if let Err(error) = self.store.save(repo, &self.stacks) {
            eprintln!("browsitory: could not save undo history: {error}");
        }
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Undo,
    Redo,
}

pub(super) fn get_state(recorder: &mut UndoRecorder, reply: Sender<Result<UndoState, String>>) {
    let _ = reply.send(Ok(recorder.state()));
}

impl WorkerHandle {
    pub fn get_undo_state(&self) -> Result<UndoState, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::GetUndoState { reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn undo(&self) -> Result<UndoOutcome, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::Undo { reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn redo(&self) -> Result<UndoOutcome, String> {
        let (reply, rx) = std::sync::mpsc::channel();
        self.tx
            .send(Command::Redo { reply })
            .map_err(|_| "worker thread stopped".to_string())?;
        rx.recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
}
```

- [ ] **Step 4: Wire into `worker/mod.rs`**

1. `Command` enum, after `CreatePullRequest { .. }`:
   ```rust
    GetUndoState {
        reply: Sender<Result<UndoState, String>>,
    },
    Undo {
        reply: Sender<Result<UndoOutcome, String>>,
    },
    Redo {
        reply: Sender<Result<UndoOutcome, String>>,
    },
   ```
2. Near the other `use` lines: `pub use undo::{UndoOutcome, UndoState};`
3. In `spawn_with`'s thread, after `let pull_request_service = ...;`:
   ```rust
            let mut undo_recorder = undo::UndoRecorder::open(&repo);
            // Browsitory's own rebase leaves `repo.state()` clean, so its paused state lives in
            // `rebase_state` instead.
            let in_progress = |repo: &git2::Repository, rebase_state: &Option<RebaseState>| {
                repo.state() != git2::RepositoryState::Clean || rebase_state.is_some()
            };
   ```
4. Wrap the dispatch:
   ```rust
            for command in rx {
                let plan = undo::plan_for(&command);
                let track = plan.is_some() || undo_recorder.has_pending();
                if let Some(plan) = plan {
                    undo_recorder.begin(&repo, plan, in_progress(&repo, &rebase_state));
                }
                match command {
                    // ...existing arms unchanged...
                    Command::GetUndoState { reply } => undo::get_state(&mut undo_recorder, reply),
                    Command::Undo { reply } => {
                        let _ = reply.send(undo_recorder.undo(&repo, in_progress(&repo, &rebase_state)));
                    }
                    Command::Redo { reply } => {
                        let _ = reply.send(undo_recorder.redo(&repo, in_progress(&repo, &rebase_state)));
                    }
                }
                if track {
                    undo_recorder.finish(&repo, in_progress(&repo, &rebase_state));
                }
            }
   ```
5. Remove the temporary `#[cfg_attr(not(test), allow(dead_code))]` from Task 3/4 items.

Ordering note: a handler replies before `finish` runs, but the worker processes commands one at a time, so a `GetUndoState` sent right after a mutation's reply is handled after `finish`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p repo-service && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: all PASS (including every pre-existing worker test).

- [ ] **Step 6: Commit**

```bash
git add crates/repo-service/src/worker
git commit -m "feat(repo-service): record undoable worker commands and undo/redo them"
```

---

### Task 6: Transport — Tauri commands, VSCode sidecar, `RepoClient`

**Files:**
- Create: `crates/tauri-app/src/commands/undo.rs`
- Modify: `crates/tauri-app/src/commands/mod.rs` (`mod undo;`, `pub use undo::{get_undo_state, redo_last, undo_last};`, DTOs + serialization tests), `crates/tauri-app/src/main.rs` (import + `generate_handler!`)
- Modify: `crates/vscode-sidecar/src/dispatch.rs` (methods + DTOs + match arms)
- Modify: `frontend/src/ipc/RepoClient.ts`, `tauriRepoClient.ts`, `vscodeRepoClient.ts`, `tauriRepoClient.test.ts`, `vscodeRepoClient.test.ts`
- Modify (mock clients gain the three methods): `frontend/src/App.test.tsx`, `state/useAppState.test.ts`, `components/RepoPicker.test.tsx`, `components/RebasePlanner.test.tsx`, `components/DiffPane.test.tsx`, `components/ConflictResolutionPane.test.tsx`

**Interfaces:**
- Consumes: `repo_service::worker::{UndoState, UndoOutcome}`, `WorkerHandle::{get_undo_state, undo, redo}`.
- Produces:
  - Tauri/sidecar methods `get_undo_state { repoPath } -> UndoStateDto`, `undo_last { repoPath } -> UndoOutcomeDto`, `redo_last { repoPath } -> UndoOutcomeDto`.
  - TS:
    ```ts
    export interface UndoState { undo: string | null; redo: string | null }
    export type UndoOutcome =
      | { kind: "Done"; label: string }
      | { kind: "Nothing" }
      | { kind: "RepositoryChanged"; label: string }
      | { kind: "OperationInProgress" };
    // RepoClient:
    getUndoState(repoPath: string): Promise<UndoState>;
    undo(repoPath: string): Promise<UndoOutcome>;
    redo(repoPath: string): Promise<UndoOutcome>;
    ```

- [ ] **Step 1: Write the failing tests**

`crates/tauri-app/src/commands/mod.rs` test module (add `UndoOutcomeDto, UndoStateDto` to its `use super::{...}`):

```rust
    #[test]
    fn undo_dtos_serialize_for_the_frontend() {
        let state = UndoStateDto::from(repo_service::worker::UndoState {
            undo: Some("Stage a.txt".into()),
            redo: None,
        });
        assert_eq!(
            serde_json::to_value(state).unwrap(),
            serde_json::json!({ "undo": "Stage a.txt", "redo": null })
        );
        let done = UndoOutcomeDto::from(repo_service::worker::UndoOutcome::Done { label: "Stage a.txt".into() });
        assert_eq!(serde_json::to_value(done).unwrap(), serde_json::json!({ "kind": "Done", "label": "Stage a.txt" }));
        let nothing = UndoOutcomeDto::from(repo_service::worker::UndoOutcome::Nothing);
        assert_eq!(serde_json::to_value(nothing).unwrap(), serde_json::json!({ "kind": "Nothing" }));
        let changed = UndoOutcomeDto::from(repo_service::worker::UndoOutcome::RepositoryChanged { label: "x".into() });
        assert_eq!(serde_json::to_value(changed).unwrap(), serde_json::json!({ "kind": "RepositoryChanged", "label": "x" }));
        let busy = UndoOutcomeDto::from(repo_service::worker::UndoOutcome::OperationInProgress);
        assert_eq!(serde_json::to_value(busy).unwrap(), serde_json::json!({ "kind": "OperationInProgress" }));
    }
```

`frontend/src/ipc/tauriRepoClient.test.ts` (follow the file's existing `invoke` mock pattern, e.g. the `resetToCommit` test near line 301):

```ts
  it("invokes the undo commands with the repo path", async () => {
    invokeMock.mockResolvedValueOnce({ undo: "Stage a.txt", redo: null });
    await expect(tauriRepoClient.getUndoState("/repo")).resolves.toEqual({ undo: "Stage a.txt", redo: null });
    expect(invokeMock).toHaveBeenLastCalledWith("get_undo_state", { repoPath: "/repo" });
    invokeMock.mockResolvedValueOnce({ kind: "Done", label: "Stage a.txt" });
    await tauriRepoClient.undo("/repo");
    expect(invokeMock).toHaveBeenLastCalledWith("undo_last", { repoPath: "/repo" });
    invokeMock.mockResolvedValueOnce({ kind: "Nothing" });
    await tauriRepoClient.redo("/repo");
    expect(invokeMock).toHaveBeenLastCalledWith("redo_last", { repoPath: "/repo" });
  });
```

`frontend/src/ipc/vscodeRepoClient.test.ts` (follow the `amendCommit`/`resetToCommit` pattern near line 1014 — posted message method names and params):

```ts
  it("posts the undo methods with the repo path", async () => {
    const p1 = vscodeRepoClient.getUndoState("/repo");
    const p2 = vscodeRepoClient.undo("/repo");
    const p3 = vscodeRepoClient.redo("/repo");
    // assert the three posted messages have methods "get_undo_state", "undo_last", "redo_last"
    // and params { repoPath: "/repo" }, then resolve them, using this file's existing helpers.
    await Promise.all([p1, p2, p3]);
  });
```

Copy the exact assertion helpers from the neighbouring `amendCommit` test (the posted-message capture and response dispatch differ per file version; the assertion content is: method names and `{ repoPath: "/repo" }` params).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tauri-app undo_dtos && (cd frontend && pnpm test -- --run src/ipc)`
Expected: FAIL (missing DTOs / methods).

- [ ] **Step 3: Implement**

`crates/tauri-app/src/commands/mod.rs` (DTOs next to `PickOutcomeDto`):

```rust
#[derive(Serialize)]
pub struct UndoStateDto {
    undo: Option<String>,
    redo: Option<String>,
}

impl From<repo_service::worker::UndoState> for UndoStateDto {
    fn from(state: repo_service::worker::UndoState) -> Self {
        Self { undo: state.undo, redo: state.redo }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind")]
pub enum UndoOutcomeDto {
    Done { label: String },
    Nothing,
    RepositoryChanged { label: String },
    OperationInProgress,
}

impl From<repo_service::worker::UndoOutcome> for UndoOutcomeDto {
    fn from(outcome: repo_service::worker::UndoOutcome) -> Self {
        use repo_service::worker::UndoOutcome;
        match outcome {
            UndoOutcome::Done { label } => Self::Done { label },
            UndoOutcome::Nothing => Self::Nothing,
            UndoOutcome::RepositoryChanged { label } => Self::RepositoryChanged { label },
            UndoOutcome::OperationInProgress => Self::OperationInProgress,
        }
    }
}
```

`crates/tauri-app/src/commands/undo.rs`:

```rust
use super::{worker_handle, AppState, UndoOutcomeDto, UndoStateDto};
use tauri::State;

#[tauri::command]
pub async fn get_undo_state(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<UndoStateDto, String> {
    Ok(UndoStateDto::from(worker_handle(&state, &repo_path)?.get_undo_state()?))
}

#[tauri::command]
pub async fn undo_last(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<UndoOutcomeDto, String> {
    Ok(UndoOutcomeDto::from(worker_handle(&state, &repo_path)?.undo()?))
}

#[tauri::command]
pub async fn redo_last(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<UndoOutcomeDto, String> {
    Ok(UndoOutcomeDto::from(worker_handle(&state, &repo_path)?.redo()?))
}
```

`crates/tauri-app/src/main.rs`: add `get_undo_state, redo_last, undo_last` to the `use commands::{...}` list (alphabetical) and to `tauri::generate_handler![...]`.

`crates/vscode-sidecar/src/dispatch.rs`: add the same two DTOs (sidecar keeps its own DTO copies, as with `PickOutcomeDto`), three match arms next to `"amend_commit"`:

```rust
        "get_undo_state" => get_undo_state(params, repos),
        "undo_last" => undo_last(params, repos),
        "redo_last" => redo_last(params, repos),
```

and handlers:

```rust
fn get_undo_state(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let state = worker_handle(repos, &params.repo_path)?.get_undo_state()?;
    serde_json::to_value(UndoStateDto::from(state)).map_err(|error| error.to_string())
}

fn undo_last(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let outcome = worker_handle(repos, &params.repo_path)?.undo()?;
    serde_json::to_value(UndoOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}

fn redo_last(params: Value, repos: &Repos) -> Result<Value, String> {
    let params: RepoPathParams = serde_json::from_value(params).map_err(|error| error.to_string())?;
    let outcome = worker_handle(repos, &params.repo_path)?.redo()?;
    serde_json::to_value(UndoOutcomeDto::from(outcome)).map_err(|error| error.to_string())
}
```

(`RepoPathParams` must deserialize `repoPath` — check its `#[serde(rename_all = "camelCase")]`; it already serves `close_repo`.)

`frontend/src/ipc/RepoClient.ts`: add the `UndoState`/`UndoOutcome` types (near `PickOutcome`) and, in the interface near `amendCommit`:

```ts
  /** Labels of the next undo/redo step for this repo; `null` when the stack is empty. */
  getUndoState(repoPath: string): Promise<UndoState>;
  /** Refusals (`Nothing`, `RepositoryChanged`, `OperationInProgress`) resolve; only real failures reject. */
  undo(repoPath: string): Promise<UndoOutcome>;
  redo(repoPath: string): Promise<UndoOutcome>;
```

`tauriRepoClient.ts`:

```ts
  getUndoState: (repoPath: string) => loggedInvoke<UndoState>("get_undo_state", { repoPath }),
  undo: (repoPath: string) => loggedInvoke<UndoOutcome>("undo_last", { repoPath }),
  redo: (repoPath: string) => loggedInvoke<UndoOutcome>("redo_last", { repoPath }),
```

`vscodeRepoClient.ts`:

```ts
  getUndoState: (repoPath: string) => call<UndoState>("get_undo_state", { repoPath }),
  undo: (repoPath: string) => call<UndoOutcome>("undo_last", { repoPath }),
  redo: (repoPath: string) => call<UndoOutcome>("redo_last", { repoPath }),
```

(Add `UndoState`, `UndoOutcome` to each file's `import type { ... } from "./RepoClient"`.)

Each mock client listed under **Files** gains:

```ts
    getUndoState: async () => ({ undo: null, redo: null }),
    undo: async () => ({ kind: "Nothing" as const }),
    redo: async () => ({ kind: "Nothing" as const }),
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && (cd frontend && pnpm test -- --run && pnpm lint && pnpm build) && (cd extension && pnpm test -- --run)`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/tauri-app crates/vscode-sidecar frontend/src
git commit -m "feat: expose undo/redo over Tauri, the VSCode sidecar and RepoClient"
```

---

### Task 7: Frontend state — `undoState`, `undo()`/`redo()`, toast actions

**Files:**
- Create: `frontend/src/state/useUndoActions.ts`, `frontend/src/state/useUndoActions.test.ts`
- Modify: `frontend/src/state/useAppState.ts` (`AppState.undoState`, initial value, `refresh()`, `refreshUndoState`, compose hook, return), `frontend/src/state/useToasts.ts`, `frontend/src/components/primitives/ToastRegion.tsx`, `ToastRegion.module.css`, `ToastRegion.test.tsx`

**Interfaces:**
- Consumes: `RepoClient.getUndoState/undo/redo`, `RunMutationWithOutcome` from `useMutationRunner`.
- Produces:
  ```ts
  // AppState
  undoState: UndoState; // initial { undo: null, redo: null }
  // UseAppStateResult
  undo(): Promise<UndoOutcome | null>;   // null when the call rejected (error banner already set)
  redo(): Promise<UndoOutcome | null>;
  refreshUndoState(): Promise<void>;
  // useToasts
  export interface ToastAction { label: string; run: () => void }
  export interface Toast { id: number; message: string; action?: ToastAction }
  push(message: string, action?: ToastAction): void;
  ```
- Error-banner messages (exact): see Global Constraints.

- [ ] **Step 1: Write the failing tests**

`frontend/src/state/useUndoActions.test.ts`:

```ts
import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient, UndoOutcome } from "../ipc/RepoClient";
import { useUndoActions } from "./useUndoActions";
import type { AppState } from "./useAppState";

function setup(outcome: UndoOutcome | Error) {
  const client = {
    undo: vi.fn(async () => {
      if (outcome instanceof Error) throw outcome;
      return outcome;
    }),
    redo: vi.fn(async () => {
      if (outcome instanceof Error) throw outcome;
      return outcome;
    }),
  } as unknown as RepoClient;
  let state = { error: null, undoState: { undo: "Stage a.txt", redo: "Commit “x”" } } as unknown as AppState;
  const setState = vi.fn((updater: (prev: AppState) => AppState) => {
    state = updater(state);
  });
  const runMutationWithOutcome = vi.fn(async (mutate: () => Promise<void>) => {
    try {
      await mutate();
      return true;
    } catch (err) {
      state = { ...state, error: String(err) };
      return false;
    }
  });
  const { result } = renderHook(() => useUndoActions(client, "/repo", runMutationWithOutcome, setState));
  return { result, client, getState: () => state };
}

describe("useUndoActions", () => {
  it("returns Done and leaves no error", async () => {
    const { result, client, getState } = setup({ kind: "Done", label: "Stage a.txt" });
    let outcome: UndoOutcome | null = null;
    await act(async () => {
      outcome = await result.current.undo();
    });
    expect(client.undo).toHaveBeenCalledWith("/repo");
    expect(outcome).toEqual({ kind: "Done", label: "Stage a.txt" });
    expect(getState().error).toBeNull();
  });

  it("shows the repository-changed message for undo and redo", async () => {
    const { result, getState } = setup({ kind: "RepositoryChanged", label: "Stage a.txt" });
    await act(async () => {
      await result.current.undo();
    });
    expect(getState().error).toBe(
      "Can't undo Stage a.txt: the repository changed since. Use the reflog to recover.",
    );
    await act(async () => {
      await result.current.redo();
    });
    expect(getState().error).toBe(
      "Can't redo Stage a.txt: the repository changed since. Use the reflog to recover.",
    );
  });

  it("asks to finish the merge or rebase first", async () => {
    const { result, getState } = setup({ kind: "OperationInProgress" });
    await act(async () => {
      await result.current.undo();
    });
    expect(getState().error).toBe("Finish or abort the merge or rebase first.");
  });

  it("stays silent when there is nothing to undo", async () => {
    const { result, getState } = setup({ kind: "Nothing" });
    await act(async () => {
      await result.current.undo();
    });
    expect(getState().error).toBeNull();
  });

  it("prefixes real failures and resolves null", async () => {
    const { result, getState } = setup(new Error("io error: permission denied"));
    let outcome: UndoOutcome | null = { kind: "Nothing" };
    await act(async () => {
      outcome = await result.current.undo();
    });
    expect(outcome).toBeNull();
    expect(getState().error).toBe("Undo failed: io error: permission denied");
  });
});
```

`frontend/src/components/primitives/ToastRegion.test.tsx` (add):

```tsx
  it("renders an action button that runs the action and dismisses the toast", async () => {
    const run = vi.fn();
    const onDismiss = vi.fn();
    render(
      <ToastRegion toasts={[{ id: 1, message: "Undid Stage a.txt", action: { label: "Redo", run } }]} onDismiss={onDismiss} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Redo" }));
    expect(run).toHaveBeenCalledOnce();
    expect(onDismiss).toHaveBeenCalledWith(1);
  });
```

`frontend/src/state/useAppState.test.ts` (add; follow the file's existing client-builder, e.g. `transferClient({...})`, overriding `getUndoState`):

```ts
  it("loads the undo state on refresh", async () => {
    const client = transferClient({ getUndoState: async () => ({ undo: "Stage a.txt", redo: null }) });
    const { result } = renderHook(() => useAppState(client, "/repo"));
    await act(async () => {
      await result.current.refresh();
    });
    expect(result.current.state.undoState).toEqual({ undo: "Stage a.txt", redo: null });
  });
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run src/state/useUndoActions.test.ts src/components/primitives/ToastRegion.test.tsx src/state/useAppState.test.ts`
Expected: FAIL.

- [ ] **Step 3: Implement**

`frontend/src/state/useUndoActions.ts`:

```ts
import { useCallback } from "react";
import type { RepoClient, UndoOutcome } from "../ipc/RepoClient";
import type { AppState } from "./useAppState";
import type { RunMutationWithOutcome } from "./useMutationRunner";

export interface UndoActions {
  undo(): Promise<UndoOutcome | null>;
  redo(): Promise<UndoOutcome | null>;
}

// Refusals arrive as resolved outcomes (they cross the VSCode transport as plain data); they are
// turned into the shared error banner here. Real failures reject and are prefixed after
// `runMutationWithOutcome` has stored them.
function refusalMessage(verb: "undo" | "redo", outcome: UndoOutcome): string | null {
  switch (outcome.kind) {
    case "RepositoryChanged":
      return `Can't ${verb} ${outcome.label}: the repository changed since. Use the reflog to recover.`;
    case "OperationInProgress":
      return "Finish or abort the merge or rebase first.";
    default:
      return null;
  }
}

export function useUndoActions(
  client: RepoClient,
  repoPath: string,
  runMutationWithOutcome: RunMutationWithOutcome,
  setState: (updater: (prev: AppState) => AppState) => void,
): UndoActions {
  const step = useCallback(
    async (verb: "undo" | "redo"): Promise<UndoOutcome | null> => {
      let outcome: UndoOutcome | null = null;
      const ok = await runMutationWithOutcome(async () => {
        outcome = verb === "undo" ? await client.undo(repoPath) : await client.redo(repoPath);
      });
      if (!ok) {
        const prefix = verb === "undo" ? "Undo failed: " : "Redo failed: ";
        setState((prev) => ({ ...prev, error: prev.error === null ? null : `${prefix}${prev.error}` }));
        return null;
      }
      const message = outcome === null ? null : refusalMessage(verb, outcome);
      if (message !== null) setState((prev) => ({ ...prev, error: message }));
      return outcome;
    },
    [client, repoPath, runMutationWithOutcome, setState],
  );
  const undo = useCallback(() => step("undo"), [step]);
  const redo = useCallback(() => step("redo"), [step]);
  return { undo, redo };
}
```

Note: `runMutationWithOutcome` maps worker-death errors to `REPO_CONNECTION_LOST_MESSAGE`; prefixing that with "Undo failed: " is acceptable and keeps one code path.

`useAppState.ts`:
- `AppState`: add `undoState: UndoState;` (import type from `../ipc/RepoClient`); initial state `undoState: { undo: null, redo: null },`.
- `refresh()`: add `client.getUndoState(repoPath)` as the last element of the big `Promise.all` and `undoState` to the destructuring and the `setState` object.
- New:
  ```ts
  // Cheap focus-time re-check: another process (CLI, the other frontend) may have changed what
  // undo would do. Errors are ignored — the next full refresh reports them.
  const refreshUndoState = useCallback(async () => {
    try {
      const undoState = await client.getUndoState(repoPath);
      setState((prev) => ({ ...prev, undoState }));
    } catch {
      // ignored, see above
    }
  }, [client, repoPath]);
  ```
- Compose: `const { undo, redo } = useUndoActions(client, repoPath, runMutationWithOutcome, setState);` next to `useHistoryActions`; add `undo`, `redo`, `refreshUndoState` to `UseAppStateResult` and the returned object.

`useToasts.ts`:

```ts
export interface ToastAction {
  label: string;
  run: () => void;
}

export interface Toast {
  id: number;
  message: string;
  action?: ToastAction;
}
// push:
  const push = useCallback(
    (message: string, action?: ToastAction) => {
      const id = nextId.current++;
      setToasts((prev) => [...prev.slice(-2), { id, message, action }]);
      timers.current.set(id, setTimeout(() => dismiss(id), TOAST_DURATION_MS));
    },
    [dismiss],
  );
```

`ToastRegion.tsx` — inside each toast, between the message and the dismiss button:

```tsx
          {toast.action !== undefined && (
            <button
              type="button"
              className={styles.action}
              onClick={() => {
                toast.action?.run();
                onDismiss(toast.id);
              }}
            >
              {toast.action.label}
            </button>
          )}
```

`ToastRegion.module.css`:

```css
.toast button.action {
  font-size: var(--text-sm);
  font-weight: 600;
  text-decoration: underline;
}
```

`App.tsx` — widen `RepoWorkspace`'s prop type to `pushToast: (message: string, action?: ToastAction) => void;` (import `ToastAction` type from `./state/useToasts`); `pushAppToast` already matches.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): undo state, undo/redo actions and toast actions"
```

---

### Task 8: Frontend UI — toolbar buttons, shortcuts, palette, focus refresh

**Files:**
- Create: `frontend/src/components/UndoRedoButtons.tsx`, `UndoRedoButtons.test.tsx`
- Modify: `frontend/src/components/SyncBar.tsx` (+ test) — `leading?: ReactNode` slot
- Modify: `frontend/src/App.tsx` (handlers, shortcuts, focus listener, pass to SyncBar and palette), `frontend/src/App.test.tsx`
- Modify: `frontend/src/lib/commands.ts` (+ `commands.test.ts`), `frontend/src/lib/shortcuts.ts`

**Interfaces:**
- Consumes: `appState.state.undoState`, `appState.undo/redo/refreshUndoState`, `pushToast(message, action)`.
- Produces:
  ```ts
  export function UndoRedoButtons(props: { undoLabel: string | null; redoLabel: string | null; disabled: boolean; disabledReason: string | null; onUndo: () => void; onRedo: () => void }): JSX.Element
  // lib/commands.ts
  export interface UndoCommands { undoLabel: string | null; redoLabel: string | null; onUndo: () => void; onRedo: () => void }
  // buildCommands(..., appCommands?: AppCommands, undoCommands?: UndoCommands)
  ```

- [ ] **Step 1: Write the failing tests**

`frontend/src/components/UndoRedoButtons.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { UndoRedoButtons } from "./UndoRedoButtons";

describe("UndoRedoButtons", () => {
  it("names the next steps in tooltips and runs them", async () => {
    const onUndo = vi.fn();
    const onRedo = vi.fn();
    render(
      <UndoRedoButtons undoLabel="Stage a.txt" redoLabel="Commit “x”" disabled={false} disabledReason={null} onUndo={onUndo} onRedo={onRedo} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Undo Stage a.txt" }));
    await userEvent.click(screen.getByRole("button", { name: "Redo Commit “x”" }));
    expect(onUndo).toHaveBeenCalledOnce();
    expect(onRedo).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: "Undo Stage a.txt" })).toHaveAttribute("title", "Undo Stage a.txt");
  });

  it("is disabled with 'Nothing to …' when a stack is empty", () => {
    render(<UndoRedoButtons undoLabel={null} redoLabel={null} disabled={false} disabledReason={null} onUndo={vi.fn()} onRedo={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Nothing to undo" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Nothing to redo" })).toBeDisabled();
  });

  it("is disabled with the busy reason while an operation runs", () => {
    render(
      <UndoRedoButtons undoLabel="Stage a.txt" redoLabel={null} disabled disabledReason="Another action is in progress." onUndo={vi.fn()} onRedo={vi.fn()} />,
    );
    const undo = screen.getByRole("button", { name: "Undo Stage a.txt" });
    expect(undo).toBeDisabled();
    expect(undo).toHaveAttribute("title", "Another action is in progress.");
  });
});
```

`frontend/src/lib/commands.test.ts` (add; reuse the file's existing `appState` fixture builder):

```ts
  it("lists undo and redo with their labels when available", () => {
    const onUndo = vi.fn();
    const commands = buildCommands(makeAppState(), undefined, [], undefined, undefined, undefined, undefined, undefined, undefined, {
      undoLabel: "Stage a.txt",
      redoLabel: null,
      onUndo,
      onRedo: vi.fn(),
    });
    const undo = commands.find((c) => c.id === "undo");
    expect(undo?.label).toBe("Undo Stage a.txt");
    expect(commands.find((c) => c.id === "redo")).toBeUndefined();
    undo?.run();
    expect(onUndo).toHaveBeenCalledOnce();
  });

  it("omits undo while a repository operation is running", () => {
    const commands = buildCommands(makeAppState({ pending: true }), undefined, [], undefined, undefined, undefined, undefined, undefined, undefined, {
      undoLabel: "Stage a.txt",
      redoLabel: "x",
      onUndo: vi.fn(),
      onRedo: vi.fn(),
    });
    expect(commands.find((c) => c.id === "undo")).toBeUndefined();
  });
```

(Use the real fixture helper name from `commands.test.ts`; `makeAppState` stands for it.)

`frontend/src/App.test.tsx` (add; follow the file's existing render helper that mounts the app with a mock client and an open repo):

```tsx
  it("undoes with Mod+Z and offers Redo in the toast", async () => {
    const undo = vi.fn(async () => ({ kind: "Done" as const, label: "Stage a.txt" }));
    const redo = vi.fn(async () => ({ kind: "Done" as const, label: "Stage a.txt" }));
    renderWithOpenRepo({ getUndoState: async () => ({ undo: "Stage a.txt", redo: null }), undo, redo });
    await screen.findByRole("button", { name: "Undo Stage a.txt" });
    await userEvent.keyboard("{Control>}z{/Control}");
    expect(undo).toHaveBeenCalledWith("/repo");
    await userEvent.click(await screen.findByRole("button", { name: "Redo" }));
    expect(redo).toHaveBeenCalledWith("/repo");
    expect(await screen.findByText("Redid Stage a.txt")).toBeInTheDocument();
  });

  it("redoes with Mod+Shift+Z and Ctrl+Y", async () => {
    const redo = vi.fn(async () => ({ kind: "Nothing" as const }));
    renderWithOpenRepo({ getUndoState: async () => ({ undo: null, redo: "Stage a.txt" }), redo });
    await screen.findByRole("button", { name: "Redo Stage a.txt" });
    await userEvent.keyboard("{Control>}{Shift>}Z{/Shift}{/Control}");
    await userEvent.keyboard("{Control>}y{/Control}");
    expect(redo).toHaveBeenCalledTimes(2);
  });

  it("ignores Mod+Z while typing in the commit message", async () => {
    const undo = vi.fn(async () => ({ kind: "Nothing" as const }));
    renderWithOpenRepo({ getUndoState: async () => ({ undo: "Stage a.txt", redo: null }), undo });
    const message = await screen.findByRole("textbox", { name: /commit message/i });
    await userEvent.click(message);
    await userEvent.keyboard("{Control>}z{/Control}");
    expect(undo).not.toHaveBeenCalled();
  });

  it("re-reads the undo state when the window regains focus", async () => {
    const getUndoState = vi.fn(async () => ({ undo: null, redo: null }));
    renderWithOpenRepo({ getUndoState });
    await screen.findByRole("button", { name: "Nothing to undo" });
    const calls = getUndoState.mock.calls.length;
    window.dispatchEvent(new Event("focus"));
    await waitFor(() => expect(getUndoState.mock.calls.length).toBe(calls + 1));
  });
```

(`renderWithOpenRepo` stands for the existing helper in `App.test.tsx` that builds the mock client with overrides and opens `/repo`; use its real name and repo path.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && pnpm test -- --run src/components/UndoRedoButtons.test.tsx src/lib/commands.test.ts src/App.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement**

`frontend/src/components/UndoRedoButtons.tsx`:

```tsx
import { Toolbar } from "./primitives/Toolbar";

/** Undo/redo the last recorded Git operation; tooltips name the step (Sublime Merge-style). */
export function UndoRedoButtons({
  undoLabel,
  redoLabel,
  disabled,
  disabledReason,
  onUndo,
  onRedo,
}: {
  undoLabel: string | null;
  redoLabel: string | null;
  disabled: boolean;
  disabledReason: string | null;
  onUndo: () => void;
  onRedo: () => void;
}) {
  const undoName = undoLabel === null ? "Nothing to undo" : `Undo ${undoLabel}`;
  const redoName = redoLabel === null ? "Nothing to redo" : `Redo ${redoLabel}`;
  const busyTitle = disabled ? (disabledReason ?? undefined) : undefined;
  return (
    <Toolbar aria-label="Undo">
      <button type="button" aria-label={undoName} title={busyTitle ?? undoName} disabled={disabled || undoLabel === null} onClick={onUndo}>
        ↶
      </button>
      <button type="button" aria-label={redoName} title={busyTitle ?? redoName} disabled={disabled || redoLabel === null} onClick={onRedo}>
        ↷
      </button>
    </Toolbar>
  );
}
```

`SyncBar.tsx`: add prop `leading?: ReactNode` (import `type ReactNode` from "react") and render `{leading}` as the first child of `<div className={styles.bar}>`. Add a `SyncBar.test.tsx` case: renders the leading node.

`lib/commands.ts`:

```ts
export interface UndoCommands {
  undoLabel: string | null;
  redoLabel: string | null;
  onUndo: () => void;
  onRedo: () => void;
}
```

Add `undoCommands?: UndoCommands` as the last `buildCommands` parameter and, right after the `repositoryOperationDisabled` const:

```ts
  if (!repositoryOperationDisabled && undoCommands !== undefined) {
    if (undoCommands.undoLabel !== null) {
      commands.push({
        id: "undo",
        label: `Undo ${undoCommands.undoLabel}`,
        keywords: ["undo", "revert", "back"],
        run: undoCommands.onUndo,
      });
    }
    if (undoCommands.redoLabel !== null) {
      commands.push({
        id: "redo",
        label: `Redo ${undoCommands.redoLabel}`,
        keywords: ["redo", "again"],
        run: undoCommands.onRedo,
      });
    }
  }
```

Also map `"undo"`/`"redo"` in `commandGroup` to the same group as other repository actions (check the function's existing groups; use the one `pull` falls into).

`lib/shortcuts.ts` — Global group, after the palette entry:

```ts
      { keys: "Ctrl/Cmd+Z", description: "Undo the last Git operation" },
      { keys: "Ctrl/Cmd+Shift+Z / Ctrl+Y", description: "Redo" },
```

`App.tsx` (inside `RepoWorkspace`):

```tsx
  const { undo: undoStep, redo: redoStep, refreshUndoState } = appState;
  const redoRef = useRef<() => void>(() => {});
  const handleUndo = useCallback(() => {
    void undoStep().then((outcome) => {
      if (outcome?.kind === "Done") pushToast(`Undid ${outcome.label}`, { label: "Redo", run: () => redoRef.current() });
    });
  }, [undoStep, pushToast]);
  const handleRedo = useCallback(() => {
    void redoStep().then((outcome) => {
      if (outcome?.kind === "Done") pushToast(`Redid ${outcome.label}`, { label: "Undo", run: handleUndo });
    });
  }, [redoStep, pushToast, handleUndo]);
  redoRef.current = handleRedo;

  useEffect(() => {
    if (!active) return;
    const onFocus = () => void refreshUndoState();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [active, refreshUndoState]);
```

In the existing `handleKeyDown` (after the `?` branch, before the Ctrl/Cmd+W branch):

```tsx
      const mod = event.metaKey || event.ctrlKey;
      const key = event.key.toLowerCase();
      const isUndo = mod && !event.altKey && key === "z" && !event.shiftKey;
      const isRedo = mod && !event.altKey && ((key === "z" && event.shiftKey) || (event.ctrlKey && !event.metaKey && key === "y"));
      if (isUndo || isRedo) {
        const target = event.target as HTMLElement | null;
        const tag = target?.tagName;
        // Text fields keep their native undo (commit message, branch-name inputs, …).
        if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT" || target?.isContentEditable) return;
        if (appState.state.pending || appState.state.transfer !== null) return;
        event.preventDefault();
        if (isUndo) handleUndo();
        else handleRedo();
        return;
      }
```

and add `handleUndo`, `handleRedo`, `appState.state.pending`, `appState.state.transfer` to that effect's dependency list (destructure `pending`/`transfer` into consts first to keep the deps lint-clean).

Render: pass to `SyncBar`:

```tsx
                leading={
                  <UndoRedoButtons
                    undoLabel={appState.state.undoState.undo}
                    redoLabel={appState.state.undoState.redo}
                    disabled={appState.state.pending || appState.state.transfer !== null}
                    disabledReason={operationDisabledReason}
                    onUndo={handleUndo}
                    onRedo={handleRedo}
                  />
                }
```

and to `buildCommands(...)` as the new last argument:

```tsx
              {
                undoLabel: appState.state.undoState.undo,
                redoLabel: appState.state.undoState.redo,
                onUndo: handleUndo,
                onRedo: handleRedo,
              },
```

Buttons are only disabled for `pending`/`transfer` (not merge/rebase) so that clicking during a paused merge explains itself via the backend's `OperationInProgress` banner; the palette omits them per its existing `repositoryOperationDisabled` rule.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && pnpm test -- --run && pnpm lint && pnpm build`
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src
git commit -m "feat(frontend): undo/redo buttons, shortcuts and palette commands"
```

---

### Task 9: E2E flows and docs

**Files:**
- Create: `e2e/specs/undo.spec.ts`, `extension/e2e/src/specs/undo.spec.ts`
- Modify: `CHANGELOG.md`, `docs/ARCHITECTURE.md`, `docs/USER_GUIDE.md`

**Interfaces:**
- Consumes: UI from Task 8 (buttons named "Undo ⟨label⟩", toast "Undid ⟨label⟩" with "Redo").

- [ ] **Step 1: Write the Tauri E2E spec**

`e2e/specs/undo.spec.ts`:

```ts
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const FILE = "undo-e2e.txt";
const MOD = process.platform === "darwin" ? "Meta" : "Control";

function fileContents(): string | null {
  try {
    return fs.readFileSync(path.join(E2E_REPO_PATH, FILE), "utf8");
  } catch {
    return null;
  }
}

describe("Browsitory undo and redo", () => {
  before(() => {
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "committed\n");
    execFileSync("git", ["add", FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: undo fixture"], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    fs.writeFileSync(path.join(E2E_REPO_PATH, FILE), "precious edit\n");
  });

  it("restores a discarded file with Mod+Z and discards it again with Mod+Shift+Z", async () => {
    // Discard via the palette command so the flow doesn't depend on diff-pane layout.
    await browser.keys([MOD, "k"]);
    const palette = await $("aria/Command palette");
    await palette.waitForExist({ timeout: 10000 });
    await browser.keys(`Discard changes to ${FILE}`.split(""));
    await browser.keys("Enter");
    const confirm = await $("button=Discard");
    await confirm.waitForClickable({ timeout: 10000 });
    await confirm.click();
    await browser.waitUntil(() => fileContents() === "committed\n", { timeout: 10000, timeoutMsg: "discard did not land" });

    await $(`aria/Undo Discard changes to ${FILE}`).waitForExist({ timeout: 10000 });
    await (await $("body")).click();
    await browser.keys([MOD, "z"]);
    await browser.waitUntil(() => fileContents() === "precious edit\n", { timeout: 10000, timeoutMsg: "undo did not restore the file" });
    await expect($(`*=Undid Discard changes to ${FILE}`)).toBeExisting();

    await browser.keys([MOD, "Shift", "z"]);
    await browser.waitUntil(() => fileContents() === "committed\n", { timeout: 10000, timeoutMsg: "redo did not discard again" });
  });
});
```

Before running, check how `e2e/specs/command-palette.spec.ts` opens the palette and selects a command, and what the discard palette command/confirm dialog are actually labelled (`lib/commands.ts`, `DiscardRequest` dialog in `App.tsx`); align the selectors with those — keep the three file-content assertions as the test's substance.

- [ ] **Step 2: Write the VSCode E2E spec**

`extension/e2e/src/specs/undo.spec.ts` (toolbar button, not the shortcut — VSCode may claim Ctrl/Cmd+Z, see spec):

```ts
import { execFileSync } from "node:child_process";
import * as vscode from "vscode";
import { connectToWebview } from "../support/webviewPage";

describe("Browsitory VSCode extension undo", () => {
  it("undoes a commit from the toolbar, leaving the change staged", async function () {
    this.timeout(120000);
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) throw new Error("no workspace folder open");
    const cwd = workspaceFolder.uri.fsPath;
    const git = (...args: string[]) => execFileSync("git", args, { cwd }).toString().trim();
    const tipBefore = git("rev-parse", "HEAD");

    await vscode.workspace.fs.writeFile(vscode.Uri.joinPath(workspaceFolder.uri, "vscode-undo.txt"), Buffer.from("undo me\n"));

    await vscode.commands.executeCommand("browsitory.open");
    const cdpPort = process.env["BROWSITORY_VSCODE_E2E_CDP_PORT"];
    if (!cdpPort) throw new Error("BROWSITORY_VSCODE_E2E_CDP_PORT not set");
    const webview = await connectToWebview(`http://127.0.0.1:${cdpPort}`);
    try {
      // Stage + commit through the app so the commit is recorded by the worker.
      await webview.waitForText("li", "vscode-undo.txt", 15000);
      await webview.clickByText("button", "Stage all");
      await webview.typeInto("textarea", "vscode-e2e: undo me");
      await webview.clickByText("button", "Commit");
      const committed = Date.now() + 15000;
      while (git("log", "-1", "--format=%s") !== "vscode-e2e: undo me") {
        if (Date.now() > committed) throw new Error("commit did not land");
        await new Promise((resolve) => setTimeout(resolve, 250));
      }

      await webview.clickByAriaLabel("Undo Commit “vscode-e2e: undo me”");
      const undone = Date.now() + 15000;
      while (git("rev-parse", "HEAD") !== tipBefore) {
        if (Date.now() > undone) throw new Error("undo did not move HEAD back");
        await new Promise((resolve) => setTimeout(resolve, 250));
      }
      if (!git("diff", "--cached", "--name-only").split("\n").includes("vscode-undo.txt")) {
        throw new Error("undone commit's change is not staged");
      }
    } finally {
      await webview.close();
      await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    }
  });
});
```

Check `extension/e2e/src/support/webviewPage.ts` for the available helpers (`waitForText`, `clickByText`, `contextMenuByText` exist; `typeInto`/`clickByAriaLabel` may not). If a helper is missing, add it there in this task, implemented the same way as `clickByText` (CDP `Runtime.evaluate` on the webview frame), and use the real "Stage all"/"Commit" button texts from `CommitBox.tsx`/`DiffPane.tsx`.

- [ ] **Step 3: Run the E2E suites**

Run (from repo root, per `CLAUDE.md` order):
```bash
cd frontend && VITE_E2E_REPO_PATH=/tmp/browsitory-e2e-repo pnpm build && cd ..
cargo build --workspace --features tauri-app/custom-protocol,tauri-app/forge-fixture-override
cd e2e && pnpm test -- --spec specs/undo.spec.ts && cd ..
cargo build --workspace
cd frontend && pnpm run generate:release-notes && VITE_E2E_REPO_PATH=/tmp/browsitory-vscode-e2e-repo pnpm exec vite build --config vite.vscode.config.ts && cd ..
cd extension && pnpm run compile && cd e2e && pnpm test
```
Expected: PASS (use `xvfb-run` on a headless box).

- [ ] **Step 4: Docs**

`CHANGELOG.md`, under `## [Unreleased]`, as the first section:

```markdown
### Undo and redo

- **Ctrl/Cmd+Z** undoes the last Git operation in the active repository and **Ctrl/Cmd+Shift+Z**
  (or Ctrl+Y) redoes it — also from the undo/redo buttons next to Fetch/Pull/Push, the command
  palette, and the toast's Undo/Redo button. Covers staging, commits, amend, reset, branch and
  tag changes, checkout, cherry-pick, revert, merge, rebase, pull and every discard (discarded
  changes are now recoverable). Up to 50 steps, kept across restarts.
- Undo refuses, instead of overwriting, when the repository changed since the operation; the
  reflog stays available for those cases.
```

`docs/ARCHITECTURE.md`, new section "Undo stack" (after the worker/repo-service section):

```markdown
## Undo stack

- **Snapshots** (`git-core::undo`): HEAD, `refs/heads/*` + `refs/tags/*`, the index as a tree,
  and — for commands that touch the working tree — a "worktree tree" (HEAD tree with every
  modified/deleted/untracked file overlaid, raw bytes, no filters). Undo restores `before` over
  `after`, redo the reverse, touching only components that differ and only after verifying each
  still equals the expected side (strict: any drift refuses).
- **Recording** (`repo-service::worker::undo`): the worker snapshots before/after each
  recordable command; failed/no-op commands record nothing; operations that stop on conflicts
  stay pending until completed (one entry) or aborted (none).
- **Storage**: `<gitdir>/browsitory/undo.json` per worktree (50 entries); objects pinned by
  `refs/browsitory/undo/<worktree>` (an anchor commit, visible in `git log --all`) and
  `…-keep/<n>` refs for tag objects.
- **Known cost**: commands that touch the working tree hash every dirty/untracked file into the
  object database, so very large untracked files make those commands slower and grow `.git`
  until gc after the entry ages out.
```

`docs/USER_GUIDE.md`, new section "Undo and redo" (follow the guide's heading level and tone; content = the CHANGELOG bullets plus: "Typing in a text field keeps that field's own undo." and "In the VSCode extension, use the toolbar buttons or the command palette if Ctrl/Cmd+Z is taken by VSCode." and "Tools that show all refs (`git log --all`, gitk) show a 'browsitory undo anchor' commit; it keeps undoable objects from being garbage-collected.").

- [ ] **Step 5: Full verification**

Run:
```bash
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd frontend && pnpm lint && pnpm test -- --run && pnpm build && cd ..
cd extension && pnpm test -- --run && pnpm run compile && cd ..
```
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add e2e/specs/undo.spec.ts extension/e2e/src CHANGELOG.md docs/ARCHITECTURE.md docs/USER_GUIDE.md
git commit -m "test(e2e): undo/redo flows; docs: undo stack"
```
