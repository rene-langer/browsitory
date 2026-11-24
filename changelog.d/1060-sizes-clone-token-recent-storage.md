### File sizes, tokens in clone URLs, and what a removed repository leaves behind

- Changed: the commit file browser shows sizes like the image diff and transfer progress do,
  "512 B" and "1.0 KB" instead of "512 bytes" and "1 KB" (#1060).
- Security: the Clone dialog no longer moves an access token pasted as the URL's user name
  (`https://ghp_…@github.com/…`, `glpat-…`, Bitbucket app passwords, or a long random string)
  into the plain-text Username field. It takes the token out of the URL, shows it nowhere and
  asks for your username and the token in the Token field. An ordinary user name still moves
  (#1092).
- Changed: a repository that leaves Recent repositories, removed or dropped off the end of the
  list, loses its recent searches, recent commit messages, recent branches and Fetch
  automatically / Prune choices; one still open in a tab keeps them (#1096).
