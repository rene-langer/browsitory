### Syntax highlighting

- Fixed: syntax highlighting no longer freezes the window. It runs in a background worker, in
  chunks, and stops working on a file you have already left; a 2,000-line TypeScript file used
  to block the UI for 1 to 5 s (#843).
