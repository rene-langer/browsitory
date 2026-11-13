### User-defined commands

- Added: a "Commands" group in Preferences to add, edit, reorder, test and delete your own
  commands, for every repository or only the active one (#893). A command is a program with
  arguments and `${repo}`, `${commit}`, `${shortCommit}`, `${branch}`, `${ref}`, `${path}` and
  `${paths}` variables; it never runs through a shell, and each value stays one argument
  whatever spaces or quotes it holds. Commands are kept in Browsitory's own settings, never in
  a file inside the repository, so a cloned repository can't add one. Unknown variables are
  refused when you save, and "Test" shows how the command splits into arguments and where its
  program was found.
