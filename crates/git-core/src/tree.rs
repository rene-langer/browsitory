//! A commit's file tree, for browsing it without checking the commit out (#446): one folder's
//! entries at a time (folders load lazily), a path filter over the whole tree, a file's content
//! at that commit, and saving that content elsewhere. Nothing here touches the work tree or the
//! index; restoring a file is `stage::restore_file_version`.

use std::path::Path;

use git2::{ErrorCode, ObjectType, Oid, Repository, Tree, TreeWalkMode, TreeWalkResult};
use thiserror::Error;

const GITLINK_MODE: i32 = 0o160000;
const LINK_MODE: i32 = 0o120000;
const EXECUTABLE_MODE: i32 = 0o100755;

/// The most matches [`find_in_tree`] returns; the rest are reported as `truncated`.
pub const MAX_TREE_MATCHES: usize = 500;

/// Files over this many bytes are reported as [`FileContent::TooLarge`] and not read.
pub const MAX_VIEW_BYTES: u64 = 2 * 1024 * 1024;

/// How many leading bytes [`file_at_commit`] looks at for a NUL byte, as git does to call a
/// file binary.
const BINARY_SNIFF_BYTES: usize = 8000;

#[derive(Debug, Error)]
pub enum TreeError {
    #[error("git operation failed: {0}")]
    Git(#[from] git2::Error),
    #[error("path must be relative to the repository: {0}")]
    InvalidPath(String),
    #[error("'{0}' isn't in this commit.")]
    NotFound(String),
    #[error("'{0}' is not a folder in this commit.")]
    NotAFolder(String),
    #[error("'{0}' is not a file in this commit.")]
    NotAFile(String),
    #[error("Couldn't save the file: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum TreeEntryKind {
    Folder,
    File,
    /// A file with the executable bit.
    Executable,
    Symlink,
    /// A gitlink: its content lives in another repository.
    Submodule,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// Repository-relative, `/`-separated.
    pub path: String,
    pub name: String,
    pub kind: TreeEntryKind,
    /// The blob's size for files and symlinks; `None` for folders and submodules.
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeListing {
    pub entries: Vec<TreeEntry>,
    /// [`find_in_tree`] stopped at [`MAX_TREE_MATCHES`]; always `false` for [`list_tree`].
    pub truncated: bool,
}

/// One file's content at a commit, as the read-only viewer shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileContent {
    /// Text (no NUL byte near the start). Invalid UTF-8 is replaced, for display only.
    Text { text: String, size: u64 },
    /// Not text; the bytes are kept for an image preview.
    Binary { bytes: Vec<u8> },
    /// Over [`MAX_VIEW_BYTES`]; nothing was read.
    TooLarge { size: u64 },
}

fn checked(path: &str) -> Result<&Path, TreeError> {
    if crate::launch::is_safe_relative(path) {
        Ok(Path::new(path))
    } else {
        Err(TreeError::InvalidPath(path.to_string()))
    }
}

fn commit_tree<'r>(repo: &'r Repository, commit_id: &str) -> Result<Tree<'r>, TreeError> {
    Ok(repo.find_commit(Oid::from_str(commit_id)?)?.tree()?)
}

fn kind_of(mode: i32, object: Option<ObjectType>) -> TreeEntryKind {
    match (mode, object) {
        (GITLINK_MODE, _) => TreeEntryKind::Submodule,
        (_, Some(ObjectType::Tree)) => TreeEntryKind::Folder,
        (LINK_MODE, _) => TreeEntryKind::Symlink,
        (EXECUTABLE_MODE, _) => TreeEntryKind::Executable,
        _ => TreeEntryKind::File,
    }
}

fn blob_size(repo: &Repository, id: Oid, kind: TreeEntryKind) -> Result<Option<u64>, TreeError> {
    Ok(match kind {
        TreeEntryKind::File | TreeEntryKind::Executable | TreeEntryKind::Symlink => {
            Some(repo.odb()?.read_header(id)?.0 as u64)
        }
        TreeEntryKind::Folder | TreeEntryKind::Submodule => None,
    })
}

