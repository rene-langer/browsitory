//! Test-only helpers shared by the workspace's Rust test suites. Never a runtime dependency:
//! every crate pulls this in as a `[dev-dependencies]` entry only, so nothing here can change
//! the behavior of `cargo run`, the Tauri app or the VSCode sidecar outside tests.
//!
//! # Hermetic git config (#111)
//!
//! [`hermetic_git!`] makes a test binary immune to the developer's (or CI runner's)
//! global/XDG/system git config — `core.autocrlf`, `init.defaultBranch`, `fetch.prune`,
//! `commit.gpgSign`, `core.hooksPath`, … Put it once at the top level of each test binary (an
//! integration test file, a shared `tests/common/mod.rs`, or a crate root under
//! `#[cfg(test)]`). It registers a process constructor that runs [`install`] before `main`,
//! i.e. before the test harness starts any thread, which:
//!
//! - points libgit2's Global, XDG, System and ProgramData search paths at an empty directory,
//! - sets `HOME` and `XDG_CONFIG_HOME` to an empty home, plus `GIT_CONFIG_NOSYSTEM=1` and
//!   `GIT_CONFIG_GLOBAL=<empty file>`, so spawned `git` subprocesses, hook scripts and child
//!   processes (e.g. the sidecar binary, whose own libgit2 reads `$HOME/.gitconfig`) see no
//!   global or system config either.
//!
//! Setting environment variables is only sound while no other thread runs, which is exactly
//! the guarantee a pre-`main` constructor gives; [`install`] is still `Once`-guarded so calling
//! it again later is a no-op.
//!
//! The directories live next to the test binaries (`target/<profile>/browsitory-hermetic-git/`)
//! and are never deleted during a run, so concurrently running tests and test processes can
//! always read them (deleting a config directory another thread still has open is what made
//! the old per-test `ConfigSearchPathOverride` fail on Windows). A test that needs a specific
//! global config builds its own `git2::Config` with `add_file(…, ConfigLevel::Global, …)`
//! instead of moving the process-wide search path.

use std::path::{Path, PathBuf};
use std::sync::{Once, OnceLock};

#[doc(hidden)]
pub use ctor as __ctor;

/// Registers a constructor that runs [`install`] before `main` in the test binary this
/// expands into. Use once per test binary, at module level.
#[macro_export]
macro_rules! hermetic_git {
    () => {
        #[$crate::__ctor::ctor(crate_path = $crate::__ctor)]
        fn __browsitory_hermetic_git() {
            $crate::install();
        }
    };
}

/// Where the hermetic guard points git.
#[derive(Debug)]
pub struct HermeticPaths {
    /// Always empty: libgit2's Global/XDG/System/ProgramData search path.
    pub search_dir: PathBuf,
    /// `HOME` (and `XDG_CONFIG_HOME` = `home/.config`) for this process and its children.
    pub home: PathBuf,
    /// Empty file: `GIT_CONFIG_GLOBAL` for spawned `git`.
    pub global_config: PathBuf,
}

/// The hermetic paths, created on first use. Never deleted while tests run.
pub fn paths() -> &'static HermeticPaths {
    static PATHS: OnceLock<HermeticPaths> = OnceLock::new();
    PATHS.get_or_init(|| {
        let root = root_dir();
        let paths = HermeticPaths {
            search_dir: root.join("empty"),
            home: root.join("home"),
            global_config: root.join("gitconfig"),
        };
        let _ = std::fs::create_dir_all(&paths.search_dir);
        let _ = std::fs::create_dir_all(paths.home.join(".config"));
        if !paths.global_config.exists() {
            let _ = std::fs::write(&paths.global_config, "");
        }
        paths
    })
}

/// `target/<profile>/browsitory-hermetic-git` (test binaries live in `target/<profile>/deps`),
/// falling back to the system temp dir when the executable path is unusual.
fn root_dir() -> PathBuf {
    const NAME: &str = "browsitory-hermetic-git";
    std::env::current_exe()
        .ok()
        .as_deref()
        .and_then(Path::parent)
        .and_then(|deps| {
            if deps.file_name().is_some_and(|name| name == "deps") {
                deps.parent()
            } else {
                Some(deps)
            }
        })
        .map(|dir| dir.join(NAME))
        .unwrap_or_else(|| std::env::temp_dir().join(NAME))
}

/// Makes this process (and the processes it spawns) blind to global/XDG/system git config.
/// Runs at most once per process; see the module docs for what it changes and why it must run
/// before any other thread (use [`hermetic_git!`]).
pub fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let paths = paths();
        std::env::set_var("HOME", &paths.home);
        std::env::set_var("XDG_CONFIG_HOME", paths.home.join(".config"));
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
        std::env::set_var("GIT_CONFIG_GLOBAL", &paths.global_config);
        for level in [
            git2::ConfigLevel::ProgramData,
            git2::ConfigLevel::System,
            git2::ConfigLevel::XDG,
            git2::ConfigLevel::Global,
        ] {
            // SAFETY: libgit2's search paths are process-global and need external
            // synchronization; this runs once, under `ONCE`, before `main` (via
            // `hermetic_git!`), so no other thread can be reading them.
            if let Err(err) = unsafe { git2::opts::set_search_path(level, &paths.search_dir) } {
                eprintln!("test-support: could not isolate git config level {level:?}: {err}");
            }
        }
    });
}
