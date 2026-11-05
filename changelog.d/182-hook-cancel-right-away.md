### Cancel a running hook

- Fixed: Cancel on the "Running post-checkout hook…" strip (and on a commit hook's status)
  could do nothing when you clicked it right as it appeared. The button then stayed on
  "Cancelling…" until the hook ended by itself. Cancel now always reaches the hook. (#182)
