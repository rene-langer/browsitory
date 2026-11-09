//! Line-range history (#447): the commits that changed a range of lines of one file, following
//! the range back through earlier changes the way `git log -L <from>,<to>:<path>` does.
//!
//! libgit2 has no line log, so the range is carried from each commit to its parents through the
//! hunks of a zero-context blob diff (see [`map_to_parent`]). Limits, all documented in the user
//! guide:
//! - The range stays one contiguous block. A change inside it widens the parent's range to the
//!   old side of that change, as `git log -L` does; `git log -L` can also split a range in two
//!   where this keeps the block between.
//! - Renames are followed with libgit2's rename detection (as file history does); copies are not.
//! - A merge whose range matches one parent's is skipped and only that parent followed (the
//!   default history simplification of `git log -L`). A merge that changed the range against
//!   every parent is listed, and every parent followed.
//! - libgit2's diff can place a change differently from Git's in ambiguous cases (a repeated
//!   line), so an edge of the range can drift by a line from what `git log -L` shows.
//! - The walk stops after [`MAX_LINE_HISTORY_COMMITS`] commits on the range's path, and says so.

use std::collections::{HashMap, HashSet};

use git2::{Blob, DiffOptions, ErrorCode, Oid, Repository, Sort};

use crate::graph::{GraphCommit, RefDecorations};
use crate::history::{blob_at, rename_source, validate_path, HistoryError};

/// Commits on the range's path a walk examines before it stops (`LineHistory::truncated`). Each
/// costs a few tree lookups, and a blob diff when the file changed.
pub const MAX_LINE_HISTORY_COMMITS: usize = 50_000;

