### One graph selection

- Changed: the commit graph's branch selection is one `GraphSelection` (`{ local, remote }`)
  passed as a single `selection` argument to `getCommitGraph`, `locateCommit` and
  `searchCommits`, and named `local` / `remote` everywhere (the saved-selection setters,
  `AppState.graphSelection`, `useCommitSearch`), so the two lists can no longer be swapped or
  `null` and `[]` mixed up without the types noticing (#498). The commit search now also re-runs
  when the remote selection changes.
