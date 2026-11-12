### Stash

- Fixed: on macOS, stashing a staged rename between two encodings of the same accented name
  (`Café` with `é` as one character, then as `e` plus an accent) was refused as differing "only
  in case", though the names look identical. It is still refused, now saying the names differ
  only in how their accented letters are encoded (#404).
