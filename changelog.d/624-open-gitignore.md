### Open .gitignore and .gitattributes

- Added: "Open .gitignore" and "Open .gitattributes" in the repository tab menu and the command
  palette open the file at the work tree root in the configured editor (in VSCode, in VSCode).
  When the file is missing the item reads "Create .gitignore" / "Create .gitattributes": it
  creates an empty file at the root, never overwriting one or writing through a symbolic link,
  and opens it. Bare repositories don't offer them (#624).
