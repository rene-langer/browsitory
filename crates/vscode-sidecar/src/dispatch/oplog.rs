//! The operation log (#888): `list_operations`, and the `operationLog` notifications that tell
//! an open log panel an entry was added or ended.

use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock};

use repo_service::oplog;
use repo_service::wire::{ErrorDto, OperationLogChangeDto, OperationLogPageDto};
use serde::Deserialize;
use serde_json::Value;

use super::{worker_handle, Repos};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListOperationsParams {
    repo_path: String,
    before: Option<u64>,
    limit: usize,
}

/// The sidecar's subscription to the logs' changes, made by the first `list_operations`, which
/// hands over the stdout the notifications are written to: nothing is sent before a log was
/// looked at.
static SUBSCRIPTION: OnceLock<oplog::Subscription> = OnceLock::new();

fn notify_changes(stdout: &Arc<Mutex<std::io::Stdout>>) {
    SUBSCRIPTION.get_or_init(|| {
        let out = Arc::clone(stdout);
        oplog::subscribe(Arc::new(move |change: &oplog::Change| {
            write_operation_log(&out, change);
        }))
    });
}

/// Writes one `operationLog` notification line, under `out`'s lock like every other line.
pub(super) fn write_operation_log<W: Write>(out: &Mutex<W>, change: &oplog::Change) {
    let notification = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "operationLog",
        "params": OperationLogChangeDto::from(change),
    });
    let Ok(line) = serde_json::to_string(&notification) else {
        return;
    };
    let mut out = out.lock().unwrap_or_else(|error| error.into_inner());
    // A failed write means the extension host is gone; the main loop exits on its next answer.
    let _ = writeln!(out, "{line}").and_then(|()| out.flush());
}

/// Up to `limit` entries of the repository's operation log older than entry `before` (from the
/// newest when `null`), newest first. Read straight from the log, never queued behind the
/// repository's running command.
pub(super) fn list_operations(
    params: ListOperationsParams,
    repos: &Repos,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, ErrorDto> {
    let handle = worker_handle(repos, &params.repo_path)?;
    notify_changes(stdout);
    let page = handle.operation_log(params.before, params.limit);
    serde_json::to_value(OperationLogPageDto::from(page)).map_err(ErrorDto::from_error)
}
