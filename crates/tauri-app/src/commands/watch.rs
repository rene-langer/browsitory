//! Watching the shown repositories for changes made outside the app (#756).

use std::sync::Arc;

use repo_service::watch::{RepoWatchers, WatchEvent};
use repo_service::wire::{repo_watch_dto, ErrorDto};
use tauri::{AppHandle, Emitter, State};

/// The Tauri event `repo_watch_dto` payloads are emitted as.
pub(super) const REPO_WATCH_EVENT: &str = "repo-watch";

/// The watchers of the repositories whose workspaces are mounted.
pub struct WatchState(Arc<RepoWatchers>);

impl WatchState {
    pub fn new(app: AppHandle) -> Self {
        Self(Arc::new(RepoWatchers::new(Arc::new(
            move |repo_path: &str, event| {
                if event == WatchEvent::Stopped {
                    log::warn!("Stopped watching {repo_path} for changes; polling instead");
                }
                let _ = app.emit(REPO_WATCH_EVENT, repo_watch_dto(repo_path, event));
            },
        ))))
    }

    pub(super) fn unwatch(&self, repo_path: &str) {
        self.0.unwatch(repo_path);
    }
}

/// Starts watching `repo_path` (a no-op when it is watched). Answers whether it is watched;
/// when not, the frontend polls the change stamp as often as before.
#[tauri::command]
pub async fn watch_repo(repo_path: String, watch: State<'_, WatchState>) -> Result<bool, ErrorDto> {
    // Walking a large work tree takes a while: off the async runtime's threads.
    let watchers = Arc::clone(&watch.0);
    let path = repo_path.clone();
    let watching = super::off_runtime(move || watchers.watch(&path)).await?;
    if !watching {
        log::info!("Not watching {repo_path} for changes; polling instead");
    }
    Ok(watching)
}

#[tauri::command]
pub async fn unwatch_repo(repo_path: String, watch: State<'_, WatchState>) -> Result<(), ErrorDto> {
    watch.unwatch(&repo_path);
    Ok(())
}
