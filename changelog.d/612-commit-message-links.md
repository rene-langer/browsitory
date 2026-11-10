### Links in commit messages

- Added: `http://` and `https://` URLs and issue references (`#123`, `owner/repo#123`) in the
  commit details' subject and body are links. A click or Enter opens them in the browser; issue
  references link to the forge of the remote the current branch tracks, else `origin`, else the
  first forge remote, and stay plain text without one (#612).
- Changed: opening an external link, in the desktop app and in the editor extension, refuses any
  URL that is not `http` or `https`, because a commit message is untrusted input (#612).
