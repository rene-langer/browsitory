### Discarding a symbolic link's hunk

- Fixed: discarding the hunk (or its lines) of a symbolic link that now points elsewhere works,
  and points the link back where it pointed; it used to fail every time and leave the link
  as it was. The file a link points to is never written (#586).
