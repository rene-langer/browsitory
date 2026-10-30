use super::{repo_worker, AppState, ChangedFileDto, CompareRangeDto, DiffHunkDto, DiffOptionsDto};
use tauri::State;

#[tauri::command]
pub async fn get_range_files(
    repo_path: String,
    from_id: String,
    to_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ChangedFileDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_range_files(from_id, to_id))
        .await?
        .into_iter()
        .map(ChangedFileDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_range_diff(
    repo_path: String,
    from_id: String,
    to_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptionsDto,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_range_diff(from_id, to_id, path, old_path, opts.into()))
        .await?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_working_tree_compare_files(
    repo_path: String,
    commit_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ChangedFileDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_working_tree_compare_files(commit_id))
        .await?
        .into_iter()
        .map(ChangedFileDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_working_tree_compare_diff(
    repo_path: String,
    commit_id: String,
    path: String,
    old_path: Option<String>,
    opts: DiffOptionsDto,
    state: State<'_, AppState>,
) -> Result<Vec<DiffHunkDto>, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_working_tree_compare_diff(commit_id, path, old_path, opts.into()))
        .await?
        .into_iter()
        .map(DiffHunkDto::from)
        .collect())
}

#[tauri::command]
pub async fn get_branch_compare_range(
    repo_path: String,
    ref_name: String,
    state: State<'_, AppState>,
) -> Result<CompareRangeDto, String> {
    Ok(repo_worker(&state, &repo_path)?
        .call(move |w| w.get_branch_compare_range(ref_name))
        .await?
        .into())
}
