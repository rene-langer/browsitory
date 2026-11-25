### Filter drivers in the Changes list

- Added: when the repository's attributes use a filter driver Browsitory can't run (Git LFS,
  git-crypt), the Changes list says so at the top, until you dismiss it (#1087).
- Changed: a changed file under such a filter is marked "'crypt' filter" in the Changes list, its
  Stage button is off, and its diff says the difference may be only the filter's. Stage all and
  Stage modified leave it out, as they leave out conflicts (#1087).
- Fixed: a Git LFS file whose content matches the pointer git stores (same size and SHA-256) no
  longer shows as changed after a `touch` or a checkout by another tool. Files under other filters
  stay listed: Browsitory can't tell without running the filter (#1087).
