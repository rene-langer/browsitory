//! #448: revert or apply a commit's file or hunk in the working tree, save a patch, apply a
//! patch file.
use super::{off_runtime, repo_worker, AppState};
use repo_service::wire::{CommitChangeRequestDto, ErrorDto, PatchContentDto, PatchSourceDto};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

/// The largest patch file "Apply patch…" reads.
const MAX_PATCH_FILE_BYTES: u64 = 64 * 1024 * 1024;

#[tauri::command]
pub async fn apply_commit_change(
    repo_path: String,
    change: CommitChangeRequestDto,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.apply_commit_change(change.into()))
        .await
}

#[tauri::command]
pub async fn get_patch(
    repo_path: String,
    source: PatchSourceDto,
    state: State<'_, AppState>,
) -> Result<PatchContentDto, ErrorDto> {
    let bytes = repo_worker(&state, &repo_path)?
        .call(move |w| w.get_patch(source.into()))
        .await?;
    Ok(PatchContentDto::from_bytes(&bytes))
}

#[tauri::command]
pub async fn apply_patch(
    repo_path: String,
    content: PatchContentDto,
    state: State<'_, AppState>,
) -> Result<(), ErrorDto> {
    let bytes = content.into_bytes()?;
    repo_worker(&state, &repo_path)?
        .call(move |w| w.apply_patch(bytes))
        .await
}

/// Asks where to save `content` (offering `suggested_name`) and writes it there. The path saved
/// to, or `null` when the user cancelled.
#[tauri::command]
pub async fn save_patch_file(
    app: AppHandle,
    suggested_name: String,
    content: PatchContentDto,
) -> Result<Option<String>, ErrorDto> {
    let bytes = content.into_bytes()?;
    let Some(picked) = app
        .dialog()
        .file()
        .set_file_name(&suggested_name)
        .add_filter("Patch", &["patch", "diff"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(ErrorDto::from_error)?;
    let shown = path.to_string_lossy().into_owned();
    off_runtime(move || std::fs::write(&path, bytes))
        .await?
        .map_err(|e| format!("Couldn't save {shown}: {e}"))?;
    Ok(Some(shown))
}

/// Asks for a patch file and returns its content, or `null` when the user cancelled.
#[tauri::command]
pub async fn open_patch_file(app: AppHandle) -> Result<Option<PatchContentDto>, ErrorDto> {
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("Patch", &["patch", "diff", "mbox", "txt"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(ErrorDto::from_error)?;
    let shown = path.to_string_lossy().into_owned();
    let bytes = off_runtime(move || -> std::io::Result<Vec<u8>> {
        if std::fs::metadata(&path)?.len() > MAX_PATCH_FILE_BYTES {
            return Err(std::io::Error::other("the file is larger than 64 MB"));
        }
        std::fs::read(&path)
    })
    .await?
    .map_err(|e| format!("Couldn't read {shown}: {e}"))?;
    Ok(Some(PatchContentDto::from_bytes(&bytes)))
}
