### User-defined commands

- Added: the backend runs your commands (#894): without a shell, in the repository's root or the
  command's working directory, with the selected commit, branch, ref and files filled in and
  checked against the repository first. Output is kept (its last 256 KiB), a command can be
  stopped, and one that runs longer than 30 minutes is stopped. Only commands you defined can
  run, and only programs on your `PATH` or given by an absolute path.
  Each run is an entry in the operation log, with its output.
