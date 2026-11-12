### Commit toast Push

- Fixed: the commit toast's Push, clicked right after the toast appeared, sometimes did nothing,
  and clicked right after another branch was checked out it could push that branch instead of
  saying "Not pushed: … is no longer the current branch" (#816).
