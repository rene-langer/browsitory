### Stalled fetch, pull and push

- Fixed: Cancel did nothing on a fetch or pull whose connection went silent after connecting
  (the Wi-Fi changed, a VPN dropped, the laptop slept), and actions that change the repository
  waited for it until the app restarted. Cancel now stops it within about two seconds, without
  updating any remote branch (#1064).
- Fixed: a fetch, pull, push or clone that receives nothing for two minutes now fails instead of
  waiting forever, also without Cancel (not for HTTPS remotes on Windows) (#1064).
