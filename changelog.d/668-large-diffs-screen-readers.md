### Diffs: large files and screen readers

- Changed: a diff with more than 1,000 lines, or of a generated file (a lockfile, minified code
  or a source map), starts collapsed behind Load diff (N lines), and a loaded diff shows 2,000
  lines at a time with Show more and Show all (#686).
- Fixed: screen readers could not tell added lines from removed ones. Each diff line is now read
  as "Added line 12", "Removed line 7" or "Unchanged line 3" before its text, and selectable lines
  are list options that say whether they are selected, with the line cursor followed as it moves
  (#668).
