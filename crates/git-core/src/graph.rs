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

/// The branches the graph walks, and a commit search without a `branch:` term (#385).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphSelection {
    /// Local branch names: `None` (nothing saved) walks every local branch, `Some(vec![])`
    /// none of them.
    pub local: Option<Vec<String>>,
    /// Remote-tracking branches as `<remote>/<branch>`, walked on top of `local`. Empty (also
    /// what a never-saved remote selection maps to) walks no remote branch, so remote-only
    /// commits stay out of the default graph.
    pub remote: Vec<String>,
}

impl GraphSelection {
    /// True when the walk follows first parents only: exactly one branch is selected, local
    /// and remote together (what "Isolate branch" produces). That branch's line is then its
    /// merge commits plus the commits made directly on it; the commits of merged side branches
    /// are left out. `local: None` (every local branch) or several branches walk every
    /// ancestor (#380).
    pub(crate) fn walks_first_parent(&self) -> bool {
        matches!(&self.local, Some(local) if local.len() + self.remote.len() == 1)
    }

    /// The commits the walk starts from, sorted and deduplicated: the selected branches plus a
    /// detached HEAD (#467). A persisted selection can
    /// outlive its branches (deleted or renamed, possibly outside the app, or a remote branch
    /// pruned by a fetch): missing names are skipped instead of failing the walk (#97).
    pub(crate) fn start_oids(&self, repo: &Repository) -> Result<Vec<Oid>, git2::Error> {
        let named = |refname: &str| -> Result<Option<Oid>, git2::Error> {
            match repo.find_reference(refname) {
                Ok(reference) => Ok(reference.peel_to_commit().ok().map(|c| c.id())),
                // A name that isn't a valid ref name can't resolve either.
                Err(error)
                    if matches!(
                        error.code(),
                        git2::ErrorCode::NotFound | git2::ErrorCode::InvalidSpec
                    ) =>
                {
                    Ok(None)
                }
                Err(error) => Err(error),
            }
        };
        let mut oids = Vec::new();
        match &self.local {
            Some(names) => {
                for name in names {
                    oids.extend(named(&format!("refs/heads/{name}"))?);
                }
            }
            // A repo with no commits yet has no local branches: the walk is simply empty, no
            // special case for an unborn HEAD.
            None => {
                for reference in repo.references_glob("refs/heads/*")? {
                    if let Ok(commit) = reference?.peel_to_commit() {
                        oids.push(commit.id());
                    }
                }
            }
        }
        for name in &self.remote {
            oids.extend(named(&format!("refs/remotes/{name}"))?);
        }
        // A detached HEAD is what the user is on, whatever the selection: its commits may be on
        // no branch at all (a commit made after checking out a tag or a commit, a submodule's
        // checkout, a rebase in progress), and would otherwise be missing (#467). An attached
        // HEAD is a branch, walked when that branch is selected.
        if repo.head_detached().unwrap_or(false) {
            if let Ok(commit) = repo.head().and_then(|head| head.peel_to_commit()) {
                oids.push(commit.id());
            }
        }
        oids.sort();
        oids.dedup();
        Ok(oids)
    }
}

/// The commits of `selection`, children before parents (`TOPOLOGICAL | TIME`), at most
/// `limit`. With exactly one selected branch the walk follows first parents only (see
/// [`GraphSelection::walks_first_parent`]); a merge commit still lists all its `parent_ids`,
/// so its other parents are usually not in the result.
pub fn graph_log(
    repo: &Repository,
    limit: usize,
    selection: &GraphSelection,
) -> Result<Vec<GraphCommit>, GraphError> {
    let decorations = RefDecorations::load(repo)?;

    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    if selection.walks_first_parent() {
        revwalk.simplify_first_parent()?;
    }
    for oid in selection.start_oids(repo)? {
        revwalk.push(oid)?;
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
