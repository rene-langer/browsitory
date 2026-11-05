use repo_service::wire::{CommitMessageSettingsDto, DiffSettingsDto};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
pub(super) struct SetDiffSettingsParams {
    settings: DiffSettingsDto,
}

pub(super) fn get_diff_settings() -> Result<Value, String> {
    let settings = config::get_diff_settings().map_err(|error| error.to_string())?;
    serde_json::to_value(DiffSettingsDto::from(settings)).map_err(|error| error.to_string())
}

pub(super) fn set_diff_settings(params: SetDiffSettingsParams) -> Result<Value, String> {
    config::set_diff_settings(&params.settings.into()).map_err(|error| error.to_string())?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
pub(super) struct SetCommitMessageSettingsParams {
    settings: CommitMessageSettingsDto,
}

pub(super) fn get_commit_message_settings() -> Result<Value, String> {
    let settings = config::get_commit_message_settings().map_err(|error| error.to_string())?;
    serde_json::to_value(CommitMessageSettingsDto::from(settings))
        .map_err(|error| error.to_string())
}

pub(super) fn set_commit_message_settings(
    params: SetCommitMessageSettingsParams,
) -> Result<Value, String> {
    config::set_commit_message_settings(&params.settings.into())
        .map_err(|error| error.to_string())?;
    Ok(Value::Null)
}
