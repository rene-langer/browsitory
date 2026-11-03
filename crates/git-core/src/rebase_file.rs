//! The wire format of Browsitory's own paused rebase, `<gitdir>/browsitory/rebase.json`
//! ([`file`]): repo-service's rebase store writes and loads it for its own worktree, and
//! `repo::branches_held` loads other worktrees' files the same way (#291), so a file
//! counts there exactly when its own worktree would load it.
//!
//! Versioning follows `sequence_store`: a later version that only adds fields with a safe
//! default bumps `FORMAT_VERSION` and keeps reading the older one (`OLDEST_READABLE_VERSION`).
//! An unknown version is refused, and the next `StartRebase` overwrites it.
use std::path::{Path, PathBuf};

use git2::{Oid, Repository};
use serde::{Deserialize, Serialize};

use crate::rebase::{RebaseAction, RebasePlanEntry, RebaseState, RebaseStop};

/// The file in a worktree's own git folder (`git_dir`): every worktree has its own rebase.
pub fn file(git_dir: &Path) -> PathBuf {
    git_dir.join("browsitory").join("rebase.json")
}

/// The file's contents for `state`.
pub fn encode(state: &RebaseState) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(&StoredRebase::from(state)).map_err(|error| error.to_string())
}

/// The state in `bytes`, checked against `repo` ([`to_state`]); `Err` says why it can't be used.
pub fn decode(bytes: &[u8], repo: &Repository) -> Result<RebaseState, String> {
    let stored = serde_json::from_slice::<StoredRebase>(bytes)
        .map_err(|error| format!("malformed: {error}"))?;
    if !(OLDEST_READABLE_VERSION..=FORMAT_VERSION).contains(&stored.version) {
        return Err(format!("unknown version {}", stored.version));
    }
    to_state(stored, repo)
}

const FORMAT_VERSION: u32 = 1;
const OLDEST_READABLE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredRebase {
    version: u32,
    original_branch_ref: String,
    original_tip: String,
    onto: String,
    plan: Vec<StoredEntry>,
    cursor: usize,
    group_start_parent: Option<String>,
    expected_head: String,
    stop: StoredStop,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredEntry {
    commit_id: String,
    action: StoredAction,
    combined_message: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind")]
enum StoredAction {
    Pick,
    Reword { message: String },
    Edit,
    Squash,
    Fixup,
    Drop,
}

#[derive(Serialize, Deserialize)]
enum StoredStop {
    Conflict,
    Edit,
    Error,
}

impl From<&RebaseState> for StoredRebase {
    fn from(state: &RebaseState) -> Self {
        Self {
            version: FORMAT_VERSION,
            original_branch_ref: state.original_branch_ref.clone(),
            original_tip: state.original_tip.to_string(),
            onto: state.onto.to_string(),
            plan: state
                .plan
                .iter()
                .map(|entry| StoredEntry {
                    commit_id: entry.commit_id.clone(),
                    action: match &entry.action {
                        RebaseAction::Pick => StoredAction::Pick,
                        RebaseAction::Reword { message } => StoredAction::Reword {
                            message: message.clone(),
                        },
                        RebaseAction::Edit => StoredAction::Edit,
                        RebaseAction::Squash => StoredAction::Squash,
                        RebaseAction::Fixup => StoredAction::Fixup,
                        RebaseAction::Drop => StoredAction::Drop,
                    },
                    combined_message: entry.combined_message.clone(),
                })
                .collect(),
            cursor: state.cursor,
            group_start_parent: state.group_start_parent.map(|oid| oid.to_string()),
            expected_head: state.expected_head.to_string(),
            stop: match state.stop {
                RebaseStop::Conflict => StoredStop::Conflict,
                RebaseStop::Edit => StoredStop::Edit,
                RebaseStop::Error => StoredStop::Error,
            },
        }
    }
}

/// Rebuilds the state, checking it against `repo`: the plan is non-empty with the cursor in
/// range and not on a `Drop`, the branch is a valid `refs/heads/` name, every id is a commit
/// here, and a `Reword` has a message.
fn to_state(stored: StoredRebase, repo: &Repository) -> Result<RebaseState, String> {
    if stored.plan.is_empty() || stored.cursor > stored.plan.len() {
        return Err(format!(
            "step {} of {} is out of range",
            stored.cursor + 1,
            stored.plan.len()
        ));
    }
    let branch = &stored.original_branch_ref;
    if !branch.starts_with("refs/heads/") || !git2::Reference::is_valid_name(branch) {
        return Err(format!("{branch:?} is not a branch"));
    }
    let commit = |id: &str| -> Result<Oid, String> {
        Oid::from_str(id)
            .ok()
            .filter(|oid| repo.find_commit(*oid).is_ok())
            .ok_or_else(|| format!("{id:?} is not a commit in this repository"))
    };
    let mut plan = Vec::with_capacity(stored.plan.len());
    for entry in stored.plan {
        commit(&entry.commit_id)?;
        let action = match entry.action {
            StoredAction::Pick => RebaseAction::Pick,
            StoredAction::Reword { message } if message.trim().is_empty() => {
                return Err("a reworded step has no message".to_string())
            }
            StoredAction::Reword { message } => RebaseAction::Reword { message },
            StoredAction::Edit => RebaseAction::Edit,
            StoredAction::Squash => RebaseAction::Squash,
            StoredAction::Fixup => RebaseAction::Fixup,
            StoredAction::Drop => RebaseAction::Drop,
        };
        plan.push(RebasePlanEntry {
            commit_id: entry.commit_id,
            action,
            combined_message: entry.combined_message,
        });
    }
    if plan
        .get(stored.cursor)
        .is_some_and(|entry| entry.action == RebaseAction::Drop)
    {
        return Err("the current step is dropped".to_string());
    }
    Ok(RebaseState {
        plan,
        cursor: stored.cursor,
        original_tip: commit(&stored.original_tip)?,
        onto: commit(&stored.onto)?,
        group_start_parent: stored
            .group_start_parent
            .as_deref()
            .map(commit)
            .transpose()?,
        expected_head: commit(&stored.expected_head)?,
        original_branch_ref: stored.original_branch_ref,
        stop: match stored.stop {
            StoredStop::Conflict => RebaseStop::Conflict,
            StoredStop::Edit => RebaseStop::Edit,
            StoredStop::Error => RebaseStop::Error,
        },
    })
}
