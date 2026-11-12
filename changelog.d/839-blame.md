### Faster blame

- Changed: blame no longer uses libgit2's blame, which compared the whole folder trees of every
  commit in the history; it now follows the file's own path and compares the file only where it
  changed. On a repository with 130,000 commits, blaming a rarely changed file takes about 3 s
  instead of over a minute, close to `git blame`. Lines get the same commits as before (#839).
- Fixed: blame no longer gives every line to a commit whose unrelated folders couldn't be read
  (libgit2 then lost the file's origin), and in a file whose history had other files renamed
  or deleted next to it, lines are given to the commit `git blame` names (#839).
