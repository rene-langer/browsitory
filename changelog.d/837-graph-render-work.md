### Faster commit graph in long histories

- Changed: hovering a row in the commit graph redraws only the lines it dims or undims, and
  scrolling no longer does work for every loaded commit. Loading the next page of history lays
  out only the new rows (and, when they bring a merge's side branch, a few rows before it),
  instead of every loaded row again (#837).
