//! User-defined commands (#893): stored app-wide and per repository in the app's config, never
//! read from a repository.

use std::path::Path;

use repo_service::wire::{ErrorDto, UserCommandCheckDto, UserCommandDto, UserCommandInput};
use repo_service::wire::{UserCommandContextDto, UserCommandsDto};
use serde::Deserialize;
use serde_json::Value;

use super::{worker_handle, Repos};

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RunUserCommandParams {
    repo_path: String,
    command_id: String,
    run_id: String,
    context: UserCommandContextDto,
}

/// Runs a stored command (#894) on this request's own thread (`main.rs` gives each request
/// one), so the repository and every other request go on while it runs.
pub(super) fn run_user_command(
    params: RunUserCommandParams,
    repos: &Repos,
) -> Result<Value, ErrorDto> {
    let reads = worker_handle(repos, &params.repo_path)?.reads();
    let prepared = repo_service::user_commands::prepare(
        &reads,
        Path::new(&params.repo_path),
        &params.command_id,
        params.context.into(),
    )?;
    let outcome = repo_service::user_commands::execute(&prepared, &params.run_id)?;
    serde_json::to_value(outcome).map_err(ErrorDto::from_error)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct CancelUserCommandParams {
    run_id: String,
}

pub(super) fn cancel_user_command(params: CancelUserCommandParams) -> Result<Value, ErrorDto> {
    repo_service::user_commands::cancel(&params.run_id);
    Ok(Value::Null)
}
