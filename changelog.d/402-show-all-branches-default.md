### Show all branches

- Fixed: **Show all branches** saved the names of today's local branches instead of going back to
  the default graph. A branch created later stayed hidden until you clicked its dot, and in a
  repository with a single local branch the graph then showed only that branch's first-parent
  line. It now goes back to the default: every local branch, also new ones, no remote-only
  commits, and every remote branch's badge. Turning the dots back on until every local branch
  is shown, or deleting a branch so that every remaining one is shown, does the same for the
  local branches (#402).
