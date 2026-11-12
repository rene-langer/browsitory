//! A cheap fingerprint of what Browsitory shows of a repository (#650): two equal stamps mean
//! nothing the app reads has changed in between, so the frontend reloads a tab only when the
//! stamp it last loaded with differs from the current one. It is how changes made outside the
//! app (a terminal, an editor, another Git tool) show up without a manual refresh.
//!
//! What goes in, and what stays out on purpose:
//! - the work tree as `status::status` reads it (same options, so ignored files never count),
//!   with each changed file's size and modification time, so editing a file that is already
//!   modified changes the stamp too;
//! - the index's entries (path, id, mode, flags), not the file's time stamps: `git status` in a
//!   terminal may rewrite the index only to refresh its stat cache, which shows nothing new;
//! - HEAD, every reference, the stash list, the configuration (remotes, upstreams) and the
//!   files that mark a merge, cherry-pick, revert, rebase or bisect in progress;
//! - each linked worktree's HEAD, for the worktree list.
//!
//! Left out: objects, reflogs other than the stash's, `FETCH_HEAD`, `ORIG_HEAD` and the rest of
//! the git folder's bookkeeping, which change without changing anything the app shows.
//!
//! The stamp has two parts (#685): [`ChangeStamp::refs`] covers HEAD, the references, the stash
//! list, the configuration, the operation files and the linked worktrees (what the history,
//! branch, tag and remote reads show); [`ChangeStamp::work_tree`] covers the index and the work
//! tree. A refresh whose `refs` part is unchanged can keep the history it has: staging, unstaging
//! or editing a file changes only `work_tree`.
//!
//! The stamp is only ever compared with another stamp from the same process: each part is a hash
//! of the above, not a stable format.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::UNIX_EPOCH;

/// The files of the git folder (`Repository::path`, per worktree) whose content is part of the
/// stamp: HEAD and the markers of an operation in progress.
const STATE_FILES: [&str; 12] = [
    "HEAD",
    "MERGE_HEAD",
    "MERGE_MSG",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "BISECT_LOG",
    "rebase-merge/head-name",
    "rebase-merge/msgnum",
    "rebase-merge/end",
    "rebase-apply/next",
    "rebase-apply/last",
    "sequencer/todo",
];

/// Changes to a work tree file that the stamp follows with its size and modification time.
const WORKDIR_CHANGES: git2::Status = git2::Status::WT_NEW
    .union(git2::Status::WT_MODIFIED)
    .union(git2::Status::WT_TYPECHANGE)
    .union(git2::Status::WT_RENAMED)
    .union(git2::Status::CONFLICTED);

/// The two parts of a repository's stamp; see the module documentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeStamp {
    /// HEAD, references, stash list, configuration, operation files and linked worktrees.
    pub refs: String,
    /// The index entries and the work tree as `status` reads it.
    pub work_tree: String,
}

/// A status read and the stamp from one scan of the work tree (#841).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusWithStamp {
    pub status: Vec<crate::status::StatusEntry>,
    pub stamp: ChangeStamp,
}

/// [`crate::status::status`] and [`change_stamp`] in one read, which scans the work tree once
/// and refreshes the index's stat data on the way (#841).
pub fn status_with_stamp(repo: &git2::Repository) -> Result<StatusWithStamp, git2::Error> {
    let refs = refs_stamp(repo)?;
    let (statuses, index) = crate::index_refresh::scan(repo)?;
    let workdir = repo.workdir().unwrap_or(Path::new(""));
    Ok(StatusWithStamp {
        status: crate::status::entries(&statuses),
        stamp: ChangeStamp {
            refs,
            work_tree: hash_work_tree(Some(&index), Some((&statuses, workdir))),
        },
    })
}

/// The stamp of `repo` as it is now; see the module documentation for what it covers.
pub fn change_stamp(repo: &git2::Repository) -> Result<ChangeStamp, git2::Error> {
    Ok(ChangeStamp {
        refs: refs_stamp(repo)?,
        work_tree: work_tree_stamp(repo)?,
    })
}

