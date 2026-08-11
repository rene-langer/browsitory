use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

use crate::worker::Worker;

#[derive(Serialize)]
pub struct StatusEntryDto {
    pub path: String,
    pub staged: bool,
    pub kind: String,
}

#[derive(Default)]
pub struct AppState {
    pub worker: Mutex<Option<Worker>>,
}

#[tauri::command]
pub fn open_repo(path: String, state: State<AppState>) -> Result<(), String> {
    let worker = Worker::spawn(PathBuf::from(path))?;
    *state.worker.lock().unwrap() = Some(worker);
    Ok(())
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> Result<Vec<StatusEntryDto>, String> {
    let guard = state.worker.lock().unwrap();
    let worker = guard.as_ref().ok_or_else(|| "no repo open".to_string())?;
    let entries = worker.get_status()?;
    Ok(entries
        .into_iter()
        .map(|e| StatusEntryDto {
            path: e.path,
            staged: e.staged,
            kind: format!("{:?}", e.kind),
        })
        .collect())
}
