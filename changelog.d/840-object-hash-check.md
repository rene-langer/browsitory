### Faster history reads

- Changed: reading history no longer checks each Git object's hash again on every read, as Git
  itself doesn't (it checks in `git fsck` and when objects arrive). File history, line history,
  blame, the commit graph and ahead/behind counts read up to two to three times faster in large
  repositories; a corrupted object now reads as Git would read it instead of failing (#840).
