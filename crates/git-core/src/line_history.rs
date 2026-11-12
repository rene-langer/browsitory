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
use std::sync::Arc;
use std::time::{Duration, Instant};

use git2::{Blob, DiffOptions, ErrorCode, Oid, Repository};

use crate::graph::{walk_order, GraphCommit, RefDecorations, WalkCache};
use crate::history::{
    history_order, parent_path_entries, path_entries, rename_source, validate_path, HistoryError,
    HistoryKey, PathEntries,
};

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
    /// No older entry will come: the range's history ended, or the walk was truncated.
    pub complete: bool,
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
    let mut cache = WalkCache::default();
    line_history_cached(
        repo,
        &mut cache,
        revision,
        path,
        lines,
        limit,
        Duration::MAX,
    )
}

/// [`line_history`] continued from where the cache's walk for the same start commit, path and
/// lines stopped (#860). Answers the first `limit` entries, walking further only when the cache
/// has fewer, and only until `budget` runs out (checked after each commit on the range's path,
/// so a call always makes progress). Sorting the walk, the first time, isn't charged to the
/// budget. "HEAD" is resolved first, so a HEAD that moved starts a new walk.
pub fn line_history_cached(
    repo: &Repository,
    cache: &mut WalkCache,
    revision: &str,
    path: &str,
    lines: LineRange,
    limit: usize,
    budget: Duration,
) -> Result<LineHistory, HistoryError> {
    let start = resolve(repo, revision)?;
    let Some(start) = start.filter(|_| limit > 0) else {
        return Ok(LineHistory {
            entries: Vec::new(),
            truncated: false,
            complete: start.is_none(),
        });
    };
    let decorations = cache.decorations(repo)?;
    let key = HistoryKey::new(repo, start, path, Some(lines));
    let mut walk = match cache.histories.lines.take(&key) {
        Some(walk) => walk,
        None => {
            let entries = start_entries(repo, start, path, lines)?;
            let order = history_order(repo, cache, &key.walk, start)?;
            cache.stats.history_walks += 1;
            LineWalk::new(order, start, path, lines, entries)
        }
    };
    let deadline = Instant::now().checked_add(budget);
    let advanced = walk.advance(
        repo,
        limit,
        deadline,
        &|| false,
        &mut cache.stats.history_commits,
    );
    let page = advanced.and_then(|()| walk.page(repo, &decorations, limit));
    // A walk that failed part-way isn't kept: the next call starts again.
    if page.is_ok() {
        cache.histories.lines.put(key, walk);
    }
    page
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
    let entries = start_entries(repo, start, path, lines)?;
    let Some(order) = walk_order(repo, &[start], false, cancel)? else {
        return Ok(None);
    };
    let mut walk = LineWalk::new(order, start, path, lines, entries);
    walk.advance(repo, usize::MAX, None, cancel, &mut 0)?;
    Ok((!cancel()).then(|| walk.found.iter().map(|found| found.id).collect()))
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

/// The path's tree entries in `start`, once the path, the file and the range are checked: a
/// text file that has every line of the range.
fn start_entries(
    repo: &Repository,
    start: Oid,
    path: &str,
    lines: LineRange,
) -> Result<PathEntries, HistoryError> {
    validate_path(path)?;
    let commit = repo.find_commit(start)?;
    let Some(start_entries) = path_entries(repo, commit.tree_id(), path)? else {
        return Err(HistoryError::NotAFile {
            path: path.to_string(),
        });
    };
    let blob = repo.find_blob(start_entries.blob().0)?;
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
    Ok(start_entries)
}

/// What a commit did to the tracked range, seen from one parent.
enum ParentOutcome {
    /// The range's lines are the same in the parent (possibly moved); carry it there.
    Unchanged(Tracked),
    /// The commit changed the range; carry the parent's range there when some of it existed.
    Changed(Option<Tracked>),
}

/// A commit still to be examined: the path and lines followed there, and the path's tree
/// entries in it (found when its child was examined, so they aren't looked up twice).
struct Tracked {
    path: String,
    lines: LineRange,
    entries: PathEntries,
}

/// A listed commit, without its badges (read as they are when a page is answered).
struct Found {
    id: Oid,
    path: String,
    old_path: Option<String>,
    lines: LineRange,
}

/// A line history walk where it stopped (#860): the next position in the walk order, the range
/// tracked at each commit still to be examined, and what it found so far.
pub(crate) struct LineWalk {
    order: Arc<[Oid]>,
    next: usize,
    tracked: HashMap<Oid, Tracked>,
    /// Commits on the range's path examined so far, at most [`MAX_LINE_HISTORY_COMMITS`].
    examined: usize,
    /// The walk stopped at [`MAX_LINE_HISTORY_COMMITS`] before the range's history ended.
    truncated: bool,
    found: Vec<Found>,
}

impl LineWalk {
    fn new(
        order: Arc<[Oid]>,
        start: Oid,
        path: &str,
        lines: LineRange,
        entries: PathEntries,
    ) -> Self {
        Self {
            order,
            next: 0,
            tracked: HashMap::from([(
                start,
                Tracked {
                    path: path.to_string(),
                    lines,
                    entries,
                },
            )]),
            examined: 0,
            truncated: false,
            found: Vec::new(),
        }
    }

    pub(crate) fn order(&self) -> &Arc<[Oid]> {
        &self.order
    }

    /// No commit is left to examine, or the walk stopped at its cap.
    fn done(&self) -> bool {
        self.truncated || self.tracked.is_empty() || self.next >= self.order.len()
    }

    /// Walks on until `want` commits are found, the history ends, the walk reaches
    /// [`MAX_LINE_HISTORY_COMMITS`], or `deadline` passed after a commit was examined.
    /// `cancel` is polled every [`CANCEL_INTERVAL`] commits; a true answer just ends the call
    /// (the caller checks it again). `examined` counts the commits examined.
    fn advance(
        &mut self,
        repo: &Repository,
        want: usize,
        deadline: Option<Instant>,
        cancel: &dyn Fn() -> bool,
        examined: &mut usize,
    ) -> Result<(), HistoryError> {
        // Topological: every child comes before its parents, so a parent's range is final by
        // the time the walk reaches it.
        while self.found.len() < want && !self.done() {
            if self.next.is_multiple_of(CANCEL_INTERVAL) && cancel() {
                return Ok(());
            }
            let oid = self.order[self.next];
            self.next += 1;
            let Some(tracked) = self.tracked.remove(&oid) else {
                continue;
            };
            if self.examined == MAX_LINE_HISTORY_COMMITS {
                self.truncated = true;
                return Ok(());
            }
            self.examined += 1;
            *examined += 1;
            self.examine(repo, oid, tracked)?;
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                break;
            }
        }
        Ok(())
    }

    fn examine(
        &mut self,
        repo: &Repository,
        oid: Oid,
        tracked: Tracked,
    ) -> Result<(), HistoryError> {
        let Tracked {
            path: current_path,
            lines: range,
            entries: current,
        } = tracked;
        let commit = repo.find_commit(oid)?;
        let blob_id = current.blob().0;
        // Read only when a parent's version differs.
        let mut blob = None;

        let mut outcomes = Vec::new();
        let mut renamed_from = None;
        for (index, parent) in commit.parents().enumerate() {
            let (parent_path, parent_entries) =
                match parent_path_entries(repo, parent.tree_id(), &current, &current_path)? {
                    Some(entries) => (current_path.clone(), entries),
                    None => {
                        let parent_tree = parent.tree()?;
                        let Some(source) =
                            rename_source(repo, &parent_tree, &commit.tree()?, &current_path)?
                        else {
                            outcomes.push((parent.id(), ParentOutcome::Changed(None)));
                            continue;
                        };
                        if index == 0 {
                            renamed_from = Some(source.clone());
                        }
                        let Some(entries) = path_entries(repo, parent_tree.id(), &source)? else {
                            outcomes.push((parent.id(), ParentOutcome::Changed(None)));
                            continue;
                        };
                        (source, entries)
                    }
                };
            let parent_blob = parent_entries.blob().0;
            let outcome = if parent_blob == blob_id {
                ParentOutcome::Unchanged(Tracked {
                    path: parent_path,
                    lines: range,
                    entries: parent_entries,
                })
            } else {
                if blob.is_none() {
                    blob = Some(repo.find_blob(blob_id)?);
                }
                let new = blob.as_ref().expect("read just above");
                let old = repo.find_blob(parent_blob)?;
                match map_to_parent(repo, &old, new, range)? {
                    Mapped::Untouched(mapped) => ParentOutcome::Unchanged(Tracked {
                        path: parent_path,
                        lines: mapped,
                        entries: parent_entries,
                    }),
                    Mapped::Touched(mapped) => {
                        ParentOutcome::Changed(mapped.map(|lines| Tracked {
                            path: parent_path,
                            lines,
                            entries: parent_entries,
                        }))
                    }
                }
            };
            outcomes.push((parent.id(), outcome));
        }

        // A parent with the range unchanged explains it: follow that parent only, as Git's
        // history simplification does, and don't list the commit.
        if let Some(index) = outcomes
            .iter()
            .position(|(_, o)| matches!(o, ParentOutcome::Unchanged(..)))
        {
            if let (parent, ParentOutcome::Unchanged(unchanged)) = outcomes.swap_remove(index) {
                track(&mut self.tracked, parent, unchanged);
            }
            return Ok(());
        }

        self.found.push(Found {
            id: oid,
            path: current_path,
            old_path: renamed_from,
            lines: range,
        });
        for (parent, outcome) in outcomes {
            if let ParentOutcome::Changed(Some(changed)) = outcome {
                track(&mut self.tracked, parent, changed);
            }
        }
        Ok(())
    }

    /// The first `limit` entries found, with the badges in `decorations`.
    fn page(
        &self,
        repo: &Repository,
        decorations: &RefDecorations,
        limit: usize,
    ) -> Result<LineHistory, HistoryError> {
        let entries = self
            .found
            .iter()
            .take(limit)
            .map(|found| {
                Ok(LineHistoryEntry {
                    commit: decorations.graph_commit(&repo.find_commit(found.id)?),
                    path: found.path.clone(),
                    old_path: found.old_path.clone(),
                    lines: found.lines,
                })
            })
            .collect::<Result<Vec<_>, git2::Error>>()?;
        let all_found = self.found.len() <= limit;
        Ok(LineHistory {
            entries,
            truncated: self.truncated && all_found,
            complete: self.done() && all_found,
        })
    }
}

/// Records that `parent` is to be examined for `lines` of `path`. When two children reach the
/// same parent with the same path, it is examined for the block covering both ranges; with
/// different paths, the first (nearest) child wins, as in file history.
fn track(tracked: &mut HashMap<Oid, Tracked>, parent: Oid, next: Tracked) {
    tracked
        .entry(parent)
        .and_modify(|existing| {
            if existing.path == next.path {
                existing.lines.start = existing.lines.start.min(next.lines.start);
                existing.lines.end = existing.lines.end.max(next.lines.end);
            }
        })
        .or_insert(next);
}

/// Lines in `content` as a diff counts them: a last line without a newline still counts.
pub(crate) fn line_count(content: &[u8]) -> usize {
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
