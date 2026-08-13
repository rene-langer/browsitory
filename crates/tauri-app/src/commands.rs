use std::path::{Path, PathBuf};
use std::sync::Mutex;

use git_core::diff::DiffHunk;
use serde::Serialize;
use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::worker::Worker;

#[derive(Serialize)]
pub struct StatusEntryDto {
    pub path: String,
    pub staged: bool,
    pub kind: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BranchInfoDto {
    pub name: String,
    pub is_current: bool,
}

impl From<git_core::branch::BranchInfo> for BranchInfoDto {
    fn from(b: git_core::branch::BranchInfo) -> Self {
        BranchInfoDto {
            name: b.name,
            is_current: b.is_current,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StashEntryDto {
    pub index: usize,
    pub message: String,
    pub commit_id: String,
}

impl From<git_core::stash::StashEntry> for StashEntryDto {
    fn from(s: git_core::stash::StashEntry) -> Self {
        StashEntryDto {
            index: s.index,
            message: s.message,
            commit_id: s.commit_id,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlameLineDto {
    pub line_number: usize,
    pub content: String,
    pub commit_id: String,
    pub short_id: String,
    pub author_name: String,
    pub timestamp: i64,
}

impl From<git_core::blame::BlameLine> for BlameLineDto {
    fn from(l: git_core::blame::BlameLine) -> Self {
        BlameLineDto {
            line_number: l.line_number,
            content: l.content,
            commit_id: l.commit_id,
            short_id: l.short_id,
            author_name: l.author_name,
            timestamp: l.timestamp,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphCommitDto {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub parent_ids: Vec<String>,
    pub branch_refs: Vec<String>,
}

impl From<git_core::graph::GraphCommit> for GraphCommitDto {
    fn from(c: git_core::graph::GraphCommit) -> Self {
        GraphCommitDto {
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

#[derive(Default)]
pub struct AppState {
    pub worker: Mutex<Option<Worker>>,
}

#[tauri::command]
pub async fn open_repo(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let worker = Worker::spawn(PathBuf::from(&path))?;
    // A poisoned lock is recoverable here: the worker thread never touches this mutex, so
    // the `Option<Worker>` behind it can't have been left half-updated.
    *state.worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(worker);
    // Best-effort: a repo that opened successfully should count as "recent" even if we can't
    // persist that fact (e.g. an unwritable config dir) — don't fail the whole open_repo call
    // over it.
    let _ = config::add_recent_repo(Path::new(&path));
    Ok(())
}

#[tauri::command]
pub async fn pick_repo_folder(app: tauri::AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .blocking_pick_folder()
        .map(|path| path.to_string())
}

#[tauri::command]
pub fn list_recent_repos() -> Result<Vec<String>, String> {
    config::list_recent_repos()
        .map(|paths| {
            paths
                .into_iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        })
        .map_err(|e| e.to_string())
}

// Clone the worker's channel handle and drop the guard before blocking on the reply —
// holding the mutex across the round-trip would serialize every command behind this one
// and let a wedged worker hold the lock forever.
fn worker_handle(state: &State<AppState>) -> Result<crate::worker::WorkerHandle, String> {
    let guard = state.worker.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .as_ref()
        .map(Worker::handle)
        .ok_or_else(|| "no repo open".to_string())
}

#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> Result<Vec<StatusEntryDto>, String> {
    let entries = worker_handle(&state)?.get_status()?;
    Ok(entries
        .into_iter()
        .map(|e| StatusEntryDto {
            path: e.path,
            staged: e.staged,
            kind: format!("{:?}", e.kind),
        })
        .collect())
}

#[tauri::command]
pub async fn get_commit_graph(
    limit: usize,
    state: State<'_, AppState>,
) -> Result<Vec<GraphCommitDto>, String> {
    let commits = worker_handle(&state)?.get_commit_graph(limit)?;
    Ok(commits.into_iter().map(GraphCommitDto::from).collect())
}

#[tauri::command]
pub async fn get_working_diff(
    path: String,
    staged: bool,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    let hunks = worker_handle(&state)?.get_working_diff(path, staged)?;
    Ok(hunks.into_iter().map(DiffHunkDto::from).collect())
}

#[tauri::command]
pub async fn get_commit_diff(
    commit_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    let hunks = worker_handle(&state)?.get_commit_diff(commit_id, path)?;
    Ok(hunks.into_iter().map(DiffHunkDto::from).collect())
}

#[tauri::command]
pub async fn get_commit_files(
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    worker_handle(&state)?.get_commit_files(commit_id)
}

#[tauri::command]
pub async fn get_blame(
    commit_id: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<BlameLineDto>, String> {
    let lines = worker_handle(&state)?.get_blame(commit_id, path)?;
    Ok(lines.into_iter().map(BlameLineDto::from).collect())
}

#[tauri::command]
pub async fn stage_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.stage_file(path)
}

#[tauri::command]
pub async fn unstage_file(path: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.unstage_file(path)
}

#[tauri::command]
pub async fn commit(message: String, state: State<'_, AppState>) -> Result<String, String> {
    worker_handle(&state)?.commit(message)
}

#[tauri::command]
pub async fn list_branches(state: State<'_, AppState>) -> Result<Vec<BranchInfoDto>, String> {
    let branches = worker_handle(&state)?.list_branches()?;
    Ok(branches.into_iter().map(BranchInfoDto::from).collect())
}

#[tauri::command]
pub async fn create_branch(
    name: String,
    start_point: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.create_branch(name, start_point)
}

#[tauri::command]
pub async fn switch_branch(name: String, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.switch_branch(name)
}

#[tauri::command]
pub async fn delete_branch(
    name: String,
    force: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.delete_branch(name, force)
}

#[tauri::command]
pub async fn rename_branch(
    old_name: String,
    new_name: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    worker_handle(&state)?.rename_branch(old_name, new_name)
}

#[tauri::command]
pub async fn list_stashes(state: State<'_, AppState>) -> Result<Vec<StashEntryDto>, String> {
    let stashes = worker_handle(&state)?.list_stashes()?;
    Ok(stashes.into_iter().map(StashEntryDto::from).collect())
}

#[tauri::command]
pub async fn save_stash(state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.save_stash()
}

#[tauri::command]
pub async fn apply_stash(index: usize, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.apply_stash(index)
}

#[tauri::command]
pub async fn drop_stash(index: usize, state: State<'_, AppState>) -> Result<(), String> {
    worker_handle(&state)?.drop_stash(index)
}

#[cfg(test)]
mod tests {
    use git_core::diff::DiffLineOrigin;
    use git_core::status::StatusKind;

    /// The `kind` field of `StatusEntryDto` is produced by `format!("{:?}", kind)`, so the
    /// `Debug` output *is* the wire format. Its counterpart contract is the `StatusKind`
    /// union in `frontend/src/ipc/RepoClient.ts`
    /// (`"New" | "Modified" | "Deleted" | "Renamed" | "TypeChange"`) — these must stay in
    /// sync. The match below is exhaustive on purpose: adding a `StatusKind` variant breaks
    /// compilation here, which is the reminder to extend the TypeScript union too.
    fn expected_wire_value(kind: StatusKind) -> &'static str {
        match kind {
            StatusKind::New => "New",
            StatusKind::Modified => "Modified",
            StatusKind::Deleted => "Deleted",
            StatusKind::Renamed => "Renamed",
            StatusKind::TypeChange => "TypeChange",
        }
    }

    #[test]
    fn status_kind_wire_values_match_the_typescript_union() {
        for kind in [
            StatusKind::New,
            StatusKind::Modified,
            StatusKind::Deleted,
            StatusKind::Renamed,
            StatusKind::TypeChange,
        ] {
            assert_eq!(format!("{:?}", kind), expected_wire_value(kind));
        }
    }

    /// `DiffLineDto::origin` is produced by `format!("{:?}", origin)`, so the `Debug`
    /// output *is* the wire format. Counterpart contract: the `DiffLineOrigin` union in
    /// `frontend/src/ipc/RepoClient.ts` (`"Add" | "Remove" | "Context"`) — these must stay
    /// in sync. Exhaustive on purpose: adding a `DiffLineOrigin` variant breaks compilation
    /// here, which is the reminder to extend the TypeScript union too.
    fn expected_diff_origin_wire_value(origin: DiffLineOrigin) -> &'static str {
        match origin {
            DiffLineOrigin::Add => "Add",
            DiffLineOrigin::Remove => "Remove",
            DiffLineOrigin::Context => "Context",
        }
    }

    #[test]
    fn diff_line_origin_wire_values_match_the_typescript_union() {
        for origin in [
            DiffLineOrigin::Add,
            DiffLineOrigin::Remove,
            DiffLineOrigin::Context,
        ] {
            assert_eq!(
                format!("{:?}", origin),
                expected_diff_origin_wire_value(origin)
            );
        }
    }
}
