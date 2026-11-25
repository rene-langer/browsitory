### Large commits, long change lists and long files

- Fixed: selecting a commit or a stash that changes many files (a first commit, a vendor import,
  a mass reformat) loaded and drew every file's diff at once and could freeze the window. With
  more than 50 files, every file now starts collapsed and its diff loads when you expand it; the
  first 200 files are listed, with **Show 200 more** and **Show all** for the rest (also in a
  comparison) (#1153).
- Fixed: the Changes list in the Tree layout drew every row and slowed down with each one, so
  thousands of changed files took seconds to show and to select in. It now draws only the rows on
  screen, as the Flat layout does (#1157).
- Fixed: Blame on a long file drew every line at once and could freeze the window; it now draws
  only the lines on screen and loads less data. Blame of a file over 20 MB says how large it is
  instead of reading it, and blame of a binary file says it has none (#1158).
