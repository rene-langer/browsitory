//! User-defined commands (#893): stored app-wide and per repository in the app's config, never
//! read from a repository.

use std::path::Path;

use repo_service::wire::{
    ErrorDto, UserCommandCheckDto, UserCommandDto, UserCommandInput, UserCommandsDto,
};

/// The app-wide commands and, with `repo_path`, that repository's own.
#[tauri::command]
pub fn list_user_commands(repo_path: Option<String>) -> Result<UserCommandsDto, ErrorDto> {
    config::list_user_commands(repo_path.as_deref().map(Path::new))
        .map(Into::into)
        .map_err(ErrorDto::from_error)
}

/// Replaces the app-wide commands (`repo_path` `None`) or one repository's; refuses an invalid
/// one, storing nothing.
#[tauri::command]
pub fn set_user_commands(
    repo_path: Option<String>,
    commands: Vec<UserCommandInput>,
) -> Result<Vec<UserCommandDto>, ErrorDto> {
    let commands: Vec<config::UserCommand> = commands.into_iter().map(Into::into).collect();
    let stored = config::set_user_commands(repo_path.as_deref().map(Path::new), &commands)
        .map_err(ErrorDto::from_error)?;
    Ok(stored.into_iter().map(Into::into).collect())
}

/// How a command line splits and where its program is, without running it.
#[tauri::command]
pub fn check_user_command(
    command_line: String,
    working_directory: Option<String>,
) -> Result<UserCommandCheckDto, ErrorDto> {
    UserCommandCheckDto::check(&command_line, working_directory.as_deref())
}
