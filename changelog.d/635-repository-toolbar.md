### Repository toolbar and the sidebar in narrow windows

- Fixed: below 900 px (or in a VS Code editor group that narrow) the sidebar could not be opened
  at all. It now opens as a drawer over the graph from the toolbar's sidebar button,
  Ctrl/Cmd+Alt+B, "Show sidebar" in the palette or a double-click on the divider; Escape or a
  click beside it closes it. The breakpoint follows the workspace's width, not the window's (#635).
- Fixed: Pull on a diverged branch showed no Merge / Rebase prompt while the Branches section was
  closed, and froze the page behind an invisible prompt while the sidebar was hidden. The prompt
  now opens over the workspace, and Pull waits until it is answered (#636).
- Changed: one repository toolbar runs above the sidebar, graph and diff: the sidebar button, the
  current branch, Back/Forward, Fetch/Pull/Push with the ahead/behind counts, Stash, Undo/Redo and
  a new Refresh button. Stash moved there from the commit dock, and the sidebar's second Pull is
  gone (#637).
- Added: the toolbar shows the current branch and its upstream ("main → origin/main"). A click
  opens "Switch branch…", a filterable list with the recently used branches first, which can also
  create a new branch from the current one and switch to it (#638).
