### Internal: test and CI infrastructure

- Fixed: local runs of both E2E suites, the VSIX smoke test and the extension's real-sidecar
  test no longer read the developer's global git config, GnuPG home or SSH agent: they run with
  a throwaway HOME, XDG dirs, `GIT_CONFIG_GLOBAL` and `GNUPGHOME`, without `SSH_AUTH_SOCK` or
  inherited `GIT_*` variables (#1112).
- Fixed: Rust tests that need `gpg`, `ssh-keygen`, `ssh-agent`, `ssh-add`, the git CLI or Git for
  Windows' `sh.exe` fail on CI when it is missing instead of passing without running; locally
  they still skip, now with a visible note (`test_support::require_tool`) (#1113).
- Fixed: `scripts/check-license-compliance.py` also checks the direct dependencies declared in
  platform-specific `[target.'cfg(...)'.*]` tables, and has unit tests (#1114).
- Fixed: two E2E runs of the same suite at once (two worktrees) no longer delete each other's
  fixtures: each run takes a lock in the temp folder before it resets them, and a second run
  stops at once, naming the first; the Tauri suite also checks its driver ports up front (#1144).
- Fixed: six E2E specs' comments no longer say the stage control is transparent until hovered,
  which it hasn't been since #648 (#1199).
