### Internal: test reliability

- Fixed: ten commit search tests no longer read one page under the 250 ms page time budget. A
  slow runner could end the page before the match. The `git-core` tests now search without a
  time budget, and the worker tests follow `next_cursor` (#182).
- Fixed: the state lock's fork test waited a fixed 200 ms for the child to fork. It now waits
  for a byte the child writes after the fork, and fails if the child has already exec'd when
  the lock is checked (#182).
- Fixed: E2E specs that checked too early now wait for the real condition: the stored sequence
  file to go away (`cherry-pick-sequence`), the hunk commit to land (`hunk-staging`), a commit
  menu item that a graph refresh can't close before the click (`history-rewrite`), and the header's
  "Release notes" button to render (VSCode `third-party-licences`) (#182).
