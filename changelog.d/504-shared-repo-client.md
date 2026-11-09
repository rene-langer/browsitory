### Internal: one RepoClient for both transports

- Changed: the desktop and VSCode clients now share one implementation of every method they
  send alike (`createRepoClient`), and the transport parity test also compares the params each
  transport sends (#504).
- Fixed: on the desktop, a force push and the commands that run hooks (switch branch, checkout,
  merge, cherry-pick, revert, continue) now wait for the progress listeners like fetch, push and
  commit do, so a fast operation's first events are not lost (#504).
