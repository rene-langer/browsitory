### Fetch automatically

- Fixed: a background fetch to a server that doesn't answer (VPN down, captive portal, server
  offline) no longer delays the action you start until the connection times out; the action
  runs at once, and a remote that doesn't answer within 30 seconds is given up on for that fetch
  while the other remotes are still fetched (#462).
- Changed: a remote that couldn't be reached is fetched in the background less often, at twice
  the interval, then four times, up to once an hour, until a fetch gets through again (#462).
