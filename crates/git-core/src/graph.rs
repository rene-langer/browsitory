use std::collections::HashMap;

use git2::{BranchType, Oid, Repository, Sort};

use crate::repo::Result;

#[derive(Debug, Clone)]
pub struct GraphCommit {
    pub id: Oid,
    pub parent_ids: Vec<Oid>,
    pub summary: String,
    pub author_name: String,
    pub time: i64,
    pub refs: Vec<String>,
}

/// Walks the union of commits reachable from every local branch tip, newest
/// first, de-duplicated by `Oid` and annotated with which local branch(es)
/// (plus "HEAD", for whichever commit HEAD currently resolves to) point
/// directly at each commit.
pub fn graph_log(repo: &Repository, max_count: usize) -> Result<Vec<GraphCommit>> {
    let mut refs_by_oid: HashMap<Oid, Vec<String>> = HashMap::new();
    for branch in repo.branches(Some(BranchType::Local))? {
        let (branch, _) = branch?;
        let Some(oid) = branch.get().target() else {
            continue;
        };
        if let Ok(Some(name)) = branch.name() {
            refs_by_oid.entry(oid).or_default().push(name.to_string());
        }
    }
    if let Ok(head) = repo.head()
        && let Some(oid) = head.target()
    {
        refs_by_oid.entry(oid).or_default().push("HEAD".to_string());
    }

    let mut revwalk = repo.revwalk()?;
    // Same ordering gotcha as log.rs: TIME alone doesn't guarantee parents
    // sort after children when several commits share a timestamp.
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    // A brand-new repo has no refs/heads/* yet; push_glob matching nothing
    // just leaves the walk empty rather than erroring.
    revwalk.push_glob("refs/heads/*")?;

    let mut out = Vec::with_capacity(max_count.min(1024));
    for oid_result in revwalk.take(max_count) {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;
        let author = commit.author();
        out.push(GraphCommit {
            id: oid,
            parent_ids: commit.parent_ids().collect(),
            summary: commit
                .summary()
                .ok()
                .flatten()
                .unwrap_or_default()
                .to_string(),
            author_name: author.name().unwrap_or_default().to_string(),
            time: commit.time().seconds(),
            refs: refs_by_oid.remove(&oid).unwrap_or_default(),
        });
    }
    Ok(out)
}
