### Internal: e2e test stability

- Fixed: the commit graph test for a commit on a detached HEAD wrote its file after the page had
  reloaded, so the app could read the status before the file existed and never show it; the
  test now reloads after the change on disk (#556).
