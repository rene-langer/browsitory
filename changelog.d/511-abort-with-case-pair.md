### Files that differ only in case

- Fixed: with `core.ignorecase` on (Linux), a rebase whose picked commit added a file whose name
  differs only in case from an existing one (`a` next to `A`) couldn't be aborted: Abort refused
  with the case-pair message. Aborting a cherry-pick or revert in that state refused, saying it
  would discard a change. Abort now always restores the branch, writing such files directly,
  and keeps a file you wrote there yourself (#511).
