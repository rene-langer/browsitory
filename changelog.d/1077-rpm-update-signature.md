### Updates

- Fixed: the `.rpm` is signed with an OpenPGP key, so Fedora 45 and newer (rpm 6, which refuses
  unsigned packages) can update it from inside the app. Import the key once with
  `sudo rpm --import` (see the install guide) (#1077).
- Fixed: when installing an update fails, the update notice says so with the reason instead of
  doing nothing, and the error goes to the log file; **Restart to update** tries again (#1077).
