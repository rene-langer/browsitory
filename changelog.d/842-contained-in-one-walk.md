### Contained in

- Fixed: the commit details' "Contained in" checked one branch or tag at a time and, in
  repositories with long history and many tags, never finished within its time budget. It now
  checks all of them in one walk over the history above the commit (#842).
