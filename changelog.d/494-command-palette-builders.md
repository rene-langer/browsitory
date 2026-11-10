### Internal: command palette builders

- Changed: `buildCommands` takes one context object instead of 14 positional parameters and is
  split into one builder per command family. Each command declares its palette group, set by its
  builder, instead of the group being derived from the id; the ids stay, so recent commands still
  match (#494).
- Changed: the palette's sidebar commands ("Go to Tags", "Delete tag …", "Drop stash …",
  "Remove worktree …", "Browse branches on origin") send reveal requests through the workspace's
  state, which the sidebar sections and the branch tree handle, instead of finding and clicking
  the sidebar's buttons by their labels (#494).
