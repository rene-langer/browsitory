### Remotes with a user name in their URL

- Fixed: a repository with a remote whose `https://` URL holds a user name or token (cloned with
  Git or another program, such as Bitbucket's `https://alice@bitbucket.org/…`) showed no remotes
  at all. Such a remote is now listed with `***` in place of the user name and password, and
  fetch, pull and push use the saved URL. Edit remote keeps the saved URL while you leave it as
  shown, and Manage credentials saves the token for it (#1081).
