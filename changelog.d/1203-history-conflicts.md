### Messages in other encodings, files that aren't merged as text, aborting a sequence

- Fixed: a commit whose message is stored in another encoding than UTF-8 (`i18n.commitEncoding`,
  a repository converted from SVN or CVS) showed an empty subject in the graph, the rebase
  planner and the reflog, and a garbled one in the commit header, a squash merge's message,
  Export as patch and a file/folder conflict's file name; each now reads it as git does. The
  reflog no longer fails to load when git wrote such a subject into it, and the stash list no
  longer comes up empty after a stash on such a commit (#1203).
- Fixed: a conflicted file marked `-merge` or `merge=binary` (or `binary`, as lock files often
  are), or under a custom merge driver, held both sides merged as text, so it could look merged
  and Mark resolved staged that text merge. Like git, it now holds your side, and the conflict
  view still shows both (#1204).
- Fixed: a failed **Abort cherry-pick** or **Abort revert** showed only Git's text; the banner now
  names the operation and the next step, as for Abort merge and Abort rebase (#1205).
