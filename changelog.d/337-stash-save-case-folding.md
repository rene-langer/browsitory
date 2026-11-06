### Stashes

- Fixed: Saving a stash now spots a file in the way of a restored folder when the names differ
  only in case or Unicode form (such as `ß` and `ss`), the same way applying a stash does. This
  applies on macOS and Windows, and on Linux when the repository sets `core.ignorecase`. Before,
  saving could delete that file (#337).
