### Configuration file

- Fixed: an entry added by hand to `config.toml` as a `[[user_commands]]` section (or any other
  list kept as sections, such as `[[repo_user_commands]]`) made the whole file unreadable with
  "duplicate key", because the app had written the empty list as `user_commands = []`. Empty
  lists are no longer written (#967).
