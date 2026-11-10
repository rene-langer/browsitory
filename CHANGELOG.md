# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.6.0] - 2026-11-10

Revert or apply a single file or hunk of a commit and save and apply patches, the history of a
range of lines or a folder, a commit's whole file tree, several changed files selected at once,
`or`, `not` and parentheses in commit search, a choice of diff algorithm, steadier background
fetching and updates, and files whose names differ only in case kept intact on Linux.

### Patches and partial reverts

- Added: a commit's file menu has **Revert this file**, **Apply this file to the working tree**
  and **Save file as patch…**; each hunk of a commit's diff has **Revert hunk** and **Apply
  hunk**. The result lands as unstaged changes, never as a commit; a change that no longer
  applies leaves the working tree unchanged, like a failed `git apply`, and every revert or
  apply can be undone (#448).
- Added: **Save as patch…** for a commit, for several selected commits (`git format-patch`
  mails, oldest first) and for the staged changes, and **Apply patch…** in the command palette,
  which applies a patch file to the working tree, all of it or nothing (#448).

### History of lines and folders

- Added: select lines in a commit's diff, its Blame view, a comparison or a file under All files
  and click **History of lines N–M** to list the commits that changed them, following the lines
  back through earlier edits and renames like `git log -L` (#447).
- Added: the `line:PATH:FROM-TO` search key finds the same commits and combines with other
  terms (#447).
- Added: **Folder history** in a file's header, and on each folder under All files, searches
  for the commits that changed anything in that folder (#447).

### Files at a commit

- Added: a commit's details switch between **Changes** and **All files**. All files browses the
  commit's whole tree (folders load as you open them, a filter searches every path) and shows a
  file's content at that commit read-only, with syntax highlighting; images are shown, binary
  and large files get a notice. A file's actions open its history, compare it with the working
  tree, save a copy elsewhere, or restore it to the working tree, one undo step (#446).

### Selecting several changed files

- Added: the Changes and Staged lists select several files: Ctrl/Cmd+click adds or removes one,
  Shift+click and Shift+↑/↓ select a range, Ctrl/Cmd+A the whole list, Esc keeps only the
  current file. With two or more selected, a bar above the list, a selected file's menu, `s` and
  the command palette stage, unstage, discard (one confirmation, one undo step), ignore or stash
  them together; conflicted files are left out. The lists are announced as multi-selectable and
  screen readers hear how many files are selected (#449).

### Search: or, not, parentheses and merge filters

- Added: commit search understands `or`, `not` (or a leading `-`) and parentheses between terms;
  terms side by side still all have to match (#450).
- Added: `merges:only` and `merges:none` list only merge commits or leave them out (#450).
- Changed: a search that can't be read shows why under the search field, which is marked
  invalid, instead of in place of the results (#450).
- Internal: a new `InvalidSearchQuery` error kind tells an invalid query apart from other
  search failures (#450).

### Diff

- Added: **Preferences → Diff → Diff algorithm** chooses Default (Myers), Patience or Minimal
  for every diff. Staging, unstaging and discarding hunks or lines, and reverting or applying a
  commit's hunk, find the hunk with the algorithm the diff was shown with, so they act on the
  lines you see (#469).
- Added: **Toggle wrap lines** and **Toggle show whole file** in the command palette act on the
  current diff (#469).

### One-click history edits

- Fixed: the Undo entry of Reword, Squash, Fixup, Move up, Move down and Drop from the commit
  menu read "Rebase onto <id>"; it now names the edit, for example "Drop 1a2b3c4" or "Squash
  1a2b3c4 into its parent", also after a paused edit is continued (#473).

### Fetch automatically

- Fixed: a background fetch to a server that doesn't answer (VPN down, captive portal, server
  offline) no longer delays the action you start until the connection times out; the action
  runs at once, and a remote that doesn't answer within 30 seconds is given up on for that fetch
  while the other remotes are still fetched (#462).
- Changed: a remote that couldn't be reached is fetched in the background less often, at twice
  the interval, then four times, up to once an hour, until a fetch gets through again (#462).
- Fixed: a background fetch that was due while another fetch, pull or push ran could be skipped
  until the next interval, when it started the moment that operation finished: the finished
  operation still counted as running for a moment after it reported its end. This also made a
  repo-service test fail now and then on Windows (#567).

### Updates

- Fixed: when an update check or download failed (for example while the release download host
  answered with an error), the app waited 6 hours before checking again. It now retries after
  10 minutes, doubling the wait up to 3 hours, and goes back to the 6-hour interval once a check
  succeeds (#477).
- Added: a second update endpoint. The app reads the update manifest from the user guide site
  when the release download URL fails; the docs workflow publishes there a checked copy of the
  latest published release's manifest (#477).

### Files that differ only in case

On Linux with `core.ignorecase` on, the operations that could lose one of two files whose names
differ only in case (`A` and `a`) now refuse before anything is written and name both files, and
aborting no longer gets stuck on such a pair.

- Fixed: with two committed files whose names differ only in case, Reset (hard) and aborting a
  rebase kept only one of them in the index and could delete the other's file, and discarding
  one of them left a broken index. Hard reset, starting a rebase, discarding one of the pair and
  the other operations that overwrite files with it now refuse; Undo writes such files directly
  and keeps both (#403).
- Fixed: a rebase whose picked commit added a file whose name differs only in case from an
  existing one couldn't be aborted: Abort refused with the case-pair message. Aborting a
  cherry-pick or revert in that state refused, saying it would discard a change. Abort now
  always restores the branch, writing such files directly, and keeps a file you wrote there
  yourself (#511).
- Fixed: a rebase step, cherry-pick, revert, merge, pull or stash apply that added a file whose
  name differs only in case from an existing one silently kept only one of the two in the
  commit and the index. It is now refused; a rebase or a cherry-pick of several commits stops
  at that step and can be aborted. Aborting a cherry-pick of several commits after such a step
  had landed outside Browsitory now goes back to where it started (#522).
- Fixed: switching to a branch, creating a branch from one, checking out a commit, tag or remote
  branch, or adding a worktree, whose files include such a pair that the current commit
  doesn't, kept only one of the two in the index and didn't write the other's file, so the next
  commit dropped it. It is now refused (#532).

### Actions that can't run now

- Fixed: while a rebase or `git am` started in a terminal was paused between steps, the command
  palette still offered Cherry-pick, Revert, Checkout, Reset, switching branches and other
  repository actions that the menus disable (#503).
- Fixed: the command palette offered **Stash all changes** and **Stash…**, and a file's menu
  offered **Stash file…**, during a rebase or a cherry-pick/revert, when the Stash button is
  disabled because stashing then loses the paused step's changes (#495).
- Fixed: the Reset dialog, the parent picker of a merge commit's Cherry-pick and Revert, the
  Merge dialog, **Push branch to…**, **Delete remote branch…** and the force push confirmation
  still ran their operation when the repository became busy while they were open (for example a
  rebase started in a terminal, picked up by a refresh); their confirm button is now disabled
  with the reason (#514).
- Fixed: the Stash dialog still stashed when a rebase or a cherry-pick/revert sequence started
  while it was open (for example in a terminal); its Stash button is now disabled with the
  reason (#508).
- Changed: **Create tag at …** stays in the command palette while another operation runs, like
  the commit menu's **Create tag…**; the dialog says why it can't create the tag yet (#495).
- Changed: disabled commit menu items say why they are disabled (#495).
- Internal: menus, buttons and the command palette decide availability with one shared rule
  (`operationBlockers`, `stashBlockReason`, `commitActions`) (#495).

### Command palette commit actions

- Fixed: Cherry-pick and Revert from the command palette showed no "Cherry-picked" or "Reverted"
  toast, unlike the commit menu; they now run the menu's own action (#507).
- Fixed: the command palette's **Reset to …** and a merge commit's Cherry-pick and Revert did
  nothing while search results or a file's history replaced the graph; they now open the reset
  dialog or the parent picker directly (#507).
- Changed: the command palette offers Cherry-pick and Revert for a merge commit too; they ask
  which parent to use first, like the commit menu (#495, #507).

### Error messages

- Fixed: a pull you cancel says "Pull cancelled." again, not "the transfer was cancelled"
  (#496).
- Fixed: when a repository's worker has stopped, starting a fetch, push or pull says to close
  and reopen the repository, as every other action does, instead of "worker thread stopped"
  (#496).
- Fixed: in the VSCode extension, the errors of a pull, a fetch or push that can't start, an
  unreadable conflict and the repository and workspace pickers no longer start with "Error: "
  (#496).
- Internal: a failed request now answers `{ kind, message }` (`repo_service::wire::ErrorDto`,
  generated to TypeScript) on both transports: the Tauri command's error, and the sidecar's
  JSON-RPC error `data`. `RepoClient` methods reject with `RepoError`, and the frontend switches
  on its `kind` instead of searching the message text. Removed the checks that matched no
  backend text: a direct fetch's "Fetch failed" rejection (gone since fetches report failures
  only through their terminal event), the "Transport failed", "failed to read working diff" and
  "permission denied" hints, and the "missing credential" and "SSH agent failure" rejections
  (#496).

### Internal: worker command policy

- Changed: each worker command's policy (the undo entry it records, whether it takes the state
  lock, is refused while that lock stays busy, has its reply held until its undo entry is
  recorded, is refused while a rebase or cherry-pick/revert sequence is stored, and aborts a
  paused operation) is stated in one place, `worker::policy::of`, which names every command, so
  a new command no longer compiles without one. A table test pins every command's policy.
  Behaviour is unchanged (#485).

### Internal: tests and CI

- Changed: the Tauri E2E specs run the git CLI through one helper, `e2e/support/git.ts`
  (`git`, `gitIn`, `gitWith`), instead of 13 local helpers and about 170 direct calls (#500).
- Changed: `wdio.conf.ts` sets a 10 s `waitforTimeout` (`UI_TIMEOUT` in
  `e2e/support/timeouts.ts`); waits no longer repeat it, and longer ones use named constants
  (#500).
- Changed: `waitForAppReady` and the retrying `openPalette` live in `e2e/support/app.ts`, and
  both E2E suites create their fixture repository with one shared `initFixtureRepo` (#500).
- Changed: CI's mutation-testing job posts its findings as one pull request comment, updated on
  re-runs, instead of only in the job summary (#548).

## [0.5.0] - 2026-11-09

Tags at any commit and on remotes, go-to pickers, one-click history edits from the commit menu,
automatic fetching, recent commit messages, word wrap and whole-file view in diffs, submodules in
their own tab, and the commits of a detached HEAD in the graph.

### Tags

- Added: **Create tag…** in a commit's menu in the graph tags that commit (lightweight or
  annotated, optionally pushed right away), without checking it out first. The name is checked
  as you type (invalid or taken names). The command palette has **Create tag at HEAD…** and
  **Create tag at ⟨commit⟩…** for the selected commit (#438).
- Added: **Branch from tag** in a tag's menu (#438).
- Added: **Delete from remote…** for a tag, in the Tags panel, the tag badge menu and the
  command palette: picks the remote when there are several, can delete the local tag as well,
  and runs `pre-push` with the remote's credentials like a push (#439).
- Changed: the Tags panel is now the list of tags: one line per tag with a name filter, and each
  tag's actions in a menu (the "…" button, right-click, Menu key or Shift+F10) with the same
  items as its badge in the graph. Creating a tag moved to the graph and the command palette
  (#438).

### Go to a commit, branch, tag or stash

- Added: **Go to commit…**, **Go to branch…**, **Go to tag…** and **Go to stash…** in the
  command palette select the target and scroll the graph to it without checking anything out.
  Go to commit takes a SHA prefix, message text or any revision; the graph loads more history
  when the commit is further down, and when the graph doesn't show it (a hidden branch, a side
  branch of an isolated branch, no branch at all) a note says why and offers to show it (#441).
- Added: clicking a tag in the Tags panel, or a branch in the Branches section, selects its
  commit in the graph the same way. Tag rows are now focusable: Enter or Space goes to the tag,
  the Menu key or Shift+F10 opens its menu (#441).
- Changed: in the command palette, a command whose whole label you typed comes first ("go to
  branch" picks **Go to branch…** over **Go to Branches**) (#441).

### One-click history edits

- Added: the commit menu edits a commit of the current branch in one step: **Reword…**,
  **Squash with parent**, **Fixup into parent**, **Move up**, **Move down** and **Drop**. Each
  runs as a one-step interactive rebase, so a conflict pauses it like any rebase and a finished
  edit is one Undo step. An edit that can't run is disabled with the reason (another branch, a
  merge commit, the first commit, uncommitted changes, an operation in progress). The palette
  has them for the selected commit too (#442).
- Changed: **Edit commit message…** on the latest commit is now **Reword…**; it still opens the
  amend box (#442).

### Committing

- Added: **Recent messages** in the commit message box puts an earlier message back: the last 20
  messages committed or amended in Browsitory in this repository (kept per repository on this
  computer), then the newest commits on the current branch. Picking one replaces the text in the
  box; Ctrl/Cmd+Z in the field or **Restore message** undoes that (#443).
- Fixed: with a file history or search results shown in the graph, amending left the **Author**
  field empty when HEAD wasn't among those results. It now takes HEAD's author from the graph's
  commits (#464).

### Fetch automatically

- Added: **Preferences → Fetch → Fetch automatically** fetches all remotes in the background
  every 5, 10 or 30 minutes or every hour (off by default), with an override per repository. It
  runs quietly for the open tab, uses the repository's Prune choice, waits while the window or
  VSCode panel is hidden or another action runs, and never asks for credentials or makes the
  system ask to unlock its credential store: a remote that would need that is skipped and named
  once in a notice. The sync bar shows when the repository was last fetched (#440).

### Diff

- Added: **Wrap lines** above each diff wraps long lines or lets them scroll sideways, keeping
  line numbers aligned and line selection working in the unified and split views. Its default
  is the new **Preferences → Diff → Word wrap** setting: On, Off or Auto (wraps the split view,
  prose files and diffs with very long lines) (#444).
- Added: **Show whole file** shows every line of a file with its changes highlighted, in the
  working tree, commits, stashes and compares. Hunk and line staging work on it; a file over
  10,000 lines keeps the normal context (#444).
- Changed: code diffs in the unified view no longer wrap by default (word wrap Auto); choose On
  in Preferences to wrap everywhere as before (#444).

### Submodules

- Added: **Open in new tab** in a submodule's menu in the Submodules panel (the "…" button,
  right-click, Menu key or Shift+F10) opens the checked-out submodule as a repository tab of its
  own, like a worktree; also in the command palette as **Open submodule ⟨path⟩ in new tab**. For
  a submodule that isn't checked out yet the item is disabled and says why, and the menu offers
  **Initialize** and **Update** instead. In a linked worktree it opens that worktree's copy of
  the submodule (#445).

### Detached HEAD in the graph

- Fixed: the graph and commit search now include the commits of a detached HEAD that no branch
  contains (a commit made after checking out a commit or a tag, a submodule's checkout),
  whichever branches the graph shows, and the commit HEAD is on has a **HEAD** badge (#467).

### Internal: build and release

- Changed: CI's desktop build and the release build cache the tools the app bundler downloads
  (for the Windows installers and the Linux AppImage), keyed on the locked bundler version, and
  download what is missing before bundling with retries and hash checks
  (`scripts/prefetch-bundler-tools.py`), so a server error during a download no longer fails the
  bundling (#454).
- Changed: CI warns on every pull request when the locked bundler version no longer matches the
  prefetch script's tool list; `docs/RELEASING.md` says how to update it (#458).
- Changed: the release workflow installs its Linux packages with retries, like CI, so a package
  mirror hiccup no longer fails a release build (#459).
- Added: `scripts/prune-merged-branches.sh` lists or deletes local branches whose pull request
  was squash-merged (#481).

### Internal: test robustness

- Fixed: the desktop E2E test that continues a paused rebase after an app restart could click
  **Continue rebase** while the restarted app was still settling. It now waits until the button
  has stayed in place for a second, allows 20 s for the rebase to finish, and names the status
  strip's last text and any error when it doesn't (#453).
- Fixed: the E2E test of the data folder move after the identifier change waits until the app
  has written its stored settings to disk before it stops the app, and says what it found when
  it fails (#465).
- Fixed: a desktop E2E test that restarts the app could hang until the test timeout when the new
  test-driver session never answered. The restart now gets 30 s per attempt and one retry with a
  fresh driver, and the session logs the driver's and the app's process ids and ports, so a hang
  shows where it stopped (#457).
- Fixed: the desktop commit-search E2E test failed now and then because the query typed right
  after the page reload did not stick; it now types the query again until the matching result
  shows (#479).
- Fixed: the desktop rebase-abort E2E test clicks **Abort rebase** once the button is enabled,
  instead of with a lower-level click that sometimes left the rebase running. A rebase left behind
  by a failed test is undone afterwards, so the later rebase tests no longer fail on it (#479).

## [0.4.1] - 2026-11-08

The desktop app 0.4.0 was built but could not be published, so 0.4.1 ships the same changes. If
you use 0.3.x, the update brings everything listed under 0.4.0 below.

### Re-release of 0.4.0

- Changed: the same code as 0.4.0, released again so the desktop app can update. The VSCode
  extension 0.4.1 is the same as 0.4.0.

## [0.4.0] - 2026-11-08

Remote branches in the graph, a first-parent view for a single branch, a new identifier for the
desktop app, and fixes for hard-linked and executable files, stashes, merges, checkouts that
stop part-way and names that are already taken.

### New app identifier

The desktop app's identifier changes from `com.browsitory.app` to `com.browsitory.desktop`
(an identifier ending in `.app` clashes with the extension of macOS app bundles). Updating
needs nothing from you.

- Changed: the first start after the update moves the app's folders to the new name, so the
  theme, panel sizes and other choices you made in the app stay. The log folder is now
  `~/.local/share/com.browsitory.desktop/logs` on Linux, `~/Library/Logs/com.browsitory.desktop`
  on macOS and `%LOCALAPPDATA%\com.browsitory.desktop\logs` on Windows (#433).
- Changed: macOS treats the app as a new app once, so it may ask again for access to folders
  such as Documents or to your saved credentials in the keychain. Saved credentials, recent
  repositories, workspaces and settings are not stored under the identifier and stay as they
  were (#433).

### Branches in the graph

- Fixed: turning on a remote branch's dot in the Branches section (**Show ⟨remote⟩/⟨branch⟩ in
  graph**) only toggled its badge; it now adds the branch's commits to the graph and to commit
  search, also those no local branch has. Until you pick remote branches the graph is unchanged
  (#385).
- Added: **Isolate branch** on a remote branch. A single branch in the graph, local or remote,
  shows its first-parent line (#385).
- Changed: with a single branch in the graph (for example after **Isolate branch**), the graph
  shows only that branch's own line: its merge commits and the commits made directly on it, not
  every commit of the branches merged into it. A merge whose merged branch isn't shown gets a
  short dashed stub instead of a line running to the bottom of the graph. Commit search without
  `branch:` follows the same rule, so it finds what the graph shows (#380).
- Fixed: **Show all branches** saved the names of today's local branches instead of going back to
  the default graph. A branch created later stayed hidden until you clicked its dot, and in a
  repository with a single local branch the graph then showed only that branch's first-parent
  line. It now goes back to the default: every local branch, also new ones, no remote-only
  commits, and every remote branch's badge. Turning the dots back on until every local branch
  is shown, or deleting a branch so that every remaining one is shown, does the same for the
  local branches (#402).

### Hard-linked files

- Fixed: a tracked file that is a hard link to a file elsewhere (another folder, or outside the
  repository) changed under its other name too when Browsitory wrote it through libgit2:
  Discard (file, hunk or lines), branch switches, resets, merges, cherry-picks, reverts, rebase
  steps, Stash and stash apply or pop, aborts, Undo/Redo of a staged path and submodule
  updates. As with git, the file is now replaced: the work-tree name gets the new content and
  the other name keeps its old content, on Linux, macOS and Windows (#420).
- Fixed: on Windows, Save resolution, Add to .gitignore and Undo/Redo failed with "a folder of
  the working tree changed while it was being written" for a tracked file that is a hard link to
  a file in another folder, and Mark resolved and the image preview couldn't read it. Windows may
  report the file's other name, so it looked moved. Such a file is now written like any other:
  it is replaced by a new file, as git's checkout does, and the other name keeps its old
  content. A folder replaced by a junction or symbolic link while it is written is still refused
  (#260).

### Executable files and hunks

- Fixed: staging, unstaging or discarding a hunk or selected lines of an executable file dropped
  its executable bit, in the index (the next commit lost it) or in the work tree, on Linux and
  macOS. As with `git add -p` and `git checkout -p`, the file now keeps its mode, and a pending
  exec bit change is neither staged nor reverted by a hunk or line operation (#427).

### Stashes and names that differ only in case

- Fixed: on macOS, Windows or with `core.ignorecase`, **Stash all** with a staged rename that
  changes only the case of a name (`Old` to `old`) wrote a stash that didn't undo the rename
  (Linux), or refused with a misleading message about an ignored file (macOS, Windows). It now
  refuses before writing anything, like **Stash file…**: "Can't stash 'old': it differs from
  'Old' only in case. Commit the rename first, or undo it." (#370)
- Fixed: with `core.ignorecase` on, **Stash all** in a repository holding two files whose names
  differ only in case (`A` and `a`, committed on a case-sensitive drive) left one of them
  staged as deleted, and sometimes deleted its file. It now refuses before writing anything;
  **Stash file…** on other files still works (#392).
- Fixed: **Stash file…** on one of such a pair was refused as a case-only rename, or, for the
  pair's first name, stashed and broke the pair; it now gets the same clear refusal (#392).
- Fixed: on macOS (`Straße` → `Strasse`, `Café` → `CAFÉ`, composed → decomposed accents) and
  Windows (`Café` → `CAFÉ`), a rename the drive treats as the same name was refused with a
  misleading "would overwrite the ignored file" message. It is now refused as a case-only
  rename, like `README` → `readme`. **Stash file…** on one of two such names staged side by
  side no longer deletes the file until Pop or writes a stash Pop refuses (#389).

### Conflicts between a file and a folder of the same name

- Fixed: a merge, squash merge, pull with merge, cherry-pick, revert, rebase step or stash apply
  where one side has a file and the other a folder of the same name, with another name sorting
  between them (`c`, `c.txt`, `c/g`), no longer loses a file. libgit2 merged the two without a
  conflict, so a cherry-pick, revert or rebase step committed without the file, and a merge or
  stash apply stopped half-way with "directory exists". Browsitory now refuses such an operation
  before changing anything and says to run it with git, which reports the conflict (#410).

### Stopped checkouts

- Fixed: a cherry-pick, revert, rebase, pull or stash apply whose checkout stopped while writing a
  file (a full disk, an I/O error) left that file cut short, so trying again was refused; it is
  now put back with the other files (#329).
- Fixed: a rebase step after the first whose checkout stopped part-way (a file open in another
  program, a read-only file) left the files it had written and showed git's raw error; the step
  is now rolled back, a file open in another program is named, and Continue applies the step
  again (#329).
- Fixed: rolling back a stopped checkout no longer removes an empty folder you already had, and
  removes the conflict files git had written under other names (`f.txt~HEAD`) and its leftover
  `.lock` file (#329).

### Names: renaming and taken names

- Fixed: renaming a branch in the Branches tree kept spaces around the new name, so git refused it
  with its own error; the name is now trimmed (#418).
- Fixed: pressing Enter on an unchanged branch name renamed the branch onto itself; it now just
  closes the field, like Save on an unchanged remote in Edit remote (#418).
- Fixed: the branch rename field, Add remote and Edit remote now say why a name can't be used
  (a space, `..`, `~` and so on) as you type, as New branch does, instead of git's error after
  saving (#418).
- Fixed: adding a remote under a name that already exists showed that remote twice in the
  Branches tree until git refused it, then showed git's raw error; the Add remote form now says
  `A remote named "origin" already exists.` and keeps what you typed (#408).
- Fixed: creating a tag or a worktree under a name that already exists showed it twice until git
  refused it; the form now says the name is taken and keeps what you typed (#413).
- Fixed: creating or renaming a branch onto a taken name (or one that clashes as a folder, such as
  `feature/x` next to `feature`) briefly replaced the existing branch in the Branches tree; it is
  now refused at once with a clear message (#413).
- Fixed: renaming a remote to another remote's name showed that name twice until git refused it;
  it is now refused at once (#413).
- Fixed: a worktree whose folder was deleted and made again listed both entries with the same
  key, and a new worktree at a listed folder showed that folder twice while git checked it (#413).

### Refresh

- Fixed: a refresh that answered late could put older repository state back on screen, so after
  a commit the toolbar could keep offering to undo the step before it instead of the commit,
  until the next refresh. This happened when a read sent before the commit (another refresh, or
  the undo check when the window regains focus) answered after the commit's own refresh. Results
  from a read now only apply when no read sent after it has already landed (#342).

### Release notes and licences dialogs

- Fixed: the Release notes opened scrolled to the bottom in the desktop app; they now open at the
  newest release (#378).
- Changed: the Release notes' Close, User guide and Third-party licences buttons stay in view
  while the notes scroll, so the dialog can be closed at any time. A click outside the dialog
  closes it too, as Escape does (#379).
- Changed: the Third-party licences dialog keeps its Close button in view as well (#379).
- Fixed: on a short window the licence text scrolled inside a second scroll area around it, and
  its end could be cut off. The text now fills the height the dialog has left and is the only
  part that scrolls (#383).

### Internal: end-to-end tests

- Changed: the Tauri E2E suite runs on WebdriverIO 10 and expect-webdriverio 8 (#362). It first
  kept WebdriverIO 9's first-match behaviour of `$`; the suite and the documentation screenshot
  script now run with strict selectors, so every `$` selector matches exactly one element and a
  test can't act on the wrong one by accident (#394).
- Fixed: the desktop E2E test that opens Edit remote from the command palette could press Enter
  before the app had listed the remote the test added, so nothing ran and the palette stayed
  open. It now waits until the palette highlights that command (#434).

### Internal: build and release

- Changed: releases are built with `tauri-action` v1 (#363). The updater's `latest.json` now
  links each bundle by its GitHub API asset URL instead of the browser download URL; see
  `docs/RELEASING.md` for what that means for updating.
- Changed: the release and CI's `build-dist` run the Tauri CLI locked in
  `frontend/pnpm-lock.yaml` (#397, #430), and the release checks the draft's `latest.json`
  (version, every platform, URLs and signatures) before anything is published (#398).
- Changed: the shipped Linux builds are pinned to Ubuntu 24.04, so the Linux floor stays at
  glibc 2.39 (#386).
- Changed: all four pnpm projects and the workflows use pnpm 12 (#361); the user guide site is
  built with Zensical instead of MkDocs Material (#340).

## [0.3.1] - 2026-11-06

A patch release: stash fixes, a roomier Stashes sidebar, a clear stop for an invalid
`core.ignorecase`, and safer release publishing. The VSCode extension is now a stable release on
the Visual Studio Marketplace and Open VSX.

### VSCode extension is a stable release

- Changed: the extension is published on the Visual Studio Marketplace and Open VSX as a stable
  release, no longer as a pre-release (#345).

### Stashes

- Fixed: "Stash file…" on a staged rename stashes the whole rename. Before, it stashed only the
  new name and left the old name's deletion staged. If a new file was made at the old name
  since, it refuses instead of stashing that file too. Where names fold (macOS, Windows,
  `core.ignorecase`), "Stash file…" refuses a rename that only changes case, since the stash
  couldn't undo it (#152).
- Fixed: "Stash file…" on a file that was removed from the index, ignored and then edited now
  refuses with the same clear message as "Stash all", like git. Before, the stash put the
  committed version back and tracked the file again (#259).
- Fixed: stashing with a repository staged inside this one (`git add lib`, where `lib` has its
  own `.git`) showed a raw libgit2 error. "Stash all", and "Stash file…" on it, now refuse with
  a clear message. A new submodule that isn't committed yet, or a staged link whose folder is
  gone, gets its own message (#244).
- Fixed: saving a stash now spots a file in the way of a restored folder when the names differ
  only in case or Unicode form (such as `ß` and `ss`), the same way applying a stash does. This
  applies on macOS and Windows, and on Linux when the repository sets `core.ignorecase`. Before,
  saving could delete that file (#337).

### Stashes sidebar

- Fixed: in a narrow sidebar, the Stashes section stacked its Apply, Pop and Drop buttons and cut
  stash names to a few letters. Each stash now has a "…" menu with Apply, Pop and Drop… (also on
  right-click, the Menu key or Shift+F10), its name uses the full row, and hovering a long name
  shows all of it (#339).
- Fixed: a second click on a menu button ("…" on sidebar rows and file headers, "Add", the
  arrow beside Fetch and Stash) now closes its menu instead of opening it again, also when the
  menu was opened by a right-click or a key (#339).

### An invalid core.ignorecase

- Fixed: a `core.ignorecase` that isn't true or false (or a config that can't be read) no
  longer counts as "case doesn't fold" on Linux, which skipped the check for names that differ
  only in case. Switching branches, merging, pulling, cherry-picking, rebasing, cloning, adding a
  worktree and updating submodules now stop with "Can't read core.ignorecase from the Git
  config: …". Every reset, discard, Abort and undo also stops, on every system, before it
  changes anything. Fix the setting, then try again (#225).
- Fixed: saving a stash, in every mode and with "Stash file…", now stops with the same message
  before it writes anything. Before, it could write the stash and then leave the files only half
  reset. Applying and popping a stash stop too, on every system, before any other check (#369).

### Worktree names on Windows

- Fixed: on Windows, a worktree whose name or folder ended in a dot or a space was created but
  couldn't be found, removed or rolled back from the app. Such names, and names Windows reserves
  for devices (`con`, `aux`, `nul`, `com1` and the like), are now refused before anything is
  created, with a message naming the part to change (#261).

### Hook cancel on macOS

- Changed: when a hook is cancelled or runs out of time, Browsitory now kills its process group
  a second time once the hook itself has exited. This closes a suspected race where a hook's
  background job could survive the cancel on macOS (#349).
- Internal: the hook timeout and cancel tests on Unix and Windows no longer depend on how fast
  the runner starts the hook's background job, which made them flaky on macOS CI (#349).

### Internal: security review leftovers

- Internal: the commands refused while another window holds the undo history are listed once,
  over every command, so the wait and the refusal can't disagree (#225).
- Internal: Mark resolved records a link's own stat data, so status doesn't read the link again
  on every refresh (#225).
- Internal: comments and docs on stat data, the document extension list and the undo lock are
  corrected, and tests check status right after Mark resolved (#225).

### Internal: late cancels no longer pile up

- Internal: a cancel for an operation that already finished (a late click on Cancel) is now
  dropped instead of being kept until the repository is closed. A late cancel for a clone is
  kept only among the last few, since the clone dialog can also cancel before its clone
  starts (#219).

### Internal: frontend tests under load

- Internal: frontend tests no longer time out on a busy machine (#217). A failed
  `findBy*`/`waitFor` retry no longer pretty-prints the whole document (5% of App.test.tsx's CPU
  time when idle, 15% under load). Testing Library waits up to 4 s instead of 1 s, the default
  test timeout is 10 s instead of 5 s, and App.test.tsx (whole-App renders) and the VSCode
  bootstrap test get 20 s.

### Internal: smaller source files

- Internal: the five largest files are split by feature, with no behaviour change (#307). The
  worker's and the desktop commands' inline tests moved to `worker/tests/` and
  `commands/tests/`, the sidecar's handlers to `dispatch/<feature>.rs`, `App.tsx`'s
  `RepoWorkspace` and its state hooks to their own files, and `useAppState.test.ts` to one test
  file per area.

### Internal: Marketplace publishing checks every platform package

- Internal: the release's Marketplace job publishes one platform package, waits until the
  Marketplace validated it, then publishes the rest, and fails if any target is still missing
  from the public gallery after 30 minutes. The Open VSX job runs the same check. Re-running the
  job uploads only the missing targets (`scripts/verify-marketplace-listing.py`, #344).

## [0.3.0] - 2026-11-05

The first public release. Browsitory ships as a desktop app and as a VSCode extension, available
on the Visual Studio Marketplace and Open VSX as a pre-release. This release adds commit search,
compare, git hooks, undo and redo, commit signing and a user guide website. Worktrees, and
rebases, cherry-picks and bisects you start in a terminal, are now handled much more safely.

### Rebases, cherry-picks and bisects started in a terminal

- Fixed: a `git rebase` started in a terminal and paused on a conflict showed as "Merging" with
  Abort merge, which reset the rebase's conflicted step. The bar now says "Rebasing topic, step
  2 of 3 (started in a terminal)" with what to run there, and has no buttons; `git am` shows as
  "Applying patches". Abort merge, cherry-pick, revert, a new rebase, reset and amend now say
  "A rebase started in a terminal isn't finished. Continue it there with git rebase --continue,
  or abort it with git rebase --abort." and change nothing. Abort merge refuses the merge step of
  a git rebase --rebase-merges too (aborting it dropped the merged side branch), and so a merge
  started by hand during the rebase; use git merge --abort there (#263).
- Fixed: committing in Browsitory during a terminal rebase now works like git commit. At a merge
  step of git rebase --rebase-merges it makes the merge commit (it made a plain commit, so git
  rebase --continue lost the side branch); at an empty pick it keeps the original author (#263).
- Fixed: the branch being rebased in a terminal could be deleted, renamed or checked out in a new
  worktree, and another worktree could switch to it (a commit made there was lost when the rebase
  was aborted). This is now refused, as git refuses it; Undo doesn't switch to it either. Stash and
  amend are disabled during the rebase (#263).
- Fixed: starting a rebase during a paused merge, cherry-pick or revert now refuses up front
  and names it ("A merge is in progress. Commit or abort it before starting a rebase."). A
  merge whose conflicts were resolved to the current side used to get through, and during a
  terminal rebase the refusal said only that HEAD was detached (#265).
- Fixed: a pending Undo entry is no longer dropped at each step of a rebase started in a
  terminal; the rebase counts as one operation until it ends (#264).
- Fixed: when a cherry-pick or revert of several commits started in a terminal paused on a
  conflict, committing the resolution in Browsitory dropped the remaining commits. They are now
  kept, as with git commit, so git cherry-pick --continue (or git revert --continue) carries on;
  the list is removed after its last commit. A squash merge in Browsitory no longer drops such
  a list either (#241).
- Fixed: committing a merge or cherry-pick no longer ends a git bisect started in a terminal
  (#241).
- Fixed: a cherry-pick or revert of several commits started in a terminal and paused between
  commits was invisible in Browsitory, and a cherry-pick, revert or rebase started in the app
  could mix with it or delete its list. A bar now says "Cherry-pick paused in a terminal. Run
  git cherry-pick --continue there to apply the remaining commits.", and cherry-pick, revert
  and rebase say "A cherry-pick started in a terminal isn't finished. …" until it is (#258).
- Changed: a single cherry-pick or revert in Browsitory is now refused while such a list is
  paused (git allows it). Merge, commit and reset still work and keep the list (#258).
- Fixed: Abort cherry-pick (or revert) on a cherry-pick of several commits started in a
  terminal ended the list but left the commits it had made. It now works like git cherry-pick
  --abort: after asking, it removes the commits it made and puts the branch back where it
  started. If HEAD moved since, it leaves HEAD and only ends the cherry-pick (#253).
- Like git, this Abort checks everything first and changes nothing when it would lose work: a
  changed or untracked file where putting the branch back would write (it names the file), a
  list that changed in the terminal since you confirmed, or damaged git files for the list
  (#253).
- Fixed: Abort merge (and Abort cherry-pick or revert) discarded a file you had staged and then
  changed again, losing both versions. It now refuses, as git merge --abort does: "Abort would
  discard your changes to 'f'. …". This also applies to a staged file replaced by a symlink.
- Fixed: Abort (of a merge, cherry-pick, revert or several-commit cherry-pick) and the end of a
  rebase ended a git bisect started in a terminal. They now remove only their own state, as git
  does, so the bisect goes on. They no longer delete a terminal rebase's state, nor the rest of
  a terminal `git cherry-pick A B C` (only aborting its last step ends it, as with git). Reset
  (mixed or hard) keeps them too (#249).
- Fixed: Abort of a merge or cherry-pick started in a terminal after Browsitory last refreshed
  could leave the conflicted files with their markers. Abort now reads the index from disk
  first.
- Fixed: during a git bisect started in a terminal, cherry-pick, revert, reset, amend, Undo and
  Redo said "Finish or abort the current merge/cherry-pick/revert/rebase first." and a rebase
  showed as interrupted. They now work as with git (#250).
- Added: a bar shows a bisect started in a terminal: "Bisecting (started from main). Use git
  bisect good, bad or reset in a terminal." (#250).
- If Abort can't remove one of git's merge state files, it now says "Couldn't remove git's merge
  state: …" instead of "failed to write resolved file: …" (#249).

### VSCode extension on the Marketplace and Open VSX (pre-release)

- Added: release tags now publish the VSCode extension to the Visual Studio Marketplace and to
  Open VSX (for VSCodium, Cursor and similar editors), one package per platform. It ships as a
  pre-release for now. Each registry is skipped, with a notice, until its credentials are set
  up (#271).
- Added: the extension's listing has an icon, a feature list with screenshots, links to the
  repository and the User Guide, a changelog link, and a `THIRD_PARTY_NOTICES.md` with the
  licence texts of libgit2 and the other native libraries the bundled helper program links
  (#271).
- Docs: install steps for the Marketplace, Open VSX and a `.vsix` file in the User Guide and the
  README (#271).
- Added: the VSCode extension. It shows the same interface as the desktop app in a restricted
  webview, and uses VSCode itself to pick folders, open links and report its version. The rest
  runs in a bundled helper program (`vscode-sidecar`). If the helper stops, pending actions fail
  with a message in the app, and the next action starts a fresh helper without repeating
  earlier changes.
- Added: one VSIX package per platform (Linux x64, macOS x64 and arm64, Windows x64), each with
  the matching helper program. Tag releases attach all four to the GitHub release.
- Fixed: a slow operation in one open repository (a large blame, a big commit graph) no longer
  freezes the extension's other open repositories. The helper now handles each request on its
  own thread.
- Fixed: uncaught errors in the extension's webview are now written to the log, as in the
  desktop app. They were silently dropped before.
- Added: fetch, pull and push show their progress in VSCode, as in the desktop app.
- Fixed: in VSCode, a request the extension doesn't handle now gets an error reply instead of
  never being answered (#306).
- Internal: the webview forwards its `RepoClient` calls to `vscode-sidecar` over JSON-RPC, and
  error logging goes through `RepoClient` instead of a Tauri-only plugin. Main-branch CI keeps all
  four VSIX packages as artifacts.

### Commit search

- A search field above the commit graph (**Ctrl/Cmd+F** on the commit list, or "Search commits"
  in the palette) finds commits by message or SHA, `author:`, `committer:`, `message:`,
  `commit:`, `path:`, `contents:` (added or removed lines), `after:`/`before:` dates and
  `branch:`. Terms combine with AND; quote text with spaces or a colon.
- Results replace the graph, load 100 at a time with **Load more**, and show how many commits
  were scanned; Esc or × returns to the graph and your previous selection.
- A slow search (`path:`, `contents:`) works in short slices, so committing or staging while it
  runs is not held up; a row you pick while results load keeps its selection.

### Compare

- **Compare these commits** (two selected commits, from the selection bar or the right-click
  menu) shows every file that differs between them in the diff pane, the older commit on the
  left, with **Swap**. Renamed files are listed once with only their edits.
- **Compare with current branch** on a branch (local or remote-tracking) shows what it changed
  since it diverged; **Diff against working tree** on a commit compares it with your files on
  disk, untracked files included.
- Compare views are read-only, with Blame and History on each file. Clicking any commit or
  stash row (or ×) closes the compare.
- Comparing a commit with the working tree shows the changes of a conflicted file, conflict
  markers included (#117).

### Git hooks

- Commits and amends run your repository's `pre-commit`, `prepare-commit-msg`, `commit-msg` and
  `post-commit` hooks; every push (branch, force push, tags, **Push to…**, deleting a remote
  branch) runs `pre-push`; switching branches and checking out a commit run `post-checkout`;
  merging and a fast-forward pull run `post-merge`. `core.hooksPath` is honored.
- A failing hook stops the commit or push and shows its output, with **Commit without hooks** /
  **Amend without hooks** / **Push without hooks** to retry (`--no-verify`). A **Skip hooks**
  checkbox under the commit message (shown when a `pre-commit` or `commit-msg` hook is installed) or
  "Skip hooks for the next commit" in the palette skips them for the next commit.
- While a hook runs, "Running pre-commit hook…" (or "Running pre-push hook…" in the transfer
  panel) shows with a Cancel button; a hook is stopped after 10 minutes. On Windows hooks run
  with Git for Windows' `sh`; without it a blocking hook fails with a message saying so.
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
- Merging a branch now runs the pre-merge-commit hook (skipped by Skip hooks and "Commit without
  hooks") instead of pre-commit when you commit a merge that had no conflicts, and that commit no
  longer runs post-commit, as with git merge; post-merge runs after that commit instead of when the
  merge is staged. A merge that had conflicts runs the commit hooks (including post-commit), as in
  git (#124).
- Fixed: post-merge ran when a merge without conflicts was only staged, before its merge commit
  existed; it now runs after that commit, as in git (#124).
- Continue on a paused multi-commit cherry-pick or revert runs pre-commit, prepare-commit-msg,
  commit-msg and post-commit for the step it commits, so a Gerrit Change-Id hook works. A failing
  hook keeps the step paused and offers "Continue without hooks", and a running hook shows on the
  panel with Cancel (#133).
- Fixed: started from a terminal, a hook (or signing program) that reads the terminal no longer
  hangs until Cancel or the time limit; it fails at once, as when Browsitory is started from the
  desktop (#126).
- Fixed (Windows): Cancel and the time limit now also stop the programs a hook started, not just its
  shell (#122).
- Fixed (desktop app): a long-running hook, signing or transfer could stall other actions, including
  Cancel, across all open repositories (#125).
- After you commit a merge that had no conflicts, a running post-merge hook shows under the commit
  message with Cancel, like post-commit; cancelling stops the hook and keeps the commit (#178).
- Fixed: Cancel on the "Running post-checkout hook…" strip (and on a commit hook's status)
  could do nothing when you clicked it right as it appeared. The button then stayed on
  "Cancelling…" until the hook ended by itself. Cancel now always reaches the hook. (#182)

### Worktrees

- Fixed: when the folder of one worktree had been deleted, removing any worktree failed with
  "git operation failed". Other worktrees no longer get in the way. Removing a worktree whose
  folder is gone now works too, as `git worktree remove` does: it drops the worktree from the
  list and frees its branch. A locked worktree is still kept (#252).
- Fixed: if a worktree's folder had been replaced, for example by another repository cloned to
  the same place, Remove could delete that folder. Browsitory now deletes a folder only when it
  still belongs to the worktree, as `git worktree remove` does. Otherwise it keeps the folder
  and says so: "The folder '…' is no longer this worktree. Move it away or delete it, then
  remove the worktree again." (#252)
- Fixed: one worktree with a broken entry, or with a file where its folder's parent was, no
  longer hides the whole worktree list. When two worktree entries point to the same folder,
  both are listed, and Remove acts on the one you chose (#252).
- Fixed: worktrees of a bare repository can now be listed and removed (#252).
- Fixed: a relative worktree path (for example `../feature`) was taken from the app's own
  working folder, which you can't see, and could create folders there. It now starts from the
  top folder of the open repository, as `git worktree add` run there does. In a bare
  repository, enter the full path (#231).
- Changed: a worktree can no longer be created inside the repository's `.git` folder, where
  git keeps its own data. git allows this, but nothing good comes of it. On Windows, a path
  like `C:feature` or `\feature`, which depends on the app's current drive or folder, is
  refused with a request for the full path (#231).
- Creating a worktree in a folder that doesn't exist yet now creates the missing folders, as git
  worktree add does (#200).
- Fixed: a worktree add that failed could leave a half-made worktree entry and the new branch
  behind, so trying again with the same name failed with "git operation failed". When an add
  fails before the worktree is set up, Browsitory now removes the folders, entry and branch it
  created, and a folder it can't create is named in the error (#200).
- Fixed: a worktree add that failed while checking out the files (a full disk, or on Windows a
  file name such as aux or a path that's too long) left the worktree registered with its branch,
  so it could neither be added again nor removed. Browsitory now removes the files it checked
  out, the folder, the worktree entry and the new branch, and says why it failed instead of "git
  operation failed"; if something else is in the folder, the folder is kept and named (#245).
- Fixed: when adding a worktree failed part-way, files the checkout had already written were
  not removed if their names weren't valid UTF-8 (possible on Linux). The new folder was then
  kept and you had to delete it by hand. Those files are now removed like any other, and so is
  the folder (#262).

### User guide and documentation

- Added: a user guide website, <https://rene-langer.github.io/browsitory/>, with one page per
  topic, navigation and search. **Open user guide** in the command palette and **User guide**
  in the release notes open it in your browser, in the desktop app and in VSCode (#320).
- Added: the guide now covers the theme switch, the release notes, the update banner,
  **Expand all sections** and **Collapse all sections**, choosing which branches the commit
  graph shows, **Split view**, and the palette's commands and keyboard shortcuts (#320).
- Fixed: the guide described some controls wrongly, for example the upstream buttons at the
  bottom of the Branches section, a "Conflicted" group that doesn't exist, and the names of the
  squash, fixup and drop buttons (#320).
- Changed: "Pull Requests", "Uncommitted Changes", "Release Notes", "New Workspace",
  "Edit Workspace" and "Choose Root Folder" are now written in sentence case, like the rest of
  the app. Recent commands in the palette keep working (#320).
- Fixed: with no GitHub or Bitbucket remote, the Pull requests section pointed to a Remotes
  section that no longer exists. It now points to **Add remote** in the Branches section (#320).
- Changed: the README, the VSCode Marketplace page and the screenshots were rewritten for 0.3.0
  (#320).
- Internal: the screenshots come from `e2e/screenshots/capture.sh`, which builds a fixed demo
  repository and drives the app through the E2E harness. The site is built with MkDocs
  (`mkdocs.yml`, `.github/workflows/docs.yml`); build and contributor notes moved to
  `CONTRIBUTING.md` (#320).

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
- Fixed: if Undo or Redo failed while writing a file back (for example a full disk), the file
  was left half-written and its previous content was gone. Browsitory now writes the restored
  file next to it and swaps it in only when it is complete, so a failure leaves the file as it
  was (#251).
- Fixed: after such a failure, running Undo or Redo again said the repository had changed.
  It now finishes the restore (#251).
- Windows: if another program keeps the file open so it can't be swapped, Undo and Redo now say
  "'f.txt' is open in another program. Close it, then try again." This also applies to a
  read-only file (#251).

### Commit signing

- Commit signing: when `commit.gpgSign` is set, commits, amends, merges, cherry-picks, reverts and
  rebased commits are signed with gpg (`gpg.format=openpgp`) or SSH (`gpg.format=ssh`), using
  your existing gpg-agent / ssh-agent. If signing fails, no commit is made.
- The commit details pane shows the signature status of signed commits (good, untrusted, unknown
  key, expired, bad, revoked).
- Committing or amending with a signing program that never finishes (for example a passphrase
  prompt that never appears) no longer blocks the repository: the commit box shows "Waiting for
  the signing program…" with **Cancel**, and signing stops after 2 minutes with an error that
  says so. Cherry-pick, revert and rebase commits get the same 2-minute limit, and checking a
  commit's signature in the history gives up after 30 seconds (#110).

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

### Line staging, diff options and discard

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
- "Drop stash" in the command palette dropped the stash without asking; it now shows the stash
  and opens the Stashes section, where Drop asks first (#105).
- Stashes moved out of the Branches sidebar section into their own "Stashes" accordion section, with
  a confirmation before dropping one.

### Refs, remotes, fetch and push

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
- Push and force push follow `push.default` (unset = `simple`). A branch whose upstream has a
  different name (e.g. `feat` created from `origin/main`) is no longer pushed onto that
  branch: under `simple` Push opens **Push to…** with the remote and the branch's own name;
  with `push.default=upstream` it updates the upstream branch, with `current` the same-named one.
- Fixed: the add-remote form's "Push URL (optional)" section could reopen right after a
  successful add instead of collapsing (#276).
- Transfer progress shows MB/GB instead of ever-larger KB values, and the transfer panel now has
  a working Cancel button. Cancel stops a fetch or pull even while it waits on the network. A
  push can only be cancelled before its data is sent; a cancel pressed later lets the push
  finish and report its real outcome, since the remote may already have applied it. A cancelled
  transfer shows a "Fetch/Pull/Push cancelled." notice rather than a failure message.
- Internal: (PERF-002) cancelling aborts the in-flight `git2` transfer itself: a shared registry
  of cancelled operation IDs is polled from inside the fetch progress callbacks. A push is
  cancellable only up to `push_negotiation`. New `TransferErrorKind::Cancelled` and a
  `cancel_transfer` command on both the Tauri and VSCode transports.
- Pulling a fast-forward no longer force-checks out over untracked or ignored files: the
  checkout is now safe and aborts untouched with a "would be overwritten" error, and the pull
  surfaces its dirty-worktree, no-upstream, detached-HEAD and checkout-conflict reasons instead
  of a generic "pull failed".

### Conflicts and multi-commit cherry-picks

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

### Interactive rebase from selected commits

- Ctrl/Cmd+click commits in the graph to select several, including non-adjacent ones (Shift
  selects a range). A toolbar and the context menu offer Interactive rebase…, Squash, Fixup and
  Drop, each opening the rebase planner with the selection pre-marked. Squash and Fixup require
  adjacent commits; Drop and Interactive rebase… accept any selection.

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

### Image diff, commit message and co-authors

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
- The co-author picker opens above its button when there is no room below, stays inside the window,
  keeps its side while you filter, and closes when its button scrolls out of view (#141).
- VS Code: a large image preview no longer freezes the extension host for seconds while its
  data arrives from the backend.
- The image preview no longer hangs the repository if a working-tree file is replaced by a named
  pipe while it is being read.

### Commit graph, branch tree and sync bar

- Selecting a commit now shows a header with its full message, author, date, full SHA (copyable)
  and parents (click to jump). History rows also show author and date columns that hide when
  the pane is narrow.
- History pagination: the graph loads 300 commits at a time with a "Load more" row, and
  arrowing past the last row loads the next page instead of silently stopping.
- Sync bar above the commit graph with Fetch, Pull and Push buttons and ahead/behind counts
  (shown once the upstream branch has been fetched).
- "Checkout" is now the first item of a local branch's context menu; the user guide documents
  double-click checkout too.
- The "Uncommitted Changes" row now has a hollow lane node and aligns with commit rows.
- Internal: the commit header is backed by a new `get_commit_message` IPC command wired through
  git-core, repo-service, tauri-app and the VSCode sidecar (UX-001). `UpstreamInfo` gains
  optional `ahead`/`behind`, null until the tracking ref is fetched (UX-008). Also UX-002, UX-003
  and VIS-004.
- Branch tree: local branches get an "Actions" button and a bold current-branch marker; inline
  new-branch/add-remote/rename forms focus on open and cancel on Escape; remote lists show
  "Loading..." and "No branches"; the Upstream block is styled, explains a disabled Pull, and
  confirms before clearing the upstream.
- Commit graph keyboard access: Enter, the Menu key or Shift+F10 open the commit menu; Home/End,
  PageUp/PageDown navigate; Shift+Arrow extends the squash range.
- Command palette groups commands by kind, uses the UI font size and shows key hints; the header
  shows a Ctrl/Cmd+K hint; the empty-state repository picker is a styled row list.
- The branch tree (local and each remote) now nests branches into folders split on each `/` in
  their name, matching a filesystem-style tree view. Checkout is now double-click instead of
  single-click (single click just selects/highlights a row); the graph-visibility control is now
  a per-branch colored swatch button instead of a checkbox.
- Remote-tracking branch tips now show inline in the commit graph as a dashed-border badge next to
  the existing solid local-branch badge, with their own per-remote-branch graph-visibility toggle
  in the branch tree.
- Pressing Enter or Space on a branch row's graph-visibility swatch or "..." button no longer
  checks the branch out; key events from controls nested in a row stay with that control.
- Branch tree rows stay on one line with truncating names (full name in the tooltip), the "+"
  button moved into the Branches header, and the graph-visibility swatch has a larger hit area
  and a hollow "hidden" state.
- The commit graph's lane lines could render broken: a lane-shifting connector could be cut by
  a later-painted, unrelated pass-through line crossing it, a pass-through lane's straight
  continuation could be dropped entirely when another commit's connector happened to land on
  the same lane, and even once both were fixed, a straight pass-through line split into two
  segments at the row's midpoint could still show a hairline rendering seam. Connectors are now
  painted last, and each pass-through lane is drawn as a single unbroken line.
- Internal: the lane fix is in `CommitLaneGraphic`.
- A saved graph branch selection naming a deleted or renamed branch no longer breaks the whole
  refresh; deleting or renaming a branch in the app now updates the selection, and deleting the
  only isolated branch shows all branches again (#97).
- Each branch's context menu in the Branches tree gained "Isolate branch", a one-click way to filter
  the commit graph down to just that branch; the tree's "+" menu gained "Show all branches" to clear
  the filter.
- The separate branch-switcher dropdown and Remotes accordion were replaced by a single,
  always-expanded Branches tree: local branches and one folder per remote, with every mutating
  action (create/rename/delete a branch, merge, add/edit/remove a remote, set upstream, manage
  credentials, push/fetch) moved to right-click context menus and the command palette.

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
- A gear popover in the sidebar lets each panel (Stashes, Worktrees, Submodules, Reflog, Tags, Pull
  Requests) be shown or hidden. The choice applies to every repository and open tab, so
  clutter from features a project doesn't use can be tucked away.
- Every remaining sidebar section (Worktrees, Submodules, Reflog, Remotes, Tags, Pull Requests)
  uses the new design: consistent row styling, per-row icons, and empty states.
- Internal: the panel choice is one `localStorage` setting; the sidebar reskin followed the
  Phase 5 design system.

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

### Third-party licences

- Added: the app now lists the licences and copyright notices of the open source code it
  includes. Open "Third-party licences" from the command palette or from the release notes
  dialog. The same text ships as `THIRD_PARTY_NOTICES.md` with every desktop installer and
  every VSCode extension package, and is attached to each GitHub Release (#301, #202).
- Changed: C++ files (`.cpp`, `.cc`, `.hpp`) are now highlighted with the C grammar. Shiki's
  C++ grammar includes a grammar that has no licence, so Browsitory no longer ships it. Most
  C++ code looks the same; C++-only keywords and syntax may stay plain.

### Release notes in the app

- Fixed: the release notes in the app left out every changelog section with a topic heading,
  such as "Commit signing" or "Undo and redo", so most of 0.3.0 was missing. Each topic now
  shows as its own group, and a "Fixed", "Added" or "Changed" label marks each note. Notes
  for developers only are left out (#335).
- Release notes no longer truncate a changelog bullet that wraps across lines, and wrapped diff
  lines get a hanging indent.

### Paused rebases and sequences

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
- Fixed: starting a rebase over uncommitted changes could discard them (when the first step
  failed, and on Abort). A rebase now refuses to start while tracked files have staged or
  unstaged changes (including an edited skip-worktree file), as git does; untracked files are
  fine (#170).
- The undo entry of a rebase names the commit it was rebased onto by its short id, and a rebase
  start that failed but still changed something is listed as "Rebase onto … failed (rolled
  back)" (#170).
- While a cherry-pick/revert sequence is paused, deleting, renaming or adding a worktree on its
  branch and saving a stash are refused, as during a rebase (#171).

### Desktop app and VSCode open on one repository

- Fixed: with the desktop app and the VSCode extension open on one repository, finishing a
  paused operation in one could lose its undo entry, and two saves could drop an entry. Undo
  history changes now take a lock in .git/browsitory (#172). If the other window keeps it for
  more than about 10 seconds, Undo, Redo, discards and a hard reset ask you to try again rather
  than run without an undo entry; other operations run but can't be undone.
- Fixed: a merge, cherry-pick or revert aborted outside Browsitory and started again with the
  same commits reused the first one's undo snapshot (#172).
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
- Fixed: while the other window keeps the undo history busy, a quick run of actions (for
  example staging files one by one) waits about a second once, not a second per action. Undo,
  Redo, discards and Reset hard still wait about a second each before they refuse (#197).

### Operations that stop part-way can be tried again

- Fixed: when a cherry-pick, revert, rebase start, pull or stash apply (or pop) stopped
  part-way, for example at a file another program kept open on Windows or at a read-only
  file, the files already written stayed changed. Trying again was then refused ("Commit or
  unstage your staged changes first", local changes, a dirty work tree, or local changes the
  stash would overwrite). These operations now put those files back, so nothing is left
  changed and trying again works. On Windows they also name the file: "'f.txt' is open in
  another program. Close it, then try again." Your own changes to other files are kept, and a
  failed pop keeps the stash (#321).
- Fixed: when aborting a merge, cherry-pick or revert stopped part-way, aborting again ended
  it but left the files it hadn't reached with the merge's changes. Aborting again now puts
  them back too. On Windows the message names the file: "'f.txt' is open in another program.
  Some files were already put back. Close it, then abort again to finish." (#321)
- Fixed: on Windows, a rebase abort stopped by a file another program kept open said "Some
  files were already put back" even when it hadn't changed anything yet. It now says only
  "Close it, then try again." in that case (#321).

### Leftover merge, cherry-pick and revert state

Sometimes git's notes about a merge, cherry-pick or revert stay behind although its changes
are already in the last commit: the .git folder couldn't be written right after the commit, or
you cherry-picked a commit again in a terminal and git stopped because there was nothing left
to apply. Browsitory now recognises these leftovers. A merge, cherry-pick or revert with
unresolved conflicts is never one.

- Fixed: "Amend last commit" was hidden, and amend refused, while such a leftover was there.
  Amend now works and clears the leftover first. An operation that is really still in progress
  still blocks amend, as in git (#240).
- Fixed: after a cherry-pick in a terminal stopped because its changes were already there,
  your next commit or amend ends it the way `git cherry-pick --skip` does. If more commits were
  left to pick, `git cherry-pick --continue` goes on with them.
- Fixed: after a leftover revert, the commit box was pre-filled with the revert's message and
  the `prepare-commit-msg` hook was told the next commit was a merge. The next commit is now
  your own: the box starts empty, the hook gets `message`, and the leftover is cleared
  (#232). The same goes for a leftover cherry-pick or merge.
- Changed: for a leftover, the status strip says "Cherry-pick already applied" (or revert, or
  merge) instead of "Cherry-picking", that its changes are already in the last commit, and that
  your next commit, an amend or Abort clears it. Disabled actions explain the same.
- Fixed: when git's cherry-pick state couldn't be cleared after a cherry-pick was committed,
  your next commit was authored as the picked commit's author. Browsitory now recognises the
  leftover, clears it and commits as you; if it can't be cleared, the commit is refused with
  "This cherry-pick was already committed, but git's cherry-pick state can't be cleared. Check
  the permissions of the .git folder, then commit again." (#226).

### Branches in use by other worktrees

- Fixed: Switch failed in a linked worktree of a bare repository. It now reads the other
  worktrees from the repository's git folder, as Reflog restore and Undo do.
- Fixed: the branch a git bisect started from could be moved, deleted, renamed or checked out
  in a new worktree, so git bisect reset returned to a changed or missing branch, or checked it
  out twice. Reflog restore, Undo, branch delete and rename and worktree add now refuse it, in
  this worktree and from the others, and Switch refuses it from another worktree, as git does
  ("branch 'topic' is being bisected in this worktree").
- Fixed: Reflog restore, Undo and branch delete moved or deleted a branch that a git rebase
  --update-refs in another worktree will update. They now refuse it, as git branch -f and -D
  do.
- Fixed: a rebase paused in Browsitory protected its branch only in its own worktree. The other
  worktrees now refuse to move, delete, rename, switch to or add a worktree on that branch too
  ("branch 'topic' is being rebased in another worktree at '…'"). Only a rebase that worktree
  would load itself counts, so a damaged or leftover rebase file blocks nothing.
- Changed: these refusals name the other worktree's folder, as git does ("branch 'topic' is
  checked out in another worktree at '/work/topic'"). When that folder is gone, or is no
  longer a worktree, they say to run git worktree prune. If that worktree is locked, they say
  so instead, because git worktree prune keeps locked worktrees.
- Changed: Undo names these reasons too ("Can't undo Commit: branch 'topic' is …"), instead of
  "the repository changed since", and keeps the step so you can try again (#291).
- Fixed: a cherry-pick or revert paused in Browsitory protected its branch from renaming only
  in its own worktree. Another worktree's window could rename it, and Abort then left the
  picked commits in place instead of putting the branch back. The other worktrees now refuse
  to rename it, unlike git, whose abort doesn't depend on the branch name (#293).
- Changed: moving, deleting or switching to that branch from another worktree, or adding a
  worktree on it, was already refused because it is checked out there. The message now names
  the paused sequence ("branch 'topic' has a paused cherry-pick in another worktree at '…'").
  Only a sequence that worktree would load itself counts, and only while its HEAD is on that
  branch.
- Fixed: after the branch of a paused cherry-pick or revert was renamed in a terminal (`git
  branch -m`, which git allows), Abort left the picked commits in place. It now puts the
  renamed branch back, as `git cherry-pick --abort` does. It recognises the rename by the
  branch's reflog entries written during the sequence, so a branch you switched to, or one
  renamed before the sequence started, is not rewound.
- Fixed: Reflog restore moved a branch that another worktree had checked out, so that
  worktree's files and staged changes no longer matched its branch. It also moved a branch that
  a rebase started in a terminal was rebasing, in any worktree; git rebase --abort then undid
  the restore. Both are now refused, as git branch -f refuses them ("branch 'topic' is checked
  out in another worktree"; "topic is being rebased by a rebase started in a terminal. …").
  A worktree whose folder is missing (a locked one on an unmounted drive) still counts, as for
  git; this applies to Undo too. Restoring the branch checked out in this worktree works as
  before, but like Reset it is now refused while a merge, cherry-pick, revert or rebase is
  paused: during a terminal rebase it made git rebase --continue drop the steps already
  replayed, and during a merge it changed the merge's first parent. The confirmation now says
  that your files and staged changes stay as they are. A branch moved by another program while
  it is being restored is left alone (#287).

### Stash fixes

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
- Fixed: Stash all lost your edits to files in a folder you had removed from the index
  (`git rm -r --cached d`, without ignoring it). Without Include untracked files Pop deleted
  them, with Keep staged changes they were deleted at once, and with Include untracked files
  Pop wrote conflict markers and said it would overwrite local changes (also for a single file
  removed this way). The edits are now stashed and Pop brings them back; with Restore staged
  changes the removal from the index comes back too (#247).
- Fixed: after stashing a file removed from the index (Stash file…, or Stash all when the file
  wasn't edited or was one of several files in a removed folder), its removal stayed staged, so
  Pop said "Commit or unstage your staged changes before applying a stash." The stash now puts
  the file back in the index, so Pop works (#248).
- Fixed: Stash file… on a file with merge conflicts (and Stash all in the cases above) stashed
  the conflict markers and dropped the conflict from the index while the merge was still in
  progress, so the next commit lost the other side. Stashing now says "Resolve the current
  conflicts before stashing." and changes nothing, as git does (#247).
- Known limitation: in these cases Stash all works like Stash file… on every change, so it also
  refuses a tracked file that is now a folder ("Can't stash 't': it was a file and is now a
  directory. Commit or move its contents first.") (#247).
- Fixed: Apply and Pop now refuse a stash whose untracked file `d/x` needs a folder where a
  tracked file differs only in case (`D`). This applies when the repository sets
  `core.ignorecase`, and always on macOS and Windows, even if a copied repository says
  otherwise. On a case-insensitive drive `d` and `D` are the same name, so the stash could not be
  applied cleanly, and git refuses it too. On a case-sensitive drive with `core.ignorecase` set,
  git would apply it; Browsitory refuses on purpose, to be safe. Nothing is written and the
  stash is kept. (#234)

### Files and folders that replace each other

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
- Discarding or stashing a file whose folder was replaced by a file now names the file in the way,
  and Discard all with both succeeds. Discarding a file that was replaced by a folder refuses when
  the folder holds something Undo can't bring back (an ignored file or a nested repository) (#154).
- When a folder and a file or symbolic link replace each other, Stage all works in any order (a file
  below a link stages as its deletion), and unstaging any of them, the folder's name included, puts
  the whole swap back as it was in the last commit (#157).
- The diff of a file that became a folder (or the reverse) no longer shows the folder's files,
  and staging, unstaging or discarding one of its hunks or lines can no longer pick a hunk of a
  file inside that folder (#116).

### Mark resolved on folders and nested repositories

- Fixed: when a conflicted path had become a folder, Mark resolved opened it again by its path
  before staging it. Something swapped in at that moment could hang Browsitory (a FIFO) or be
  staged in its place (a link, or another repository). Browsitory now holds the folder open and
  reads the nested repository's commit through it, never by path. A FIFO or link inside the
  nested repository is not opened or followed either, and its files are read only up to a size
  limit (#213).
- Fixed: Mark resolved on a file below a folder that has become a file (for example `a/b/c.txt`
  where `a` is now a file) failed with "Not a directory". It now stages the deletion, as
  `git add` does. Discard hunk and Discard lines there now say which file is in the way, as
  Discard does, instead of a raw error (#213).
- Changed: a nested repository is still staged as its checked-out commit, as `git add` does,
  also when it is a submodule (its git folder in `.git/modules`, nested ones included) or a
  linked worktree of a repository in the work tree. Browsitory never opens anything outside the
  work tree for this. A repository whose git folder is elsewhere, or at a path Browsitory can't
  match to the work tree, is refused; stage it with `git add <path>` in a terminal. That
  includes a linked worktree of a repository outside the work tree, and one git wrote with a
  path the work tree wasn't opened by (for example macOS `/private/var` for `/var`) (#213).
- Changed: when Browsitory can't stage the folder, it now says why instead of showing a raw
  git error, and how to stage it with git: the folder isn't a repository, the repository has no
  commit yet, its git folder is outside the work tree or can't be matched to it, a linked
  worktree's main repository can't be found, or Browsitory can't read it (for example a
  repository in one of the newer formats git supports, or one with damaged or unusual branch
  data) (#213).
- Internal: "can't read it" covers SHA-256 and reftable repositories, ref names git refuses, and
  a ref or `packed-refs` that isn't a regular file (#213).

### Files open in another program (Windows)

- Fixed: on Windows, a branch switch or commit checkout, a hard reset, a fast-forward merge, a
  rebase abort, a clone and a submodule update showed libgit2's raw error when another
  program kept a file open: "could not remove 'C:/…/f.txt': The process cannot access the file
  because it is being used by another process." They now name the file and say how to
  finish, for example "'f.txt' is open in another program. Some files were already switched.
  Close it, then try again to finish." All but a clone also say that some files were already
  changed; a failed clone removes its folder, so it says "Close it, then try again." (#303).
- Fixed: on Windows, an undo or redo that puts back a file's staged content, a discard of a
  file's changes, and taking one side's whole file in a conflict showed libgit2's raw "could
  not remove … being used by another process" error when another program kept the file open.
  They now say "'f.txt' is open in another program. Close it, then try again.", as the other
  writes do (#268). Closing the file and trying again finishes it.
- Fixed: an undo or redo that stopped part-way through putting back staged content (a file
  open in another program) refused to try again with `core.autocrlf` on, calling the files it
  had already written changed. Trying again now finishes it (#268).

### Safe file writes and read-only files

- Fixed: if writing a resolved conflict or .gitignore failed partway (for example a full
  disk), the file was left empty or half-written. Browsitory now writes a new copy next to it
  and swaps it in only when it is complete, so a failure leaves the file as it was (#239).
- Windows: if another program keeps the file open so it can't be swapped, Browsitory now says
  "'f.txt' is open in another program. Close it, then try again." and changes nothing (#239).
- Changed: because the file is now replaced rather than rewritten, you need permission to
  create files in its folder (and on Windows to delete the old file). A file that is a hard
  link gets its own copy: its other names keep the old content. If Browsitory stops in the
  middle of a write (a crash or a power cut), a `.f.txt.browsitory-….tmp` file (hidden on
  macOS and Linux) can be left next to the file; it is safe to delete (#239).
- Fixed: resolving a conflict or adding a pattern to .gitignore failed with "Access is denied"
  (Windows) or "Permission denied" (macOS, Linux) when the file was read-only, and Use ours /
  Use theirs (whole file) replaced the read-only file. They now say "'f.txt' is read-only. Make
  it writable, then try again." and change nothing (#209).
- Fixed: Browsitory's own settings file is now written to a new copy and swapped in, so two
  saves at once can no longer leave it corrupted.
- Internal: `config.toml` writes are atomic (write-temp-then-rename) instead of an in-place
  `fs::write`, which could interleave with a concurrent save and leave trailing bytes from the
  prior write.

### Windows hardening

- Fixed (Windows): a hook that starts a program right away could leave it running after Cancel or
  the time limit; hooks now start only once they are in their job (#180).
- Fixed (Windows): undo, redo and discards no longer fail when another program has the file open
  with delete sharing, as Microsoft Defender and the search indexer do. This needs NTFS or ReFS;
  FAT/exFAT drives and network shares without POSIX deletes behave as before (#168).
- Windows: a missing folder is created through the folder Browsitory already holds, so a folder
  swapped for a junction at that moment can't get an empty folder created outside the repository
  (#163).
- Internal: tests cover folders turned into junctions during a write (#164).
- Fixed (Windows): undo, redo and discards no longer fail with "Access is denied" on read-only
  files in the working tree (#194).
- Windows: every file and folder of a working-tree read, write or delete is opened through the
  folder Browsitory already holds, so on a local drive a folder swapped for a junction or
  symbolic link at that moment can't make Browsitory open a file in the link's target, briefly
  create one there, or contact a network share it points to (#193).

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
- Fixed: resolving a conflict now checks that the file really has a conflict before it writes
  anything. Before, any path, even one inside the `.git` folder, could be written and staged.
- Fixed: updated the TLS library to pick up the fix for RUSTSEC-2026-0285 (TLS 1.3 handshake
  messages accepted across encryption-level boundaries).
- Fixed: updated the desktop app updater's zip library to fix three security advisories
  (RUSTSEC-2026-0367, -0368 and -0369) (#81).
- Changed: in VSCode, the extension's webview can no longer call the three operations of the
  helper program that are meant only for the extension itself. Such a request now gets an error
  reply and never reaches the helper (#306).
- Internal: `resolve_conflict` validates that the path is an index conflict; `rustls` is now
  0.23.45.
- Fixed: Mark resolved stages exactly the content it checked for conflict markers, read without
  following a symbolic link or waiting on a named pipe; a file swapped at that moment can no
  longer hang it or get other content staged (#199).
- Fixed: Open in editor (no editor configured) no longer refuses a document such as notes.txt or
  README.md that quotes a "[Desktop Entry]" launcher near its start. On Linux, files whose
  extension marks them as a document are not checked for launcher contents, as the Linux
  desktops do; macOS and Windows no longer check file contents for Linux launchers (.desktop
  and .kdelnk files are still refused by name everywhere) (#201).
- Mark resolved keeps the mode git would keep for a file deleted on the current branch, and says
  "'x' can't be read safely; check its permissions and try again." for a file it can't open
  instead of calling it a FIFO, socket or device (#199).
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
- With HEAD already holding a link and a folder whose names differ only in case, unrelated
  merges, cherry-picks, reverts, stash applies, pulls, rebases and switches to a branch with the
  same pair now work on case-sensitive file systems; only a new collision, or the same one with
  its link changed, is refused. On macOS and Windows, where the link replaces the folder on disk,
  the on-disk link check still refuses them (#167).
- Rebase abort and cherry-pick/revert sequence abort are no longer refused when the tree they
  return to holds such a case collision (#161).

### Pull requests trust your system's certificates

- Changed: the GitHub and Bitbucket pull-request features now check server certificates against
  the operating system's certificate store, not a list bundled into the app. A root CA your
  system trusts (for example a company proxy's) now works there too.
- Internal: reqwest 0.13; TLS still uses rustls with the ring crypto library.

### Open repositories and background work

- Fixed: a repository opened through two different paths (a trailing slash, a subfolder, a
  symbolic link, or different letter case on Windows or macOS), for example from a worktree's
  Open button, got two background workers, so actions in the two tabs could run at the same
  time. They now share one worker, and closing one tab keeps the other working. The two tabs
  also share the undo history, and a new search in one stops a search still running in the
  other (#223).
- Fixed (desktop app): while a hook ran for a long time, repeated refreshes could use up the
  app's background threads, and actions in every other open repository then hung until the hook
  ended. Actions waiting for one repository now share one background thread (#181).
- A worker thread that panics is now detected and evicted, so reopening the affected repository
  respawns a fresh worker instead of leaving the repository permanently stuck.
- A worker-thread failure now surfaces as "connection to this repository was lost, reopen it"
  instead of the raw internal error string.

### Feedback, states and polish

- A persistent status strip now shows while a merge or rebase is in progress (step, conflict
  count, Abort), and Commit / Continue rebase show a visible reason when disabled.
- Success toasts (polite live region) after commit, checkout, branch delete, stash, fetch, push
  and pull. Error banners float instead of shifting the layout. A failed diff, blame or
  commit-file fetch in the diff pane keeps the raw error text, adds a plain-language hint for known
  error kinds and offers Retry; a failed repository open also offers Retry, and a lost backend
  connection shows a fixed hint. Errors from actions (the top-level banner for a failed commit,
  checkout, push and so on) still show the message only, with no hint and no Retry.
- New success, warning and info colours for light and dark themes, used in the status strip,
  toasts and conflicted file tinting.
- The inline New branch form shows its base, validates the name inline, and can check the new
  branch out. Pull request source/target branches default sensibly and suggest known branches.
- The repository picker leads with folder names, explains workspaces, and hints at the command
  palette. The palette hint is platform-aware and release notes use a "What's new" icon.
- Sentence case for UI labels: picker, workspace and rebase buttons, the branch "+" menu
  ("New branch…", "Add remote…"), hunk actions, conflict resolution ("Accept ours", "Keep their
  version", …) and "Back to diff".
- Internal: FB-004, VIS-002 (colour tokens), FB-007; the label rules and the Title Case labels
  that remained are documented in `docs/CONTENT_GUIDELINES.md`.
- Errors now stay on screen until you dismiss them or start another change (stage, commit, checkout,
  …); refreshing no longer clears them. Same in the desktop app and the VSCode extension (#177).
- Accessibility: a remote branch's actions button is named with its remote ("Actions for
  origin/main"), and closing a dialog opened from the commit menu returns focus to the commit list
  (#106, #146).

### Diff pane, layout and accessibility

- Staged files no longer offer a discard action (the hunk and line discard buttons and `d` in a
  staged file's diff); it could previously revert an unrelated hunk of unstaged changes.
- Diff pane: a file's diff shows "Loading diff…" instead of a false "No differences" while it
  loads; empty diffs now read "No text differences (binary file or mode-only change)".
  Collapsed file sections no longer fetch their diff until expanded, and sections far from the
  viewport not until scrolled near. Open diffs refetch on every
  refresh (an action, the palette's Refresh, a stash apply or pull), and a hunk stage/unstage/
  discard refetches both the staged and unstaged diff of that file once the action has finished.
  Only the active repository tab is kept loaded, so many open tabs don't slow the app down; a
  half-typed commit message is kept per repository across tab switches, and a tab switched away
  from mid-action no longer stays unclosable.
- Diff hunk actions are no longer tab stops. The diff is one tab stop with `[`/`]` (prev/next
  hunk), `s` (stage/unstage) and `d` (discard, press twice). The armed "Confirm discard" is
  styled as danger and disarms on blur, Escape or after 5 seconds.
- Diff lines show old/new line-number gutters; long tokens wrap and hunk headers wrap cleanly
  at narrow widths. Paired Remove/Add lines now highlight only the changed words (computed once
  per loaded diff; line pairs too long to compare cheaply, such as minified code, are marked
  wholly changed instead), and a Split view / Unified view toggle switches the diff between the
  single-column and a side-by-side layout.
- Commit dock is lighter (single border, no nested panel) and no longer lets diff content show
  beneath it.
- Accessibility pass: confirm and form dialogs return focus to the control that opened them;
  deleting a workspace asks in a proper dialog; repository tabs move with Left/Right/Home/End
  and link to their workspace panel. A tab's close button is for the mouse only and out of the
  Tab order; from the keyboard, Delete closes the focused tab, the palette has a "Close tab"
  command, and Ctrl/Cmd+W closes the active tab where the host doesn't take the key first (the
  desktop app's macOS menu binds Cmd+W to Close Window). Swatches, tab close buttons, sidebar
  toolbar buttons and split dividers have at least 24x24 pointer targets; context menus no
  longer close on mouse leave and stay inside the window; the reduced-motion setting turns off
  smooth scrolling and transitions; sidebar toolbar icons have tooltips.
- Internal: (A11Y-003) workspace deletion uses the shared `ConfirmDialog`; tabs use roving
  tabindex, and the close button stays inside the tablist as `aria-hidden`; the target size is
  `--size-target-min`. Also a shared `Field` primitive (label, hint, error, `aria-invalid`,
  `aria-describedby`, `required`). PERF-001 (`IntersectionObserver`), UX-007, RESP-001, RESP-002
  and UX-006 belong to the diff pane and layout bullets above and below.
- Narrow windows: the Tauri window has a minimum size (800x500), the diff pane keeps a minimum
  width instead of collapsing to zero, and the sidebar auto-collapses below 900px.
- The header "+" (open repository) button is pinned outside the scrolling tab strip.
- Keyboard shortcut sheet, opened with `?` or the "Show keyboard shortcuts" palette command.
- Sidebar section header buttons (such as the Branches "+") no longer sit under the sidebar's
  overlay scrollbar, which swallowed clicks on them once the sections overflowed.
- Internal: this also fixes the remote e2e specs that go through that button.
- Remote-tracking branch rows in the Branches tree are now keyboard-focusable, and context
  menus support standard keyboard navigation (arrow keys, Home/End, focus on the first item
  when opened) instead of only closing on Escape.
- The commit-message textarea now has an explicit accessible name.
- The window no longer flashes blank while open repositories restore.
- Commit, Continue rebase and Abort rebase/merge buttons are pinned to the bottom of the diff pane
  instead of sitting below every file section, where a large change set pushed them thousands of
  pixels off screen.
- Modal dialogs (release notes, command palette, repo picker) open scrolled to the top instead of
  to whichever control received focus. The command palette also fills its dialog width, and the
  empty-state header, empty-state text and Uncommitted Changes row match the rest of the UI.
- The diff view now renders every changed file's diff expanded by default, instead of a file list
  where each row had to be clicked to reveal its diff. Each file's header carries an inline
  Stage/Unstage control and a Blame button, plus a per-file collapse chevron; a "Collapse
  all"/"Expand all" toggle gives an overview of a large changeset.

### Failure log

- Added: the desktop app now writes a log file to the system's log folder, rotated as it grows.
  Failed actions, uncaught errors and crashes of background work all go into it, so a bug report
  can include what went wrong.
- Internal: the log uses `tauri-plugin-log` in the backend, with a panic hook for worker-thread
  crashes, and `@tauri-apps/plugin-log` for every failed IPC call and uncaught error or
  rejection in the frontend.
- HTTPS credential-store failures now retain a safe diagnostic in the application log while keeping
  the on-screen message generic.

### Other fixes

- A staged rename shows its new name with "Renamed from …", previews an image's old version
  against the new one (instead of "Added"), diffs only the edits, opens the diff tool against the
  old path, and unstages as a whole. A rename with edits shows as a rename, not as a
  modification. A file moved but not staged shows as deleted plus new, like `git status` (#137).
- A commit that fails (for example because of a hook) no longer clears the message you typed
  (#109).
- Merge in the "Pull has diverged" dialog failed with "reference not found" (#104).
- Sidebar menus (e.g. the Branches "Add" menu) are no longer clipped by their section while it
  is opening: the section open animation now fades only, without the transform that confined
  fixed-position menus to the section body (#98).
- Fixed: after a rebase paused on a conflict or was aborted, the app could stay stuck showing
  the content from before the rebase. A detached HEAD, normal during a rebase, now counts as
  "no upstream" instead of failing the refresh.
- Internal: `current_upstream` no longer errors on a detached HEAD; that error rejected the
  frontend's whole per-mutation state refresh.

### Internal: test reliability

- Fixed: ten commit search tests no longer read one page under the 250 ms page time budget. A
  slow runner could end the page before the match. The `git-core` tests now search without a
  time budget, and the worker tests follow `next_cursor` (#182).
- Fixed: the state lock's fork test waited a fixed 200 ms for the child to fork. It now waits
  for a byte the child writes after the fork, and fails if the child has already exec'd when
  the lock is checked (#182).
- Fixed: E2E specs that checked too early now wait for the real condition: the stored sequence
  file to go away (`cherry-pick-sequence`), the hunk commit to land (`hunk-staging`), a commit
  menu item that a graph refresh can't close before the click (`history-rewrite`), and the header's
  "Release notes" button to render (VSCode `third-party-licences`) (#182).
- Tests that change the repository while a command waits for the state lock now know the
  command is waiting, instead of sleeping a fixed time. They no longer fail on a slow runner
  (#274).
- Tests already ignored a developer's global and system git config. Now every test that runs
  the git CLI uses one shared helper, and every test process also gets its own empty GNUPGHOME
  and English messages. It drops the GIT_* variables a hook or `git rebase -x` passes on, so
  running the tests from a hook can't touch the real repository or gpg agent (#282).
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
- Fixed (tests): the gpg signing tests left a gpg-agent and a /tmp/bgpg* directory behind on
  every run, which over time used up the per-user inotify limit; they now stop the agent and
  delete the directory, also when a test fails. CI fails if a test run leaks one (#188).
- Rust tests no longer pick up the developer's or CI runner's global/system git config
  (`init.defaultBranch`, `core.autocrlf`, `fetch.prune`, `commit.gpgSign`, `core.hooksPath`, …),
  and no longer race a temporary config directory's cleanup on Windows (#111).
- Fixed an intermittent SSH-signing test failure caused by one test's throwaway ssh-agent
  leaking into concurrently running signing tests (#153).
- E2E: specs type-check cleanly and CI now type-checks both E2E suites (#103); "row is gone"
  checks wait for the row to disappear instead of racing a stale element (#145).
- Tests: the VSCode extension's E2E covers a failing hook, "Commit without hooks" and Cancel (#123).
- Tests: the test that cancels a pull's post-merge hook now waits until the hook has started
  instead of a fixed 300 ms, so it no longer depends on how busy the machine is (#285).
- E2E: the Tauri suite runs with its own throwaway webview data directory, wiped before every
  spec file, instead of sharing localStorage with the installed app (#99).
- E2E: on a Wayland desktop, both suites now pin the app window to the Xvfb X11 display; it
  previously opened unfocused on the real desktop, received no frames, and never rendered diff
  lines (#100). The command-palette spec re-sends Ctrl+K if the first keypress after startup is
  lost.
- Added test coverage for the `?` keyboard shortcut guard and sidebar auto-collapse behavior in
  `App`.
- A new `extension/e2e/` layer gives the VSCode extension its own black-box E2E coverage:
  `@vscode/test-electron` drives the unpacked extension inside a real Extension Development
  Host, and a hand-rolled raw Chrome DevTools Protocol client (no Playwright or other
  browser-automation library) attaches to the webview's nested content frame for DOM assertions.
  One flow so far — open repo → stage a file → commit → see it in history — mirroring `e2e/`'s
  existing pattern. Wired into CI as a new `e2e-vscode` job that gates `build-vsix`.
- The workspaces E2E spec's Edit/Delete steps now allow up to 45s for the picker to reload its
  workspace list after a full app restart, matching the CI-runner contention already documented for
  this suite's repo-scan wait — the previous 10s budget was flaking on `main`.
- `abort_rebase_restores_the_original_branch_and_tip_exactly` no longer asserts against the
  pre-rebase commit's OID, which only matched by coincidence when two independently-generated
  committer timestamps landed in the same second; it was flaking on the `windows-latest` CI runner.
  Also: the `rust` CI job's macOS/Windows legs now run on every pull request instead of only on
  pushes to `main`, so a platform-specific regression like this one is caught before merge.

### Internal: wire DTOs and transport parity

- Changed: each method in the VSCode sidecar's `methods!` table now names its params struct,
  and the macro reads the request's params into it (#306). Handlers take the typed struct
  instead of raw JSON. Error messages for bad params are unchanged, and methods that take no
  params still ignore them. New tests pin more of the error text through the sidecar binary.
- Changed: wire fields that hold one of a fixed set of names (status kind, diff line origin,
  transfer operation, phase and error kind, head operation, hook source and phase, rebase
  divergence reason, sequence kind, terminal rebase kind) now carry an enum instead of a
  string built from its `Debug` output (#306). The `git-core` and `repo-service` enums derive
  `Serialize` themselves; the transfer and hook progress phases get small enums in
  `repo_service::wire`. The generated TypeScript names the two new phase types. The
  commit trailer helper is shared by both transports. The wire format is unchanged: every wire
  snapshot file is byte for byte the same.
- Changed: the desktop app and the VSCode sidecar now share one set of wire DTOs and their
  mappings, in the new `repo_service::wire` module (#306). Each transport had its own copy of
  about 70 DTOs, which could drift apart. Only transport-specific types stay in the transport
  crates (clone and init and the merge tool result for the desktop app; `list_open_repos`'s
  object and `read_file_version` for the sidecar). The sidecar's handlers that built DTOs
  inline now use the same `From` impls as the snapshot tests. The TypeScript bindings are now
  `frontend/src/ipc/generated/wire-dto.ts` plus two small per-transport files. ts-rs stays a
  test-only dependency. The wire format is unchanged: every wire snapshot file is byte for
  byte the same.
- Added: tests that pin the exact bytes each transport sends for every wire DTO, with every
  enum variant and both null and set optional fields (#306). The desktop app's and the VSCode
  sidecar's snapshots are checked in under `crates/*/tests/wire/`, and a third test compares
  the DTOs both transports share. Today they differ only in field order: the sidecar's
  answers go through `serde_json::Value`, which sorts keys. For the sidecar, tests also run
  the real binary and the real notification writers and compare their lines with the
  snapshots, and pin the error line of every method called with missing or wrong params. The
  desktop app now names its `hook-progress` event in one constant. The wire format is
  unchanged.
- Added: the frontend type check now fails when a wire DTO in the desktop app or the VSCode
  sidecar no longer matches its TypeScript type in `RepoClient.ts`, for example after a
  renamed field (#306). Both transports' DTOs generate TypeScript with ts-rs (test builds
  only) into `frontend/src/ipc/generated/`, and `frontend/src/ipc/dtoContract.ts` compares
  them with the hand-written types. The sidecar's `get_file_sides`, `abort_rebase` and
  `list_open_repos` answers now come from DTOs too, so they are checked. Answers that are
  plain strings or lists, notification envelopes and the host-only `read_file_version` are
  not. The wire format is unchanged.
- Added: a test fails when an operation reaches only one of the two transports, the desktop
  app (Tauri commands) or the VSCode extension (sidecar and extension host), unless an
  allow-list in `frontend/src/ipc/transportParity.test.ts` says why (#306). The Tauri command
  list, the sidecar's dispatch table and the extension host's own methods are each declared
  once in code and checked in as generated lists under `frontend/src/ipc/generated/`.
  Today's differences are all intended: clone and init are desktop-only, frontend error
  logging needs no Tauri command, and three sidecar methods serve only the extension host.
- Internal: the three sidecar methods the webview can no longer call are `read_file_version`,
  `repo_workdir` and `check_merge_target`; the extension host answers them with an error and
  never forwards them to the sidecar. The unanswered requests were extension host methods without
  a handler (#306).

### Internal: code structure

- The transport-isolation ESLint rule now covers all of `frontend/src/**` (previously missed
  `src/lib/**`) and also bans `vscode` imports outside `vscodeRepoClient.ts`/`extension/`.
- Extracted the git worker/credential/forge service layer out of `tauri-app` into a new
  internal `repo-service` crate, in preparation for a future VSCode extension — no
  user-visible behavior change.
- `crates/vscode-sidecar` now wires the full `RepoClient` method surface (every method except
  the five VSCode-native ones — repo folder picking, app/last-seen version, and opening an
  external URL — which the extension host wires directly against VSCode APIs), including a
  transfer-progress JSON-RPC notification mechanism for fetch/push/pull.
- BranchTree's edit-remote, credentials, set-upstream and diverged-pull dialogs now share a
  `FormDialog` primitive that focuses the first field.
- `sidecarBridge.ts` no longer forwards a JSON-RPC response to the webview unless it actually
  matches a pending request.

### Internal: licence checks and notices

- Added: CI's new `licenses` job checks the licence of every dependency, transitive and dev
  included, not just the direct ones (#273). Rust: `cargo deny check licenses` (`deny.toml`)
  for every platform we build for. npm: `scripts/check-dependency-licenses.py` for
  `frontend/`, `extension/`, `e2e/` and `extension/e2e/`, including other platforms' native
  binaries. What ships in the app or the VSIX must use an allowed licence; dev tooling may
  also match a short list of recorded exceptions. Exceptions that no longer match anything
  fail the check.
- Changed: BlueOak-1.0.0 and PSF-2.0 / Python-2.0 are now allowed licences. The libgit2
  exception is scoped to the `libgit2-sys` crate. MPL-2.0 is allowed for `option-ext`
  (linked into the app and the sidecar) and a few build-time crates, each as a recorded
  exception. `extension/THIRD_PARTY_NOTICES.md` says where `option-ext`'s source is.
- Added: a workflow regenerates `THIRD_PARTY_NOTICES.md` on Renovate pull requests, commits it
  to the Renovate branch and starts CI on the new commit, so dependency updates no longer fail
  the `licenses` job until someone regenerates the file by hand. CI can now also be started by
  hand (`workflow_dispatch`) (#325).
- Changed: `THIRD_PARTY_NOTICES.md` is now generated from the Rust and npm dependency trees
  (`scripts/generate-third-party-notices.py`, using cargo-about), plus a hand-kept part for the
  bundled C libraries. CI fails when it no longer matches the dependencies or `Cargo.lock`.

### Internal: extension publishing checks

- Added: CI checks every packaged `.vsix` before it can be published: publisher, name, version,
  target, pre-release flag, icon, repository link, the file list, and that the bundled sidecar
  is an executable for the right platform (#271). Pull requests also install the linux-x64
  package into a real VSCode and check that it activates and its sidecar opens a repository.
- Changed: the extension package no longer ships source maps. The compiled code's
  `dist/*.js.map` files are left out on purpose; the new check found them.
- Added: a manual dry run of the release workflow. It checks the Visual Studio Marketplace and
  Open VSX credentials and prints the Entra app's Marketplace member id, without building or
  publishing anything. See `docs/RELEASING.md`.

### Internal: dependencies

- Updated the frontend's JavaScript dependencies (React 19.3, Vite 8.3, jsdom 30.1 and others).
- Updated Vitest to 5 in the frontend and the VSCode extension. Vitest 5 changed its assertion
  types, and @testing-library/jest-dom 7.0.1 no longer adds its matchers to them, so the
  frontend now declares them itself (`frontend/src/jest-dom-vitest.d.ts`, the augmentation the
  Vitest 5 migration guide documents). The test setup imports `@testing-library/jest-dom/vitest`.
- Changed: the test-only `ctor` dependency is now 1.0 (the version Tauri already uses); the
  hermetic-git test guard's constructor is marked `unsafe`, as ctor 0.13+ requires.
- Changed: `.cargo/audit.toml` no longer ignores RUSTSEC-2026-0367, -0368 and -0369. The
  updater now uses zip 9, which fixes them (#81).

### Internal: changelog fragments

Pull requests no longer conflict over `CHANGELOG.md`. Each one now adds its entry as a file in
`changelog.d/`, and a release folds them into `CHANGELOG.md`.

- Changed: `scripts/check-changelog.py` (pre-push hook and CI) accepts a new or edited fragment,
  `changelog.d/<issue>-<short-slug>.md`, as the changelog entry, and checks that it starts with
  a `### ` heading. Editing `CHANGELOG.md` still counts. Any other file in `changelog.d/`
  (such as `240-x.MD`) fails the check. The format is in `changelog.d/README.md`.
- Changed: the pre-push hook compares against `origin/main` when the clone has it, else `main`.
- Added: `scripts/fold-changelog.py` moves all fragments to the top of `## [Unreleased]`,
  newest first, and deletes them. With `--check` it fails while fragments are left; the release
  workflow runs that check, so a release always ships a complete `CHANGELOG.md`.
