### Tags

- Added: **Create tag…** in a commit's menu in the graph tags that commit (lightweight or
  annotated, optionally pushed right away), without checking it out first. The command palette
  has **Create tag at HEAD…** and **Create tag at ⟨commit⟩…** for the selected commit (#438).
- Added: **Branch from tag** in a tag badge's menu (#438).
- Added: **Delete … from remote…** for a tag, in the Tags panel, the tag badge menu and the
  command palette: picks the remote when there are several, can delete the local tag as well,
  and runs `pre-push` with the remote's credentials like a push (#439).
- Changed: the Tags panel is now the list of tags, with a name filter; creating a tag moved to
  the graph and the command palette (#438).
