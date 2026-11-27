### VSCode extension: tabs per window, sidecar restarts and remote windows

- Changed: each VSCode window keeps its own Browsitory tabs. The desktop app and every VSCode
  window used to share one list of open tabs and overwrite each other's. The first time you open
  Browsitory in a window after this update, it restores no tabs there; **Browsitory: Open** opens
  the workspace's repositories as before, and the desktop app keeps its tabs (#1128).
- Fixed: after its helper process restarted, Browsitory reopened the tabs another window or the
  desktop app had saved last, and requests for this window's tabs failed with "repo not open"
  (#1128).
- Fixed: reloading or closing a VSCode window while Browsitory switched branches, staged many
  files or wrote the index could cut that write off halfway, leaving thousands of files shown as
  changed. The helper process now finishes the running requests first, for up to 5 seconds, and
  no longer stays behind when one doesn't end (#1129).
- Fixed: after its helper process restarted, Browsitory no longer noticed changes made outside
  it at once, only every 30 seconds, until the panel was reopened (#1130).
- Fixed: an open diff or merge editor kept showing the old index, last commit or conflict side
  after you staged, committed or resolved; it now shows the current one (#1132).
- Fixed: in remote windows (WSL, Remote - SSH, Dev Containers), **Open in Browsitory**, **Show
  File History in Browsitory** and **Show Line History in Browsitory** were missing from the
  Explorer and editor menus and the Command Palette. The guide now says that remote windows work
  and that Browsitory shows its built-in shortcut keys there (#1134).
