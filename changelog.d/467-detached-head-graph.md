### Detached HEAD in the graph

- Fixed: the graph and commit search now include the commits of a detached HEAD that no branch
  contains (a commit made after checking out a commit or a tag, a submodule's checkout), whichever
  branches the graph shows, and the commit HEAD is on has a **HEAD** badge (#467).
- Fixed: the E2E test of the data folder move after the identifier change waits until the
  webview has written its localStorage to disk before it stops the app, and says what it found
  when it fails (#465).
