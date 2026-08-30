use std::collections::HashMap;

use git_core::graph::GraphCommit;
use git_core::status::StatusEntry;
use repo_service::worker::Worker;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn dispatch(
    method: &str,
    params: Value,
    repos: &mut HashMap<String, Worker>,
) -> Result<Value, String> {
    match method {
        "open_repo" => open_repo(params, repos),
        "close_repo" => close_repo(params, repos),
        "get_status" => get_status(params, repos),
        "get_commit_graph" => get_commit_graph(params, repos),
        other => Err(format!("unknown method: {other}")),
    }
}

fn worker_handle(
    repos: &HashMap<String, Worker>,
    repo_path: &str,
) -> Result<repo_service::worker::WorkerHandle, String> {
    repos
        .get(repo_path)
        .map(Worker::handle)
        .ok_or_else(|| format!("repo not open: {repo_path}"))
}

#[derive(Deserialize)]
struct OpenRepoParams {
    path: String,
}

fn open_repo(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: OpenRepoParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    if !repos.contains_key(&params.path) {
        let worker = Worker::spawn(params.path.clone().into())?;
        repos.insert(params.path, worker);
    }
    Ok(Value::Null)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepoPathParams {
    repo_path: String,
}

fn close_repo(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    repos.remove(&params.repo_path);
    Ok(Value::Null)
}

#[derive(Serialize)]
struct StatusEntryDto {
    path: String,
    staged: bool,
    kind: String,
}

impl From<StatusEntry> for StatusEntryDto {
    fn from(entry: StatusEntry) -> Self {
        Self {
            path: entry.path,
            staged: entry.staged,
            kind: format!("{:?}", entry.kind),
        }
    }
}

fn get_status(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: RepoPathParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let entries: Vec<StatusEntryDto> = worker_handle(repos, &params.repo_path)?
        .get_status()?
        .into_iter()
        .map(StatusEntryDto::from)
        .collect();
    serde_json::to_value(entries).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetCommitGraphParams {
    repo_path: String,
    limit: usize,
    selected_branches: Option<Vec<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GraphCommitDto {
    id: String,
    short_id: String,
    summary: String,
    author_name: String,
    author_email: String,
    timestamp: i64,
    parent_ids: Vec<String>,
    branch_refs: Vec<String>,
}

impl From<GraphCommit> for GraphCommitDto {
    fn from(c: GraphCommit) -> Self {
        Self {
            id: c.id,
            short_id: c.short_id,
            summary: c.summary,
            author_name: c.author_name,
            author_email: c.author_email,
            timestamp: c.timestamp,
            parent_ids: c.parent_ids,
            branch_refs: c.branch_refs,
        }
    }
}

fn get_commit_graph(params: Value, repos: &mut HashMap<String, Worker>) -> Result<Value, String> {
    let params: GetCommitGraphParams =
        serde_json::from_value(params).map_err(|error| error.to_string())?;
    let commits: Vec<GraphCommitDto> = worker_handle(repos, &params.repo_path)?
        .get_commit_graph(params.limit, params.selected_branches)?
        .into_iter()
        .map(GraphCommitDto::from)
        .collect();
    serde_json::to_value(commits).map_err(|error| error.to_string())
}
