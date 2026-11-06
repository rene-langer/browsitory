### Internal: Late cancels no longer pile up

- Internal: a cancel for an operation that already finished (a late click on Cancel) is now
  dropped instead of being kept until the repository is closed. A late cancel for a clone is
  kept only among the last few, since the clone dialog can also cancel before its clone
  starts (#219).
