### Applying patch files with symbolic links

- Security: "Apply patch…" could write through a symbolic link that the same patch creates, so
  a crafted patch could install a git hook (run on the next commit) or place links outside the
  repository. A patch whose entries lie below a link it creates is now refused, as `git apply`
  refuses it ("beyond a symbolic link") (GHSA-5j9j-62g5-846m).
- Security: link entries are checked like files for names that open the `.git` folder on some
  file system (`git~1`, `.git.`, `.Git `, HFS+ ignorable characters, `\` as a separator), and a
  link can no longer point into the `.git` folder or be named `.gitmodules`, `.gitattributes`,
  `.gitignore` or `.mailmap` (GHSA-5j9j-62g5-846m).
