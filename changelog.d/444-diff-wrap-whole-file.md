### Diff

- Added: **Wrap lines** above each diff wraps long lines or lets them scroll sideways, keeping
  line numbers aligned and line selection working in the unified and split views. Its default
  is the new **Preferences → Diff → Word wrap** setting: On, Off or Auto (wraps the split view,
  prose files and diffs with very long lines) (#444).
- Added: **Show whole file** shows every line of a file with its changes highlighted, in the
  working tree, commits, stashes and compares. Hunk and line staging work on it; a file over
  10,000 lines keeps the normal context (#444).
- Changed: code diffs in the unified view no longer wrap by default (word wrap Auto); choose On
  in Preferences to wrap everywhere as before (#444).
