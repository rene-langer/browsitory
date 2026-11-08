//! One-time move of the per-user folders named after the old bundle identifier (#433).
//!
//! Up to 0.3.x the desktop app's bundle identifier was `com.browsitory.app`; from 0.4.0 it is the
//! one in `tauri.conf.json` (`com.browsitory.desktop`). Tauri, WebKitGTK, WebView2 and WebKit name
//! per-user folders after the identifier: the log folder and the webview's storage (its
//! `localStorage` holds the theme, panel sizes and other UI choices). On the first start of the
//! new version, [`migrate`] renames each old folder to the new name.
//!
//! A rename, not a copy: it is atomic (the old and new folder share a parent, so they are on one
//! file system), so an interrupted start leaves either the old folder or the new one, never half
//! of each, and no data is deleted. A folder is only moved when the new one doesn't exist yet, so
//! a second start does nothing and a newer folder is never overwritten. A failure is reported,
//! not raised: the app then starts with fresh folders and the old ones stay where they were.
//!
//! [`plugin`] runs it as the first plugin, before the log plugin opens its file and before any
//! webview is created.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tauri::plugin::TauriPlugin;
use tauri::{Manager, Runtime};

/// The bundle identifier of Browsitory 0.3.x and older.
pub const OLD_IDENTIFIER: &str = "com.browsitory.app";

/// What [`migrate`] did for one base folder. Bases with no old folder report nothing.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// `from` was renamed to `to`.
    Moved { from: PathBuf, to: PathBuf },
    /// Both folders exist; neither was touched.
    BothExist { old: PathBuf, new: PathBuf },
    /// The rename failed; `from` is unchanged.
    Failed {
        from: PathBuf,
        to: PathBuf,
        error: String,
    },
}

/// Renames `<base>/<old_id>` to `<base>/<new_id>` in each of `bases` where the old folder exists
/// and the new one doesn't. Repeated bases are handled once.
pub fn migrate(bases: &[PathBuf], old_id: &str, new_id: &str) -> Vec<Outcome> {
    let mut outcomes = Vec::new();
    if old_id == new_id {
        return outcomes;
    }
    let mut seen = HashSet::new();
    for base in bases {
        if !seen.insert(base.as_path()) {
            continue;
        }
        let from = base.join(old_id);
        let to = base.join(new_id);
        if !exists(&from) {
            continue;
        }
        if exists(&to) {
            outcomes.push(Outcome::BothExist { old: from, new: to });
            continue;
        }
        match std::fs::rename(&from, &to) {
            Ok(()) => outcomes.push(Outcome::Moved { from, to }),
            Err(error) => outcomes.push(Outcome::Failed {
                from,
                to,
                error: error.to_string(),
            }),
        }
    }
    outcomes
}

/// Whether anything (folder, file or link, even a dangling one) is at `path`.
fn exists(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

/// The folders, from Tauri's own path resolver, that hold identifier-named folders: Tauri's app
/// data, local data (the Linux and Windows webview data and, there, the log folder), config and
/// cache folders, and on macOS the log folder and WebKit's per-app storage.
fn identifier_bases<R: Runtime>(app: &tauri::AppHandle<R>) -> Vec<PathBuf> {
    let path = app.path();
    #[allow(unused_mut)]
    let mut bases: Vec<PathBuf> = [
        path.data_dir(),
        path.local_data_dir(),
        path.config_dir(),
        path.cache_dir(),
    ]
    .into_iter()
    .flatten()
    .collect();
    #[cfg(target_os = "macos")]
    if let Ok(home) = path.home_dir() {
        let library = home.join("Library");
        bases.push(library.join("Logs"));
        bases.push(library.join("WebKit"));
        bases.push(library.join("HTTPStorages"));
    }
    bases
}

/// The outcomes of the migration, kept until the log plugin is up (see [`log_report`]).
struct Report(Vec<Outcome>);

/// A plugin that runs [`migrate`] in its setup. Register it before every other plugin: plugins
/// are set up in registration order, and the log plugin opens its file in its own setup.
pub fn plugin<R: Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::new("identifier-migration")
        .setup(|app, _api| {
            let outcomes = migrate(
                &identifier_bases(app),
                OLD_IDENTIFIER,
                &app.config().identifier,
            );
            app.manage(Report(outcomes));
            Ok(())
        })
        .build()
}

