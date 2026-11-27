### Tabs, menus and error messages

- Fixed: A failed stage, branch delete, new worktree or other action shown at once undoes only
  its own change. A fetch that finished meanwhile no longer shows as running for good, and a
  commit selected meanwhile stays selected (#1115).
- Fixed: A notification from a tab that isn't shown starts with that tab's name, and its Pull,
  Push, Undo, Redo and Show output first show that tab, then act there. A fetch, pull or push
  that fails in a hidden tab says so in a notification. Closing a tab closes its notifications
  that have buttons, and a hidden tab's command output or operation log no longer leaves the
  window unresponsive (#1116).
- Fixed: An open commit search, file history, remote branch list and branch compare start again
  only when the history changed, not on every saved or staged file. A search that does start
  again keeps its results on screen until the new ones are in (#1118).
- Fixed: On a detached HEAD, the commit menu's rebase items, a multi-selection's Interactive
  rebase, Squash, Fixup and Drop, **Push current branch to ⟨remote⟩** and **Set as upstream for
  current branch** are disabled and say "Check out a branch first." instead of failing (#1119).
- Fixed: The current branch's **Delete** is disabled and says "Switch to another branch first."
  instead of failing (#1120).
- Fixed: A failed stage, unstage, discard, restore, ignore, conflict resolution, remote,
  credential, upstream, graph branch choice and pull request connection says what failed and
  what to do next, not only Git's own text (#1122).
- Fixed: A failed **Create tag** or **New worktree** shows in its dialog only, without the same
  error in the banner behind it; a branch the branch switcher can't create shows in the banner
  only, not also as a success notification (#1124).
- Added: The commit message you are writing is kept per repository when the app quits, the
  VSCode panel closes or the window is reloaded, until you commit or clear it (#1143).
- Changed: The syntax highlighting cache is limited by the size of the code it holds (about
  25 MB), not by its number of entries (#1163).
