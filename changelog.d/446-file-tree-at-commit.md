### Files at a commit

- Added: a commit's details switch between **Changes** and **All files**. All files browses the
  commit's whole tree (folders load as you open them, a filter searches every path) and shows a
  file's content at that commit read-only, with syntax highlighting; images are shown, binary
  and large files get a notice. A file's actions open its history, compare it with the working
  tree, save a copy elsewhere, or restore it to the working tree, one undo step (#446).