/// Logs what [`plugin`] did. Call it once the log plugin is set up (the app's `setup` hook).
pub fn log_report<R: Runtime>(app: &tauri::AppHandle<R>) {
    let Some(report) = app.try_state::<Report>() else {
        return;
    };
    for outcome in &report.0 {
        match outcome {
            Outcome::Moved { from, to } => {
                log::info!("moved {} to {}", from.display(), to.display());
            }
            Outcome::BothExist { old, new } => log::warn!(
                "{} was not moved: {} already exists",
                old.display(),
                new.display()
            ),
            Outcome::Failed { from, to, error } => log::warn!(
                "could not move {} to {}: {error}",
                from.display(),
                to.display()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const OLD: &str = "com.browsitory.app";
    const NEW: &str = "com.browsitory.desktop";

    fn seed_old(base: &Path) {
        fs::create_dir_all(base.join(OLD).join("logs")).unwrap();
        fs::write(base.join(OLD).join("logs/browsitory.log"), "old log\n").unwrap();
        fs::create_dir_all(base.join(OLD).join("localstorage")).unwrap();
        fs::write(base.join(OLD).join("localstorage/data"), "theme=dark").unwrap();
    }

    #[test]
    fn moves_the_old_folder_with_its_contents_to_the_new_name() {
        let base = TempDir::new().unwrap();
        seed_old(base.path());

        let outcomes = migrate(&[base.path().to_path_buf()], OLD, NEW);

        assert_eq!(
            outcomes,
            vec![Outcome::Moved {
                from: base.path().join(OLD),
                to: base.path().join(NEW),
            }]
        );
        assert!(!base.path().join(OLD).exists());
        assert_eq!(
            fs::read_to_string(base.path().join(NEW).join("logs/browsitory.log")).unwrap(),
            "old log\n"
        );
        assert_eq!(
            fs::read_to_string(base.path().join(NEW).join("localstorage/data")).unwrap(),
            "theme=dark"
        );
    }

    #[test]
    fn does_nothing_without_an_old_folder() {
        let base = TempDir::new().unwrap();

        let outcomes = migrate(&[base.path().to_path_buf()], OLD, NEW);

        assert_eq!(outcomes, vec![]);
        assert!(!base.path().join(NEW).exists());
    }

    #[test]
    fn does_nothing_for_a_missing_base_folder() {
        let parent = TempDir::new().unwrap();
        let base = parent.path().join("not-there");

        assert_eq!(migrate(std::slice::from_ref(&base), OLD, NEW), vec![]);
        assert!(!base.exists());
    }

    #[test]
    fn leaves_both_folders_alone_when_the_new_one_exists() {
        let base = TempDir::new().unwrap();
        seed_old(base.path());
        fs::create_dir_all(base.path().join(NEW)).unwrap();
        fs::write(base.path().join(NEW).join("newer"), "keep").unwrap();

        let outcomes = migrate(&[base.path().to_path_buf()], OLD, NEW);

        assert_eq!(
            outcomes,
            vec![Outcome::BothExist {
                old: base.path().join(OLD),
                new: base.path().join(NEW),
            }]
        );
        assert!(base.path().join(OLD).join("logs/browsitory.log").exists());
        assert_eq!(
            fs::read_to_string(base.path().join(NEW).join("newer")).unwrap(),
            "keep"
        );
        assert!(!base.path().join(NEW).join("logs").exists());
    }

    #[test]
    fn a_second_run_does_nothing() {
        let base = TempDir::new().unwrap();
        seed_old(base.path());
        let bases = [base.path().to_path_buf()];
        migrate(&bases, OLD, NEW);

        assert_eq!(migrate(&bases, OLD, NEW), vec![]);
        assert!(base.path().join(NEW).join("logs/browsitory.log").exists());
    }

    #[test]
    fn moves_the_folder_in_every_base_and_handles_a_repeated_base_once() {
        // On Linux the data and local data folders are the same (`~/.local/share`).
        let data = TempDir::new().unwrap();
        let config = TempDir::new().unwrap();
        let untouched = TempDir::new().unwrap();
        seed_old(data.path());
        seed_old(config.path());
        let bases = [
            data.path().to_path_buf(),
            data.path().to_path_buf(),
            config.path().to_path_buf(),
            untouched.path().to_path_buf(),
        ];

        let outcomes = migrate(&bases, OLD, NEW);

        assert_eq!(
            outcomes,
            vec![
                Outcome::Moved {
                    from: data.path().join(OLD),
                    to: data.path().join(NEW),
                },
                Outcome::Moved {
                    from: config.path().join(OLD),
                    to: config.path().join(NEW),
                },
            ]
        );
        assert!(!untouched.path().join(NEW).exists());
    }

    #[test]
    fn does_nothing_when_the_identifier_is_unchanged() {
        let base = TempDir::new().unwrap();
        seed_old(base.path());

        assert_eq!(migrate(&[base.path().to_path_buf()], OLD, OLD), vec![]);
        assert!(base.path().join(OLD).join("logs/browsitory.log").exists());
    }

    #[cfg(unix)]
    #[test]
    fn reports_a_failed_rename_and_keeps_the_old_folder() {
        use std::os::unix::fs::PermissionsExt;

        let base = TempDir::new().unwrap();
        seed_old(base.path());
        // A read-only base folder: its entries can't be renamed.
        fs::set_permissions(base.path(), fs::Permissions::from_mode(0o555)).unwrap();

        let outcomes = migrate(&[base.path().to_path_buf()], OLD, NEW);

        fs::set_permissions(base.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            matches!(
                outcomes.as_slice(),
                [Outcome::Failed { from, to, .. }]
                    if *from == base.path().join(OLD) && *to == base.path().join(NEW)
            ),
            "{outcomes:?}"
        );
        assert!(base.path().join(OLD).join("logs/browsitory.log").exists());
        assert!(!base.path().join(NEW).exists());
    }
}
