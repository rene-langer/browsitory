### Changes from outside the app show sooner

- Changed: changes made outside Browsitory now show within about a second. The app asks the
  operating system to report changes to the repository's files (ignored files and git's own
  bookkeeping left out) and checks only then, plus every 30 seconds in case a change went
  unreported (some network drives report none). Where changes can't be reported (no file watches
  left on Linux, more than 20 000 folders, some network drives), it checks every few seconds as
  before (#756).
