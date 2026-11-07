### Add remote

- Fixed: adding a remote under a name that already exists showed that remote twice in the
  Branches tree until git refused it, then showed git's raw error; the Add remote form now says
  `A remote named "origin" already exists.` and keeps what you typed (#408).
