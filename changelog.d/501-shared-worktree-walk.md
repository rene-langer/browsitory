### Code clean-ups

- Internal: the work-tree walk that guards every write against links swapped in meanwhile is
  written once, over a small per-platform trait (`HeldDir`), instead of once for Unix and once
  for Windows; the shared loop is tested on every platform with a scripted folder (#501).
