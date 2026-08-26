//! Repo registry and user preferences.
//!
//! Phase 1 scope: a recent-repos list backed by a single TOML file in the OS config
//! directory. See `list_recent_repos`/`add_recent_repo`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};
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

use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) const MAX_RECENT_REPOS: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub root_path: PathBuf,
    pub member_paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphBranchSelection {
    pub repo_path: PathBuf,
    pub selected_branches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRepoEntry {
    pub path: PathBuf,
    #[serde(default)]
    pub workspace_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum OpenReposFile {
    Current(Vec<OpenRepoEntry>),
    Legacy(Vec<PathBuf>),
}

fn deserialize_open_repos<'de, D>(deserializer: D) -> Result<Vec<OpenRepoEntry>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match OpenReposFile::deserialize(deserializer)? {
        OpenReposFile::Current(entries) => entries,
        OpenReposFile::Legacy(paths) => paths
            .into_iter()
            .map(|path| OpenRepoEntry {
                path,
                workspace_id: None,
            })
            .collect(),
    })
}

static WORKSPACE_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// A nanosecond timestamp plus an in-process counter, hex-formatted. Not a UUID — this repo
/// has no `uuid` dependency and doesn't need one: workspace ids are generated locally by a
/// single user, never compared across machines, so global uniqueness guarantees are overkill.
/// The counter alone (not just the timestamp) is what protects against two calls landing in
/// the same clock tick on a coarse-grained OS clock.
fn generate_workspace_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = WORKSPACE_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{nanos:x}-{counter:x}")
}

fn dedupe_workspace_name(existing: &[Workspace], name: &str, excluding_id: Option<&str>) -> String {
    let taken = |candidate: &str| {
        existing
            .iter()
            .any(|w| w.name == candidate && Some(w.id.as_str()) != excluding_id)
    };
    if !taken(name) {
        return name.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{name} ({n})");
        if !taken(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    recent_repos: Vec<PathBuf>,
    #[serde(default)]
    active_repo: Option<PathBuf>,
    #[serde(default)]
    last_seen_version: Option<String>,
    // These fields serialize as TOML array-of-tables because their element type is a struct.
    // TOML requires plain `key = value` lines to precede an array-of-tables section, so they
    // must remain last.
    #[serde(default, deserialize_with = "deserialize_open_repos")]
    open_repos: Vec<OpenRepoEntry>,
    #[serde(default)]
    workspaces: Vec<Workspace>,
    #[serde(default)]
    graph_branch_selections: Vec<GraphBranchSelection>,
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

/// Finds repositories directly beneath `root` by looking for a `.git` entry.
///
/// The scan intentionally examines only immediate child directories. A `.git` entry may be
/// either a directory (normal checkout) or a file (worktree/submodule checkout).
pub fn scan_repos_in_root(root: &Path) -> Result<Vec<PathBuf>, ConfigError> {
    let mut repos = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path();
        if path.join(".git").exists() {
            repos.push(path);
        }
    }
    repos.sort();
    Ok(repos)
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

pub fn get_last_seen_version() -> Result<Option<String>, ConfigError> {
    get_last_seen_version_at(&config_file_path()?)
}

pub fn set_last_seen_version(version: &str) -> Result<(), ConfigError> {
    set_last_seen_version_at(&config_file_path()?, version)
}

pub fn get_last_seen_version_at(config_file: &Path) -> Result<Option<String>, ConfigError> {
    Ok(read_config(config_file)?.last_seen_version)
}

pub fn set_last_seen_version_at(config_file: &Path, version: &str) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.last_seen_version = Some(version.to_string());
    write_config(config_file, &config)
}

pub fn list_workspaces() -> Result<Vec<Workspace>, ConfigError> {
    list_workspaces_at(&config_file_path()?)
}

pub fn list_workspaces_at(config_file: &Path) -> Result<Vec<Workspace>, ConfigError> {
    Ok(read_config(config_file)?.workspaces)
}

pub fn save_workspace(name: &str, root: &Path, members: &[PathBuf]) -> Result<String, ConfigError> {
    save_workspace_at(&config_file_path()?, name, root, members)
}

pub fn save_workspace_at(
    config_file: &Path,
    name: &str,
    root: &Path,
    members: &[PathBuf],
) -> Result<String, ConfigError> {
    let mut config = read_config(config_file)?;
    let id = generate_workspace_id();
    let deduped_name = dedupe_workspace_name(&config.workspaces, name, None);
    config.workspaces.push(Workspace {
        id: id.clone(),
        name: deduped_name,
        root_path: root.to_path_buf(),
        member_paths: members.to_vec(),
    });
    write_config(config_file, &config)?;
    Ok(id)
}

pub fn update_workspace(id: &str, name: &str, members: &[PathBuf]) -> Result<(), ConfigError> {
    update_workspace_at(&config_file_path()?, id, name, members)
}

pub fn update_workspace_at(
    config_file: &Path,
    id: &str,
    name: &str,
    members: &[PathBuf],
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    let deduped_name = dedupe_workspace_name(&config.workspaces, name, Some(id));
    if let Some(workspace) = config.workspaces.iter_mut().find(|w| w.id == id) {
        workspace.name = deduped_name;
        workspace.member_paths = members.to_vec();
    }
    write_config(config_file, &config)
}

pub fn delete_workspace(id: &str) -> Result<(), ConfigError> {
    delete_workspace_at(&config_file_path()?, id)
}

pub fn delete_workspace_at(config_file: &Path, id: &str) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.workspaces.retain(|w| w.id != id);
    write_config(config_file, &config)
}

pub fn list_open_repos() -> Result<(Vec<OpenRepoEntry>, Option<PathBuf>), ConfigError> {
    list_open_repos_at(&config_file_path()?)
}

pub fn set_open_repos(entries: &[OpenRepoEntry], active: Option<&Path>) -> Result<(), ConfigError> {
    set_open_repos_at(&config_file_path()?, entries, active)
}

pub fn list_open_repos_at(
    config_file: &Path,
) -> Result<(Vec<OpenRepoEntry>, Option<PathBuf>), ConfigError> {
    let config = read_config(config_file)?;
    Ok((config.open_repos, config.active_repo))
}

pub fn set_open_repos_at(
    config_file: &Path,
    entries: &[OpenRepoEntry],
    active: Option<&Path>,
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.open_repos = entries.to_vec();
    config.active_repo = active.map(|p| p.to_path_buf());
    write_config(config_file, &config)
}

pub fn get_graph_branch_selection(repo_path: &Path) -> Result<Option<Vec<String>>, ConfigError> {
    get_graph_branch_selection_at(&config_file_path()?, repo_path)
}

pub fn set_graph_branch_selection(
    repo_path: &Path,
    selected_branches: &[String],
) -> Result<(), ConfigError> {
    set_graph_branch_selection_at(&config_file_path()?, repo_path, selected_branches)
}

pub fn get_graph_branch_selection_at(
    config_file: &Path,
    repo_path: &Path,
) -> Result<Option<Vec<String>>, ConfigError> {
    let config = read_config(config_file)?;
    Ok(config
        .graph_branch_selections
        .into_iter()
        .find(|s| s.repo_path == repo_path)
        .map(|s| s.selected_branches))
}

pub fn set_graph_branch_selection_at(
    config_file: &Path,
    repo_path: &Path,
    selected_branches: &[String],
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config
        .graph_branch_selections
        .retain(|s| s.repo_path != repo_path);
    config.graph_branch_selections.push(GraphBranchSelection {
        repo_path: repo_path.to_path_buf(),
        selected_branches: selected_branches.to_vec(),
    });
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
