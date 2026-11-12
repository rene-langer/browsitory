//! Blame (#839): the commit that last changed each line of a file, the way `git blame` assigns
//! them. libgit2 has a blame (`git_blame_file`), but it diffs both commits' whole trees at every
//! commit of history, about a minute on a 130k-commit repository where git needs a second
//! (libgit2/libgit2#7472 is the upstream fix, unreleased). This one walks history with file
//! history's top-down path comparison ([`parent_path_entries`]), which reads about one root tree
//! per commit and nothing below a folder whose id didn't change, and diffs blobs only where the
//! file changed:
//!
//! - Every line still to be explained sits with a "suspect": a commit and the file's path and
//!   line number there. It starts with the blamed commit.
//! - Suspects are taken in topological order (children before parents). If a parent has the
//!   identical file, all the suspect's lines pass to the first such parent. Otherwise, for each
//!   parent in order that has the file (at the same path, or under the name libgit2's rename
//!   detection finds), the lines a zero-context diff of the two versions leaves unchanged pass to
//!   that parent; a parent with the same version as an earlier one is skipped. The lines left
//!   belong to the suspect's commit, and so do all of a root commit's.
//!
//! As in libgit2 and git (without `-M`/`-C`), lines moved or copied within or between files
//! aren't followed.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::path::Path;

use git2::{DiffOptions, Oid, Patch, Repository, Sort};
use thiserror::Error;

use crate::history::{parent_path_entries, path_entries, rename_source, PathEntries};
use crate::line_history::line_count;

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
    pub timestamp: i64, // Unix seconds, UTC — matches GraphCommit's existing convention
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
    let owners = line_owners(repo, oid, path)?;

    let mut commits: HashMap<Oid, (String, String, i64)> = HashMap::new();
    let mut lines = Vec::new();
    // `.lines()` drops a trailing empty "line" after a final `\n`, matching how the file
    // actually renders — a file ending in a newline has no dangling blank final line.
    for (index, line_content) in content.lines().enumerate() {
        let line_number = index + 1;
        // Both count lines the same way, so every line has an owner.
        let owner = owners[index];
        if let Entry::Vacant(slot) = commits.entry(owner) {
            let commit = repo.find_commit(owner)?;
            let author = commit.author();
            slot.insert((
                owner.to_string(),
                author.name().unwrap_or_default().to_string(),
                author.when().seconds(),
            ));
        }
        let (full_id, author_name, timestamp) = &commits[&owner];
        lines.push(BlameLine {
            line_number,
            content: line_content.to_string(),
            commit_id: full_id.clone(),
            short_id: full_id[..7].to_string(),
            author_name: author_name.clone(),
            timestamp: *timestamp,
        });
    }

    Ok(lines)
}

/// Lines of a suspect: each line's number in the suspect's version (1-based) and its index in
/// the blamed file.
type Lines = Vec<(usize, usize)>;

/// Lines of the file still to be explained at one commit: the path there, its tree entries, and
/// for each line its number in this commit's version (1-based) and its index in the blamed file.
struct Suspect {
    path: String,
    entries: PathEntries,
    lines: Vec<(usize, usize)>,
}

/// The commit that last changed each line of `path` as it is in `start` (one per line, in
/// order). See the module docs.
fn line_owners(repo: &Repository, start: Oid, path: &str) -> Result<Vec<Oid>, git2::Error> {
    let Some(entries) = path_entries(repo, repo.find_commit(start)?.tree_id(), path)? else {
        return Ok(Vec::new());
    };
    let count = line_count(repo.find_blob(entries.blob().0)?.content());
    let mut owners = vec![start; count];
    if count == 0 {
        return Ok(owners);
    }

    let mut walk = repo.revwalk()?;
    // Topological: every child passes its lines on before the walk reaches the parent.
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    walk.push(start)?;
    let mut suspects: HashMap<Oid, Vec<Suspect>> = HashMap::from([(
        start,
        vec![Suspect {
            path: path.to_string(),
            entries,
            lines: (1..=count).map(|n| (n, n - 1)).collect(),
        }],
    )]);
    for oid in walk {
        if suspects.is_empty() {
            break;
        }
        let oid = oid?;
        let Some(at_commit) = suspects.remove(&oid) else {
            continue;
        };
        let commit = repo.find_commit(oid)?;
        for suspect in at_commit {
            let left = pass_to_parents(repo, &commit, suspect, &mut suspects)?;
            for (_, index) in left {
                owners[index] = oid;
            }
        }
    }
    Ok(owners)
}

