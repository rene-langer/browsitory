//! Which paths a repository's watcher follows (#756): what the change stamp
//! (`git_core::change_stamp`) reads, so an event the watcher reports is one the stamp may show.

use std::path::{Component, Path, PathBuf};

/// The folders of one repository, canonicalized where possible, so they match the paths the
/// operating system reports (FSEvents reports `/private/var/...` for `/var/...`).
#[derive(Debug, Clone)]
pub(super) struct Layout {
    /// The work tree root; `None` for a bare repository.
    pub workdir: Option<PathBuf>,
    /// The repository's own git folder (`.git/worktrees/<name>` for a linked worktree).
    pub git_dir: PathBuf,
    /// The folder shared by all worktrees (`.git`); the git folder itself when there are none.
    pub common_dir: PathBuf,
}

/// Where a reported path lies.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Place<'a> {
    /// In the work tree, relative to its root; never inside a folder named `.git`.
    Work(&'a Path),
    /// In the git folder or the common folder, relative to it.
    Git(&'a Path),
    /// Elsewhere, or in a nested repository's `.git`.
    Outside,
}

impl Layout {
    pub fn of(repo: &git2::Repository) -> Self {
        Self {
            workdir: repo.workdir().map(canonical),
            git_dir: canonical(repo.path()),
            common_dir: canonical(repo.commondir()),
        }
    }

    pub fn place<'a>(&self, path: &'a Path) -> Place<'a> {
        for folder in [&self.git_dir, &self.common_dir] {
            if let Ok(relative) = path.strip_prefix(folder) {
                return Place::Git(relative);
            }
        }
        if let Some(relative) = self
            .workdir
            .as_ref()
            .and_then(|workdir| path.strip_prefix(workdir).ok())
        {
            if relative
                .components()
                .any(|part| part == Component::Normal(".git".as_ref()))
            {
                return Place::Outside;
            }
            return Place::Work(relative);
        }
        Place::Outside
    }
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.components().collect())
}

