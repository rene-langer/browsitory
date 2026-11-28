### Filter drivers and links kept as files

- Changed: **Discard all…**, **Stash** and the actions on several selected files no longer fail as a
  whole when one changed file is under a filter driver (Git LFS, git-crypt) or working-tree-encoding
  Browsitory can't run. They leave such files out, act on the others and name the files left out in
  the confirmation, the stash dialog or the message after stashing, as **Stage all** already left
  them out (#1107).
- Fixed: a stash took along the other side of a staged rename without checking it for a filter
  driver, so it could store that file unfiltered (#1107).
- Fixed: with `core.symlinks` off, a link kept as a file holding its target went through Git's clean
  filters (`core.autocrlf`, `text`, `eol`) in some places and not in others. Stage, Mark resolved,
  Stash file… and Stash all now store it as Git does (Stash all also kept it as a regular file),
  and a hard reset or a rebase no longer takes such a file with only CRLF line endings for an edit
  (#802).
