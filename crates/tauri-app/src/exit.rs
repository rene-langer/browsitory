//! Letting running repository writes finish before the process exits (#965, #1131).
//!
//! Exiting at once cut a worker off wherever it was: in the middle of a libgit2 write that leaves
//! `.git/index.lock` behind, and every later git command in that repository fails on it. So the
//! workers finish the command they run, start no other, and the exit waits for them up to
//! `EXIT_GRACE`.
//!
//! Every exit runs `AppHandle::cleanup_before_exit` just before the process ends, which empties
//! the app's resource table: after the `RunEvent::Exit` handler when the last window closes, on
//! a quit or a restart (also "Restart to update" on macOS and Linux, through the process plugin's
//! `relaunch`), and as the only step before `std::process::exit` when the updater installs on
//! Windows, which delivers no `RunEvent::Exit` (#1131). A resource whose drop waits for the
//! writes covers all of them; the single-instance plugin releases its lock the same way.

use std::sync::OnceLock;

use tauri::{AppHandle, Manager, Resource, Runtime};

use crate::commands::AppState;

/// How long the app waits at exit for running repository commands, like the VSCode extension's
/// `SIDECAR_EXIT_GRACE_MS` (#961).
const EXIT_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// Whether the wait has run: the `RunEvent::Exit` handler and the cleanup after it both ask.
#[derive(Default)]
struct Waited(OnceLock<()>);

struct WaitOnCleanup<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Resource for WaitOnCleanup<R> {}

impl<R: Runtime> Drop for WaitOnCleanup<R> {
    fn drop(&mut self) {
        let_running_writes_finish(&self.0);
    }
}

/// Makes every exit of `app` wait for its running repository commands first.
pub fn wait_for_writes_at_exit<R: Runtime>(app: &AppHandle<R>) {
    app.manage(Waited::default());
    app.resources_table().add(WaitOnCleanup(app.clone()));
}

/// Stops every worker and waits for running commands, once; a later call returns when that wait
/// is over.
pub fn let_running_writes_finish<R: Runtime>(app: &AppHandle<R>) {
    let Some(waited) = app.try_state::<Waited>() else {
        return;
    };
    waited.0.get_or_init(|| {
        if !app.state::<AppState>().registry.stop_all(EXIT_GRACE) {
            log::warn!(
                "exiting while a repository command still runs after {EXIT_GRACE:?}; \
                 it may leave a git lock file behind"
            );
        }
    });
}

/// After a failed "Restart to update" (#1194): whether the app has to be restarted.
pub fn recover_from_failed_update<R: Runtime>(_app: &AppHandle<R>) -> bool {
    false
}

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

    /// On Windows the updater runs `cleanup_before_exit` before it starts the installer, and
    /// returns the error when the installer doesn't start: the process then keeps running with
    /// its workers stopped (#1194). The recovery tells the frontend that only a restart helps.
    #[test]
    fn a_failed_install_after_the_cleanup_needs_a_restart() {
        let app = tauri::test::mock_builder()
            .manage(AppState::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        super::wait_for_writes_at_exit(app.handle());

        app.handle().cleanup_before_exit();

        assert!(
            super::recover_from_failed_update(app.handle()),
            "an install that failed after the cleanup before an exit isn't reported as stopped"
        );
    }

    /// An install that fails before that cleanup (a package that doesn't verify or extract, or
    /// any failure on macOS and Linux) leaves the window and the workers as they were: trying
    /// again needs no restart.
    #[test]
    fn a_failed_install_without_the_cleanup_needs_no_restart() {
        let app = tauri::test::mock_builder()
            .manage(AppState::default())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        super::wait_for_writes_at_exit(app.handle());
        let dir = repo();

        assert!(
            !super::recover_from_failed_update(app.handle()),
            "an install that failed before the cleanup before an exit is reported as stopped"
        );
        assert!(
            app.state::<AppState>()
                .registry
                .open(dir.path().to_str().unwrap())
                .is_ok(),
            "the recovery stopped the workers"
        );
    }
}
