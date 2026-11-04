### Release notes in the app

- Fixed: the release notes in the app left out every changelog section with a topic heading,
  such as "Commit signing" or "Undo and redo", so most of 0.3.0 was missing. Each topic now
  shows as its own group, and a "Fixed", "Added" or "Changed" label marks each note. Notes
  for developers only are left out (#335).
