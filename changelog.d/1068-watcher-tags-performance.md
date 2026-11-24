### Performance with large repositories

- Fixed: on Linux, saving a `.gitignore` or switching to a branch that changes many of them made
  the change watcher walk the whole work tree once per file event, keeping a CPU core busy for
  minutes in a large monorepo. It now walks once per batch of changes (#1068).
- Fixed: the Tags section rendered every tag, and "Select all" froze the app for seconds with
  thousands of tags. Only the tags in view are rendered now, as in the Stashes and Reflog
  sections, and selecting is fast at any count (#1069).
