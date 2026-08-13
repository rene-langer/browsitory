use std::collections::HashMap;

use git2::{BranchType, Oid, Repository, Sort};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GraphError {
    #[error("failed to read commit graph: {0}")]
    Read(#[from] git2::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphCommit {
    pub id: String,
    pub short_id: String,
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64,
    pub parent_ids: Vec<String>,
    pub branch_refs: Vec<String>,
}

pub fn graph_log(repo: &Repository, limit: usize) -> Result<Vec<GraphCommit>, GraphError> {
    let mut tips_by_oid: HashMap<Oid, Vec<String>> = HashMap::new();
    for entry in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = entry?;
        let Ok(Some(name)) = branch.name() else {
            continue;
        };
        if let Some(oid) = branch.get().target() {
            tips_by_oid.entry(oid).or_default().push(name.to_string());
        }
    }

    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    // On a repo with no commits yet, there are no local branches to match, so this simply
    // pushes nothing — the loop below then runs zero times, giving an empty graph. Unlike
    // `push_head()` (which errors on an unborn HEAD, the case the removed `log()` had to
    // special-case), `push_glob` doesn't error on zero matches — no special-casing needed here.
    revwalk.push_glob("refs/heads/*")?;

    let mut commits = Vec::new();
    for oid_result in revwalk.take(limit) {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;

        let id = oid.to_string();
        let short_id = id[..7].to_string();
        let summary = commit
            .summary()
            .ok()
            .flatten()
            .unwrap_or_default()
            .to_string();
        let author_name = commit.author().name().ok().unwrap_or_default().to_string();
        let author_email = commit.author().email().ok().unwrap_or_default().to_string();
        let timestamp = commit.time().seconds();
        let parent_ids = commit.parent_ids().map(|p| p.to_string()).collect();
        let branch_refs = tips_by_oid.get(&oid).cloned().unwrap_or_default();

        commits.push(GraphCommit {
            id,
            short_id,
            summary,
            author_name,
            author_email,
            timestamp,
            parent_ids,
            branch_refs,
        });
    }

    Ok(commits)
}
