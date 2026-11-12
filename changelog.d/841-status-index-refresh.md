### Faster status reads

- Changed: a status read writes refreshed file times back to the index, as `git status` does, so
  files whose times changed but whose content didn't (a `touch`, a formatter, a copied folder)
  are read once instead of on every refresh. It skips the write when another program holds the
  index lock or changed the index meanwhile (#841).
- Changed: a refresh reads the status and the change stamp in one scan of the work tree instead
  of two, and focusing the window no longer checks for changes while the repository is watched
  (#841).
