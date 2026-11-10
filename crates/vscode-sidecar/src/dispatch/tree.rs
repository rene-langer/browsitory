//! A commit's file tree (#446): browse it, read a file at that commit, restore it to the work
//! tree, and (for the extension host's "Save as…") write it to a picked path.

use repo_service::wire::ErrorDto;
use repo_service::wire::{FileContentDto, TreeListingDto};
use serde::Deserialize;
use serde_json::Value;

use super::{worker_handle, Repos};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct GetCommitTreeParams {
    repo_path: String,
    commit_id: String,
    dir: String,
    filter: Option<String>,
}

pub(super) fn get_commit_tree(
    params: GetCommitTreeParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let listing = worker_handle(repos, &params.repo_path)?.get_commit_tree(
        params.commit_id,
        params.dir,
        params.filter,
    )?;
    serde_json::to_value(TreeListingDto::from(listing)).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CommitFileParams {
    repo_path: String,
    commit_id: String,
    path: String,
}

pub(super) fn get_file_at_commit(
    params: CommitFileParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let content = worker_handle(repos, &params.repo_path)?
        .get_file_at_commit(params.commit_id, params.path)?;
    serde_json::to_value(FileContentDto::from(content)).map_err(ErrorDto::from_error)
}

pub(super) fn restore_file_version(
    params: CommitFileParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    worker_handle(repos, &params.repo_path)?.restore_file_version(params.commit_id, params.path)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WriteFileVersionParams {
    repo_path: String,
    commit_id: String,
    path: String,
    /// Absolute: the path the user picked in VSCode's save dialog.
    destination: String,
}

/// Internal: not on `RepoClient`. The extension host's native `save_file_version` asks where to
/// save, then calls this with the picked path; the webview may not call it.
pub(super) fn write_file_version(
    params: WriteFileVersionParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let destination = std::path::PathBuf::from(params.destination);
    if !destination.is_absolute() {
        return Err("the save destination must be an absolute path".into());
    }
    worker_handle(repos, &params.repo_path)?.save_file_version(
        params.commit_id,
        params.path,
        destination,
    )?;
    Ok(Value::Null)
}
