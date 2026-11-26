# Changelog

All notable changes to this project are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.17.0] - 2026-11-26

0.17.0 fixes data loss and crashes found in an audit of the app: in sparse checkouts and with
files marked skip-worktree or added with `git add -N`, in branch and remote settings, with
settings written without a value, and in Undo. It makes large repositories, big commits and
files under filter drivers (Git LFS, git-crypt) safe and fast to work with, and it hardens the
release workflow. Changes in behaviour to know about:

- A signing program (`gpg.program`, `gpg.ssh.program`, …) or `gpg.ssh.allowedSignersFile` set
  only in a repository's `.git/config` is ignored: move it to your global git config.
- Reset hard refuses when it would overwrite local edits of a file outside a sparse checkout or
  marked skip-worktree, and while a merge, cherry-pick, revert or rebase is paused.
- A discard whose files can't be saved for Undo is refused, and so is any action that would
  store or write a file under a filter driver Browsitory can't run.
- The undo history file is now version 3: an older Browsitory starts with an empty undo history.
- A commit or stash with more than 50 files starts with every file collapsed, and a file over
  20 MB isn't diffed or blamed until you ask.

### Security

- Changed: selecting a signed commit no longer runs a program that the repository's own
  `.git/config` names. `gpg.program`, `gpg.openpgp.program`, `gpg.ssh.program` and
  `gpg.ssh.allowedSignersFile` now count only from your own git config (global, XDG or system),
  for checking signatures and for signing. When only the repository sets a program, the
  signature shows "Signature could not be checked" and signing fails, both saying which setting
  to move to your global config; an allowed signers file set only there is ignored (#1121).
- Fixed: the token typed in the clone dialog was sent to another host when the server redirected
  the clone there. It now goes only to the host and port of the clone URL (#1126).
- Fixed: Clone refuses an `https://` or `http://` URL with a user name or token in it, as the
  remotes list already did. Such a URL landed in the repository's Git config, broke the remotes
  list and could leave a saved token unusable. The Clone dialog moves a user name without a
  password (Bitbucket's copy URL, `https://alice@bitbucket.org/…`) into the Username field when
  you click Clone (#1052).
- Fixed: the Clone dialog no longer moves an access token pasted as the URL's user name
  (`https://ghp_…@github.com/…`, `glpat-…`, Bitbucket app passwords, or a long random string)
  into the plain-text Username field. It takes the token out of the URL, shows it nowhere and
  asks for your username and the token in the Token field. An ordinary user name still moves
  (#1092).
- Fixed: errors written to the log file (desktop) or the output channel (VS Code), and the text
  Copy details copies, no longer contain the user name, password or token of a URL or the value
  of an `Authorization` header (#1056). The operation log, the log file, the output channel and
  Copy details also no longer keep a URL's password when it holds `'`, `(`, `)`, `,`, `[`, `]` or a
  raw `/` (#1125).
- Fixed: the desktop app's window no longer holds the file dialog and opener permissions, which
  the interface never uses; the app opens dialogs, links and folders from its backend only
  (#1053).

### Sparse checkouts

- Fixed: in a sparse checkout, Reset to a commit that adds files outside the chosen folders no
  longer lists them as deleted (mixed; Stage all could then commit their deletion) or writes
  them to disk (hard): they are marked as outside the sparse checkout, as with `git reset`. The
  same for the files Abort merge, cherry-pick or revert and Undo put back. Abort rebase no
  longer writes every file outside the sparse checkout and drops their mark. With non-cone
  patterns (`git sparse-checkout set --no-cone`), such a file is treated as outside whenever it
  isn't on disk (#1172).
- Fixed: in a sparse checkout, Stash recorded the files outside the chosen folders as deleted,
  wrote them to disk and dropped their mark; Pop then deleted them again and the change list
  showed them as deleted, so Stage all could commit their deletion. Stash now leaves them alone,
  as `git stash` does, and so it does with files marked with `git update-index --skip-worktree`
  (#1184).
- Fixed: applying or popping a stash that changes a file outside the sparse checkout (made
  before it was narrowed, say) left that change out without a word, and Pop then dropped the
  stash. The file is now written with the stashed change, as with `git stash apply`. A file
  whose staged change Apply or Pop brings back outside the chosen folders stays outside, as
  with git (#1184).

### Index integrity

- Fixed: Reset (mixed and hard) and Abort merge, cherry-pick or revert no longer drop the mark of
  files outside a sparse checkout or marked with `git update-index --skip-worktree`, which then
  showed up as modified or deleted. Reset hard leaves such a file alone when the commit has the
  same version of it, doesn't bring back the files outside a sparse checkout, and refuses
  (changing nothing) when it would overwrite a marked file's local edits, as `git reset --hard`
  does; before, it overwrote them and Undo couldn't bring them back. Reset hard also keeps a
  file added with `git add -N` instead of deleting it (#1145).
- Fixed: in a sparse checkout the files outside the chosen folders are no longer listed as
  deleted, and Stage all no longer stages their deletion; such a file can't be staged, as with
  `git add` (#1146).
- Fixed: a file added with `git add -N` is listed as an untracked file instead of a staged one,
  and a commit, amend or the other commands that commit leave it out instead of committing it
  as an empty file (#1164).
- Fixed: a stage that failed because another program held the index lock no longer takes effect
  later with the next command that writes the index (#1166).
- Fixed: staging, unstaging or resolving in the app while a merge, cherry-pick, revert or rebase
  is paused no longer undoes what was staged in a terminal or another tool since the app last
  read the status (#1167).

### Undo

- Fixed: discarding changes (a hunk, lines, a file, Discard all, restoring a file from a commit,
  applying a commit's change or a patch to the work tree) while a merge, cherry-pick, revert or
  rebase is paused, or after a stash pop that conflicted, wasn't recorded, so Undo couldn't
  bring the content back. Such a discard is now an Undo step of its own, which Undo and Redo
  run during the pause too. Reset hard and applying and staging a patch or a commit's change are
  refused then, saying why (#1147).
- Changed: a discard whose files couldn't be saved for Undo is refused instead of run, and the
  message names the file (#1147).
- Fixed: Undo of Rename branch or Delete branch brought the branch back without its upstream and
  other settings, and a rename left them behind under the new name (#1151).
- Fixed: Undo of a new branch created from another commit and switched to left that commit's
  files in the work tree (#1154).
- Fixed: Undo and Redo were refused in a sparse checkout, or with a file marked skip-worktree or
  added with intent to add (`git add -N`), whenever they changed the index. They now keep those
  marks (#1174).
- Changed: with more than 10,000 untracked files (a build folder or dataset that isn't ignored),
  or for a changed or untracked file over 50 MB, Switch, Pull, Merge, Rebase, Reset (soft or
  mixed), Checkout, Cherry-pick, Revert and Ignore no longer save a copy for Undo; they note each
  file's size and time instead, so a big dump or dataset next to your code no longer slows every
  click. Undo still works and leaves those files as they are; if it would have to bring back the
  old contents of one that changed since, it says so and changes nothing. Discards, Reset hard,
  restoring a file and applying a patch or a commit's change still save every file they replace,
  however many or big (#1065, #1090).
- Fixed: discarding one hunk next to a big un-ignored folder saved every file of it for Undo;
  only what a discard can change is saved whole now (#1104).
- Fixed: Undo and Redo of a discard read a big file into memory whole, twice, and files Undo
  saves are no longer held in memory whole (#1065, #1089).
- Changed: the undo history file is now version 3. An older Browsitory starts with an empty undo
  history in a repository this version used, instead of misreading it, and leaves the newer
  history untouched (#1093, #1147, #1151, #1174).

### Branch and remote settings

- Fixed: Delete branch removed, and Rename branch moved, the settings of another branch whose name
  continues after a dot (deleting `release` lost `release.1`'s upstream and description), and
  both failed halfway on a branch setting in your own git config. They now change exactly the
  settings `git branch -D` and `git branch -m` change, in the repository's own config (#1181).
- Fixed: Rename remote and Remove remote did the same to a remote named like `origin.mirror`,
  Rename remote wrote Browsitory's own sign-in settings of the remote under a stray remote, failed
  halfway on a remote with several fetch refspecs, and left `pushRemote` and `remote.pushDefault`
  naming the old name; Remove remote deleted local branches the remote's push refspec named and
  could take the upstream settings of the wrong branch. Both now do what `git remote rename` and
  `git remote rm` do (#1181).
- Fixed: Delete branch and Rename branch crashed the app's backend (or the VS Code extension's
  helper) when the branch had a setting written without a value (a line such as `flag` alone in
  `.git/config`), and so did Rename remote and Remove remote for such a setting of the remote, and
  a failed worktree add removing its new branch. Delete and Remove now remove the setting as git
  does; Rename carries it over as `true`, which is what git reads it as (#1180).
- Fixed: Rename remote, Remove remote and Add remote crashed on a `branch.<name>.remote` or a
  left-over `remote.<name>.fetch` written without a value; they now refuse and name the setting,
  as git does. A setting with several values, one of them without a value, is refused the same
  way by the branch and remote commands (#1180).
- Fixed: A `merge.default` or `merge.conflictstyle` setting written without a value (the name
  alone in a config file) crashed the app's backend (or the VS Code extension's helper) on every
  merge, pull, cherry-pick, revert, rebase, stash, switch, reset, discard, patch apply, Undo of a
  work-tree change and worktree add. These commands now refuse and name the setting, as git does,
  without changing anything (#1182).

### Git LFS, git-crypt and other filter drivers

- Fixed: Browsitory can't run filter drivers, but staged and checked out their files anyway: with
  git-crypt unlocked, Stage and Commit committed the secret in plain text, and with Git LFS a
  switch, reset or checkout wrote the pointer text over the real file. Every action that would
  store or write a file whose `filter` attribute names a configured driver (staging, discarding,
  stashing, applying a stash or patch, resolving conflicts, switching, checking out, resetting,
  merging, pulling, cherry-picking, reverting, rebasing, undo) now refuses before it changes
  anything, naming the file and the filter (#1063).
- Added: when the repository's attributes use a filter driver Browsitory can't run (Git LFS,
  git-crypt), the Changes list says so at the top, until you dismiss it (#1087).
- Changed: a changed file under such a filter is marked "'crypt' filter" in the Changes list, its
  Stage button is off, and its diff says the difference may be only the filter's. Stage all and
  Stage modified leave it out, as they leave out conflicts (#1087).
- Fixed: a Git LFS file whose content matches the pointer git stores (same size and SHA-256) no
  longer shows as changed after a `touch` or a checkout by another tool. Files under other filters
  stay listed: Browsitory can't tell without running the filter (#1087).

### Large repositories, commits and files

- Fixed: selecting a commit or a stash that changes many files (a first commit, a vendor import,
  a mass reformat) loaded and drew every file's diff at once and could freeze the window. With
  more than 50 files, every file now starts collapsed and its diff loads when you expand it; the
  first 200 files are listed, with **Show 200 more** and **Show all** for the rest (also in a
  comparison) (#1153).
- Fixed: the Changes list in the Tree layout drew every row and slowed down with each one, so
  thousands of changed files took seconds to show and to select in. It now draws only the rows on
  screen, as the Flat layout does (#1157).
- Fixed: Blame on a long file drew every line at once and could freeze the window; it now draws
  only the lines on screen and loads less data. Blame of a file over 20 MB says how large it is
  instead of reading it, and blame of a binary file says it has none (#1158).
- Changed: a file over 20 MB on either side isn't diffed until you ask: the diff says how large
  it is and offers **Load diff anyway**, so opening one no longer holds up the app (#1066).
- Fixed: Delete branch took seconds in a repository with thousands of tags or branches while it
  counted the commits only that branch has, and a second click started a second count and a
  delete that failed. The count is now quick, and Delete is unavailable while it runs (#1067).
- Fixed: every refresh walked each stash's whole untracked folder again; a stash's file count is
  now worked out once (#1070).
- Fixed: on Linux, saving a `.gitignore` or switching to a branch that changes many of them made
  the change watcher walk the whole work tree once per file event, keeping a CPU core busy for
  minutes in a large monorepo. It now walks once per batch of changes (#1068).
- Fixed: the Tags section rendered every tag, and "Select all" froze the app for seconds with
  thousands of tags. Only the tags in view are rendered now, as in the Stashes and Reflog
  sections, and selecting is fast at any count (#1069).

### Remotes

- Fixed: a repository with a remote whose `https://` URL holds a user name or token (cloned with
  Git or another program, such as Bitbucket's `https://alice@bitbucket.org/…`) showed no remotes
  at all. Such a remote is now listed with `***` in place of the user name and password, and
  fetch, pull and push use the saved URL. Edit remote keeps the saved URL while you leave it as
  shown, and Manage credentials saves the token for it (#1081).
- Fixed: Cancel did nothing on a fetch or pull whose connection went silent after connecting
  (the Wi-Fi changed, a VPN dropped, the laptop slept), and actions that change the repository
  waited for it until the app restarted. Cancel now stops it within about two seconds, without
  updating any remote branch (#1064).
- Fixed: a fetch, pull, push or clone that receives nothing for two minutes now fails instead of
  waiting forever, also without Cancel (not for HTTPS remotes on Windows) (#1064).

### Updates

- Fixed: the `.rpm` is signed with an OpenPGP key, so Fedora 45 and newer (rpm 6, which refuses
  unsigned packages) can update it from inside the app. Import the key once with
  `sudo rpm --import` (see the install guide) (#1077).
- Fixed: when installing an update fails, the update notice says so with the reason instead of
  doing nothing, and the error goes to the log file; **Restart to update** tries again (#1077).

### Privacy and stored data

- Changed: a repository that leaves Recent repositories, removed or dropped off the end of the
  list, loses its recent searches, recent commit messages, recent branches and Fetch
  automatically / Prune choices; one still open in a tab keeps them (#1096).
- Fixed: the privacy notice says that a pull request connection applies to every repository on
  that forge, and that an expanded card lists a repository's pull requests as soon as it opens,
  without a click (#1054).
- Fixed: the privacy notice lists everything the app keeps in its web storage, including the
  last 5 branches you switched to in each repository and the view choices, and says these stay
  after a repository leaves Recent repositories (#1055).
- Changed: the commit file browser shows sizes like the image diff and transfer progress do,
  "512 B" and "1.0 KB" instead of "512 bytes" and "1 KB" (#1060).

### Internal

- Changed: the release workflow pins every action by commit SHA (Renovate updates them), gives
  `contents: write` only to the two jobs that upload to the draft release, and leaves no token in
  `.git/config` after a checkout; CI's `mutants` and `removed-assertions` jobs don't either
  (#1109).
- Changed: the Open VSX publish and dry-run jobs run `ovsx` from `extension/publish/`, a locked
  pnpm project installed without install scripts before the token is used, instead of an
  unlocked `npx ovsx@1.3.0`; Renovate and the licence check now cover it (#1110).
- Added: the release workflow's `verify` job fails unless `CHANGELOG.md` has a section for the
  tag's version right under an empty `## [Unreleased]`, the tagged commit is on `main`, and CI
  passed for it (`scripts/check-release-tag.py`) (#1111).
- Changed: the Marketplace publish and dry-run jobs install `vsce` without restoring a pnpm store
  cache, so nothing from an Actions cache runs with the Azure session (#1170).
- Changed: the release workflow's `verify` job restores main's Rust cache instead of compiling
  from scratch (#1061).
- Changed: the changelog check (pre-push hook and CI) also requires an entry for changes to
  `extension/`, the VS Code extension that ships in every VSIX, its E2E tests included (#1057),
  and for a `package.json` under `extension/` or `e2e/` that changes outside its dependency
  fields, such as the extension's `contributes` block; Renovate's dependency-only changes stay
  exempt (#1094).
- Changed: CI's Tauri E2E job installs a pinned tauri-driver that Renovate updates, instead of
  the newest release on every run (#1058), in its own pull request instead of the Tauri group
  (#1095).
- Added: an extension test fails when its copies of `HostRequest` and `DiffSource` drift from the
  frontend's `RepoClient.ts` (#1059).
- Changed: the frontend keeps one copy each of the byte-size formatter used by the image diff and
  transfer progress, `isUntracked` and the syntax-highlighting error logger, and drops the unused
  `HISTORY_EDIT_KINDS` (#1060).

### Internal: test flakes

- Fixed: the frontend tests for the merge dialog's preselected option and the repository
  picker's first focus no longer check before the loaded state applies, which failed them now
  and then on a loaded machine (#1100, #1108).

## [0.16.0] - 2026-11-24

0.16.0 is a maintenance release: security fixes in the bundled zlib (RUSTSEC-2026-0544) and
the `url` and `cookie` crates (RUSTSEC-2026-0592, RUSTSEC-2026-0594). The rest is internal:
test flakes and test isolation fixes, and CI and hook setup changes.

### Security

- Fixed: updated the bundled zlib (`libz-sys` 1.1.33) to pick up the fix for RUSTSEC-2026-0544,
  a heap buffer overflow (#1045).
- Fixed: updated `url` to 2.5.9 for RUSTSEC-2026-0592, a stack overflow in `Url::origin()` on
  nested `blob:` URLs, and `cookie` to 0.18.3 for RUSTSEC-2026-0594, private cookie nonces that
  can repeat in processes that fork (#1076).

### Internal

- Changed: CI and the documented headless E2E commands give `xvfb-run` an explicit
  1280x1024x24 screen, since its default differs by distribution (640x480 on Fedora, too small
  for the VS Code merge editor spec) (#1046).
- Fixed: the documented `pre-push` hook setup always links the main checkout's script, so the
  hook no longer dangles once the linked worktree that ran the setup is removed (#1049).

### Internal: test flakes

- Fixed: the multi-repo E2E test read the second tab's changes marker before the status check
  for the file it had just written answered, and saw "1 file"; it now waits for "2 files" (#1038).
- Fixed: the remote-management E2E test clicked "Set upstream…" while Add remote was still
  running, when the item is disabled and the click does nothing; it now clicks the item once it
  is enabled, and so does the remote-transfer test (#862).
- Fixed: the DiffView test of the 2,000-line window stops after one step of 2,000 lines
  instead of rendering all 4,500, and looks for its buttons in the bar under the diff, so it no
  longer times out on a busy machine (#853).

### Internal: test isolation

- Fixed: the SSH signing tests failed at random on a machine with an SSH agent in its session:
  the hermetic test guard now removes `SSH_AUTH_SOCK` and `SSH_AGENT_PID`, as it already hides
  the developer's gpg agent (#805).
- Fixed: on Windows, the work-tree guard's test seam reported a link as cleared even when none
  was left to delete; it now reports only a link it removed, as on Unix (#800).

## [0.15.0] - 2026-11-15

0.15.0 is the UI/UX review (#978): about fifty issues that make the app read and behave the
same everywhere. Every dialog has the same frame, and confirmations say what is lost; error
banners are one row that names the operation and the next step, with **Details** in the
operation log; aborting a paused operation asks first when it would throw away work, and the
status strip is the one place for its status. The commit box is one line until you use it,
tabs show **±N** changed files, untracked files are marked **?**, the less used diff and graph
options are in **View** menus, and the sidebar's branches, worktrees and pull requests use
dialogs and menus instead of bare fields. The merge editor gets the room while a conflict is
open, and the desktop app opens at 1280×800. Besides: "Interactive rebase from here…", Reword
on the latest commit without folding in staged changes, opening a folder that isn't a
repository, and tag compare errors that name the tag.

### Dialogs

Every dialog now has the same frame: a bold title, its buttons at the right of one row, and the
button that does the work highlighted.

- Changed: every dialog shows its title, and the dialog is named by it; Edit remote, Push, Set
  upstream, Credentials, Stash, Identity and every confirmation had none. Screen readers read a
  confirmation's message, or the line under a dialog's title, when it opens. Reflog's Restore
  asks in the same confirmation as the others, with **Restore main** as its button (#987).
- Changed: buttons sit at the right of one row in every dialog, the main one first and
  highlighted like the Commit button, Cancel last (#987).
- Changed: red marks only actions that lose data, in menus, panes and dialogs alike, with an
  outline and bold text, not colour alone. Mark resolved, Run, Open, Restore and Commit without
  hooks are no longer red; Reset to here, Discard all…, Delete untracked…, a stash's Drop… and
  the Abort buttons now are. Reset's button names the mode ("Reset hard") (#988).
- Changed: confirmations say what is lost and whether Undo brings it back. Force push says which
  commits the remote branch loses, that the lease refuses it if the branch moved, and that Undo
  can't take back a push; Remove remote says its remote-tracking branches go and Undo can't
  bring it back (its button is "Remove remote"); Delete tag says a copy on a remote stays and
  Undo brings the tag back; Drop stash says "Undo can't bring it back." instead of repeating its
  title. Delete branch's confirm, which Undo can take back, is a plain **Delete branch** (#989,
  #988).
- Changed: Set upstream opens with the only remote (or origin) and the branch's own name filled
  in. Identity, Create tag, Delete remote branch and Set upstream say under the form why their
  button is off. Edit remote's Push URL says "Same as fetch URL" when empty and its copy buttons
  read "Copy". Credentials offers Forget only for a saved credential, and Cancel is plain
  "Cancel" (#1014).
- Changed: the keyboard shortcut sheet has padding, key chips, a key column as wide as its
  longest keys, and keeps Change shortcuts… and Close in view while the list scrolls (#1016).
- Changed: Merge's options are under "How to merge", and the Squash hint sits under its checkbox
  and says the choices above don't apply. Reset asks "What happens to your changes", with each
  explanation on its own line (#1018).
- Changed: a diverged pull's dialog is titled "main and origin/main have diverged" with the
  commit counts, its buttons are "Pull with merge" and "Pull with rebase" as in the Pull menu,
  and its checkbox reads "Always do this for ⟨repository⟩ (change it in the Pull menu)" (#1019).
- Changed: the rebase planner's title names the branch and the target ("Rebase main onto
  4c5beaf"), a line says how many commits it rewrites and that Undo restores the branch, the
  rows line up under one "Action" header, and the move buttons are arrows (#1015).
- Fixed: the Clone dialog's fields and Cancel button ran past its right edge (#974).

### Errors

- Changed: the error banner is one row: the message, its next steps, **Details** and ✕. A
  rejected push offers **Pull** first and **Force push…** second; **Details** opens the operation
  log on the failed operation, where its full text and **Copy details** are. An error with no
  operation behind it (reading the repository failed) offers **Copy details** in the banner
  instead, with the full text (#984).
- Fixed: the banner's filled first step (**Pull**, **Remove**) and its ✕ no longer lose their
  colours on hover, which hid their labels (#984).
- Fixed: no error banner while a dialog handles the same error. A missing git identity showed two
  near-identical lines beside the identity dialog and stayed after Cancel; a failed hook repeated
  the dialog's first line. Rename branch, Connect to GitHub and New pull request show their
  failure in the dialog only, also after Cancel (#996).
- Changed: Delete branch counts the commits only that branch has before it deletes. With none, it
  deletes at once; otherwise one confirm says "feature has 1 commit on no other branch. Delete it
  anyway?", instead of a failed plain delete's error banner next to a Force delete confirm (#996).
- Changed: errors name the operation, what it worked on and the next step: "Couldn't fetch from
  origin. Check the remote's URL and your connection, then try again." instead of "Fetch failed",
  and "Cherry-pick of 4c5beaf didn't start: …". libgit2's `class=…; code=…` is left out of the
  banner; the operation log and **Copy details** keep it (#997).
- Changed: the remaining error banners name the operation and what it worked on, then Git's
  reason, then the next step: "Couldn't switch to feature/x: your local changes to src/map.js would
  be overwritten" with "Commit or stash the changes to those files first, or use Stash and switch."
  This covers switching branches and commits, creating, renaming and deleting branches and tags,
  stash save, apply, pop and drop, reset, commit and amend (also when a hook fails), merge, rebase,
  history edits, cherry-picks and reverts of several commits, worktrees, submodules, patches and
  restoring a reflog entry. The create forms show the same text. The operation log keeps Git's
  full message (#1032).

### Notifications

- Fixed: after a fetch, a branch both ahead of and behind its upstream was reported as only
  behind. The notification now says "main and origin/main have diverged: 1 commit ahead, 1
  behind."; a branch only ahead says so and offers **Push** (#999).
- Changed: notifications and the desktop app's update notice share one place, the bottom-left
  corner, so they stack instead of covering each other, and none covers the **Commit** button.
  Screen readers announce the update notice once (#1000).
- Changed: the operation log shows why an operation failed first, in the app's words, then git's
  own text, then the facts and **Run again**; **Copy details** copies them. Rows read "Fetch from
  origin" and "Push to origin" (#1022).

### Main window

- Changed: an untracked file is marked **?**, as in `git status --short`, instead of **U**, which
  git uses for unmerged files. Above the diff, a file's state reads **Untracked**, **Modified, not
  staged** or **Added, staged** (#1027).
- Changed: a tab with uncommitted changes shows **±** and the number of changed files instead of a
  dot that looked like an unsaved-file marker; the tooltip and the tab's accessible name say it in
  words. The list of open tabs (**⌄**) shows the same (#1028).
- Changed: the start page puts the keyboard focus in the filter (or on **Open repository…**), and
  a dialog starts on its first control, a radio group on its chosen option, never on its frame.
  The focus ring is blue with a small gap, so a focused button no longer looks like a pressed
  toggle (#1029).
- Changed: the commit box is one line with **Commit** beside it until you click into the message,
  so the diff above gets the room; it stays open while it has the focus or holds anything, and it
  never takes more than half the column. **Add co-author…** and **Amend last commit** moved into
  the menu of the arrow next to **Commit**, which now opens without a message; **Cancel amend**
  ends amend mode (#1003).
- Changed: a clean working tree's commit box no longer says "Stage changes to commit" in amber: it
  says "Nothing to commit.", and "Stage changes to commit." only when there are changes to
  stage, both in muted text (#1005).
- Changed: the commit graph starts at about 40% of the window, so a wide window shows longer
  subjects. Its date column is compact ("3h", "2d", "4mo"; the date in the tooltip; "3 hours ago"
  for screen readers), and a row
  with several branches and tags shows the first and "+N" for the rest (#1004).
- Changed: the toolbar keeps a place for "Fetched …", so Fetch, Pull and Push no longer move when it
  appears. The counts sit on the buttons, `↓n` on **Pull** and `↑n` on **Push**, and only when
  they aren't zero, and screen readers hear "2 ahead, 1 behind origin/main" when they change. In a
  narrow window the "Fetched …" place shrinks to its text. **Apply patch…** moved from an icon
  button into the **Stash** menu, where it stays available when there is nothing to stash (#1006).
- Changed: the diff options you change less often (**Wrap lines**, **Show whole file**, **Ignore
  whitespace**, **Show whitespace**, **Show syntax highlighting**) and the graph's **Show lines
  changed** and **Collapse merges** moved from icon buttons into **View** menus with their names
  and check marks; a menu stays open while you turn its options on and off. Word wrap on **Auto**
  reads **Wrap lines (automatic)** with a small **A** instead of a dashed outline that looked like
  keyboard focus (#1007).
- Changed: Escape closes the compare, the commit search (and a `path:` filter), the new branch
  form and the add remote form wherever the focus is inside them, not only in their text field,
  and the focus goes back to where it was before. The search's syntax help closes first (#1001).

### Menus and command palette

Each action now has one name, the same in its menu, the command palette and the shortcut list.

- Changed: an item ends in "…" exactly when a dialog or confirmation follows ("Edit remote…",
  "Manage credentials…", "Remove remote…", "Add remote…", "Discard all unstaged changes…", a
  worktree's "Remove…"). The palette's name is the menu's plus what it acts on ("Push
  current branch to origin" in both, "Push current branch to…", "Reset main to 1a2b3c4…"). The
  palette's diff options name what they will do ("Show whitespace" or "Hide whitespace", "Wrap
  lines" or "Stop wrapping lines", "More context lines"), and the shortcut list uses the
  palette's names ("Pull", "Go to commit message", "Close tab"). The toolbar's stash menu reads
  "Stash all changes". The status strip's Continue says what it continues, as the palette does:
  "Continue rebase", "Continue cherry-pick", "Continue revert" (#990).
- Changed: the palette's "Delete tag ⟨name⟩…", "Drop stash: ⟨name⟩…" and "Remove worktree
  ⟨name⟩…" open the same confirmation as the Tags, Stashes and Worktrees sections, instead of
  only opening the section. Tag commands have their own "Tags" group (#993, #990).
- Changed: the commit menu's Cherry-pick and Revert say where the result lands: "Cherry-pick onto
  main" and "Revert (new commit on main)" (#1020).
- Changed: a commit's hunk buttons and file menu name the working tree: "Revert hunk in working
  tree", "Apply hunk to working tree", "Revert file in working tree", "Apply file to working
  tree and stage" (#1008).
- Changed: the commit menu's "Rebase onto here" is "Rebase current branch onto here…", and a new
  "Interactive rebase from here…" opens the rebase planner from that commit's parent, so the
  commit is the first one listed. Both are in the command palette for the selected commit (#991).
- Changed: the commit menu scrolls when it is taller than the window, a reason that applies to
  a whole group of items shows once under the group, and the copy items are in a "Copy" submenu
  (#992).
- Changed: the empty command palette lists "Recent", then "Suggested": Continue or Abort while
  an operation is paused, Pull while the branch is behind its upstream, Go to commit message
  while changes are staged. Letters scattered over several words no longer match, so "stash"
  no longer finds "Save staged changes as patch…" (#1021).
- Changed: "Line history" says why it can't run yet ("Select lines first.") and can be focused;
  in a commit's or comparison's file, "History of folder ⟨folder⟩" moved into the file's "…"
  menu (#1023).
- Changed: a commit's details mark tags with the graph's tag icon instead of "#" (#1024).

### Sidebar

- Added: while the graph leaves out some local branch ("Isolate branch", or the branch dots in
  the sidebar), a strip above it says so ("Showing feature/offline-tiles only") with a Show all
  branches button (#985).
- Added: while the graph is filtered, Show all branches is in every branch's menu, local and
  remote, not only in the Branches "+" menu (#985).
- Fixed: the strip's Show all branches no longer loses its colours on hover (#985).
- Changed: when the graph doesn't show the branch you are on, the Uncommitted changes row names
  it ("Uncommitted changes on main") (#985).
- Changed: renaming a branch opens a "Rename branch" dialog ("Rename…" in the branch's menu) in
  place of a bare field in the row. A name that can't be used, or a rename that fails, is
  explained in the dialog, which keeps what you typed, and not again in a banner (#1002, #422).
- Changed: the current branch is marked once, in bold with "(current)", without the extra dot
  (#1025).
- Changed: the Upstream block at the bottom of Branches is gone. Point at the current branch to
  see what it tracks, and use "Clear upstream…" in its menu (#1025).
- Fixed: open sidebar sections no longer squeeze each other: each keeps room for its content up
  to a cap, a longer list scrolls inside its section with the cut edge faded, and the sidebar
  scrolls when the open sections don't fit (#1010).
- Fixed: the New branch and Add remote forms scroll into view with their buttons when they open
  (#1010).
- Changed: the Worktrees section lists the worktrees first, each with its name, branch and
  folder and a "…" menu (Open in new tab, Remove…). "+" in the section header opens a "New
  worktree" dialog whose fields say what they are for ("The folder the worktree's files go in."),
  in place of the four empty fields above the list (#1011).
- Changed: the main worktree, which can't be removed, no longer shows a Remove action (#1011).
- Changed: a pull request card shows "Connect to GitHub…" (or Bitbucket) until you connect; a
  dialog asks for the account and the access token and says why if saving fails. The token
  form with Save, Forget and Cancel is gone from the card (#1012).
- Changed: a connected card lists its pull requests by itself when you open it, without a List
  button. "New pull request…" opens a dialog that keeps what you typed and says why if creating
  fails; the card's "…" menu has Refresh and Disconnect (#1012).
- Changed: each pull request reads "#7 Title" over its branches, author and state, with a
  button to open it in your browser in place of its full address (#1012).
- Fixed: a card's title, such as "GitHub: acme/widget (origin)", is no longer shown in
  uppercase (#1012).
- Changed: with no stashes, the Stashes section points to Stash in the repository toolbar, not
  the commit box (#1026).
- Changed: Submodules shows Update recursively only once the repository has a submodule (#1026).
- Changed: Reflog shows the reflog of HEAD at once, without picking a reference first, and its
  reference picker fits the sidebar (#1026).

### Opening repositories

- Fixed: a new repository with no commits opened with a false "git remote operation failed:
  reference 'refs/heads/main' not found" error, and the toolbar said "no branch". It now opens
  cleanly, and the branch button reads "main (no commits yet)" (#983).
- Added: opening a folder that isn't a Git repository says so and lists the repositories in its
  direct subfolders, each with **Open**; hidden folders and `node_modules` are left out. A folder
  with a `.git` that Git refuses (owned by another user, an unsupported format, a broken HEAD)
  shows Git's message with **Retry** instead, and **Copy details** includes Git's message (#973).
- Changed: a recent repository whose folder is gone no longer opens from the start page;
  **Locate…** in place of the pin picks the folder's new place and replaces the entry. Opened
  another way, the error asks to remove it from the recent repositories, without a Retry that
  couldn't help (#998).
- Changed: the picker opened from the tab bar's **+** is an **Open repository** dialog with a
  **×**, without the empty band at the top or the card inside the dialog (#1013).
- Added: in that dialog, repositories already open in a tab are marked **Open in a tab**, and a
  click switches to their tab (#1013).
- Changed: the command palette tip shows only on the start page, below the lists. The start
  page's picker is titled **Open repository** too (#1013).

### History edits

- Changed: Abort merge, cherry-pick, revert or rebase asks first when it would throw away
  conflict resolutions, unsaved edits in the merge editor, files changed at an edit stop, a new
  commit message or a split's committed parts, which Undo can't bring back. When nothing was
  touched it still aborts at once (#979).
- Added: a toast says "Aborted merge" (or cherry-pick, revert) once the abort worked (#979).
- Fixed: Reword… on the latest commit no longer folds staged changes into it. It opens the same
  Reword dialog as for older commits and changes only the message; staged changes stay staged.
  The command palette offers **Reword ⟨commit⟩…** for it too (#981).
- Fixed: a commit below a merge commit said to use "Rebase onto here…", which can't edit it. Its
  edits now say "Can't edit: a merge commit comes after this one." (#982).
- Fixed: the rebase planner marks a merge commit's row ("Merge commit: can't be replayed"),
  offers it no action, and Start rebase is unavailable with the reason, instead of failing once
  started (#982).
- Changed: the status strip is the only place for a paused merge's, cherry-pick's, revert's or
  rebase's status, Continue and Abort. The commit area no longer repeats them; at an edit stop
  it keeps the commit message (#995).
- Changed: the rebase strip names the branch and what it is rebased onto ("Rebasing main onto
  feature/x, step 1 of 2"), and the toolbar says "Rebasing main" instead of a detached HEAD
  (#995).
- Changed: Edit commit… and Split commit… name their controls and toasts after the edit:
  "Finish editing" / "Cancel edit", "Finish split" / "Cancel split", "Edited 1a2b3c4". "Step 1
  of 1" is gone, and Split says what to do with the parts (#994).

### Merge editor

- Changed: while a conflicted file is open, the merge editor gets the room: the history column
  narrows (it comes back afterwards, and a drag meanwhile is kept), the file list shrinks to
  the conflicts ("Show all changes" brings the rest back), the diff options are hidden, and the
  commit box is one line until every conflict is resolved. The app now opens at 1280×800
  (was 1200×800), so the three panes show on a first launch (#980).
- Fixed: the shrunk file list opens at its top, with the conflicts in view (#980).
- Added: a pane too narrow for three panes shows two: ours and theirs side by side, each
  block's result under them (#980).
- Fixed: why the panes aren't available in a narrow pane is visible text next to the layout
  buttons, not only a tooltip (#980).
- Fixed: in the three panes, and in four with Show base, a long unchanged line wrapped onto more
  rows in ours and theirs than in the base and the result, so the lines after it drifted apart
  across the panes in a narrow window or VS Code editor group. Every pane's lines are now
  equally wide and stay level (#931).

### Diffs, blame and compare

- Fixed: "Compare with current branch" on a tag that was deleted meanwhile said
  `No branch named "refs/tags/v1.0".`; it now says `No tag named "v1.0".`, and a tag on
  unrelated history is named `v1.0` instead of `refs/tags/v1.0` (#872).
- Fixed: the blame view keeps each line's indentation, and its columns no longer run together:
  the short SHA, author and the new date column have padded, fixed widths, and a long author
  name ends in "…" (#986).
- Changed: blame shows the short SHA, author and date once for each run of lines from the same
  commit, and only the SHA is a link to that commit; a click elsewhere on a line does nothing
  (#986).
- Added: blame colors the code like the diff, with the same Syntax highlighting setting (#986).
- Changed: **Back to diff** sits in the file's header, in place of **Blame**, instead of below
  the blame (#986).
- Added: a merge commit's diff says which parent it compares with, "Changes compared with
  4c5beaf (first parent)", and that picker shows the changes against another parent instead
  (#1009).

### Preferences

- Changed: every Appearance setting has its help under its own field, as in the other groups
  (#1017).
- Changed: the editor and terminal commands under External tools save when you leave the field
  or press Enter, like every other setting; the Save button is gone. A refused command stays in
  the field with the reason (#1017).
- Added: a diff or merge tool that isn't configured says which git config key to set, with a
  link to the user guide (#1017).
- Changed: the repository's own fetch interval is labelled "For this repository (name)" instead
  of "In name" (#1017).
- Fixed: in a narrow window the fields no longer slide under the scrollbar, and the code font
  size field has room for "Default" (#1017).
- Added: the settings filter highlights the matching settings (#1017).

### Internal

- Added: `pnpm screenshots:inventory` in `e2e/` captures every reachable screen, menu and dialog
  of the desktop app for UX reviews (#978).

## [0.14.0] - 2026-11-14

0.14.0 brings the operation log (#892): what the app ran in a repository, with its hooks'
output and errors, and Run again for a failed fetch, pull or push. It adds your own commands
(#897): programs with variables for the selected commit, branch or files, in the menus and the
palette, also in VS Code (trusted workspaces only). And the keyboard shortcuts can be changed
(#902): in Preferences → Keyboard in the desktop app, in VS Code's Keyboard Shortcuts editor in
the extension. Besides: co-author chips for a squash message, and closing the window or the
VS Code panel no longer leaves a stale `.git/index.lock` behind.

### Operation log

- Added: an operation log panel lists what the app ran in the repository, newest first: name,
  target, time, duration and result. Selecting an operation shows its parameters, its error and
  the output of the hooks and signing programs it ran, which can be copied. Filters show failed
  operations only or hide automatic fetches; the panel follows the log while open. Open it from
  the palette ("Show operation log"), the Help menu ("Operation log") or an error's "Show
  details", which selects the newest failed operation. Works in the VS Code extension too (#889).
- Added: Run again on a failed fetch, pull or push in the operation log repeats it with the same
  parameters, as a new entry. It is refused, saying why, when the remote or branch it named is
  gone. Local operations (commit, checkout, merge, rebase and the rest) are not offered (#890).
- Added: the user guide's [Operation log](https://rene-langer.github.io/browsitory/operation-log/)
  page, and the privacy page lists the log: memory only, the last 500 operations per repository,
  without credentials, gone when you quit (#891).
- Fixed: the operation log could still show a finished command as running to a client that listed
  it right after the command answered; the entry now ends before the answer is sent (#958).

### User-defined commands

- Added: a "Commands" group in Preferences to add, edit, reorder, test and delete your own
  commands, for every repository or only the active one (#893). A command is a program with
  arguments and `${repo}`, `${commit}`, `${shortCommit}`, `${branch}`, `${ref}`, `${path}` and
  `${paths}` variables; it never runs through a shell, and each value stays one argument
  whatever spaces or quotes it holds. Commands are kept in Browsitory's own settings, never in
  a file inside the repository, so a cloned repository can't add one. Unknown variables are
  refused when you save, and "Test" shows how the command splits into arguments and where its
  program was found.
- Added: the backend runs your commands (#894): without a shell, in the repository's root or the
  command's working directory, with the selected commit, branch, ref and files filled in and
  checked against the repository first. Output is kept (its last 256 KiB), a command can be
  stopped, and one that runs longer than 30 minutes is stopped. Only commands you defined can
  run, and only programs on your `PATH` or given by an absolute path.
  Each run is an entry in the operation log, with its output.
- Added: "Run again" in the operation log works for your commands too (#890, #894): it runs the
  same command on the same selection, asking first and showing its output as the command is set
  up. It is unavailable once the command was deleted.
- Added: your commands appear in the commit, branch, tag, file and repository tab menus they are
  set for, last in a group of their own, and in the command palette as "Run: <name>" (#895). A
  command whose variables can't be filled where it is offered (`${commit}` with no commit
  selected, say) is greyed out with the reason. It asks first when it is set to, shows its output
  in a window with Stop while it runs (or a toast with Show output when it fails), and refreshes
  the repository afterwards when set to.
- Added: in VS Code, your commands run only in a trusted workspace (#896). In Restricted Mode
  they are left out of the menus and the palette lists them as unavailable, saying why; the
  extension never passes a run to the backend there. A new guide page, "Your own commands",
  explains the variables with examples.
- Fixed: in the command editor in Preferences → Commands, each checkbox sat above its label,
  centred; it now sits next to it (#970).
- Fixed: the same in other dialogs: the checkboxes and radio buttons of Merge, Reset, Stash, Push,
  Create tag, Delete tag from remote, the identity prompt, the parent picker, a diverged pull, a
  switch blocked by local changes, and Preferences sat above their labels. They now sit next to
  them (#972).

### Keyboard shortcuts

- Added: Preferences → Keyboard lists the shortcuts by group with their keys. Add key records new
  keys by pressing them; a key another command has is named, with Replace. Keys can be removed,
  and one shortcut or all of them reset. The shortcut sheet's "Change shortcuts…" opens it. Desktop
  app only (#899).
- Added: in VSCode, Preferences → Keyboard opens VSCode's Keyboard Shortcuts on Browsitory's
  commands, and the shortcut sheet and tooltips show the keys you gave them there, read from
  your `keybindings.json` (#900).
- Changed: the user guide's "Change the shortcuts" explains how to change the keys, in the desktop
  app and in VSCode; an E2E flow rebinds Refresh, uses the new key and resets it (#901).

### Co-authors

- Changed: a pre-filled squash message now offers co-author chips, so a squash merge's commit can
  name its co-authors. A merge's and a paused cherry-pick's or revert's message still don't
  (#142).

### Configuration file

- Fixed: an entry added by hand to `config.toml` as a `[[user_commands]]` section (or any other
  list kept as sections, such as `[[repo_user_commands]]`) made the whole file unreadable with
  "duplicate key", because the app had written the empty list as `user_commands = []`. Empty
  lists are no longer written (#967).

### VSCode extension: closing the panel

- Fixed: closing the Browsitory panel while an operation was still writing to the repository
  could stop it halfway and leave `.git/index.lock` behind, so every later git command in that
  repository failed until the file was removed by hand. The panel now lets running operations
  finish (up to 5 seconds) before it stops the background process (#961).

### Desktop app: closing the window

- Fixed: closing the window or quitting the app while an operation was still writing to the
  repository could stop it halfway and leave `.git/index.lock` behind, so every later git command
  in that repository failed until the file was removed by hand. The app now lets running
  operations finish (up to 5 seconds) before it exits, and starts no new one (#965).

### Start page

- Changed: the start page's "Open folder" button is now "Open repository…", since only a
  repository can be opened there (#976).

### Internal

- Internal: the backend keeps a log of the operations it ran in each repository: name, target,
  start, duration, outcome, error, and the hooks and signing programs each ran with their
  output. Memory only, the last 500 per repository, until the app quits; URLs lose their user
  information and `Authorization` headers their value (#887).
- Internal: `list_operations` and the `operation-log` event (`operationLog` in VS Code) expose it
  to the frontend, in both apps (#888).
- Internal: one E2E flow per app opens the log from a failed operation's "Show details" (#891).
- Internal: your keyboard shortcuts are kept in `config.toml`'s `[keybindings]` section with the
  other Preferences settings, through new `get_keybinding_settings`/`set_keybinding_settings`
  commands, instead of the app's web storage (#898).
- Internal: a keymap layer keeps the user's keys on top of the built-in shortcuts and the merge
  editor's keys, with reserved keys and conflict checks; everything that shows or binds a
  shortcut reads the effective keys (#898).
- Internal: the VSCode E2E suite removes a git lock file a spec left behind, so the specs after it
  still run, and fails the run naming that spec (#961).

## [0.13.0] - 2026-11-13

0.13.0 brings the three-way merge editor (#881): the whole conflicted file in three panes,
ours, the result and theirs, with an optional base pane, take-side arrows, an editable result
with Undo and Redo, and word marks. It also edits older commits (#886): Edit commit…, folding
staged changes into a commit and splitting a commit, also from the rebase planner's Edit step.
Besides: on Windows, changing files on a FAT32 or exFAT drive works and rewriting a file
on ReFS keeps its attributes; and the Rust toolchain is pinned.

### Three-way merge editor

- Added: the merge editor shows the whole conflicted file in three panes, ours, the result and
  theirs, each with its own line numbers; the unchanged lines stay level across the panes, which
  scroll together, and every block keeps its choices, keys and editable result. **Blocks**
  switches back to one block per conflict, and the editor remembers the layout; a pane too narrow
  for three (the VS Code side panel) shows the blocks (#875).
- Added: in the three-pane merge editor, an arrow next to each block's lines in the ours and the
  theirs pane puts that side into the result; taking the second side adds it after the first,
  as Ours then theirs or Theirs then ours. The arrows, the choices and keys 1–5 stay in sync, and
  Reset undoes them (#876).
- Added: in the three-pane merge editor, Show base adds the common ancestor as a fourth pane,
  Base, on the left: aligned with the other panes, with its own line numbers, and marked when a
  block takes the base. It replaces the base strip above each block; the block view keeps its
  strip (#877).
- Added: in the three-pane merge editor the whole result is editable, unchanged lines included.
  A block without a choice shows git's conflict markers there; each result line is marked with
  where it came from (ours, theirs, the base, typed, or still in conflict), and typing into a
  resolved block marks it Edited. Save resolution stays disabled while a `<<<<<<<` or `>>>>>>>`
  line is left (#878).
- Added: Undo and Redo in the merge editor (also Ctrl+Z, Ctrl+Shift+Z and Ctrl+Y while the focus
  is in it) step through typing, take-side arrows, choices and Reset; keystrokes in one place
  are one step (#878).
- Added: inside a conflict block the merge editor marks the words that differ between our line
  and their line, in the three panes and in the block view, as the diff does (#879).
- Changed: with the focus in an unchanged part of the three-pane result, F7 and Shift+F7, the
  Previous and Next conflict buttons and the command palette go to the conflict after or before
  it, not the one focused last (#879).
- Changed: for screen readers, the three-pane merge editor names a block's side panes after
  their side ("Ours (main)", "Theirs (feature)", "Base") and reads the unchanged lines once, from
  the result, instead of once per pane (#880).
- Added: the shortcut sheet and the guide list the merge editor's own Undo and Redo keys; the
  guide describes resolving a file from the keyboard alone and with a screen reader (#880).

### Edit older commits

- Added: **Edit commit…** in the commit menu (and the palette) stops at the commit with its
  changes staged on its parent. The status strip says "Editing 1a2b3c4: change the files, then
  Continue", and the commit area offers the commit's message, editable. Continue commits the
  staged result with that message and the commit's author, then replays the commits after it;
  Abort puts the branch back. A paused edit survives a restart, and the whole edit is one Undo
  step ("Edit 1a2b3c4") (#882).
- Added: with changes staged, the commit menu (and the palette) offers **Fold staged changes
  into this commit**: like `git commit --fixup` and `git rebase --autosquash` in one step, the
  commit keeps its message and author and the commits after it are replayed (the latest commit
  is amended). A conflict pauses it like any rebase; Abort puts the branch back with the changes
  staged again. One Undo step ("Fold staged changes into 1a2b3c4") (#883).
- Added: **Split commit…** in the commit menu (and the palette) stops at the commit with its
  changes staged, like Edit commit…. Unstage what belongs in a later commit, then **Commit
  part** commits what is staged with the message in the commit area (the original one is
  offered first); repeat for each part. Continue commits what is still staged as the last part
  and replays the commits after it. Every part keeps the commit's author; the status strip says
  "Splitting 1a2b3c4", and the whole split is one Undo step ("Split 1a2b3c4"). Edit commit… has
  Commit part too (#884).
- Changed: Continue at an edit stop no longer carries changes that aren't staged into the
  replay: it asks whether to commit them with the commit, discard them, or cancel. When nothing
  of the commit is left, Continue drops it, as Drop would (#884).
- Changed: the rebase planner's **Edit** step stops like Edit commit…: the commit's changes
  staged on its parent, "Editing 1a2b3c4: change the files, then Continue" in the status strip,
  the commit's message editable in the commit area, and **Commit part** to split it. The guide's
  Rebase and Rewriting pages describe it, with a new screenshot (#885).
- Internal: an E2E flow per app edits an older commit's file, continues and checks the history
  (#885). The guide's merge editor screenshot shows the three panes (#933).

### Windows: FAT32, exFAT and ReFS drives

- Fixed: on a FAT32 or exFAT drive (a USB stick, an SD card), changing files in a working tree
  failed with "The parameter is incorrect." (#228).
- Fixed: on ReFS and Dev Drive, rewriting a file dropped its "no scrub data" attribute (#228).
- Internal: Windows CI tests on FAT32, exFAT and ReFS volumes, and tests that swap a link in or
  rename the file between classifying a leaf and reopening it (#228).

### Internal

- Fixed: the guide's merge editor screenshot cut off the editor's toolbar row ("0 of 1
  resolved", Previous conflict); the capture now scrolls to the toolbar (#944).
- Added: tests for the delete fallback to the classic disposition and for a hook that can't be
  resumed, both on Windows (#195).
- Changed: `rust-toolchain.toml` pins Rust 1.100.0 for local builds and every CI and release job,
  which install it from there; Renovate proposes each new release a week after it ships (#220).

## [0.12.0] - 2026-11-12

0.12.0 is the performance release. History, blame and file history read large repositories
many times faster; status reads, the Changes list, the commit graph, commit search, syntax
highlighting and the windows with many branches do far less work; and a render error no longer
blanks the whole window. Besides: external tools get their files as `git checkout` writes them,
the code font field suggests installed fonts, C++ is highlighted with its own grammar again, and
tags offer "Compare with current branch".

### History, blame and file history on large repositories

- Changed: reading history no longer checks each Git object's hash again on every read, as Git
  itself doesn't (it checks in `git fsck` and when objects arrive). File history, line history,
  blame, the commit graph and ahead/behind counts read up to two to three times faster in large
  repositories; a corrupted object now reads as Git would read it instead of failing (#840).
- Changed: blame no longer uses libgit2's blame, which compared the whole folder trees of every
  commit in the history; it now follows the file's own path and compares the file only where it
  changed. On a repository with 130,000 commits, blaming a rarely changed file takes about 3 s
  instead of over a minute, close to `git blame`. Lines get the same commits as before (#839).
- Fixed: blame no longer gives every line to a commit whose unrelated folders couldn't be read
  (libgit2 then lost the file's origin), and in a file whose history had other files renamed
  or deleted next to it, lines are given to the commit `git blame` names (#839).
- Changed: file history and line history read only the folders on the file's path that changed
  between a commit and its parent, instead of looking the whole path up twice per commit. On a
  repository with 130,000 commits, the history of a rarely changed file takes about 4 s instead
  of a minute (#838).
- Changed: file and line history keep their place between loads. "Load more" goes on from
  where the last load stopped instead of searching from the newest commit again, and a refresh
  while HEAD hasn't moved (after a fetch, say) answers at once instead of searching again
  (#860).
- Changed: a long search no longer holds up other actions. It runs a quarter of a second at a
  time, the changes found so far show at once, and the list's footer reads "Searching older
  commits… (N changes so far)" until a page of 300 is found or the history ends. Closing the
  history stops the search (#860).
- Fixed: "Load more" no longer appears under a history of exactly 300 changes (#860).

### Commit graph, search and Contained in

- Changed: the commit graph's later pages, "Go to" and further search pages no longer sort the
  whole history again: the worker keeps the walk order and the branch and tag badges until a
  ref or the selection changes, so scrolling through a large repository costs milliseconds a
  page instead of a full walk each (#835).
- Changed: after a commit, a fetch or a branch switch, the graph no longer reads and sends every
  row already loaded again: the worker sends only the new rows, which loaded rows stay, and the
  branch and tag badges that moved. With thousands of rows loaded a refresh costs about one page
  (#835).
- Changed: hovering a row in the commit graph redraws only the lines it dims or undims, and
  scrolling no longer does work for every loaded commit. Loading the next page of history lays
  out only the new rows (and, when they bring a merge's side branch, a few rows before it),
  instead of every loaded row again (#837).
- Changed: commit search checks each commit with less work: no id formatting or lowercased
  copies for ASCII text, and a `path:` term naming a file or folder compares tree entries
  instead of building a diff for every commit, so a page scans more history (#848).
- Fixed: the commit details' "Contained in" checked one branch or tag at a time and, in
  repositories with long history and many tags, never finished within its time budget. It now
  checks all of them in one walk over the history above the commit (#842).

### Status and the Changes list

- Changed: a status read writes refreshed file times back to the index, as `git status` does, so
  files whose times changed but whose content didn't (a `touch`, a formatter, a copied folder)
  are read once instead of on every refresh. It skips the write when another program holds the
  index lock or changed the index meanwhile (#841).
- Changed: a refresh reads the status and the change stamp in one scan of the work tree instead
  of two, and focusing the window no longer checks for changes while the repository is watched
  (#841).
- Changed: an untracked folder with more than 500 files (a `node_modules` or build output not yet
  in `.gitignore`) is listed as one row, "untracked folder, more than 500 files", instead of file
  by file; smaller folders are listed as before. Its chevron lists its files on demand; selected,
  it offers Stage folder, Ignore folder and Delete folder…, and Stage all and Delete untracked…
  include it (#845).
- Changed: the flat Changes and Staged lists render only the rows in view, so thousands of
  changed files no longer slow the window down (#845).
- Fixed: during a conflict, "Delete untracked…" no longer wraps onto a line of its own under the
  Changes heading; the heading's buttons move under the title together when the pane is narrow
  (#832).
- Changed: "Mark as resolved" streams the file into the repository while it checks it for
  conflict markers, instead of reading the whole file into memory first. Resolving a conflict
  in a multi-gigabyte file no longer needs that much memory (#212).

### Many branches

- Changed: the branch list and the upstream counts keep each branch's ahead/behind count until
  the branch or its upstream moves, and the upstream reads of a refresh reuse the branch list's
  counts instead of counting every pair again. A refresh with many diverged tracking branches no
  longer holds up the repository's other reads for seconds (#112).
- Fixed: undoing a branch delete puts the branch back into the graph's saved branch selection
  (isolating it again if it was the isolated branch); redoing the delete takes it out again
  (#112).
- Changed: an update in one part of a repository's window (opening the command palette, a toast,
  a refresh with nothing new) no longer draws the branch list, the commit graph, the worktree
  panel and the diff again, and the branch list does its per-row work once per update instead of
  once per row. With thousands of branches the sidebar stays responsive (#844).
- Changed: the command palette draws only the commands in view. With 2,000 local branches its
  list held more than 6,000 commands, and opening it drew every one; it now opens in about a
  tenth of the time. Keys, search and what screen readers announce are unchanged (#864).

### Syntax highlighting

- Changed: syntax highlighting uses a WebAssembly build of the Oniguruma regular expression
  library, about 4 times faster on TypeScript in the desktop app on Linux. The app's content
  security policy now allows compiling WebAssembly (`'wasm-unsafe-eval'`), not `eval` (#843).
- Fixed: syntax highlighting no longer freezes the window. It runs in a background worker, in
  chunks, and stops working on a file you have already left; a 2,000-line TypeScript file used
  to block the UI for 1 to 5 s (#843).
- Changed: C++ files (`.cpp`, `.cc`, `.cxx`, `.hpp`, `.hxx`, `.hh`) are highlighted with the C++
  grammar again, so templates, namespaces, `::` and raw strings look right. GLSL inside
  `R"glsl(...)glsl"` raw strings uses a small grammar of Browsitory's own, in place of one that
  has no licence. `.h` files stay C (#326).

### External diff and merge tools

- Fixed: the files handed to a diff or merge tool, and the versions VSCode's diff and merge
  editors show, are now written as `git checkout` writes them: with `core.autocrlf` and the
  `text`/`eol` attributes applied and `ident` expanded. With `core.autocrlf=true` an unchanged
  line no longer shows as changed against the working tree file (#130).
- Fixed: p4merge gets git's virtual base for a conflict where both sides added the file, so it
  highlights only the real differences instead of every line (#159).
- Fixed: a diff or merge tool left open for more than a day no longer loses its files to the
  clean-up of old temporary files, even after Browsitory was closed (#129).
- Added: a stash's file rows have the Open in editor, Reveal in file manager and Open in diff
  tool items, like a commit's (#129).

### Stashes

- Changed: Stash all checks the working tree with one status pass and one read of HEAD's files,
  instead of up to three passes and two reads. On a 20,000-file repository the checks take
  about 180 ms less (#243).
- Fixed: on macOS, stashing a staged rename between two encodings of the same accented name
  (`Café` with `é` as one character, then as `e` plus an accent) was refused as differing "only
  in case", though the names look identical. It is still refused, now saying the names differ
  only in how their accented letters are encoded (#404).

### Render errors

- Fixed: a bug that stopped a view from being drawn blanked the whole window. Now that repository's
  tab shows "Couldn't show …" with the error, **Reload tab**, **Copy details** and **Close tab**,
  while the other tabs keep working; an error outside the tabs shows **Reload** for the whole
  window. The error goes to the log (#846).

### Preferences

- Added: **Preferences → Appearance → Code font** suggests installed fonts while you type,
  monospaced ones first and marked, each shown in its own font. Pick one with the arrow keys and
  Enter or a click; free text, and a comma-separated list of fonts, still work (#858).

### Compare from tags

- Added: a tag's menu, on its badge in the graph and its row in the Tags section, offers
  "Compare with current branch" ("Compare with HEAD" on a detached HEAD), like a branch's: what
  the tag's commit changed since it diverged from HEAD (#119).

### Co-author suggestions

- Changed: "Add co-author" keeps its suggestions until HEAD moves, instead of reading the last
  1,000 commits again each time it opens (#140).

### Help menu

- Added: **Check for updates** in the header's **Help** menu (desktop app), the same check as the
  command palette's command (#852).

### Merge editor

- Fixed: a conflicted file with a run of more than about 120,000 lines threw an error when the
  merge editor read it (#847).

### Internal

- Internal: end-to-end tests for external tools: in the desktop app, Open in editor and Open in
  diff tool run a fake tool configured in Preferences and git config; in VSCode, the diff and
  merge editor handoffs and the `browsitory-file:` content provider (#128).
- Internal: the desktop app's external tools end-to-end test no longer fails now and then on a
  diff tool launch it read while the fake tool was still writing it; a unit test of the fake
  tool runs in CI's `e2e` job (`pnpm test:support` in `e2e/`) (#916).
- Internal: tests pin that file history reuses the graph's walk of the same start, and that a
  line history page walks no further than its last entry (#860, mutation testing).

## [0.11.0] - 2026-11-12

Changes made outside the app now show by themselves, within about a second, and switching tabs
keeps the view; the commit history and the other long lists load and draw only what is on
screen; a commit search highlights its matches in the graph; the Resolve conflict pane is now a
merge editor; drag and drop for branches, commits and the rebase planner; a file tree and a path
filter for the working tree's files; Preferences in groups, with a date format; keyboard
shortcuts for the core loop; plainer words for actions and Git names ("Switch to", "Lines
12–19"); and, in the VSCode extension, a status bar button, an Open key, Timeline commits and
opening the workspace's repository.

### Changes from outside the app, tab switching

- Added: changes made outside Browsitory (a terminal, an editor, another Git tool) show up by
  themselves. While a tab is shown, the app checks every few seconds whether anything it displays
  changed (files, staging, commits, branches, tags, stashes, remotes, an operation in progress) and
  reloads only then; it also checks at once when the window gets the focus back or comes into
  view, and when you switch to a tab. Ignored files and git's own bookkeeping don't count, nothing
  is checked while one of the tab's own actions runs, and a slow check in a very large repository
  stretches the interval (#650).
- Changed: switching tabs keeps the view. The last four tabs you looked at stay loaded, so the
  selected commit, scroll positions, open panels, search and file history are still there when you
  switch back, and the repository is read again only if it changed meanwhile (#642).
- Changed: Left/Right, Home and End in the tab bar move the focus between tabs without switching;
  Enter or Space switches to the focused tab (#642).
- Changed: changes made outside Browsitory now show within about a second. The app asks the
  operating system to report changes to the repository's files (ignored files and git's own
  bookkeeping left out) and checks only then, plus every 30 seconds in case a change went
  unreported (some network drives report none). Where changes can't be reported (no file watches
  left on Linux, more than 20 000 folders, some network drives), it checks every few seconds as
  before (#756).

### History that scales, search in the graph, matching branch colours

- Changed: the commit graph loads older history as you scroll towards the end instead of with
  **Load more**, reads only the next page (from where the last one ended) instead of reloading
  everything, and draws only the rows on screen, so long histories stay fast (#685).
- Changed: a commit search highlights its matches in the graph instead of replacing it: the
  other commits are dimmed, a count shows which match is selected ("3 of 42"), and Enter / F3
  and Shift+Enter / Shift+F3 (or the arrows next to the count) go to the next and previous
  match, loading history or expanding a collapsed merge as needed; **List** shows the matches
  alone as before (#687).
- Changed: a branch's dot in the sidebar and its badges in the graph now have the colour the
  graph draws that branch in; a branch whose latest commit isn't loaded has a grey dot, and
  only the checked-out branch's badge is filled (#673).
- Fixed: the window went blank after scrolling the commit graph past about 30,000 commits (#836).
- Fixed: a commit search that matches few or no commits no longer stalls on a large repository:
  each page spent its time budget re-reading the history up to where the last page ended and
  then looked at one commit, so the search crawled on for hours; a page now gets its full budget
  for searching (#834).

### Long lists

The lists that can grow to thousands of rows now draw only the rows in view, like the commit
history (#685).

- Changed: file, folder and line history, the search's "List" view, the Reflog and the Stashes
  lists render only the rows on screen; screen readers still hear each row's place in the whole
  list, and the keyboard selection scrolls into view as before (#775).
- Changed: a Reflog entry now takes two lines, its message, then "old → new · name · date" with
  short IDs; the full IDs, email, time and summary are in its tooltip, and Restore sits at the
  end of the row (#775).
- Fixed: the stash and reflog lists (and the history pane's long lists) showed blank rows, or
  none, after a section above them opened or closed, or the branch list finished loading, until
  you scrolled. They now follow the move at once (#828).

### Lighter refreshes

- Changed: staging, unstaging, discarding or editing a file no longer reloads the history,
  branches, tags and upstreams; they are read again only when a commit, branch, tag, remote or
  the graph's selection changed, or on Refresh (#685).
- Fixed: actions and checks that ask for a refresh while one runs now share one that follows it,
  so a busy repository no longer works through a queue of outdated refreshes (#222).

### Reads during a fetch, pull or push

- Fixed: while a fetch, pull, push or the automatic fetch was running, opening a commit, a diff,
  blame, file history, search or Refresh waited until the transfer ended; they now answer at once.
  They still wait while a pull merges and runs its `post-merge` hook (#734).
- Internal: a Tauri E2E spec opens a commit while a fetch waits on a remote that never answers,
  then cancels the fetch (#734).

### Conflicts, the merge editor and rebase messages

The Resolve conflict pane is now a merge editor (#720).

- Added: each conflicting block shows the common ancestor's lines behind **Show base**, and can
  take them with **Accept base**, or both sides in either order (**Ours then theirs**, **Theirs
  then ours**) (#720).
- Added: a block's result is editable; typing in it marks the block "Edited", and **Reset**
  clears it. **Edit whole result** edits the whole file as one text, with conflict markers
  around blocks still open, and asks before dropping those edits on the way back (#720).
- Added: long runs of unchanged lines between the conflicts are collapsed to the lines next to
  them, with **Show N unchanged lines** (#720).
- Changed: a conflict where both sides added the file gets the merge editor, with an empty base,
  instead of the keep-ours/keep-theirs/delete choice (#720).
- Changed: **Accept both** is now **Ours then theirs** (#720).
- Fixed: a line of the file that looks like a conflict marker (`=======`, `<<<<<<<`) is no
  longer read as one when the file is split into blocks (#720).
- Fixed: opening a conflicted binary file read a merge result libgit2 doesn't produce for
  binary files (undefined behaviour, which could show an empty block view). The pane now says
  the file is binary and offers the whole-file choices (#720).
- Fixed: the rebase planner's combined message for Squash rows held only the commit summaries,
  so a rebase started without editing it dropped every body. It now starts from the whole
  messages of the group's first commit and its Squash rows, bodies included, as `git rebase -i`
  does; Start waits until they have loaded (#721).
- Added: the merge editor highlights our lines, their lines, the base and the unchanged lines
  like the diff, following its **Syntax highlighting** option; the result fields stay plain
  text (#763).
- Added: merge editor keys. With a conflict block focused, **1** to **5** take **Accept ours**,
  **Accept theirs**, **Ours then theirs**, **Theirs then ours** and **Accept base**; **F7** and
  **Shift+F7** go to the next and previous conflict. In a result field the digits type text:
  **Esc** goes back to the block. The keys are on the buttons, in the shortcut sheet's new
  "Merge editor" group, and in the command palette, which offers the moves and choices for the
  current conflict too (#764).

### Rebase onto a branch, file list tree and filter

- Added: **Rebase current branch onto ⟨branch⟩…** in the menus of local and remote branches and
  in the command palette opens the rebase planner on that branch, named in the planner's title.
  It is disabled with the reason during a merge, rebase or cherry-pick and not offered on a
  detached HEAD (#619).
- Changed: **Undo** names a rebase onto a branch by the branch ("Rebase onto origin/main"), also
  for **Rebase** after a pull has diverged, instead of the full ref name (#619).
- Added: **Filter files** above the working tree's file list narrows both groups by path. While it
  hides files, the group buttons act on the shown files only and say so (**Stage 3 shown**,
  **Discard 3 shown…**, whose confirmation lists those files), and Ctrl+A selects only the shown
  files (#729).
- Added: **Flat** and **Tree** switch the file list to a tree of folders, with collapsible folders
  and a stage or unstage button per folder. The choice is kept on this computer and is also in
  **Preferences → Diff** (#729).

### Drag and drop

- Added: drag a branch onto the current branch, or the current branch onto another branch, to
  merge or rebase. The drop opens a small menu with both choices, naming both branches, and they
  open the same merge dialog or rebase planner as the menu items. Drops that can't run (two
  branches that aren't current, a detached HEAD, a busy repository) are refused, and the label
  beside the pointer says why. Collapsed folders open while a branch hovers them (#744).
- Added: drag a commit from the graph onto the current branch in the sidebar to cherry-pick it.
  The drop opens a menu with "Cherry-pick ⟨commit⟩ onto ⟨branch⟩" and Cancel; a row of a
  multi-selection drags all selected commits (picked oldest first), and a merge commit opens the
  parent picker. Other branches, a detached HEAD, HEAD itself, a selection with a merge commit and
  a busy repository refuse the drop, and the label beside the pointer says why (#744).
- Added: rows of the rebase planner can be dragged by their handle and dropped above or below
  another row; a line shows where the row will land, and Escape cancels. Move up/Move down and
  Alt+Up/Down still work, and every move, by drag or by key, is announced to screen readers
  (#744).

### Staging several files, no-ff pulls

- Changed: staging, unstaging or ignoring several files at once (the selection bar, Stage all,
  Unstage all) is one Undo step ("Stage 3 files") instead of one per file, and all or nothing:
  when one file can't be staged, none is. The notification after ignoring several files now
  offers Undo (#528).
- Fixed: Pull follows `pull.ff=false`, `--no-ff` in `branch.<name>.mergeoptions` and
  `merge.ff=false` like `git pull`: a branch that could fast-forward gets a merge commit instead,
  ready to commit in the commit box. A configured rebase still fast-forwards (#740).

### Stashes

- Changed: "Restore staged changes" is no longer a checkbox above the stash list that silently
  changed what Apply and Pop do. Apply and Pop bring staged work back as staged; a stash that
  holds staged changes also offers "Apply as unstaged" and "Pop as unstaged", in its menu, in
  the stash view and in the command palette (#657).
- Added: each stash shows its branch, age and file count ("On main · 2 days ago · 3 files"),
  and its name without git's "On main:" prefix; the full message and date are in its tooltip
  (#657).
- Fixed: a stash holding a file that replaced a folder, or files in a folder that replaced a
  file, was refused by Apply and Pop. Browsitory now applies it in git's order, removing the
  folder or file first; a folder holding anything the stash doesn't know is still refused
  (#229). `git stash pop` itself can't apply the second kind.
- Fixed: "Stash file…" on a folder that replaced a file was refused although git accepts it.
  It now stashes the file's deletion with the folder's untracked files, and Pop brings both
  back; git's own stash leaves the deletion out. A folder that also holds ignored files, a
  repository or untracked files left out is still refused, where git stops half-way (#207).
- Changed: the stash view's **Apply**, **Pop** and **Drop…** show why they are blocked in
  visible text tied to the button, and both Drop confirmations tie their reason to the confirm
  button (#772).

### Commit notification, disabled reasons and the working tree

- Changed: after a commit the notification offers **Push** and **Undo** together, not one or the
  other (#733).
- Fixed: the commit notification's **Push** pushed whichever branch was current when it was
  clicked; it now pushes only the branch it names and otherwise says "Not pushed: main is no
  longer the current branch." (#735).
- Fixed: in the working tree, the Changes heading broke its labels over two lines and the diff
  options toolbar was cut in half when the file list grew (#752).
- Fixed: "Clone repository" opened as a dialog inside another dialog, announced twice by screen
  readers (#761).
- Changed: the buttons of the sidebar panels (worktrees, tags, pull requests, branches, remotes),
  the operation status strip, the rebase planner, the files-at-a-commit view and the remaining
  dialogs and confirmations say why they can't run in visible text tied to the button, not only
  in a tooltip (#664, #748).
- Internal: the worktree E2E spec waits for the new tab's current-branch row; since tabs stay
  mounted, it could read the row of the tab it was leaving.
- Fixed: the commit toast's Push, clicked right after the toast appeared, sometimes did nothing,
  and clicked right after another branch was checked out it could push that branch instead of
  saying "Not pushed: … is no longer the current branch" (#816).

### Preferences in groups, date format

- Changed: Preferences shows one group of settings at a time (Appearance, Diff, Commit message,
  Fetch, External tools and, in the desktop app, Updates), picked in a list on the left or with
  the arrow keys. The dialog keeps its size, only the group scrolls, and it reopens on the group
  you looked at last (#784).
- Added: **Filter settings** in Preferences keeps the groups with a setting whose name matches
  and says how many match (#784).
- Added: next to an untracked file's **Show contents**, **Diff preferences** opens Preferences on
  the Diff group, where **Expand untracked files** is (#784).
- Added: Preferences → Appearance → Date format: "ISO" (2026-11-11 14:05, the default) or
  "System", your operating system's short date and time (#675).
- Changed: every date in the app follows it: the history's date column, search results, file
  history, the Reflog, stashes, commit details, patches and the "Fetched … ago" tooltip. None
  shows seconds any more, and "3 days ago"-style labels are always in English, like the rest of
  the app (#675).

### One word for each action, plain names instead of Git syntax

- Changed: branch menus say "Switch to" instead of "Checkout", like the command palette; commit
  and tag menus say "Switch to (detached)", the palette "Switch to abc1234 (detached)" and "Switch
  to tag v1 (detached)", and the New branch form "Switch to it after creating" (#681).
- Changed: the release notes are "What's new" everywhere: the header button's name now matches
  its tooltip, and the dialog has the same title (#681).
- Changed: a diff's hunk headers read "Lines 12–19" (or "Removed lines 4–6") instead of git's
  "@@ -10,6 +12,8 @@"; the tooltip names the lines before and after the change (#682).
- Changed: the Reflog panel names branches as the branch list does, "main" rather than
  "refs/heads/main", in its picker, Restore buttons and confirmation (#682).
- Changed: removing a remote that a branch still tracks names those branches in plain words
  instead of a bracketed list (#682).

### Controls, states and targets

- Changed: a pressed toggle button (diff options, Blame, Changes / All files, conflict choices,
  the search bar's toggles) now has a tinted fill and an outline, not just a text colour; Wrap
  lines on Auto has a dashed outline (#667).
- Changed: lines selected for partial staging in a diff have a bar on their right edge, apart from
  the line cursor's bar on the left, and keep their added or removed colour (#722).
- Changed: the Tags section no longer has a push form. Tick tags (or **Select all**) and a bar
  under the list pushes them; a tag's menu still pushes one tag and the command palette all of
  them (#674).
- Changed: Undo and Redo, the close buttons, the context-line stepper, toasts and the signature
  badge use the same icon set as the rest of the app instead of text symbols; the commit
  message's column ruler is a faint dotted line, no longer a second border (#674).
- Fixed: the remove button on a co-author chip, an error's dismiss button and a toast's dismiss
  button are at least 24 × 24 px (#672).
- Fixed: the command palette, keyboard shortcuts, rebase plan, open repository and clone dialogs
  have names for screen readers; the commit graph's menus are no longer inside its list; and an
  invalid search is read out politely once you stop typing, not as an alert on every keystroke
  (#669).

### Keyboard shortcuts for the core loop

- Added: global shortcuts for Refresh (F5), Fetch (Shift+F5), Pull (Ctrl+Shift+L), Push
  (Ctrl+Shift+K), Stage all (Ctrl+Shift+A) and the commit message (Alt+C), with ⌘ for Ctrl on
  macOS. They run the command palette's commands, so they follow the same rules, and a
  notification says why one can't run. The palette shows each command's shortcut beside it, and
  the toolbar's tooltips name theirs (#646).
- Added: Ctrl+Tab and Ctrl+Shift+Tab switch to the next and previous tab, Ctrl+1 … Ctrl+8 to the
  first eight tabs and Ctrl+9 to the last (#646).
- Added: F6 and Shift+F6 move the focus between the toolbar, the sidebar, the history and the
  diff, back to where it last was in each; the sidebar, history and diff are labelled regions
  for screen readers (#670).
- Changed: shortcuts are written for the platform in one notation: Ctrl+Shift+Z on Windows and
  Linux, ⇧⌘Z on macOS, arrows as ← ↑ → ↓ (#683).
- Internal: the global shortcuts are defined once in `frontend/src/lib/keybindings.ts`, for the
  shortcut list, the palette, the tooltips and the VS Code keybindings to come (#679).

### First launch, the repository picker and patches

- Changed: the first launch shows a welcome in the repository picker (open, clone or create a
  repository, the command palette key, the user guide) instead of opening release notes (#688).
- Added: the picker's recent repositories can be removed from the list, a repository whose
  folder is gone is marked "Folder not found", and a filter narrows the recent repositories and
  workspaces (#690).
- Changed: "Open workspace root" is now "New workspace…", next to the picker's workspaces (#690).
- Added: in VSCode, the picker's and palette's Clone… and New repository… run VSCode's Git: Clone
  and Git: Initialize Repository (#680).
- Changed: in VSCode, an update shows a notice with a "What’s new" button instead of opening the
  release notes over the editor (#680).
- Added: recent repositories can be pinned in the picker: pinned ones come first and don't count
  against the ten recent ones (#766).
- Added: "Apply as commits" runs the `applypatch-msg`, `pre-applypatch` and `post-applypatch`
  hooks like `git am`; a rejecting hook takes the whole patch file back and shows its output
  (#747).

### VSCode extension

- Changed: "Browsitory: Open" opens the repository of the file in the active editor, or else, when
  the panel opens, the workspace folders' repositories, instead of always showing the picker
  (#677).
- Added: "Open in Browsitory" in the Explorer's folder menu and in a Git repository's menu in the
  Source Control view; "Show File History in Browsitory" in the Explorer's file menu, an editor
  tab's menu and the editor's menu; "Show Line History in Browsitory" for the lines selected in an
  editor (#678).
- Changed: the Browsitory panel has its icon, opens in the active editor group and comes back
  after a window reload (#678).
- Fixed: Ctrl/Cmd+K, F5, Shift+F5, F6, Shift+F6, Ctrl/Cmd+Alt+B, Ctrl+Tab, Ctrl/Cmd+1 … 9,
  Ctrl/Cmd+W, Ctrl/Cmd+Shift+T and Back/Forward ran VSCode's commands instead of Browsitory's. The
  extension now binds them while Browsitory has the focus; change them in VSCode's Keyboard
  Shortcuts (#679).
- Added: a Browsitory button in the status bar that opens the active editor's repository; the
  `browsitory.statusBar.enabled` setting hides it (#759).
- Added: Ctrl+K Ctrl+G (Cmd+K Cmd+G on macOS) runs "Browsitory: Open" (#759).
- Added: "Show Commit in Browsitory" on a commit in VSCode's Timeline view selects it in the
  graph, loading as much history as it needs (#759).
- Changed: when "Browsitory: Open" opens the workspace's repositories and finds several, it asks
  which ones to open, all ticked. It also finds the repositories directly inside a workspace
  folder, including ones nested in another repository (#759).

### Reliability

- Fixed: a mixed or hard reset writes the index before it moves the branch, as git does. HEAD
  no longer points at the target while the index still holds the old tree, and a reset whose
  index can't be written (another git command holds `index.lock`) now leaves HEAD where it was
  instead of moving it (#796).
- Internal: a file watcher marks itself stopped before it reports `Stopped`, so a watch
  requested right after the event starts a new watcher; this also fixes the flaky
  `growing_past_the_folder_limit_stops_the_watcher` test (#797).
- Internal: CI's mutation testing comment says "Mutation testing didn't run" when an earlier
  step failed or cargo-mutants left no results, instead of claiming every mutant was caught
  (#798).
- Internal: the blame E2E spec looks the commit message field up again before it types into
  it, and a failed commit there records the commit box's state and a screenshot (#810).
- Fixed: a toast's Undo clicked the moment the toast appeared could be refused as stale, or
  ignored, instead of undoing its step (#769).
- Internal: the lock-contention burst test compares the waits the worker chose instead of
  elapsed time, so a slow CI runner no longer fails it (#736).
- Internal: the search test fixture with a merged side branch commits at fixed times, so the
  order of its two sibling commits no longer depends on the clock (#770).
- Internal: the VSCode E2E specs run git through one shared helper,
  `extension/e2e/src/support/git.ts`, which the Tauri E2E suite re-exports (#524).

### E2E reliability

- Internal: the Tauri E2E specs open a sidebar section only after the workspace's first load has
  landed, and check that it opened. A header clicked while the branch tree replaced its loading
  placeholder could miss (the worktree spec never saw its form, #824) or move far out of view
  with its body, where the windowed reflog list renders no rows (#825). The reflog spec also
  scrolls its list into view before counting entries, and both specs print the sidebar's state
  and save a screenshot when they fail.
- Internal: the Tauri E2E specs set a field that starts with text (the reword and combined
  messages) inside the page with the new `setValueInPage` helper, so a render between
  WebKitWebDriver's clear and its typing no longer leaves the old text in front of the new
  (#783, #791).
- Internal: the cherry-pick sequence and stash specs click Continue and Pop with `clickInPage`,
  which waits while the button is blocked (`aria-disabled`), instead of a click that a blocked
  button ignores (#790).
- Internal: the history rewrite spec reports the history when its wait gives up, not when it
  started (#783).
- Internal: WebDriver's own clicks reach the whole 1200×800 E2E window, its bottom edge
  included; the bottom-edge failures seen on #749 no longer reproduce on main (#751).

### Code clean-ups

- Internal: a push is built from named steps (negotiation with lease check and `pre-push`, the
  remote's per-ref answer, the outcome) and takes its settings as one `PushRequest` instead of
  nine parameters (#502).
- Internal: the path-limited stash is built in named steps (select and check the paths, build
  the index, untracked and worktree commits, push `refs/stash`, revert the paths) instead of one
  300-line function (#502).
- Internal: creating a worktree, the search query tokenizer and the line-staging patch builder
  are split into named steps instead of one long function each (#502).
- Internal: the work-tree walk that guards every write against links swapped in meanwhile is
  written once, over a small per-platform trait (`HeldDir`), instead of once for Unix and once
  for Windows; the shared loop is tested on every platform with a scripted folder (#501).
- Internal: the wire snapshot tests of the desktop app and the VSCode sidecar take their shared
  DTO cases from one list (`crates/test-support/wire_cases.rs`), so a new DTO is snapshotted by
  both transports at once (#488).
- Internal: committing and a cherry-pick or revert step are built from named phases (merge state
  read, hooks, commit, clean-up) instead of one long function each (#502).
- Internal: the git-core tests share `git_ok`, `head_id` and `index_paths` from
  `tests/common` instead of keeping their own copies (#505).
- Internal: stash, conflict resolution and rebase write a work-tree file's blob through git's
  clean filters with one shared helper (#210).

### Internal: faster pull request checks

- Internal: CI cancels a pull request's older run when a new commit is pushed, skips the Rust
  jobs when a pull request changes no file they read, and skips every build and test job for
  documentation-only pull requests. The skipped jobs still report success, so the required
  checks keep their names; pushes to main, the schedule and dispatched runs run everything.

## [0.10.0] - 2026-11-11

A security fix for "Apply patch…" (GHSA-5j9j-62g5-846m), and the work of 0.9.0 and 0.10.0
together (0.9.0 was not released on its own). Fetch, pull and push no longer block the window;
the working tree has one file list with one diff viewer; the command palette lists every command,
with fuzzy matching; a preview before applying a patch, which can also apply it as commits;
toasts with Undo; errors that sit above the workspace instead of covering it, with hints;
disabled controls that say why; menus with groups and key hints; large diffs that load on
request; repository tabs you can rename and reopen; Back and Forward through commits; appearance
preferences; and Git defaults that follow git (pull strategy, switching with local changes,
identity).

### Applying patch files with symbolic links

- Security: "Apply patch…" could write through a symbolic link that the same patch creates, so
  a crafted patch could install a git hook (run on the next commit) or place links outside the
  repository. A patch whose entries lie below a link it creates is now refused, as `git apply`
  refuses it ("beyond a symbolic link") (GHSA-5j9j-62g5-846m).
- Security: link entries are checked like files for names that open the `.git` folder on some
  file system (`git~1`, `.git.`, `.Git `, HFS+ ignorable characters, `\` as a separator), and a
  link can no longer point into the `.git` folder or be named `.gitmodules`, `.gitattributes`,
  `.gitignore` or `.mailmap` (GHSA-5j9j-62g5-846m).

### Errors, disabled reasons and empty states

- Changed: error banners sit between the toolbar and the columns instead of covering the toolbar
  or the top of the graph (#663).
- Added: error banners have **Copy details**, and an error you didn't dismiss is listed under the
  next one as an earlier error instead of being replaced (#663).
- Added: hints with the next step for every error kind that has one, such as "Commit or stash
  your changes, then try again." (#663).
- Changed: a toolbar button that can't run (Fetch, Pull, Push, Stash, Undo, Redo) stays
  focusable and shows why under itself on hover and focus; a disabled menu item shows its reason
  under its label; dialogs' buttons are described by the reason they show (#664).
- Added: a search in the command palette also lists the matching commands that can't run now,
  greyed out under **Unavailable now** with the reason (#644).
- Added: **Working tree clean** and, in a repository without commits, **No commits yet** with
  how to make the first commit, instead of an empty pane (#665).
- Added: conflicted files head **Changes** in a **Conflicts** group (#665).
- Added: the status strip says what is merged into which branch ("Merging feature/x into main",
  "Cherry-picking onto main") and has **Go to conflict**; during a rebase it also has **Continue**
  (#665).

### Patches: preview, apply and stage, apply as commits

- Added: **Apply patch…** shows a preview first: each mail's subject, author and date, the files
  it changes with their lines added and removed, and why it doesn't apply when it doesn't. From
  there, **Apply**, **Apply and stage**, or **Apply as commits**, which commits each
  `git format-patch` mail with its own author, date and message (you are the committer), all or
  nothing and one Undo step (#658).
- Added: a commit's file menu has **Revert this file and stage** and **Apply this file and
  stage**; Shift+click on **Revert hunk** or **Apply hunk** stages the result too. A file with
  unstaged changes is named in the error instead of a bare "does not match index" (#536).

### Menus, visible controls and drops

- Changed: menus are split into groups by separators, with destructive items in a group of their
  own; the commit menu goes from new refs and applying the commit to moving HEAD, history edits,
  Drop, exports and copying (#645).
- Added: menu items show the key that does the same outside the menu, such as `s` for staging the
  selected files, F2 for Rename tab… and Ctrl/Cmd+Shift+T for Reopen closed tab (#645).
- Changed: the stage and unstage buttons of changed files and the close buttons of tabs are always
  shown, dimmed until you point at them; hunk actions look like buttons, and Discard hunk is red
  (#648).
- Added: Apply patch… in the repository toolbar, and a + in the Tags panel header for Create tag
  at HEAD… (#649).
- Added: in the desktop app, drop a folder on the window to open it as a repository, or a .patch,
  .diff or .mbox file to apply it to the working tree (#649).

### Git conventions by default

- Changed: a diverged pull follows your pull strategy instead of always asking: the menu next to
  **Pull** (and the command palette) offers **Pull with merge**, **Pull with rebase** and **Pull,
  fast-forward only**, remembered per repository; until then `branch.<name>.rebase`,
  `pull.rebase` and `pull.ff=only` decide, and only without any of them does the pull ask, with
  **Remember for this repository** (#652).
- Fixed: **Rebase** for a diverged pull opened the interactive planner; it now runs a plain
  rebase onto the upstream. The prompt names the branches as `origin/main`, not
  `refs/remotes/origin/main` (#652).
- Added: switching branches over local changes in the way names the files and offers **Stash and
  switch**, which can re-apply the changes on the new branch; one **Undo** goes back (#653).
- Fixed: the New branch form switched to the new branch even with **Check out after creating**
  unticked. The option now works, starts ticked and is remembered; creating and switching is one
  undo step. **Create branch…** is in the command palette and the branch switcher (#656).
- Added: committing without `user.name` and `user.email` opens a dialog to set them, for all
  repositories or this one, and then commits, instead of git's raw error (#689).
- Added: the toolbar's branch switcher lists remote branches without a local branch and checks
  one out as a new tracking branch (#726).

### Diffs: large files and screen readers

- Changed: a diff with more than 1,000 lines, or of a generated file (a lockfile, minified code
  or a source map), starts collapsed behind Load diff (N lines), and a loaded diff shows 2,000
  lines at a time with Show more and Show all (#686).
- Fixed: screen readers could not tell added lines from removed ones. Each diff line is now read
  as "Added line 12", "Removed line 7" or "Unchanged line 3" before its text, and selectable lines
  are list options that say whether they are selected, with the line cursor followed as it moves
  (#668).

### Git correctness

- Added: a rebase step waiting for the signing program shows "Waiting for the signing program…"
  with Cancel in the hook status strip, as a commit, cherry-pick or revert already did. Before,
  only the 120 s timeout ended it (#151).
- Fixed: a merge, cherry-pick, revert or rebase step that adds files below a folder link the
  current commit tracks now stops at git's directory/file conflict instead of refusing: the link
  is moved aside as `<name>~HEAD` and the files are written in a real folder, never through the
  link (#155).
- Fixed: committing a merge drops only the merged commits that are already in another parent's
  history, as `git commit` does, instead of keeping all of them or none. A `--no-ff` merge
  (fast-forward set to "never") keeps every parent (#227).
- Fixed: with `core.ignorecase` on and two files whose names differ only in case already
  committed, a merge failed with "1 conflict prevents checkout". It now says which two files
  are in the way. A clone of such a commit under a global `core.ignorecase` is refused before it
  checks anything out (#512).

### Fetch, pull and push without a blocking dialog

- Changed: a running fetch, pull or push no longer covers the window with a dialog. The
  repository toolbar names it ("Pushing main to origin/main") with a progress bar, the objects
  and bytes so far, and Cancel; the history, the sidebar and what is already shown stay usable.
  Actions that change the repository wait until it ends, and their buttons say what for (#659).
- Fixed: Escape closed the transfer dialog, and its progress and Cancel were gone for the rest
  of the transfer (#659).
- Added: an action that takes more than a moment shows "Working…" with a progress bar in the
  toolbar, and the graph and the Branches section show placeholder rows while a repository
  opens, instead of looking empty (#660).
- Changed: fetch, push and pull notifications say what happened: "Pushed 2 commits to
  origin/main", "Nothing to push: origin/main already has main", "Fetched from origin. main is
  3 commits behind origin/main." with a Pull button, "Fast-forwarded main to origin/main",
  "Deleted origin/topic". The toolbar's Fetch and Push and the palette's now notify too (#661).

### Working tree: file list and one diff viewer

- Changed: the working tree is a compact list of the changed files, with Changes and Staged
  always on screen, and one diff viewer for the current file, instead of a stack of every
  file's diff (#639). Each row has a status letter, the path, its menu and its stage button.
  When the current file is staged or discarded, the next one takes its place.
- Added: Down and Up move on into the other group past either end, Enter moves into the diff,
  and j and k in the diff go to the next and previous file (#639).
- Changed: with Expand untracked files off, an untracked file's diff loads when you press Show
  contents (#623, #639).
- Changed: each pane has one diff toolbar, with icon buttons and tooltips, that applies to every
  file in it and stays on one line, instead of a row of options above every file (#640).
- Changed: Split view and Show whole file are remembered, and Unified view and Split view are one
  pressed-state control; Wrap lines sets the Word wrap setting for every diff (#640).

### Command palette and help

- Fixed: the highlighted command in the palette stays in view as you move through the list
  (#643).
- Added: PageUp/PageDown, Ctrl+Home/Ctrl+End (Home/End before you type) and the mouse pointer
  move the palette's highlight (#643).
- Changed: with nothing typed, the palette lists every command, recently used ones first under
  "Recent", then the rest in their groups; before, the list stopped after 50 (#644).
- Changed: the palette matches the words you type in any order and shortened ("sw main",
  "ftch"), ranks the best match first and marks the matched letters (#644).
- Added: the header's "Ctrl+K" is now a "Commands" button that opens the palette, and a Help menu
  next to it opens the user guide, the keyboard shortcuts and what's new (#647).
- Fixed: the letters a search matched no longer split a command's name with gaps ("Browse
  branch es"); the name reads as one word again (#644).

### Commit flow and commit details

- Added: the commit button says what it commits and where, for example "Commit 3 files to main",
  and its arrow has "Commit and push" and "Amend last commit" (#651).
- Added: after a commit a notification names it ("Committed 4c5beaf to main") with Push, or with
  Undo when the repository has no remote (#651).
- Changed: an empty commit message now says why Commit is disabled (#651).
- Added: the commit details show the committer when someone else committed the change, the
  commit time when it differs from the author's, and the branches, remote branches and tags that
  contain the commit (#641).
- Changed: the commit details show dates as `2026-09-08 21:00`, the form the graph uses (#641).

### Notifications and Undo

- Changed: notifications have a tone (done, information, warning, error), each with its own
  colour and icon; warnings and errors also say so to screen readers. They stay 5 to 15 seconds
  depending on their length, at least 10 seconds with a button, and errors stay until closed.
  None close while the pointer is over one or focus is on one of its buttons (#661).
- Changed: notifications name what changed: "Cherry-picked 1234567", "Reset main to 1234567",
  "Checked out 1234567", "Switched to topic", "Created tag v1", "Pulled origin/main" (#661).
- Added: the notification of an operation Undo covers (commit, amend, discards, staging lines,
  reset, checkout, branch switch and delete, tag creation, fast-forward, squash merge,
  cherry-pick, revert, history edits, restoring or applying a file) has an Undo button. It
  undoes only that operation, and says so instead when something newer was recorded (#662).
- Fixed: the discard, Delete untracked, Reset hard and force-delete branch confirmations said
  the action can't be undone; Undo restores all of them, and they now say so (#662).
- Changed: Discard hunk and Discard lines (and `d` in a diff) no longer ask twice: they discard
  at once, and the notification offers Undo (#662).

### Repository toolbar and the sidebar in narrow windows

- Fixed: below 900 px (or in a VS Code editor group that narrow) the sidebar could not be opened
  at all. It now opens as a drawer over the graph from the toolbar's sidebar button,
  Ctrl/Cmd+Alt+B, "Show sidebar" in the palette or a double-click on the divider; Escape or a
  click beside it closes it. The breakpoint follows the workspace's width, not the window's (#635).
- Fixed: Pull on a diverged branch showed no Merge / Rebase prompt while the Branches section was
  closed, and froze the page behind an invisible prompt while the sidebar was hidden. The prompt
  now opens over the workspace, and Pull waits until it is answered (#636).
- Changed: one repository toolbar runs above the sidebar, graph and diff: the sidebar button, the
  current branch, Back/Forward, Fetch/Pull/Push with the ahead/behind counts, Stash, Undo/Redo and
  a new Refresh button. Stash moved there from the commit dock, and the sidebar's second Pull is
  gone (#637).
- Added: the toolbar shows the current branch and its upstream ("main → origin/main"). A click
  opens "Switch branch…", a filterable list with the recently used branches first, which can also
  create a new branch from the current one and switch to it (#638).

### Conflict choices and the rebase planner

- Fixed: the Resolve conflict pane chose "ours" for every block on its own, so one click on
  **Save resolution** took your side everywhere. Blocks now start unresolved, the chosen button
  shows as pressed, a **Result** preview shows what the block becomes, a counter reads
  "1 of 3 resolved", and **Save resolution** is enabled only when every block has a choice (#654).
- Changed: the conflict sides are labelled with what they are ("Ours (main)", "Theirs
  (feature)"; during a rebase the base and the rebased branch), use neutral colours instead of
  the diff's addition and deletion colours, and **Previous conflict**/**Next conflict** move
  between blocks (#654).
- Fixed: rewording a commit in the rebase planner dropped its body, because the message field
  held only the summary. Reword now loads the whole message into a multi-line field (#655).
- Fixed: the rebase planner showed nothing while it listed the commits and nothing when that
  failed, with **Start rebase** enabled on an empty plan. It now shows loading, error (with
  **Retry**) and empty states, and Start waits for them (#655).
- Changed: the rebase planner says its rows are oldest first, names each row's controls after its
  commit for screen readers, and moves the row you are in with Alt+Up/Alt+Down (#655).

### Theme and contrast

- Fixed: the light theme failed the WCAG AA contrast levels in many places: the accent as text,
  pressed toggles, branch badges, the Commit button, muted text, comments in code, placeholders
  and input borders. Both themes now meet WCAG 2.2 AA (4.5:1 for text, 3:1 for controls and the
  focus ring), and a selected row carries a marker besides its tint (#666).
- Added: forced-colours (high contrast) support: the system's colours, with the focus ring,
  selection and pressed toggles kept visible (#671).
- Changed: in VSCode the panel follows the editor's theme (light, dark and high contrast) and its
  fonts; the theme button is gone there (#676).
- Added: Preferences > Appearance > Theme: System, Light or Dark. System is the default and
  follows the system's setting while Browsitory runs (#691).
- Internal: every colour is a token in `styles/tokens.css`, and a test checks the contrast of the
  token pairs and that no stylesheet or script writes a colour literal (#666).

### Internal: collapse merges tests

- Internal: tests for the "Collapse merges" code the mutation job found untested in #716: the
  commit time order of a merge's merged commits, an octopus merge holding a hidden commit, a
  commit that is already a row of the collapsed graph, and the binary search over a first-parent
  line at every position, guarded so a search that never ends fails instead of hanging (#617).

### Collapse merges in the commit graph

- Added: **Collapse merges** (a toggle in the search bar above the graph and a palette command,
  saved per repository) makes the graph follow only first parents from every branch it shows.
  Each merge row gets a **+N** control with the number of commits it brought in; it expands them
  right below the merge, with their lanes, and **−N** hides them again (→ and ← from the
  keyboard, **More** for a long side branch). Off by default (#617).
- Added: Go to commit and the parent key expand the collapsed merge that holds a commit instead
  of saying the graph does not show it; commit search still finds those commits. With a single
  isolated branch, merge rows expand the same way (#617).

### Lines changed per commit

- Added: each commit row in the history shows the lines the commit added and removed, for
  example `+12 −3` (green and red; read as "12 lines added, 3 removed"). They are counted as
  rows come into view, a batch at a time, without holding up the graph; a commit too large to
  count in time shows none. The ± button in the search bar above the graph hides them (#620).
- Added: the commit details show the totals ("4 files changed, +120 −37") and each file's
  header its own `+/−`, or "Binary" (#620).

### Open .gitignore and .gitattributes

- Added: "Open .gitignore" and "Open .gitattributes" in the repository tab menu and the command
  palette open the file at the work tree root in the configured editor (in VSCode, in VSCode).
  When the file is missing the item reads "Create .gitignore" / "Create .gitattributes": it
  creates an empty file at the root, never overwriting one or writing through a symbolic link,
  and opens it. Bare repositories don't offer them (#624).

### Expand untracked files preference

- Added: **Preferences → Diff → Expand untracked files** (on by default). Off, untracked files
  start collapsed in the uncommitted changes and load nothing until expanded; changes to tracked
  files still start expanded. A file you expand or collapse keeps that state across refreshes
  for as long as the pane is open. Stored as `[diff] expand_untracked` (#623).

### Command palette: commits beyond the loaded history

- Fixed: a commit selected in the search results or a file history that the graph hasn't loaded
  yet got none of the commit commands in the palette. Cherry-pick, Revert, Reset, Checkout and
  Create tag are now offered for it, with the same availability as the commit menu (#515).

### Show whitespace in diffs

- Added: a "Show whitespace" toggle in the diff toolbar and the command palette ("Toggle
  whitespace"). It marks spaces with a faint dot, tabs with a faint arrow as wide as the tab size
  and carriage returns with a faint return sign, and highlights trailing whitespace on added
  lines, in the unified and the split view. Copied text and staging are unchanged; it is off by
  default and remembered with the other diff settings (#616).

### Copy from the context menus

- Added: a commit's menu ends with **Copy SHA**, **Copy short SHA**, **Copy subject**, **Copy
  author** (`Name <email>`) and **Copy date** (ISO 8601); a local or remote branch's menu has
  **Copy branch name** (`remote/branch` for a remote branch), a remote's menu **Copy fetch URL**
  (and **Copy push URL** when it differs), and a tag's menu, in the Tags panel and on its badge
  in the graph, **Copy tag name**. They sit before the destructive items and stay enabled while
  an operation runs (#614).
- Changed: the Edit remote dialog's copy buttons confirm with a notification too, and a
  clipboard failure there or on the commit details' **Copy** shows an error instead of failing
  silently (#614).

### Repository tabs

- Added: "Rename tab…" in the tab menu and the command palette, and F2 on a focused tab, gives a
  tab a name of its own, for example to tell several clones or worktrees of one project apart.
  The name is kept per repository path, so it comes back after closing the tab and after a
  restart; an empty name shows the folder name again. The tab's tooltip still shows the full
  path, and the recent repositories and the workspace editor show the name too (#625).
- Added: a button at the end of the tab bar lists every open tab, grouped by workspace, with a
  filter field and the uncommitted-changes dot; Enter or a click switches to a tab. The command
  palette's "Switch to tab…" lists the other tabs by name, folder name or path (#625).
- Added: a tab whose repository has uncommitted changes (staged, unstaged or untracked) shows a
  dot after its name, also announced as "Uncommitted changes". The active tab follows its live
  status; the other tabs are checked when you leave them, when the window comes back into view
  and after a fetch (#613).
- Added: "Reopen closed tab" in the command palette and the tab menu, and Ctrl/Cmd+Shift+T where
  the host doesn't use the key. It brings back the most recently closed tab at its old place and
  in its workspace group; repeat it to go further back, up to ten tabs per session. A repository
  that can no longer be opened is skipped with a message (#613).

### Commit navigation

- Added: Back and Forward through the commits a tab selected: the ← and → buttons above the
  history, Alt+Left/Right (Cmd+[ / Cmd+] on macOS) where the host doesn't use them, the mouse's
  back and forward buttons, and "Back to previous commit" / "Forward to next commit" in the
  command palette. Moving through a list with the arrow keys counts as one step; a commit no
  longer shown is skipped (#618).
- Added: `[` and `]` in the commit list select the commit's first parent and its child (of
  several, the one on the same lane, else a list to choose from); Shift+`[` lists a merge
  commit's parents. A parent older than the loaded history is found as Go to does (#618).
- Added: the commit details list the commit's loaded children as links next to its parents
  (#618).

### Delete untracked

- Added: "Delete untracked…" next to "Discard all…" in the Changes heading deletes only the
  untracked files and keeps the changes to tracked files, conflicts and ignored files. Its
  confirmation names the count and lists the first five paths, and follows the same busy rule as
  "Discard all…"; the palette has it as "Delete untracked files…" (#622).
- Changed: "Discard all…"'s confirmation points to "Delete untracked…" when it would delete
  untracked files along with the other changes (#622).

### Internal: worker command policy table

- Changed: the worker command policy table test no longer compiles while a command has no row,
  as `policy::of` already doesn't without a policy; before, only a comment asked for the row
  (#518).

### Appearance preferences

- Added: an Appearance section in Preferences with a code font, a code font size (10 to 24 px),
  an interface size (Small, Normal, Large) and a tab size (1 to 16 columns) for diffs, blame,
  the file viewer and conflicts. Changes apply at once and are kept in the app's settings, in the
  desktop app and in the editor extension (#615).
- Changed: a tab in file content is now 4 columns wide instead of 8; set Tab size to 8 for the
  old look (#615).

### Links in commit messages

- Added: `http://` and `https://` URLs and issue references (`#123`, `owner/repo#123`) in the
  commit details' subject and body are links. A click or Enter opens them in the browser; issue
  references link to the forge of the remote the current branch tracks, else `origin`, else the
  first forge remote, and stay plain text without one (#612).
- Changed: opening an external link, in the desktop app and in the editor extension, refuses any
  URL that is not `http` or `https`, because a commit message is untrusted input (#612).

### File coming in over a folder

- Fixed: a revert, cherry-pick, merge, rebase step or stash apply that brings a file `c` back
  where your branch now has a folder `c/` (with a name such as `c.txt` sorting between them)
  stopped with "could not open 'c' for writing: Is a directory" and couldn't be done in the app.
  Like git, it now keeps the folder's files and moves the incoming file aside as the conflict
  `c~<label>` (`c~parent of <commit> (<subject>)` for a revert, `c~<commit> (<subject>)` for a
  cherry-pick or rebase step, `c~<branch>`, `c~Stashed changes`), which you resolve or abort.
  Without a name between them the file now gets git's name too (#607).

### Stage modified

- Added: "Stage modified" next to "Stage all" in the Changes heading stages the changes to tracked
  files (modified, deleted, type changes, renames) and leaves untracked files and conflicts
  alone. It shows when the group holds both tracked changes and untracked files, and is one
  undo step; the palette has it as "Stage modified files (tracked only)" (#621).

### Column widths

- Changed: the sidebar and the commit graph keep the width you set when the window is resized;
  only the diff pane grows or shrinks. When the window gets too narrow for both, the sidebar
  collapses instead of squeezing the graph, and it comes back at its width once there is room
  (#605).
- Fixed: expanding a collapsed sidebar by double-click or arrow key restored the default width
  instead of the width it had before (#605).

### Header row height

- Fixed: the top row with the app title, workspace groups and repository tabs was cut off at the
  bottom when a workspace group was open, because its height was fixed. The row now grows to fit
  the group's label above its tabs, at any window size, in the desktop app and in the editor
  extension (#606).

### Internal: file/folder conflict tests

- Internal: tests for the two stash paths the mutation job found untested in #600: a stash
  apply with staged changes refused before the file/folder path, and which checkout errors count
  as a file in the way (#416, #608).

### Internal: one owner per dialog

- Changed: the nine modal dialogs `AppState` kept in fields of their own (push, force push,
  delete remote branch, create tag, delete remote tag, a remote's edit / credentials / remove,
  merge, reword, stash) are one `dialog: WorkspaceDialog | null` union with one `openDialog` and
  one `closeDialog`, replacing 17 open/close actions; an action that finishes closes only its own
  dialog. The inline New branch and Add remote forms keep their own fields, since they can be open
  together and next to a dialog. `docs/ARCHITECTURE.md`'s "Who owns a dialog's open state" now
  says which dialogs live in `AppState`, in `RepoWorkspace` or in the component (#493).
- Changed: `BranchTree` and `DiffPane` no longer switch between local and lifted dialog state:
  `BranchTree` asks its owner to open a remote dialog (`onOpenRemoteDialog`) and no longer renders
  one, and `DiffPane` always takes `discardRequest` from its owner. Their tests hold the state in a
  small wrapper, as the workspace does (#493).

### Internal: workspace actions through a context

- Changed: `RepoWorkspace` builds the diff pane's and commit dock's actions once and provides
  them through `WorkspaceActionsContext` (`frontend/src/state/useWorkspaceActions.ts`).
  `DiffPane` drops from 54 props to 22 data props, and its views, file sections and the new
  `CommitDock` read the actions they need, so a new action is one member of `WorkspaceActions`
  plus its use. A required action can't be left out by mistake (a type error, where a `noop`
  default used to hide it), and a component rendered without the provider fails loudly. Tests
  build the actions with `frontend/src/test/workspaceActions.ts` (#491).
- Changed: `RepoWorkspace`'s JSX is split into `WorkspaceSidebar`, `WorkspaceMain` and
  `WorkspaceDialogs`; `RepoWorkspace.tsx` goes from 1,194 to about 610 lines (#491).
- Changed: `DiffPane.tsx` is split into one file per view (`UncommittedDiffPane`,
  `CommitDiffPane`, `StashDiffPane`, `UncommittedFileSection`), a pure move (#491).

### Internal: BranchTree split by job

- Changed: `BranchTree` (1,288 lines, 28 `useState`) is split by job, as pure moves: the tree
  rows, folders and graph swatches in `BranchTreeView`, the inline forms and the upstream dialog
  in `NewBranchForm`, `AddRemoteForm` and `SetUpstreamDialog` (each owns its fields), the lazy
  remote branch lists in `useRemoteBranchCache`, and the context menu items in
  `branchMenuItems.ts`, like `tagMenuItems.ts`. Its props and behaviour are unchanged; the tests
  are split to match (#492).

### Internal: one graph selection

- Changed: the commit graph's branch selection is one `GraphSelection` (`{ local, remote }`)
  passed as a single `selection` argument to `getCommitGraph`, `locateCommit` and
  `searchCommits`, and named `local` / `remote` everywhere (the saved-selection setters,
  `AppState.graphSelection`, `useCommitSearch`), so the two lists can no longer be swapped or
  `null` and `[]` mixed up without the types noticing (#498). The commit search now also re-runs
  when the remote selection changes.

### File/folder conflicts libgit2 misses

- Changed: a merge, squash merge, cherry-pick, revert, rebase step or stash apply (or pop) where a
  file and a folder of the same name meet and libgit2 sees no conflict (a name sorts between
  them, as `c`, `c.txt`, `c/g`) no longer refuses. Like git, it merges the folder's files and
  keeps your file as the conflict `c~HEAD` (`c~Updated upstream` for a stash), which you resolve
  or abort; a file coming in is moved to git's name for the other side (`c~<branch>`, with a `/`
  as `_`; `c~<commit> (<subject>)`; `c~Stashed changes`). A stash apply restoring the staged
  changes refuses, as `git stash apply --index` does (#416, #410).

## [0.8.0] - 2026-11-10

A privacy notice, a setting to turn off the automatic update check, recent searches in the
commit search field, and fixes: Cancel stops a transfer that is still connecting, and patches
and discarded hunks now handle symbolic links. Most of the rest is internal clean-up of the
worker loop, the transfer actions and the command palette.

### Privacy notice

- Added: a privacy notice in the user guide (`docs/guide/privacy.md`, linked from `PRIVACY.md`
  and both READMEs). Browsitory collects no data; the notice lists every connection the app
  makes, the data it keeps on your computer and how to delete it (#627).
- Changed: the privacy notice names who is responsible for it and gives a private contact,
  privacy@browsitory.com, instead of public GitHub issues. It gives the legal basis for the
  site's hosting logs, says which providers may process data in the US and on what terms, and
  describes how email to that address is kept and deleted (#627).
- Fixed: the privacy notice also lists the hosts an update download goes through
  (`api.github.com`, `release-assets.githubusercontent.com`), submodule addresses, the sign-in
  mode and HTTPS user name kept in each repository's `.git/config`, and that the log file holds
  notes as well as errors; its delete steps cover linked worktrees and that Git config (#627).
- Changed: the user guide site no longer loads its fonts from Google Fonts, and no longer shows
  the repository's star count and latest version in its header, so visiting it contacts neither
  Google nor `api.github.com`; the header keeps a plain link to the repository (#627).

### Update check setting

- Added: **Preferences → Updates → Check for updates automatically** in the desktop app
  (`[updates] check_automatically` in `config.toml`, on by default). Turned off, the app makes no
  update request at start or every 6 hours. **Check for updates**, in that section and in the
  command palette, checks once whatever the setting and says whether you are up to date (#630).

### Recent searches

- Added: the commit search field lists your last 10 searches in the repository when it's empty
  and focused; pick one with ↓/↑ and Enter or a click, remove one with Delete or its ×. Only
  searches that ran are remembered, not ones the field reports as invalid (#540).

### Cancelling a transfer that is still connecting

- Fixed: Cancel had no effect on a Fetch, Fetch all, Pull or Push that was still connecting to a
  server that doesn't answer (or a VPN that is down): the app kept waiting for the connection,
  for minutes or for good. Cancel now stops it at once, nothing is fetched or sent, and other
  actions can go ahead (#547).

### Symbolic links in patches and discarded hunks

- Fixed: applying a patch, or applying or reverting a commit's file or hunk, failed whenever it
  changed, deleted or renamed a symbolic link. The link itself is now changed, as `git apply`
  does, and a patch series that fails later puts such a link back as it was (#563).
- Fixed: discarding the hunk (or its lines) of a symbolic link that now points elsewhere works,
  and points the link back where it pointed; it used to fail every time and leave the link
  as it was. The file a link points to is never written (#586).

### Command palette groups

- Fixed: "Fetch all remotes", "Pull", "Push branch to…" and "Delete remote branch…" are listed
  under Remotes, and "Toggle wrap lines", "Toggle show whole file", "File history of current
  file", "Apply patch…" and "Save staged changes as patch…" under Diff, instead of General.
  Creating, checking out and deleting a tag are all listed under Branches; deleting a tag on a
  remote stays under Remotes. Command ids are unchanged, so recent commands keep working (#583,
  #592).

### Error messages

- Fixed: when an action such as Stage failed because file permissions blocked it, the error banner
  showed only the raw message. It now adds the same hint as the diff pane and other views (#573).

### Internal: worker loop split up

- Changed: `Worker::spawn_paused` no longer holds the whole worker thread. Its state lives in a
  `WorkerLoop` struct (`worker/worker_loop.rs`) whose `step` runs each command through `acquire`
  (the state lock), `before` (undo settle/begin, held reply, operation gate), `dispatch` (the
  `match` over `Command`, `worker_loop/dispatch.rs`) and `after` (undo finish, lock release).
  The `probe!`/`rebase_slot!` macros and the `marker` closure became methods. No behaviour
  change (#486).

### Internal: paused-operation stores

- Internal: the stores for a paused rebase and a stopped cherry-pick/revert are one generic
  `StateFile` over a codec, so loading, the "ignore a bad file, log once" rule and the atomic
  write live in one place (#487).

### Internal: remote transfer actions

- Changed: `useRemoteTransferActions` takes one context object (`client`, `repoPath`, `refresh`,
  `runners`, `setState`, `snapshot`) instead of 15 positional parameters, and reads the
  repository state it needs from one latest-snapshot ref instead of five mirror refs (#490).
- Changed: the transfer lifecycle (requested, active, completing, idle) moved into the React-free
  `lib/transferSession.ts`, unit-tested with progress event sequences; the failure messages are a
  lookup table keyed by error kind with per-operation overrides. No visible change (#490).

### Internal: command palette builders

- Changed: `buildCommands` takes one context object instead of 14 positional parameters and is
  split into one builder per command family. Each command declares its palette group, set by its
  builder, instead of the group being derived from the id; the ids stay, so recent commands still
  match (#494).
- Changed: the palette's sidebar commands ("Go to Tags", "Delete tag …", "Drop stash …",
  "Remove worktree …", "Browse branches on origin") send reveal requests through the workspace's
  state, which the sidebar sections and the branch tree handle, instead of finding and clicking
  the sidebar's buttons by their labels (#494).

### Internal: privacy host check

- Internal: CI's `audit` job fails when the code names a host the privacy notice doesn't list
  (`scripts/check-privacy-hosts.py`, #627). The script scans the whole text of each file, matches
  the scheme in any case, finds protocol-relative `//host` URLs, flags URLs whose host isn't
  written after the scheme (`format!`, `concat!`, template literals) unless they are listed,
  skips the files of `#[cfg(test)] mod name;` modules, and also scans `frontend/index.html`,
  `frontend/public/`, `frontend/vite*.ts`, `.json` sources, `extension/package.json` and the
  Tauri capabilities (#706).
- Internal: CI's licence-doc and privacy-host checks run even when `cargo audit` or
  `pnpm audit` fails (#707).

### Internal: tests and E2E

- Added: a VS Code E2E spec for "Fetch automatically": with the interval at one minute, the open
  panel fetches a local bare remote's new commit in the background and the sync bar shows
  "Fetched just now" (#546).
- Changed: the VS Code E2E host runs with its own empty `BROWSITORY_CONFIG_DIR`, like the Tauri
  suite, so it no longer reads or writes the developer's real Browsitory config (#546).
- Fixed: `e2e/screenshots/capture.sh` sets `TMPDIR=/tmp` only for the capture, so the build no
  longer fails when `/tmp` is a small tmpfs (#574).
- Fixed: E2E builds (`VITE_E2E_REPO_PATH`) skip the update check, so the E2E suite and the guide
  screenshots no longer contact the update endpoints, download a release bundle or show the
  "Update ready" banner (#576).
- Fixed: `first-flow.spec.ts` clicked Commit right after the stage click, so when the stage had
  not finished the commit never happened and the spec timed out waiting for it in history. It
  now waits for the file's Unstage control (the row is under Staged) and clicks Commit with
  `clickInPage`, which waits for the button to be enabled. `commit-graph.spec.ts` (both tests),
  `blame-viewer.spec.ts` and `commit-signing.spec.ts` had the same race and get the same fix
  (#697).
- Fixed: three `DiffPane` diff-options tests selected a line right after a hunk action, while
  the refetch that action starts could still land and clear the line selection, so "Stage lines
  (1)" was sometimes missing. They now wait for that refetch to land before selecting a line, and
  query the button with `findByRole` (#596).
- Fixed: the `CommitTreeBrowser` line history test selected a line and read the button's label
  at once, but the effect that subscribes the button to selection changes mounts together with
  the file's lines and could still be pending, so "History of line 2 of README.txt" was
  sometimes missing. The test now waits for the label with `findByRole` (#712).

## [0.7.0] - 2026-11-10

A release of fixes: actions now follow one rule for when the repository is busy (a merge,
rebase, cherry-pick or revert, another action, or a fetch, pull or push) everywhere they appear,
and say why they are disabled. Aborting a sequence, rolling back a patch series and updating a
submodule with files that differ only in case no longer lose or leave behind changes. Most of
the rest is internal clean-up of the frontend's state and IPC code.

### Same rules for branch, remote, stash and worktree actions in menus and the palette

- Fixed: a branch's **Checkout** (and double-click) and **Rename** were only disabled during a
  rebase run here, while the command palette left out **Switch to …** during any merge,
  cherry-pick, revert, terminal rebase, action or transfer; the menu now follows the same rule
  and says why (#509).
- Fixed: a remote's **Edit remote** and **Manage credentials** stayed enabled while the
  repository was busy, though the palette left them out, and **Remove remote** gave no reason;
  all three are now disabled with the reason (#509).
- Fixed: the command palette offered **Apply stash** and **Pop stash** with staged changes or
  unresolved conflicts, which the Stashes panel refuses; it now leaves them out (#509).
- Changed: the command palette now offers **Drop stash** while a merge, rebase, cherry-pick or
  revert is paused, like the Stashes panel (#521), **Show stash** whenever a stash exists, and
  **Add remote** while the repository is busy, like the **+** menu (its form waits) (#509).
- Added: the main worktree's disabled **Remove** says why (#509).
- Internal: branch, remote, stash and worktree action labels and rules live in
  `lib/actions/*Actions.ts`, shared by the menus, panels and the palette, with an agreement test;
  `CommitGraph`'s `pending` prop is now `operationsBlocked` (#509).

### Busy guards for the remaining confirmations

- Fixed: the force delete branch confirmation and the branch menu's **Delete** were only disabled
  during a rebase run here; they now follow the one busy rule (a merge, cherry-pick, revert,
  terminal rebase, another action or a transfer) and say why (#521).
- Fixed: the delete local tag confirmations and the rebase planner's **Start rebase** now show
  why they are disabled (#521).
- Fixed: the stash view's **Drop…** and the discard confirmations could run while another action
  or a fetch, pull or push was running; they are now disabled with the reason (#521).
- Changed: dropping a stash from the Stashes panel, and the command palette's discard commands,
  are now available while a merge, rebase, cherry-pick or revert is paused, like the stash view
  and the diff pane's discard; only another action or a transfer blocks them (#521).
- Fixed: **Clear upstream** and **Set upstream…** are disabled with the reason while the
  repository is busy, like **Set as upstream for current branch** (#521).

### Busy rule gaps

- Fixed: the New branch form's **Create** was enabled during a merge, a cherry-pick or revert, a
  rebase started in a terminal, another action or a transfer; only a rebase run here disabled it.
  It now follows the same rule as the other branch actions and says why (#561).
- Fixed: **Edit remote** and **Manage credentials** saved even when the repository became busy
  after they opened; they now disable saving and forgetting, and say why (#561).
- Fixed: the Remove remote and Remove worktree confirmations now say why they can't remove while
  the repository is busy (#561).
- Added: the command palette's **Open main worktree**, like the Worktrees panel's Open for the main
  worktree (#561).

### Aborting a cherry-pick or revert sequence

- Fixed: aborting a sequence whose steps left a pair of file names that differ only in case
  (with `core.ignorecase` on) unstaged changes staged on other files the rewind doesn't
  change. They now stay staged, as they do when no such pair is involved (#553).

### Files that differ only in case

- Fixed: with `core.ignorecase` on in a submodule (or set globally for a new submodule clone),
  updating the submodule to a commit whose files include two names that differ only in case
  (`A` and `a`) that its current commit doesn't kept only one of the two in the submodule's
  index and didn't write the other's file. The update is now refused before anything is
  written and names both files (#534).
- Fixed: with `core.ignorecase` on (Linux), aborting a cherry-pick or revert that had brought in
  a file whose name differs only in case from an existing one (`a` next to `A`) overwrote what
  you had written into the existing file during the pause. Abort now refuses, saying it would
  discard that change, and leaves the file as it is (#543).
- Internal: tests for the paths the mutation job found untested: Abort's refusal for a case
  pair, the rewind around a case pair, and the patch module (a staged patch without commits, a
  hunk ending without a final newline, a patch creating a file that is already there, a failed
  or re-adding series, a message line starting with "From ") (#543, #544).
- Internal: finding a commit no longer lower-cases an upper-case commit id itself; libgit2
  reads hex digits in either case (#545).

### Patch series rollback

- Fixed: when a later mail of a patch series doesn't apply, the folders an earlier mail created
  for a new file are removed too (deepest first, only when empty; a folder that was there before
  stays), so the working tree is left exactly as it was (#539).

### Error messages

- Fixed: when file permissions block reading a file's changes, the error now says to check that
  you can read and write the file or folder (#527).
- Fixed: saving or forgetting a forge token while the operating-system credential store is
  locked shows the same "unlock it and try again" advice as a remote's HTTPS token (#527).
- Internal: the worker decides each error's kind where the error is made: replies carry
  `ErrorDto`, typed errors get their kind from their type, every stopped-worker reply comes
  from one helper, and nothing reads a kind from the message any more (#527).

### Internal: one RepoClient for both transports

- Changed: the desktop and VSCode clients now share one implementation of every method they
  send alike (`createRepoClient`), and the transport parity test also compares the params each
  transport sends (#504).
- Fixed: on the desktop, a force push and the commands that run hooks (switch branch, checkout,
  merge, cherry-pick, revert, continue) now wait for the progress listeners like fetch, push and
  commit do, so a fast operation's first events are not lost (#504).

### Internal: backend limits and defaults generated for the frontend

- Changed: the frontend's diff context limits (`MAX_CONTEXT_LINES`, `WHOLE_FILE_CONTEXT_LINES`),
  the commit message column range, the default diff, commit message and fetch settings, and the
  commit search keys now come from `frontend/src/ipc/generated/wire-constants.ts`, which
  `wire_bindings_are_current` writes from the Rust constants and `Default` impls and checks for
  freshness. The `config` crate uses `git-core`'s `MAX_CONTEXT_LINES` instead of its own copy,
  the Preferences column fields take their range from the generated constant, and a test checks
  the search help's keys against the parser's. Behaviour is unchanged (#497).

### Internal: useAppState built from its action slices

- Changed: `UseAppStateResult` extends the action slice interfaces (`StagingActions`,
  `BranchActions`, `RemoteTransferActions` and the others) and declares only the members
  `useAppState` defines itself, and the hook spreads the slice objects into its result instead
  of naming each action again. Each action is now declared and documented once, in its slice;
  where the two comments had drifted, the more accurate one was kept. A compile-time check fails
  when two slices share a member name, which the spread would otherwise resolve silently.
  `useAppState.ts` shrinks from 949 to 594 lines. Behaviour is unchanged (#489).

### Internal: one shared fake RepoClient for frontend tests

- Changed: frontend tests build their `RepoClient` fakes with one `Proxy`-based helper,
  `frontend/src/test/fakeRepoClient.ts`, instead of five complete hand-written copies, 21
  inline ones in the `useAppState` tests and about 20 partial casts. A method a test doesn't
  override fails with `not used in this test: <method>`, or answers an empty value in `"empty"`
  mode, so a new `RepoClient` method needs no edit in the tests (#499).

### Internal: e2e test stability

- Fixed: the commit graph test for a commit on a detached HEAD wrote its file after the page had
  reloaded, so the app could read the status before the file existed and never show it; the
  test now reloads after the change on disk (#556).

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