fn joined(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

/// Folders first, then everything else, each by name (case-insensitive, then exact).
fn sort_entries(entries: &mut [TreeEntry]) {
    entries.sort_by(|a, b| {
        (a.kind != TreeEntryKind::Folder)
            .cmp(&(b.kind != TreeEntryKind::Folder))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// The entries directly inside `dir` (`""`: the root) of `commit_id`'s tree: folders first,
/// then files, each sorted by name. A name that isn't UTF-8 is shown lossily.
pub fn list_tree(repo: &Repository, commit_id: &str, dir: &str) -> Result<TreeListing, TreeError> {
    let root = commit_tree(repo, commit_id)?;
    let folder = if dir.is_empty() {
        root
    } else {
        let entry = match root.get_path(checked(dir)?) {
            Ok(entry) => entry,
            Err(e) if e.code() == ErrorCode::NotFound => {
                return Err(TreeError::NotFound(dir.to_string()))
            }
            Err(e) => return Err(e.into()),
        };
        if entry.kind() != Some(ObjectType::Tree) || entry.filemode() == GITLINK_MODE {
            return Err(TreeError::NotAFolder(dir.to_string()));
        }
        repo.find_tree(entry.id())?
    };
    let mut entries = Vec::with_capacity(folder.len());
    for entry in folder.iter() {
        let name = String::from_utf8_lossy(entry.name_bytes()).into_owned();
        let kind = kind_of(entry.filemode(), entry.kind());
        entries.push(TreeEntry {
            path: joined(dir, &name),
            size: blob_size(repo, entry.id(), kind)?,
            name,
            kind,
        });
    }
    sort_entries(&mut entries);
    Ok(TreeListing {
        entries,
        truncated: false,
    })
}

/// Every non-folder entry of `commit_id`'s tree whose path contains `query` (case-insensitive),
/// in path order, at most `limit` of them (`truncated` when there were more). An empty query
/// matches nothing.
pub fn find_in_tree(
    repo: &Repository,
    commit_id: &str,
    query: &str,
    limit: usize,
) -> Result<TreeListing, TreeError> {
    let tree = commit_tree(repo, commit_id)?;
    let needle = query.trim().to_lowercase();
    let mut found: Vec<(String, TreeEntryKind, Oid)> = Vec::new();
    let mut truncated = false;
    if !needle.is_empty() {
        tree.walk(TreeWalkMode::PreOrder, |dir, entry| {
            let kind = kind_of(entry.filemode(), entry.kind());
            if kind == TreeEntryKind::Folder {
                return TreeWalkResult::Ok;
            }
            let path = format!("{dir}{}", String::from_utf8_lossy(entry.name_bytes()));
            if path.to_lowercase().contains(&needle) {
                if found.len() == limit {
                    truncated = true;
                    return TreeWalkResult::Abort;
                }
                found.push((path, kind, entry.id()));
            }
            TreeWalkResult::Ok
        })
        .or_else(|e| if truncated { Ok(()) } else { Err(e) })?;
    }
    let mut entries = Vec::with_capacity(found.len());
    for (path, kind, id) in found {
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        entries.push(TreeEntry {
            size: blob_size(repo, id, kind)?,
            path,
            name,
            kind,
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(TreeListing { entries, truncated })
}

/// The blob of the file (or symlink) at `path` in `commit_id`.
fn file_blob(repo: &Repository, commit_id: &str, path: &str) -> Result<Oid, TreeError> {
    let tree = commit_tree(repo, commit_id)?;
    match tree.get_path(checked(path)?) {
        Ok(entry) if entry.kind() == Some(ObjectType::Blob) && entry.filemode() != GITLINK_MODE => {
            Ok(entry.id())
        }
        Ok(_) => Err(TreeError::NotAFile(path.to_string())),
        Err(e) if e.code() == ErrorCode::NotFound => Err(TreeError::NotFound(path.to_string())),
        Err(e) => Err(e.into()),
    }
}

/// `path`'s content at `commit_id`, for the read-only viewer: text, binary bytes, or `TooLarge`
/// over `cap` (judged from the object header, before the blob is read). A symlink's content is
/// its target.
pub fn file_at_commit(
    repo: &Repository,
    commit_id: &str,
    path: &str,
    cap: u64,
) -> Result<FileContent, TreeError> {
    let id = file_blob(repo, commit_id, path)?;
    let size = repo.odb()?.read_header(id)?.0 as u64;
    if size > cap {
        return Ok(FileContent::TooLarge { size });
    }
    let blob = repo.find_blob(id)?;
    let bytes = blob.content();
    let sniffed = &bytes[..bytes.len().min(BINARY_SNIFF_BYTES)];
    if sniffed.contains(&0) {
        return Ok(FileContent::Binary {
            bytes: bytes.to_vec(),
        });
    }
    Ok(FileContent::Text {
        text: String::from_utf8_lossy(bytes).into_owned(),
        size,
    })
}

/// Writes `path`'s content at `commit_id` to `destination` (a path the user picked in a save
/// dialog), byte for byte as git stores it (`git show <commit>:<path>`): no line-ending or other
/// filters. Replaces an existing file there.
pub fn save_file_version(
    repo: &Repository,
    commit_id: &str,
    path: &str,
    destination: &Path,
) -> Result<(), TreeError> {
    let id = file_blob(repo, commit_id, path)?;
    let blob = repo.find_blob(id)?;
    std::fs::write(destination, blob.content())?;
    Ok(())
}
