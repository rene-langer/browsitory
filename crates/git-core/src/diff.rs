use git2::{Diff, DiffFormat, DiffOptions, Repository};
use similar::{ChangeTag, TextDiff};

use crate::repo::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Addition,
    Deletion,
}

#[derive(Debug, Clone)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub lines: Vec<DiffLine>,
}

/// Diffs a single path between the index and the working tree (the "unstaged
/// changes" view).
pub fn unstaged_file_diff(repo: &Repository, path: &str) -> Result<FileDiff> {
    let mut opts = DiffOptions::new();
    opts.pathspec(path);
    let diff = repo.diff_index_to_workdir(None, Some(&mut opts))?;
    diff_to_file_diff(path, &diff)
}

/// Diffs a single path between HEAD and the index (the "staged changes" view).
/// `repo.head()` fails on a brand-new repo with no commits yet (unborn HEAD);
/// in that case everything in the index is shown as newly added.
pub fn staged_file_diff(repo: &Repository, path: &str) -> Result<FileDiff> {
    let head_tree = match repo.head() {
        Ok(head) => Some(head.peel_to_tree()?),
        Err(_) => None,
    };
    let mut opts = DiffOptions::new();
    opts.pathspec(path);
    let diff = repo.diff_tree_to_index(head_tree.as_ref(), None, Some(&mut opts))?;
    diff_to_file_diff(path, &diff)
}

fn diff_to_file_diff(path: &str, diff: &Diff) -> Result<FileDiff> {
    let mut lines = Vec::new();
    diff.print(DiffFormat::Patch, |_delta, _hunk, line| {
        let kind = match line.origin() {
            '+' => LineKind::Addition,
            '-' => LineKind::Deletion,
            ' ' => LineKind::Context,
            _ => return true, // file/hunk header pseudo-lines, not real content
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
    })?;
    Ok(FileDiff {
        path: path.to_string(),
        lines,
    })
}

/// Word-level diff between two lines, for highlighting an intra-line edit on
/// a paired removal/addition (GitHub/GitLab-style), rather than the noisier
/// character-level or whole-line-level highlighting.
pub fn word_diff(old: &str, new: &str) -> Vec<(ChangeTag, String)> {
    TextDiff::from_words(old, new)
        .iter_all_changes()
        .map(|change| (change.tag(), change.to_string()))
        .collect()
}
