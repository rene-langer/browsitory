//! Repo registry and user preferences.
//!
//! Phase 1 scope: a recent-repos list backed by a single TOML file in the OS config
//! directory. See `list_recent_repos`/`add_recent_repo`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
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

pub(crate) const MAX_RECENT_REPOS: usize = 10;

#[derive(Debug, Default, Serialize, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    recent_repos: Vec<PathBuf>,
    #[serde(default)]
    open_repos: Vec<PathBuf>,
    #[serde(default)]
    active_repo: Option<PathBuf>,
}

fn config_file_path() -> Result<PathBuf, ConfigError> {
    if let Ok(dir) = std::env::var("BROWSITORY_CONFIG_DIR") {
        return Ok(PathBuf::from(dir).join("config.toml"));
    }
    let dirs = directories::ProjectDirs::from("com", "browsitory", "Browsitory")
        .ok_or(ConfigError::NoConfigDir)?;
    Ok(dirs.config_dir().join("config.toml"))
}

pub fn list_recent_repos() -> Result<Vec<PathBuf>, ConfigError> {
    list_recent_repos_at(&config_file_path()?)
}

/// Adds `path` to the front of the recent-repos list, de-duplicating it if already present
/// and capping the list at 10 entries (oldest dropped).
pub fn add_recent_repo(path: &Path) -> Result<(), ConfigError> {
    add_recent_repo_at(&config_file_path()?, path)
}

// Not `pub(crate)`: integration tests in `crates/config/tests/` are a separate compiled
// crate, so they need `pub` visibility to call these directly. They're still an
// implementation seam for tests, not part of the crate's intended public API surface —
// `list_recent_repos`/`add_recent_repo` are the real entry points other crates use.
pub fn list_recent_repos_at(config_file: &Path) -> Result<Vec<PathBuf>, ConfigError> {
    Ok(read_config(config_file)?.recent_repos)
}

pub fn add_recent_repo_at(config_file: &Path, path: &Path) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.recent_repos.retain(|p| p != path);
    config.recent_repos.insert(0, path.to_path_buf());
    config.recent_repos.truncate(MAX_RECENT_REPOS);
    write_config(config_file, &config)
}

pub fn list_open_repos() -> Result<(Vec<PathBuf>, Option<PathBuf>), ConfigError> {
    list_open_repos_at(&config_file_path()?)
}

pub fn set_open_repos(paths: &[PathBuf], active: Option<&Path>) -> Result<(), ConfigError> {
    set_open_repos_at(&config_file_path()?, paths, active)
}

pub fn list_open_repos_at(
    config_file: &Path,
) -> Result<(Vec<PathBuf>, Option<PathBuf>), ConfigError> {
    let config = read_config(config_file)?;
    Ok((config.open_repos, config.active_repo))
}

pub fn set_open_repos_at(
    config_file: &Path,
    paths: &[PathBuf],
    active: Option<&Path>,
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.open_repos = paths.to_vec();
    config.active_repo = active.map(|p| p.to_path_buf());
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
