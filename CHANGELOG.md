# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Stash: edits removed from the index

- Fixed: Stash all lost your edits to files in a folder you had removed from the index
  (`git rm -r --cached d`, without ignoring it). Without Include untracked files Pop deleted
  them, with Keep staged changes they were deleted at once, and with Include untracked files
  Pop wrote conflict markers and said it would overwrite local changes (also for a single file
  removed this way). The edits are now stashed and Pop brings them back; with Restore staged
  changes the removal from the index comes back too (#247).

### Stash: things in the way

- Fixed: Stash all deleted an ignored file or folder that was in the way of a file or folder it
  put back, with or without Include untracked files. It now says "Stashing would delete the
  ignored 'd', which is in the way of the tracked 'd/f'. Move 'd' first." and stashes nothing
  (#208).
- Fixed: Stash all could delete a repository nested where the branch has a file of that name,
  including its history. It now refuses with "Stashing would delete the repository in 'd',
  which is in the way of the tracked 'd'. Move 'd' first." (#211).
- Fixed: Stash all with Include untracked files deleted a repository inside an untracked folder
  (for example `vendor/lib` in a new `vendor/` folder), including its history. It now keeps the
  repository and stashes the folder's other untracked files, as `git stash -u` does (#211).
- Fixed: Stash all with Include untracked files deleted ignored files inside an untracked
  folder (for example `vendor/x.o`); it now keeps them, as `git stash -u` does (#208).
- Fixed: Stash all overwrote an edited file that had been removed from the index and then
  ignored (for example `.env` after `git rm --cached .env`, or a whole `build/` folder after
  `git rm -r --cached build`), and Pop deleted it or was refused. It now says "Stashing would
  overwrite the ignored '.env' with its committed version. Commit the removal of '.env' first,
  or move it." and stashes nothing (#208).
- Fixed: applying or popping a stash whose untracked files sit in a folder that replaced a
  tracked file reported success, left a "d~Updated upstream" file, and Pop dropped the stash.
  It now says "This stash can't be applied here: its untracked file 'd/x' needs a folder where
  'd' is tracked. Commit the deletion of 'd' first, then apply it; the stash is kept." (#206).

### Worktree add

- Creating a worktree in a folder that doesn't exist yet now creates the missing folders, as git
  worktree add does (#200).
- Fixed: a worktree add that failed could leave a half-made worktree entry and the new branch
  behind, so trying again with the same name failed with "git operation failed". When an add
  fails before the worktree is set up, Browsitory now removes the folders, entry and branch it
  created, and a folder it can't create is named in the error. A failure later, while the files
  are checked out, still leaves the worktree and branch; delete the folder, prune, and delete
  the branch (#200).

### Repository identity

- Fixed: a repository opened through two different paths (a trailing slash, a subfolder, a
  symbolic link, or different letter case on Windows or macOS), for example from a worktree's
  Open button, got two background workers, so actions in the two tabs could run at the same
  time. They now share one worker, and closing one tab keeps the other working. The two tabs
  also share the undo history, and a new search in one stops a search still running in the
  other (#223).

### Read-only files

- Fixed: resolving a conflict or adding a pattern to .gitignore failed with "Access is denied"
  (Windows) or "Permission denied" (macOS, Linux) when the file was read-only, and Use ours /
  Use theirs (whole file) replaced the read-only file. They now say "'f.txt' is read-only. Make
  it writable, then try again." and change nothing (#209).

### Cherry-pick leftovers

- Fixed: when git's cherry-pick state couldn't be cleared after a cherry-pick was committed,
  your next commit was authored as the picked commit's author. Browsitory now recognises the
  leftover, clears it and commits as you; if it can't be cleared, the commit is refused with
  "This cherry-pick was already committed, but git's cherry-pick state can't be cleared. Check
  the permissions of the .git folder, then commit again." (#226).

### Windows walker

- Fixed (Windows): undo, redo and discards no longer fail with "Access is denied" on read-only
  files in the working tree (#194).
- Windows: every file and folder of a working-tree read, write or delete is opened through the
  folder Browsitory already holds, so on a local drive a folder swapped for a junction or
  symbolic link at that moment can't make Browsitory open a file in the link's target, briefly
  create one there, or contact a network share it points to (#193).

### Security follow-ups

- Fixed: Mark resolved stages exactly the content it checked for conflict markers, read without
  following a symbolic link or waiting on a named pipe; a file swapped at that moment can no
  longer hang it or get other content staged (#199).
- Fixed: Open in editor (no editor configured) no longer refuses a document such as notes.txt or
  README.md that quotes a "[Desktop Entry]" launcher near its start. On Linux, files whose
  extension marks them as a document are not checked for launcher contents, as the Linux
  desktops do; macOS and Windows no longer check file contents for Linux launchers (.desktop
  and .kdelnk files are still refused by name everywhere) (#201).
- Fixed: while the other window keeps the undo history busy, a quick run of actions (for
  example staging files one by one) waits about a second once, not a second per action. Undo,
  Redo, discards and Reset hard still wait about a second each before they refuse (#197).
- Mark resolved keeps the mode git would keep for a file deleted on the current branch, and says
  "'x' can't be read safely; check its permissions and try again." for a file it can't open
  instead of calling it a FIFO, socket or device (#199).

### Test robustness

- Fixed (tests): the Unix tests for stopping a hook or a signing program now check that a
  background process it started can't finish its work after a timeout or Cancel; control tests
  (also for the Windows job object) show the checks fail without the process group or job
  (#179).
- Fixed (tests): the /dev/tty check now runs under a real terminal, so it also proves something
  on CI (#179).
- Fixed (tests): flaky desktop E2E tests: a merge test that could start the next test before the
  app's merge commit landed, and stale-element failures when a row disappears during a lookup
  (#182).
- E2E harness: tauri-driver, WebKitWebDriver and the app run in one process group that is
  stopped as a whole after each session; leftovers of an aborted earlier session are reaped,
  a driver port held by another process fails with a clear message, and each session logs how
  long it took to start (#182).
- CI: the desktop E2E suite runs with a D-Bus session bus, so each WebDriver session starts in
  under a second instead of ~31 seconds (#182).

### Folder/file swaps

- Fixed: unstaging a file whose folder a merge left conflicts in failed with "invalid entry
  mode" and could unstage other files below it; it now says "Can't unstage 'd': the merge left
  conflicts in it. Resolve them first." and changes nothing (#187).
- Stashing a file that replaced a tracked folder now also stashes the folder's deletion and puts
  the folder back, as git stash does, instead of refusing with "'d' is now a file" (#186).
- Fixed: applying or popping a stash that holds a file which replaced a folder reported success
  but left the file as "d~Stashed changes", and Pop dropped the stash. It now says "This stash
  can't be applied here because a file replaced a folder; use git stash apply from a terminal."
  and keeps the stash (#186).
- Stash… with Include untracked files off refuses when an untracked file is in the way of a
  stashed folder or file, instead of deleting it, also when the names differ only in case on a
  case-insensitive drive (#186).
- Applying or popping a stash whose untracked file is now a folder says "The untracked file 'd'
  in this stash would overwrite the folder 'd'; move the folder first." and keeps the stash
  (#186).

### Worker calls

- Fixed (desktop app): while a hook ran for a long time, repeated refreshes could use up the
  app's background threads, and actions in every other open repository then hung until the hook
  ended. Actions waiting for one repository now share one background thread (#181).

### Hooks: status and parity

- While post-checkout runs after you switch branches or check out a commit, or post-merge after
  a fast-forward or squash merge, "Running post-checkout hook…" shows at the top of the
  repository with Cancel. Cancelling stops the hook and keeps the switch or merge (#185).
- A pull that fast-forwards shows its post-merge hook in the transfer panel, with Cancel (#185).
- Cherry-picks and reverts that apply without conflicts now run prepare-commit-msg and
  post-commit for each commit they create, as git does, including the steps a Continue applies
  after the paused one. If prepare-commit-msg fails, that commit stays staged and paused, so you
  can fix the hook and continue or commit it. Their hooks show in the same status strip, and
  Cancel during a step's post-commit keeps that commit and stops the cherry-pick or revert after
  it ("Stopped after …; its commit was kept. Continue to resume.") (#183).
- Fixed: after a cherry-pick or revert that changed the repository but then stopped (a failing
  hook, Cancel, an error on a later step), the history, file list and operation strip now
  refresh instead of showing the state from before (#183).
- When you commit a merge that had no conflicts, prepare-commit-msg and commit-msg get
  .git/MERGE_MSG, as with git merge (#183).
- Fixed: if a merge was aborted and restarted in a terminal while pre-merge-commit ran, the
  commit used the old merge's parents; it now stops with "The merge changed while its hooks
  ran." (#183).
- Fixed: a commit (also a cherry-pick or revert step) that was created but whose merge or
  cherry-pick state couldn't be cleared was reported as failed and skipped its post-commit or
  post-merge hook; it now counts as done, and a multi-commit cherry-pick or revert moves on past
  it (#183, #216). The next commit no longer turns such a leftover merge into a second merge
  commit: it clears the leftover state first, or says it can't (#183).

### Lock and security follow-ups

- Fixed: with the desktop app and the VSCode extension open on one repository, Undo, Redo,
  discards and Reset hard could briefly refuse with "Another Browsitory window is changing this
  repository's undo history" just after the other window finished, and other actions could run
  without an undo entry, when the other window was starting a hook or git at that moment (#175).
- After the other window kept the undo history busy for the full 10 seconds once, later actions
  wait up to a second for it instead of giving up at once (#175).
- Fixed: a merge, cherry-pick or revert committed or aborted outside Browsitory while the other
  window was busy could still be listed as an undo step (#175).
- Saving a stash and adding a worktree wait for the other window's undo-history lock, so they
  can't slip past a rebase or cherry-pick sequence it is starting (#174).
- Adding a worktree on a drive or folder that ignores case (a USB stick, a casefold folder on
  Linux) now refuses a symbolic link and a folder whose names differ only in case, even when the
  repository itself is on a case-sensitive drive (#162).
- A symbolic link and a folder whose names are equal apart from how an accent is encoded (é as
  one or two code points), or apart from ẞ/ß/ss, are now recognised as the same name, as on
  macOS (#166).
- Open in editor (no editor configured) also refuses .kdelnk files, files that the desktop
  recognises as launchers by their first bytes ("[Desktop Entry]" within the first 33 bytes, or
  a start of "[Desktop Action", "[KDE Desktop Entry]", "# Config File" or "# KDE Config File"),
  and settings files with a [Desktop Entry] group after other groups, as the desktop may run
  them. A file replaced by a named pipe at that moment no longer hangs it, and Mark resolved
  refuses a named pipe ("'<file>' is a FIFO, socket or device, not a regular file; replace it
  before marking it resolved.") instead of hanging (#165).

### Windows hardening

- Fixed (Windows): a hook that starts a program right away could leave it running after Cancel or the time limit; hooks now start only once they are in their job (#180).
- Fixed (Windows): undo, redo and discards no longer fail when another program has the file open with delete sharing, as Microsoft Defender and the search indexer do. This needs NTFS or ReFS; FAT/exFAT drives and network shares without POSIX deletes behave as before (#168).
- Windows: a missing folder is created through the folder Browsitory already holds, so a folder swapped for a junction at that moment can't get an empty folder created outside the repository (#163). Tests cover folders turned into junctions during a write (#164).

### Test hygiene

- Fixed (tests): the gpg signing tests left a gpg-agent and a /tmp/bgpg* directory behind on
  every run, which over time used up the per-user inotify limit; they now stop the agent and
  delete the directory, also when a test fails. CI fails if a test run leaks one (#188).

### UX polish

- Errors now stay on screen until you dismiss them or start another change (stage, commit, checkout, …); refreshing no longer clears them. Same in the desktop app and the VSCode extension (#177).
- After you commit a merge that had no conflicts, a running post-merge hook shows under the commit message with Cancel, like post-commit; cancelling stops the hook and keeps the commit (#178).
- The co-author picker opens above its button when there is no room below, stays inside the window, keeps its side while you filter, and closes when its button scrolls out of view (#141).
- Discarding or stashing a file whose folder was replaced by a file now names the file in the way, and Discard all with both succeeds. Discarding a file that was replaced by a folder refuses when the folder holds something Undo can't bring back (an ignored file or a nested repository) (#154).
- When a folder and a file or symbolic link replace each other, Stage all works in any order (a file below a link stages as its deletion), and unstaging any of them, the folder's name included, puts the whole swap back as it was in the last commit (#157).
- Accessibility: a remote branch's actions button is named with its remote ("Actions for origin/main"), and closing a dialog opened from the commit menu returns focus to the commit list (#106, #146).

### Hooks bundle

- Merging a branch now runs the pre-merge-commit hook (skipped by Skip hooks and "Commit without hooks") instead of pre-commit when you commit a merge that had no conflicts, and that commit no longer runs post-commit, as with git merge; post-merge runs after that commit instead of when the merge is staged. A merge that had conflicts runs the commit hooks (including post-commit), as in git (#124).
- Fixed: post-merge ran when a merge without conflicts was only staged, before its merge commit existed; it now runs after that commit, as in git (#124).
- Continue on a paused multi-commit cherry-pick or revert runs pre-commit, prepare-commit-msg, commit-msg and post-commit for the step it commits, so a Gerrit Change-Id hook works. A failing hook keeps the step paused and offers "Continue without hooks", and a running hook shows on the panel with Cancel (#133).
- Fixed: started from a terminal, a hook (or signing program) that reads the terminal no longer hangs until Cancel or the time limit; it fails at once, as when Browsitory is started from the desktop (#126).
- Fixed (Windows): Cancel and the time limit now also stop the programs a hook started, not just its shell (#122).
- Fixed (desktop app): a long-running hook, signing or transfer could stall other actions, including Cancel, across all open repositories (#125).
- Tests: the VSCode extension's E2E covers a failing hook, "Commit without hooks" and Cancel (#123).

### Rebase/undo safety

- Fixed: starting a rebase over uncommitted changes could discard them (when the first step
  failed, and on Abort). A rebase now refuses to start while tracked files have staged or
  unstaged changes (including an edited skip-worktree file), as git does; untracked files are
  fine (#170).
- The undo entry of a rebase names the commit it was rebased onto by its short id, and a rebase
  start that failed but still changed something is listed as "Rebase onto … failed (rolled
  back)" (#170).
- While a cherry-pick/revert sequence is paused, deleting, renaming or adding a worktree on its
  branch and saving a stash are refused, as during a rebase (#171).
- Fixed: with the desktop app and the VSCode extension open on one repository, finishing a
  paused operation in one could lose its undo entry, and two saves could drop an entry. Undo
  history changes now take a lock in .git/browsitory (#172). If the other window keeps it for
  more than about 10 seconds, Undo, Redo, discards and a hard reset ask you to try again rather
  than run without an undo entry; other operations run but can't be undone.
- Fixed: a merge, cherry-pick or revert aborted outside Browsitory and started again with the
  same commits reused the first one's undo snapshot (#172).
- With HEAD already holding a link and a folder whose names differ only in case, unrelated
  merges, cherry-picks, reverts, stash applies, pulls, rebases and switches to a branch with the
  same pair now work on case-sensitive file systems; only a new collision, or the same one with
  its link changed, is refused. On macOS and Windows, where the link replaces the folder on disk,
  the on-disk link check still refuses them (#167).
- Rebase abort and cherry-pick/revert sequence abort are no longer refused when the tree they
  return to holds such a case collision (#161).

### Restart resilience

- A paused rebase (conflict, edit stop or error) now survives an app restart, a VSCode window
  reload, closing its tab and a backend crash, and the status strip shows it again on start
  (#108).
- A cherry-pick/revert sequence, rebase or conflicted merge finished after a restart is still
  recorded as one undo step (#134).
- If the repository was changed outside Browsitory while a rebase was paused (HEAD moved, the
  branch moved or was deleted, another git operation started), the strip shows "Rebase of
  <branch> interrupted" with the reason. Abort then stops the rebase without moving HEAD or
  discarding work it no longer owns; when the branch had changed, its toast offers "Switch to
  <branch>".
- Continue accepts a paused rebase step that was finished with git (`git cherry-pick
  --continue` or `git commit`) when the commit keeps the step's author, message and changes.
- Continue of a stopped cherry-pick/revert sequence now also requires a commit made by hand to
  carry the step's message and changes, not only its author, before counting it as the step.
- While a rebase or a sequence is paused, creating a branch (which checks it out) is refused,
  and so are deleting, renaming or adding a worktree on the branch being rebased.
- Fixed: continuing a rebase after a step failed (for example because a local edit was in the
  way) committed that step empty and lost its changes.
- Fixed: a rebase whose branch moved during the pause no longer overwrites the branch; Continue
  reports it, and a failed final step can be retried instead of only aborted.
- Fixed: a failed Abort rebase (for example a folder that couldn't be written) no longer forgets
  the rebase and strands HEAD detached; Abort can be retried.

### Interface refresh

- Calmer, editor-like look: a new charcoal dark palette and warm-neutral light palette with a
  single amber accent, flat borderless controls, hairline pane dividers and thin scrollbars.
- Slim title bar with editor-style tabs that sit flush on the workspace; a tab's close button
  only shows on the active or hovered tab. The multicolor strip under the header is gone.
- Sidebar section headings are small uppercase labels; secondary actions (Fetch/Pull/Push, Blame,
  History, diff view options, hunk actions, Stage all) are quiet text buttons that fill on hover.
- The selected changed file is marked by an accent edge instead of a tinted block; Commit is the
  one accent-filled button. Commit summaries use the UI font; branch labels are flat chips.
- The command palette and other overlays open near the top of the window.
- Denser branch tree: folders and branches share one even 22px row height with no gaps between
  folders, a tighter indent, and each branch's "…" menu shows on hover or focus.

### Commit search

- A search field above the commit graph (**Ctrl/Cmd+F** on the commit list, or "Search commits"
  in the palette) finds commits by message or SHA, `author:`, `committer:`, `message:`,
  `commit:`, `path:`, `contents:` (added or removed lines), `after:`/`before:` dates and
  `branch:`. Terms combine with AND; quote text with spaces or a colon.
- Results replace the graph, load 100 at a time with **Load more**, and show how many commits
  were scanned; Esc or × returns to the graph and your previous selection.
- A slow search (`path:`, `contents:`) works in short slices, so committing or staging while it
  runs is not held up; a row you pick while results load keeps its selection.

### Refs and remotes

- Tags show as outlined badges on commit graph rows (and in file history and search results).
  Right-click one on the graph to push it to a remote, check it out (detached), or delete it.
- **Push to…** on any local branch pushes it to any remote under any name, optionally setting
  that remote branch as its upstream. **Push** in the sync bar opens the same dialog when the
  current branch has no upstream yet.
- Remote branches have **Delete <remote>/<branch>…**, which deletes the branch on the remote and
  its remote-tracking branch. Local branches that tracked it keep their upstream setting.
- **Fetch** in the sync bar has a menu with **Fetch all remotes** and a **Prune** toggle
  (remembered per repository; until you set it, your git `fetch.prune` /
  `remote.<name>.prune` config decides). Fetch all keeps going past a remote that fails and names
  it afterwards.
- Local branches with an upstream show `↑n ↓m` in the branch tree.
- An open remote folder in the branch tree now updates after any fetch, push or remote branch
  delete, not only after its own Fetch.
- Palette: "Fetch all remotes", "Push branch to…", "Delete remote branch…".

### Merge and stash options

- "Merge into current branch…" asks how to merge: fast-forward if possible, always create a
  merge commit, or fast-forward only, plus **Squash** (stages the combined changes and pre-fills
  the commit message). The choice is remembered per repository; the first default follows
  `merge.ff`.
- Stash is a split button: **Stash all** or **Stash…** with a message, "Keep staged changes" and
  "Include untracked files". **Stash file…** in a changed file's menu stashes just that file and
  leaves every other change alone.
- Click a stash to see its changes, including untracked files, and Apply, **Pop** or Drop it from
  there or from the Stashes panel. "Restore staged changes" brings staged work back staged.
  Conflicts leave the stash in place and open in the conflict pane.
- Applying a stash while something is staged now says what to do ("Commit or unstage your staged
  changes before applying a stash.") instead of libgit2's "uncommitted changes exist in the
  index"; Apply and Pop are disabled with that reason.
- Apply, Pop and Drop check that the stash is still the one you clicked; if the stash list
  changed in the meantime (another tool stashed or dropped), they stop with "The stash list
  changed; refresh and try again."
- Palette: "Merge <branch> into current branch…", "Stash all changes" (was "Save stash"),
  "Stash…", "Stash <file>…", "Show stash: …", "Pop stash: …".

### Compare

- **Compare these commits** (two selected commits, from the selection bar or the right-click
  menu) shows every file that differs between them in the diff pane, the older commit on the
  left, with **Swap**. Renamed files are listed once with only their edits.
- **Compare with current branch** on a branch (local or remote-tracking) shows what it changed
  since it diverged; **Diff against working tree** on a commit compares it with your files on
  disk, untracked files included.
- Compare views are read-only, with Blame and History on each file. Clicking any commit or
  stash row (or ×) closes the compare.

### Git hooks

- Commits and amends run your repository's `pre-commit`, `prepare-commit-msg`, `commit-msg` and
  `post-commit` hooks; every push (branch, force push, tags, **Push to…**, deleting a remote
  branch) runs `pre-push`; switching branches and checking out a commit run `post-checkout`;
  merging and a fast-forward pull run `post-merge`. `core.hooksPath` is honored.
- A failing hook stops the commit or push and shows its output, with **Commit without hooks** /
  **Amend without hooks** / **Push without hooks** to retry (`--no-verify`). A **Skip hooks**
  checkbox under the commit message (shown when a `pre-commit` or `commit-msg` hook is installed) or "Skip hooks for
  the next commit" in the palette skips them for the next commit.
- While a hook runs, "Running pre-commit hook…" (or "Running pre-push hook…" in the transfer
  panel) shows with a Cancel button; a hook is stopped after 10 minutes. On Windows hooks run
  with Git for Windows' `sh`; without it a blocking hook fails with a message saying so.

### External tools

- File menus (the "…" button or right-click on a changed file, a commit's file or a compared
  file) open the file in your editor, reveal it in the file manager, or open its changes in your
  git diff tool. Conflicted files open in your git merge tool, and when it finishes Browsitory
  offers **Mark resolved** (it asks first if conflict markers remain).
- Right-click a repository tab (or press Shift+F10 / the Menu key on it, or use the palette) for
  **Open terminal here** and **Reveal repository**.
- A new **Preferences** dialog (gear button or "Preferences…" in the palette) sets the editor
  and terminal commands, with `{file}`, `{line}` and `{dir}` placeholders. Without an editor,
  files open with the system default app, except files that could run (programs, installers,
  scripts, shortcuts, launchers, on every platform), which Browsitory refuses to hand to the
  system. Commands are checked when you save them.
- The diff and merge tools come from your own git config (`diff.tool`, `merge.tool`,
  `difftool.<name>.cmd`, …) and are shown in Preferences read-only. meld, kdiff3, Beyond
  Compare, p4merge, VS Code and WinMerge work without a `cmd`. Tool settings in a repository's
  own `.git/config` are ignored, so a repository can't make Browsitory run commands (hooks keep
  git's behaviour). The tool sees the stored file bytes, without line-ending conversion.
- In the VSCode extension, these open in VSCode itself: editor, diff view, merge editor,
  integrated terminal and "Reveal in OS".

### Conflicts and picks

- The conflict pane has **Use ours (whole file)**, **Use theirs (whole file)** and **Mark
  resolved** (which asks first if the file still has conflict markers). They work for content,
  add/delete and binary conflicts, in merges, cherry-picks, reverts, rebases and stash pops.
- Select several commits in the graph to **Cherry-pick N commits** or **Revert N commits** as one
  operation. A conflict pauses it; the status strip shows "Cherry-picking 2/5" with **Continue**
  and **Abort**, and it survives an app restart. Commits that would change nothing (already on
  the branch) are skipped and counted in the result. One undo step covers the whole sequence.
  Reverting a revert is called "Reapply …", as in git.
- Merge commits can be cherry-picked or reverted: **Cherry-pick…** / **Revert…** asks which
  parent to apply them against.
- Fixed: a cherry-pick or revert that stopped on a conflict no longer shows a "Cherry-picked" /
  "Reverted" toast (#107).

### Image diff and commit message polish

- PNG, JPEG, GIF, WebP and SVG changes show the old and new image side by side, with their
  dimensions and file size (up to 20 MB per side), in the working tree, commits, stashes and
  compares. SVGs can switch to the text diff. The image type is checked from the file's content,
  and images are only ever shown as pictures, so an SVG can't run scripts.
- The commit message shows a summary length counter (soft limit 50) and a ruler at column 72 for
  the body; the field uses a monospace font while the ruler is shown. Both are configurable under
  **Preferences → Commit message**.
- **Add co-author** picks from the repository's recent authors (or takes a typed
  `Name <email>`) and adds `Co-authored-by:` trailers, after any existing trailers and without
  duplicates (compared ignoring case, as git does). An empty message is still refused when
  co-authors are set. **Commit without hooks** and **Amend without hooks** use the co-authors
  (and, when amending, the author) shown when you retry (#139).
- Amending shows an editable **Author** field; the original author date is kept and the
  committer stays you.

### Keyboard context menus

- Every row with a context menu opens it with the Menu key or Shift+F10: local and remote
  branches, remote folders, changed files and commit/compare file headers (commits and tabs
  already did). The menu opens at the row, not the top-left corner (#44).
- Menus return focus to where they were opened when closed with Esc or by running an item (so a
  dialog an item opens also returns there), close on Tab, and jump to an item by its first letter.
  A menu whose items are all disabled still takes focus, so Esc and Tab work.
- A file header's menu (changed files, commit/compare/stash files), opened by right-click or the
  keyboard, returns focus to that header's "…" button.
- Remote folders have a "…" actions button, like branches.
- Command palette: **Edit remote …**, **Manage credentials for …** and **Remove remote …** for
  each remote, and **Check out tag … (detached)** for each tag that points (through any tags) at
  a commit. The remote dialogs also open while the Branches section or the whole sidebar is
  collapsed. If the remote of an open remote dialog is removed or renamed elsewhere, the dialog
  closes with a notice.
- The shortcut sheet (**?**) lists the context menu keys.

### Security

- Discarding changes, hard reset, undo/redo, aborting a merge, cherry-pick, revert or rebase,
  resolving a conflict, and "Stash all" no longer write (or delete) through a symbolic link that
  replaced a tracked file or folder: the link is removed and the file or folder restored, like
  git. Previously a file outside the repository could be overwritten or deleted (#114). "Stash
  all" stashes a link that replaced a file and then restores the file; while a link replaces a
  tracked folder it refuses ("… is beyond a symbolic link"), as stashing a single file there
  already did.
- Switching branches, checking out a commit, merging, cherry-picking, reverting, rebasing,
  applying a stash and pulling refuse while a symbolic link stands where the repository has a
  folder ("'d' is a symbolic link where the repository has a folder…"); git's own safe checkout
  could otherwise create a new file through the link, overwriting a file outside the repository.
- Undo/redo snapshots and their checks no longer read a file through a symbolic link that
  replaced a folder. Staging or discarding a file (or a hunk or lines of it) below such a link
  refuses like git ("'d/f' is beyond a symbolic link.") instead of reading the link's target.
  **Stage all** (or staging the link itself) stages the link in place of the folder, like
  `git add -A`.
- Those refusals also cover a symbolic link nothing in the repository tracks, when the operation
  would create files below it (a cherry-pick could overwrite a file outside the repository), and
  a merge, cherry-pick, revert or rebase step that would add a file below a folder the branch
  has committed as a symbolic link. The message names the link ("'d' is a symbolic link where
  the repository has a folder. Remove or rename it first."); applying a stash and pulling no
  longer show a generic error instead.
- Hard reset, switching branches and undo/redo work again when the repository itself has
  committed a symbolic link where another commit has a folder (the hard reset failed and
  deleted the link); nothing is written through the link, even after it is pointed elsewhere.
- Updating a submodule whose folder was replaced by a symbolic link (or sits below one) refuses
  instead of cloning into the link's target.
- The image preview reads working-tree files without following a folder that is swapped for a
  symbolic link while it reads (#138).
- On case-insensitive file systems (macOS, Windows), switching branches, checking out a commit,
  merging, pulling, cherry-picking, reverting, rebasing, applying a stash, resetting, undo/redo,
  cloning, adding a worktree or updating a submodule refuses a commit that holds a symbolic link
  and a folder whose names differ only in case ("'D' is a symbolic link and 'd' a folder whose
  names differ only in case…"), instead of letting the checkout silently drop the folder's files.
  A clone of such a repository fails and removes what it downloaded. Linux refuses only when
  `core.ignorecase` is set (#156).
- Conflict resolution, aborting a merge, undo/redo, stashing a single file, discarding an
  untracked file, **Ignore** (appending to `.gitignore`) and the removal of symbolic links
  before a checkout act on the folders they checked, so a folder swapped for a
  link at the same moment can't redirect them outside the repository. Undo no longer hangs on a
  named pipe that replaced a file, **Ignore** no longer hangs on a named pipe `.gitignore`, and
  resolving a conflict never writes into a pipe or device
  (#149).
- Windows: the image preview, conflict resolution, undo/redo and the removal of links before a
  checkout no longer follow a junction or symbolic link in the middle of a path (even one
  pointing inside the repository, or to a network share, which is no longer contacted), and the
  preview loads OneDrive online-only files (#150).
- Opening a file with the system default app also refuses `.wsc`, `.psc1`, `.ps1xml`, `.mht`,
  `.mhtml`, `.hlp` and disk images (`.iso`, `.img`, `.vhd`, `.vhdx`, `.dmg`), names ending in a
  dot or space (`setup.exe.`), and more desktop launcher variants (`[KDE Desktop Entry]`,
  `[Desktop Action …]`, a header after a long comment block) (#131).

### Fixed

- Committing or amending with a signing program that never finishes (for example a passphrase
  prompt that never appears) no longer blocks the repository: the commit box shows "Waiting for
  the signing program…" with **Cancel**, and signing stops after 2 minutes with an error that
  says so. Cherry-pick, revert and rebase commits get the same 2-minute limit, and checking a
  commit's signature in the history gives up after 30 seconds (#110).
- The diff of a file that became a folder (or the reverse) no longer shows the folder's files,
  and staging, unstaging or discarding one of its hunks or lines can no longer pick a hunk of a
  file inside that folder (#116).
- Comparing a commit with the working tree shows the changes of a conflicted file, conflict
  markers included (#117).
- A staged rename shows its new name with "Renamed from …", previews an image's old version
  against the new one (instead of "Added"), diffs only the edits, opens the diff tool against the
  old path, and unstages as a whole. A rename with edits shows as a rename, not as a
  modification. A file moved but not staged shows as deleted plus new, like `git status` (#137).
- A commit that fails (for example because of a hook) no longer clears the message you typed
  (#109).
- VS Code: a large image preview no longer freezes the extension host for seconds while its
  data arrives from the backend.
- The image preview no longer hangs the repository if a working-tree file is replaced by a named
  pipe while it is being read.

- Merge in the "Pull has diverged" dialog failed with "reference not found" (#104).
- "Drop stash" in the command palette dropped the stash without asking; it now shows the stash
  and opens the Stashes section, where Drop asks first (#105).
- A saved graph branch selection naming a deleted or renamed branch no longer breaks the whole
  refresh; deleting or renaming a branch in the app now updates the selection, and deleting the
  only isolated branch shows all branches again (#97).
- Push and force push follow `push.default` (unset = `simple`). A branch whose upstream has a
  different name (e.g. `feat` created from `origin/main`) is no longer pushed onto that
  branch: under `simple` Push opens **Push to…** with the remote and the branch's own name;
  with `push.default=upstream` it updates the upstream branch, with `current` the same-named one.
- Sidebar menus (e.g. the Branches "Add" menu) are no longer clipped by their section while it
  is opening: the section open animation now fades only, without the transform that confined
  fixed-position menus to the section body (#98).
- E2E: the Tauri suite runs with its own throwaway webview data directory, wiped before every
  spec file, instead of sharing localStorage with the installed app (#99).
- E2E: on a Wayland desktop, both suites now pin the app window to the Xvfb X11 display; it
  previously opened unfocused on the real desktop, received no frames, and never rendered diff
  lines (#100). The command-palette spec re-sends Ctrl+K if the first keypress after startup is
  lost.

### Internal

- Rust tests no longer pick up the developer's or CI runner's global/system git config
  (`init.defaultBranch`, `core.autocrlf`, `fetch.prune`, `commit.gpgSign`, `core.hooksPath`, …),
  and no longer race a temporary config directory's cleanup on Windows (#111).
- Fixed an intermittent SSH-signing test failure caused by one test's throwaway ssh-agent
  leaking into concurrently running signing tests (#153).
- E2E: specs type-check cleanly and CI now type-checks both E2E suites (#103); "row is gone"
  checks wait for the row to disappear instead of racing a stale element (#145).

## [0.3.0] - 2026-10-21

### Commit signing

- Commit signing: when `commit.gpgSign` is set, commits, amends, merges, cherry-picks, reverts and
  rebased commits are signed with gpg (`gpg.format=openpgp`) or SSH (`gpg.format=ssh`), using
  your existing gpg-agent / ssh-agent. If signing fails, no commit is made.
- The commit details pane shows the signature status of signed commits (good, untrusted, unknown
  key, expired, bad, revoked).

### Undo and redo

- **Ctrl/Cmd+Z** undoes the last Git operation in the active repository and **Ctrl/Cmd+Shift+Z**
  (or Ctrl+Y) redoes it, also from the undo/redo buttons next to Fetch/Pull/Push, the command
  palette, and the toast's Undo/Redo button. Covers staging, commits, amend, reset, branch and
  tag changes, checkout, cherry-pick, revert, merge, rebase, pull and every discard (discarded
  changes are now recoverable). Up to 50 steps, kept across restarts.
- Undo refuses, instead of overwriting, when the repository changed since the operation; the
  reflog stays available for those cases.
- Undo also refuses while a branch it would delete, move or switch to is checked out in another
  worktree, and keeps line endings right with `core.autocrlf` and other checkout filters.

### Clone and new repository

- **Clone…** in the repository picker and a "Clone repository…" palette command (desktop app):
  clone from HTTPS, SSH or a local path with progress and cancel, an optional branch, and
  submodules. An HTTPS token entered in the dialog is saved to the keychain only after a
  successful clone.
- **New repository…** creates an empty repository in a folder you pick and opens it.

### File history

- **History** button on every tracked file (working tree and commit view) and a "File history
  of current file" palette command: the commit list shows only the commits that changed the
  file, following renames, with that file's diff for the selected commit.
- Rename commits show the edited lines instead of a whole-file add.

### Line staging, diff options, syntax highlighting, discard and ignore

- Select individual lines in a diff (click, Shift+click, Ctrl/Cmd+click, drag, or ↑/↓ with
  Shift) and stage, unstage or discard just those lines. `s` and `d` act on the selection when
  there is one, otherwise on the hunk.
- Keyboard: `↑`/`↓` move a line cursor inside the active hunk and select that line,
  `Shift+↑`/`Shift+↓` extend the selection, `s` and `d` `d` act on the selection (or the hunk
  when nothing is selected), and `[` / `]` move between hunks and clear the selection.
- Line actions refuse to run if the file changed since its diff was shown (for example an edit
  inside the hunk in another editor); nothing is changed and the diff refreshes instead.
- Staging, unstaging or discarding a whole hunk now also works with zero context lines and for
  hunks of untracked, newly added or deleted files, which previously failed. Discarding a whole
  hunk of an untracked file empties the file rather than deleting it (use Discard changes… on
  the file to delete it).
- Whole-hunk staging no longer carries a file's mode change (chmod, such as the executable
  bit); stage the whole file to stage a mode change.
- Diff toolbar: Ignore whitespace, context lines (0-20) and Syntax highlighting, remembered
  across sessions. While whitespace is ignored, partial staging is turned off in the working
  tree and a hint appears in its place; file-level stage and unstage still work. Right after
  changing an option, hunk and line actions reappear once the diff has reloaded with it.
- Diffs are syntax-highlighted for common languages, unless a file's diff has more than 2,000
  lines in total across its hunks.
- File "⋯" menu (also right-click or Shift+F10 on a file header): Discard changes…, and for
  untracked files Ignore file / Ignore all *.ext / Ignore folder, which append to `.gitignore`.
  Discarding (a file, a hunk or lines; tracked or untracked) refuses paths outside the
  repository, including through symlinked folders. Ignoring refuses a `.gitignore` that is a
  symlink, never overwrites one that can't be read, and rejects patterns with leading or
  trailing spaces.
- "Discard all…" next to "Stage all" discards the unstaged changes of exactly the files its
  confirmation lists; untracked files are deleted and conflicted files are skipped. Folders
  such as untracked nested repositories are never deleted, and files that couldn't be discarded
  are named in an error once the rest are done.
- Palette commands for each diff option and for discarding.

### Cherry-pick, revert, reset, amend and force push

- Commit context menu: Cherry-pick, Revert, Checkout (detached) and "Reset ⟨branch⟩ to here…"
  (soft, mixed or hard; hard asks first). Cherry-pick and revert conflicts use the merge
  conflict flow, with their own status-strip label and abort button. A cherry-pick finished
  after resolving conflicts keeps the original author, and aborting one leaves unrelated
  unstaged edits alone (including when a conflicted file name contains glob characters).
- "Amend last commit" toggle in the commit box, plus "Edit commit message…" on the latest
  commit; warns when that commit is already pushed.
- Force push with lease: offered when a push is rejected as non-fast-forward, and from the
  palette; refused if the remote branch moved since the last fetch.
- Detached HEAD is shown in the sync bar, with push and pull disabled.
- Palette commands for all of the above.

### Multi-select commits for interactive rebase

- Ctrl/Cmd+click commits in the graph to select several, including non-adjacent ones (Shift
  selects a range). A toolbar and the context menu offer Interactive rebase…, Squash, Fixup and
  Drop, each opening the rebase planner with the selection pre-marked. Squash and Fixup require
  adjacent commits; Drop and Interactive rebase… accept any selection.

### UX audit: feedback, states and content

- A persistent status strip now shows while a merge or rebase is in progress (step, conflict
  count, Abort), and Commit / Continue rebase show a visible reason when disabled.
- Success toasts (polite live region) after commit, checkout, branch delete, stash, fetch, push
  and pull. Error banners float instead of shifting the layout (FB-004). A failed diff, blame or
  commit-file fetch in the diff pane keeps the raw error text, adds a plain-language hint for known
  error kinds and offers Retry; a failed repository open also offers Retry, and a lost backend
  connection shows a fixed hint. Errors from actions (the top-level banner for a failed commit,
  checkout, push and so on) still show the message only, with no hint and no Retry.
- New success, warning and info color tokens for light and dark themes (VIS-002), applied to status
  strip, toasts, and conflicted file tinting.
- The inline New branch form shows its base, validates the name inline, and can check the new
  branch out. Pull request source/target branches default sensibly and suggest known branches.
- The repository picker leads with folder names, explains workspaces, and hints at the command
  palette. The palette hint is platform-aware and release notes use a "What's new" icon.
- Sentence case for UI labels (FB-007): picker, workspace and rebase buttons, the branch "+" menu
  ("New branch…", "Add remote…"), hunk actions, conflict resolution ("Accept ours", "Keep their
  version", …) and "Back to diff", documented in `docs/CONTENT_GUIDELINES.md` together with the
  Title Case labels that remain.
- Added test coverage for the `?` keyboard shortcut guard and sidebar auto-collapse behavior in `App`.

### Security

- `resolve_conflict` now validates that the given path is an actual index conflict before
  writing to the working directory, closing a gap where any path (including one under `.git/`)
  would be written and staged unconditionally.
- Updated `rustls` to 0.23.45 to pick up the fix for RUSTSEC-2026-0285 (TLS 1.3 handshake
  messages accepted across encryption-level boundaries).

### Fixed

- Staged files no longer offer a discard action (the hunk and line discard buttons and `d` in a staged file's diff); it could previously revert an
  unrelated hunk of unstaged changes.
- Diff pane: a file's diff shows "Loading diff…" instead of a false "No differences" while it
  loads; empty diffs now read "No text differences (binary file or mode-only change)".
  Collapsed file sections no longer fetch their diff until expanded, and sections far from the
  viewport not until scrolled near (PERF-001, IntersectionObserver). Open diffs refetch on every
  refresh (an action, the palette's Refresh, a stash apply or pull), and a hunk stage/unstage/
  discard refetches both the staged and unstaged diff of that file once the action has finished.
  Only the active repository tab's workspace is mounted, to avoid paying every open tab's DOM and
  IPC cost; a half-typed commit message is kept per repository across tab switches, and a tab
  switched away from mid-action no longer stays unclosable.
- Diff hunk actions are no longer tab stops. The diff is one tab stop with `[`/`]` (prev/next
  hunk), `s` (stage/unstage) and `d` (discard, press twice). The armed "Confirm discard" is
  styled as danger and disarms on blur, Escape or after 5 seconds.
- Diff lines show old/new line-number gutters; long tokens wrap and hunk headers wrap cleanly
  at narrow widths. Paired Remove/Add lines now highlight only the changed words (computed once
  per loaded diff; line pairs too long to compare cheaply, such as minified code, are marked
  wholly changed instead), and a Split view / Unified view toggle switches the diff between the
  single-column and a side-by-side layout (UX-007).
- Commit dock is lighter (single border, no nested panel) and no longer lets diff content show
  beneath it.
- Accessibility pass: confirm and form dialogs return focus to the invoking control when they
  close; workspace deletion uses the shared modal `ConfirmDialog`; repo tabs use roving tabindex
  with Left/Right/Home/End navigation and link to their workspace panel. Each tab's close button
  is still inside the tablist but is now `aria-hidden` and out of the Tab order (mouse-only), so
  the tablist exposes only tabs; from the keyboard, Delete closes the focused tab, the palette
  has a "Close tab" command, and Ctrl/Cmd+W closes the active tab where the host doesn't take
  the key first (Tauri's default macOS menu binds Cmd+W to Close Window) (A11Y-003); swatches,
  tab close buttons, sidebar toolbar buttons and split dividers have at least 24x24 pointer
  targets (`--size-target-min`); context menus no longer close on mouse leave and stay inside the
  viewport; reduced-motion preference disables smooth scrolling and transitions; sidebar toolbar
  icons have tooltips. Added a shared `Field` primitive (label, hint, error, `aria-invalid`,
  `aria-describedby`, `required`).
- Narrow windows: the Tauri window has a minimum size (800x500), the diff pane keeps a minimum
  width instead of collapsing to zero, and the sidebar auto-collapses below 900px (RESP-001).
- The header "+" (open repository) button is pinned outside the scrolling tab strip (RESP-002).
- Transfer progress shows MB/GB instead of ever-larger KB values, and the transfer panel now has
  a working Cancel button (PERF-002). Cancelling aborts the in-flight `git2` transfer itself: a
  shared registry of cancelled operation IDs is polled from inside the fetch progress callbacks,
  so a cancel lands even while the worker thread is blocked in network I/O. A push can only be
  cancelled before its data is sent (at `push_negotiation`); a cancel pressed later lets the push
  finish and report its real outcome, since the remote may already have applied it. Surfaces
  as a new `TransferErrorKind::Cancelled`, a `cancel_transfer` command on both the Tauri and
  VSCode transports, and a "Fetch/Pull/Push cancelled." notice rather than a failure message.

- Keyboard shortcut sheet, opened with `?` or the "Show keyboard shortcuts" palette command
  (UX-006).
- Sidebar section header buttons (such as the Branches "+") no longer sit under the sidebar's
  overlay scrollbar, which swallowed clicks on them once the sections overflowed; this also
  fixes the remote e2e specs that go through that button.
- Pulling a fast-forward no longer force-checks out over untracked or ignored files: the
  checkout is now safe and aborts untouched with a "would be overwritten" error, and the pull
  surfaces its dirty-worktree, no-upstream, detached-HEAD and checkout-conflict reasons instead
  of a generic "pull failed".
- `current_upstream` no longer errors on a detached HEAD (the normal state during an in-progress
  rebase); it returns "no upstream" instead. Previously this rejected the frontend's whole
  per-mutation state refresh, leaving the UI stuck showing stale pre-rebase content after a
  rebase paused on a conflict or was aborted.
- The VSCode extension's `vscode-sidecar` process now dispatches each JSON-RPC request on its
  own thread instead of one shared blocking loop, so a slow operation against one open repo (a
  large blame, a big commit graph) no longer freezes every other open repo's requests.
- `sidecarBridge.ts` no longer forwards a JSON-RPC response to the webview unless it actually
  matches a pending request.
- A worker thread that panics is now detected and evicted, so reopening the affected repository
  respawns a fresh worker instead of leaving the repository permanently stuck.
- A worker-thread failure now surfaces as "connection to this repository was lost, reopen it"
  instead of the raw internal error string.
- Remote-tracking branch rows in the Branches tree are now keyboard-focusable, and the shared
  `ContextMenu` widget implements standard menu keyboard navigation (arrow keys, Home/End,
  initial focus on open) instead of only closing on Escape.
- The commit-message textarea now has an explicit accessible name.
- The frontend's global error/rejection logging now goes through `RepoClient` instead of
  importing a Tauri-only plugin directly, so uncaught errors are actually logged from the
  VSCode webview too (previously silently dropped there).
- Pressing Enter or Space on a branch row's graph-visibility swatch or "..." button no longer
  checks the branch out; key events from controls nested in a row stay with that control.
- Branch tree rows stay on one line with truncating names (full name in the tooltip), the "+"
  button moved into the Branches header, and the graph-visibility swatch has a larger hit area
  and a hollow "hidden" state.
- Release notes no longer truncate a changelog bullet that wraps across lines, and wrapped diff
  lines get a hanging indent.
- The window no longer flashes blank while open repositories restore.
- Commit, Continue rebase and Abort rebase/merge buttons are pinned to the bottom of the diff pane
  instead of sitting below every file section, where a large change set pushed them thousands of
  pixels off screen.
- Modal dialogs (release notes, command palette, repo picker) open scrolled to the top instead of
  to whichever control received focus. The command palette also fills its dialog width, and the
  empty-state header, empty-state text and Uncommitted Changes row match the rest of the UI.

### Changed

- The transport-isolation ESLint rule now covers all of `frontend/src/**` (previously missed
  `src/lib/**`) and also bans `vscode` imports outside `vscodeRepoClient.ts`/`extension/`.

### Added

- Selecting a commit now shows a header with its full message, author, date, full SHA (copyable)
  and parents (click to jump), backed by a new `get_commit_message` IPC command wired through
  git-core, repo-service, tauri-app and the VSCode sidecar (UX-001). History rows also show
  author and date columns that hide when the pane is narrow.
- History pagination: the graph loads 300 commits at a time with a "Load more" row, and
  arrowing past the last row loads the next page instead of silently stopping (UX-002).
- Sync bar above the commit graph with Fetch, Pull and Push buttons and ahead/behind counts;
  `UpstreamInfo` gains optional `ahead`/`behind` (null until the tracking ref is fetched)
  (UX-008).
- "Checkout" is now the first item of a local branch's context menu; the user guide documents
  double-click checkout too (UX-003).
- The "Uncommitted Changes" row now has a hollow lane node and aligns with commit rows
  (VIS-004).
- Branch tree: local branches get an "Actions" button and a bold current-branch marker; inline
  new-branch/add-remote/rename forms focus on open and cancel on Escape; remote lists show
  "Loading..." and "No branches"; the Upstream block is styled, explains a disabled Pull, and
  confirms before clearing the upstream.
- Commit graph keyboard access: Enter, the Menu key or Shift+F10 open the commit menu; Home/End,
  PageUp/PageDown navigate; Shift+Arrow extends the squash range.
- Command palette groups commands by kind, uses the UI font size and shows key hints; the header
  shows a Ctrl/Cmd+K hint; the empty-state repository picker is a styled row list.
- BranchTree's edit-remote, credentials, set-upstream and diverged-pull dialogs now share a
  `FormDialog` primitive that focuses the first field.
- A desktop VSCode extension host now loads the shared React frontend in a restricted webview,
  routes folder/external-URL/version operations through native VSCode APIs, and forwards the
  remaining `RepoClient` surface to `vscode-sidecar` over JSON-RPC. Sidecar exit, process error,
  and stdin failure reject pending actions with an in-app diagnostic; a later action lazily
  starts one fresh process without replaying mutations.
- Target-specific VSIX packages now bundle the compiled extension, package-local webview assets,
  and exactly one matching `vscode-sidecar` binary for Linux x64, macOS x64/arm64, or Windows x64.
  Main-branch CI retains all four packages as artifacts, and tag releases attach them to the draft
  GitHub Release.
- A new `extension/e2e/` layer gives the VSCode extension its own black-box E2E coverage:
  `@vscode/test-electron` drives the unpacked extension inside a real Extension Development
  Host, and a hand-rolled raw Chrome DevTools Protocol client (no Playwright or other
  browser-automation library) attaches to the webview's nested content frame for DOM assertions.
  One flow so far — open repo → stage a file → commit → see it in history — mirroring `e2e/`'s
  existing pattern. Wired into CI as a new `e2e-vscode` job that gates `build-vsix`.
- The branch tree (local and each remote) now nests branches into folders split on each `/` in
  their name, matching a filesystem-style tree view. Checkout is now double-click instead of
  single-click (single click just selects/highlights a row); the graph-visibility control is now
  a per-branch colored swatch button instead of a checkbox.
- Remote-tracking branch tips now show inline in the commit graph as a dashed-border badge next to
  the existing solid local-branch badge, with their own per-remote-branch graph-visibility toggle
  in the branch tree.

### Changed

- Extracted the git worker/credential/forge service layer out of `tauri-app` into a new
  internal `repo-service` crate, in preparation for a future VSCode extension — no
  user-visible behavior change.
- `crates/vscode-sidecar` now wires the full `RepoClient` method surface (every method except
  the five VSCode-native ones — repo folder picking, app/last-seen version, and opening an
  external URL — which the extension host wires directly against VSCode APIs), including a
  transfer-progress JSON-RPC notification mechanism for fetch/push/pull.

### Fixed

- The commit graph's lane lines could render broken: a lane-shifting connector could be cut by
  a later-painted, unrelated pass-through line crossing it, a pass-through lane's straight
  continuation could be dropped entirely when another commit's connector happened to land on
  the same lane, and even once both were fixed, a straight pass-through line split into two
  segments at the row's midpoint could still show a hairline rendering seam. `CommitLaneGraphic`
  now paints connectors last and draws each non-own pass-through lane as a single unbroken line.

## [0.2.0] - 2026-08-29

### Added

- Failure logging: the backend now writes a rotated log file to the OS log
  directory (`tauri-plugin-log`), with a panic hook covering worker-thread
  crashes. The frontend routes every failed IPC call and any uncaught
  error/rejection into the same file via `@tauri-apps/plugin-log`, so a bug
  report doesn't require a live dev session to diagnose.
- A gear popover in the sidebar lets each panel (Stashes, Worktrees,
  Submodules, Reflog, Tags, Pull Requests) be shown or hidden, persisted
  globally (one `localStorage` setting shared across every repo and open
  tab) — clutter from features a project doesn't use can be tucked
  away.
- Each branch's context menu in the Branches tree gained "Isolate branch",
  a one-click way to filter the commit graph down to just that branch; the
  tree's "+" menu gained "Show all branches" to clear the filter.

### Changed

- The diff view now renders every changed file's diff expanded by default,
  instead of a file list where each row had to be
  clicked to reveal its diff. Each file's header carries an inline
  Stage/Unstage control and a Blame button, plus a per-file collapse
  chevron; a "Collapse all"/"Expand all" toggle gives an overview of a
  large changeset.
- HTTPS credential-store failures now retain a safe diagnostic in the
  application log while keeping the on-screen message generic.
- Stashes moved out of the Branches sidebar section into their own
  "Stashes" accordion section, with a confirmation before dropping one.
- Every remaining sidebar section (Worktrees, Submodules, Reflog, Remotes,
  Tags, Pull Requests) reskinned to the Phase 5 design system: consistent
  row styling, per-row icons, and empty states.
- The separate branch-switcher dropdown and Remotes accordion were
  replaced by a single, always-expanded Branches tree: local branches and
  one folder per remote, with every mutating action (create/rename/delete
  a branch, merge, add/edit/remove a remote, set upstream, manage
  credentials, push/fetch) moved to right-click context menus and the
  command palette.

### Fixed

- `config.toml` writes are now atomic (write-temp-then-rename) instead of
  an in-place `fs::write`, which could interleave with a concurrent save
  and leave corrupt trailing bytes from the prior write.
- The workspaces E2E spec's Edit/Delete steps now allow up to 45s for the
  picker to reload its workspace list after a full app restart, matching
  the CI-runner contention already documented for this suite's repo-scan
  wait — the previous 10s budget was flaking on `main`.
- `abort_rebase_restores_the_original_branch_and_tip_exactly` no longer
  asserts against the pre-rebase commit's OID, which only matched by
  coincidence when two independently-generated committer timestamps
  landed in the same second; it was flaking on the `windows-latest` CI
  runner. Also: the `rust` CI job's macOS/Windows legs now run on every
  pull request instead of only on pushes to `main`, so a platform-specific
  regression like this one is caught before merge.
