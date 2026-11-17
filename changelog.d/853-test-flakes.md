### Internal: test flakes

- Fixed: the multi-repo E2E test read the second tab's changes marker before the status check
  for the file it had just written answered, and saw "1 file"; it now waits for "2 files" (#1038).
- Fixed: the remote-management E2E test clicked "Set upstream…" while Add remote was still
  running, when the item is disabled and the click does nothing; it now clicks the item once it
  is enabled, and so does the remote-transfer test (#862).
- Fixed: the DiffView test of the 2,000-line window stops after one step of 2,000 lines
  instead of rendering all 4,500, and looks for its buttons in the bar under the diff, so it no
  longer times out on a busy machine (#853).
