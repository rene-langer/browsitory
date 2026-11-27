### Rebase, cherry-pick and merge history

- Fixed: a rebase (and so Edit commit…, Split commit…, Drop commit and a pull with rebase)
  replayed a commit whose message isn't UTF-8 (written under another `i18n.commitEncoding`,
  converted from SVN or CVS) with an empty message; cherry-pick, revert and Fold staged changes
  replaced its non-ASCII characters. They now keep the message in UTF-8 as git does, reading
  ISO-8859-1 and Windows-1252 by the commit's `encoding` header. A paused cherry-pick of such a
  commit no longer crashes the app's backend when its message is read (#1149).
- Fixed: a rebase and a pull with rebase kept commits that are already upstream (cherry-picked,
  rebased by a pull request) as empty commits, which then showed as to push. As `git rebase`,
  they are now left out, and a commit that becomes empty is dropped, also when you resolve its
  conflict to nothing and continue; a commit empty from the start is kept (#1150).
- Fixed: a file whose `merge` attribute names a custom merge driver (`merge=ours` with
  `merge.ours.driver`, lock file mergers) was silently merged as text. Browsitory doesn't run
  driver programs: when both sides changed such a file, a merge, pull, cherry-pick, revert,
  rebase or stash apply now leaves it conflicted, and the Changes pane names the driver (#1165).
