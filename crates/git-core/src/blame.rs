use std::path::Path;

use git2::{BlameOptions, Oid, Repository};

use crate::repo::Result;

#[derive(Debug, Clone)]
pub struct BlameLine {
    pub commit: Oid,
    pub author_name: String,
    pub author_email: String,
    pub time: i64,
    pub summary: String,
    pub content: String,
    pub final_lineno: usize,
}

/// Blames every line of `path`'s current working-tree content, attributing
/// each line to the commit that last touched it.
///
/// Line *text* is read from the working tree rather than from a historical
/// blob: `BlameHunk::final_start_line()`/`lines_in_hunk()` describe line
/// numbers in the file's current content, so pairing them with the current
/// on-disk text is the correct (and only sensible) way to reconstruct
/// per-line attribution. `track_copies_same_file` is enabled so lines moved
/// around within the file (not just edited in place) keep their original
/// attributing commit.
pub fn blame_file(repo: &Repository, path: &str) -> Result<Vec<BlameLine>> {
    let mut opts = BlameOptions::new();
    opts.track_copies_same_file(true);

    let blame = repo.blame_file(Path::new(path), Some(&mut opts))?;

    let workdir = repo.workdir().expect("blame requires a working tree");
    let content = std::fs::read_to_string(workdir.join(path)).unwrap_or_default();
    let file_lines: Vec<&str> = content.lines().collect();

    let mut out = Vec::new();
    for hunk in blame.iter() {
        let commit = repo.find_commit(hunk.final_commit_id())?;
        let author = commit.author();
        let author_name = author.name().unwrap_or_default().to_string();
        let author_email = author.email().unwrap_or_default().to_string();
        let time = commit.time().seconds();
        // Same gotcha as `Commit::summary()` in log.rs: `Result<Option<&str>, Error>`.
        let summary = hunk
            .summary()
            .ok()
            .flatten()
            .unwrap_or_default()
            .to_string();

        let start = hunk.final_start_line(); // 1-based
        for offset in 0..hunk.lines_in_hunk() {
            let lineno = start + offset;
            let content = file_lines
                .get(lineno - 1)
                .copied()
                .unwrap_or("")
                .to_string();
            out.push(BlameLine {
                commit: hunk.final_commit_id(),
                author_name: author_name.clone(),
                author_email: author_email.clone(),
                time,
                summary: summary.clone(),
                content,
                final_lineno: lineno,
            });
        }
    }
    Ok(out)
}
