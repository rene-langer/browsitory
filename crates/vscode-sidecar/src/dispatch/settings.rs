use repo_service::wire::ErrorDto;
use repo_service::wire::{
    AppearanceSettingsDto, CommitMessageSettingsDto, DiffSettingsDto, FetchSettingsDto,
    FontFamilyDto, UpdateSettingsDto,
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

/// The installed font families for the code font suggestions (#858).
pub(super) fn list_font_families() -> Result<Value, ErrorDto> {
    let families: Vec<FontFamilyDto> = repo_service::fonts::installed_font_families()
        .into_iter()
        .map(FontFamilyDto::from)
        .collect();
    serde_json::to_value(families).map_err(ErrorDto::from_error)
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
