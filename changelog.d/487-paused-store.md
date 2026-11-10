### Internal: paused-operation stores

- Internal: the stores for a paused rebase and a stopped cherry-pick/revert are one generic
  `StateFile` over a codec, so loading, the "ignore a bad file, log once" rule and the atomic
  write live in one place (#487).
