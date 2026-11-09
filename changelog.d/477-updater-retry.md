### Updates

- Fixed: when an update check or download failed (for example while the release download host
  answered with an error), the app waited 6 hours before checking again. It now retries after
  10 minutes, doubling the wait up to 3 hours, and goes back to the 6-hour interval once a check
  succeeds (#477).
- Added: a second update endpoint. The app reads the update manifest from the user guide site
  when the release download URL fails; the docs workflow publishes there a checked copy of the
  latest published release's manifest (#477).
