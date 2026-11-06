### Internal: Late cancels no longer pile up

- Internal: a cancel for an operation that already finished (a late click on Cancel) is now
  dropped instead of being kept until the repository is closed. A cancel for an operation that
  is still queued still stops it once it starts (#219).
