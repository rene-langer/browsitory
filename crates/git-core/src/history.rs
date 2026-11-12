use std::collections::HashMap;
use std::path::{Component, Path};
use std::sync::Arc;
use std::time::{Duration, Instant};

use git2::{Delta, DiffFindOptions, ErrorCode, ObjectType, Oid, Repository, Tree};
use thiserror::Error;

use crate::graph::{walk_order, GraphCommit, RefDecorations, WalkCache, WalkKey};
use crate::line_history::{LineRange, LineWalk};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHistoryEntry {
    pub commit: GraphCommit,
    /// Path of the file in this commit.
    pub path: String,
    /// Set when this commit renamed `old_path` to `path` (relative to its first parent).
    pub old_path: Option<String>,
}

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("Path must be relative to the repository root.")]
    InvalidPath,
    #[error("{path} isn't a file in this commit.")]
    NotAFile { path: String },
    #[error("{path} is a binary file, so it has no lines to follow.")]
    NotText { path: String },
    #[error("Lines {start}-{end} aren't in the file, which has {count} lines.")]
    InvalidRange {
        start: usize,
        end: usize,
        count: usize,
    },
    #[error(transparent)]
    Git(#[from] git2::Error),
}

/// Commits reachable from HEAD that changed `path`, newest first, following renames.
///
/// The tracked path is carried from each commit to its parents (renamed where the parent has
/// the file under another name), so a rename on one branch doesn't rename the file on another.
/// A commit is listed when it differs from every parent at its path, so a merge that took one
/// side's version unchanged is hidden. The walk follows every parent (like
/// `git log --full-history`), so a side-branch edit later discarded by a merge is still listed.
pub fn file_history(
    repo: &Repository,
    path: &str,
    limit: usize,
) -> Result<Vec<FileHistoryEntry>, HistoryError> {
    let mut cache = WalkCache::default();
    file_history_cached(repo, &mut cache, path, limit, Duration::MAX).map(|page| page.entries)
}

/// One answer of [`file_history_cached`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHistoryPage {
    /// The first `limit` entries, or fewer when the budget ran out or the history ended.
    pub entries: Vec<FileHistoryEntry>,
    /// The walk reached the end of the file's history: no older entry exists.
    pub complete: bool,
}

/// [`file_history`] continued from where the cache's walk for the same HEAD and path stopped
/// (#860). Answers the first `limit` entries, walking further only when the cache has fewer,
/// and only until `budget` runs out (checked after each commit on the path, so a call always
/// makes progress). Sorting the walk, the first time, isn't charged to the budget. A HEAD that
/// moved starts a new walk; the badges are read as they are now.
pub fn file_history_cached(
    repo: &Repository,
    cache: &mut WalkCache,
    path: &str,
    limit: usize,
    budget: Duration,
) -> Result<FileHistoryPage, HistoryError> {
    validate_path(path)?;
    let Some(head) = head_commit(repo)? else {
        return Ok(FileHistoryPage {
            entries: Vec::new(),
            complete: true,
        });
    };
    let decorations = cache.decorations(repo)?;
    let key = HistoryKey::new(repo, head, path, None);
    let mut walk = match cache.histories.files.take(&key) {
        Some(walk) => walk,
        None => {
            let order = history_order(repo, cache, &key.walk, head)?;
            cache.stats.history_walks += 1;
            FileWalk::new(repo, order, head, path)?
        }
    };
    let deadline = Instant::now().checked_add(budget);
    let advanced = walk.advance(repo, limit, deadline, &mut cache.stats.history_commits);
    let page = advanced.and_then(|()| walk.page(repo, &decorations, limit));
    // A walk that failed part-way isn't kept: the next call starts again.
    if page.is_ok() {
        cache.histories.files.put(key, walk);
    }
    page
}

