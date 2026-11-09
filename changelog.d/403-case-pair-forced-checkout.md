### Files that differ only in case

- Fixed: with `core.ignorecase` on (Linux) and two committed files whose names differ only in
  case (`A` and `a`), Reset (hard) and aborting a rebase kept only one of them in the index and
  could delete the other's file, and discarding one of them left a broken index. Hard reset,
  starting a rebase, discarding one of the pair and the other operations that overwrite files
  with it now refuse, before anything is written, and name both files; Undo writes such files
  directly and keeps both (#403).
