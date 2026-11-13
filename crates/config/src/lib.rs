//! Repo registry and user preferences.
//!
//! Phase 1 scope: a recent-repos list backed by a single TOML file in the OS config
//! directory. See `list_recent_repos`/`add_recent_repo`.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use git_core::diff::MAX_CONTEXT_LINES;
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
pub struct GraphRemoteBranchSelection {
    pub repo_path: PathBuf,
    pub selected_branches: Vec<String>,
}

/// The merge dialog's fast-forward choice, as remembered per repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MergeFastForward {
    Allow,
    Never,
    Only,
}

/// The last confirmed "Merge into current branch" options for one repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MergePreference {
    pub repo_path: PathBuf,
    pub fast_forward: MergeFastForward,
    pub squash: bool,
}

/// How a diverged pull reconciles, as remembered per repository (#652).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PullStrategyPreference {
    Merge,
    Rebase,
    FastForwardOnly,
}

/// The pull strategy last chosen for one repository (the toolbar's Pull menu, or "Remember" in
/// the diverged-pull dialog). It wins over the repository's `pull.rebase` / `pull.ff`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullPreference {
    pub repo_path: PathBuf,
    pub strategy: PullStrategyPreference,
}

/// A custom tab name for one repository (#625). Kept by path, apart from the open tabs, so it
/// survives closing the tab and a restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoAlias {
    pub repo_path: PathBuf,
    pub alias: String,
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

/// Global diff display/staging preferences, persisted as the `[diff]` TOML table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiffSettings {
    pub ignore_whitespace: bool,
    pub context_lines: u32,
    pub syntax_highlight: bool,
    pub word_wrap: WordWrap,
    pub algorithm: DiffAlgorithm,
    /// Render spaces, tabs and carriage returns as faint markers in diff lines (#616).
    pub show_whitespace: bool,
    /// Untracked files start expanded in the uncommitted changes (#623); off, they start
    /// collapsed and load nothing until expanded.
    pub expand_untracked: bool,
}

/// The diff algorithm every diff and hunk/line action uses (#469). Stored as `"default"`,
/// `"patience"` or `"minimal"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiffAlgorithm {
    #[default]
    Default,
    Patience,
    Minimal,
}

/// Whether diff lines wrap at the pane's width (`On`) or scroll sideways (`Off`). `Auto` leaves
/// it to the diff view: it wraps the split view, prose files and diffs with a very long line.
/// Stored as `"auto"`, `"on"` or `"off"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WordWrap {
    #[default]
    Auto,
    On,
    Off,
}

impl Default for DiffSettings {
    /// Whitespace and context default to `git_core::diff::DiffOptions::default()`'s.
    fn default() -> Self {
        let options = git_core::diff::DiffOptions::default();
        Self {
            ignore_whitespace: options.ignore_whitespace,
            context_lines: options.context_lines,
            syntax_highlight: true,
            word_wrap: WordWrap::Auto,
            algorithm: DiffAlgorithm::Default,
            show_whitespace: false,
            expand_untracked: true,
        }
    }
}

/// Commit message editor preferences, persisted as the `[commit_message]` TOML table: the
/// summary length counter's soft limit and the body ruler column (both clamped to
/// `COMMIT_MESSAGE_COLUMNS` on write).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommitMessageSettings {
    pub summary_limit: u32,
    pub body_ruler: u32,
    pub show_ruler: bool,
}

impl Default for CommitMessageSettings {
    fn default() -> Self {
        Self {
            summary_limit: 50,
            body_ruler: 72,
            show_ruler: true,
        }
    }
}

/// The range `summary_limit` and `body_ruler` are clamped to.
pub const COMMIT_MESSAGE_COLUMNS: std::ops::RangeInclusive<u32> = 10..=200;

/// External editor/terminal commands, persisted as the `[tools]` TOML table. `None` means "use
/// the system default". Diff/merge tools are not here; they come from the user's git config.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExternalToolSettings {
    pub editor: Option<String>,
    pub terminal: Option<String>,
}

