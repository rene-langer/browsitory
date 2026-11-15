### Blame

- Fixed: the blame view keeps each line's indentation, and its columns no longer run together:
  the short SHA, author and the new date column have padded, fixed widths (#986).
- Changed: blame shows the short SHA, author and date once for each run of lines from the same
  commit, and only the SHA is a link to that commit; a click elsewhere on a line does nothing
  (#986).
- Added: blame colors the code like the diff, with the same Syntax highlighting setting (#986).
- Changed: **Back to diff** sits in the file's header, in place of **Blame**, instead of below
  the blame (#986).
