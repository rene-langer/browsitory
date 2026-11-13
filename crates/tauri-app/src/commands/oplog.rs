//! The operation log (#888): `list_operations`, and the `operation-log` events that tell an open
//! log panel an entry was added or ended.

use std::sync::{Arc, OnceLock};

use repo_service::oplog;
use repo_service::wire::{ErrorDto, OperationLogChangeDto, OperationLogPageDto};
use tauri::{AppHandle, Emitter, State};

use super::AppState;

/// The Tauri event `OperationLogChangeDto` payloads are emitted as.
pub(super) const OPERATION_LOG_EVENT: &str = "operation-log";

/// The app's subscription to the logs' changes, made by the first `list_operations` (like the
/// sidecar's `operationLog` notifications): nothing is emitted before a log was looked at.
static SUBSCRIPTION: OnceLock<oplog::Subscription> = OnceLock::new();

fn emit_changes(app: &AppHandle) {
    SUBSCRIPTION.get_or_init(|| {
        let app = app.clone();
        oplog::subscribe(Arc::new(move |change: &oplog::Change| {
            let _ = app.emit(OPERATION_LOG_EVENT, OperationLogChangeDto::from(change));
        }))
    });
}

/// Up to `limit` entries of `repo_path`'s operation log older than entry `before` (from the
/// newest when `None`), newest first. Read straight from the log, never queued behind the
/// repository's running command.
#[tauri::command]
pub async fn list_operations(
    repo_path: String,
    before: Option<u64>,
    limit: usize,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<OperationLogPageDto, ErrorDto> {
    emit_changes(&app);
    let (_, handle) = state
        .registry
        .get(&repo_path)
        .ok_or_else(|| ErrorDto::from(format!("repo not open: {repo_path}").as_str()))?;
    Ok(OperationLogPageDto::from(
        handle.operation_log(before, limit),
    ))
}
