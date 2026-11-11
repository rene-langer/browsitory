### Keyboard shortcuts for the core loop

- Added: global shortcuts for Refresh (F5), Fetch (Shift+F5), Pull (Ctrl+Shift+L), Push
  (Ctrl+Shift+K), Stage all (Ctrl+Shift+A) and the commit message (Alt+C), with ⌘ for Ctrl on
  macOS. They run the command palette's commands, so they follow the same rules, and a
  notification says why one can't run. The palette shows each command's shortcut beside it, and
  the toolbar's tooltips name theirs (#646).
- Added: Ctrl+Tab and Ctrl+Shift+Tab switch to the next and previous tab, Ctrl+1 … Ctrl+8 to the
  first eight tabs and Ctrl+9 to the last (#646).
- Added: F6 and Shift+F6 move the focus between the toolbar, the sidebar, the history and the
  diff, back to where it last was in each; the sidebar, history and diff are labelled regions
  for screen readers (#670).
- Changed: shortcuts are written for the platform in one notation: Ctrl+Shift+Z on Windows and
  Linux, ⇧⌘Z on macOS, arrows as ← ↑ → ↓ (#683).
- Internal: the global shortcuts are defined once in `frontend/src/lib/keybindings.ts`, for the
  shortcut list, the palette, the tooltips and the VS Code keybindings to come (#679).
