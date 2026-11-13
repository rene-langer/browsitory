### VSCode extension: closing the panel

- Fixed: closing the Browsitory panel while an operation was still writing to the repository
  could stop it halfway and leave `.git/index.lock` behind, so every later git command in that
  repository failed until the file was removed by hand. The panel now lets running operations
  finish (up to 5 seconds) before it stops the background process (#961).
- Internal: the VSCode E2E suite removes a git lock file a spec left behind, so the specs after it
  still run, and fails the run naming that spec (#961).
