use repo_service::wire::ErrorDto;
use repo_service::wire::{UndoOutcomeDto, UndoStateDto};
use serde_json::Value;

use super::{worker_handle, RepoPathParams, Repos};

pub(super) fn get_undo_state(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let result = worker_handle(repos, &params.repo_path)?.get_undo_state()?;
    serde_json::to_value(UndoStateDto::from(result)).map_err(ErrorDto::from_error)
}

pub(super) fn undo_last(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let result = worker_handle(repos, &params.repo_path)?.undo()?;
    serde_json::to_value(UndoOutcomeDto::from(result)).map_err(ErrorDto::from_error)
}

pub(super) fn redo_last(params: RepoPathParams, repos: &Repos) -> Result<Value, ErrorDto> {
    let result = worker_handle(repos, &params.repo_path)?.redo()?;
    serde_json::to_value(UndoOutcomeDto::from(result)).map_err(ErrorDto::from_error)
}
