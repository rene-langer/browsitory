### Renaming branches and remotes

- Fixed: renaming a branch in the Branches tree kept spaces around the new name, so git refused it
  with its own error; the name is now trimmed (#418).
- Fixed: pressing Enter on an unchanged branch name renamed the branch onto itself; it now just
  closes the field, like Save on an unchanged remote in Edit remote (#418).
- Fixed: the branch rename field, Add remote and Edit remote now say why a name can't be used
  (a space, `..`, `~` and so on) as you type, as New branch does, instead of git's error after
  saving (#418).
