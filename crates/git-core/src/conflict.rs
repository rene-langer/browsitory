use git2::{DiffOptions, Repository};

use crate::diff::{DiffLine, FileDiff, LineKind};
use crate::repo::Result;

/// The three possible sides of a merge conflict for one path. Any side can be
/// missing: "added by us"/"added by them" conflicts have no `ancestor`,
/// "deleted by them"/"deleted by us" conflicts have a missing `theirs`/`ours`.
#[derive(Debug, Clone)]
pub struct ConflictSides {
    pub path: String,
    pub ancestor: Option<Vec<u8>>,
    pub ours: Option<Vec<u8>>,
    pub theirs: Option<Vec<u8>>,
}

/// Lists the paths that currently have unresolved merge conflicts in the
/// index (stage > 0 entries), in index order.
pub fn conflicted_paths(repo: &Repository) -> Result<Vec<String>> {
    let index = repo.index()?;
    let mut out = Vec::new();
    for conflict in index.conflicts()? {
        let conflict = conflict?;
        let path = conflict
            .ancestor
            .as_ref()
            .or(conflict.our.as_ref())
            .or(conflict.their.as_ref())
            .map(|entry| String::from_utf8_lossy(&entry.path).into_owned());
        if let Some(path) = path {
            out.push(path);
        }
    }
    Ok(out)
}

/// Reads the ancestor/ours/theirs blob contents for one conflicted path.
/// Missing sides (e.g. "added by us", where there is no ancestor or their
/// version) come back as `None` rather than an error.
pub fn read_conflict(repo: &Repository, path: &str) -> Result<ConflictSides> {
    let index = repo.index()?;
    for conflict in index.conflicts()? {
        let conflict = conflict?;
        let entry_path = conflict
            .ancestor
            .as_ref()
            .or(conflict.our.as_ref())
            .or(conflict.their.as_ref())
            .map(|entry| String::from_utf8_lossy(&entry.path).into_owned());
        if entry_path.as_deref() != Some(path) {
            continue;
        }

        return Ok(ConflictSides {
            path: path.to_string(),
            ancestor: read_blob(repo, conflict.ancestor.as_ref())?,
            ours: read_blob(repo, conflict.our.as_ref())?,
            theirs: read_blob(repo, conflict.their.as_ref())?,
        });
    }

    Ok(ConflictSides {
        path: path.to_string(),
        ancestor: None,
        ours: None,
        theirs: None,
    })
}

fn read_blob(repo: &Repository, entry: Option<&git2::IndexEntry>) -> Result<Option<Vec<u8>>> {
    match entry {
        Some(entry) => {
            let blob = repo.find_blob(entry.id)?;
            Ok(Some(blob.content().to_vec()))
        }
        None => Ok(None),
    }
}

/// Diffs two optional blob contents for one side of a conflict (e.g. ancestor
/// vs. ours, or ancestor vs. theirs), for rendering a side-by-side conflict
/// view. Missing content on either side is treated as an empty file, same as
/// `diff_blobs` does for `None`.
///
/// This mirrors `diff.rs`'s `diff_to_file_diff`/`LineKind` mapping, but is
/// fed from `Repository::diff_blobs`'s four direct callback slots rather than
/// from a `Diff` object (there is no `Diff` in between for blob-to-blob
/// diffing, unlike the tree/index/workdir diffs in `diff.rs`).
pub fn diff_blob_sides(
    repo: &Repository,
    path: &str,
    old: Option<&[u8]>,
    new: Option<&[u8]>,
) -> Result<FileDiff> {
    let mut opts = DiffOptions::new();
    let old_blob = blob_from(repo, old)?;
    let new_blob = blob_from(repo, new)?;
    let mut lines = Vec::new();

    repo.diff_blobs(
        old_blob.as_ref(),
        Some(path),
        new_blob.as_ref(),
        Some(path),
        Some(&mut opts),
        None,
        None,
        None,
        Some(&mut |_delta, _hunk, line| {
            let kind = match line.origin() {
                '+' => LineKind::Addition,
                '-' => LineKind::Deletion,
                ' ' => LineKind::Context,
                _ => return true,
            };
            let content = String::from_utf8_lossy(line.content())
                .trim_end_matches('\n')
                .to_string();
            lines.push(DiffLine {
                kind,
                old_lineno: line.old_lineno(),
                new_lineno: line.new_lineno(),
                content,
            });
            true
        }),
    )?;

    Ok(FileDiff {
        path: path.to_string(),
        lines,
    })
}

/// `diff_blobs` needs an actual `Blob` object (not just bytes) for each side,
/// so a conflict side's in-index content is written into the object database
/// as a loose blob (cheap, content-addressed, and already how git itself
/// stores conflict stage entries) before diffing.
fn blob_from<'repo>(
    repo: &'repo Repository,
    content: Option<&[u8]>,
) -> Result<Option<git2::Blob<'repo>>> {
    match content {
        Some(bytes) => {
            let oid = repo.blob(bytes)?;
            Ok(Some(repo.find_blob(oid)?))
        }
        None => Ok(None),
    }
}