/// Fetch preferences, persisted as the `[fetch]` TOML table. `auto_fetch_minutes` is the
/// app-wide "Fetch automatically" interval; 0 (the default) is off. A repository can override
/// it in the UI (a per-repository preference the frontend keeps, like the Prune choice).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FetchSettings {
    pub auto_fetch_minutes: u32,
}

/// The longest "Fetch automatically" interval `set_fetch_settings_at` keeps (a day).
pub const MAX_AUTO_FETCH_MINUTES: u32 = 24 * 60;

/// The user's keyboard shortcuts (#898), persisted as the `[keybindings]` TOML table: each
/// shortcut the user changed, by id (`refresh`, `conflict-ours`), with its keys as names such as
/// `"mod+shift+k"` or `"F4"` (`mod` is Cmd on macOS, Ctrl elsewhere); an empty list is a shortcut
/// without keys. The frontend owns the ids and the key names and drops what it can't use. An
/// entry that is no list of strings is dropped on read, so a hand edit can't break the file. The
/// VS Code extension ignores the table: VS Code's own keybindings apply there (#900).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeybindingSettings {
    #[serde(deserialize_with = "deserialize_keybindings")]
    pub overrides: BTreeMap<String, Vec<String>>,
}

impl KeybindingSettings {
    fn is_empty(&self) -> bool {
        self.overrides.is_empty()
    }
}

/// `[keybindings]`, without the entries that are no list of strings.
fn deserialize_keybindings<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Entry {
        Keys(Vec<String>),
        Other(serde::de::IgnoredAny),
    }
    let entries = BTreeMap::<String, Entry>::deserialize(deserializer)?;
    Ok(entries
        .into_iter()
        .filter_map(|(id, entry)| match entry {
            Entry::Keys(keys) => Some((id, keys)),
            Entry::Other(_) => None,
        })
        .collect())
}

/// Appearance preferences, persisted as the `[appearance]` TOML table (#615). `code_font` is a
/// font family tried before the default monospace stack, and `code_font_size` the pixel size of
/// file content; `None` keeps the built-in look for both. `tab_size` is the width of a tab in
/// columns. `date_format` is how dates are shown (#675). Out-of-range values are clamped both on
/// read and on write
/// (`AppearanceSettings::clamped`), so a hand-edited file can't break the views.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    pub code_font: Option<String>,
    pub code_font_size: Option<u32>,
    pub ui_size: UiSize,
    pub tab_size: u32,
    pub date_format: DateFormat,
}

/// How dates are shown (#675): `iso` is `2026-11-11 14:05`, `system` the operating system's
/// short date and time. Stored as `"iso"` or `"system"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DateFormat {
    #[default]
    Iso,
    System,
}

/// The interface text scale: each maps the `--text-*` sizes. Stored as `"small"`, `"normal"`
/// or `"large"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UiSize {
    Small,
    #[default]
    Normal,
    Large,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            code_font: None,
            code_font_size: None,
            ui_size: UiSize::Normal,
            tab_size: 4,
            date_format: DateFormat::Iso,
        }
    }
}

/// The range `code_font_size` is clamped to, in pixels.
pub const CODE_FONT_SIZES: std::ops::RangeInclusive<u32> = 10..=24;
/// The range `tab_size` is clamped to, in columns.
pub const TAB_SIZES: std::ops::RangeInclusive<u32> = 1..=16;

impl AppearanceSettings {
    /// The settings with the font family trimmed (blank is unset) and both sizes clamped to
    /// `CODE_FONT_SIZES` and `TAB_SIZES`.
    pub fn clamped(&self) -> Self {
        let clamp =
            |n: u32, range: &std::ops::RangeInclusive<u32>| n.clamp(*range.start(), *range.end());
        Self {
            code_font: self
                .code_font
                .as_deref()
                .map(str::trim)
                .filter(|f| !f.is_empty())
                .map(str::to_string),
            code_font_size: self.code_font_size.map(|n| clamp(n, &CODE_FONT_SIZES)),
            ui_size: self.ui_size,
            tab_size: clamp(self.tab_size, &TAB_SIZES),
            date_format: self.date_format,
        }
    }
}

