### Stash and names that differ only in case

- Fixed: with `core.ignorecase` on, **Stash all** in a repository holding two files whose names
  differ only in case (`A` and `a`, committed on a case-sensitive drive) left one of them
  staged as deleted, and sometimes deleted its file. It now refuses before writing anything;
  **Stash file…** on other files still works (#392).
- Fixed: **Stash file…** on one of such a pair was refused as a case-only rename, or, for the
  pair's first name, stashed and broke the pair; it now gets the same clear refusal (#392).
- Fixed: on macOS (`Straße` → `Strasse`, `Café` → `CAFÉ`, composed → decomposed accents) and
  Windows (`Café` → `CAFÉ`), a rename the drive treats as the same name was refused with a
  misleading "would overwrite the ignored file" message. It is now refused as a case-only
  rename, like `README` → `readme`. **Stash file…** on one of two such names staged side by
  side no longer deletes the file until Pop or writes a stash Pop refuses (#389).
