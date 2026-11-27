//! Tests that need a tool the workspace doesn't build (#1113): gpg, ssh-keygen, the git CLI,
//! Git for Windows' sh.exe.

/// Whether `program` can be started at all (`program --help` spawns).
pub fn program_available(program: &str) -> bool {
    let _ = program;
    todo!()
}

/// For a test that needs `program` on PATH: true when it is there.
pub fn require_tool(program: &str) -> bool {
    let _ = program;
    todo!()
}

/// [`require_tool`] for a requirement that isn't a program on PATH, such as Git for Windows'
/// sh.exe: `what` names it, `available` says whether it was found.
pub fn require(what: &str, available: bool) -> bool {
    let _ = (what, available);
    todo!()
}

fn require_on(what: &str, available: bool, on_ci: bool) -> bool {
    let _ = (what, available, on_ci);
    todo!()
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
