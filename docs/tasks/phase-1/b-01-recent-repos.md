# Task 1.B.01: `config` recent-repos registry

## Goal

Turn `crates/config` from a stub into a real crate: `list_recent_repos()`/`add_recent_repo(path)`
backed by one TOML file in the OS config directory. This is what Phase 1's `RepoPicker` shows on
launch instead of an empty screen. No other preferences this phase — just the recent-repos list.

## Depends on

None — new crate content, independent of `git-core`/`tauri-app`.

## Interfaces produced

`crates/config/Cargo.toml` gains dependencies:
```toml
[dependencies]
directories = "6"
serde = { version = "1", features = ["derive"] }
toml = "1"
thiserror = "1.0"

[dev-dependencies]
tempfile = "3"
```
(`thiserror = "1.0"` matches the version already pinned in `crates/git-core/Cargo.toml` — keep
the workspace on one `thiserror` major rather than introducing a second.)

`crates/config/src/lib.rs`:
```rust
use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("could not determine the OS config directory")]
    NoConfigDir,
    #[error("failed to read config file: {0}")]
    Read(#[from] std::io::Error),
    #[error("failed to parse config file: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

pub fn list_recent_repos() -> Result<Vec<PathBuf>, ConfigError> {
    // ...
}

/// Adds `path` to the front of the recent-repos list, de-duplicating it if already present
/// and capping the list at 10 entries (oldest dropped).
pub fn add_recent_repo(path: &Path) -> Result<(), ConfigError> {
    // ...
}
```

For testability (the real function reads/writes a fixed OS path, which tests can't safely
share), the implementation is split so tests exercise the read/write logic against a
tempdir-supplied path, while the two public functions above resolve the real OS path and
delegate:
```rust
pub(crate) const MAX_RECENT_REPOS: usize = 10;

pub(crate) fn list_recent_repos_at(config_file: &Path) -> Result<Vec<PathBuf>, ConfigError> { /* ... */ }
pub(crate) fn add_recent_repo_at(config_file: &Path, path: &Path) -> Result<(), ConfigError> { /* ... */ }
```

## Implementation notes

```rust
use std::fs;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    recent_repos: Vec<PathBuf>,
}

fn config_file_path() -> Result<PathBuf, ConfigError> {
    let dirs = directories::ProjectDirs::from("com", "browsitory", "Browsitory")
        .ok_or(ConfigError::NoConfigDir)?;
    Ok(dirs.config_dir().join("config.toml"))
}

pub fn list_recent_repos() -> Result<Vec<PathBuf>, ConfigError> {
    list_recent_repos_at(&config_file_path()?)
}

pub fn add_recent_repo(path: &Path) -> Result<(), ConfigError> {
    add_recent_repo_at(&config_file_path()?, path)
}

pub(crate) fn list_recent_repos_at(config_file: &Path) -> Result<Vec<PathBuf>, ConfigError> {
    Ok(read_config(config_file)?.recent_repos)
}

pub(crate) fn add_recent_repo_at(config_file: &Path, path: &Path) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.recent_repos.retain(|p| p != path);
    config.recent_repos.insert(0, path.to_path_buf());
    config.recent_repos.truncate(MAX_RECENT_REPOS);
    write_config(config_file, &config)
}

fn read_config(path: &Path) -> Result<ConfigFile, ConfigError> {
    if !path.exists() {
        return Ok(ConfigFile::default());
    }
    let contents = fs::read_to_string(path)?;
    Ok(toml::from_str(&contents)?)
}

fn write_config(path: &Path, config: &ConfigFile) -> Result<(), ConfigError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, toml::to_string_pretty(config)?)?;
    Ok(())
}
```
`directories::ProjectDirs::from(qualifier, organization, application) -> Option<ProjectDirs>` —
use `("com", "browsitory", "Browsitory")`; `.config_dir()` gives the per-OS config directory
(e.g. `~/.config/browsitory` on Linux). `ConfigError::NoConfigDir` covers the rare
platform-can't-determine-home-dir case.

## TDD requirement

`crates/config/tests/recent_repos.rs` (new file — this crate has no `tests/` directory yet,
create it):

- `list_recent_repos_at_returns_empty_for_a_missing_file`: a `tempfile::TempDir`, a
  `config.toml` path inside it that's never written, assert
  `list_recent_repos_at(&path).unwrap().is_empty()`.
- `add_recent_repo_at_persists_across_calls`: `add_recent_repo_at(&path, Path::new("/repo/a"))`,
  then `list_recent_repos_at(&path)`, assert it returns `[PathBuf::from("/repo/a")]`.
- `add_recent_repo_at_puts_the_newest_entry_first`: add `/repo/a` then `/repo/b`, assert
  `list_recent_repos_at` returns `[PathBuf::from("/repo/b"), PathBuf::from("/repo/a")]`.
- `add_recent_repo_at_deduplicates_and_moves_to_front`: add `/repo/a`, `/repo/b`, then `/repo/a`
  again, assert the result is `[PathBuf::from("/repo/a"), PathBuf::from("/repo/b")]` (length 2,
  not 3).
- `add_recent_repo_at_caps_at_ten_entries`: add 11 distinct paths (`/repo/0` through `/repo/10`),
  assert `list_recent_repos_at` returns exactly 10 entries and the oldest (`/repo/0`) is not
  among them.

These call the `pub(crate)` `_at` functions directly — since the test file lives in
`crates/config/tests/`, which is a separate compiled crate from `crates/config/src/`,
`pub(crate)` items aren't visible there. Make `list_recent_repos_at`/`add_recent_repo_at`
`pub` instead of `pub(crate)` (drop the visibility restriction) so the integration tests can
call them directly, but don't include them in any public-facing documentation/re-export beyond
that — they're an implementation seam for tests, not part of the crate's intended public API
surface (`list_recent_repos`/`add_recent_repo` are the real entry points other crates use).

Write these five tests first, run `cargo test -p config --test recent_repos`, confirm compile
failure, then implement `lib.rs` and re-run until green.

## Acceptance criteria

- [ ] `cargo test -p config --test recent_repos` passes (all 5 tests).
- [ ] `cargo test --workspace` still passes.
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` clean.
- [ ] `cargo fmt --all -- --check` clean.
- [ ] `docs/LICENSE_COMPLIANCE.md` gains rows for `directories` and `toml` (run `cargo info
      directories` / `cargo info toml`, confirm permissive, record license — both are
      MIT/Apache-2.0 dual-licensed as of this writing, but verify against the actual installed
      version rather than trusting that claim).
- [ ] Commit: `git add crates/config docs/LICENSE_COMPLIANCE.md && git commit -m "feat(config): add recent-repos registry backed by TOML"`.

## Out of scope

Any preference beyond the recent-repos list (themes, editor integration, per-repo settings).
Config file migration/versioning. Concurrent-write safety (two Browsitory instances writing the
config file at once) — single-instance use is assumed for Phase 1.
