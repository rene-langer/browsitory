# History and diffs

Click a commit in the graph to see the files it changed and their diffs. **Blame** on a file
shows who last changed each line. **All files** above the list browses every file of the commit
instead (see [Files at a commit](files-at-a-commit.md)). Each row shows the branches and tags that point at the commit,
and the author and date (they hide when the pane is narrow). Selecting a commit shows its full
message, author, date, full SHA (**Copy** copies it), parents and children (the loaded commits
it is a parent of) above the file list; click a parent or child to jump to it. The date is the
author's, as `2026-09-08 21:00` in your time zone (the graph's dates use the same form) with
"2 days ago" after it. When someone else committed it (a patch applied, a cherry-pick),
**Committer** shows them and when they committed; when the author committed it later (a rebase,
an amend), **Committed** shows that time. **Contained in** lists the local branches, remote
branches (dashed) and tags (`#`) whose tip is the commit or comes after it, the first eight and
the rest under **Show N more**, or "No branch or tag". In a repository with very many branches
and tags the check stops after a moment and says "Not every branch and tag was checked." Web links (`http://` and `https://`) in the message open in your browser
when you click them or press Enter on them; the link's tooltip shows where it goes. When the
repository has a supported forge remote, issue references such as `#123` and `owner/repo#123`
are links to that issue too (on the forge of the remote the current branch tracks, else
`origin`, else the first forge remote); without one they stay plain text. The message stays
plain text to select and copy. The graph loads the latest 300 commits; choose **Load more** at the bottom
(or keep pressing ↓ past the last row) to load the next page. Use ↑/↓ or j/k to move through the
commits, and PageUp/PageDown to move a page at a time. To jump to a commit, branch, tag or stash,
use **Go to commit…** and the other Go to commands in the [command
palette](command-palette.md#go-to-a-commit-branch-tag-or-stash), or click a branch or tag in the
sidebar. To revert or apply only one file or hunk of a commit, or to save commits as a patch,
see [Patches and partial reverts](patches.md).

**Copying a commit's details.** A commit's menu (right-click its row, or the Menu key or
Shift+F10 on it) ends with **Copy SHA**, **Copy short SHA**, **Copy subject**, **Copy author**
(`Name <email>`) and **Copy date** (ISO 8601, for example `2026-11-10T09:30:00.000Z`, the same
instant the details show). They copy plain text, work while another operation runs, and a
short notification confirms the copy; if the clipboard refuses, an error says so. Branch, remote
and tag menus have copy items too (see [Branches and remotes](branches-and-remotes.md) and
[Tags](tags.md)).

**Lines changed.** Each commit row shows how big the commit is, for example `+12 −3`: lines
added in green, lines removed in red (screen readers read "12 lines added, 3 removed"). The
numbers are counted as rows scroll into view, a few dozen at a time, so a row first appears
without them; a very large commit that would take too long to count gets none. The selected
commit's details show the totals ("4 files changed, +120 −37") under **Size**, and each file's
header its own `+/−` (or **Binary**: a binary file counts as a changed file, with no lines).
A commit is counted against its first parent (a merge too, like its file list; the first commit
against nothing), and a renamed file counts as removed and added, as the file list shows it.
The ± button in the search bar above the graph hides the row numbers or shows them again; the
choice applies to every tab and is kept after a restart.

The bar above the graph has **Fetch**, **Pull** and **Push** buttons for the
current branch's upstream, with ahead/behind counts (for example `↑2 ↓1`)
based on the last fetch.

## Parent, child, back and forward

With the commit list focused, `[` selects the commit's first parent and `]` its child. When the
parent is older than the loaded history, the graph loads up to it, as **Go to commit…** does.
On a merge commit, Shift+`[` lists all its parents to choose from. When a commit has several
loaded children, `]` takes the one drawn on the same lane, or lists them when none is.

Each tab remembers the commits you select (by clicking, with the keys, a parent or child link,
Go to, a search result or a file's history), so you can go back to where you were. Moving
through the list with the arrow keys counts as one step. Go back and forward with the **←** and
**→** buttons left of the undo buttons, Alt+← and Alt+→ (⌘[ and ⌘] on macOS), your
mouse's back and forward buttons, or **Back to previous commit** and **Forward to next commit**
in the command palette. A commit that is no longer in the list, for example after a rebase
rewrote it or when its branch is now hidden, is skipped. The keys do nothing in a text field,
and where the host uses them for its own navigation (in VSCode on Windows, Alt+Left is its own
Go Back), use the buttons or the palette. The list is kept when you switch
to another tab and back, like the selection, unless the tab was unloaded meanwhile (see
[Multiple repositories](tabs.md)).

![A commit from history with its diff](assets/commit-diff.png)

## Which branches the graph shows

At first the graph shows the commits of every local branch. In the Branches section, each branch
has a coloured dot in front of its name. Click the dot (**Show … in graph**) to add the branch to
the graph or take it out. This works for remote branches too: a remote branch you turn on adds
its commits to the graph, even ones no local branch has (a colleague's fetched branch, for
example). Until you pick remote branches, the graph shows no remote-only commits, and the badge
of every remote branch whose commit is already in the graph. A branch's menu, local or remote,
has **Isolate branch**, which shows only that branch. **Show all branches** in the **+** menu of
the Branches section goes back to the graph you started with: every local branch, also ones you
create later, no remote-only commits, and every remote branch's badge. Turning dots back on until
every local branch is on does the same for the local branches; the remote branches you picked
stay. Browsitory remembers the choice per repository.

When the graph shows a single branch, local or remote (after **Isolate branch**, for example),
it shows only that branch's own line: the commits made directly on it and its merge commits, newest
first. The commits of the branches merged into it are left out, so the list answers "what landed
on this branch, and when?". A merge commit still lists all its parents in the commit details; in
the graph it has a short dashed stub ("Merged from a branch not shown in this view") in place of
the merged branch. With two or more branches, or all of them, the graph shows every commit they
reach, merged branches included.

## Collapse merges

In a repository that merges feature branches, the graph can show just the line of each branch:
turn on **Collapse merges** with the fold button in the search bar above the graph, or the
**Collapse merges** command in the command palette. The graph then follows only the first
parents of every branch it shows, as for a single branch above, whichever branches you picked.
Browsitory remembers the choice per repository; turning it off (the same button, or **Show
every commit of merged branches** in the palette) brings back every commit.

Each merge commit in such a graph (also with a single isolated branch) has a **+N** button after
its subject: N is how many commits the merge brought in, the commits of the merged branch (and
of any branch merged into that one) that the branch had not had before. Click it to show those
commits right below the merge, with their lanes; **−N** hides them again. With the commit list
focused, → expands the selected merge and ← collapses it; on one of its commits, ← goes back to
the merge. A merge of a long-lived branch shows its first hundred commits; **More** shows the next
hundred. The commits shown this way work like any other row: select them, select several, open
their menu, compare them, see their size.

Going to a commit that a collapsed merge holds, with **Go to commit…** or the parent key `[`
(a merge's second parent, chosen with Shift+`[`), expands that merge and selects the commit.
Commit search still searches every commit of the branches the graph shows, so it finds commits
inside collapsed merges too. Back and Forward skip a commit whose merge you collapsed again.

If a merged branch still exists and the graph shows it, its commits are already rows of their
own, so expanding its merge adds no rows for them.

When HEAD is detached (you checked out a commit or a tag), the graph also shows the commit you are
on and the commits before it, whichever branches you picked, and marks that commit with a **HEAD**
badge. Commits you make there are on no branch until you create one (**Branch from here**), but
they stay in the graph and in commit search while HEAD is on them.

## Image previews

A changed PNG, JPEG, GIF, WebP or SVG file shows the old and new image side by side (**Before**
and **After**) instead of a text diff, with each image's size in pixels and file size, on a
checkerboard so transparency shows. A new file shows **Added** on the left and a deleted one
**Deleted** on the right. This works in the working tree, commits, stashes and compares. Each
side is previewed up to 20 MB; a bigger one says "Too large to preview". The image type is
checked from the file's content, not its name, and a file that isn't really one of these images
says "Can't display this image". Images are only shown as pictures, so an SVG can never run a
script. An SVG is text too: **Show text diff** switches to its diff (and back with **Show
image**). A conflicted image opens the conflict pane, like any conflict.

## Diff options

Each pane has one diff toolbar, above its files: the working tree, a commit, a stash and a
compare. Its buttons are icons; hover one for its name. Everything on it applies to every diff in
the pane and is remembered across sessions. **Unified view** shows old and new lines in one
column; **Split view** shows the old and new version side by side. **Ignore whitespace**,
**Context lines** (− and +, 0 to 20) and **Syntax highlighting** are kept in your settings. While whitespace is ignored, partial staging (lines and hunks) is unavailable in
the working tree and a hint says so; staging and unstaging whole files still works. Right after
you change an option, hunk and line actions return once the diff has reloaded with it.
Highlighting covers common languages and is skipped when a file's diff has more than 2,000 lines
in total across its hunks. The command palette has an entry for each of these three.

**Show whitespace** makes whitespace visible: a faint `·` for each space, a faint arrow across
each tab (as wide as the tab size in **Preferences → Appearance**), and a faint `↵` for a
carriage return, such as the end of a line with Windows line endings. On added lines, trailing
spaces and tabs are highlighted. The markers are only drawn over the text: copying from the diff
copies the original characters, lines wrap as before, and staging is unaffected. It is off by
default, remembered across sessions, and in the command palette as **Toggle whitespace**.

**Wrap lines** wraps long lines at the pane's width, or turns wrapping off so each line stays on
one row and the diff scrolls sideways. Line numbers stay next to their line either way, and
selecting and staging lines works the same. The button sets **Preferences → Diff → Word wrap**
to **On** or **Off** for every diff; the setting also has **Auto** (the default, shown as a
half-pressed button). Auto wraps the split view, prose files (Markdown, plain text, reStructuredText,
AsciiDoc and the like) and any diff with a line longer than 300 characters, such as minified or
generated files; code in the unified view scrolls sideways so its indentation stays readable.

**Show whole file** shows every line of each file, with the changes highlighted where they are,
in the unified and the split view; press it again for the changes with context. Like **Split
view**, it is remembered on this computer. It works in the
working tree, commits, stashes and compares. In the working tree the whole file is one hunk:
**Stage hunk** then stages every change in the file, and selected lines stage just those lines.
A file longer than 10,000 lines is too long to show whole: a note says so and the diff keeps the
normal context.

**Large and generated diffs** start collapsed, so opening one never stalls the pane: a diff with
more than 1,000 lines (context included), or of a file that tools write, shows **Load diff (N
lines)** instead. Generated files are lockfiles (such as `Cargo.lock`, `package-lock.json`,
`pnpm-lock.yaml`, `yarn.lock`, `go.sum` or `poetry.lock`), minified code (`.min.js`,
`.min.css`) and source maps (`.map`). A loaded diff stays loaded for that file, also after you
stage part of it. A loaded diff shows its first 2,000 lines; **Show 2,000 more** and **Show all**
below it show the rest. `]` and `↓` move past the last shown line and show more on their own.

With a screen reader, each line of a diff is read with what it is and its line number, such as
"Added line 12", "Removed line 7" or "Unchanged line 3", before its text; the `+`/`−` markers
and the line-number columns are not read. Where lines can be selected, each hunk's lines are a
list: the line cursor is read as you move it, with whether the line is selected.

The command palette also has **Toggle wrap lines** and **Toggle show whole file**, which change
the same options. **Toggle wrap lines** turns wrapping on or off depending on what the current
diff shows: the one you last clicked or moved the focus into, or the working tree's current file.

**Preferences → Diff → Diff algorithm** chooses how the old and new lines are lined up, for every
diff: **Default** (Myers, what `git diff` uses), **Patience** or **Minimal**. Patience lines up
the lines that occur once on each side first, which often keeps a moved or reordered block of
code readable where Default interleaves it with unrelated braces and blank lines. Minimal
searches longer for the smallest diff. The choice also changes where hunks start and end, so
hunk and line actions always work on the hunks as you see them: staging, unstaging, discarding,
reverting or applying a hunk or lines uses the algorithm the diff was shown with. git's
`histogram` algorithm isn't offered.
