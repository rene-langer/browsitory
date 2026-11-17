### Internal: test isolation

- Fixed: the SSH signing tests failed at random on a machine with an SSH agent in its session:
  the hermetic test guard now removes `SSH_AUTH_SOCK` and `SSH_AGENT_PID`, as it already hides
  the developer's gpg agent (#805).
- Fixed: on Windows, the work-tree guard's test seam reported a link as cleared even when none
  was left to delete; it now reports only a link it removed, as on Unix (#800).
