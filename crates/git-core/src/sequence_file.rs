//! The wire format of Browsitory's own stopped cherry-pick/revert sequence,
//! `<gitdir>/browsitory/sequence.json` ([`file`]): repo-service's sequence store writes and
//! loads it for its own worktree, and `repo::branches_held` loads other worktrees' files the
//! same way (#293), so a file counts there exactly when its own worktree would load it.
//!
//! A later version that only adds fields with a safe default bumps `FORMAT_VERSION` and keeps
//! reading the older one (`OLDEST_READABLE_VERSION`). An unknown version is refused, and the
//! next sequence overwrites it.
use std::path::{Path, PathBuf};

use git2::{Oid, Repository};
use serde::{Deserialize, Serialize};

use crate::pick::{OriginalRef, SequenceKind, SequenceState};

/// The file in a worktree's own git folder (`git_dir`): every worktree has its own sequence.
pub fn file(git_dir: &Path) -> PathBuf {
    git_dir.join("browsitory").join("sequence.json")
}

/// The file's contents for `state`.
pub fn encode(state: &SequenceState) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(&StoredSequence::from(state)).map_err(|error| error.to_string())
}

/// The state in `bytes`, checked against `repo` ([`validate`]); `Err` says why it can't be
/// used. Whether HEAD or git's own paused state still match is `continue_sequence`/
/// `abort_sequence`'s job: a stale but well-formed sequence still loads, so the user can see it
/// and abort it.
pub fn decode(bytes: &[u8], repo: &Repository) -> Result<SequenceState, String> {
    let stored = serde_json::from_slice::<StoredSequence>(bytes)
        .map_err(|error| format!("malformed: {error}"))?;
    if !(OLDEST_READABLE_VERSION..=FORMAT_VERSION).contains(&stored.version) {
        return Err(format!("unknown version {}", stored.version));
    }
    let state = SequenceState::from(stored);
    validate(&state, repo)?;
    Ok(state)
}

/// 2 added `stoppedOnConflict` and `originalRef`. A version 1 file still loads: it counts as an
/// error stop (Continue then needs HEAD unmoved) with an unknown original ref (Abort never
/// rewinds), the cautious reading of what it doesn't say.
const FORMAT_VERSION: u32 = 2;
const OLDEST_READABLE_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
enum StoredKind {
    CherryPick,
    Revert,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSequence {
    version: u32,
    kind: StoredKind,
    commits: Vec<String>,
    next: usize,
    skipped: Vec<String>,
    original_head: String,
    expected_head: String,
    #[serde(default)]
    stopped_on_conflict: bool,
    /// Full ref name, `null` for a detached HEAD; absent in version 1 (unknown).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    original_ref: Option<StoredRef>,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "name")]
enum StoredRef {
    Branch(String),
    Detached,
}

impl From<&SequenceState> for StoredSequence {
    fn from(state: &SequenceState) -> Self {
        Self {
            version: FORMAT_VERSION,
            kind: match state.kind {
                SequenceKind::CherryPick => StoredKind::CherryPick,
                SequenceKind::Revert => StoredKind::Revert,
            },
            commits: state.commits.clone(),
            next: state.next,
            skipped: state.skipped.clone(),
            original_head: state.original_head.clone(),
            expected_head: state.expected_head.clone(),
            stopped_on_conflict: state.stopped_on_conflict,
            original_ref: match &state.original_ref {
                OriginalRef::Branch(name) => Some(StoredRef::Branch(name.clone())),
                OriginalRef::Detached => Some(StoredRef::Detached),
                OriginalRef::Unknown => None,
            },
        }
    }
}

impl From<StoredSequence> for SequenceState {
    fn from(stored: StoredSequence) -> Self {
        Self {
            kind: match stored.kind {
                StoredKind::CherryPick => SequenceKind::CherryPick,
                StoredKind::Revert => SequenceKind::Revert,
            },
            commits: stored.commits,
            next: stored.next,
            skipped: stored.skipped,
            original_head: stored.original_head,
            expected_head: stored.expected_head,
            stopped_on_conflict: stored.stopped_on_conflict,
            original_ref: match stored.original_ref {
                Some(StoredRef::Branch(name)) => OriginalRef::Branch(name),
                Some(StoredRef::Detached) => OriginalRef::Detached,
                None => OriginalRef::Unknown,
            },
        }
    }
}

/// The step is in range, every skipped commit is part of the sequence, and every id is a
/// commit in `repo`.
fn validate(state: &SequenceState, repo: &Repository) -> Result<(), String> {
    if state.commits.is_empty() || state.next >= state.commits.len() {
        return Err(format!(
            "step {} of {} is out of range",
            state.next + 1,
            state.commits.len()
        ));
    }
    if let Some(stray) = state.skipped.iter().find(|id| !state.commits.contains(id)) {
        return Err(format!(
            "skipped commit {stray} is not part of the sequence"
        ));
    }
    let ids = state
        .commits
        .iter()
        .chain([&state.original_head, &state.expected_head]);
    for id in ids {
        let found = Oid::from_str(id)
            .ok()
            .is_some_and(|oid| repo.find_commit(oid).is_ok());
        if !found {
            return Err(format!("{id:?} is not a commit in this repository"));
        }
    }
    Ok(())
}
