//! Letting running repository writes finish before the process exits (#965, #1131).

use tauri::{AppHandle, Runtime};

pub fn wait_for_writes_at_exit<R: Runtime>(_app: &AppHandle<R>) {}

#[cfg(test)]
mod tests {
    use tauri::Manager;

    use crate::commands::AppState;

    fn repo() -> tempfile::TempDir {
        let dir = tempfile::TempDir::new().unwrap();
        git2::Repository::init(dir.path()).unwrap();
        dir
    }

    /// The updater's install on Windows ends the process with `std::process::exit` after only
    /// `AppHandle::cleanup_before_exit`, delivering no `RunEvent::Exit` (#1131). That cleanup
    /// alone stops the workers, so each finishes its command and none starts another.
    #[test]
    fn the_cleanup_before_an_exit_stops_the_workers() {
        let app = tauri::test::mock_builder()
            .manage(AppState::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        super::wait_for_writes_at_exit(app.handle());
        let (first, second) = (repo(), repo());
        let registry = &app.state::<AppState>().registry;
        registry
            .open(first.path().to_str().unwrap())
            .expect("a repository opens before the exit");

        app.handle().cleanup_before_exit();

        assert!(
            registry.open(second.path().to_str().unwrap()).is_err(),
            "a repository still opens after the cleanup before an exit"
        );
    }
}
