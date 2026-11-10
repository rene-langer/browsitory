use repo_service::wire::ErrorDto;
use repo_service::wire::{
    AppearanceSettingsDto, CommitMessageSettingsDto, DiffSettingsDto, FetchSettingsDto,
    UpdateSettingsDto,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
pub(super) struct SetDiffSettingsParams {
    settings: DiffSettingsDto,
}

pub(super) fn get_diff_settings() -> Result<Value, ErrorDto> {
    let settings = config::get_diff_settings().map_err(ErrorDto::from_error)?;
    serde_json::to_value(DiffSettingsDto::from(settings)).map_err(ErrorDto::from_error)
}

pub(super) fn set_diff_settings(params: SetDiffSettingsParams) -> Result<Value, ErrorDto> {
    config::set_diff_settings(&params.settings.into()).map_err(ErrorDto::from_error)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
pub(super) struct SetCommitMessageSettingsParams {
    settings: CommitMessageSettingsDto,
}

pub(super) fn get_commit_message_settings() -> Result<Value, ErrorDto> {
    let settings = config::get_commit_message_settings().map_err(ErrorDto::from_error)?;
    serde_json::to_value(CommitMessageSettingsDto::from(settings)).map_err(ErrorDto::from_error)
}

pub(super) fn set_commit_message_settings(
    params: SetCommitMessageSettingsParams,
) -> Result<Value, ErrorDto> {
    config::set_commit_message_settings(&params.settings.into()).map_err(ErrorDto::from_error)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
pub(super) struct SetFetchSettingsParams {
    settings: FetchSettingsDto,
}

pub(super) fn get_fetch_settings() -> Result<Value, ErrorDto> {
    let settings = config::get_fetch_settings().map_err(ErrorDto::from_error)?;
    serde_json::to_value(FetchSettingsDto::from(settings)).map_err(ErrorDto::from_error)
}

pub(super) fn set_fetch_settings(params: SetFetchSettingsParams) -> Result<Value, ErrorDto> {
    config::set_fetch_settings(&params.settings.into()).map_err(ErrorDto::from_error)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
pub(super) struct SetAppearanceSettingsParams {
    settings: AppearanceSettingsDto,
}

pub(super) fn get_appearance_settings() -> Result<Value, ErrorDto> {
    let settings = config::get_appearance_settings().map_err(ErrorDto::from_error)?;
    serde_json::to_value(AppearanceSettingsDto::from(settings)).map_err(ErrorDto::from_error)
}

pub(super) fn set_appearance_settings(
    params: SetAppearanceSettingsParams,
) -> Result<Value, ErrorDto> {
    config::set_appearance_settings(&params.settings.into()).map_err(ErrorDto::from_error)?;
    Ok(Value::Null)
}

#[derive(Deserialize)]
pub(super) struct SetUpdateSettingsParams {
    settings: UpdateSettingsDto,
}

pub(super) fn get_update_settings() -> Result<Value, ErrorDto> {
    let settings = config::get_update_settings().map_err(ErrorDto::from_error)?;
    serde_json::to_value(UpdateSettingsDto::from(settings)).map_err(ErrorDto::from_error)
}

pub(super) fn set_update_settings(params: SetUpdateSettingsParams) -> Result<Value, ErrorDto> {
    config::set_update_settings(&params.settings.into()).map_err(ErrorDto::from_error)?;
    Ok(Value::Null)
}
