### Internal: test reliability

- Fixed: ten commit search tests no longer read one page under the 250 ms page time budget. A
  slow runner could end the page before the match. The `git-core` tests now search without a
  time budget, and the worker tests follow `next_cursor` (#182).
- Fixed: E2E specs that checked too early now wait for the real condition: the stored sequence
  file to go away (`cherry-pick-sequence`), the hunk commit to land (`hunk-staging`), the commit
  graph to show the new commit before the next menu opens (`history-rewrite`), and the header's
  "Release notes" button to render (VSCode `third-party-licences`) (#182).
