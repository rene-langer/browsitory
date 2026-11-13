### Operation log

- Internal: the backend keeps a log of the operations it ran in each repository: name, target,
  start, duration, outcome, error, and the hooks and signing programs each ran with their
  output. Memory only, the last 500 per repository, until the app quits; URLs lose their user
  information and `Authorization` headers their value (#887).
- Internal: `list_operations` and the `operation-log` event (`operationLog` in VS Code) expose it
  to the frontend, in both apps (#888).