/// Passes `suspect`'s lines that `commit`'s parents explain on to them; returns the rest.
fn pass_to_parents(
    repo: &Repository,
    commit: &git2::Commit,
    suspect: Suspect,
    suspects: &mut HashMap<Oid, Vec<Suspect>>,
) -> Result<Vec<(usize, usize)>, git2::Error> {
    let blob_id = suspect.entries.blob().0;
    let mut origins: Vec<(Oid, String, PathEntries)> = Vec::new();
    for parent in commit.parents() {
        let found =
            match parent_path_entries(repo, parent.tree_id(), &suspect.entries, &suspect.path)? {
                Some(entries) => Some((suspect.path.clone(), entries)),
                None => {
                    let parent_tree = parent.tree()?;
                    match rename_source(repo, &parent_tree, &commit.tree()?, &suspect.path)? {
                        Some(source) => path_entries(repo, parent_tree.id(), &source)?
                            .map(|entries| (source, entries)),
                        None => None,
                    }
                }
            };
        let Some((parent_path, parent_entries)) = found else {
            continue;
        };
        if parent_entries.blob().0 == blob_id {
            // The parent has this very version: every line comes from there.
            add_suspect(
                suspects,
                parent.id(),
                parent_path,
                parent_entries,
                suspect.lines,
            );
            return Ok(Vec::new());
        }
        if origins
            .iter()
            .all(|(_, _, seen)| seen.blob().0 != parent_entries.blob().0)
        {
            origins.push((parent.id(), parent_path, parent_entries));
        }
    }

    let mut left = suspect.lines;
    if origins.is_empty() {
        return Ok(left);
    }
    let new = repo.find_blob(blob_id)?;
    for (parent, parent_path, parent_entries) in origins {
        if left.is_empty() {
            break;
        }
        let old = repo.find_blob(parent_entries.blob().0)?;
        let hunks = changed_hunks(old.content(), new.content())?;
        let (passed, kept) = split_unchanged(&hunks, left);
        left = kept;
        if !passed.is_empty() {
            add_suspect(suspects, parent, parent_path, parent_entries, passed);
        }
    }
    Ok(left)
}

fn add_suspect(
    suspects: &mut HashMap<Oid, Vec<Suspect>>,
    commit: Oid,
    path: String,
    entries: PathEntries,
    lines: Vec<(usize, usize)>,
) {
    let at_commit = suspects.entry(commit).or_default();
    match at_commit.iter_mut().find(|s| s.path == path) {
        Some(existing) => existing.lines.extend(lines),
        None => at_commit.push(Suspect {
            path,
            entries,
            lines,
        }),
    }
}

/// One hunk of a zero-context diff, in libgit2's numbering (see `line_history`'s `Hunk`).
#[derive(Debug, Clone, Copy)]
struct Hunk {
    old_lines: usize,
    new_start: usize,
    new_lines: usize,
}

