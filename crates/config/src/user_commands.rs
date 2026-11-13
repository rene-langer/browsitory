//! User-defined commands (#893): kept app-wide (`[[user_commands]]`) and per repository
//! (`[[repo_user_commands]]`, by path), both in the app's own config file. A repository can never
//! define a command: nothing is read from inside it.

use std::path::{Path, PathBuf};

use git_core::process::CommandStyle;
use serde::{Deserialize, Serialize};

use super::{config_file_path, generate_workspace_id, read_config, write_config, ConfigError};

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

/// A command without an id (written by hand) is listed as `global-<n>` or `repository-<n>`, its
/// 1-based position, so it can still be run.
fn with_ids(mut commands: Vec<UserCommand>, scope: &str) -> Vec<UserCommand> {
    for (index, command) in commands.iter_mut().enumerate() {
        if command.id.trim().is_empty() {
            command.id = format!("{scope}-{}", index + 1);
        }
    }
    commands
}

/// The app-wide commands and, with `repo_path`, that repository's own.
pub fn list_user_commands_at(
    config_file: &Path,
    repo_path: Option<&Path>,
) -> Result<UserCommands, ConfigError> {
    let config = read_config(config_file)?;
    let repository = repo_path
        .and_then(|path| {
            config
                .repo_user_commands
                .iter()
                .find(|entry| entry.repo_path == path)
        })
        .map(|entry| entry.commands.clone())
        .unwrap_or_default();
    Ok(UserCommands {
        global: with_ids(config.user_commands, "global"),
        repository: with_ids(repository, "repository"),
    })
}

/// Checks and cleans one command: trimmed name and working directory (blank is `None`), the
/// command line as typed, targets sorted without repeats.
fn clean(command: &UserCommand) -> Result<UserCommand, ConfigError> {
    let name = command.name.trim();
    let invalid = |problem: String| {
        let label = if name.is_empty() { "A command" } else { name };
        ConfigError::InvalidUserCommand(format!("{label}: {problem}"))
    };
    if name.is_empty() {
        return Err(invalid("The name is empty.".to_string()));
    }
    let working_directory = command
        .working_directory
        .as_deref()
        .map(str::trim)
        .filter(|dir| !dir.is_empty())
        .map(str::to_string);
    git_core::user_command::parse(
        &command.command_line,
        working_directory.as_deref(),
        CommandStyle::native(),
    )
    .map_err(|error| invalid(error.to_string()))?;
    let mut targets = command.targets.clone();
    targets.sort();
    targets.dedup();
    Ok(UserCommand {
        id: command.id.trim().to_string(),
        name: name.to_string(),
        command_line: command.command_line.clone(),
        working_directory,
        targets,
        confirm: command.confirm,
        show_output: command.show_output,
        refresh: command.refresh,
    })
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
    let mut cleaned = commands.iter().map(clean).collect::<Result<Vec<_>, _>>()?;
    let mut seen = std::collections::HashSet::new();
    for command in &mut cleaned {
        if command.id.is_empty() || !seen.insert(command.id.clone()) {
            command.id = generate_workspace_id();
            seen.insert(command.id.clone());
        }
    }
    let mut config = read_config(config_file)?;
    match repo_path {
        None => config.user_commands = cleaned.clone(),
        Some(path) => {
            config
                .repo_user_commands
                .retain(|entry| entry.repo_path != path);
            if !cleaned.is_empty() {
                config.repo_user_commands.push(RepoUserCommands {
                    repo_path: path.to_path_buf(),
                    commands: cleaned.clone(),
                });
            }
        }
    }
    write_config(config_file, &config)?;
    Ok(cleaned)
}