/// Desktop app update preferences, persisted as the `[updates]` TOML table (#630).
/// `check_automatically` (on by default) checks for a new version when the app starts and every
/// few hours; off, the app checks only when asked ("Check for updates"). The VS Code extension
/// updates through its marketplace and ignores this table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    pub check_automatically: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            check_automatically: true,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ConfigFile {
    // Recent repositories, newest first, at most `MAX_RECENT_REPOS`; never a pinned one.
    #[serde(default)]
    recent_repos: Vec<PathBuf>,
    // Pinned repositories (#766), in pin order: listed before `recent_repos`, no cap.
    #[serde(default)]
    pinned_repos: Vec<PathBuf>,
    #[serde(default)]
    active_repo: Option<PathBuf>,
    #[serde(default)]
    last_seen_version: Option<String>,
    // The repositories whose commit graph collapses merges (#617). A list of paths, so a plain
    // `key = [...]` line that must stay ahead of the tables below.
    #[serde(default)]
    graph_collapse_merges: Vec<PathBuf>,
    // `diff`, `tools`, `commit_message`, `fetch`, `appearance`, `updates` and `keybindings`
    // serialize as TOML tables (`[diff]`, `[tools]`, `[commit_message]`, `[fetch]`,
    // `[appearance]`, `[updates]`, `[keybindings]`).
    // Tables must follow plain `key = value` lines, so they sit after the plain keys.
    // The fields after it serialize as TOML array-of-tables because their element type is a
    // struct. TOML requires plain `key = value` lines and tables to precede an array-of-tables
    // section, so they must remain last.
    #[serde(default)]
    diff: DiffSettings,
    #[serde(default)]
    tools: ExternalToolSettings,
    #[serde(default)]
    commit_message: CommitMessageSettings,
    #[serde(default)]
    fetch: FetchSettings,
    #[serde(default)]
    appearance: AppearanceSettings,
    #[serde(default)]
    updates: UpdateSettings,
    #[serde(default, skip_serializing_if = "KeybindingSettings::is_empty")]
    keybindings: KeybindingSettings,
    #[serde(default, deserialize_with = "deserialize_open_repos")]
    open_repos: Vec<OpenRepoEntry>,
    #[serde(default)]
    workspaces: Vec<Workspace>,
    #[serde(default)]
    graph_branch_selections: Vec<GraphBranchSelection>,
    #[serde(default)]
    graph_remote_branch_selections: Vec<GraphRemoteBranchSelection>,
    #[serde(default)]
    merge_preferences: Vec<MergePreference>,
    #[serde(default)]
    pull_preferences: Vec<PullPreference>,
    #[serde(default)]
    repo_aliases: Vec<RepoAlias>,
}

fn config_file_path() -> Result<PathBuf, ConfigError> {
    if let Ok(dir) = std::env::var("BROWSITORY_CONFIG_DIR") {
        return Ok(PathBuf::from(dir).join("config.toml"));
    }
    let dirs = directories::ProjectDirs::from("com", "browsitory", "Browsitory")
        .ok_or(ConfigError::NoConfigDir)?;
    Ok(dirs.config_dir().join("config.toml"))
}

/// The pinned repositories (#766) in pin order, then the recent ones, newest first.
pub fn list_recent_repos() -> Result<Vec<PathBuf>, ConfigError> {
    list_recent_repos_at(&config_file_path()?)
}

/// Adds `path` to the front of the recent-repos list, de-duplicating it if already present
/// and capping the list at 10 entries (oldest dropped). A pinned `path` stays where it is.
pub fn add_recent_repo(path: &Path) -> Result<(), ConfigError> {
    add_recent_repo_at(&config_file_path()?, path)
}

/// Removes `path` from the recent-repos list (#690), unpinning it too; a path that isn't in it
/// is no error.
pub fn remove_recent_repo(path: &Path) -> Result<(), ConfigError> {
    remove_recent_repo_at(&config_file_path()?, path)
}

/// The pinned repositories (#766), in the order they were pinned.
pub fn list_pinned_repos() -> Result<Vec<PathBuf>, ConfigError> {
    list_pinned_repos_at(&config_file_path()?)
}

/// Pins `path` (#766): it is listed first, after the repositories pinned before it, and no longer
/// counts against the cap of 10 recent ones. Pinning a pinned path changes nothing.
pub fn pin_recent_repo(path: &Path) -> Result<(), ConfigError> {
    pin_recent_repo_at(&config_file_path()?, path)
}

