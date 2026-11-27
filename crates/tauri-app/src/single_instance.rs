//! One desktop app process per user (#1136). Starting Browsitory again while it runs (the Start
//! menu or desktop entry, with the window minimized or on another desktop) shows the running
//! window instead of starting a second process on the same tabs, `config.toml`, log file and
//! repositories. `tauri-plugin-single-instance` finds the running app by the bundle identifier: a
//! D-Bus name on Linux (without a session bus the launch goes ahead), a named mutex on Windows, a
//! Unix socket on macOS. The app takes no arguments, so there is nothing to hand over.

use std::ffi::OsString;

use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Manager, Runtime};

/// The variable that gives a launch its own config folder (see `config::config_file_path`).
const CONFIG_DIR_VARIABLE: &str = "BROWSITORY_CONFIG_DIR";

/// Whether a launch defers to the running app. One with its own config folder (the E2E suites, a
/// dev build beside the installed app) shares no tabs or settings with it, and runs beside it.
pub fn applies(config_dir_override: Option<OsString>) -> bool {
    config_dir_override.is_none()
}

/// The plugin for this launch, to register before every other one; `None` when it runs beside a
/// running app (see `applies`).
pub fn plugin<R: Runtime>() -> Option<TauriPlugin<R>> {
    applies(std::env::var_os(CONFIG_DIR_VARIABLE)).then(|| {
        tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app, "for a second launch")
        })
    })
}

/// Brings the main window to the front; `occasion` ends the log line when that fails.
pub fn show_main_window<R: Runtime>(app: &AppHandle<R>, occasion: &str) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    for result in [window.unminimize(), window.show(), window.set_focus()] {
        if let Err(error) = result {
            log::warn!("could not bring the window to the front {occasion}: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::applies;

    #[test]
    fn a_normal_launch_defers_to_the_running_app() {
        assert!(applies(None));
    }

    /// A launch with its own config folder (`BROWSITORY_CONFIG_DIR`: the E2E suites, a dev build
    /// beside the installed app) shares no tabs or settings with the running app.
    #[test]
    fn a_launch_with_its_own_config_folder_runs_beside_it() {
        assert!(!applies(Some("/tmp/browsitory-e2e-config".into())));
    }
}
