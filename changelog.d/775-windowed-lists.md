### Long lists outside the history

The lists that can grow to thousands of rows now draw only the rows in view, like the commit
history (#685).

- Changed: file, folder and line history, the search's "List" view, the Reflog and the Stashes
  lists render only the rows on screen; screen readers still hear each row's place in the whole
  list, and the keyboard selection scrolls into view as before (#775).
- Changed: a Reflog entry now takes two lines, its message, then "old → new · name · date" with
  short IDs; the full IDs, email, time and summary are in its tooltip, and Restore sits at the
  end of the row (#775).
