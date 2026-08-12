use git2::{Repository, Sort};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum LogError {
    #[error("failed to read commit log: {0}")]
    Read(#[from] git2::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub id: String,       // full 40-char hex OID
    pub short_id: String, // first 7 hex chars of `id` — fixed length, not libgit2's
    // variable unique-prefix short_id() (simpler, deterministic)
    pub summary: String,
    pub author_name: String,
    pub author_email: String,
    pub timestamp: i64, // Unix seconds, UTC — frontend formats for display
}

pub fn log(repo: &Repository, limit: usize) -> Result<Vec<CommitInfo>, LogError> {
    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;

    // On a repo with no commits yet, push_head() returns Err (unborn branch) —
    // treat that as "empty log", not a propagated error.
    if revwalk.push_head().is_err() {
        return Ok(Vec::new());
    }

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

        commits.push(CommitInfo {
            id,
            short_id,
            summary,
            author_name,
            author_email,
            timestamp,
        });
    }

    Ok(commits)
}
