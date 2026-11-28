//! Replacing `config.toml` with the written temp file (#1219).

use std::io;
use std::time::Duration;

#[allow(dead_code)]
fn retrying(
    mut attempt: impl FnMut() -> io::Result<()>,
    _transient: impl Fn(&io::Error) -> bool,
    _wait: Duration,
) -> io::Result<()> {
    attempt()
}

#[allow(dead_code)]
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
