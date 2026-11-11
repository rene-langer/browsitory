### VSCode extension: open the workspace's repository, menus, keyboard shortcuts

- Changed: "Browsitory: Open" opens the repository of the file in the active editor, or else, when
  the panel opens, the workspace folders' repositories, instead of always showing the picker
  (#677).
- Added: "Open in Browsitory" in the Explorer's folder menu and in a Git repository's menu in the
  Source Control view; "Show File History in Browsitory" in the Explorer's file menu, an editor
  tab's menu and the editor's menu; "Show Line History in Browsitory" for the lines selected in an
  editor (#678).
- Changed: the Browsitory panel has its icon, opens in the active editor group and comes back
  after a window reload (#678).
- Fixed: Ctrl/Cmd+K, F5, Shift+F5, F6, Shift+F6, Ctrl/Cmd+Alt+B, Ctrl+Tab, Ctrl/Cmd+1 … 9,
  Ctrl/Cmd+W, Ctrl/Cmd+Shift+T and Back/Forward ran VSCode's commands instead of Browsitory's. The
  extension now binds them while Browsitory has the focus; change them in VSCode's Keyboard
  Shortcuts (#679).
