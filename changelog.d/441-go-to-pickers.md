### Go to a commit, branch, tag or stash

- Added: **Go to commit…**, **Go to branch…**, **Go to tag…** and **Go to stash…** in the
  command palette select the target and scroll the graph to it without checking anything out.
  Go to commit takes a SHA prefix, message text or any revision; the graph loads more history
  when the commit is further down, and when the graph doesn't show it (a hidden branch, a side
  branch of an isolated branch, no branch at all) a note says why and offers to show it (#441).
- Added: clicking a tag in the Tags panel, or a branch in the Branches section, selects its
  commit in the graph the same way. Tag rows are now focusable: Enter or Space goes to the tag,
  the Menu key or Shift+F10 opens its menu (#441).
- Changed: in the command palette, a command whose whole label you typed comes first ("go to
  branch" picks **Go to branch…** over **Go to Branches**) (#441).
