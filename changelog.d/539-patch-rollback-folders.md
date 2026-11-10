### Patch series rollback

- Fixed: when a later mail of a patch series doesn't apply, the folders an earlier mail created
  for a new file are removed too (deepest first, only when empty; a folder that was there before
  stays), so the working tree is left exactly as it was (#539).
