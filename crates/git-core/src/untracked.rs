//! Untracked folders (#845). The status scan doesn't recurse into a wholly untracked folder:
//! libgit2 reports it as one `dir/` entry, as `git status` does by default. A folder with at most
//! [`crate::status::UNTRACKED_FOLDER_THRESHOLD`] files is then listed file by file from a walk
//! that stops one file past the threshold, so a forgotten `node_modules` costs a few hundred
//! file checks, not a listing of every file in it.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::status::UNTRACKED_FOLDER_THRESHOLD;

/// What a walk of an untracked folder found.
pub(crate) enum Folder {
    /// Its files as `git status --untracked-files=all` lists them (sorted), a nested repository
    /// as one `sub/` entry.
    Files(Vec<String>),
    /// More files than the walk's limit.
    Large,
}

/// Walks the untracked folder `folder` (`dir/`, relative to `workdir`): its non-ignored files,
/// and each nested repository as one `sub/` entry. With `limit`, stops at the first file past it.
pub(crate) fn walk(
    repo: &git2::Repository,
    workdir: &Path,
    folder: &str,
    limit: Option<usize>,
) -> Folder {
    let ignored = |path: &str| repo.status_should_ignore(Path::new(path)).unwrap_or(false);
    let mut files = Vec::new();
    let mut stack = vec![folder.to_string()];
    while let Some(dir) = stack.pop() {
        let Ok(read) = fs::read_dir(workdir.join(&dir)) else {
            continue;
        };
        for entry in read.flatten() {
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if name == ".git" {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = format!("{dir}{name}");
            if kind.is_dir() {
                let folder = format!("{path}/");
                if ignored(&folder) {
                    continue;
                }
                if is_repository(workdir, &path) {
                    files.push(folder);
                } else {
                    stack.push(folder);
                }
            } else if !ignored(&path) {
                files.push(path);
            }
            if limit.is_some_and(|limit| files.len() > limit) {
                return Folder::Large;
            }
        }
    }
    files.sort();
    Folder::Files(files)
}

/// Whether the folder `path` (relative to `workdir`) is a nested repository.
pub(crate) fn is_repository(workdir: &Path, path: &str) -> bool {
    workdir.join(path).join(".git").exists()
}

/// The untracked folders of a scan, sorted out: small ones replaced by their files, large ones
/// kept as one entry.
#[derive(Default)]
pub(crate) struct Expansion {
    /// Folder entries (`dir/`) listed file by file instead.
    pub small: HashSet<String>,
    /// Folder entries kept as one entry.
    pub large: HashSet<String>,
    /// The files of the small folders.
    pub files: Vec<String>,
}

/// Sorts out the untracked folder entries of `statuses` (see [`Expansion`]).
pub(crate) fn expand(repo: &git2::Repository, statuses: &git2::Statuses<'_>) -> Expansion {
    let mut expansion = Expansion::default();
    let Some(workdir) = repo.workdir() else {
        return expansion;
    };
    for entry in statuses.iter() {
        let Ok(path) = entry.path() else { continue };
        if entry.status() != git2::Status::WT_NEW
            || !path.ends_with('/')
            || is_repository(workdir, path)
        {
            continue;
        }
        match walk(repo, workdir, path, Some(UNTRACKED_FOLDER_THRESHOLD)) {
            Folder::Files(files) => {
                expansion.small.insert(path.to_string());
                expansion.files.extend(files);
            }
            Folder::Large => {
                expansion.large.insert(path.to_string());
            }
        }
    }
    expansion
}
