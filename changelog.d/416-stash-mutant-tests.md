### Internal: file/folder conflict tests

- Internal: tests for the two stash paths the mutation job found untested in #600: a stash
  apply with staged changes refused before the file/folder path, and which checkout errors count
  as a file in the way (#416, #608).
