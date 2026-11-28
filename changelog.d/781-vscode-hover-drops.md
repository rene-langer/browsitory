### VSCode: commit hovers and file drops

- Added: **Show in Browsitory** in the commit hovers of VSCode's inline blame, Timeline and Source
  Control Graph selects the commit in Browsitory's graph. VSCode shows one extension's hover
  links, so it is missing for repositories with a GitHub remote and for unpushed commits; the
  `browsitory.gitHover.enabled` setting turns it off (#781).
- Added: dropping files on the Browsitory panel in VSCode. A folder or patch file dragged from
  VSCode's Explorer with Shift held opens as a tab or is applied; a `.patch`, `.diff` or `.mbox`
  file from the file manager is applied. A folder from the file manager can't be opened, since
  VSCode doesn't pass its location, and Browsitory says so (#818).
