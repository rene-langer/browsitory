### Stashes

- Fixed: On macOS and Windows, saving a stash now spots a file in the way of a restored folder
  when the names differ only in case or Unicode form (such as `ß` and `ss`), the same way
  applying a stash does. Before, saving could delete that file (#337).
