### Fetch automatically

- Added: **Preferences → Fetch → Fetch automatically** fetches all remotes in the background
  every 5, 10 or 30 minutes or every hour (off by default), with an override per repository. It
  runs quietly for the open tab, uses the repository's Prune choice, waits while the window or
  VSCode panel is hidden or another action runs, and never asks for credentials: a remote that
  would need them is skipped and named once in a notice. The sync bar shows when the repository
  was last fetched (#440).
