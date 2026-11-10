### Menus, visible controls and drops

- Changed: menus are split into groups by separators, with destructive items in a group of their
  own; the commit menu goes from new refs and applying the commit to moving HEAD, history edits,
  Drop, exports and copying (#645).
- Added: menu items show the key that does the same outside the menu, such as `s` for staging the
  selected files, F2 for Rename tab… and Ctrl/Cmd+Shift+T for Reopen closed tab (#645).
- Changed: the stage and unstage buttons of changed files and the close buttons of tabs are always
  shown, dimmed until you point at them; hunk actions look like buttons, and Discard hunk is red
  (#648).
- Added: Apply patch… in the repository toolbar, and a + in the Tags panel header for Create tag
  at HEAD… (#649).
- Added: in the desktop app, drop a folder on the window to open it as a repository, or a .patch,
  .diff or .mbox file to apply it to the working tree (#649).
