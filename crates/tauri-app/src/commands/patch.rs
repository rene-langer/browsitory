//! #448: revert or apply a commit's file or hunk in the working tree, save a patch, apply a
//! patch file.
use super::{hook_event_channel, off_runtime, repo_worker, AppState};
use repo_service::wire::{
    CommitChangeRequestDto, ErrorDto, PatchApplyDto, PatchApplyOutcomeDto, PatchContentDto,
    PatchPreviewDto, PatchSourceDto,
};
use std::path::{Path, PathBuf};
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

/// As commits, `git am`'s hooks are reported as `hook-progress` events with
/// `source: "Patch"` (#747).
#[tauri::command]
pub async fn apply_patch(
    repo_path: String,
    content: PatchContentDto,
    how: PatchApplyDto,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PatchApplyOutcomeDto, ErrorDto> {
    let bytes = content.into_bytes()?;
    let events = hook_event_channel(app, &repo_path);
    Ok(PatchApplyOutcomeDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.apply_patch_reported(bytes, how.into(), events))
            .await?,
    ))
}

#[tauri::command]
pub async fn preview_patch(
    repo_path: String,
    content: PatchContentDto,
    state: State<'_, AppState>,
) -> Result<PatchPreviewDto, ErrorDto> {
    let bytes = content.into_bytes()?;
    let preview = repo_worker(&state, &repo_path)?
        .call(move |w| w.preview_patch(bytes))
        .await?;
    Ok(preview.into())
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
    read_patch_at(path).await.map(Some)
}

/// Reads a patch file dropped on the window (#649): its content. Only a file named like a patch
/// (`.patch`, `.diff`, `.mbox`, `.txt`, the "Apply patch…" dialog's filter) is read.
#[tauri::command]
pub async fn read_patch_file(path: String) -> Result<PatchContentDto, ErrorDto> {
    let path = PathBuf::from(path);
    if !is_patch_file_name(&path) {
        return Err(format!(
            "{} isn't a patch file (.patch, .diff, .mbox or .txt)",
            path.display()
        )
        .into());
    }
    read_patch_at(path).await
}

/// The extensions "Apply patch…" offers and a dropped file may have, in any case.
const PATCH_EXTENSIONS: [&str; 4] = ["patch", "diff", "mbox", "txt"];

fn is_patch_file_name(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            PATCH_EXTENSIONS
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

async fn read_patch_at(path: PathBuf) -> Result<PatchContentDto, ErrorDto> {
    let shown = path.to_string_lossy().into_owned();
    let bytes = off_runtime(move || -> std::io::Result<Vec<u8>> {
        if std::fs::metadata(&path)?.len() > MAX_PATCH_FILE_BYTES {
            return Err(std::io::Error::other("the file is larger than 64 MB"));
        }
        std::fs::read(&path)
    })
    .await?
    .map_err(|e| format!("Couldn't read {shown}: {e}"))?;
    Ok(PatchContentDto::from_bytes(&bytes))
}

#[cfg(test)]
mod tests {
    use super::is_patch_file_name;
    use std::path::Path;

    #[test]
    fn reads_only_files_named_like_a_patch() {
        for name in ["a.patch", "b.DIFF", "dir/c.mbox", "d.txt"] {
            assert!(is_patch_file_name(Path::new(name)), "{name}");
        }
        for name in ["repo", "a.patch.exe", "notes.md", ".patch", "a.patchx"] {
            assert!(!is_patch_file_name(Path::new(name)), "{name}");
        }
    }
}
