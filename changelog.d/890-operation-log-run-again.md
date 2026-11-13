### Operation log

- Added: Run again on a failed fetch, pull or push in the operation log repeats it with the same
  parameters, as a new entry. It is refused, saying why, when the remote or branch it named is
  gone. Local operations (commit, checkout, merge, rebase and the rest) are not offered (#890).