/// The hunks of a zero-context diff from `old` to `new`, both read as text, after trimming
/// their common tail as libgit2's and git's blame do (it decides where an ambiguous change,
/// such as one of two equal lines, lands).
fn changed_hunks(old: &[u8], new: &[u8]) -> Result<Vec<Hunk>, git2::Error> {
    let (old, new) = trim_common_tail(old, new);
    let mut opts = DiffOptions::new();
    opts.context_lines(0).interhunk_lines(0).force_text(true);
    let patch = Patch::from_buffers(old, None, new, None, Some(&mut opts))?;
    (0..patch.num_hunks())
        .map(|index| {
            let (hunk, _) = patch.hunk(index)?;
            Ok(Hunk {
                old_lines: hunk.old_lines() as usize,
                new_start: hunk.new_start() as usize,
                new_lines: hunk.new_lines() as usize,
            })
        })
        .collect()
}

/// Drops the 1 KiB blocks both versions end with, then gives back the part of the last block
/// up to its first newline (xdiff's `trim_common_tail`, which blame applies before diffing).
fn trim_common_tail<'a>(old: &'a [u8], new: &'a [u8]) -> (&'a [u8], &'a [u8]) {
    (old, new)
}

/// Splits `lines` (line numbers in the new version, with their blamed-file index) into those
/// the diff leaves unchanged, renumbered to the old version, and those it changed.
fn split_unchanged(hunks: &[Hunk], mut lines: Lines) -> (Lines, Lines) {
    lines.sort_unstable();
    let (mut passed, mut kept) = (Vec::new(), Vec::new());
    // Lines added minus lines removed by the hunks wholly before the current line.
    let mut shift: isize = 0;
    let mut next = 0;
    for (line, index) in lines {
        while let Some(hunk) = hunks.get(next) {
            // A pure deletion (no new lines) sits after new line `new_start`.
            let before = if hunk.new_lines == 0 {
                hunk.new_start < line
            } else {
                hunk.new_start + hunk.new_lines - 1 < line
            };
            if !before {
                break;
            }
            shift += hunk.new_lines as isize - hunk.old_lines as isize;
            next += 1;
        }
        let changed = hunks.get(next).is_some_and(|hunk| {
            hunk.new_lines > 0 && hunk.new_start <= line && line < hunk.new_start + hunk.new_lines
        });
        if changed {
            kept.push((line, index));
        } else {
            passed.push(((line as isize - shift) as usize, index));
        }
    }
    (passed, kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_common_tail_under_a_block_is_kept() {
        assert_eq!(
            trim_common_tail(b"a\nsame\n", b"b\nsame\n"),
            (&b"a\nsame\n"[..], &b"b\nsame\n"[..])
        );
    }

    #[test]
    fn whole_common_blocks_are_trimmed_back_to_the_first_newline_in_them() {
        // 410 lines of 5 bytes: two 1 KiB blocks of it are common to both versions.
        let tail = "line\n".repeat(410);
        let old = format!("A\n{tail}");
        let new = format!("B\nB\n{tail}");

        let (old, new) = trim_common_tail(old.as_bytes(), new.as_bytes());

        // The trimmed part starts 2 bytes into the tail, at "ne\n": its first newline is kept.
        assert_eq!(old, b"A\nline\n");
        assert_eq!(new, b"B\nB\nline\n");
    }

    #[test]
    fn a_tail_that_differs_in_its_last_block_is_kept() {
        let tail = "line\n".repeat(410);
        let old = format!("A\n{tail}x");
        let new = format!("B\n{tail}y");

        let (trimmed_old, trimmed_new) = trim_common_tail(old.as_bytes(), new.as_bytes());

        assert_eq!(trimmed_old, old.as_bytes());
        assert_eq!(trimmed_new, new.as_bytes());
    }

    #[test]
    fn a_common_block_without_a_newline_is_given_back_whole() {
        // xdiff gives the trimmed blocks back up to their first newline; with none, all of it.
        let tail = "x".repeat(1024);
        let old = format!("A\n{tail}");
        let new = format!("B\n{tail}");

        let (trimmed_old, trimmed_new) = trim_common_tail(old.as_bytes(), new.as_bytes());

        assert_eq!(trimmed_old, old.as_bytes());
        assert_eq!(trimmed_new, new.as_bytes());
    }
}
