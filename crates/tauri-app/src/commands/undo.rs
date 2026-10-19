use super::{worker_handle, AppState, UndoOutcomeDto, UndoStateDto};
use tauri::State;

#[tauri::command]
pub async fn get_undo_state(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<UndoStateDto, String> {
    Ok(UndoStateDto::from(
        worker_handle(&state, &repo_path)?.get_undo_state()?,
    ))
}

#[tauri::command]
pub async fn undo_last(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<UndoOutcomeDto, String> {
    Ok(UndoOutcomeDto::from(
        worker_handle(&state, &repo_path)?.undo()?,
    ))
}

#[tauri::command]
pub async fn redo_last(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<UndoOutcomeDto, String> {
    Ok(UndoOutcomeDto::from(
        worker_handle(&state, &repo_path)?.redo()?,
    ))
}
