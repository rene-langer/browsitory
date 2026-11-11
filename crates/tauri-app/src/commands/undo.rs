use super::{repo_worker, AppState};
use repo_service::wire::ErrorDto;
use repo_service::wire::{UndoOutcomeDto, UndoStateDto};
use tauri::State;

#[tauri::command]
pub async fn get_undo_state(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<UndoStateDto, ErrorDto> {
    Ok(UndoStateDto::from(
        repo_worker(&state, &repo_path)?
            .read(move |w| w.get_undo_state())
            .await?,
    ))
}

#[tauri::command]
pub async fn undo_last(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<UndoOutcomeDto, ErrorDto> {
    Ok(UndoOutcomeDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.undo())
            .await?,
    ))
}

#[tauri::command]
pub async fn redo_last(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<UndoOutcomeDto, ErrorDto> {
    Ok(UndoOutcomeDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.redo())
            .await?,
    ))
}
