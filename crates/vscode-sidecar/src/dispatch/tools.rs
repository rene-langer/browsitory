use repo_service::wire::{DiffSourceDto, ExternalToolSettingsDto, FileSidesDto, GitToolsDto};
use serde::Deserialize;
use serde_json::Value;

use super::{worker_handle, RepoPathParams, Repos};

#[derive(Deserialize)]
pub(super) struct SetExternalToolSettingsParams {
    settings: ExternalToolSettingsDto,
}

pub(super) fn get_external_tool_settings() -> Result<Value, String> {
    let settings = config::get_external_tool_settings().map_err(|error| error.to_string())?;
    serde_json::to_value(ExternalToolSettingsDto::from(settings)).map_err(|error| error.to_string())
}

pub(super) fn set_external_tool_settings(
    params: SetExternalToolSettingsParams,
) -> Result<Value, String> {
    git_core::launch::check_tool_commands(
        params.settings.editor.as_deref(),
        params.settings.terminal.as_deref(),
    )?;
    config::set_external_tool_settings(&params.settings.into())
        .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

pub(super) fn get_git_tools(params: RepoPathParams, repos: &Repos) -> Result<Value, String> {
    let tools = worker_handle(repos, &params.repo_path)?.get_git_tools()?;
    serde_json::to_value(GitToolsDto::from(tools)).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(rename = "FileVersion"))]
#[serde(tag = "kind")]
pub(super) enum FileVersionDto {
    Head,
    Index,
    Stage {
        stage: u8,
    },
    #[serde(rename_all = "camelCase")]
    Commit {
        commit_id: String,
    },
    #[serde(rename_all = "camelCase")]
    FirstParent {
        commit_id: String,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ReadFileVersionParams {
    repo_path: String,
    path: String,
    version: FileVersionDto,
}

/// Internal: not on `RepoClient`. Called by the extension host's `browsitory-file:` content
/// provider (diff and merge editor sides). Contents are lossy UTF-8, capped at
/// `MAX_FILE_VERSION_BYTES`; `text: null` when the path doesn't exist in that version.
pub(super) fn read_file_version(
    params: ReadFileVersionParams,
    repos: &Repos,
) -> Result<Value, String> {
    use git_core::tools::FileVersion;
    let version = match params.version {
        FileVersionDto::Head => FileVersion::Head,
        FileVersionDto::Index => FileVersion::Index,
        FileVersionDto::Stage { stage } if (1..=3).contains(&stage) => FileVersion::Stage(stage),
        FileVersionDto::Stage { .. } => return Err("stage must be 1, 2 or 3".into()),
        FileVersionDto::Commit { commit_id } => FileVersion::Commit(commit_id),
        FileVersionDto::FirstParent { commit_id } => FileVersion::FirstParent(commit_id),
    };
    let bytes = worker_handle(repos, &params.repo_path)?.read_file_version(params.path, version)?;
    Ok(serde_json::json!({ "text": bytes.map(|b| String::from_utf8_lossy(&b).into_owned()) }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RepoFileParams {
    repo_path: String,
    path: String,
}

/// Internal: not on `RepoClient`. Called by the extension host before it opens VSCode's merge
/// editor, with the same checks as the desktop merge tool: `null`, or an error saying why the
/// path can't be merged there.
pub(super) fn check_merge_target(params: RepoFileParams, repos: &Repos) -> Result<Value, String> {
    worker_handle(repos, &params.repo_path)?.check_merge_target(params.path)?;
    Ok(Value::Null)
}

/// Internal: not on `RepoClient`. The work tree root for the extension host's native launch
/// methods (the repo path may be a subdirectory of it).
pub(super) fn repo_workdir(params: RepoPathParams, repos: &Repos) -> Result<Value, String> {
    let workdir = worker_handle(repos, &params.repo_path)?.workdir()?;
    Ok(Value::String(workdir.to_string_lossy().into_owned()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct FileSidesParams {
    repo_path: String,
    path: String,
    source: DiffSourceDto,
}

pub(super) fn get_file_sides(params: FileSidesParams, repos: &Repos) -> Result<Value, String> {
    let sides = worker_handle(repos, &params.repo_path)?
        .get_file_sides(params.path, params.source.into())?;
    serde_json::to_value(FileSidesDto::from(sides)).map_err(|error| error.to_string())
}
