//! Watching the shown repositories for changes made outside the app (#756): `watch_repo` and
//! `unwatch_repo`, and the `repoWatch` notifications the watchers send.

use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock};

use repo_service::watch::{RepoWatchers, WatchEvent};
use repo_service::wire::{repo_watch_dto, ErrorDto};
use serde_json::Value;

use super::RepoPathParams;

/// The process's watchers. Made by the first `watch_repo`, which hands over the stdout the
/// notifications are written to (the same one every request answers on).
static WATCHERS: OnceLock<RepoWatchers> = OnceLock::new();

fn watchers(stdout: &Arc<Mutex<std::io::Stdout>>) -> &'static RepoWatchers {
    WATCHERS.get_or_init(|| {
        let out = Arc::clone(stdout);
        RepoWatchers::new(Arc::new(move |repo_path: &str, event| {
            if event == WatchEvent::Stopped {
                eprintln!(
                    "vscode-sidecar: stopped watching {repo_path} for changes; polling instead"
                );
            }
            write_repo_watch(&out, repo_path, event);
        }))
    })
}

/// Writes one `repoWatch` notification line, under `out`'s lock like every other line.
pub(super) fn write_repo_watch<W: Write>(out: &Mutex<W>, repo_path: &str, event: WatchEvent) {
    let notification = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "repoWatch",
        "params": repo_watch_dto(repo_path, event),
    });
    let Ok(line) = serde_json::to_string(&notification) else {
        return;
    };
    let mut out = out.lock().unwrap_or_else(|error| error.into_inner());
    // A failed write means the extension host is gone; the main loop exits on its next answer.
    let _ = writeln!(out, "{line}").and_then(|()| out.flush());
}

/// Answers whether `repoPath` is watched; when not, the frontend polls as often as before.
pub(super) fn watch_repo(
    params: RepoPathParams,
    stdout: &Arc<Mutex<std::io::Stdout>>,
) -> Result<Value, ErrorDto> {
    Ok(Value::Bool(watchers(stdout).watch(&params.repo_path)))
}

pub(super) fn unwatch_repo(params: RepoPathParams) -> Result<Value, ErrorDto> {
    stop_watching(&params.repo_path);
    Ok(Value::Null)
}

/// Stops watching `repo_path`, if anything watches it (`close_repo` too).
pub(super) fn stop_watching(repo_path: &str) {
    if let Some(watchers) = WATCHERS.get() {
        watchers.unwatch(repo_path);
    }
}
