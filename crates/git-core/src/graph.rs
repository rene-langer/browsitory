use std::collections::HashMap;

use git2::{BranchType, Oid, ReferenceType, Repository, Sort};
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
    pub remote_branch_refs: Vec<String>,
    /// Tag names (without "refs/tags/") whose target peels to this commit, sorted.
    pub tags: Vec<String>,
}

/// Local/remote branch names and tag names keyed by the commit they point at — the badges on
/// graph rows (and file history / search results).
pub(crate) struct RefDecorations {
    local: HashMap<Oid, Vec<String>>,
    remote: HashMap<Oid, Vec<String>>,
    tags: HashMap<Oid, Vec<String>>,
}

impl RefDecorations {
    pub(crate) fn load(repo: &Repository) -> Result<Self, git2::Error> {
        let mut local: HashMap<Oid, Vec<String>> = HashMap::new();
        for entry in repo.branches(Some(BranchType::Local))? {
            let (branch, _) = entry?;
            let Ok(Some(name)) = branch.name() else {
                continue;
            };
            if let Some(oid) = branch.get().target() {
                local.entry(oid).or_default().push(name.to_string());
            }
        }

        let mut remote: HashMap<Oid, Vec<String>> = HashMap::new();
        for entry in repo.branches(Some(BranchType::Remote))? {
            let (branch, _) = entry?;
            // Symbolic refs like `origin/HEAD` point at another ref rather than a commit
            // directly and would otherwise duplicate whatever branch they alias — skip them so
            // they don't produce a fake "origin/HEAD" badge on every default-branch commit.
            if branch.get().kind() != Some(ReferenceType::Direct) {
                continue;
            }
            let Ok(Some(name)) = branch.name() else {
                continue;
            };
            if let Some(oid) = branch.get().target() {
                remote.entry(oid).or_default().push(name.to_string());
            }
        }
        let mut tags: HashMap<Oid, Vec<String>> = HashMap::new();
        for reference in repo.references_glob("refs/tags/*")? {
            let reference = reference?;
            let Ok(full_name) = reference.name() else {
                continue;
            };
            let Some(name) = full_name.strip_prefix("refs/tags/") else {
                continue;
            };
            // Lightweight and (nested) annotated tags alike; a tag on a tree or blob has no row.
            let Ok(commit) = reference.peel_to_commit() else {
                continue;
            };
            tags.entry(commit.id()).or_default().push(name.to_string());
        }
        for names in tags.values_mut() {
            names.sort();
        }
        Ok(Self {
            local,
            remote,
            tags,
        })
    }

    pub(crate) fn graph_commit(&self, commit: &git2::Commit) -> GraphCommit {
        let oid = commit.id();
        let id = oid.to_string();
        GraphCommit {
            short_id: id[..7].to_string(),
            summary: commit
                .summary()
                .ok()
                .flatten()
                .unwrap_or_default()
                .to_string(),
            author_name: commit.author().name().ok().unwrap_or_default().to_string(),
            author_email: commit.author().email().ok().unwrap_or_default().to_string(),
            timestamp: commit.time().seconds(),
            parent_ids: commit.parent_ids().map(|p| p.to_string()).collect(),
            branch_refs: self.local.get(&oid).cloned().unwrap_or_default(),
            remote_branch_refs: self.remote.get(&oid).cloned().unwrap_or_default(),
            tags: self.tags.get(&oid).cloned().unwrap_or_default(),
            id,
        }
    }
}

pub fn graph_log(
    repo: &Repository,
    limit: usize,
    selected_branches: Option<&[String]>,
) -> Result<Vec<GraphCommit>, GraphError> {
    let decorations = RefDecorations::load(repo)?;

    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    match selected_branches {
        // On a repo with no commits yet, there are no local branches to match, so this simply
        // pushes nothing — the loop below then runs zero times, giving an empty graph. Unlike
        // `push_head()` (which errors on an unborn HEAD, the case the removed `log()` had to
        // special-case), `push_glob` doesn't error on zero matches — no special-casing needed
        // here.
        None => revwalk.push_glob("refs/heads/*")?,
        Some(names) => {
            // A persisted selection can outlive its branches (deleted or renamed, possibly
            // outside the app). Skip the missing ones, like `search_commits`, instead of
            // failing the whole graph (#97).
            for name in names {
                let refname = format!("refs/heads/{name}");
                match repo.find_reference(&refname) {
                    Ok(_) => revwalk.push_ref(&refname)?,
                    Err(error) if error.code() == git2::ErrorCode::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
    }

    let mut commits = Vec::new();
    for oid_result in revwalk.take(limit) {
        let commit = repo.find_commit(oid_result?)?;
        commits.push(decorations.graph_commit(&commit));
    }
    Ok(commits)
}

/// Full raw message (subject plus body) of `commit_id`, for the commit header.
pub fn commit_message(repo: &Repository, commit_id: &str) -> Result<String, GraphError> {
    let commit = repo.find_commit(Oid::from_str(commit_id)?)?;
    Ok(String::from_utf8_lossy(commit.message_bytes()).into_owned())
}
