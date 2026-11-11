# Staging and committing

Select **Uncommitted changes** at the top of the commit graph to see your working tree in the
right-hand pane. At the top is a compact list of the changed files in two groups, **Changes** and
**Staged**, each with **Stage all** or **Unstage all**. Both groups are always on screen: each
scrolls on its own when it holds many files, and an empty one says so. Each row shows a status
letter (**M** modified, **A** added, **U** untracked, **D** deleted, **R** renamed, **T** type
changed, **C** conflicted; hover it for the word), the path, a **⋯** menu and a stage or unstage
button. The two buttons are always there, dimmed until you point at the row or select it.

When nothing has changed, the pane says **Working tree clean** instead. In a new repository
without commits it says **No commits yet**: add files to the repository folder, stage them and
commit to make the first commit.

Below the list, one diff viewer shows the current file: the first one when you open the view, or
the one you click. Its header names the file and has **Blame** (who last changed each line) and
**History**. When the current file leaves its group, for example because you staged it, the file
now in its place becomes current.

Type a commit message and click **Commit**. The button says what it commits and where, for
example **Commit 3 files to main** (without the branch on a detached HEAD). Without a message it
is disabled, and "Write a commit message to commit." says why. After a commit a notification
names it, "Committed 4c5beaf to main", with **Push** to push the branch (to its upstream, or
through the Push dialog when it has none) and **Undo**; without a remote it has **Undo** only.
Push only pushes the branch the notification names: if you have checked out another branch since,
it says "Not pushed: main is no longer the current branch." instead. Undo only acts while that
commit is still the latest. The arrow next to the button has **Commit and
push**, which commits and then pushes the same way (it needs a branch and a remote; otherwise its
tooltip says why), and **Amend last commit**, which turns on amend mode (see [Rewriting
commits](rewriting-commits.md)). **Stash** in the repository toolbar sets the current changes
aside; see [Stashes](stashes.md).

When **Changes** holds both changes to tracked files and untracked (new) files, **Stage
modified** sits next to **Stage all**: it stages the tracked files' changes (edits, deletions,
type changes) and leaves the untracked files where they are, like `git add -u`. **Stage all**
stages the untracked files too. Both leave conflicted files alone, and each is one undo step. The
command palette has the same actions as **Stage all changes** and **Stage modified files
(tracked only)**; **Ctrl+Shift+A** (**⇧⌘A** on macOS) stages all changes from anywhere outside a
text field.

Only the current file's diff is loaded. To keep large new or generated files from loading when
you move onto them, turn off **Preferences → Diff → Expand untracked files**: an untracked file
then shows **Show contents** in the viewer and loads only when you press it (it stays shown when
the list refreshes); changes to tracked files always load at once.

The options above the viewer (**Unified view** / **Split view**, **Wrap lines**, **Show whole
file**, **Ignore whitespace**, **Show whitespace**, **Syntax highlighting** and the context
lines) apply to every file and are remembered; see [Diff options](history.md#diff-options).

Each hunk header has the buttons **Stage hunk** (or **Unstage hunk**) and **Discard hunk**, which
is red; their keys are `s` and `d`. Discard doesn't ask: the hunk goes at once, and the
notification that says so has an **Undo** button (Ctrl+Z, ⌘Z on macOS, works too).

A rename you staged (`git mv`, or staging both the deletion and the new file) is one **Staged**
row under the new name with "Renamed from …": its diff shows only the edits, an image previews
the old version against the new one, and unstaging it unstages both names. A file moved on disk
but not staged yet shows as deleted plus new, like `git status`.

![Staging a change, with the diff and commit message visible](assets/staging.png)

## Selecting several files

Click a file's row to select it. **Ctrl+click** (**Cmd+click** on macOS) adds a file to the
selection or removes it, and **Shift+click** selects every file from the last one clicked. With
the **Changes** or **Staged** list focused, `↑`/`↓` (or `j`/`k`) move to the next file and on into
the other list past either end, `Shift+↑`/`Shift+↓` extend the selection, **Ctrl+A** (**⌘A**) selects
the whole list, **Esc** keeps only the current file and **Enter** moves the focus into its diff.
With the focus in the diff, `j` and `k` go to the next and previous file and keep the focus
there. A selection stays within one list: clicking a file in the other list starts a new one.
The current file, marked by a bar on its left, is the one the viewer shows; screen readers
announce how many files are selected.

With two or more files selected, a bar above the list shows how many and offers **Stage N
files** (or **Unstage N files**), **Discard N files…**, **Ignore N untracked files** (when the
selection has untracked files) and **Stash N files…**; `s`, a selected file's **⋯** menu and
the command palette offer the same actions. **Discard N files…** asks once for all of them and
is one step for **Undo**. **Ignore** adds each untracked file to `.gitignore` as **Ignore file**
does. Staging, unstaging or ignoring a selection (like **Stage all** and **Unstage all**) is one
step for **Undo** too ("Stage 3 files", "Ignore 2 patterns"), and the notification after
**Ignore** has an **Undo** button. If one of the files can't be staged or unstaged, none of them
is, and the error names why. Conflicted files in
a selection are left out of these actions, as **Stage all** leaves them out. While another
action is running, the bar's buttons are disabled.

## Writing the commit message

**Alt+C** (**⌥C** on macOS), or **Go to commit message** in the command palette, shows the
working tree and puts the cursor in the message from anywhere outside a text field.

Below the message, **Summary 42/50** counts the characters of the first line. Past the limit it
turns to a warning color, because many tools cut longer summaries; it never stops a commit. A
faint dotted ruler marks column 72 for wrapping the body, and the field uses a monospace font while the
ruler is shown so the column lines up. Change the limit, the ruler column (both 10 to 200) or
turn the ruler off under **Preferences → Commit message**.

**Add co-author** lists the authors of the last 1,000 commits on the current branch (newest
first, without you); type to filter, or type a new `Name <email>` and press Enter. Each co-author
shows as a chip (× removes it) and is added to the message as a `Co-authored-by: Name <email>`
trailer when you commit, after any trailers the message already ends with and never twice.
The chips stay until the commit lands, so a failed hook keeps them, and **Commit without hooks**
uses them too. They are hidden while a merge, cherry-pick or revert pre-fills the message.

**Recent messages** (next to the counter) opens a menu of earlier messages: first the last 20
you committed or amended in Browsitory in this repository (newest first; kept per repository on
this computer, not in the repository), then the 10 newest commits on the current branch, each
summary once. Use the arrow keys and Enter, or click, to put that full message in the box; it
replaces what was there. **Ctrl+Z** (**⌘Z** on macOS) in the field, or **Restore message**,
brings back the replaced text until you edit the field. A merge, cherry-pick, revert or squash
message that git wrote and you committed unchanged is not kept.

A commit that fails, for example because a hook rejected it, keeps its message in the box, so
you can fix it and commit again without retyping it.

**Your name and email.** Git records a name and an email address with every commit
(`user.name` and `user.email`). When none is set, committing (or amending) opens **Set your git
identity** instead of failing with git's error: enter both, choose **All repositories** (your
global git config, as `git config --global` does) or **This repository only**, and **Save and
commit** commits what you were committing. After an amend, the dialog only saves; amend again.

