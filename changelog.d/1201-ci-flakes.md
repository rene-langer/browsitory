### Settings saves and dragging

- Fixed: with the desktop app and VS Code windows saving settings at once on a slow disk, a save
  could undo another one or fail: a save waited at most 5 s for the others, however many of
  them still finished in that time. It now waits as long as they keep finishing (#1212).
- Fixed: a drag (rebase planner rows, commits, branches) that began right after the view
  changed could act on the view from before the change; the rebase planner then announced
  nothing when the drag started (#1207).

### Internal: CI and test flakes

- Internal: the mutants comment says no mutants were generated, not that mutation testing
  didn't run, when a pull request changes only test code in `src/` (#1201).
- Internal: the Linux CI jobs' apt installs read only the Ubuntu archive's sources, so a
  refusing third-party source on the runner (packages.microsoft.com) no longer fails them
  (#1211).
- Internal: the tokenizer test compares chunked and whole tokens without Shiki's per-line time
  limit, which cut the cold first line short on a loaded machine (#1210).
