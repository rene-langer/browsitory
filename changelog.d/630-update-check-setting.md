### Update check setting

- Added: **Preferences → Updates → Check for updates automatically** in the desktop app
  (`[updates] check_automatically` in `config.toml`, on by default). Turned off, the app makes no
  update request at start or every 6 hours. **Check for updates**, in that section and in the
  command palette, checks once whatever the setting and says whether you are up to date (#630).
