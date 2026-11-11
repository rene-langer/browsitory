### Changes from outside the app, tab switching

- Added: changes made outside Browsitory (a terminal, an editor, another Git tool) show up by
  themselves. While a tab is shown, the app checks every few seconds whether anything it displays
  changed (files, staging, commits, branches, tags, stashes, remotes, an operation in progress) and
  reloads only then; it also checks at once when the window gets the focus back or comes into
  view, and when you switch to a tab. Ignored files and git's own bookkeeping don't count, nothing
  is checked while one of the tab's own actions runs, and a slow check in a very large repository
  stretches the interval (#650).
- Changed: switching tabs keeps the view. The last four tabs you looked at stay loaded, so the
  selected commit, scroll positions, open panels, search and file history are still there when you
  switch back, and the repository is read again only if it changed meanwhile (#642).
- Changed: Left/Right, Home and End in the tab bar move the focus between tabs without switching;
  Enter or Space switches to the focused tab (#642).
