# Troubleshooting

An error stays at the top of the window until you dismiss it or start another change (stage,
commit, checkout, …); refreshing doesn't clear it.

Browsitory writes a rotated failure log (keeping the 5 most recent files) to the OS log
directory, so a bug report doesn't require a live dev session to reproduce. Attach the
newest file there to a bug report:

- **Linux:** `~/.local/share/com.browsitory.browsitory/logs`
- **macOS:** `~/Library/Logs/com.browsitory.Browsitory`
- **Windows:** `%APPDATA%\com.browsitory.Browsitory\logs`

Report problems on [GitHub](https://github.com/rene-langer/browsitory/issues).
