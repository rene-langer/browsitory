# History and diffs

Click a commit in the graph to see the files it changed and their diffs. **Blame** on a file
shows who last changed each line. Each row shows the branches and tags that point at the commit,
and the author and date (they hide when the pane is narrow). Selecting a commit shows its full
message, author, date, full SHA (**Copy** copies it) and parents above the file list; click a
parent to jump to it. The graph loads the latest 300 commits; choose **Load more** at the bottom
(or keep pressing ↓ past the last row) to load the next page. Use ↑/↓ or j/k to move through the
commits, and PageUp/PageDown to move a page at a time. To jump to a commit, branch, tag or stash,
use **Go to commit…** and the other Go to commands in the [command
palette](command-palette.md#go-to-a-commit-branch-tag-or-stash), or click a branch or tag in the
sidebar.

The bar above the graph has **Fetch**, **Pull** and **Push** buttons for the
current branch's upstream, with ahead/behind counts (for example `↑2 ↓1`)
based on the last fetch.

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
