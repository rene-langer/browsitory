### Marking a big file resolved

- Changed: "Mark as resolved" streams the file into the repository while it checks it for
  conflict markers, instead of reading the whole file into memory first. Resolving a conflict
  in a multi-gigabyte file no longer needs that much memory (#212).
