//! A write that another Git program's lock file stopped (#1033), read from a typed error.
//!
//! git and libgit2 write the index, a ref, `packed-refs` or the config by creating
//! `<file>.lock` first and renaming it over the file; while one program holds it, every other
//! writer fails. The worker gives such a failure `ErrorKind::LockHeld` from this, and a transfer
//! `TransferErrorKind::LockHeld`, so the frontend can say what to do without reading the message.

use std::error::Error;

/// Whether another Git program's lock file stopped `error`'s write, anywhere along its
/// `source()` chain.
pub fn is_lock_held(error: &(dyn Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(error) = current {
        if error
            .downcast_ref::<git2::Error>()
            .is_some_and(names_a_lock_file)
        {
            return true;
        }
        current = error.source();
    }
    false
}

/// libgit2 answers `Locked` for a lock file that exists (`index.lock`, `<ref>.lock`,
/// `config.lock`), but also for a file the system refused to remove or replace (`EACCES`, or on
/// Windows a file another program holds open: "failed remove - '<path>' is locked") and for a
/// worktree locked with `git worktree lock`. Only the first is another Git program's lock: the
/// index says so in its own class, every other lock names the `.lock` file it found.
///
/// A fetch's ref updates (`git_remote_update_tips`) answer any failure with code -1, so there a
/// held lock keeps only libgit2's message for it (`git_filebuf`'s `lock_file`), class `Os`.
fn names_a_lock_file(error: &git2::Error) -> bool {
    match error.code() {
        git2::ErrorCode::Locked => {
            error.class() == git2::ErrorClass::Index || error.message().contains(".lock'")
        }
        git2::ErrorCode::GenericError => {
            error.class() == git2::ErrorClass::Os
                && error.message().starts_with("failed to lock file '")
        }
        _ => false,
    }
}
