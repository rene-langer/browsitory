### Internal: build and test robustness

- Fixed: the desktop commit-search e2e test failed now and then because the search query typed
  right after the page reload did not stick, so the results list never appeared; it now puts the
  query back until the matching result shows (#479).
- Fixed: the desktop rebase-abort e2e test now clicks "Abort rebase" in the page once the button
  is enabled, instead of with a native WebDriver click that sometimes left the rebase running. A
  rebase left behind by a failed test is undone afterwards, so the later rebase tests no longer
  fail on it (#479).
