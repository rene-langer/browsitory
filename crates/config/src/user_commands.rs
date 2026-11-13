//! User-defined commands (#893): kept app-wide (`[[user_commands]]`) and per repository
//! (`[[repo_user_commands]]`, by path), both in the app's own config file. A repository can never
//! define a command: nothing is read from inside it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{config_file_path, ConfigError};

/// Where a command appears besides the command palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserCommandTarget {
    Commit,
    Branch,
    Tag,
    File,
    Repository,
}

fn yes() -> bool {
    true
}

/// One user command, as stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserCommand {
    /// Stable within its list; given on save when blank (see `list_user_commands_at`).
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub command_line: String,
    /// `None`: the repository's root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    #[serde(default)]
    pub targets: Vec<UserCommandTarget>,
    #[serde(default)]
    pub confirm: bool,
    #[serde(default = "yes")]
    pub show_output: bool,
    #[serde(default = "yes")]
    pub refresh: bool,
}

/// The commands of one repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoUserCommands {
    pub repo_path: PathBuf,
    pub commands: Vec<UserCommand>,
}

/// The app-wide commands, and those of one repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserCommands {
    pub global: Vec<UserCommand>,
    pub repository: Vec<UserCommand>,
}

pub fn list_user_commands(repo_path: Option<&Path>) -> Result<UserCommands, ConfigError> {
    list_user_commands_at(&config_file_path()?, repo_path)
}

pub fn set_user_commands(
    repo_path: Option<&Path>,
    commands: &[UserCommand],
) -> Result<Vec<UserCommand>, ConfigError> {
    set_user_commands_at(&config_file_path()?, repo_path, commands)
}

/// The app-wide commands and, with `repo_path`, that repository's own.
pub fn list_user_commands_at(
    config_file: &Path,
    repo_path: Option<&Path>,
) -> Result<UserCommands, ConfigError> {
    let _ = (config_file, repo_path);
    todo!()
}

/// Replaces the app-wide commands (`repo_path` `None`) or that repository's, in the given order,
/// and returns them as stored. Every command is checked first, so nothing is stored when one is
/// invalid. A blank or repeated id gets a new one. An empty list for a repository removes its
/// entry.
pub fn set_user_commands_at(
    config_file: &Path,
    repo_path: Option<&Path>,
    commands: &[UserCommand],
) -> Result<Vec<UserCommand>, ConfigError> {
    let _ = (config_file, repo_path, commands);
    todo!()
}