/// [`ChangeStamp::refs`].
fn refs_stamp(repo: &git2::Repository) -> Result<String, git2::Error> {
    let mut hasher = DefaultHasher::new();
    let git_dir = repo.path();
    let common_dir = repo.commondir();

    for name in STATE_FILES {
        read_optional(&git_dir.join(name)).hash(&mut hasher);
    }
    read_optional(&common_dir.join("config")).hash(&mut hasher);
    read_optional(&git_dir.join("config.worktree")).hash(&mut hasher);

    let mut references = Vec::new();
    for reference in repo.references()? {
        let reference = reference?;
        references.push((
            reference.name_bytes().to_vec(),
            reference.target().map(|id| id.as_bytes().to_vec()),
            reference.symbolic_target_bytes().map(<[u8]>::to_vec),
        ));
    }
    references.sort();
    references.hash(&mut hasher);

    // The stash list is the reflog of `refs/stash`: dropping an older stash leaves the ref as
    // it was.
    let stashes = repo.reflog("refs/stash")?;
    for entry in stashes.iter() {
        entry.id_new().as_bytes().hash(&mut hasher);
    }

    let names = repo.worktrees()?;
    let mut worktrees: Vec<String> = (0..names.len())
        .filter_map(|i| names.get(i).ok().flatten().map(str::to_string))
        .collect();
    worktrees.sort();
    for name in &worktrees {
        name.hash(&mut hasher);
        let folder = common_dir.join("worktrees").join(name);
        read_optional(&folder.join("HEAD")).hash(&mut hasher);
        folder.join("locked").exists().hash(&mut hasher);
    }
    Ok(format!("{:016x}", hasher.finish()))
}

/// [`ChangeStamp::work_tree`].
fn work_tree_stamp(repo: &git2::Repository) -> Result<String, git2::Error> {
    let index_path = repo.path().join("index");
    let index = if index_path.exists() {
        Some(git2::Index::open(&index_path)?)
    } else {
        None
    };
    let statuses = match repo.workdir() {
        Some(workdir) => Some((
            repo.statuses(Some(&mut crate::status::status_options()))?,
            workdir,
        )),
        None => None,
    };
    Ok(hash_work_tree(
        index.as_ref(),
        statuses
            .as_ref()
            .map(|(statuses, workdir)| (statuses, *workdir)),
    ))
}

/// The hash of the index's entries and of a status scan of the work tree at `workdir`.
fn hash_work_tree(
    index: Option<&git2::Index>,
    scan: Option<(&git2::Statuses<'_>, &Path)>,
) -> String {
    let mut hasher = DefaultHasher::new();
    for entry in index.into_iter().flat_map(git2::Index::iter) {
        entry.path.hash(&mut hasher);
        entry.id.as_bytes().hash(&mut hasher);
        entry.mode.hash(&mut hasher);
        // The conflict stage and intent-to-add live in the flags; the stat cache doesn't.
        entry.flags.hash(&mut hasher);
        entry.flags_extended.hash(&mut hasher);
    }

    if let Some((statuses, workdir)) = scan {
        for entry in statuses.iter() {
            let flags = entry.status();
            entry.path_bytes().hash(&mut hasher);
            flags.bits().hash(&mut hasher);
            if flags.intersects(WORKDIR_CHANGES) {
                if let Ok(path) = std::str::from_utf8(entry.path_bytes()) {
                    file_version(&workdir.join(path)).hash(&mut hasher);
                }
            }
        }
    }

    format!("{:016x}", hasher.finish())
}

/// A file's content, or `None` when it can't be read (most often: it doesn't exist).
fn read_optional(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

/// A work tree entry's size and modification time (nanoseconds), not following a symbolic link;
/// `None` when it is gone.
fn file_version(path: &Path) -> Option<(u64, u128)> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_nanos());
    Some((meta.len(), modified))
}
