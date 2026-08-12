# Task 1.C.01: Worker + Tauri commands for log/diff/stage/commit

## Goal

Extend `crates/tauri-app`'s `Worker`/`WorkerHandle` and `commands.rs` to expose Tasks 1.A.01-03's
`git-core` functions over IPC: `get_log`, `get_working_diff`, `get_commit_diff`,
`get_commit_files`, `stage_file`, `unstage_file`, `commit`. Same shape as the existing
`GetStatus`/`get_status` pair — one `Command` variant + one `WorkerHandle` method + one
`#[tauri::command]` per operation.

## Depends on

1.A.01 (`git_core::log`), 1.A.02 (`git_core::diff`), 1.A.03 (`git_core::stage`/`commit`).

## Interfaces produced

Seven new `#[tauri::command]` functions in `crates/tauri-app/src/commands.rs`:
`get_log(limit, state) -> Result<Vec<CommitInfoDto>, String>`,
`get_working_diff(path, staged, state) -> Result<Vec<DiffHunkDto>, String>`,
`get_commit_diff(commit_id, path, state) -> Result<Vec<DiffHunkDto>, String>`,
`get_commit_files(commit_id, state) -> Result<Vec<String>, String>`,
`stage_file(path, state) -> Result<(), String>`,
`unstage_file(path, state) -> Result<(), String>`,
`commit(message, state) -> Result<String, String>` — all take `state: State<AppState>`, same as
`get_status`. These get registered in `crates/tauri-app/src/main.rs`'s `invoke_handler`. Task
1.D.01 (frontend `RepoClient`) calls each of these by name via `invoke(...)`.

## Implementation notes

**`crates/tauri-app/src/worker.rs`** — add six more `Command` variants alongside the existing
`GetStatus` (full new enum; keep `GetStatus` unchanged):
```rust
use git_core::diff::DiffHunk;
use git_core::log::CommitInfo;
use git_core::status::StatusEntry;

pub(crate) enum Command {
    GetStatus {
        reply: Sender<Result<Vec<StatusEntry>, String>>,
    },
    GetLog {
        limit: usize,
        reply: Sender<Result<Vec<CommitInfo>, String>>,
    },
    GetWorkingDiff {
        path: String,
        staged: bool,
        reply: Sender<Result<Vec<DiffHunk>, String>>,
    },
    GetCommitDiff {
        commit_id: String,
        path: String,
        reply: Sender<Result<Vec<DiffHunk>, String>>,
    },
    GetCommitFiles {
        commit_id: String,
        reply: Sender<Result<Vec<String>, String>>,
    },
    StageFile {
        path: String,
        reply: Sender<Result<(), String>>,
    },
    UnstageFile {
        path: String,
        reply: Sender<Result<(), String>>,
    },
    Commit {
        message: String,
        reply: Sender<Result<String, String>>,
    },
}
```
`Worker::spawn`'s `match command` block gains one arm per variant, each following `GetStatus`'s
exact shape (`let result = git_core::<module>::<fn>(&repo, ...).map_err(|e| e.to_string());
let _ = reply.send(result);`) — e.g.:
```rust
Command::GetLog { limit, reply } => {
    let result = git_core::log::log(&repo, limit).map_err(|e| e.to_string());
    let _ = reply.send(result);
}
Command::GetWorkingDiff { path, staged, reply } => {
    let result =
        git_core::diff::working_diff(&repo, &path, staged).map_err(|e| e.to_string());
    let _ = reply.send(result);
}
// ... same pattern for GetCommitDiff (commit_diff), GetCommitFiles (commit_files),
// StageFile (stage_file), UnstageFile (unstage_file), Commit (commit)
```
`WorkerHandle` gains one method per variant, each following `get_status`'s exact
send-then-block-on-reply shape:
```rust
impl WorkerHandle {
    pub fn get_status(&self) -> Result<Vec<StatusEntry>, String> { /* unchanged */ }

    pub fn get_log(&self, limit: usize) -> Result<Vec<CommitInfo>, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.tx
            .send(Command::GetLog { limit, reply: reply_tx })
            .map_err(|_| "worker thread stopped".to_string())?;
        reply_rx
            .recv()
            .map_err(|_| "worker thread stopped before replying".to_string())?
    }

    // get_working_diff(&self, path: String, staged: bool) -> Result<Vec<DiffHunk>, String>
    // get_commit_diff(&self, commit_id: String, path: String) -> Result<Vec<DiffHunk>, String>
    // get_commit_files(&self, commit_id: String) -> Result<Vec<String>, String>
    // stage_file(&self, path: String) -> Result<(), String>
    // unstage_file(&self, path: String) -> Result<(), String>
    // commit(&self, message: String) -> Result<String, String>
    // — same send-then-recv shape as get_log/get_status for each.
}
```

