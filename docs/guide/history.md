# History and diffs

Click a commit in the graph to see the files it changed and their diffs. **Blame** on a file
shows who last changed each line. **All files** above the list browses every file of the commit
instead (see [Files at a commit](files-at-a-commit.md)). Each row shows the branches and tags that point at the commit,
and the author and date (they hide when the pane is narrow). Selecting a commit shows its full
message, author, date, full SHA (**Copy** copies it), parents and children (the loaded commits
it is a parent of) above the file list; click a parent or child to jump to it. Web links (`http://` and `https://`) in the message open in your browser
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
**→** buttons left of the undo buttons, Alt+Left and Alt+Right (Cmd+[ and Cmd+] on macOS), your
mouse's back and forward buttons, or **Back to previous commit** and **Forward to next commit**
in the command palette. A commit that is no longer in the list, for example after a rebase
rewrote it or when its branch is now hidden, is skipped. The keys do nothing in a text field,
and where the host uses them for its own navigation (in VSCode on Windows, Alt+Left is its own
Go Back), use the buttons or the palette. The list starts afresh when you
switch to another tab and back, like the selection.

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

The diff toolbar above each file has these controls. **Split view** shows the old and new
version side by side; **Unified view** goes back to one column. **Ignore whitespace**, **Context
lines** (− and +, 0 to 20) and **Syntax highlighting** are remembered across sessions and apply
to every diff. While whitespace is ignored, partial staging (lines and hunks) is unavailable in
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
selecting and staging lines works the same. The button changes only that diff, until you leave
it. The default comes from **Preferences → Diff → Word wrap**: **On**, **Off** or **Auto**
(the default). Auto wraps the split view, prose files (Markdown, plain text, reStructuredText,
AsciiDoc and the like) and any diff with a line longer than 300 characters, such as minified or
generated files; code in the unified view scrolls sideways so its indentation stays readable.

**Show whole file** shows every line of that file, with the changes highlighted where they are,
in the unified and the split view; press it again for the changes with context. It works in the
working tree, commits, stashes and compares. In the working tree the whole file is one hunk:
**Stage hunk** then stages every change in the file, and selected lines stage just those lines.
A file longer than 10,000 lines is too long to show whole: a note says so and the diff keeps the
normal context.

The command palette also has **Toggle wrap lines** and **Toggle show whole file**. They act on
the current diff: the one you last clicked or moved the focus into, or the working tree's current
file. Without either, they act on the first diff on screen.

**Preferences → Diff → Diff algorithm** chooses how the old and new lines are lined up, for every
diff: **Default** (Myers, what `git diff` uses), **Patience** or **Minimal**. Patience lines up
the lines that occur once on each side first, which often keeps a moved or reordered block of
code readable where Default interleaves it with unrelated braces and blank lines. Minimal
searches longer for the smallest diff. The choice also changes where hunks start and end, so
hunk and line actions always work on the hunks as you see them: staging, unstaging, discarding,
reverting or applying a hunk or lines uses the algorithm the diff was shown with. git's
`histogram` algorithm isn't offered.
