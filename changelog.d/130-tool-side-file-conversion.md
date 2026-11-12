### External diff and merge tools

- Fixed: the files handed to a diff or merge tool, and the versions VSCode's diff and merge
  editors show, are now written as `git checkout` writes them: with `core.autocrlf` and the
  `text`/`eol` attributes applied and `ident` expanded. With `core.autocrlf=true` an unchanged
  line no longer shows as changed against the working tree file (#130).
- Fixed: p4merge gets git's virtual base for a conflict where both sides added the file, so it
  highlights only the real differences instead of every line (#159).
