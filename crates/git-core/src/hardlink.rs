//! A tracked file with other hard links, written by libgit2 (#420).
//!
//! libgit2 writes each file a checkout (or a patch apply, which checks out its result) updates
//! in place: it opens the existing name with `O_TRUNC`, so the file's other names, possibly
//! outside the repository, get the new content too. git's checkout and `git apply` remove the
//! name and create a new file, so the other names keep theirs. Before libgit2 writes, each file
//! it is about to write that has more than one name is given a name of its own
//! ([`worktree_guard::separate_hard_link`]: a copy, same content and mode, renamed over the
//! name), so libgit2's write then reaches only this name.
//!
//! For a checkout, [`on_checkout`] (or `partial_checkout::Written::record`, which installs the
//! same callbacks) learns the files libgit2 updates from its notify callback (`UPDATED`, sent
//! while it plans the checkout, for every file it will write, a forced overwrite of a modified
//! file included), and separates them when the progress callback reports the "0 of n"
//! baseline (no path), which libgit2 does once the whole checkout is planned and before it
//! writes or removes anything. A checkout refused while planning (a conflict with a local
//! change) separates nothing, as git changes nothing then. Only planned files are looked at,
//! one `lstat` each. Conflicted files libgit2 writes with markers go through a lock file renamed
//! over the name, which separates them already.
//!
//! A patch apply ([`before_apply`]) touches the patch's own paths; a stash of everything
//! ([`before_writing`]) the changed paths, which libgit2's `stash_save2` resets with a checkout
//! of its own that takes no callbacks. A fresh clone and a new worktree write into folders with
//! nothing of the user's yet and are left alone.
//!
//! Best effort: a file that can't be copied (read-only, held open on Windows, beyond a link) is
//! left as it is, and libgit2's own write then behaves as it did before. The copy is not atomic
//! with libgit2's write: a process that writes the file in between loses that write, as it
//! would to libgit2's.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use git2::Repository;

use crate::partial_checkout::Written;
use crate::worktree_guard;

/// Installs the callbacks that separate each file the checkout `builder` configures writes in
/// `repo`'s work tree before libgit2 writes it. Replaces the builder's notify and progress
/// callbacks; a checkout that is recorded for a rollback uses `Written::record` instead, which
/// does this too.
pub(crate) fn on_checkout(repo: &Repository, builder: &mut git2::build::CheckoutBuilder<'_>) {
    install(repo, builder, None);
}

/// The callbacks of [`on_checkout`], also recording into `written` (see `Written::record`):
/// each planned path as it was before (after it was separated, so a rollback doesn't take the
/// copy for a write), and each path libgit2 reports done.
pub(crate) fn install(
    repo: &Repository,
    builder: &mut git2::build::CheckoutBuilder<'_>,
    written: Option<Written>,
) {
    let workdir = repo.workdir().map(Path::to_path_buf);
    let planned: Rc<RefCell<Vec<PathBuf>>> = Rc::default();
    {
        let planned = planned.clone();
        let workdir = workdir.clone();
        let written = written.clone();
        builder.progress(move |path, _, _| {
            // Without a path: the "0 of n" baseline of a checkout (an operation may run
            // several, as a stash apply does), once planned and before writing anything.
            if path.is_none() {
                if let Some(workdir) = &workdir {
                    for rel in planned.borrow_mut().drain(..) {
                        if let Ok(true) = worktree_guard::separate_hard_link(workdir, &rel) {
                            if let Some(written) = &written {
                                written.restamp(&rel);
                            }
                        }
                    }
                }
            }
            if let (Some(path), Some(written)) = (path, &written) {
                written.push(crate::partial_checkout::path_bytes(path));
            }
        });
    }
    let Some(workdir) = workdir else {
        return;
    };
    if let Some(written) = &written {
        written.set_workdir(&workdir);
    }
    // Called while libgit2 plans the checkout, before it writes anything.
    builder.notify_on(git2::CheckoutNotificationType::UPDATED);
    builder.notify(move |_, path, _, _, _| {
        if let Some(path) = path {
            if let Some(written) = &written {
                written.plan(&workdir, path);
            }
            planned.borrow_mut().push(path.to_path_buf());
        }
        true
    });
}

/// Before `repo.apply(diff, location)`: separates each work-tree file the patch changes, once a
/// check shows the patch applies (`git apply` changes nothing when it doesn't). Nothing for a
/// patch to the index alone.
pub(crate) fn before_apply(
    repo: &Repository,
    diff: &git2::Diff<'_>,
    location: git2::ApplyLocation,
) {
    if matches!(location, git2::ApplyLocation::Index) {
        return;
    }
    let Some(workdir) = repo.workdir() else {
        return;
    };
    let linked: Vec<&Path> = diff
        .deltas()
        .filter_map(|delta| delta.old_file().path())
        .filter(|rel| worktree_guard::link_count(&workdir.join(rel)).is_ok_and(|count| count > 1))
        .collect();
    if linked.is_empty() {
        return;
    }
    let mut check = git2::ApplyOptions::new();
    check.check(true);
    if repo.apply(diff, location, Some(&mut check)).is_err() {
        return;
    }
    for rel in linked {
        let _ = worktree_guard::separate_hard_link(workdir, rel);
    }
}

/// Separates each of `paths` (relative, `/`-separated) before a libgit2 call that writes them
/// and takes no checkout options of ours (`stash_save2`'s reset).
pub(crate) fn before_writing<S: AsRef<str>>(repo: &Repository, paths: &[S]) {
    let Some(workdir) = repo.workdir() else {
        return;
    };
    for rel in paths {
        let _ = worktree_guard::separate_hard_link(workdir, Path::new(rel.as_ref()));
    }
}
