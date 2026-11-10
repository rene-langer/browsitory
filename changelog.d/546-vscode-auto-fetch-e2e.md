### Internal: VS Code e2e for Fetch automatically

- Added: a VS Code E2E spec for "Fetch automatically": with the interval at one minute, the open
  panel fetches a local bare remote's new commit in the background and the sync bar shows
  "Fetched just now" (#546).
- Changed: the VS Code E2E host runs with its own empty `BROWSITORY_CONFIG_DIR`, like the Tauri
  suite, so it no longer reads or writes the developer's real Browsitory config (#546).
