### Stash and an invalid core.ignorecase

- Fixed: saving a stash with a `core.ignorecase` that isn't true or false (or a config that
  can't be read) now stops with "Can't read core.ignorecase from the Git config: …" before it
  writes anything, in every mode and for Stash file…. Before, it read the value as "not set",
  skipped its check for names that differ only in case, and could write the stash and then
  leave the files only half reset. Applying and popping a stash now stop on every system too,
  before any check. Fix the setting, then try again (#369).
