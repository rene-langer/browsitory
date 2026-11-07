### Taken names

- Fixed: creating a tag or a worktree under a name that already exists showed it twice until git
  refused it; the form now says the name is taken and keeps what you typed (#413).
- Fixed: creating or renaming a branch onto a taken name (or one that clashes as a folder, such as
  `feature/x` next to `feature`) briefly replaced the existing branch in the Branches tree; it is
  now refused at once with a clear message (#413).
- Fixed: renaming a remote to another remote's name showed that name twice until git refused it;
  it is now refused at once (#413).
- Fixed: a worktree whose folder was deleted and made again listed both entries with the same
  key, and a new worktree at a listed folder showed that folder twice while git checked it (#413).
