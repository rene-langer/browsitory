### Desktop app: closing the window

- Fixed: closing the window or quitting the app while an operation was still writing to the
  repository could stop it halfway and leave `.git/index.lock` behind, so every later git command
  in that repository failed until the file was removed by hand. The app now lets running
  operations finish (up to 5 seconds) before it exits, and starts no new one (#965).
