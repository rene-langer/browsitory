# Hunk-based staging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user stage, unstage, or discard a single diff hunk from the diff view, instead of only whole files.

**Architecture:** Three new `git-core::stage` functions (`stage_hunk`/`unstage_hunk`/`discard_hunk`) apply a single hunk — identified by its `(old_start, new_start)` header pair — via `git2::Repository::apply` with a `hunk_callback` filter. These reach the frontend through the existing four-layer IPC stack (`Command` enum → `Worker` match arm → `WorkerHandle` method → `#[tauri::command]`), then through `RepoClient` → `useAppState`'s `runMutation` pattern → `DiffPane` → `DiffView`, mirroring `stageFile`/`unstageFile` and `applyStash`/`dropStash` exactly.

**Tech Stack:** Rust (git2 0.21.0, Tauri), TypeScript/React (Vitest, Testing Library), WebdriverIO e2e.

**Spec:** `docs/superpowers/specs/2026-08-20-hunk-staging-design.md` (sections 1-5; section 6, the sitewide Toolbar right-align, is a separate plan)

## Global Constraints

- Hunk identity crossing every layer is the pair `(oldStart: number, newStart: number)` (camelCase on the wire/frontend, `old_start`/`new_start` snake_case in Rust) — no new id field on `DiffHunk`.
- A hunk action that can't find a matching hunk (stale diff — file changed since fetch) must return an explicit error, never silently no-op. `git2`'s `hunk_callback` returning `false` for every hunk makes `apply()` succeed with zero changes, so every one of the three functions below tracks whether it matched anything and returns a new `StageError::HunkNotFound` if not.
- Hunk action buttons get **no** pending/disabled gate — the file-level Stage/Unstage buttons they sit next to have none today, so hunk buttons don't introduce a new pattern. The Discard action still requires a two-click confirm (`BranchSwitcher`'s force-delete pattern), because it's destructive, not because of pending-state.
- `CommitDiffPane` (historical commit diffs) never renders hunk action buttons — `DiffView`'s three callback props are optional and it simply doesn't pass them.

---

## Task 1: `git_core::stage::stage_hunk`

**Files:**
- Modify: `crates/git-core/src/stage.rs`
- Test: `crates/git-core/tests/stage_commit.rs`

**Interfaces:**
- Produces: `pub fn stage_hunk(repo: &git2::Repository, path: &str, old_start: u32, new_start: u32) -> Result<(), StageError>`
- Produces: new variant `StageError::HunkNotFound`

- [ ] **Step 1: Write the failing test**

Add to `crates/git-core/tests/stage_commit.rs` (extend the existing `use` line to also import `stage_hunk`):

```rust
use git_core::stage::{stage_file, stage_hunk, unstage_file};
```

```rust
#[test]
fn stage_hunk_stages_only_the_targeted_hunk_leaving_the_other_unstaged() {
    let (dir, mut repo) = init_repo();
    let original: String = (1..=15).map(|n| format!("line {n}\n")).collect();
    write_file(dir.path(), "tracked.txt", &original);
    stage_file(&repo, "tracked.txt").unwrap();
    commit(&mut repo, "initial commit").unwrap();

    let mut lines: Vec<String> = (1..=15).map(|n| format!("line {n}")).collect();
    lines[1] = "line 2 changed".to_string();
    lines[13] = "line 14 changed".to_string();
    let changed = lines.join("\n") + "\n";
    write_file(dir.path(), "tracked.txt", &changed);

    let hunks = git_core::diff::working_diff(&repo, "tracked.txt", false).unwrap();
    assert_eq!(hunks.len(), 2, "expected two separate hunks from two far-apart edits");

    stage_hunk(&repo, "tracked.txt", hunks[0].old_start, hunks[0].new_start).unwrap();

    let staged = git_core::diff::working_diff(&repo, "tracked.txt", true).unwrap();
    let staged_text: String = staged.iter().flat_map(|h| h.lines.iter()).map(|l| l.content.clone()).collect();
    assert!(staged_text.contains("line 2 changed"));
    assert!(!staged_text.contains("line 14 changed"));

    let still_unstaged = git_core::diff::working_diff(&repo, "tracked.txt", false).unwrap();
    let unstaged_text: String = still_unstaged.iter().flat_map(|h| h.lines.iter()).map(|l| l.content.clone()).collect();
    assert!(unstaged_text.contains("line 14 changed"));
    assert!(!unstaged_text.contains("line 2 changed"));
}

#[test]
fn stage_hunk_on_a_hunk_that_no_longer_matches_returns_hunk_not_found() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\nline two\n");

    let result = stage_hunk(&repo, "tracked.txt", 999, 999);

    assert!(matches!(result, Err(git_core::stage::StageError::HunkNotFound)));
}
```

Also add `use git_core::commit::commit;` to the top of `stage_commit.rs` if not already present (it already is, per the existing file).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test stage_commit stage_hunk`
Expected: FAIL to compile — `stage_hunk` and `StageError::HunkNotFound` don't exist yet.

- [ ] **Step 3: Implement `stage_hunk`**

In `crates/git-core/src/stage.rs`, add the new error variant and function:

```rust
#[derive(Debug, Error)]
pub enum StageError {
    #[error("failed to update the index: {0}")]
    Index(#[from] git2::Error),
    #[error("repository has no working directory (bare repository)")]
    NoWorkdir,
    #[error("hunk not found (file changed since the diff was fetched)")]
    HunkNotFound,
}
```

```rust
pub fn stage_hunk(
    repo: &git2::Repository,
    path: &str,
    old_start: u32,
    new_start: u32,
) -> Result<(), StageError> {
    let mut opts = git2::DiffOptions::new();
    // Same options as `diff::working_diff`'s unstaged branch — the hunk identity the caller
    // passed in was read from that exact diff, so this one must be built identically or the
    // `(old_start, new_start)` pair won't line up with what's on screen.
    opts.pathspec(path)
        .disable_pathspec_match(true)
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true);
    let diff = repo.diff_index_to_workdir(None, Some(&mut opts))?;

    let matched = std::cell::Cell::new(false);
    let mut apply_opts = git2::ApplyOptions::new();
    apply_opts.hunk_callback(|hunk| {
        let is_match = matches!(hunk, Some(h) if h.old_start() == old_start && h.new_start() == new_start);
        if is_match {
            matched.set(true);
        }
        is_match
    });
    repo.apply(&diff, git2::ApplyLocation::Index, Some(&mut apply_opts))?;

