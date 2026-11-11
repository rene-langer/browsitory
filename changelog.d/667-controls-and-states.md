### Controls, states and targets

- Changed: a pressed toggle button (diff options, Blame, Changes / All files, conflict choices,
  the search bar's toggles) now has a tinted fill and an outline, not just a text colour; Wrap
  lines on Auto has a dashed outline (#667).
- Changed: lines selected for partial staging in a diff have a bar on their right edge, apart from
  the line cursor's bar on the left, and keep their added or removed colour (#722).
- Changed: the Tags section no longer has a push form. Tick tags (or **Select all**) and a bar
  under the list pushes them; a tag's menu still pushes one tag and the command palette all of
  them (#674).
- Changed: Undo and Redo, the close buttons, the context-line stepper, toasts and the signature
  badge use the same icon set as the rest of the app instead of text symbols; the commit
  message's column ruler is a faint dotted line, no longer a second border (#674).
- Fixed: the remove button on a co-author chip, an error's dismiss button and a toast's dismiss
  button are at least 24 × 24 px (#672).
- Fixed: the command palette, keyboard shortcuts, rebase plan, open repository and clone dialogs
  have names for screen readers; the commit graph's menus are no longer inside its list; and an
  invalid search is read out politely once you stop typing, not as an alert on every keystroke
  (#669).