**`crates/tauri-app/src/commands.rs`** — add DTOs and commands alongside the existing
`StatusEntryDto`/`open_repo`/`get_status` (keep those unchanged):
```rust
use git_core::diff::DiffHunk;
use git_core::log::CommitInfo;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitInfoDto {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
}

impl From<CommitInfo> for CommitInfoDto {
    fn from(c: CommitInfo) -> Self {
        CommitInfoDto {
            id: c.id,
            short_id: c.short_id,
            summary: c.summary,
            author_name: c.author_name,
            author_email: c.author_email,
            timestamp: c.timestamp,
        }
    }
}

#[derive(Serialize)]
pub struct DiffLineDto {
    pub origin: String,
    pub content: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunkDto {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<DiffLineDto>,
}

impl From<DiffHunk> for DiffHunkDto {
    fn from(h: DiffHunk) -> Self {
        DiffHunkDto {
            old_start: h.old_start,
            old_lines: h.old_lines,
            new_start: h.new_start,
            new_lines: h.new_lines,
            lines: h
                .lines
                .into_iter()
                .map(|l| DiffLineDto {
                    origin: format!("{:?}", l.origin),
                    content: l.content,
                })
                .collect(),
        }
    }
}

fn worker_handle(state: &State<AppState>) -> Result<crate::worker::WorkerHandle, String> {
    let guard = state.worker.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .as_ref()
        .map(Worker::handle)
        .ok_or_else(|| "no repo open".to_string())
}

#[tauri::command]
pub fn get_log(limit: usize, state: State<AppState>) -> Result<Vec<CommitInfoDto>, String> {
    let commits = worker_handle(&state)?.get_log(limit)?;
    Ok(commits.into_iter().map(CommitInfoDto::from).collect())
}

#[tauri::command]
pub fn get_working_diff(
    path: String,
    staged: bool,
    state: State<AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    let hunks = worker_handle(&state)?.get_working_diff(path, staged)?;
    Ok(hunks.into_iter().map(DiffHunkDto::from).collect())
}

#[tauri::command]
pub fn get_commit_diff(
    commit_id: String,
    path: String,
    state: State<AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    let hunks = worker_handle(&state)?.get_commit_diff(commit_id, path)?;
    Ok(hunks.into_iter().map(DiffHunkDto::from).collect())
}

#[tauri::command]
pub fn get_commit_files(commit_id: String, state: State<AppState>) -> Result<Vec<String>, String> {
    worker_handle(&state)?.get_commit_files(commit_id)
}

#[tauri::command]
pub fn stage_file(path: String, state: State<AppState>) -> Result<(), String> {
    worker_handle(&state)?.stage_file(path)
}

#[tauri::command]
pub fn unstage_file(path: String, state: State<AppState>) -> Result<(), String> {
    worker_handle(&state)?.unstage_file(path)
}

#[tauri::command]
pub fn commit(message: String, state: State<AppState>) -> Result<String, String> {
    worker_handle(&state)?.commit(message)
}
```
`worker_handle` is a new private helper that factors out the "clone the handle, drop the lock"
pattern already inline in `get_status` — refactor `get_status` to call it too, so there's exactly
one place implementing that pattern:
```rust
#[tauri::command]
pub fn get_status(state: State<AppState>) -> Result<Vec<StatusEntryDto>, String> {
    let entries = worker_handle(&state)?.get_status()?;
    Ok(entries.into_iter().map(/* existing mapping */).collect())
}
```

**`crates/tauri-app/src/main.rs`** — extend the `invoke_handler` list:
```rust
use commands::{
    commit, get_commit_diff, get_commit_files, get_log, get_status, get_working_diff, open_repo,
    stage_file, unstage_file, AppState,
};

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            open_repo,
            get_status,
            get_log,
            get_working_diff,
            get_commit_diff,
            get_commit_files,
            stage_file,
            unstage_file,
            commit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## TDD requirement

Extend `crates/tauri-app/src/worker.rs`'s existing `#[cfg(test)] mod tests` (which already has
`get_status_reflects_an_untracked_file`/`spawn_fails_on_a_non_repository_path` using its own
local `init_repo`/`write_file` helpers — reuse those, don't duplicate):

- `get_log_reflects_a_commit`: spawn a `Worker` against a repo with one commit (use the test
  module's own helpers to create a repo, stage a file, and commit — write a local
  `commit_all`-equivalent in this test module if one doesn't already exist here, following
  `crates/git-core/tests/common/mod.rs`'s pattern), call `worker.handle().get_log(10)`, assert
  one entry whose `summary` matches.
- `stage_then_commit_round_trips_through_the_worker`: spawn against a fresh repo, `write_file` an
  untracked file, `worker.handle().stage_file("new.txt".into())`, then
  `worker.handle().commit("message".into())`, assert the result is `Ok(_)` and
  `worker.handle().get_status().unwrap().is_empty()` afterward.

Add `crates/tauri-app/src/commands.rs`'s existing `#[cfg(test)] mod tests` (which already pins
`StatusKind`'s wire format — keep that test):

- `diff_line_origin_wire_values_match_the_typescript_union`: same exhaustive-match pinning
  pattern as the existing `status_kind_wire_values_match_the_typescript_union` test, but for
  `git_core::diff::DiffLineOrigin`'s three variants against `"Add" | "Remove" | "Context"`.

Write these three tests first (referencing `Worker`/`WorkerHandle` methods and
`DiffLineOrigin` that don't exist yet), confirm compile failure with
`cargo test -p tauri-app`, then implement the `worker.rs`/`commands.rs`/`main.rs` changes above
and re-run until green.

## Acceptance criteria

- [ ] `cargo test -p tauri-app` passes (existing 3 tests + 3 new ones).
- [ ] `cargo build --workspace` succeeds.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo fmt --all -- --check` clean.
- [ ] Commit: `git add crates/tauri-app/src/worker.rs crates/tauri-app/src/commands.rs crates/tauri-app/src/main.rs && git commit -m "feat(tauri-app): expose log/diff/stage/commit as Tauri commands"`.

## Out of scope

The repo-picker commands (`pick_repo_folder`, `list_recent_repos`, `add_recent_repo`) — Task
1.C.02, separate because they don't go through the worker (no repo needs to be open yet).
Frontend consumption of these commands — Task 1.D.01.