/// Unpins `path` (#766): it becomes the newest recent repository (the oldest drops off past 10).
/// A path that isn't pinned is no error and changes nothing.
pub fn unpin_recent_repo(path: &Path) -> Result<(), ConfigError> {
    unpin_recent_repo_at(&config_file_path()?, path)
}

/// The recent repositories, pinned ones included, whose folder is gone (#690): each listed path
/// that isn't a directory now, in the list's order.
pub fn list_missing_recent_repos() -> Result<Vec<PathBuf>, ConfigError> {
    list_missing_recent_repos_at(&config_file_path()?)
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
    let config = read_config(config_file)?;
    let recent: Vec<PathBuf> = config
        .recent_repos
        .into_iter()
        .filter(|path| !config.pinned_repos.contains(path))
        .collect();
    let mut listed = config.pinned_repos;
    listed.extend(recent);
    Ok(listed)
}

pub fn add_recent_repo_at(config_file: &Path, path: &Path) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    if config.pinned_repos.iter().any(|p| p == path) {
        return Ok(());
    }
    config.recent_repos.retain(|p| p != path);
    config.recent_repos.insert(0, path.to_path_buf());
    config.recent_repos.truncate(MAX_RECENT_REPOS);
    write_config(config_file, &config)
}

pub fn remove_recent_repo_at(config_file: &Path, path: &Path) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.recent_repos.retain(|p| p != path);
    config.pinned_repos.retain(|p| p != path);
    write_config(config_file, &config)
}

pub fn list_pinned_repos_at(config_file: &Path) -> Result<Vec<PathBuf>, ConfigError> {
    Ok(read_config(config_file)?.pinned_repos)
}

pub fn pin_recent_repo_at(config_file: &Path, path: &Path) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    if config.pinned_repos.iter().any(|p| p == path) {
        return Ok(());
    }
    config.recent_repos.retain(|p| p != path);
    config.pinned_repos.push(path.to_path_buf());
    write_config(config_file, &config)
}

pub fn unpin_recent_repo_at(config_file: &Path, path: &Path) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    if !config.pinned_repos.iter().any(|p| p == path) {
        return Ok(());
    }
    config.pinned_repos.retain(|p| p != path);
    config.recent_repos.retain(|p| p != path);
    config.recent_repos.insert(0, path.to_path_buf());
    config.recent_repos.truncate(MAX_RECENT_REPOS);
    write_config(config_file, &config)
}

