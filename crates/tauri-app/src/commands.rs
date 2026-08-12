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
    // A poisoned lock is recoverable here: the worker thread never touches this mutex, so
    // the `Option<Worker>` behind it can't have been left half-updated.
    *state.worker.lock().unwrap_or_else(|e| e.into_inner()) = Some(worker);
    Ok(())
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> Result<Vec<StatusEntryDto>, String> {
    // Clone the worker's channel handle and drop the guard before blocking on the reply —
    // holding the mutex across the round-trip would serialize every command behind this one
    // and let a wedged worker hold the lock forever.
    let worker = {
        let guard = state.worker.lock().unwrap_or_else(|e| e.into_inner());
        guard
            .as_ref()
            .map(Worker::handle)
            .ok_or_else(|| "no repo open".to_string())?
    };
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

#[cfg(test)]
mod tests {
    use git_core::status::StatusKind;

    /// The `kind` field of `StatusEntryDto` is produced by `format!("{:?}", kind)`, so the
    /// `Debug` output *is* the wire format. Its counterpart contract is the `StatusKind`
    /// union in `frontend/src/ipc/RepoClient.ts`
    /// (`"New" | "Modified" | "Deleted" | "Renamed" | "TypeChange"`) — these must stay in
    /// sync. The match below is exhaustive on purpose: adding a `StatusKind` variant breaks
    /// compilation here, which is the reminder to extend the TypeScript union too.
    fn expected_wire_value(kind: StatusKind) -> &'static str {
        match kind {
            StatusKind::New => "New",
            StatusKind::Modified => "Modified",
            StatusKind::Deleted => "Deleted",
            StatusKind::Renamed => "Renamed",
            StatusKind::TypeChange => "TypeChange",
        }
    }

    #[test]
    fn status_kind_wire_values_match_the_typescript_union() {
        for kind in [
            StatusKind::New,
            StatusKind::Modified,
            StatusKind::Deleted,
            StatusKind::Renamed,
            StatusKind::TypeChange,
        ] {
            assert_eq!(format!("{:?}", kind), expected_wire_value(kind));
        }
    }
}
