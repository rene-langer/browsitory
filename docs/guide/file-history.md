# File history

Click **History** in a file's header — in the working tree (any tracked file) or in a commit's
file list — or run **File history of current file** from the command palette. In VSCode,
**Show File History in Browsitory** and **Show Line History in Browsitory** in the Explorer's
and the editor's menus open it for a file or the selected lines (see
[Install](install.md#browsitory-in-the-vscode-menus)). The commit list
is replaced by the commits that changed that file, newest first, starting from the checked-out
commit (HEAD). The graph's branch selection (see [Which branches the graph shows](history.md#which-branches-the-graph-shows)) does not apply here.

Renames are followed: commits before a rename list the file under its old name, and the commit
that renamed it is marked "renamed from …". Its diff shows the real edits, not a whole-file add.
A merge is listed only when its version of the file differs from every parent.

Selecting a commit shows only that file's diff; **Show all N files** shows the whole commit.
Use ↑/↓ (or j/k) to move, **Load more** for older changes, and **×** or Esc to return to the
full history with your previous selection.

## History of lines

To see only the commits that changed some lines, select them with the mouse in a commit's diff
(or in its **Blame** view, in a comparison of two commits, or in a file under
[All files](files-at-a-commit.md)), then click **History of lines N–M** in the file's header;
the button reads **Line history** and is disabled while nothing is selected. Lines of the newer side count: in a diff, removed lines are skipped.

The list shows the commits whose changes touched those lines, newest first, starting at the
commit you selected them in. Like `git log -L`, the lines are followed back through earlier
changes: when lines are inserted above them, they are found at their old position, and a commit
that rewrote them is followed into its version of them. Each row shows where the lines were in
that commit ("lines 7–9"). The list ends at the commit that added the lines.

- Renames are followed, as in file history. Copies from another file are not.
- At a merge whose lines match one parent's, only that parent's history is followed and the
  merge isn't listed. A merge that changed the lines against every parent is listed, and both
  sides are followed.
- The lines are followed as one block: a change in the middle of the block keeps the whole block
  in older commits, where `git log -L` can split it in two.
- Where a diff is ambiguous (a repeated line), the edge of the block can land one line off from
  what `git log -L` shows.
- On a very long history the search stops after 50,000 commits that have the file and says that
  older changes may be missing.

The same search is available in the search field as `line:PATH:FROM-TO` (see
[Searching commits](search.md)), where it combines with other terms.

## Folder history

**Folder history** in a file's header (in the working tree, a commit or a comparison) lists
the commits that changed anything in that file's folder; under
[All files](files-at-a-commit.md) each folder's row has one too. It fills the search field with a
`path:` search for the folder (for example `path:src/ui/`), so it covers the commits the graph
shows, and you can narrow it further there. Esc or × goes back to the graph.

