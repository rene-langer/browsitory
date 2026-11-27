//! One desktop app process per user (#1136).

use std::ffi::OsString;

pub fn applies(_config_dir_override: Option<OsString>) -> bool {
    true
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
