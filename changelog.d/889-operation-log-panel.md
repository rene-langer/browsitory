### Operation log

- Added: an operation log panel lists what the app ran in the repository, newest first: name,
  target, time, duration and result. Selecting an operation shows its parameters, its error and
  the output of the hooks and signing programs it ran, which can be copied. Filters show failed
  operations only or hide automatic fetches; the panel follows the log while open. Open it from
  the palette ("Show operation log"), the Help menu ("Operation log") or an error's "Show
  details", which selects the newest failed operation. Works in the VS Code extension too (#889).