/// The parts of a relative path as strings; `None` when one isn't a plain UTF-8 name.
fn parts(relative: &Path) -> Option<Vec<&str>> {
    relative
        .components()
        .map(|part| match part {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .collect()
}

/// The git folder's files (relative to it) the change stamp reads.
const GIT_FILES: [&str; 10] = [
    "HEAD",
    "index",
    "packed-refs",
    "config",
    "config.worktree",
    "MERGE_HEAD",
    "MERGE_MSG",
    "CHERRY_PICK_HEAD",
    "REVERT_HEAD",
    "BISECT_LOG",
];

/// Whether a change at `relative` in the git folder (or the common folder) can change what the
/// app shows. Lock files never do: git renames them onto the real name when it is done, and that
/// rename is reported for the real name.
pub(super) fn is_relevant_git_path(relative: &Path) -> bool {
    let Some(parts) = parts(relative) else {
        return false;
    };
    if parts.last().is_some_and(|name| name.ends_with(".lock")) {
        return false;
    }
    match parts.as_slice() {
        ["refs", ..] | ["rebase-merge", ..] | ["rebase-apply", ..] | ["sequencer", ..] => true,
        [file] => GIT_FILES.contains(file),
        ["logs", "refs", "stash"] | ["info", "exclude"] => true,
        ["worktrees", _] => true,
        ["worktrees", _, file] => matches!(*file, "HEAD" | "locked"),
        _ => false,
    }
}

/// Whether the folder at `relative` in the git folder needs a watch of its own where watches
/// don't cover subfolders (Linux): those holding the files above, and their parents. Objects,
/// most reflogs and the hooks stay unwatched.
pub(super) fn is_watched_git_folder(relative: &Path) -> bool {
    let Some(parts) = parts(relative) else {
        return false;
    };
    match parts.as_slice() {
        [] | ["refs", ..] => true,
        ["rebase-merge"] | ["rebase-apply"] | ["sequencer"] | ["info"] => true,
        ["logs"] | ["logs", "refs"] => true,
        ["worktrees"] | ["worktrees", _] => true,
        _ => false,
    }
}

/// Whether a change at `relative` in the work tree changes which files git ignores, so the
/// watched folders must be worked out again.
pub(super) fn changes_ignore_rules(relative: &Path) -> bool {
    relative.file_name() == Some(".gitignore".as_ref())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{changes_ignore_rules, is_relevant_git_path, is_watched_git_folder, Layout, Place};

    #[test]
    fn git_files_the_stamp_reads_are_relevant() {
        for path in [
            "HEAD",
            "index",
            "packed-refs",
            "config",
            "MERGE_HEAD",
            "refs/heads/main",
            "refs/heads/feature/x",
            "refs/stash",
            "logs/refs/stash",
            "rebase-merge/msgnum",
            "rebase-apply",
            "sequencer/todo",
            "info/exclude",
            "worktrees/wt",
            "worktrees/wt/HEAD",
            "worktrees/wt/locked",
        ] {
            assert!(is_relevant_git_path(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn git_bookkeeping_is_not_relevant() {
        for path in [
            "index.lock",
            "refs/heads/main.lock",
            "HEAD.lock",
            "objects/ab/cdef",
            "objects",
            "logs/HEAD",
            "logs/refs/heads/main",
            "FETCH_HEAD",
            "ORIG_HEAD",
            "COMMIT_EDITMSG",
            "hooks/pre-commit",
            "worktrees/wt/index",
            "description",
            "",
        ] {
            assert!(!is_relevant_git_path(Path::new(path)), "{path}");
        }
    }

    #[test]
    fn only_folders_holding_relevant_files_get_watches() {
        for path in [
            "",
            "refs",
            "refs/heads/feature",
            "logs",
            "logs/refs",
            "rebase-merge",
            "info",
            "worktrees",
            "worktrees/wt",
        ] {
            assert!(is_watched_git_folder(Path::new(path)), "{path}");
        }
        for path in [
            "objects",
            "objects/ab",
            "logs/refs/heads",
            "hooks",
            "worktrees/wt/logs",
            "rebase-merge/x",
        ] {
            assert!(!is_watched_git_folder(Path::new(path)), "{path}");
        }
    }

    fn layout() -> Layout {
        Layout {
            workdir: Some(PathBuf::from("/r")),
            git_dir: PathBuf::from("/r/.git"),
            common_dir: PathBuf::from("/r/.git"),
        }
    }

    #[test]
    fn a_path_is_placed_in_the_git_folder_before_the_work_tree() {
        let layout = layout();
        assert_eq!(
            layout.place(Path::new("/r/.git/HEAD")),
            Place::Git(Path::new("HEAD"))
        );
        assert_eq!(
            layout.place(Path::new("/r/src/a.rs")),
            Place::Work(Path::new("src/a.rs"))
        );
        assert_eq!(layout.place(Path::new("/r")), Place::Work(Path::new("")));
        assert_eq!(layout.place(Path::new("/elsewhere/a")), Place::Outside);
        // A nested repository's git folder (a submodule's) isn't this repository's.
        assert_eq!(layout.place(Path::new("/r/sub/.git/HEAD")), Place::Outside);
    }

    #[test]
    fn a_linked_worktree_places_paths_in_both_git_folders() {
        let layout = Layout {
            workdir: Some(PathBuf::from("/wt")),
            git_dir: PathBuf::from("/r/.git/worktrees/wt"),
            common_dir: PathBuf::from("/r/.git"),
        };
        assert_eq!(
            layout.place(Path::new("/r/.git/worktrees/wt/HEAD")),
            Place::Git(Path::new("HEAD"))
        );
        assert_eq!(
            layout.place(Path::new("/r/.git/refs/heads/main")),
            Place::Git(Path::new("refs/heads/main"))
        );
        assert_eq!(layout.place(Path::new("/r/src")), Place::Outside);
    }

    #[test]
    fn a_gitignore_anywhere_changes_the_ignore_rules() {
        assert!(changes_ignore_rules(Path::new(".gitignore")));
        assert!(changes_ignore_rules(Path::new("sub/.gitignore")));
        assert!(!changes_ignore_rules(Path::new("src/gitignore.rs")));
    }
}