pub fn list_missing_recent_repos_at(config_file: &Path) -> Result<Vec<PathBuf>, ConfigError> {
    let recent = list_recent_repos_at(config_file)?;
    Ok(recent.into_iter().filter(|path| !path.is_dir()).collect())
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

pub fn get_diff_settings() -> Result<DiffSettings, ConfigError> {
    get_diff_settings_at(&config_file_path()?)
}

pub fn set_diff_settings(settings: &DiffSettings) -> Result<(), ConfigError> {
    set_diff_settings_at(&config_file_path()?, settings)
}

pub fn get_diff_settings_at(config_file: &Path) -> Result<DiffSettings, ConfigError> {
    Ok(read_config(config_file)?.diff)
}

/// Persists `settings`, clamping `context_lines` to `MAX_CONTEXT_LINES` on write.
pub fn set_diff_settings_at(
    config_file: &Path,
    settings: &DiffSettings,
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.diff = DiffSettings {
        context_lines: settings.context_lines.min(MAX_CONTEXT_LINES),
        ..*settings
    };
    write_config(config_file, &config)
}

pub fn get_commit_message_settings() -> Result<CommitMessageSettings, ConfigError> {
    get_commit_message_settings_at(&config_file_path()?)
}

pub fn set_commit_message_settings(settings: &CommitMessageSettings) -> Result<(), ConfigError> {
    set_commit_message_settings_at(&config_file_path()?, settings)
}

pub fn get_commit_message_settings_at(
    config_file: &Path,
) -> Result<CommitMessageSettings, ConfigError> {
    Ok(read_config(config_file)?.commit_message)
}

/// Persists `settings`, clamping both columns to `COMMIT_MESSAGE_COLUMNS` on write.
pub fn set_commit_message_settings_at(
    config_file: &Path,
    settings: &CommitMessageSettings,
) -> Result<(), ConfigError> {
    let clamp = |n: u32| {
        n.clamp(
            *COMMIT_MESSAGE_COLUMNS.start(),
            *COMMIT_MESSAGE_COLUMNS.end(),
        )
    };
    let mut config = read_config(config_file)?;
    config.commit_message = CommitMessageSettings {
        summary_limit: clamp(settings.summary_limit),
        body_ruler: clamp(settings.body_ruler),
        show_ruler: settings.show_ruler,
    };
    write_config(config_file, &config)
}

pub fn get_fetch_settings() -> Result<FetchSettings, ConfigError> {
    get_fetch_settings_at(&config_file_path()?)
}

pub fn set_fetch_settings(settings: &FetchSettings) -> Result<(), ConfigError> {
    set_fetch_settings_at(&config_file_path()?, settings)
}

pub fn get_fetch_settings_at(config_file: &Path) -> Result<FetchSettings, ConfigError> {
    Ok(read_config(config_file)?.fetch)
}

/// Persists `settings`, clamping `auto_fetch_minutes` to `MAX_AUTO_FETCH_MINUTES` on write.
pub fn set_fetch_settings_at(
    config_file: &Path,
    settings: &FetchSettings,
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.fetch = FetchSettings {
        auto_fetch_minutes: settings.auto_fetch_minutes.min(MAX_AUTO_FETCH_MINUTES),
    };
    write_config(config_file, &config)
}

pub fn get_appearance_settings() -> Result<AppearanceSettings, ConfigError> {
    get_appearance_settings_at(&config_file_path()?)
}

pub fn set_appearance_settings(settings: &AppearanceSettings) -> Result<(), ConfigError> {
    set_appearance_settings_at(&config_file_path()?, settings)
}

/// The stored settings, clamped (see `AppearanceSettings::clamped`).
pub fn get_appearance_settings_at(config_file: &Path) -> Result<AppearanceSettings, ConfigError> {
    Ok(read_config(config_file)?.appearance.clamped())
}

/// Persists `settings`, clamped (see `AppearanceSettings::clamped`).
pub fn set_appearance_settings_at(
    config_file: &Path,
    settings: &AppearanceSettings,
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.appearance = settings.clamped();
    write_config(config_file, &config)
}

pub fn get_update_settings() -> Result<UpdateSettings, ConfigError> {
    get_update_settings_at(&config_file_path()?)
}

pub fn set_update_settings(settings: &UpdateSettings) -> Result<(), ConfigError> {
    set_update_settings_at(&config_file_path()?, settings)
}

pub fn get_update_settings_at(config_file: &Path) -> Result<UpdateSettings, ConfigError> {
    Ok(read_config(config_file)?.updates)
}

pub fn set_update_settings_at(
    config_file: &Path,
    settings: &UpdateSettings,
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.updates = *settings;
    write_config(config_file, &config)
}

pub fn get_external_tool_settings() -> Result<ExternalToolSettings, ConfigError> {
    get_external_tool_settings_at(&config_file_path()?)
}

pub fn set_external_tool_settings(settings: &ExternalToolSettings) -> Result<(), ConfigError> {
    set_external_tool_settings_at(&config_file_path()?, settings)
}

pub fn get_external_tool_settings_at(
    config_file: &Path,
) -> Result<ExternalToolSettings, ConfigError> {
    Ok(read_config(config_file)?.tools)
}

/// Trims both commands; blank ones are stored as unset.
pub fn set_external_tool_settings_at(
    config_file: &Path,
    settings: &ExternalToolSettings,
) -> Result<(), ConfigError> {
    let clean = |value: &Option<String>| {
        value
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string)
    };
    let mut config = read_config(config_file)?;
    config.tools = ExternalToolSettings {
        editor: clean(&settings.editor),
        terminal: clean(&settings.terminal),
    };
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

pub fn set_graph_branch_selection(repo_path: &Path, local: &[String]) -> Result<(), ConfigError> {
    set_graph_branch_selection_at(&config_file_path()?, repo_path, local)
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
    local: &[String],
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config
        .graph_branch_selections
        .retain(|s| s.repo_path != repo_path);
    config.graph_branch_selections.push(GraphBranchSelection {
        repo_path: repo_path.to_path_buf(),
        selected_branches: local.to_vec(),
    });
    write_config(config_file, &config)
}

/// Forgets the saved graph branch selection of `repo_path`, so the graph shows every local
/// branch again, also ones created later ("Show all branches", #402). Not a setter taking
/// `None`: clearing is its own call, so no caller clears a selection by passing an empty value.
pub fn clear_graph_branch_selection(repo_path: &Path) -> Result<(), ConfigError> {
    clear_graph_branch_selection_at(&config_file_path()?, repo_path)
}

pub fn clear_graph_branch_selection_at(
    config_file: &Path,
    repo_path: &Path,
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config
        .graph_branch_selections
        .retain(|s| s.repo_path != repo_path);
    write_config(config_file, &config)
}

pub fn get_graph_remote_branch_selection(
    repo_path: &Path,
) -> Result<Option<Vec<String>>, ConfigError> {
    get_graph_remote_branch_selection_at(&config_file_path()?, repo_path)
}

pub fn set_graph_remote_branch_selection(
    repo_path: &Path,
    remote: &[String],
) -> Result<(), ConfigError> {
    set_graph_remote_branch_selection_at(&config_file_path()?, repo_path, remote)
}

pub fn get_graph_remote_branch_selection_at(
    config_file: &Path,
    repo_path: &Path,
) -> Result<Option<Vec<String>>, ConfigError> {
    let config = read_config(config_file)?;
    Ok(config
        .graph_remote_branch_selections
        .into_iter()
        .find(|s| s.repo_path == repo_path)
        .map(|s| s.selected_branches))
}

pub fn set_graph_remote_branch_selection_at(
    config_file: &Path,
    repo_path: &Path,
    remote: &[String],
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config
        .graph_remote_branch_selections
        .retain(|s| s.repo_path != repo_path);
    config
        .graph_remote_branch_selections
        .push(GraphRemoteBranchSelection {
            repo_path: repo_path.to_path_buf(),
            selected_branches: remote.to_vec(),
        });
    write_config(config_file, &config)
}

/// Forgets the saved remote graph branch selection of `repo_path`: the graph walks no remote
/// branch and shows every remote badge again (#402). See `clear_graph_branch_selection`.
pub fn clear_graph_remote_branch_selection(repo_path: &Path) -> Result<(), ConfigError> {
    clear_graph_remote_branch_selection_at(&config_file_path()?, repo_path)
}

pub fn clear_graph_remote_branch_selection_at(
    config_file: &Path,
    repo_path: &Path,
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config
        .graph_remote_branch_selections
        .retain(|s| s.repo_path != repo_path);
    write_config(config_file, &config)
}

/// Whether the commit graph of `repo_path` collapses merges ("Collapse merges", #617). Off until
/// turned on.
pub fn get_graph_collapse_merges(repo_path: &Path) -> Result<bool, ConfigError> {
    get_graph_collapse_merges_at(&config_file_path()?, repo_path)
}

pub fn set_graph_collapse_merges(repo_path: &Path, collapse: bool) -> Result<(), ConfigError> {
    set_graph_collapse_merges_at(&config_file_path()?, repo_path, collapse)
}

pub fn get_graph_collapse_merges_at(
    config_file: &Path,
    repo_path: &Path,
) -> Result<bool, ConfigError> {
    let config = read_config(config_file)?;
    Ok(config.graph_collapse_merges.iter().any(|p| p == repo_path))
}

pub fn set_graph_collapse_merges_at(
    config_file: &Path,
    repo_path: &Path,
    collapse: bool,
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.graph_collapse_merges.retain(|p| p != repo_path);
    if collapse {
        config.graph_collapse_merges.push(repo_path.to_path_buf());
    }
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
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(&mut tmp, toml::to_string_pretty(config)?.as_bytes())?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

pub fn get_merge_preference(repo_path: &Path) -> Result<Option<MergePreference>, ConfigError> {
    get_merge_preference_at(&config_file_path()?, repo_path)
}

pub fn set_merge_preference(
    repo_path: &Path,
    fast_forward: MergeFastForward,
    squash: bool,
) -> Result<(), ConfigError> {
    set_merge_preference_at(&config_file_path()?, repo_path, fast_forward, squash)
}

pub fn get_merge_preference_at(
    config_file: &Path,
    repo_path: &Path,
) -> Result<Option<MergePreference>, ConfigError> {
    let config = read_config(config_file)?;
    Ok(config
        .merge_preferences
        .into_iter()
        .find(|p| p.repo_path == repo_path))
}

pub fn set_merge_preference_at(
    config_file: &Path,
    repo_path: &Path,
    fast_forward: MergeFastForward,
    squash: bool,
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config
        .merge_preferences
        .retain(|p| p.repo_path != repo_path);
    config.merge_preferences.push(MergePreference {
        repo_path: repo_path.to_path_buf(),
        fast_forward,
        squash,
    });
    write_config(config_file, &config)
}

pub fn get_pull_preference(
    repo_path: &Path,
) -> Result<Option<PullStrategyPreference>, ConfigError> {
    get_pull_preference_at(&config_file_path()?, repo_path)
}

pub fn set_pull_preference(
    repo_path: &Path,
    strategy: PullStrategyPreference,
) -> Result<(), ConfigError> {
    set_pull_preference_at(&config_file_path()?, repo_path, strategy)
}

pub fn get_pull_preference_at(
    config_file: &Path,
    repo_path: &Path,
) -> Result<Option<PullStrategyPreference>, ConfigError> {
    Ok(read_config(config_file)?
        .pull_preferences
        .into_iter()
        .find(|p| p.repo_path == repo_path)
        .map(|p| p.strategy))
}

pub fn set_pull_preference_at(
    config_file: &Path,
    repo_path: &Path,
    strategy: PullStrategyPreference,
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.pull_preferences.retain(|p| p.repo_path != repo_path);
    config.pull_preferences.push(PullPreference {
        repo_path: repo_path.to_path_buf(),
        strategy,
    });
    write_config(config_file, &config)
}

pub fn list_repo_aliases() -> Result<Vec<RepoAlias>, ConfigError> {
    list_repo_aliases_at(&config_file_path()?)
}

pub fn set_repo_alias(repo_path: &Path, alias: Option<&str>) -> Result<(), ConfigError> {
    set_repo_alias_at(&config_file_path()?, repo_path, alias)
}

pub fn list_repo_aliases_at(config_file: &Path) -> Result<Vec<RepoAlias>, ConfigError> {
    Ok(read_config(config_file)?.repo_aliases)
}

/// Stores `alias`, trimmed, as the tab name of `repo_path`. `None` or a blank alias removes it,
/// so the tab shows the folder name again.
pub fn set_repo_alias_at(
    config_file: &Path,
    repo_path: &Path,
    alias: Option<&str>,
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.repo_aliases.retain(|a| a.repo_path != repo_path);
    if let Some(alias) = alias.map(str::trim).filter(|a| !a.is_empty()) {
        config.repo_aliases.push(RepoAlias {
            repo_path: repo_path.to_path_buf(),
            alias: alias.to_string(),
        });
    }
    write_config(config_file, &config)
}

pub fn get_keybinding_settings() -> Result<KeybindingSettings, ConfigError> {
    get_keybinding_settings_at(&config_file_path()?)
}

pub fn set_keybinding_settings(settings: &KeybindingSettings) -> Result<(), ConfigError> {
    set_keybinding_settings_at(&config_file_path()?, settings)
}

pub fn get_keybinding_settings_at(config_file: &Path) -> Result<KeybindingSettings, ConfigError> {
    Ok(read_config(config_file)?.keybindings)
}

/// Persists `settings`; no overrides leave no `[keybindings]` table.
pub fn set_keybinding_settings_at(
    config_file: &Path,
    settings: &KeybindingSettings,
) -> Result<(), ConfigError> {
    let mut config = read_config(config_file)?;
    config.keybindings = settings.clone();
    write_config(config_file, &config)
}
