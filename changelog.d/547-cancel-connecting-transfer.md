### Cancelling a transfer that is still connecting

- Fixed: Cancel had no effect on a Fetch, Fetch all, Pull or Push that was still connecting to a
  server that doesn't answer (or a VPN that is down): the app kept waiting for the connection,
  for minutes or for good. Cancel now stops it at once, nothing is fetched or sent, and other
  actions can go ahead (#547).
