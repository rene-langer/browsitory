### Internal: worker command policy

- Changed: each worker command's policy (the undo entry it records, whether it takes the state
  lock, is refused while that lock stays busy, has its reply held until its undo entry is
  recorded, is refused while a rebase or cherry-pick/revert sequence is stored, and aborts a
  paused operation) is stated in one place, `worker::policy::of`, which names every command, so
  a new command no longer compiles without one. A table test pins every command's policy.
  Behaviour is unchanged (#485).
