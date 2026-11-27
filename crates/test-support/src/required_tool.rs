//! Tests that need a tool the workspace doesn't build (#1113): gpg, ssh-keygen, the git CLI,
//! Git for Windows' sh.exe. Such a test starts with
//!
//! ```ignore
//! if !test_support::require_tool("git") {
//!     return;
//! }
//! ```
//!
//! Locally a missing tool skips the test with a note on stderr. Under CI (`CI` set, as GitHub
//! Actions does) it fails the test instead: CI must have every tool, so a runner image that
//! drops one can't quietly turn the test into a pass.

use std::io::Write;

/// Whether `program` can be started at all (`program --help` spawns; its exit code doesn't
/// matter).
pub fn program_available(program: &str) -> bool {
    std::process::Command::new(program)
        .arg("--help")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

/// For a test that needs `program` on PATH: true when it is there. When it isn't, the test
/// returns at once; see the module docs for what happens locally and on CI.
#[track_caller]
pub fn require_tool(program: &str) -> bool {
    require(program, program_available(program))
}

/// [`require_tool`] for a requirement that isn't a program on PATH, such as Git for Windows'
/// sh.exe: `what` names it, `available` says whether it was found.
#[track_caller]
pub fn require(what: &str, available: bool) -> bool {
    require_on(what, available, std::env::var_os("CI").is_some())
}

#[track_caller]
fn require_on(what: &str, available: bool, on_ci: bool) -> bool {
    if available {
        return true;
    }
    assert!(
        !on_ci,
        "{what} is missing; CI must run this test, so the runner needs it (#1113)"
    );
    // Straight to stderr: the test harness captures `eprintln!` of a passing test, so the
    // note would never show.
    let test = std::thread::current()
        .name()
        .unwrap_or("this test")
        .to_string();
    let _ = writeln!(
        std::io::stderr(),
        "note: skipping {test}: {what} is missing (CI fails here instead)"
    );
    false
}

#[cfg(test)]
mod tests {
    use super::{program_available, require_on};

    #[test]
    fn an_available_tool_lets_the_test_run_locally_and_on_ci() {
        assert!(require_on("gpg", true, false));
        assert!(require_on("gpg", true, true));
    }

    #[test]
    fn a_missing_tool_skips_the_test_locally() {
        assert!(!require_on("gpg", false, false));
    }

    #[test]
    #[should_panic(expected = "gpg is missing; CI must run this test")]
    fn a_missing_tool_fails_the_test_on_ci() {
        require_on("gpg", false, true);
    }

    #[test]
    fn a_program_that_cannot_be_started_is_not_available() {
        assert!(!program_available("browsitory-no-such-program-1113"));
    }
}
