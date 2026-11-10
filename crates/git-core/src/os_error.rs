//! What the operating system said about a failed file access, read from a typed error (#527).
//!
//! The worker gives a rejected request `ErrorKind::PermissionDenied` from this, so the frontend
//! can say what to check without reading the message.

use std::error::Error;
use std::io;

/// Whether the operating system refused `error`'s file access for lack of permission, anywhere
/// along its `source()` chain: an `io::Error` of kind `PermissionDenied`, or a libgit2 error of
/// class `Os` for the same reason.
///
/// libgit2 keeps no error number: it ends an `Os` error's message with the system's own text for
/// it (`strerror`, or `FormatMessage` on Windows), so that text is compared with what the
/// operating system says for the permission error numbers, never with a copy of it.
pub fn is_permission_denied(error: &(dyn Error + 'static)) -> bool {
    let mut current = Some(error);
    while let Some(error) = current {
        if let Some(io_error) = error.downcast_ref::<io::Error>() {
            if io_error.kind() == io::ErrorKind::PermissionDenied {
                return true;
            }
        } else if let Some(git_error) = error.downcast_ref::<git2::Error>() {
            if git_error.class() == git2::ErrorClass::Os && ends_with_permission_text(git_error) {
                return true;
            }
        }
        current = error.source();
    }
    false
}

/// The error numbers `io::Error` reads as `PermissionDenied`: `EACCES` and `EPERM` (13 and 1 on
/// every Unix Browsitory runs on), `ERROR_ACCESS_DENIED` on Windows.
#[cfg(unix)]
const PERMISSION_CODES: &[i32] = &[13, 1];
#[cfg(windows)]
const PERMISSION_CODES: &[i32] = &[5];
#[cfg(not(any(unix, windows)))]
const PERMISSION_CODES: &[i32] = &[];

fn ends_with_permission_text(error: &git2::Error) -> bool {
    let message = error.message().trim_end();
    PERMISSION_CODES.iter().any(|&code| {
        let text = os_text(code);
        !text.is_empty() && message.ends_with(text.as_str())
    })
}

/// The system's text for error number `code`, without the ` (os error N)` that `io::Error`
/// appends.
fn os_text(code: i32) -> String {
    let text = io::Error::from_raw_os_error(code).to_string();
    let detail = match text.rfind(" (os error ") {
        Some(end) => &text[..end],
        None => text.as_str(),
    };
    detail.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    #[error("wrapped: {0}")]
    struct Wrapped(#[from] io::Error);

    #[test]
    fn an_io_permission_error_is_one_anywhere_in_the_chain() {
        let denied = io::Error::from(io::ErrorKind::PermissionDenied);
        assert!(is_permission_denied(&Wrapped(denied)));
        assert!(!is_permission_denied(&Wrapped(io::Error::from(
            io::ErrorKind::NotFound
        ))));
    }

    #[cfg(unix)]
    #[test]
    fn a_libgit2_os_error_ending_in_the_systems_text_is_one() {
        let text = os_text(13);
        let denied = git2::Error::new(
            git2::ErrorCode::GenericError,
            git2::ErrorClass::Os,
            format!("failed to open 'a.txt': {text}"),
        );
        assert!(is_permission_denied(&denied));

        let other_class = git2::Error::new(
            git2::ErrorCode::GenericError,
            git2::ErrorClass::Index,
            format!("failed to open 'a.txt': {text}"),
        );
        assert!(!is_permission_denied(&other_class));

        let not_found = git2::Error::new(
            git2::ErrorCode::GenericError,
            git2::ErrorClass::Os,
            format!("failed to open 'a.txt': {}", os_text(2)),
        );
        assert!(!is_permission_denied(&not_found));
    }

    #[cfg(unix)]
    #[test]
    fn the_systems_text_has_no_error_number() {
        let text = os_text(13);
        assert!(!text.is_empty());
        assert!(!text.contains("os error"), "{text}");
    }
}
