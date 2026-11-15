### Opening repositories

- Fixed: a new repository with no commits opened with a false "git remote operation failed:
  reference 'refs/heads/main' not found" error, and the toolbar said "no branch". It now opens
  cleanly, and the branch button reads "main (no commits yet)" (#983).
- Added: opening a folder that isn't a Git repository says so and lists the repositories in its
  direct subfolders, each with **Open**; hidden folders and `node_modules` are left out (#973).
- Changed: a recent repository whose folder is gone no longer opens from the start page;
  **Locate…** in place of the pin picks the folder's new place and replaces the entry. Opened
  another way, the error asks to remove it from the recent repositories, without a Retry that
  couldn't help (#998).
