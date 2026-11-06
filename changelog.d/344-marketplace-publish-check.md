### Internal: Marketplace publishing checks every platform package

- Internal: the release's Marketplace job publishes one platform package, waits until the
  Marketplace validated it, then publishes the rest, and fails if any target is still missing
  from the public gallery after 30 minutes. The Open VSX job runs the same check. Re-running the
  job uploads only the missing targets (`scripts/verify-marketplace-listing.py`, #344).
