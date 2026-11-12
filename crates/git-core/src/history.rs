use std::collections::HashMap;
use std::path::{Component, Path};

use git2::{Delta, DiffFindOptions, ErrorCode, ObjectType, Oid, Repository, Sort, Tree};
use thiserror::Error;

use crate::graph::{GraphCommit, RefDecorations};

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
    validate_path(path)?;
    let head = match repo.head() {
        Ok(head) => head,
        Err(error) if matches!(error.code(), ErrorCode::UnbornBranch | ErrorCode::NotFound) => {
            return Ok(Vec::new())
        }
        Err(error) => return Err(error.into()),
    };
    let head_id = head.peel_to_commit()?.id();
    let decorations = RefDecorations::load(repo)?;

    let mut walk = repo.revwalk()?;
    // Topological: every child is visited before its parents, so a parent's tracked path is
    // final by the time the walk reaches it.
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
    walk.push(head_id)?;

    let head_entries = path_entries(repo, repo.find_commit(head_id)?.tree_id(), path)?;
    let mut tracked: HashMap<Oid, (String, Option<PathEntries>)> =
        HashMap::from([(head_id, (path.to_string(), head_entries))]);
    let mut entries = Vec::new();
    for oid in walk {
        if entries.len() >= limit || tracked.is_empty() {
            break;
        }
        let oid = oid?;
        let Some((current_path, Some(current))) = tracked.remove(&oid) else {
            continue;
        };
        let commit = repo.find_commit(oid)?;
        let blob = current.blob();

        let mut same_as_a_parent = false;
        let mut renamed_from = None;
        for (index, parent) in commit.parents().enumerate() {
            let tracked_in_parent =
                match parent_path_entries(repo, parent.tree_id(), &current, &current_path)? {
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
                tracked.entry(parent.id()).or_insert(tracked_in_parent);
            }
        }

        if !same_as_a_parent {
            entries.push(FileHistoryEntry {
                commit: decorations.graph_commit(&commit),
                path: current_path,
                old_path: renamed_from,
            });
        }
    }
    Ok(entries)
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

fn parse_octal(digits: &[u8]) -> Option<u32> {
    if digits.is_empty() || digits.len() > 7 {
        return None;
    }
    digits.iter().try_fold(0u32, |mode, &digit| {
        (b'0'..=b'7')
            .contains(&digit)
            .then(|| mode * 8 + u32::from(digit - b'0'))
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
