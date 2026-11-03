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
//!   global or system config either,
//! - removes every inherited `GIT_*` variable (a hook's `GIT_DIR`/`GIT_INDEX_FILE`/
//!   `GIT_WORK_TREE`, `GIT_CONFIG_PARAMETERS`, `GIT_AUTHOR_*`, …), so nothing in the run acts
//!   on the repository a hook or `git rebase -x` started it in,
//! - sets `GNUPGHOME` to an empty directory (no gpg agent of the developer's) and `LC_ALL=C`.
//!
//! [`git_command`] applies the same isolation to each `git` CLI command a test starts, even
//! in a binary without the guard or after a test changed the environment (#282).
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
        #[$crate::__ctor::ctor(unsafe, crate_path = $crate::__ctor)]
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
    /// Empty `GNUPGHOME` (mode 0700 on Unix) for the git CLI started by [`git_command`], so a
    /// twin never talks to the developer's gpg agent.
    pub gnupg_home: PathBuf,
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
            gnupg_home: root.join("gnupg"),
        };
        let _ = std::fs::create_dir_all(&paths.search_dir);
        let _ = std::fs::create_dir_all(paths.home.join(".config"));
        let _ = std::fs::create_dir_all(&paths.gnupg_home);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                std::fs::set_permissions(&paths.gnupg_home, std::fs::Permissions::from_mode(0o700));
        }
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
        for name in inherited_git_env() {
            std::env::remove_var(name);
        }
        std::env::set_var("GNUPGHOME", &paths.gnupg_home);
        std::env::set_var("LC_ALL", "C");
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

/// Every `GIT_*` variable in this process's environment: what a hook or `git rebase -x` hands
/// its children (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE` pointing at the real repository),
/// config that bypasses `GIT_CONFIG_GLOBAL` (`GIT_CONFIG_PARAMETERS`, `GIT_CONFIG_COUNT`/
/// `_KEY_n`/`_VALUE_n`), and `GIT_AUTHOR_*`/`GIT_COMMITTER_*` (#282 review).
fn inherited_git_env() -> Vec<std::ffi::OsString> {
    std::env::vars_os()
        .map(|(name, _)| name)
        .filter(|name| name.to_string_lossy().starts_with("GIT_"))
        .collect()
}

/// A `git` CLI command ("twin") run in `dir`, isolated from the developer's setup whatever this
/// process's environment says (#282): every inherited `GIT_*` variable is removed (so the twin
/// works on `dir`, never a repository a hook's `GIT_DIR` names), `HOME`/`XDG_CONFIG_HOME` are the
/// hermetic home, `GIT_CONFIG_GLOBAL` is the empty hermetic file, `GIT_CONFIG_NOSYSTEM=1`,
/// `GNUPGHOME` is an empty directory of its own (no gpg agent of the developer's), and
/// `LC_ALL=C` gives English messages that tests may match. Every test that runs the git CLI
/// uses this (or [`git`]); only a bare `git --version` availability check may skip it.
pub fn git_command(dir: &Path) -> std::process::Command {
    let paths = paths();
    let mut command = std::process::Command::new("git");
    for name in inherited_git_env() {
        command.env_remove(name);
    }
    command
        .current_dir(dir)
        .env("LC_ALL", "C")
        .env("HOME", &paths.home)
        .env("XDG_CONFIG_HOME", paths.home.join(".config"))
        .env("GIT_CONFIG_GLOBAL", &paths.global_config)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GNUPGHOME", &paths.gnupg_home);
    command
}

/// Set to `1` to rewrite generated files (see [`assert_generated_file`]) instead of failing.
pub const UPDATE_ENV: &str = "UPDATE_TRANSPORT_METHODS";

/// Checks that the checked-in file at `path` holds exactly `expected`, or rewrites it when
/// [`UPDATE_ENV`] is `1`. Line endings are ignored, so a CRLF checkout still compares equal.
/// Used for the transport method lists in `frontend/src/ipc/generated/` (#306).
pub fn assert_generated_file(path: &Path, expected: &str) {
    if std::env::var(UPDATE_ENV).as_deref() == Ok("1") {
        // CI must check the checked-in file, never rewrite it.
        assert!(
            std::env::var_os("CI").is_none(),
            "{UPDATE_ENV} must not be set in CI"
        );
        std::fs::write(path, expected).unwrap_or_else(|err| panic!("write {path:?}: {err}"));
        return;
    }
    let actual = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        actual.replace("\r\n", "\n") == expected,
        "{} is out of date; rerun this test with {UPDATE_ENV}=1 to regenerate it",
        path.display()
    );
}

/// `names`, sorted, as a pretty-printed JSON array of strings (one per line, trailing newline).
/// The names are plain identifiers, so no JSON escaping is needed.
pub fn json_name_list(names: &[&str]) -> String {
    let mut names = names.to_vec();
    names.sort_unstable();
    if names.is_empty() {
        return "[]\n".to_string();
    }
    let lines: Vec<String> = names.iter().map(|name| format!("  \"{name}\"")).collect();
    format!("[\n{}\n]\n", lines.join(",\n"))
}

/// The repository root (the parent of `crates/`), for tests that read checked-in files.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/test-support has a repository root")
        .to_path_buf()
}

/// Runs `git <args>` in `dir` through [`git_command`] and returns its output.
pub fn git(dir: &Path, args: &[&str]) -> std::process::Output {
    git_command(dir)
        .args(args)
        .output()
        .unwrap_or_else(|err| panic!("run git {args:?}: {err}"))
}
