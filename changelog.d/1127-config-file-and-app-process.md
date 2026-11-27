### Settings file and the desktop app process

- Fixed: Settings, recent repositories and tabs saved at the same moment no longer overwrite each
  other: at the start with several restored tabs, between two requests of a VSCode window, or
  between the desktop app and a VSCode window. A `config.toml` that is a link (for example into
  a dotfiles folder) stays a link (#1127).
- Fixed: An older Browsitory no longer deletes settings a newer one saved in the shared
  `config.toml`, and a setting value it doesn't know costs that setting, not every setting and
  the restored tabs (#1137).
- Fixed: Starting the desktop app while it runs brings its window to the front instead of
  starting a second copy on the same tabs, settings and log file (#1136).
- Fixed: On Windows, **Restart to update** waits for a running change in a repository to finish,
  so it leaves no Git lock file behind (#1131).