## Staging individual lines

In a diff, select lines by clicking one, Shift+clicking to extend, Ctrl/Cmd+clicking to add
or remove one, or dragging across several. A selected line has a bar on its right edge (the
line cursor's bar is on the left) and keeps its added or removed colour. Then use **Stage lines**, **Unstage lines** or
**Discard lines** in the hunk header. With the keyboard, `↑`/`↓` move a line cursor inside the
active hunk and select that line, `Shift+↑`/`Shift+↓` extend the selection, and `[` / `]` jump
to the previous or next hunk (which clears the selection). `s` stages or unstages and `d`
discards the selection, or the whole hunk when nothing is selected.
A screen reader reads the line under the cursor, whether it is added or removed, its number and
whether it is selected (see [Diff options](history.md#diff-options)).

If the file changed since its diff was shown (for example you edited it in another editor),
line actions don't run; the diff refreshes so you can select again. Discarding a whole hunk of
an untracked file empties the file rather than deleting it. Staging, unstaging or discarding a
hunk or lines keeps the file's mode (such as the executable bit), as `git add -p` does, and
doesn't stage or undo a mode change; stage or discard the whole file for that.

## Discarding and ignoring files

Open a file's **⋯** menu, right-click the file row, or press Shift+F10 (or the Menu key)
with the file row focused, for **Discard changes…**. Staged files don't offer discard; unstage them first. A confirmation dialog
appears. A discard can be reverted with **Undo** (see [Undo and redo](undo-redo.md)) unless the
file is changed again afterwards. You can discard while a merge, rebase, cherry-pick or revert
is paused, to resolve it; while another action or a fetch, pull or push is running, the
confirmation's button is disabled and the dialog says why. **Discard all…** next to **Stage all** discards
the unstaged changes of exactly the files its confirmation lists. It deletes untracked files
and skips conflicted ones. Folders, such as an untracked nested repository, are never deleted;
any file that couldn't be discarded is named in an error after the rest are done.

When **Changes** has untracked (new) files, **Delete untracked…** sits next to **Discard all…**:
it deletes only the untracked files and keeps every change to tracked files, conflicts and
ignored files. Its confirmation names how many files go and lists the first five; like **Discard
all…**, its button is disabled while another action or a fetch, pull or push is running. Folders
such as a nested repository are left alone here too. The command palette has it as **Delete untracked files…**, and **Discard
all…**'s confirmation points to it when untracked files would be deleted along with the rest.

Discarding
never writes or deletes outside the repository, including through a symlinked folder. If a
tracked file was replaced by a symbolic link, discarding removes the link and restores the file
(the link's target is never touched), like git; reset, undo/redo, aborting a merge or rebase and
resolving a conflict do the same. If a tracked folder was replaced by a file of the same name,
discarding a file that was in the folder, or a hunk or lines of it, refuses and names the file in
the way ("Couldn't discard 'd/f': 'd' is now a file, not a folder. Discard or move 'd' first."),
also when the folder was further up; **Discard all** with both
the file and the folder's files deletes the file first and then restores the folder. If a tracked
file was replaced by a folder, discarding the file deletes the untracked files in the folder, like
git, and Undo brings them back; if the folder holds anything Undo can't bring back (an ignored
file or a nested repository), the discard refuses and names it ("Couldn't discard 'd': it is now a
folder holding 'd/x.log', which Undo can't bring back. Move it first."), and **Discard all**
leaves the folder's untracked files alone too.

For untracked files the menu also offers ignore items. Each appends a pattern to the
repository's `.gitignore` (creating it if needed; a `.gitignore` that is a symlink is left
alone and reported). **Ignore file** is always offered.
**Ignore all \*.ext** appears only when the file name has an extension. **Ignore folder
⟨folder⟩/** appears only when the file is in a subfolder. If `.gitignore` is read-only, the
ignore items change nothing and say "'.gitignore' is read-only. Make it writable, then try
again." `.gitignore` is replaced whole, so a failed write leaves it as it was. On Windows, if
another program keeps `.gitignore` open so it can't be replaced, the ignore items say
"'.gitignore' is open in another program. Close it, then try again." and change nothing.
