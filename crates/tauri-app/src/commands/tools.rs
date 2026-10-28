//! External tools on the desktop: the editor, file manager, terminal, and git's diff/merge
//! tools. Programs are spawned from Rust with argv arrays (never a shell, except a git tool
//! `cmd` from the user's own git config, which runs as `sh -c` exactly like git runs it). Paths
//! from the frontend are confined to the repository's work tree by `git_core::launch`.

use git_core::launch::{self, EditorTarget};
use tauri::State;

use super::{
    worker_handle, AppState, DiffSourceDto, ExternalToolSettingsDto, GitToolsDto,
    MergeToolOutcomeDto,
};

fn tool_settings() -> Result<config::ExternalToolSettings, String> {
    config::get_external_tool_settings().map_err(|e| e.to_string())
}

/// Opens a work tree file in the configured editor, or with the OS default app when none is
/// configured and the file can't run (`launch::editor_target` refuses runnable files).
#[tauri::command]
pub async fn open_in_editor(
    repo_path: String,
    path: String,
    line: Option<u32>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let workdir = worker_handle(&state, &repo_path)?.workdir()?;
    let editor = tool_settings()?.editor;
    match launch::prepare_editor(&workdir, &path, line, editor.as_deref())
        .map_err(|e| e.to_string())?
    {
        EditorTarget::Command(invocation) => {
            launch::run_invocation(&invocation, &workdir).map_err(|e| e.to_string())
        }
        EditorTarget::SystemDefault(file) => {
            tauri_plugin_opener::open_path(file, None::<&str>).map_err(|e| e.to_string())
        }
    }
}

/// Shows a file (or the work tree root for `null`) in the system file manager. A deleted file
/// reveals its nearest existing parent folder.
#[tauri::command]
pub async fn reveal_in_file_manager(
    repo_path: String,
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let workdir = worker_handle(&state, &repo_path)?.workdir()?;
    let target = launch::reveal_target(&workdir, path.as_deref()).map_err(|e| e.to_string())?;
    tauri_plugin_opener::reveal_item_in_dir(target).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_terminal(repo_path: String, state: State<'_, AppState>) -> Result<(), String> {
    let workdir = worker_handle(&state, &repo_path)?.workdir()?;
    let terminal = tool_settings()?.terminal;
    let invocation =
        launch::prepare_terminal(&workdir, terminal.as_deref()).map_err(|e| e.to_string())?;
    launch::run_invocation(&invocation, &workdir).map_err(|e| e.to_string())
}

/// Prepares on the repo worker, then launches here. Returns after the diff tool's 1 s grace
/// period at the latest.
#[tauri::command]
pub async fn open_diff_tool(
    repo_path: String,
    path: String,
    source: DiffSourceDto,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let prepared = worker_handle(&state, &repo_path)?.prepare_diff_tool(path, source.into())?;
    tauri::async_runtime::spawn_blocking(move || {
        git_core::tools::launch_diff_tool(prepared, git_core::tools::DIFF_TOOL_GRACE)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// Waits for the merge tool on a blocking thread, never on the repo worker, which stays free
/// for other commands while the tool window is open.
#[tauri::command]
pub async fn open_merge_tool(
    repo_path: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<MergeToolOutcomeDto, String> {
    let prepared = worker_handle(&state, &repo_path)?.prepare_merge_tool(path)?;
    let outcome =
        tauri::async_runtime::spawn_blocking(move || git_core::tools::run_merge_tool(prepared))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
    Ok(outcome.into())
}

#[tauri::command]
pub fn get_external_tool_settings() -> Result<ExternalToolSettingsDto, String> {
    tool_settings().map(Into::into)
}

#[tauri::command]
pub fn set_external_tool_settings(settings: ExternalToolSettingsDto) -> Result<(), String> {
    launch::check_tool_commands(settings.editor.as_deref(), settings.terminal.as_deref())?;
    config::set_external_tool_settings(&settings.into()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_git_tools(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<GitToolsDto, String> {
    Ok(worker_handle(&state, &repo_path)?.get_git_tools()?.into())
}
