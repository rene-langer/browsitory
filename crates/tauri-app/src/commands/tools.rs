//! External tools on the desktop: the editor, file manager, terminal, and git's diff/merge
//! tools. Programs are spawned from Rust with argv arrays (never a shell, except a git tool
//! `cmd` from the user's own git config, which runs as `sh -c` exactly like git runs it). Paths
//! from the frontend are confined to the repository's work tree by `git_core::launch`.

use repo_service::wire::ErrorDto;
use std::path::PathBuf;

use git_core::launch::{self, EditorTarget};
use tauri::State;

use super::{off_runtime, repo_worker, AppState, MergeToolOutcomeDto};
use repo_service::wire::{DiffSourceDto, ExternalToolSettingsDto, FileSidesDto, GitToolsDto};

/// The repository's work tree root. A stored path, so it skips the repository's gate: Open in
/// editor, Reveal and Open terminal answer at once even while a long hook holds it (#181).
pub(crate) fn workdir_of(state: &AppState, repo_path: &str) -> Result<PathBuf, ErrorDto> {
    repo_worker(state, repo_path)?.signal().workdir()
}

fn tool_settings() -> Result<config::ExternalToolSettings, ErrorDto> {
    config::get_external_tool_settings().map_err(ErrorDto::from_error)
}

/// Opens a work tree file in the configured editor, or with the OS default app when none is
/// configured and the file can't run (`launch::editor_target` refuses runnable files).
#[tauri::command]
pub async fn open_in_editor(
    repo_path: String,
    path: String,
    line: Option<u32>,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    let workdir = workdir_of(&state, &repo_path)?;
    let editor = tool_settings()?.editor;
    match launch::prepare_editor(&workdir, &path, line, editor.as_deref())
        .map_err(ErrorDto::from_error)?
    {
        EditorTarget::Command(invocation) => {
            Ok(launch::run_invocation(&invocation, &workdir).map_err(ErrorDto::from_error)?)
        }
        EditorTarget::SystemDefault(file) => {
            Ok(tauri_plugin_opener::open_path(file, None::<&str>).map_err(ErrorDto::from_error)?)
        }
    }
}

/// Which of `.gitignore` and `.gitattributes` exist at the work tree root; `None` for a bare
/// repository (#624). Like `workdir_of`, answers at once without the repository's gate.
#[tauri::command]
pub async fn get_root_files(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Option<Vec<String>>, ErrorDto> {
    Ok(repo_worker(&state, &repo_path)?.signal().root_files())
}

/// Creates a missing `.gitignore` or `.gitattributes` at the work tree root, empty (#624).
#[tauri::command]
pub async fn create_root_file(
    repo_path: String,
    name: String,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.create_root_file(name))
        .await
}

/// Shows a file (or the work tree root for `null`) in the system file manager. A deleted file
/// reveals its nearest existing parent folder.
#[tauri::command]
pub async fn reveal_in_file_manager(
    repo_path: String,
    path: Option<String>,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    let workdir = workdir_of(&state, &repo_path)?;
    let target = launch::reveal_target(&workdir, path.as_deref()).map_err(ErrorDto::from_error)?;
    tauri_plugin_opener::reveal_item_in_dir(target).map_err(ErrorDto::from_error)
}

#[tauri::command]
pub async fn open_terminal(repo_path: String, state: State<'_, AppState>) -> Result<(), ErrorDto> {
    let workdir = workdir_of(&state, &repo_path)?;
    let terminal = tool_settings()?.terminal;
    let invocation =
        launch::prepare_terminal(&workdir, terminal.as_deref()).map_err(ErrorDto::from_error)?;
    launch::run_invocation(&invocation, &workdir).map_err(ErrorDto::from_error)
}

/// Both sides of `path` in `source`, for the image preview (each capped at 20 MB).
#[tauri::command]
pub async fn get_file_sides(
    repo_path: String,
    path: String,
    source: DiffSourceDto,
    state: State<'_, AppState>,
) -> Result<FileSidesDto, ErrorDto> {
    Ok(FileSidesDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.get_file_sides(path, source.into()))
            .await?,
    ))
}

/// Prepares on the repo worker, then launches here. Returns after the diff tool's 1 s grace
/// period at the latest.
#[tauri::command]
pub async fn open_diff_tool(
    repo_path: String,
    path: String,
    source: DiffSourceDto,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    let prepared = repo_worker(&state, &repo_path)?
        .call(move |w| w.prepare_diff_tool(path, source.into()))
        .await?;
    off_runtime(move || {
        git_core::tools::launch_diff_tool(prepared, git_core::tools::DIFF_TOOL_GRACE)
    })
    .await?
    .map_err(ErrorDto::from_error)
}

/// Waits for the merge tool on a blocking thread, never on the repo worker, which stays free
/// for other commands while the tool window is open.
#[tauri::command]
pub async fn open_merge_tool(
    repo_path: String,
    path: String,
    state: State<'_, AppState>,
) -> Result<MergeToolOutcomeDto, ErrorDto> {
    let prepared = repo_worker(&state, &repo_path)?
        .call(move |w| w.prepare_merge_tool(path))
        .await?;
    let outcome = off_runtime(move || git_core::tools::run_merge_tool(prepared))
        .await?
        .map_err(ErrorDto::from_error)?;
    Ok(outcome.into())
}

#[tauri::command]
pub fn get_external_tool_settings() -> Result<ExternalToolSettingsDto, ErrorDto> {
    tool_settings().map(Into::into)
}

#[tauri::command]
pub fn set_external_tool_settings(settings: ExternalToolSettingsDto) -> Result<(), ErrorDto> {
    launch::check_tool_commands(settings.editor.as_deref(), settings.terminal.as_deref())?;
    config::set_external_tool_settings(&settings.into()).map_err(ErrorDto::from_error)
}

#[tauri::command]
pub async fn get_git_tools(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<GitToolsDto, ErrorDto> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_git_tools())
        .await?
        .into())
}
