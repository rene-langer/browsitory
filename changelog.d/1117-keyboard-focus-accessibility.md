### Keyboard, focus and accessibility

- Fixed: Undo, Redo, Close tab, Reopen closed tab, Back/Forward, the sidebar toggle, the shortcut
  sheet and the command palette's shortcut ran behind an open dialog or menu; Ctrl/Cmd+Z in a
  confirmation undid the last Git operation behind it. In the palette its own key still closes it
  (#1117).
- Fixed: Backspace, Delete, Home, End, Page Up and Page Down on their own can no longer be given to
  a shortcut, and with a modifier they stay with a text field, so typing keeps working (#1142).
- Fixed: the multi-selection's "Squash N commits…", "Fixup N commits…" and "Drop N commits…" end
  in "…", since they open the rebase planner; the Reflog's Restore confirmation says why it is
  disabled when the repository is busy (#1133).
- Fixed: with several repository tabs open, the Commit button's reason and the New branch form's
  label could come from a hidden tab; every element id is now unique (#1135).
- Fixed: in a diff, the keyboard's line cursor scrolls into view as ↑/↓ move it, so s and d never
  act on lines off screen (#1138).
- Fixed: a screen reader reads the commit you move to in the commit history, the file history and
  the search results (#1139).
- Fixed: diff line numbers, a file's line numbers, the tabs' close ×, a file row's stage and
  unstage buttons and the error banner's next step were faded below the theme's contrast; they now
  use the muted text colour at full strength (#1140).
- Fixed: deleting a tag, worktree or branch from its menu left the keyboard focus at the top of
  the window; it moves to the row now in that place, and a notification's Undo or Dismiss returns
  the focus to where it was (#1141).
