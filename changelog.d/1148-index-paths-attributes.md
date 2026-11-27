### Index, paths and attributes

- Fixed: Unstaging a file whose name holds `[`, `]`, `*` or `?` (`pages/[id].tsx`, a Next.js or
  SvelteKit route) also unstaged every staged file the name matched as a pattern, so the next
  commit silently left them out. File names are now always taken as names (#1148).
- Fixed: **Ignore file**, **Ignore folder**, **Ignore all \*.ext** and a selection's **Ignore**
  wrote such a name into `.gitignore` as it is, so the line ignored other files and not the one
  picked. The characters `.gitignore` reads as a pattern (`[`, `*`, `?`, `\`, trailing spaces)
  now get a backslash (#1168).
- Fixed: **Stage all**, **Unstage all**, **Discard all…** and the actions on a selection wrote
  the whole index or scanned the whole work tree once per file: minutes in a large repository
  with many changes. They now do it once for all files (#1155).
- Fixed: A file with a `working-tree-encoding` attribute
  (`*.ps1 working-tree-encoding=UTF-16LE-BOM`) was staged as raw UTF-16 and checked out as
  UTF-8. Browsitory can't convert it, so, like a file under Git LFS or git-crypt, it now refuses
  to stage, discard or check such a file out, names the encoding in the Changes list's notice and
  marks the file (#1156).
- Fixed: A status read that was killed (a crash, `kill -9`) left a `browsitory-index-refresh-*`
  folder with a copy of the index in `.git` for good. The next status read removes such folders
  once no running process owns them (#1162).
- Fixed: A file added with `git add -N` was shown as untracked: **Delete** emptied it instead of
  deleting it, and **Stage modified** left it out. It is now shown as git does (**A**, added but
  not staged), **Stage modified** stages it as `git add -u` does, and **Discard changes…** says
  that it empties the file, as `git restore` does, which no longer stages the emptied file
  (#1173).
