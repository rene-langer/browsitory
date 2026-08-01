use git2::{Oid, Repository, Sort};

use crate::repo::Result;

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub id: Oid,
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    /// Seconds since the Unix epoch, author time (matches `git log`'s default).
    pub time: i64,
    pub parent_ids: Vec<Oid>,
}

/// Walks commit history starting from `start` (or HEAD), newest first.
///
/// `skip`/`limit` page through history without materializing the whole log up
/// front — `Revwalk` itself is a lazy iterator, so this stays cheap even on
/// large repos as long as callers request pages instead of everything at once.
pub fn commit_log(
    repo: &Repository,
    start: Option<Oid>,
    skip: usize,
    limit: usize,
) -> Result<Vec<CommitInfo>> {
    let mut revwalk = repo.revwalk()?;
    // TOPOLOGICAL ensures parents always sort after their children even when
    // several commits share a timestamp (as they do in fast test setups, and
    // can in real history from rebases/imports) — TIME alone doesn't
    // guarantee that ordering on ties, matching plain `git log`'s default.
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    match start {
        Some(oid) => revwalk.push(oid)?,
        None => revwalk.push_head()?,
    }

    let mut out = Vec::with_capacity(limit.min(1024));
    for oid_result in revwalk.skip(skip).take(limit) {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;
        let author = commit.author();
        out.push(CommitInfo {
            id: oid,
            summary: commit
                .summary()
                .ok()
                .flatten()
                .unwrap_or_default()
                .to_string(),
            author_name: author.name().unwrap_or_default().to_string(),
            author_email: author.email().unwrap_or_default().to_string(),
            time: commit.time().seconds(),
            parent_ids: commit.parent_ids().collect(),
        });
    }
    Ok(out)
}