/// Commits walked between two polls of a walk's `cancel`.
const CANCEL_INTERVAL: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    /// First line, 1-based.
    pub start: usize,
    /// Last line, 1-based and inclusive.
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineHistoryEntry {
    pub commit: GraphCommit,
    /// Path of the file in this commit.
    pub path: String,
    /// Set when this commit renamed `old_path` to `path` (relative to its first parent).
    pub old_path: Option<String>,
    /// The tracked lines in this commit's version of the file.
    pub lines: LineRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineHistory {
    pub entries: Vec<LineHistoryEntry>,
    /// The walk stopped at [`MAX_LINE_HISTORY_COMMITS`] before the range's history ended, so
    /// older changes may be missing.
    pub truncated: bool,
}

/// Commits that changed lines `lines` of `path` as it is in `revision` (a commit id or "HEAD"),
/// newest first, at most `limit` of them. See the module docs for how the range is followed.
pub fn line_history(
    repo: &Repository,
    revision: &str,
    path: &str,
    lines: LineRange,
    limit: usize,
) -> Result<LineHistory, HistoryError> {
    let start = resolve(repo, revision)?;
    let Some(start) = start.filter(|_| limit > 0) else {
        return Ok(LineHistory {
            entries: Vec::new(),
            truncated: false,
        });
    };
    let decorations = RefDecorations::load(repo)?;
    let mut entries = Vec::new();
    let truncated = walk(
        repo,
        start,
        path,
        lines,
        MAX_LINE_HISTORY_COMMITS,
        &|| false,
        &mut |commit, path, old_path, lines| {
            entries.push(LineHistoryEntry {
                commit: decorations.graph_commit(commit),
                path: path.to_string(),
                old_path,
                lines,
            });
            entries.len() < limit
        },
    )?;
    Ok(LineHistory { entries, truncated })
}

/// The ids of every commit [`line_history`] lists for the range, from `start`, for search's
/// `line:` key. `None` when `cancel` stopped the walk.
pub(crate) fn line_history_ids(
    repo: &Repository,
    start: Oid,
    path: &str,
    lines: LineRange,
    cancel: &dyn Fn() -> bool,
) -> Result<Option<HashSet<Oid>>, HistoryError> {
    let mut ids = HashSet::new();
    walk(
        repo,
        start,
        path,
        lines,
        MAX_LINE_HISTORY_COMMITS,
        cancel,
        &mut |commit, _, _, _| {
            ids.insert(commit.id());
            true
        },
    )
    .map(|_| (!cancel()).then_some(ids))
}

/// `revision` peeled to a commit, or `None` for "HEAD" on an unborn branch.
fn resolve(repo: &Repository, revision: &str) -> Result<Option<Oid>, HistoryError> {
    if revision == "HEAD" {
        return match repo.head() {
            Ok(head) => Ok(Some(head.peel_to_commit()?.id())),
            Err(error) if matches!(error.code(), ErrorCode::UnbornBranch | ErrorCode::NotFound) => {
                Ok(None)
            }
            Err(error) => Err(error.into()),
        };
    }
    Ok(Some(repo.revparse_single(revision)?.peel_to_commit()?.id()))
}

/// What a commit did to the tracked range, seen from one parent.
enum ParentOutcome {
    /// The range's lines are the same in the parent (possibly moved); carry it there.
    Unchanged(String, LineRange),
    /// The commit changed the range; carry the parent's range there when some of it existed.
    Changed(Option<(String, LineRange)>),
}

/// Called with each listed commit, its path, the path it was renamed from and the range in it;
/// returns whether to go on.
type Visit<'a> = dyn FnMut(&git2::Commit, &str, Option<String>, LineRange) -> bool + 'a;

/// Walks the range's history from `start`, calling `visit` for each commit that changed it
/// (newest first) until `visit` returns false or the history ends. Returns whether it stopped at
/// `max_commits`. `cancel` is polled every [`CANCEL_INTERVAL`] commits; a true answer just ends
/// the walk (the caller checks it again).
fn walk(
    repo: &Repository,
    start: Oid,
    path: &str,
    lines: LineRange,
    max_commits: usize,
    cancel: &dyn Fn() -> bool,
    visit: &mut Visit,
) -> Result<bool, HistoryError> {
    validate_path(path)?;
    let commit = repo.find_commit(start)?;
    let Some((blob_id, _)) = blob_at(&commit.tree()?, path) else {
        return Err(HistoryError::NotAFile {
            path: path.to_string(),
        });
    };
    let blob = repo.find_blob(blob_id)?;
    if blob.is_binary() {
        return Err(HistoryError::NotText {
            path: path.to_string(),
        });
    }
    let count = line_count(blob.content());
    if lines.start < 1 || lines.end < lines.start || lines.end > count {
        return Err(HistoryError::InvalidRange {
            start: lines.start,
            end: lines.end,
            count,
        });
    }

    let mut revwalk = repo.revwalk()?;
    // Topological: every child is visited before its parents, so a parent's range is final by
    // the time the walk reaches it.
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    revwalk.push(start)?;

    let mut tracked: HashMap<Oid, (String, LineRange)> =
        HashMap::from([(start, (path.to_string(), lines))]);
    let mut examined = 0;
    for (walked, oid) in revwalk.enumerate() {
        if tracked.is_empty() || (walked % CANCEL_INTERVAL == 0 && cancel()) {
            return Ok(false);
        }
        let oid = oid?;
        let Some((current_path, range)) = tracked.remove(&oid) else {
            continue;
        };
        if examined == max_commits {
            return Ok(true);
        }
        examined += 1;
        let commit = repo.find_commit(oid)?;
        let Some((blob_id, _)) = blob_at(&commit.tree()?, &current_path) else {
            continue;
        };
        let blob = repo.find_blob(blob_id)?;

        let mut outcomes = Vec::new();
        let mut renamed_from = None;
        for (index, parent) in commit.parents().enumerate() {
            let parent_tree = parent.tree()?;
            let (parent_path, parent_blob) = match blob_at(&parent_tree, &current_path) {
                Some((id, _)) => (current_path.clone(), id),
                None => {
                    let Some(source) =
                        rename_source(repo, &parent_tree, &commit.tree()?, &current_path)?
                    else {
                        outcomes.push((parent.id(), ParentOutcome::Changed(None)));
                        continue;
                    };
                    if index == 0 {
                        renamed_from = Some(source.clone());
                    }
                    let Some((id, _)) = blob_at(&parent_tree, &source) else {
                        outcomes.push((parent.id(), ParentOutcome::Changed(None)));
                        continue;
                    };
                    (source, id)
                }
            };
            let outcome = if parent_blob == blob_id {
                ParentOutcome::Unchanged(parent_path, range)
            } else {
                let old = repo.find_blob(parent_blob)?;
                match map_to_parent(repo, &old, &blob, range)? {
                    Mapped::Untouched(mapped) => ParentOutcome::Unchanged(parent_path, mapped),
                    Mapped::Touched(mapped) => {
                        ParentOutcome::Changed(mapped.map(|lines| (parent_path, lines)))
                    }
                }
            };
            outcomes.push((parent.id(), outcome));
        }

        // A parent with the range unchanged explains it: follow that parent only, as Git's
        // history simplification does, and don't list the commit.
        if let Some((parent, outcome)) = outcomes
            .iter()
            .find(|(_, o)| matches!(o, ParentOutcome::Unchanged(..)))
        {
            if let ParentOutcome::Unchanged(parent_path, lines) = outcome {
                track(&mut tracked, *parent, parent_path.clone(), *lines);
            }
            continue;
        }

        if !visit(&commit, &current_path, renamed_from, range) {
            return Ok(false);
        }
        for (parent, outcome) in outcomes {
            if let ParentOutcome::Changed(Some((parent_path, lines))) = outcome {
                track(&mut tracked, parent, parent_path, lines);
            }
        }
    }
    Ok(false)
}

/// Records that `parent` is to be examined for `lines` of `path`. When two children reach the
/// same parent with the same path, it is examined for the block covering both ranges; with
/// different paths, the first (nearest) child wins, as in file history.
fn track(
    tracked: &mut HashMap<Oid, (String, LineRange)>,
    parent: Oid,
    path: String,
    lines: LineRange,
) {
    tracked
        .entry(parent)
        .and_modify(|(existing_path, existing)| {
            if *existing_path == path {
                existing.start = existing.start.min(lines.start);
                existing.end = existing.end.max(lines.end);
            }
        })
        .or_insert((path, lines));
}

/// Lines in `content` as a diff counts them: a last line without a newline still counts.
fn line_count(content: &[u8]) -> usize {
    let newlines = content.iter().filter(|&&b| b == b'\n').count();
    newlines + usize::from(content.last().is_some_and(|&b| b != b'\n'))
}

/// One hunk of a zero-context diff, in libgit2's numbering: a pure insertion has
/// `old_lines == 0` and `old_start` the old line it follows; a pure deletion has
/// `new_lines == 0` and `new_start` the new line it follows.
#[derive(Debug, Clone, Copy)]
struct Hunk {
    old_start: i64,
    old_lines: i64,
    new_start: i64,
    new_lines: i64,
}

impl Hunk {
    fn new_end(&self) -> i64 {
        self.new_start + self.new_lines - 1
    }
    /// The hunk lies wholly before new line `n`.
    fn before(&self, n: i64) -> bool {
        if self.new_lines == 0 {
            self.new_start < n
        } else {
            self.new_end() < n
        }
    }
    fn contains(&self, n: i64) -> bool {
        self.new_lines > 0 && self.new_start <= n && n <= self.new_end()
    }
    /// The hunk changes line range `a..=b` (new side): it replaces or adds a line inside it, or
    /// deletes lines between two of its lines.
    fn touches(&self, a: i64, b: i64) -> bool {
        if self.new_lines == 0 {
            a <= self.new_start && self.new_start < b
        } else {
            self.new_start <= b && a <= self.new_end()
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Mapped {
    /// No change inside the range; its lines sit at this range in the parent.
    Untouched(LineRange),
    /// The commit changed the range. The parent's range, or `None` when every line of it was
    /// added by this commit.
    Touched(Option<LineRange>),
}

/// Maps `range` of `new` to the matching range of `old` through their zero-context diff. An
/// unchanged line maps to its old line; an edge inside a changed hunk maps to that hunk's old
/// side, so the parent's range covers the old version of every change in the range.
fn map_to_parent(
    repo: &Repository,
    old: &Blob,
    new: &Blob,
    range: LineRange,
) -> Result<Mapped, git2::Error> {
    if old.is_binary() || new.is_binary() {
        return Ok(Mapped::Touched(None));
    }
    let mut opts = DiffOptions::new();
    opts.context_lines(0).interhunk_lines(0);
    let mut hunks = Vec::new();
    repo.diff_blobs(
        Some(old),
        None,
        Some(new),
        None,
        Some(&mut opts),
        None,
        None,
        Some(&mut |_, hunk| {
            hunks.push(Hunk {
                old_start: i64::from(hunk.old_start()),
                old_lines: i64::from(hunk.old_lines()),
                new_start: i64::from(hunk.new_start()),
                new_lines: i64::from(hunk.new_lines()),
            });
            true
        }),
        None,
    )?;
    Ok(map_through_hunks(&hunks, range))
}

fn map_through_hunks(hunks: &[Hunk], range: LineRange) -> Mapped {
    let (a, b) = (range.start as i64, range.end as i64);
    let delta_before = |n: i64| -> i64 {
        hunks
            .iter()
            .filter(|h| h.before(n))
            .map(|h| h.new_lines - h.old_lines)
            .sum()
    };
    let start = match hunks.iter().find(|h| h.contains(a)) {
        Some(h) if h.old_lines > 0 => h.old_start,
        Some(h) => h.old_start + 1,
        None => a - delta_before(a),
    };
    let end = match hunks.iter().find(|h| h.contains(b)) {
        Some(h) if h.old_lines > 0 => h.old_start + h.old_lines - 1,
        Some(h) => h.old_start,
        None => b - delta_before(b),
    };
    let mapped = (start >= 1 && start <= end).then_some(LineRange {
        start: start as usize,
        end: end as usize,
    });
    if hunks.iter().any(|h| h.touches(a, b)) {
        Mapped::Touched(mapped)
    } else {
        // Untouched lines always map one to one.
        Mapped::Untouched(mapped.expect("an untouched range maps to lines of the parent"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hunk(old_start: i64, old_lines: i64, new_start: i64, new_lines: i64) -> Hunk {
        Hunk {
            old_start,
            old_lines,
            new_start,
            new_lines,
        }
    }

    fn r(start: usize, end: usize) -> LineRange {
        LineRange { start, end }
    }

    // Old l1..l5; new l1 l2 X Y l3 l4 l5 (two lines inserted after l2).
    const INSERT: [Hunk; 1] = [Hunk {
        old_start: 2,
        old_lines: 0,
        new_start: 3,
        new_lines: 2,
    }];

    #[test]
    fn lines_after_an_insertion_move_up_untouched() {
        assert_eq!(
            map_through_hunks(&INSERT, r(5, 7)),
            Mapped::Untouched(r(3, 5))
        );
    }

    #[test]
    fn lines_all_inserted_have_no_parent_range() {
        assert_eq!(map_through_hunks(&INSERT, r(3, 4)), Mapped::Touched(None));
    }

    #[test]
    fn a_range_around_an_insertion_keeps_its_old_lines() {
        assert_eq!(
            map_through_hunks(&INSERT, r(2, 5)),
            Mapped::Touched(Some(r(2, 3)))
        );
    }

    #[test]
    fn a_replaced_line_maps_to_its_old_version() {
        // Old l1..l5; new l1 l2 Z l4 l5.
        let hunks = [hunk(3, 1, 3, 1)];
        assert_eq!(
            map_through_hunks(&hunks, r(3, 3)),
            Mapped::Touched(Some(r(3, 3)))
        );
        assert_eq!(
            map_through_hunks(&hunks, r(4, 5)),
            Mapped::Untouched(r(4, 5))
        );
    }

    #[test]
    fn a_deletion_inside_the_range_touches_it_but_not_at_its_edge() {
        // Old l1..l5; new l1 l2 l5 (l3 and l4 deleted after new line 2).
        let hunks = [hunk(3, 2, 2, 0)];
        assert_eq!(
            map_through_hunks(&hunks, r(2, 3)),
            Mapped::Touched(Some(r(2, 5)))
        );
        assert_eq!(
            map_through_hunks(&hunks, r(3, 3)),
            Mapped::Untouched(r(5, 5))
        );
        assert_eq!(
            map_through_hunks(&hunks, r(1, 2)),
            Mapped::Untouched(r(1, 2))
        );
    }

    #[test]
    fn a_range_starting_inside_a_bigger_rewrite_widens_to_its_old_side() {
        // Old l1..l8; new l1 l2 A B C D E l6 l7 l8 (l3..l5 replaced by five lines).
        let hunks = [hunk(3, 3, 3, 5)];
        assert_eq!(
            map_through_hunks(&hunks, r(6, 9)),
            Mapped::Touched(Some(r(3, 7)))
        );
    }

    #[test]
    fn counts_a_last_line_without_a_newline() {
        assert_eq!(line_count(b""), 0);
        assert_eq!(line_count(b"a\nb\n"), 2);
        assert_eq!(line_count(b"a\nb"), 2);
    }
}
