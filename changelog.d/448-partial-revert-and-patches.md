### Patches and partial reverts

- Added: a commit's file menu has **Revert this file**, **Apply this file to the working tree**
  and **Save file as patch…**; each hunk of a commit's diff has **Revert hunk** and **Apply
  hunk**. The result lands as unstaged changes, never as a commit; a change that no longer
  applies leaves the working tree unchanged, like a failed `git apply`, and every revert or
  apply can be undone (#448).
- Added: **Save as patch…** for a commit, for several selected commits (`git format-patch`
  mails, oldest first) and for the staged changes, and **Apply patch…** in the command palette,
  which applies a patch file to the working tree, all of it or nothing (#448).
