use repo_service::wire::{OpenRepoEntryDto, OpenRepoEntryInput, WorkspaceDto};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

use super::{RepoPathParams, Repos};

#[derive(Deserialize)]
pub(super) struct OpenRepoParams {
    path: String,
}

pub(super) fn open_repo(params: OpenRepoParams, repos: &Repos) -> Result<Value, String> {
    // `WorkerRegistry::open` doesn't hold its lock across `Worker::spawn`, and evicts a dead
    // worker (its thread having exited, e.g. from a panic inside a `Command` match arm) before
    // deciding whether to reuse or respawn — a plain `contains_key`/`Entry::Vacant` fast path
    // would keep handing back a dead entry forever. See AUD-2026-09-05-CONC-001. A second
    // spelling of an open repository shares its worker (#223).
    repos.open(&params.path)?;
    let _ = config::add_recent_repo(Path::new(&params.path));
    Ok(Value::Null)
}

pub(super) fn close_repo(params: RepoPathParams, repos: &Repos) -> Result<Value, String> {
    // Drops the worker only when no other spelling of the repository is still open.
    repos.close(&params.repo_path);
    Ok(Value::Null)
}

pub(super) fn list_recent_repos() -> Result<Value, String> {
    let paths = config::list_recent_repos().map_err(|error| error.to_string())?;
    let paths: Vec<String> = paths
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    serde_json::to_value(paths).map_err(|error| error.to_string())
}

/// `list_open_repos`'s answer. (The desktop app's command returns an `(entries, activePath)`
/// tuple instead, which `tauriRepoClient.ts` turns into this shape.)
#[derive(Serialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "OpenRepos"))]
#[serde(rename_all = "camelCase")]
pub(super) struct OpenReposDto {
    entries: Vec<OpenRepoEntryDto>,
    active_path: Option<String>,
}

/// `config::list_open_repos`'s `(entries, active path)`.
impl From<(Vec<config::OpenRepoEntry>, Option<PathBuf>)> for OpenReposDto {
    fn from((entries, active): (Vec<config::OpenRepoEntry>, Option<PathBuf>)) -> Self {
        Self {
            entries: entries.into_iter().map(OpenRepoEntryDto::from).collect(),
            active_path: active.map(|p| p.to_string_lossy().into_owned()),
        }
    }
}

pub(super) fn list_open_repos_handler() -> Result<Value, String> {
    let open = config::list_open_repos().map_err(|error| error.to_string())?;
    serde_json::to_value(OpenReposDto::from(open)).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PersistOpenReposParams {
    entries: Vec<OpenRepoEntryInput>,
    active_path: Option<String>,
}

pub(super) fn persist_open_repos(params: PersistOpenReposParams) -> Result<Value, String> {
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
pub(super) struct ScanReposInRootParams {
    root: String,
}

pub(super) fn scan_repos_in_root(params: ScanReposInRootParams) -> Result<Value, String> {
    let paths =
        config::scan_repos_in_root(Path::new(&params.root)).map_err(|error| error.to_string())?;
    let paths: Vec<String> = paths
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    serde_json::to_value(paths).map_err(|error| error.to_string())
}

pub(super) fn list_workspaces_handler() -> Result<Value, String> {
    let workspaces = config::list_workspaces().map_err(|error| error.to_string())?;
    let workspaces: Vec<WorkspaceDto> = workspaces.into_iter().map(WorkspaceDto::from).collect();
    serde_json::to_value(workspaces).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
pub(super) struct SaveWorkspaceParams {
    name: String,
    root: String,
    members: Vec<String>,
}

pub(super) fn save_workspace(params: SaveWorkspaceParams) -> Result<Value, String> {
    let members: Vec<PathBuf> = params.members.into_iter().map(PathBuf::from).collect();
    let id = config::save_workspace(&params.name, Path::new(&params.root), &members)
        .map_err(|error| error.to_string())?;
    Ok(Value::String(id))
}

#[derive(Deserialize)]
pub(super) struct UpdateWorkspaceParams {
    id: String,
    name: String,
    members: Vec<String>,
}

pub(super) fn update_workspace(params: UpdateWorkspaceParams) -> Result<Value, String> {
    let members: Vec<PathBuf> = params.members.into_iter().map(PathBuf::from).collect();
    config::update_workspace(&params.id, &params.name, &members)
        .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
pub(super) struct DeleteWorkspaceParams {
    id: String,
}

pub(super) fn delete_workspace(params: DeleteWorkspaceParams) -> Result<Value, String> {
    config::delete_workspace(&params.id).map_err(|error| error.to_string())?;
    Ok(Value::Null)
}