    if !matched.get() {
        return Err(StageError::HunkNotFound);
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-core --test stage_commit stage_hunk`
Expected: PASS (both new tests)

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/stage.rs crates/git-core/tests/stage_commit.rs
git commit -m "feat(git-core): add stage_hunk for per-hunk staging"
```

---

## Task 2: `git_core::stage::unstage_hunk`

**Files:**
- Modify: `crates/git-core/src/stage.rs`
- Test: `crates/git-core/tests/stage_commit.rs`

**Interfaces:**
- Consumes: `StageError::HunkNotFound` from Task 1
- Produces: `pub fn unstage_hunk(repo: &git2::Repository, path: &str, old_start: u32, new_start: u32) -> Result<(), StageError>`

- [ ] **Step 1: Write the failing test**

Add to `crates/git-core/tests/stage_commit.rs` (extend the `use` line again):

```rust
use git_core::stage::{stage_file, stage_hunk, unstage_file, unstage_hunk};
```

```rust
#[test]
fn unstage_hunk_unstages_only_the_targeted_hunk_leaving_the_other_staged() {
    let (dir, mut repo) = init_repo();
    let original: String = (1..=15).map(|n| format!("line {n}\n")).collect();
    write_file(dir.path(), "tracked.txt", &original);
    stage_file(&repo, "tracked.txt").unwrap();
    commit(&mut repo, "initial commit").unwrap();

    let mut lines: Vec<String> = (1..=15).map(|n| format!("line {n}")).collect();
    lines[1] = "line 2 changed".to_string();
    lines[13] = "line 14 changed".to_string();
    let changed = lines.join("\n") + "\n";
    write_file(dir.path(), "tracked.txt", &changed);

    // Capture hunk identity before staging — same (old_start, new_start) pair identifies the
    // hunk in both the unstaged diff (index vs workdir) and, once staged, the staged diff
    // (HEAD vs index): HEAD is the "old" side in both cases, and staging the whole file just
    // copies workdir content into the index unchanged, so the "new" side matches too.
    let hunks_before = git_core::diff::working_diff(&repo, "tracked.txt", false).unwrap();
    assert_eq!(hunks_before.len(), 2);
    let target = (hunks_before[0].old_start, hunks_before[0].new_start);

    stage_file(&repo, "tracked.txt").unwrap();

    unstage_hunk(&repo, "tracked.txt", target.0, target.1).unwrap();

    let staged = git_core::diff::working_diff(&repo, "tracked.txt", true).unwrap();
    let staged_text: String = staged.iter().flat_map(|h| h.lines.iter()).map(|l| l.content.clone()).collect();
    assert!(!staged_text.contains("line 2 changed"));
    assert!(staged_text.contains("line 14 changed"));

    let unstaged = git_core::diff::working_diff(&repo, "tracked.txt", false).unwrap();
    let unstaged_text: String = unstaged.iter().flat_map(|h| h.lines.iter()).map(|l| l.content.clone()).collect();
    assert!(unstaged_text.contains("line 2 changed"));
    assert!(!unstaged_text.contains("line 14 changed"));
}

#[test]
fn unstage_hunk_on_a_hunk_that_no_longer_matches_returns_hunk_not_found() {
    let (dir, mut repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\n");
    stage_file(&repo, "tracked.txt").unwrap();
    commit(&mut repo, "initial commit").unwrap();
    write_file(dir.path(), "tracked.txt", "line one changed\n");
    stage_file(&repo, "tracked.txt").unwrap();

    let result = unstage_hunk(&repo, "tracked.txt", 999, 999);

    assert!(matches!(result, Err(git_core::stage::StageError::HunkNotFound)));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test stage_commit unstage_hunk`
Expected: FAIL to compile — `unstage_hunk` doesn't exist yet.

- [ ] **Step 3: Implement `unstage_hunk`**

In `crates/git-core/src/stage.rs`:

```rust
pub fn unstage_hunk(
    repo: &git2::Repository,
    path: &str,
    old_start: u32,
    new_start: u32,
) -> Result<(), StageError> {
    let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
    let mut opts = git2::DiffOptions::new();
    opts.pathspec(path).disable_pathspec_match(true).reverse(true);
    let diff = repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?;

    let matched = std::cell::Cell::new(false);
    let mut apply_opts = git2::ApplyOptions::new();
    // `reverse(true)` swaps which side is "old" and which is "new", so the hunk this callback
    // sees has old/new flipped relative to the (old_start, new_start) the caller captured from
    // the forward (non-reversed) diff.
    apply_opts.hunk_callback(|hunk| {
        let is_match = matches!(hunk, Some(h) if h.old_start() == new_start && h.new_start() == old_start);
        if is_match {
            matched.set(true);
        }
        is_match
    });
    repo.apply(&diff, git2::ApplyLocation::Index, Some(&mut apply_opts))?;

    if !matched.get() {
        return Err(StageError::HunkNotFound);
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-core --test stage_commit unstage_hunk`
Expected: PASS (both new tests)

- [ ] **Step 5: Commit**

```bash
git add crates/git-core/src/stage.rs crates/git-core/tests/stage_commit.rs
git commit -m "feat(git-core): add unstage_hunk for per-hunk unstaging"
```

---

## Task 3: `git_core::stage::discard_hunk`

**Files:**
- Modify: `crates/git-core/src/stage.rs`
- Test: `crates/git-core/tests/stage_commit.rs`

**Interfaces:**
- Consumes: `StageError::HunkNotFound` from Task 1
- Produces: `pub fn discard_hunk(repo: &git2::Repository, path: &str, old_start: u32, new_start: u32) -> Result<(), StageError>`

- [ ] **Step 1: Write the failing test**

Add to `crates/git-core/tests/stage_commit.rs` (extend the `use` line again):

```rust
use git_core::stage::{discard_hunk, stage_file, stage_hunk, unstage_file, unstage_hunk};
```

```rust
#[test]
fn discard_hunk_reverts_only_the_targeted_hunk_in_the_workdir() {
    let (dir, mut repo) = init_repo();
    let original: String = (1..=15).map(|n| format!("line {n}\n")).collect();
    write_file(dir.path(), "tracked.txt", &original);
    stage_file(&repo, "tracked.txt").unwrap();
    commit(&mut repo, "initial commit").unwrap();

    let mut lines: Vec<String> = (1..=15).map(|n| format!("line {n}")).collect();
    lines[1] = "line 2 changed".to_string();
    lines[13] = "line 14 changed".to_string();
    let changed = lines.join("\n") + "\n";
    write_file(dir.path(), "tracked.txt", &changed);

    let hunks = git_core::diff::working_diff(&repo, "tracked.txt", false).unwrap();
    assert_eq!(hunks.len(), 2);

    discard_hunk(&repo, "tracked.txt", hunks[0].old_start, hunks[0].new_start).unwrap();

    let on_disk = std::fs::read_to_string(dir.path().join("tracked.txt")).unwrap();
    assert!(on_disk.contains("line 2\n"), "discarded hunk's line should be back to original");
    assert!(!on_disk.contains("line 2 changed"));
    assert!(on_disk.contains("line 14 changed"), "the other hunk's edit must survive");
}

#[test]
fn discard_hunk_on_a_hunk_that_no_longer_matches_returns_hunk_not_found() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\nline two\n");

    let result = discard_hunk(&repo, "tracked.txt", 999, 999);

    assert!(matches!(result, Err(git_core::stage::StageError::HunkNotFound)));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p git-core --test stage_commit discard_hunk`
Expected: FAIL to compile — `discard_hunk` doesn't exist yet.

- [ ] **Step 3: Implement `discard_hunk`**

In `crates/git-core/src/stage.rs`:

```rust
pub fn discard_hunk(
    repo: &git2::Repository,
    path: &str,
    old_start: u32,
    new_start: u32,
) -> Result<(), StageError> {
    let mut opts = git2::DiffOptions::new();
    opts.pathspec(path)
        .disable_pathspec_match(true)
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true)
        .reverse(true);
    let diff = repo.diff_index_to_workdir(None, Some(&mut opts))?;

    let matched = std::cell::Cell::new(false);
    let mut apply_opts = git2::ApplyOptions::new();
    // Same old/new swap as `unstage_hunk` — see its comment.
    apply_opts.hunk_callback(|hunk| {
        let is_match = matches!(hunk, Some(h) if h.old_start() == new_start && h.new_start() == old_start);
        if is_match {
            matched.set(true);
        }
        is_match
    });
    repo.apply(&diff, git2::ApplyLocation::WorkDir, Some(&mut apply_opts))?;

    if !matched.get() {
        return Err(StageError::HunkNotFound);
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p git-core --test stage_commit discard_hunk`
Expected: PASS (both new tests)

- [ ] **Step 5: Run the full git-core test suite**

Run: `cargo test -p git-core`
Expected: PASS, no regressions

- [ ] **Step 6: Commit**

```bash
git add crates/git-core/src/stage.rs crates/git-core/tests/stage_commit.rs
git commit -m "feat(git-core): add discard_hunk for per-hunk workdir discard"
```

---

## Task 4: Wire hunk staging through the Tauri worker and commands

**Files:**
- Modify: `crates/tauri-app/src/worker.rs`
- Modify: `crates/tauri-app/src/commands.rs`
- Modify: `crates/tauri-app/src/main.rs`

**Interfaces:**
- Consumes: `git_core::stage::{stage_hunk, unstage_hunk, discard_hunk}` from Tasks 1-3
- Produces: `WorkerHandle::stage_hunk(&self, path: String, old_start: u32, new_start: u32) -> Result<(), String>` (and `unstage_hunk`, `discard_hunk` with the same signature)
- Produces: `#[tauri::command] pub async fn stage_hunk(repo_path: String, path: String, old_start: u32, new_start: u32, state: State<'_, AppState>) -> Result<(), String>` (and `unstage_hunk`, `discard_hunk`)

- [ ] **Step 1: Write the failing tests**

In `crates/tauri-app/src/worker.rs`, inside `mod tests` (near `stage_then_commit_round_trips_through_the_worker` at line ~2225), add:

```rust
#[test]
fn stage_hunk_then_commit_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\nline two\n");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "tracked.txt", "line one changed\nline two\n");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let hunks = handle.get_working_diff("tracked.txt".into(), false).unwrap();
    assert_eq!(hunks.len(), 1);

    handle
        .stage_hunk("tracked.txt".into(), hunks[0].old_start, hunks[0].new_start)
        .unwrap();

    let staged = handle.get_status().unwrap();
    assert!(staged.iter().any(|e| e.path == "tracked.txt" && e.staged));
}

#[test]
fn unstage_hunk_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\nline two\n");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "tracked.txt", "line one changed\nline two\n");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let hunks = handle.get_working_diff("tracked.txt".into(), false).unwrap();
    handle
        .stage_hunk("tracked.txt".into(), hunks[0].old_start, hunks[0].new_start)
        .unwrap();

    handle
        .unstage_hunk("tracked.txt".into(), hunks[0].old_start, hunks[0].new_start)
        .unwrap();

    let status = handle.get_status().unwrap();
    assert!(!status.iter().any(|e| e.path == "tracked.txt" && e.staged));
}

#[test]
fn discard_hunk_round_trips_through_the_worker() {
    let (dir, repo) = init_repo();
    write_file(dir.path(), "tracked.txt", "line one\nline two\n");
    commit_all(&repo, "initial commit");
    write_file(dir.path(), "tracked.txt", "line one changed\nline two\n");

    let worker = Worker::spawn(dir.path().to_path_buf()).unwrap();
    let handle = worker.handle();
    let hunks = handle.get_working_diff("tracked.txt".into(), false).unwrap();

    handle
        .discard_hunk("tracked.txt".into(), hunks[0].old_start, hunks[0].new_start)
        .unwrap();

    let on_disk = std::fs::read_to_string(dir.path().join("tracked.txt")).unwrap();
    assert_eq!(on_disk, "line one\nline two\n");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p tauri-app --lib worker::tests::stage_hunk worker::tests::unstage_hunk worker::tests::discard_hunk`
Expected: FAIL to compile — `Command::StageHunk`/`handle.stage_hunk` etc. don't exist yet.

- [ ] **Step 3: Add `Command` enum variants**

In `crates/tauri-app/src/worker.rs`, in `enum Command` (right after the existing `UnstageFile { path: String, reply: Sender<Result<(), String>> },` variant, ~line 95):

```rust
    StageHunk {
        path: String,
        old_start: u32,
        new_start: u32,
        reply: Sender<Result<(), String>>,
    },
    UnstageHunk {
        path: String,
        old_start: u32,
        new_start: u32,
        reply: Sender<Result<(), String>>,
    },
    DiscardHunk {
        path: String,
        old_start: u32,
        new_start: u32,
        reply: Sender<Result<(), String>>,
    },
```

- [ ] **Step 4: Add worker loop match arms**

Right after the existing `Command::UnstageFile { path, reply } => { ... }` arm (~line 471):

```rust
                    Command::StageHunk {
                        path,
                        old_start,
                        new_start,
                        reply,
                    } => {
                        let result = git_core::stage::stage_hunk(&repo, &path, old_start, new_start)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::UnstageHunk {
                        path,
                        old_start,
                        new_start,
                        reply,
                    } => {
                        let result = git_core::stage::unstage_hunk(&repo, &path, old_start, new_start)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
                    Command::DiscardHunk {
                        path,
                        old_start,
                        new_start,
                        reply,
                    } => {
                        let result = git_core::stage::discard_hunk(&repo, &path, old_start, new_start)
                            .map_err(|e| e.to_string());
                        let _ = reply.send(result);
                    }
```

- [ ] **Step 5: Add `WorkerHandle` methods**

Right after the existing `pub fn unstage_file(...)` method (~line 1333):

```rust
    pub fn stage_hunk(&self, path: String, old_start: u32, new_start: u32) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::StageHunk {
                path,
                old_start,
                new_start,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn unstage_hunk(&self, path: String, old_start: u32, new_start: u32) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::UnstageHunk {
                path,
                old_start,
                new_start,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    pub fn discard_hunk(&self, path: String, old_start: u32, new_start: u32) -> Result<(), String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::DiscardHunk {
                path,
                old_start,
                new_start,
                reply: reply_tx,
            })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }
```

- [ ] **Step 6: Run worker tests to verify they pass**

Run: `cargo test -p tauri-app --lib worker::tests::stage_hunk worker::tests::unstage_hunk worker::tests::discard_hunk`
Expected: PASS

- [ ] **Step 7: Add `#[tauri::command]` functions**

In `crates/tauri-app/src/commands.rs`, right after the existing `unstage_file` command (~line 861):

```rust
#[tauri::command]
pub async fn stage_hunk(
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.stage_hunk(path, old_start, new_start)
}

#[tauri::command]
pub async fn unstage_hunk(
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.unstage_hunk(path, old_start, new_start)
}

#[tauri::command]
pub async fn discard_hunk(
    repo_path: String,
    path: String,
    old_start: u32,
    new_start: u32,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state, &repo_path)?.discard_hunk(path, old_start, new_start)
}
```

- [ ] **Step 8: Register the new commands**

In `crates/tauri-app/src/main.rs`, add `discard_hunk, stage_hunk, unstage_hunk` to the `use commands::{...}` import block (alphabetically — `discard_hunk` lands near `delete_tag`/`detect_forge_repository`, `stage_hunk` right before `stage_file`... actually alphabetically `stage_hunk` < `stage_file`? No: `'h' > 'f'` so `stage_file` < `stage_hunk`; keep the existing list's alphabetical order and insert accordingly), and add `stage_hunk`, `unstage_hunk`, `discard_hunk` to the `tauri::generate_handler![...]` list right after the existing `stage_file`/`unstage_file` entries (~line 39):

```rust
            stage_file,
            stage_hunk,
            unstage_file,
            unstage_hunk,
            discard_hunk,
```

(Leave `commit` and everything after it untouched — just these three new lines inserted.)

- [ ] **Step 9: Build the whole workspace to confirm it compiles**

Run: `cargo build --workspace`
Expected: builds cleanly

- [ ] **Step 10: Run the full tauri-app test suite**

Run: `cargo test -p tauri-app`
Expected: PASS, no regressions

- [ ] **Step 11: Commit**

```bash
git add crates/tauri-app/src/worker.rs crates/tauri-app/src/commands.rs crates/tauri-app/src/main.rs
git commit -m "feat(tauri-app): wire stage_hunk/unstage_hunk/discard_hunk commands"
```

---

## Task 5: `RepoClient` interface + Tauri client — compile-green checkpoint

**Files:**
- Modify: `frontend/src/ipc/RepoClient.ts`
- Modify: `frontend/src/ipc/tauriRepoClient.ts`
- Modify: `frontend/src/components/DiffPane.test.tsx`
- Modify: `frontend/src/components/RebasePlanner.test.tsx`
- Modify: `frontend/src/components/ConflictResolutionPane.test.tsx`
- Modify: `frontend/src/components/RepoPicker.test.tsx`
- Modify: `frontend/src/lib/commands.test.ts`
- Modify: `frontend/src/state/useAppState.test.ts`

**Interfaces:**
- Consumes: Tauri commands `stage_hunk`/`unstage_hunk`/`discard_hunk` from Task 4
- Produces: `RepoClient.stageHunk(repoPath: string, path: string, oldStart: number, newStart: number): Promise<void>` (and `unstageHunk`, `discardHunk` with the same signature)

This task exists purely to keep the tree compiling: adding 3 required methods to `RepoClient` breaks every object literal implementing that interface until each one is updated. No new behavior lands here — every fake stays a stub (`unused`/`unimplemented()`/`vi.fn()`, matching each file's existing style). Task 6 adds the first real test coverage.

- [ ] **Step 1: Add the interface methods**

In `frontend/src/ipc/RepoClient.ts`, right after `unstageFile(repoPath: string, path: string): Promise<void>;` (line 218):

```typescript
  stageHunk(repoPath: string, path: string, oldStart: number, newStart: number): Promise<void>;
  unstageHunk(repoPath: string, path: string, oldStart: number, newStart: number): Promise<void>;
  discardHunk(repoPath: string, path: string, oldStart: number, newStart: number): Promise<void>;
```

- [ ] **Step 2: Implement the real client**

In `frontend/src/ipc/tauriRepoClient.ts`, right after `unstageFile: (repoPath: string, path: string) => invoke("unstage_file", { repoPath, path }),` (line 54):

```typescript
  stageHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    invoke("stage_hunk", { repoPath, path, oldStart, newStart }),
  unstageHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    invoke("unstage_hunk", { repoPath, path, oldStart, newStart }),
  discardHunk: (repoPath: string, path: string, oldStart: number, newStart: number) =>
    invoke("discard_hunk", { repoPath, path, oldStart, newStart }),
```

- [ ] **Step 3: Verify the compile break**

Run: `cd frontend && npx tsc --noEmit -p tsconfig.app.json`
Expected: FAIL — "Property 'stageHunk' is missing in type ... but required in type 'RepoClient'" (or similar) in every file listed above.

- [ ] **Step 4: Fix `DiffPane.test.tsx`, `RebasePlanner.test.tsx`, `ConflictResolutionPane.test.tsx` (bare `unused` style)**

```bash
cd frontend
sed -i -E 's/^([ ]*)unstageFile: unused,$/&\n\1stageHunk: unused,\n\1unstageHunk: unused,\n\1discardHunk: unused,/' \
  src/components/DiffPane.test.tsx src/components/RebasePlanner.test.tsx src/components/ConflictResolutionPane.test.tsx
```

- [ ] **Step 5: Fix `RepoPicker.test.tsx` and `useAppState.test.ts` (`async () => unimplemented()` style)**

```bash
cd frontend
sed -i -E 's/^([ ]*)unstageFile: async \(\) => unimplemented\(\),$/&\n\1stageHunk: async () => unimplemented(),\n\1unstageHunk: async () => unimplemented(),\n\1discardHunk: async () => unimplemented(),/' \
  src/components/RepoPicker.test.tsx src/state/useAppState.test.ts
```

- [ ] **Step 6: Fix `commands.test.ts` (`vi.fn()` style)**

```bash
cd frontend
sed -i -E 's/^([ ]*)unstageFile: vi\.fn\(\),$/&\n\1stageHunk: vi.fn(),\n\1unstageHunk: vi.fn(),\n\1discardHunk: vi.fn(),/' \
  src/lib/commands.test.ts
```

- [ ] **Step 7: Verify the compile is green again**

Run: `cd frontend && npx tsc --noEmit -p tsconfig.app.json`
Expected: no errors

- [ ] **Step 8: Run the full frontend test suite**

Run: `cd frontend && npx vitest run`
Expected: PASS, no regressions (every fake still satisfies `RepoClient`, none of them are exercised differently yet)

- [ ] **Step 9: Commit**

```bash
git add frontend/src/ipc/RepoClient.ts frontend/src/ipc/tauriRepoClient.ts \
  frontend/src/components/DiffPane.test.tsx frontend/src/components/RebasePlanner.test.tsx \
  frontend/src/components/ConflictResolutionPane.test.tsx frontend/src/components/RepoPicker.test.tsx \
  frontend/src/lib/commands.test.ts frontend/src/state/useAppState.test.ts
git commit -m "feat(frontend): add stageHunk/unstageHunk/discardHunk to RepoClient"
```

---

## Task 6: `useAppState` actions

**Files:**
- Modify: `frontend/src/state/useAppState.ts`
- Modify: `frontend/src/state/useAppState.test.ts`

**Interfaces:**
- Consumes: `RepoClient.{stageHunk,unstageHunk,discardHunk}` from Task 5, `runMutation` (existing, `useAppState.ts` ~line 283)
- Produces: `UseAppStateResult.stageHunk(path: string, oldStart: number, newStart: number): Promise<void>` (and `unstageHunk`, `discardHunk` with the same signature)

- [ ] **Step 1: Write the failing test**

In `frontend/src/state/useAppState.test.ts`, add a new `describe`/`it` block near the existing `it("stageFile calls client.stageFile then refreshes status", ...)` test (~line 878), following its exact structure:

```typescript
  it("stageHunk calls client.stageHunk then refreshes status", async () => {
    const status: StatusEntry[] = [{ path: "a.txt", staged: true, kind: "Modified" }];
    let stageHunkArgs: [string, number, number] | null = null;
    const client = fakeClient({
      getStatus: async () => status,
      stageHunk: async (_repoPath: string, path: string, oldStart: number, newStart: number) => {
        stageHunkArgs = [path, oldStart, newStart];
      },
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.stageHunk("a.txt", 3, 4));

    expect(stageHunkArgs).toEqual(["a.txt", 3, 4]);
    expect(result.current.state.status).toEqual(status);
  });

  it("unstageHunk calls client.unstageHunk with the given path and hunk identity", async () => {
    let unstageHunkArgs: [string, number, number] | null = null;
    const client = fakeClient({
      unstageHunk: async (_repoPath: string, path: string, oldStart: number, newStart: number) => {
        unstageHunkArgs = [path, oldStart, newStart];
      },
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.unstageHunk("a.txt", 3, 4));

    expect(unstageHunkArgs).toEqual(["a.txt", 3, 4]);
  });

  it("discardHunk calls client.discardHunk with the given path and hunk identity", async () => {
    let discardHunkArgs: [string, number, number] | null = null;
    const client = fakeClient({
      discardHunk: async (_repoPath: string, path: string, oldStart: number, newStart: number) => {
        discardHunkArgs = [path, oldStart, newStart];
      },
      stageFile: async () => unimplemented(),
      unstageFile: async () => unimplemented(),
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.discardHunk("a.txt", 3, 4));

    expect(discardHunkArgs).toEqual(["a.txt", 3, 4]);
  });
```

Check the top of `useAppState.test.ts` for the exact `fakeClient` helper name, the `renderHook`/`act` imports, and `TEST_REPO_PATH` constant already used by the neighboring `stageFile` test (~line 878-930) — match them exactly; don't introduce new helpers.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && npx vitest run src/state/useAppState.test.ts -t "Hunk"`
Expected: FAIL — `result.current.stageHunk` is not a function.

- [ ] **Step 3: Add the interface methods**

In `frontend/src/state/useAppState.ts`, in `interface UseAppStateResult`, right after `unstageFile(path: string): Promise<void>;` (line 98):

```typescript
  stageHunk(path: string, oldStart: number, newStart: number): Promise<void>;
  unstageHunk(path: string, oldStart: number, newStart: number): Promise<void>;
  discardHunk(path: string, oldStart: number, newStart: number): Promise<void>;
```

- [ ] **Step 4: Implement the actions**

Right after the existing `unstageFile` callback (line ~324, right before `const commit = useCallback(...)`):

```typescript
  const stageHunk = useCallback(
    (path: string, oldStart: number, newStart: number) =>
      runMutation(() => client.stageHunk(repoPath, path, oldStart, newStart)),
    [client, runMutation, repoPath],
  );
  const unstageHunk = useCallback(
    (path: string, oldStart: number, newStart: number) =>
      runMutation(() => client.unstageHunk(repoPath, path, oldStart, newStart)),
    [client, runMutation, repoPath],
  );
  const discardHunk = useCallback(
    (path: string, oldStart: number, newStart: number) =>
      runMutation(() => client.discardHunk(repoPath, path, oldStart, newStart)),
    [client, runMutation, repoPath],
  );
```

- [ ] **Step 5: Add them to the returned object**

Right after `unstageFile,` in the `return { ... }` block (~line 713):

```typescript
    stageHunk,
    unstageHunk,
    discardHunk,
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd frontend && npx vitest run src/state/useAppState.test.ts`
Expected: PASS, full file green (all existing tests plus the 3 new ones)

- [ ] **Step 7: Typecheck**

Run: `cd frontend && npx tsc --noEmit -p tsconfig.app.json`
Expected: no errors

- [ ] **Step 8: Commit**

```bash
git add frontend/src/state/useAppState.ts frontend/src/state/useAppState.test.ts
git commit -m "feat(frontend): add stageHunk/unstageHunk/discardHunk actions to useAppState"
```

---

## Task 7: `DiffView` per-hunk action buttons

**Files:**
- Modify: `frontend/src/components/DiffView.tsx`
- Modify: `frontend/src/components/DiffView.module.css`
- Modify: `frontend/src/components/DiffView.test.tsx`

**Interfaces:**
- Consumes: `Toolbar` from `./primitives/Toolbar` (existing)
- Produces: `DiffView` gains three new optional props: `onStageHunk?: (oldStart: number, newStart: number) => void`, `onUnstageHunk?: (oldStart: number, newStart: number) => void`, `onDiscardHunk?: (oldStart: number, newStart: number) => void`

- [ ] **Step 1: Write the failing tests**

Add to `frontend/src/components/DiffView.test.tsx` (reuse the `hunks` fixture already defined per-test — copy the one-hunk fixture from the existing "renders each line's content" test):

```typescript
  it("renders no action buttons when no hunk callbacks are passed", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 1, oldLines: 2, newStart: 1, newLines: 3, lines: [] },
    ];

    render(<DiffView hunks={hunks} />);

    expect(screen.queryByText("Stage Hunk")).not.toBeInTheDocument();
    expect(screen.queryByText("Unstage Hunk")).not.toBeInTheDocument();
    expect(screen.queryByText("Discard Hunk")).not.toBeInTheDocument();
  });

  it("clicking Stage Hunk calls onStageHunk with that hunk's old/new start", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 5, oldLines: 2, newStart: 7, newLines: 3, lines: [] },
    ];
    const onStageHunk = vi.fn();

    render(<DiffView hunks={hunks} onStageHunk={onStageHunk} />);
    fireEvent.click(screen.getByText("Stage Hunk"));

    expect(onStageHunk).toHaveBeenCalledWith(5, 7);
  });

  it("clicking Unstage Hunk calls onUnstageHunk with that hunk's old/new start", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 5, oldLines: 2, newStart: 7, newLines: 3, lines: [] },
    ];
    const onUnstageHunk = vi.fn();

    render(<DiffView hunks={hunks} onUnstageHunk={onUnstageHunk} />);
    fireEvent.click(screen.getByText("Unstage Hunk"));

    expect(onUnstageHunk).toHaveBeenCalledWith(5, 7);
  });

  it("Discard Hunk requires a second click (Confirm Discard) before calling onDiscardHunk", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 5, oldLines: 2, newStart: 7, newLines: 3, lines: [] },
    ];
    const onDiscardHunk = vi.fn();

    render(<DiffView hunks={hunks} onDiscardHunk={onDiscardHunk} />);
    fireEvent.click(screen.getByText("Discard Hunk"));

    expect(onDiscardHunk).not.toHaveBeenCalled();
    expect(screen.getByText("Confirm Discard")).toBeInTheDocument();

    fireEvent.click(screen.getByText("Confirm Discard"));

    expect(onDiscardHunk).toHaveBeenCalledWith(5, 7);
  });

  it("switching to a different hunks array resets any pending discard confirmation", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 5, oldLines: 2, newStart: 7, newLines: 3, lines: [] },
    ];
    const onDiscardHunk = vi.fn();

    const { rerender } = render(<DiffView hunks={hunks} onDiscardHunk={onDiscardHunk} />);
    fireEvent.click(screen.getByText("Discard Hunk"));
    expect(screen.getByText("Confirm Discard")).toBeInTheDocument();

    const otherHunks: DiffHunk[] = [
      { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [] },
    ];
    rerender(<DiffView hunks={otherHunks} onDiscardHunk={onDiscardHunk} />);

    expect(screen.getByText("Discard Hunk")).toBeInTheDocument();
    expect(screen.queryByText("Confirm Discard")).not.toBeInTheDocument();
  });
```

Add `fireEvent` and `vi` to the existing `import { render, screen } from "@testing-library/react";` / `import { describe, expect, it } from "vitest";` lines at the top of the file.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && npx vitest run src/components/DiffView.test.tsx`
Expected: FAIL — the 5 new tests fail (no buttons render yet); the 3 pre-existing tests still pass.

- [ ] **Step 3: Implement the buttons**

Replace the full contents of `frontend/src/components/DiffView.tsx`:

```tsx
import { useEffect, useState } from "react";
import type { DiffHunk, DiffLineOrigin } from "../ipc/RepoClient";
import { Toolbar } from "./primitives/Toolbar";
import styles from "./DiffView.module.css";

export function DiffView({
  hunks,
  onStageHunk,
  onUnstageHunk,
  onDiscardHunk,
}: {
  hunks: DiffHunk[];
  onStageHunk?: (oldStart: number, newStart: number) => void;
  onUnstageHunk?: (oldStart: number, newStart: number) => void;
  onDiscardHunk?: (oldStart: number, newStart: number) => void;
}) {
  const [confirmingDiscardIndex, setConfirmingDiscardIndex] = useState<number | null>(null);

  // A stale armed confirmation must never carry over to a different hunk list (switching files,
  // or any hunk mutation refetching this file's own hunks) — `hunkIndex` is only meaningful
  // relative to the specific `hunks` array it was armed against. Same class of bug
  // `BranchSwitcher`'s `closePopoverState` guards against for `pendingForceFor`.
  useEffect(() => {
    setConfirmingDiscardIndex(null);
  }, [hunks]);

  if (hunks.length === 0) {
    return <p>No differences</p>;
  }

  return (
    <div>
      {hunks.map((hunk, hunkIndex) => (
        <div key={hunkIndex}>
          <div className={styles.hunkHeader}>
            <span>
              @@ -{hunk.oldStart},{hunk.oldLines} +{hunk.newStart},{hunk.newLines} @@
            </span>
            {(onStageHunk !== undefined || onUnstageHunk !== undefined || onDiscardHunk !== undefined) && (
              <Toolbar>
                {onStageHunk !== undefined && (
                  <button onClick={() => onStageHunk(hunk.oldStart, hunk.newStart)}>Stage Hunk</button>
                )}
                {onUnstageHunk !== undefined && (
                  <button onClick={() => onUnstageHunk(hunk.oldStart, hunk.newStart)}>Unstage Hunk</button>
                )}
                {onDiscardHunk !== undefined &&
                  (confirmingDiscardIndex === hunkIndex ? (
                    <button
                      onClick={() => {
                        onDiscardHunk(hunk.oldStart, hunk.newStart);
                        setConfirmingDiscardIndex(null);
                      }}
                    >
                      Confirm Discard
                    </button>
                  ) : (
                    <button onClick={() => setConfirmingDiscardIndex(hunkIndex)}>Discard Hunk</button>
                  ))}
              </Toolbar>
            )}
          </div>
          <pre>
            {hunk.lines.map((line, lineIndex) => (
              <div
                key={lineIndex}
                className={`${styles.line} ${styles[`line${line.origin}`]} diff-line diff-line-${line.origin.toLowerCase()}`}
              >
                <span aria-hidden="true">{originPrefix(line.origin)}</span>
                {line.content}
              </div>
            ))}
          </pre>
        </div>
      ))}
    </div>
  );
}

function originPrefix(origin: DiffLineOrigin): string {
  switch (origin) {
    case "Add":
      return "+";
    case "Remove":
      return "-";
    case "Context":
      return " ";
  }
}
```

Update `frontend/src/components/DiffView.module.css`'s `.hunkHeader` rule to lay the label and the `Toolbar` out on one row, label left, buttons right:

```css
.hunkHeader {
  font-family: var(--font-mono);
  font-size: var(--text-sm);
  background: var(--color-bg-subtle);
  color: var(--color-text-muted);
  padding: var(--space-1) var(--space-2);
  margin-top: var(--space-2);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2);
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && npx vitest run src/components/DiffView.test.tsx`
Expected: PASS, all 8 tests (3 pre-existing + 5 new)

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/DiffView.tsx frontend/src/components/DiffView.module.css frontend/src/components/DiffView.test.tsx
git commit -m "feat(frontend): add per-hunk Stage/Unstage/Discard buttons to DiffView"
```

---

## Task 8: Wire hunk actions through `DiffPane` and `App`

**Files:**
- Modify: `frontend/src/components/DiffPane.tsx`
- Modify: `frontend/src/components/DiffPane.test.tsx`
- Modify: `frontend/src/App.tsx`

**Interfaces:**
- Consumes: `DiffView`'s three optional callback props from Task 7; `useAppState.{stageHunk,unstageHunk,discardHunk}` from Task 6
- Produces: `DiffPane` gains three new required props: `onStageHunk(path: string, oldStart: number, newStart: number): void`, `onUnstageHunk(...)`, `onDiscardHunk(...)` — same shape as the existing `onStageFile`/`onUnstageFile`

- [ ] **Step 1: Bulk-add the 3 new props to every `<DiffPane .../>` render call in the test file**

`DiffPane.test.tsx` has 31 render call sites, every one already passing `onUnstageFile={...}` (as `vi.fn()` or a named variable). Insert the 3 new props right after every `onUnstageFile={...}` line, defaulting to `vi.fn()`:

```bash
cd frontend
sed -i -E 's/^([ ]*)onUnstageFile=\{[^}]*\}$/&\n\1onStageHunk={vi.fn()}\n\1onUnstageHunk={vi.fn()}\n\1onDiscardHunk={vi.fn()}/' \
  src/components/DiffPane.test.tsx
```

- [ ] **Step 2: Add the failing wiring tests**

Add to `frontend/src/components/DiffPane.test.tsx`, inside the `describe("uncommitted", ...)` block, following the exact render-prop-list shape every other test there uses (all 18+ props, now 21+ after Step 1's sed):

```typescript
    it("shows Stage Hunk (not Unstage Hunk) and Discard Hunk for an unstaged file's diff", async () => {
      const hunks: DiffHunk[] = [
        { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add", content: "x" }] },
      ];
      const client = fakeClient({ getWorkingDiff: async () => hunks });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("a.txt (Modified)"));

      expect(await screen.findByText("Stage Hunk")).toBeInTheDocument();
      expect(screen.queryByText("Unstage Hunk")).not.toBeInTheDocument();
      expect(screen.getByText("Discard Hunk")).toBeInTheDocument();
    });

    it("shows Unstage Hunk (not Stage Hunk) and Discard Hunk for a staged file's diff", async () => {
      const hunks: DiffHunk[] = [
        { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [{ origin: "Add", content: "x" }] },
      ];
      const client = fakeClient({ getWorkingDiff: async () => hunks });

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageHunk={vi.fn()}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("b.txt (New)"));

      expect(await screen.findByText("Unstage Hunk")).toBeInTheDocument();
      expect(screen.queryByText("Stage Hunk")).not.toBeInTheDocument();
      expect(screen.getByText("Discard Hunk")).toBeInTheDocument();
    });

    it("clicking Stage Hunk calls onStageHunk with the selected file's path and the hunk's start lines", async () => {
      const hunks: DiffHunk[] = [
        { oldStart: 3, oldLines: 1, newStart: 4, newLines: 1, lines: [{ origin: "Add", content: "x" }] },
      ];
      const client = fakeClient({ getWorkingDiff: async () => hunks });
      const onStageHunk = vi.fn();

      render(
        <DiffPane
          repoPath={TEST_REPO_PATH}
          client={client}
          selectedRow="uncommitted"
          status={status}
          onStageFile={vi.fn()}
          onUnstageFile={vi.fn()}
          onStageHunk={onStageHunk}
          onUnstageHunk={vi.fn()}
          onDiscardHunk={vi.fn()}
          onCommit={vi.fn()}
          onSaveStash={vi.fn()}
          onSelectRow={vi.fn()}
          onResolveConflict={vi.fn()}
          onResolveAddDeleteConflict={vi.fn()}
          mergeMessage={null}
          onAbortMerge={vi.fn()}
          rebaseProgress={null}
          onRebaseContinue={vi.fn()}
          onRebaseAbort={vi.fn()}
        />,
      );

      fireEvent.click(screen.getByText("a.txt (Modified)"));
      fireEvent.click(await screen.findByText("Stage Hunk"));

      expect(onStageHunk).toHaveBeenCalledWith("a.txt", 3, 4);
    });
```

`status` here is the same `[{ path: "a.txt", staged: false, kind: "Modified" }, { path: "b.txt", staged: true, kind: "New" }]` fixture already defined at the top of the `describe("uncommitted", ...)` block.

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd frontend && npx vitest run src/components/DiffPane.test.tsx`
Expected: FAIL to compile at first (Step 1's sed hasn't run yet if done out of order — run Step 1 first) or, once Step 1's sed has run, FAIL on the 3 new tests specifically (`onStageHunk` prop doesn't exist on `DiffPane` yet) while the other (now prop-padded) tests still pass since extra props on a not-yet-updated component just get ignored by TS... actually TS will reject the extra props as excess — expect FAIL to compile until Step 4 lands.

- [ ] **Step 4: Wire the props through `DiffPane`/`UncommittedDiffPane`**

In `frontend/src/components/DiffPane.tsx`, add the 3 new props to `DiffPane`'s own prop type and pass-through call to `UncommittedDiffPane`:

```typescript
export function DiffPane({
  repoPath,
  client,
  selectedRow,
  status,
  onStageFile,
  onUnstageFile,
  onStageHunk,
  onUnstageHunk,
  onDiscardHunk,
  onCommit,
  onSaveStash,
  onSelectRow,
  onResolveConflict,
  onResolveAddDeleteConflict,
  mergeMessage,
  onAbortMerge,
  rebaseProgress,
  onRebaseContinue,
  onRebaseAbort,
}: {
  repoPath: string;
  client: RepoClient;
  selectedRow: SelectedRow;
  status: StatusEntry[];
  onStageFile: (path: string) => void;
  onUnstageFile: (path: string) => void;
  onStageHunk: (path: string, oldStart: number, newStart: number) => void;
  onUnstageHunk: (path: string, oldStart: number, newStart: number) => void;
  onDiscardHunk: (path: string, oldStart: number, newStart: number) => void;
  onCommit: (message: string) => void;
  onSaveStash: () => void;
  onSelectRow: (row: SelectedRow) => void;
  onResolveConflict: (path: string, resolvedContent: string) => void;
  onResolveAddDeleteConflict: (path: string, choice: FileConflictChoice) => void;
  mergeMessage: string | null;
  onAbortMerge: () => void;
  rebaseProgress: { currentStep: number; totalSteps: number } | null;
  onRebaseContinue: () => void;
  onRebaseAbort: () => void;
}) {
  if (selectedRow === "uncommitted") {
    return (
      <UncommittedDiffPane
        repoPath={repoPath}
        client={client}
        status={status}
        onStageFile={onStageFile}
        onUnstageFile={onUnstageFile}
        onStageHunk={onStageHunk}
        onUnstageHunk={onUnstageHunk}
        onDiscardHunk={onDiscardHunk}
        onCommit={onCommit}
        onSaveStash={onSaveStash}
        onSelectRow={onSelectRow}
        onResolveConflict={onResolveConflict}
        onResolveAddDeleteConflict={onResolveAddDeleteConflict}
        mergeMessage={mergeMessage}
        onAbortMerge={onAbortMerge}
        rebaseProgress={rebaseProgress}
        onRebaseContinue={onRebaseContinue}
        onRebaseAbort={onRebaseAbort}
      />
    );
  }
  return (
    <CommitDiffPane
      key={selectedRow.commitId}
      repoPath={repoPath}
      client={client}
      commitId={selectedRow.commitId}
      onSelectRow={onSelectRow}
    />
  );
}
```

Then in `UncommittedDiffPane`, add the 3 props to its own signature, and wire them into the `DiffView` call — replacing the current:

```typescript
        <DiffView hunks={displayedHunks} />
```

with:

```typescript
        <DiffView
          hunks={displayedHunks}
          onStageHunk={
            selected !== null && !selected.staged
              ? (hunkOldStart: number, hunkNewStart: number) => onStageHunk(selected.path, hunkOldStart, hunkNewStart)
              : undefined
          }
          onUnstageHunk={
            selected !== null && selected.staged
              ? (hunkOldStart: number, hunkNewStart: number) => onUnstageHunk(selected.path, hunkOldStart, hunkNewStart)
              : undefined
          }
          onDiscardHunk={
            selected !== null
              ? (hunkOldStart: number, hunkNewStart: number) => onDiscardHunk(selected.path, hunkOldStart, hunkNewStart)
              : undefined
          }
        />
```

(Note: parameters renamed to `hunkOldStart`/`hunkNewStart` in these inline callbacks only to avoid shadowing `DiffHunk`'s own `oldStart`/`newStart` field names in scope — no behavior difference.)

Add `onStageHunk: (path: string, oldStart: number, newStart: number) => void;`, `onUnstageHunk: ...`, `onDiscardHunk: ...` to `UncommittedDiffPane`'s destructured parameters and its prop type object, in the same position as the `DiffPane` change above (right after `onUnstageFile`).

- [ ] **Step 5: Wire `App.tsx`**

In `frontend/src/App.tsx`, in the `<DiffPane ... />` call (the one inside the `right={<SplitView ... right={<DiffPane .../>} />}` block), add right after the existing `onUnstageFile={appState.unstageFile}` line:

```typescript
                onStageHunk={appState.stageHunk}
                onUnstageHunk={appState.unstageHunk}
                onDiscardHunk={appState.discardHunk}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd frontend && npx vitest run src/components/DiffPane.test.tsx`
Expected: PASS, all tests (pre-existing + 3 new)

- [ ] **Step 7: Typecheck and full suite**

Run: `cd frontend && npx tsc --noEmit -p tsconfig.app.json && npx vitest run`
Expected: no type errors, full suite green

- [ ] **Step 8: Lint**

Run: `cd frontend && npx eslint src/components/DiffPane.tsx src/components/DiffPane.test.tsx src/App.tsx src/components/DiffView.tsx src/components/DiffView.test.tsx frontend/src/state/useAppState.ts frontend/src/state/useAppState.test.ts`
Expected: clean

- [ ] **Step 9: Commit**

```bash
git add frontend/src/components/DiffPane.tsx frontend/src/components/DiffPane.test.tsx frontend/src/App.tsx
git commit -m "feat(frontend): wire per-hunk stage/unstage/discard through DiffPane and App"
```

---

## Task 9: End-to-end coverage

**Files:**
- Create: `e2e/specs/hunk-staging.spec.ts`

**Interfaces:**
- Consumes: the full stack from Tasks 1-8; `expandSidebarSection` is NOT needed here (diff pane isn't behind an accordion)

- [ ] **Step 1: Write the spec**

Model this closely on `e2e/specs/stash-management.spec.ts` (same fixture repo path, same before/after cleanup shape). Create `e2e/specs/hunk-staging.spec.ts`:

```typescript
import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { expect } from "@wdio/globals";

const E2E_REPO_PATH = path.join(os.tmpdir(), "browsitory-e2e-repo");
const HUNK_FIXTURE_FILE = "hunk-fixture.txt";

describe("Browsitory hunk staging", () => {
  before(() => {
    const filePath = path.join(E2E_REPO_PATH, HUNK_FIXTURE_FILE);
    const original = Array.from({ length: 15 }, (_, i) => `line ${i + 1}`).join("\n") + "\n";
    fs.writeFileSync(filePath, original);
    execFileSync("git", ["add", HUNK_FIXTURE_FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
    execFileSync("git", ["commit", "-m", "e2e: seed hunk fixture file"], {
      cwd: E2E_REPO_PATH,
      stdio: "inherit",
    });
    const lines = original.split("\n");
    lines[1] = "line 2 changed";
    lines[13] = "line 14 changed";
    fs.writeFileSync(filePath, lines.join("\n"));
  });

  after(() => {
    // Leaves two new commits in the shared fixture repo's history (the seed commit plus this
    // spec's "stage one hunk" commit) — same tradeoff `stash-management.spec.ts`'s seed commit
    // already makes; nothing in this suite asserts an exact commit count. All that matters for
    // the next spec (alphabetically after "hunk-") is a clean working tree, so just check the
    // fixture file back out to match the latest commit.
    execFileSync("git", ["checkout", "--", HUNK_FIXTURE_FILE], { cwd: E2E_REPO_PATH, stdio: "inherit" });
  });

  it("stages a single hunk, commits it, and leaves the other hunk's edit unstaged", async () => {
    const fileRow = await $(`li*=${HUNK_FIXTURE_FILE}`);
    await fileRow.waitForExist({ timeout: 10000 });
    await browser.execute((el) => (el as HTMLElement).click(), await $(`button=${HUNK_FIXTURE_FILE} (Modified)`));

    const stageHunkButtons = await $$("button=Stage Hunk");
    await expect(stageHunkButtons).toBeElementsArrayOfSize(2);
    await browser.execute((el) => (el as HTMLElement).click(), stageHunkButtons[0]);

    await browser.waitUntil(
      async () => (await $$("button=Stage Hunk")).length === 1,
      { timeout: 10000, timeoutMsg: "expected only one unstaged hunk to remain after staging the other" },
    );

    const commitMessageBox = await $("textarea");
    await commitMessageBox.setValue("stage one hunk");
    const commitButton = await $("button=Commit");
    await browser.execute((el) => (el as HTMLElement).click(), commitButton);

    await browser.waitUntil(
      async () => {
        const content = fs.readFileSync(path.join(E2E_REPO_PATH, HUNK_FIXTURE_FILE), "utf8");
        return content.includes("line 14 changed");
      },
      { timeout: 10000, timeoutMsg: "expected the file to still have the unstaged hunk's dirty content" },
    );

    const committedShow = execFileSync("git", ["show", "HEAD:" + HUNK_FIXTURE_FILE], {
      cwd: E2E_REPO_PATH,
      encoding: "utf8",
    });
    expect(committedShow).toContain("line 2 changed");
    expect(committedShow).not.toContain("line 14 changed");
  });
});
```

Selectors above are already verified against the real markup: `DiffPane.tsx`'s file-list button renders literal text `{entry.path} ({entry.kind})` (so `button=hunk-fixture.txt (Modified)` is correct), and `CommitBox.tsx` renders a single `<textarea placeholder="Commit message">` and a `<button>Commit</button>` inside its `Toolbar` — matching the spec above as written, no adjustment needed.

- [ ] **Step 2: Run the new spec**

Run: `cd e2e && npx wdio run wdio.conf.ts --spec specs/hunk-staging.spec.ts`
Expected: PASS

- [ ] **Step 3: Run the full e2e suite to confirm no fixture-repo collisions with other specs**

Run: `cd e2e && npx wdio run wdio.conf.ts`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add e2e/specs/hunk-staging.spec.ts
git commit -m "test(e2e): add hunk staging spec"
```

---

## Final Verification

- [ ] Run `cargo test --workspace` — all green
- [ ] Run `cargo clippy --workspace --all-targets -- -D warnings` (or whatever this repo's clippy gate is — check `crates/*/Cargo.toml`/CI config for the exact invocation) — clean
- [ ] Run `cd frontend && npx tsc --noEmit -p tsconfig.app.json && npx eslint . && npx vitest run` — all green
- [ ] Run `cd e2e && npx wdio run wdio.conf.ts` — all green
- [ ] Manually launch the app (per the `run` skill/CLAUDE.md dev-server instructions) and exercise: stage a hunk, unstage a hunk, discard a hunk (with confirm), on a real multi-hunk file — confirm the buttons look right and the two-click Discard confirm doesn't feel broken
