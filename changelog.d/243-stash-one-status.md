### Faster Stash all

- Changed: Stash all checks the working tree with one status pass and one read of HEAD's files,
  instead of up to three passes and two reads. On a 20,000-file repository the checks take
  about 180 ms less (#243).
