### New app identifier

The desktop app's identifier changes from `com.browsitory.app` to `com.browsitory.desktop`
(an identifier ending in `.app` clashes with the extension of macOS app bundles). Updating
needs nothing from you.

- Changed: The first start after the update moves the app's folders to the new name, so the
  theme, panel sizes and other choices you made in the app stay. The log folder is now
  `~/.local/share/com.browsitory.desktop/logs` on Linux, `~/Library/Logs/com.browsitory.desktop`
  on macOS and `%LOCALAPPDATA%\com.browsitory.desktop\logs` on Windows (#433).
- Changed: macOS treats the app as a new app once, so it may ask again for access to folders
  such as Documents or to your saved credentials in the keychain. Saved credentials, recent
  repositories, workspaces and settings are not stored under the identifier and stay as they
  were (#433).
