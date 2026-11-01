use super::{
    emit_hook_events, hook_event_channel, repo_worker, trailers_from, AppState, CommitOutcomeDto,
    HeadInfoDto, PickOutcomeDto, ResetModeDto, SequenceAbortDto, SequenceOutcomeDto,
    SignatureInputDto, TrailerDto,
};
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn get_head_info(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<HeadInfoDto, String> {
    Ok(HeadInfoDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.get_head_info())
            .await?,
    ))
}

/// `author` replaces HEAD's author name and email (keeping its time); `trailers` (absent: none)
/// are appended before the hooks run.
#[tauri::command]
pub async fn amend_commit(
    repo_path: String,
    message: String,
    no_verify: bool,
    author: Option<SignatureInputDto>,
    trailers: Option<Vec<TrailerDto>>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CommitOutcomeDto, String> {
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    emit_hook_events(app, repo_path.clone(), event_rx);
    Ok(CommitOutcomeDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| {
                w.amend_commit(
                    message,
                    no_verify,
                    author.map(Into::into),
                    trailers_from(trailers),
                    event_tx,
                )
            })
            .await?,
    ))
}

/// The commit box's co-author suggestions.
#[tauri::command]
pub async fn list_recent_authors(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<Vec<SignatureInputDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.list_recent_authors())
        .await?
        .into_iter()
        .map(Into::into)
        .collect())
}

/// A clean pick's `prepare-commit-msg`/`post-commit` are reported as `hook-progress` events
/// with `source: "Pick"` (#183).
#[tauri::command]
pub async fn cherry_pick(
    repo_path: String,
    commit_id: String,
    mainline: Option<u32>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PickOutcomeDto, String> {
    let events = hook_event_channel(app, &repo_path);
    Ok(PickOutcomeDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.cherry_pick(commit_id, mainline, events))
            .await?,
    ))
}

#[tauri::command]
pub async fn revert_commit(
    repo_path: String,
    commit_id: String,
    mainline: Option<u32>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<PickOutcomeDto, String> {
    let events = hook_event_channel(app, &repo_path);
    Ok(PickOutcomeDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.revert_commit(commit_id, mainline, events))
            .await?,
    ))
}

/// `commit_ids` oldest-first; a conflict pauses the sequence until continue/abort.
#[tauri::command]
pub async fn cherry_pick_many(
    repo_path: String,
    commit_ids: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SequenceOutcomeDto, String> {
    let events = hook_event_channel(app, &repo_path);
    Ok(SequenceOutcomeDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.cherry_pick_many(commit_ids, events))
            .await?,
    ))
}

/// `commit_ids` oldest-first; they are reverted newest-first.
#[tauri::command]
pub async fn revert_many(
    repo_path: String,
    commit_ids: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SequenceOutcomeDto, String> {
    let events = hook_event_channel(app, &repo_path);
    Ok(SequenceOutcomeDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.revert_many(commit_ids, events))
            .await?,
    ))
}

/// The step Continue commits runs the commit hooks (`no_verify`, absent: false), reported as
/// `hook-progress` events (#133).
#[tauri::command]
pub async fn continue_sequence(
    repo_path: String,
    no_verify: Option<bool>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SequenceOutcomeDto, String> {
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    emit_hook_events(app, repo_path.clone(), event_rx);
    Ok(SequenceOutcomeDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.continue_sequence(no_verify.unwrap_or(false), event_tx))
            .await?,
    ))
}

#[tauri::command]
pub async fn abort_sequence(
    repo_path: String,
    state: State<'_, AppState>,
) -> Result<SequenceAbortDto, String> {
    Ok(SequenceAbortDto::from(
        repo_worker(&state, &repo_path)?
            .call(move |w| w.abort_sequence())
            .await?,
    ))
}

#[tauri::command]
pub async fn reset_to_commit(
    repo_path: String,
    commit_id: String,
    mode: ResetModeDto,
    state: State<'_, AppState>,
) -> Result<(), String> {
    repo_worker(&state, &repo_path)?
        .call(move |w| w.reset_to_commit(commit_id, mode.into()))
        .await
}

/// `post-checkout` is reported as `hook-progress` events with `source: "Checkout"` (#185).
#[tauri::command]
pub async fn checkout_commit(
    repo_path: String,
    commit_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let events = hook_event_channel(app, &repo_path);
    repo_worker(&state, &repo_path)?
        .call(move |w| w.checkout_commit(commit_id, events))
        .await
}
