### Security

- Fixed: Clone refuses an `https://` or `http://` URL with a user name or token in it, as the
  remotes list already did. Such a URL landed in the repository's Git config, broke the remotes
  list and could leave a saved token unusable. The Clone dialog moves a user name without a
  password (Bitbucket's copy URL, `https://alice@bitbucket.org/…`) into the Username field when
  you click Clone (#1052).
- Fixed: the desktop app's window no longer holds the file dialog and opener permissions, which
  the interface never uses; the app opens dialogs, links and folders from its backend only
  (#1053).
- Fixed: errors written to the log file (desktop) or the output channel (VS Code), and the text
  Copy details copies, no longer contain the user name, password or token of a URL or the value
  of an `Authorization` header (#1056).
