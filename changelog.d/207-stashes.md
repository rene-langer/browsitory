### Stashes

- Changed: "Restore staged changes" is no longer a checkbox above the stash list that silently
  changed what Apply and Pop do. Apply and Pop bring staged work back as staged; a stash that
  holds staged changes also offers "Apply as unstaged" and "Pop as unstaged", in its menu, in
  the stash view and in the command palette (#657).
- Added: each stash shows its branch, age and file count ("On main · 2 days ago · 3 files"),
  and its name without git's "On main:" prefix; the full message and date are in its tooltip
  (#657).
- Fixed: a stash holding a file that replaced a folder, or files in a folder that replaced a
  file, was refused by Apply and Pop. Browsitory now applies it in git's order, removing the
  folder or file first; a folder holding anything the stash doesn't know is still refused
  (#229). `git stash pop` itself can't apply the second kind.
- Fixed: "Stash file…" on a folder that replaced a file was refused although git accepts it.
  It now stashes the file's deletion with the folder's untracked files, and Pop brings both
  back; git's own stash leaves the deletion out. A folder that also holds ignored files, a
  repository or untracked files left out is still refused, where git stops half-way (#207).
