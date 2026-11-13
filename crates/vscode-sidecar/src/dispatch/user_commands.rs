//! User-defined commands (#893): stored app-wide and per repository in the app's config, never
//! read from a repository.

use std::path::Path;

use repo_service::wire::UserCommandsDto;
use repo_service::wire::{ErrorDto, UserCommandCheckDto, UserCommandDto, UserCommandInput};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ListUserCommandsParams {
    repo_path: Option<String>,
}

pub(super) fn list_user_commands(params: ListUserCommandsParams) -> Result<Value, ErrorDto> {
    let commands = config::list_user_commands(params.repo_path.as_deref().map(Path::new))
        .map_err(ErrorDto::from_error)?;
    serde_json::to_value(UserCommandsDto::from(commands)).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SetUserCommandsParams {
    repo_path: Option<String>,
    commands: Vec<UserCommandInput>,
}

pub(super) fn set_user_commands(params: SetUserCommandsParams) -> Result<Value, ErrorDto> {
    let commands: Vec<config::UserCommand> = params.commands.into_iter().map(Into::into).collect();
    let stored = config::set_user_commands(params.repo_path.as_deref().map(Path::new), &commands)
        .map_err(ErrorDto::from_error)?;
    let stored: Vec<UserCommandDto> = stored.into_iter().map(Into::into).collect();
    serde_json::to_value(stored).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CheckUserCommandParams {
    command_line: String,
    working_directory: Option<String>,
}

pub(super) fn check_user_command(params: CheckUserCommandParams) -> Result<Value, ErrorDto> {
    let check =
        UserCommandCheckDto::check(&params.command_line, params.working_directory.as_deref())?;
    serde_json::to_value(check).map_err(ErrorDto::from_error)
}