/// HEAD's commit, or `None` on an unborn branch.
fn head_commit(repo: &Repository) -> Result<Option<Oid>, git2::Error> {
    match repo.head() {
        Ok(head) => Ok(Some(head.peel_to_commit()?.id())),
        Err(error) if matches!(error.code(), ErrorCode::UnbornBranch | ErrorCode::NotFound) => {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

/// The order of the walk from `start`: a graph's or history's sorted already, or sorted now.
pub(crate) fn history_order(
    repo: &Repository,
    cache: &WalkCache,
    key: &WalkKey,
    start: Oid,
) -> Result<Arc<[Oid]>, git2::Error> {
    match cache.cached_order(key) {
        Some(order) => Ok(order),
        None => Ok(walk_order(repo, &[start], false, &|| false)?
            .expect("a walk that is never cancelled finishes")),
    }
}

/// What a kept history walk is for: the walk from one start commit (with the repository and
/// what changes parents, as the graph's walks are keyed), the path, and the lines for a line
/// history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HistoryKey {
    pub(crate) walk: WalkKey,
    path: String,
    lines: Option<LineRange>,
}

impl HistoryKey {
    pub(crate) fn new(repo: &Repository, start: Oid, path: &str, lines: Option<LineRange>) -> Self {
        Self {
            walk: WalkKey::new(repo, &[start], false),
            path: path.to_string(),
            lines,
        }
    }
}

/// How many file history walks, and how many line history walks, a [`WalkCache`] keeps: the
/// file open in the history panel and the one before it.
const HISTORIES_KEPT: usize = 2;

/// The file and line history walks a [`WalkCache`] keeps, where they stopped (#860).
#[derive(Default)]
pub(crate) struct HistoryWalks {
    pub(crate) files: Kept<FileWalk>,
    pub(crate) lines: Kept<LineWalk>,
}

impl HistoryWalks {
    /// The order of a kept walk with the key `key`.
    pub(crate) fn order(&self, key: &WalkKey) -> Option<Arc<[Oid]>> {
        let files = self.files.0.iter().map(|(k, walk)| (k, &walk.order));
        let lines = self.lines.0.iter().map(|(k, walk)| (k, walk.order()));
        files
            .chain(lines)
            .find(|(k, _)| k.walk == *key)
            .map(|(_, order)| Arc::clone(order))
    }
}

/// Walks kept by key, most recently used first, at most [`HISTORIES_KEPT`].
pub(crate) struct Kept<T>(Vec<(HistoryKey, T)>);

impl<T> Default for Kept<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<T> Kept<T> {
    /// Takes the walk for `key` out, to be `put` back when the call is done with it.
    pub(crate) fn take(&mut self, key: &HistoryKey) -> Option<T> {
        let index = self.0.iter().position(|(k, _)| k == key)?;
        Some(self.0.remove(index).1)
    }

    pub(crate) fn put(&mut self, key: HistoryKey, walk: T) {
        self.0.insert(0, (key, walk));
        self.0.truncate(HISTORIES_KEPT);
    }
}

/// A file history walk where it stopped: the next position in the walk order, the path (and
/// its tree entries) tracked at each commit still to be examined, and what it found so far.
pub(crate) struct FileWalk {
    order: Arc<[Oid]>,
    next: usize,
    /// `None` entries: the path isn't a file there, but the commit still counts as tracked, so
    /// a later child doesn't track it under another path (first, nearest child wins).
    tracked: HashMap<Oid, (String, Option<PathEntries>)>,
    found: Vec<Found>,
}

/// A listed commit, without its badges (read as they are when a page is answered).
struct Found {
    id: Oid,
    path: String,
    old_path: Option<String>,
}

impl FileWalk {
    fn new(
        repo: &Repository,
        order: Arc<[Oid]>,
        head: Oid,
        path: &str,
    ) -> Result<Self, HistoryError> {
        let head_entries = path_entries(repo, repo.find_commit(head)?.tree_id(), path)?;
        Ok(Self {
            order,
            next: 0,
            tracked: HashMap::from([(head, (path.to_string(), head_entries))]),
            found: Vec::new(),
        })
    }

    /// No commit is left to examine.
    fn done(&self) -> bool {
        self.tracked.is_empty() || self.next >= self.order.len()
    }

    /// Walks on until `want` commits are found, the history ends, or `deadline` passed after a
    /// commit was examined. `examined` counts the commits examined.
    fn advance(
        &mut self,
        repo: &Repository,
        want: usize,
        deadline: Option<Instant>,
        examined: &mut usize,
    ) -> Result<(), HistoryError> {
        // Topological: every child comes before its parents, so a parent's tracked path is final
        // by the time the walk reaches it.
        while self.found.len() < want && !self.done() {
            let oid = self.order[self.next];
            self.next += 1;
            let Some((current_path, Some(current))) = self.tracked.remove(&oid) else {
                continue;
            };
            *examined += 1;
            self.examine(repo, oid, current_path, &current)?;
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
        current_path: String,
        current: &PathEntries,
    ) -> Result<(), HistoryError> {
        let commit = repo.find_commit(oid)?;
        let blob = current.blob();

        let mut same_as_a_parent = false;
        let mut renamed_from = None;
        for (index, parent) in commit.parents().enumerate() {
            let tracked_in_parent =
                match parent_path_entries(repo, parent.tree_id(), current, &current_path)? {
                    Some(parent_entries) => {
                        same_as_a_parent |= parent_entries.blob() == blob;
                        Some((current_path.clone(), Some(parent_entries)))
                    }
                    None => {
                        let parent_tree = parent.tree()?;
                        let source =
                            rename_source(repo, &parent_tree, &commit.tree()?, &current_path)?;
                        if index == 0 {
                            renamed_from.clone_from(&source);
                        }
                        match source {
                            Some(source) => {
                                let parent_entries = path_entries(repo, parent_tree.id(), &source)?;
                                Some((source, parent_entries))
                            }
                            None => None,
                        }
                    }
                };
            if let Some(tracked_in_parent) = tracked_in_parent {
                // First (nearest) child wins when two children track the same parent.
                self.tracked.entry(parent.id()).or_insert(tracked_in_parent);
            }
        }

        if !same_as_a_parent {
            self.found.push(Found {
                id: oid,
                path: current_path,
                old_path: renamed_from,
            });
        }
        Ok(())
    }

    /// The first `limit` entries found, with the badges in `decorations`.
    fn page(
        &self,
        repo: &Repository,
        decorations: &RefDecorations,
        limit: usize,
    ) -> Result<FileHistoryPage, HistoryError> {
        let entries = self
            .found
            .iter()
            .take(limit)
            .map(|found| {
                Ok(FileHistoryEntry {
                    commit: decorations.graph_commit(&repo.find_commit(found.id)?),
                    path: found.path.clone(),
                    old_path: found.old_path.clone(),
                })
            })
            .collect::<Result<Vec<_>, git2::Error>>()?;
        Ok(FileHistoryPage {
            entries,
            complete: self.done() && self.found.len() <= limit,
        })
    }
}

pub(crate) fn validate_path(path: &str) -> Result<(), HistoryError> {
    let relative = Path::new(path);
    if path.is_empty()
        || !relative
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err(HistoryError::InvalidPath);
    }
    Ok(())
}

/// The path `new_path` was renamed from between `old` and `new`, if libgit2 sees a rename.
pub(crate) fn rename_source(
    repo: &Repository,
    old: &Tree,
    new: &Tree,
    new_path: &str,
) -> Result<Option<String>, git2::Error> {
    let mut diff = repo.diff_tree_to_tree(Some(old), Some(new), None)?;
    let mut find = DiffFindOptions::new();
    find.renames(true);
    diff.find_similar(Some(&mut find))?;
    Ok(diff
        .deltas()
        .find(|d| d.status() == Delta::Renamed && d.new_file().path() == Some(Path::new(new_path)))
        .and_then(|d| {
            d.old_file()
                .path()
                .map(|p| p.to_string_lossy().into_owned())
        }))
}

/// The tree entries along a file's path in one commit, from the root tree's entry down to the
/// file's own (`a`, `a/b`, `a/b/c` for `a/b/c`), each as (id, file mode). Where a parent's
/// entry equals the child's, everything below it is equal too, so the walks compare these
/// top-down and read a parent's folders only where they differ (#838).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PathEntries(Vec<(Oid, i32)>);

impl PathEntries {
    /// The file's blob id and mode.
    pub(crate) fn blob(&self) -> (Oid, i32) {
        *self.0.last().expect("a path has at least one component")
    }
}

/// The entries along `path` in the tree `tree`, or `None` when a component is missing, a folder
/// component isn't a folder, or the last one isn't a file (a folder or a submodule).
pub(crate) fn path_entries(
    repo: &Repository,
    tree: Oid,
    path: &str,
) -> Result<Option<PathEntries>, git2::Error> {
    entries_from(repo, tree, &components(path), Vec::new())
}

/// The entries along `path` in a parent's root tree `parent_tree`, given the child's entries
/// `child` for the same path. Reads only the parent's folders whose ids differ from the child's.
pub(crate) fn parent_path_entries(
    repo: &Repository,
    parent_tree: Oid,
    child: &PathEntries,
    path: &str,
) -> Result<Option<PathEntries>, git2::Error> {
    let names = components(path);
    let mut found = Vec::with_capacity(names.len());
    let mut tree = parent_tree;
    for (depth, name) in names.iter().enumerate() {
        let Some(entry) = tree_entry(repo, tree, name)? else {
            return Ok(None);
        };
        let pair = (entry.id, entry.mode);
        if child.0.get(depth) == Some(&pair) {
            found.extend_from_slice(&child.0[depth..]);
            return Ok(Some(PathEntries(found)));
        }
        let last = depth + 1 == names.len();
        if entry.kind != expected_kind(last) {
            return Ok(None);
        }
        found.push(pair);
        tree = entry.id;
    }
    Ok(Some(PathEntries(found)))
}

fn components(path: &str) -> Vec<&str> {
    path.split('/').collect()
}

fn expected_kind(last: bool) -> ObjectType {
    if last {
        ObjectType::Blob
    } else {
        ObjectType::Tree
    }
}

fn entries_from(
    repo: &Repository,
    tree: Oid,
    names: &[&str],
    mut found: Vec<(Oid, i32)>,
) -> Result<Option<PathEntries>, git2::Error> {
    let Some((name, rest)) = names.split_first() else {
        return Ok(Some(PathEntries(found)));
    };
    let Some(entry) = tree_entry(repo, tree, name)? else {
        return Ok(None);
    };
    if entry.kind != expected_kind(rest.is_empty()) {
        return Ok(None);
    }
    found.push((entry.id, entry.mode));
    if rest.is_empty() {
        return Ok(Some(PathEntries(found)));
    }
    entries_from(repo, entry.id, rest, found)
}

/// One entry of a tree: its id, its file mode as `git2::TreeEntry::filemode` reports it, and
/// what kind of object it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RawEntry {
    id: Oid,
    mode: i32,
    kind: ObjectType,
}

/// The entry `name` of tree `tree`, read from the tree's stored form without building a
/// `git2::Tree`: parsing every entry of a big root tree (thousands) was most of a history walk's
/// time, and a walk needs one entry of it.
fn tree_entry(repo: &Repository, tree: Oid, name: &str) -> Result<Option<RawEntry>, git2::Error> {
    let odb = repo.odb()?;
    let object = odb.read(tree)?;
    if object.kind() != ObjectType::Tree {
        return Err(git2::Error::from_str(&format!("{tree} isn't a tree")));
    }
    find_raw_entry(object.data(), name.as_bytes())
        .ok_or_else(|| git2::Error::from_str(&format!("tree {tree} is corrupted")))
}

/// Looks `name` up in a tree's stored form: entries of `<octal mode> <name>\0<20-byte id>`.
/// `Some(None)` when the tree has no such entry, `None` when the data isn't a valid tree.
fn find_raw_entry(mut data: &[u8], name: &[u8]) -> Option<Option<RawEntry>> {
    const ID_LEN: usize = 20;
    while !data.is_empty() {
        let space = data.iter().position(|&b| b == b' ')?;
        let raw_mode = parse_octal(&data[..space])?;
        let rest = &data[space + 1..];
        let nul = rest.iter().position(|&b| b == 0)?;
        let id = rest.get(nul + 1..nul + 1 + ID_LEN)?;
        if &rest[..nul] == name {
            return Some(Some(RawEntry {
                id: Oid::from_bytes(id).ok()?,
                mode: normalize_mode(raw_mode),
                kind: kind_of_mode(raw_mode),
            }));
        }
        data = &rest[nul + 1 + ID_LEN..];
    }
    Some(None)
}

/// An entry's octal mode as libgit2 reads it: leading zeros allowed, at most 0o177777.
fn parse_octal(digits: &[u8]) -> Option<u32> {
    if digits.is_empty() {
        return None;
    }
    digits.iter().try_fold(0u32, |mode, &digit| {
        let digit = digit.checked_sub(b'0').filter(|&d| d < 8)?;
        let mode = mode * 8 + u32::from(digit);
        (mode <= 0o177777).then_some(mode)
    })
}

const MODE_TYPE_MASK: u32 = 0o170000;
const MODE_TREE: u32 = 0o040000;
const MODE_COMMIT: u32 = 0o160000;
const MODE_LINK: u32 = 0o120000;

/// libgit2's `normalize_filemode`, which `git_tree_entry_filemode` applies.
fn normalize_mode(mode: u32) -> i32 {
    match mode & MODE_TYPE_MASK {
        MODE_TREE => 0o040000,
        _ if mode & 0o111 != 0 => 0o100755,
        MODE_COMMIT => 0o160000,
        MODE_LINK => 0o120000,
        _ => 0o100644,
    }
}

/// libgit2's `git_tree_entry_type`.
fn kind_of_mode(mode: u32) -> ObjectType {
    match mode & MODE_TYPE_MASK {
        MODE_COMMIT => ObjectType::Commit,
        MODE_TREE => ObjectType::Tree,
        _ => ObjectType::Blob,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn tree_with(repo: &Repository, entries: &[(&str, Oid, i32)]) -> Oid {
        let mut builder = repo.treebuilder(None).unwrap();
        for (name, id, mode) in entries {
            builder.insert(name, *id, *mode).unwrap();
        }
        builder.write().unwrap()
    }

    fn loose_object(repo: &Repository, id: Oid) -> std::path::PathBuf {
        let hex = id.to_string();
        repo.path().join("objects").join(&hex[..2]).join(&hex[2..])
    }

    #[test]
    fn a_parent_folder_equal_to_the_childs_is_not_read() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let blob = repo.blob(b"f\n").unwrap();
        let other = repo.blob(b"other\n").unwrap();
        let inner = tree_with(&repo, &[("f.txt", blob, 0o100644)]);
        let folder = tree_with(&repo, &[("b", inner, 0o040000)]);
        let child_root = tree_with(&repo, &[("a", folder, 0o040000), ("x", blob, 0o100644)]);
        let parent_root = tree_with(&repo, &[("a", folder, 0o040000), ("x", other, 0o100644)]);
        let child = path_entries(&repo, child_root, "a/b/f.txt")
            .unwrap()
            .unwrap();
        // The shared folder `a` (and `a/b` in it) is no longer in the object database: equal
        // ids at `a` must answer without reading below it.
        for id in [folder, inner] {
            let path = loose_object(&repo, id);
            let mut permissions = std::fs::metadata(&path).unwrap().permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            std::fs::set_permissions(&path, permissions).unwrap();
            std::fs::remove_file(path).unwrap();
        }
        let repo = Repository::open(dir.path()).unwrap();

        let parent = parent_path_entries(&repo, parent_root, &child, "a/b/f.txt").unwrap();

        assert_eq!(parent, Some(child));
    }

    #[test]
    fn a_parent_reads_only_the_folders_that_differ() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let blob = repo.blob(b"f\n").unwrap();
        let edited = repo.blob(b"edited\n").unwrap();
        let inner = tree_with(&repo, &[("f.txt", blob, 0o100644)]);
        let inner_edited = tree_with(&repo, &[("f.txt", edited, 0o100644)]);
        let folder = tree_with(&repo, &[("b", inner, 0o040000)]);
        let folder_edited = tree_with(&repo, &[("b", inner_edited, 0o040000)]);
        let child_root = tree_with(&repo, &[("a", folder_edited, 0o040000)]);
        let parent_root = tree_with(&repo, &[("a", folder, 0o040000)]);
        let child = path_entries(&repo, child_root, "a/b/f.txt")
            .unwrap()
            .unwrap();

        let parent = parent_path_entries(&repo, parent_root, &child, "a/b/f.txt")
            .unwrap()
            .unwrap();

        assert_eq!(parent.blob(), (blob, 0o100644));
        assert_eq!(
            parent,
            PathEntries(vec![
                (folder, 0o040000),
                (inner, 0o040000),
                (blob, 0o100644)
            ])
        );
        // A file where the path needs a folder, or a missing entry, ends the path.
        let file_at_a = tree_with(&repo, &[("a", blob, 0o100644)]);
        assert_eq!(
            parent_path_entries(&repo, file_at_a, &child, "a/b/f.txt").unwrap(),
            None
        );
        let empty = tree_with(&repo, &[]);
        assert_eq!(
            parent_path_entries(&repo, empty, &child, "a/b/f.txt").unwrap(),
            None
        );
    }

    /// The stored-form lookup agrees with libgit2's parsed tree for every kind of entry,
    /// including modes libgit2 normalizes (group-writable, any execute bit).
    #[test]
    fn raw_tree_entries_match_libgit2s() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let blob = repo.blob(b"x\n").unwrap();
        let folder = tree_with(&repo, &[("f", blob, 0o100644)]);
        let modes: [(&str, u32, Oid); 8] = [
            ("dir", 0o40000, folder),
            ("exec", 0o100755, blob),
            ("group", 0o100664, blob),
            ("link", 0o120000, blob),
            ("module", 0o160000, blob),
            ("odd exec", 0o100711, blob),
            ("plain", 0o100644, blob),
            ("z", 0o100644, blob),
        ];
        let mut raw = Vec::new();
        for (name, mode, id) in modes {
            raw.extend_from_slice(format!("{mode:o} {name}\0").as_bytes());
            raw.extend_from_slice(id.as_bytes());
        }
        let tree_id = repo.odb().unwrap().write(ObjectType::Tree, &raw).unwrap();
        let tree = repo.find_tree(tree_id).unwrap();

        for (name, _, _) in modes {
            let ours = tree_entry(&repo, tree_id, name).unwrap().unwrap();
            let theirs = tree.get_name(name).unwrap();
            assert_eq!(ours.id, theirs.id(), "{name}");
            assert_eq!(ours.mode, theirs.filemode(), "{name}");
            assert_eq!(Some(ours.kind), theirs.kind(), "{name}");
        }
        assert_eq!(tree_entry(&repo, tree_id, "missing").unwrap(), None);
        assert_eq!(tree_entry(&repo, tree_id, "di").unwrap(), None);
        assert!(tree_entry(&repo, blob, "f").is_err());
    }

    #[test]
    fn a_corrupted_tree_is_an_error_not_a_missing_entry() {
        assert_eq!(find_raw_entry(b"100644 f", b"f"), None);
        assert_eq!(find_raw_entry(b"100644 f\0short", b"f"), None);
        assert_eq!(find_raw_entry(b"10064x f\0", b"f"), None);
        assert_eq!(find_raw_entry(b"", b"f"), Some(None));
    }

    /// Modes are read as libgit2 reads them: octal digits, leading zeros allowed, at most
    /// 0o177777; an empty or larger mode makes the tree corrupted.
    #[test]
    fn modes_parse_as_libgit2_parses_them() {
        let dir = TempDir::new().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let blob = repo.blob(b"x\n").unwrap();
        let tree_of = |mode: &str| {
            let mut raw = format!("{mode} f\0").into_bytes();
            raw.extend_from_slice(blob.as_bytes());
            repo.odb().unwrap().write(ObjectType::Tree, &raw).unwrap()
        };

        for mode in ["0100644", "177777"] {
            let tree = tree_of(mode);
            let theirs = repo.find_tree(tree).unwrap();
            let ours = tree_entry(&repo, tree, "f").unwrap().unwrap();
            let theirs = theirs.get_name("f").unwrap();
            assert_eq!(ours.mode, theirs.filemode(), "{mode:?}");
            assert_eq!(Some(ours.kind), theirs.kind(), "{mode:?}");
        }

        for mode in ["", "100648", "200000", "1777777", "100000000000644"] {
            let tree = tree_of(mode);
            assert!(repo.find_tree(tree).is_err(), "libgit2 accepts {mode:?}");
            assert!(tree_entry(&repo, tree, "f").is_err(), "{mode:?}");
        }
    }
}
