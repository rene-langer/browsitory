use std::path::Path;

use git2::{BlameOptions, Oid, Repository};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BlameError {
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    pub line_number: usize, // 1-indexed, matching git2's own blame line numbering
    pub content: String,
    pub commit_id: String,
    pub short_id: String,
    pub author_name: String,
    pub timestamp: i64, // Unix seconds, UTC — matches CommitInfo's existing convention
}

fn resolve_commit_id(repo: &Repository, commit_id: &str) -> Result<Oid, BlameError> {
    if commit_id == "HEAD" {
        Ok(repo.head()?.peel_to_commit()?.id())
    } else {
        Ok(repo.revparse_single(commit_id)?.peel_to_commit()?.id())
    }
}

pub fn blame_file(
    repo: &Repository,
    commit_id: &str,
    path: &str,
) -> Result<Vec<BlameLine>, BlameError> {
    let oid = resolve_commit_id(repo, commit_id)?;
    let commit = repo.find_commit(oid)?;

    let blob_content = commit
        .tree()?
        .get_path(Path::new(path))?
        .to_object(repo)?
        .peel_to_blob()?
        .content()
        .to_vec();
    let content = String::from_utf8_lossy(&blob_content).into_owned();

    let mut opts = BlameOptions::new();
    opts.newest_commit(oid);
    let blame = repo.blame_file(Path::new(path), Some(&mut opts))?;

    let mut lines = Vec::new();
    // `.lines()` drops a trailing empty "line" after a final `\n`, matching how the file
    // actually renders — a file ending in a newline has no dangling blank final line.
    for (index, line_content) in content.lines().enumerate() {
        let line_number = index + 1;
        let (commit_id, short_id, author_name, timestamp) = match blame.get_line(line_number) {
            Some(hunk) => {
                let full_id = hunk.final_commit_id().to_string();
                let short_id = full_id[..7].to_string();
                let signature = hunk.final_signature();
                let author_name = signature
                    .as_ref()
                    .and_then(|s| s.name().ok())
                    .unwrap_or_default()
                    .to_string();
                let timestamp = signature.map(|s| s.when().seconds()).unwrap_or_default();
                (full_id, short_id, author_name, timestamp)
            }
            None => (String::new(), String::new(), String::new(), 0),
        };
        lines.push(BlameLine {
            line_number,
            content: line_content.to_string(),
            commit_id,
            short_id,
            author_name,
            timestamp,
        });
    }

    Ok(lines)
}
