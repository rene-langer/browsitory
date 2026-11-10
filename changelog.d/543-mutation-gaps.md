### Files that differ only in case

- Fixed: with `core.ignorecase` on (Linux), aborting a cherry-pick or revert that had brought in
  a file whose name differs only in case from an existing one (`a` next to `A`) overwrote what
  you had written into the existing file during the pause. Abort now refuses, saying it would
  discard that change, and leaves the file as it is (#543).
- Internal: tests for the paths the mutation job found untested: Abort's refusal for a case
  pair, the rewind around a case pair, and the patch module (a staged patch without commits, a
  hunk ending without a final newline, a patch creating a file that is already there, a failed
  or re-adding series, a message line starting with "From ") (#543, #544).
- Internal: finding a commit no longer lower-cases an upper-case commit id itself; libgit2
  reads hex digits in either case (#545).
