### Error messages

- Fixed: when file permissions block reading a file's changes, the error now says to check that
  you can read and write the file or folder (#527).
- Fixed: saving or forgetting a forge token while the operating-system credential store is
  locked shows the same "unlock it and try again" advice as a remote's HTTPS token (#527).
- Internal: the worker decides each error's kind where the error is made: replies carry
  `ErrorDto`, typed errors get their kind from their type, every stopped-worker reply comes
  from one helper, and nothing reads a kind from the message any more (#527).
