### Changes list

- Changed: an untracked folder with more than 500 files (a `node_modules` or build output not yet
  in `.gitignore`) is listed as one row, "untracked folder, more than 500 files", instead of file
  by file; smaller folders are listed as before. Its chevron lists its files on demand; selected,
  it offers Stage folder, Ignore folder and Delete folder…, and Stage all and Delete untracked…
  include it (#845).
- Changed: the flat Changes and Staged lists render only the rows in view, so thousands of
  changed files no longer slow the window down (#845).
- Fixed: during a conflict, "Delete untracked…" no longer wraps onto a line of its own under the
  Changes heading; the heading's buttons move under the title together when the pane is narrow
  (#832).
