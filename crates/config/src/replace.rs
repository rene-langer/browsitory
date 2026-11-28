//! Replacing `config.toml` with the written temp file (#1219). Getters read without the write
//! lock, so another window can have the file open while a save replaces it. Linux and macOS
//! replace an open file. On Windows `MoveFileExW` refuses ("Access is denied"); std's
//! `fs::rename` then renames with POSIX semantics (`FileRenameInfoEx`), which replaces a file
//! that is open with `FILE_SHARE_DELETE`, as std opens every file it reads. A handle opened
//! without that share mode (another program, a virus scanner) or a file system without POSIX
//! renames still refuses: such a replace is tried again until the wait runs out.

use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

/// How long a refused replace is retried. A reader keeps the file open for a millisecond or
/// two; a virus scanner for longer.
const WAIT: Duration = Duration::from_secs(2);

/// Renames `from` onto `to`, replacing it.
pub(crate) fn replace(from: &Path, to: &Path) -> io::Result<()> {
    retrying(|| std::fs::rename(from, to), is_sharing_conflict, WAIT)
}

fn retrying(
    mut attempt: impl FnMut() -> io::Result<()>,
    transient: impl Fn(&io::Error) -> bool,
    wait: Duration,
) -> io::Result<()> {
    let deadline = Instant::now() + wait;
    let mut pause = Duration::from_millis(1);
    loop {
        match attempt() {
            Err(error) if transient(&error) && Instant::now() < deadline => {
                std::thread::sleep(pause.min(deadline.saturating_duration_since(Instant::now())));
                pause = (pause * 2).min(Duration::from_millis(50));
            }
            result => return result,
        }
    }
}

/// ERROR_ACCESS_DENIED (5): the destination is open; ERROR_SHARING_VIOLATION (32): it is open
/// without sharing.
#[cfg(windows)]
fn is_sharing_conflict(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(5 | 32))
}

/// Elsewhere a rename replaces an open file.
#[cfg(not(windows))]
fn is_sharing_conflict(_error: &io::Error) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn refused() -> io::Error {
        io::Error::from(io::ErrorKind::PermissionDenied)
    }

    /// Another program has the file open for a moment: the replace goes through once it closes.
    #[test]
    fn a_replace_refused_for_a_moment_is_retried_until_it_goes_through() {
        let mut attempts = 0;
        let result = retrying(
            || {
                attempts += 1;
                if attempts < 3 {
                    Err(refused())
                } else {
                    Ok(())
                }
            },
            |_| true,
            Duration::from_secs(10),
        );

        assert!(result.is_ok(), "gave up: {result:?}");
        assert_eq!(attempts, 3);
    }

    /// A file that stays open is waited for only so long: the save then fails with the refusal.
    #[test]
    fn a_replace_that_stays_refused_fails_after_the_wait() {
        let wait = Duration::from_millis(300);
        let mut attempts = 0;
        let started = Instant::now();
        let result = retrying(
            || {
                attempts += 1;
                Err(refused())
            },
            |_| true,
            wait,
        );
        let waited = started.elapsed();

        assert_eq!(
            result.map_err(|error| error.kind()),
            Err(io::ErrorKind::PermissionDenied)
        );
        assert!(attempts > 1, "tried only once");
        assert!(
            waited >= wait,
            "gave up after {waited:?}, before the wait of {wait:?}"
        );
    }

    /// Any other failure (a missing folder, a full disk) is reported at once.
    #[test]
    fn a_replace_that_fails_otherwise_is_not_retried() {
        let mut attempts = 0;
        let result = retrying(
            || {
                attempts += 1;
                Err(io::Error::from(io::ErrorKind::NotFound))
            },
            |_| false,
            Duration::from_secs(10),
        );

        assert_eq!(
            result.map_err(|error| error.kind()),
            Err(io::ErrorKind::NotFound)
        );
        assert_eq!(attempts, 1);
    }

    /// `MoveFileExW`/`FileRenameInfoEx` onto a file another handle keeps open refuse with
    /// ERROR_ACCESS_DENIED (5), or ERROR_SHARING_VIOLATION (32) while one has it open without
    /// sharing; a missing file (ERROR_FILE_NOT_FOUND, 2) is no such conflict.
    #[cfg(windows)]
    #[test]
    fn access_denied_and_sharing_violations_are_sharing_conflicts() {
        assert!(is_sharing_conflict(&io::Error::from_raw_os_error(5)));
        assert!(is_sharing_conflict(&io::Error::from_raw_os_error(32)));
        assert!(!is_sharing_conflict(&io::Error::from_raw_os_error(2)));
    }
}
