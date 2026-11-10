//! Lines added and removed by a commit (#620): the badge in the commit list and the totals in
//! the commit details.
//!
//! A commit is counted against its first parent (a root commit against the empty tree), with
//! the same diff as `diff::commit_files`, so the numbers add up to what the details' file
//! sections show: no rename detection (a rename is a removed and an added file, as in the file
//! list) and no similarity pass, whose cost grows with the square of the files a commit touches.
//! A binary file counts as a changed file with no lines.

use std::time::{Duration, Instant};

use git2::Repository;

use crate::diff::{commit_and_parent_trees, DiffError};

/// One changed file's line counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStats {
    pub path: String,
    pub insertions: usize,
    pub deletions: usize,
    /// No line counts: libgit2 found binary content on either side.
    pub binary: bool,
}

/// A commit's line counts. `files` is filled only when asked for (the details pane); the commit
/// list needs the totals alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitStats {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
    pub files: Vec<FileStats>,
}

/// How much one request may count, so it can't hold the repository's worker for long.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsBudget {
    /// No further commit is started once a request has run this long; the commits it didn't
    /// reach are left out of the answer, and the caller asks again.
    pub request: Duration,
    /// A commit still being counted after this long is given up (`None`), as is one that
    /// changes more than `max_files` files.
    pub per_commit: Duration,
    pub max_files: usize,
}

impl Default for StatsBudget {
    fn default() -> Self {
        Self {
            request: Duration::from_millis(150),
            per_commit: Duration::from_millis(500),
            max_files: 5000,
        }
    }
}

/// `commit_id`'s line counts, or `None` when it changes more than `max_files` files or is still
/// being counted at `deadline`: a commit over the cap gets no numbers rather than partial ones.
pub fn commit_stats(
    repo: &Repository,
    commit_id: &str,
    with_files: bool,
    max_files: usize,
    deadline: Option<Instant>,
) -> Result<Option<CommitStats>, DiffError> {
    let (old_tree, new_tree) = commit_and_parent_trees(repo, commit_id)?;
    let mut opts = git2::DiffOptions::new();
    // Context lines are never shown here; without them libgit2 builds smaller hunks.
    opts.context_lines(0);
    let diff = repo.diff_tree_to_tree(old_tree.as_ref(), Some(&new_tree), Some(&mut opts))?;
    let deltas = diff.deltas().len();
    if deltas > max_files {
        return Ok(None);
    }
    let mut stats = CommitStats {
        files_changed: deltas,
        insertions: 0,
        deletions: 0,
        files: Vec::new(),
    };
    for index in 0..deltas {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Ok(None);
        }
        let patch = git2::Patch::from_diff(&diff, index)?;
        let (insertions, deletions) = match &patch {
            Some(patch) => {
                let (_, insertions, deletions) = patch.line_stats()?;
                (insertions, deletions)
            }
            None => (0, 0),
        };
        stats.insertions += insertions;
        stats.deletions += deletions;
        if with_files {
            // Loading the patch set the delta's binary flag.
            let delta = diff.get_delta(index).expect("delta in range");
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default();
            stats.files.push(FileStats {
                path,
                insertions,
                deletions,
                binary: delta.flags().is_binary(),
            });
        }
    }
    Ok(Some(stats))
}

/// The line counts of `commit_ids`, in order, within `budget`. Each answered commit comes with
/// its stats, or `None` when it was over the cap or couldn't be read (a commit that is gone);
/// the commits after the request budget ran out are left out. The first commit is always
/// answered, so asking again always makes progress.
pub fn commit_stats_batch(
    repo: &Repository,
    commit_ids: &[String],
    with_files: bool,
    budget: StatsBudget,
) -> Vec<(String, Option<CommitStats>)> {
    let start = Instant::now();
    let mut answered = Vec::new();
    for commit_id in commit_ids {
        if !answered.is_empty() && start.elapsed() >= budget.request {
            break;
        }
        let deadline = Instant::now() + budget.per_commit;
        let stats = commit_stats(repo, commit_id, with_files, budget.max_files, Some(deadline))
            .ok()
            .flatten();
        answered.push((commit_id.clone(), stats));
    }
    answered
}
