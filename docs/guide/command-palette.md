# Command palette and shortcuts

## Command palette

Press **Ctrl+K** (**Cmd+K** on macOS) anywhere to open the command palette, or click
**Commands** in the header. Before you type, it lists every command it can run now: the ones you
used last under **Recent**, then the rest in their groups. Type to filter the list. The words you
type can come in any order and can be shortened: `sw main` finds **Switch to main**, and `ftch`
finds **Fetch origin**. The best match comes first, and the letters that matched are marked.

Use ↑/↓ to pick a command and Enter to run it, or click one. PageUp/PageDown move a page at a
time, and Ctrl+Home/Ctrl+End (or Home/End before you type) go to the first or last command.
Pointing at a command with the mouse picks it too.

![The command palette, filtered to "branch"](assets/command-palette.png)

The palette has almost every action in Browsitory, grouped like this:

- **Branches:** switch to a branch, **Switch branch…** (the list the toolbar's branch button
  opens, see [The main window](main-window.md)), **Create branch…** (the New branch form),
  merge a branch into the current one, and create, check out or delete a tag.
- **Remotes:** fetch one or all remotes, pull, **Pull with merge**, **Pull with rebase** and
  **Pull, fast-forward only** (each also remembered for the repository), push, **Push branch to…**, **Force push current
  branch…**, push all tags, **Browse branches on ⟨remote⟩**, add, edit or remove a remote,
  manage its credentials, delete a remote branch, and delete a tag on a remote.
- **Stash:** stash all, **Stash…**, stash the current file or the selected files, and show,
  apply, pop or drop a stash.
- **Diff:** ignore whitespace, show whitespace, more or fewer context lines, syntax
  highlighting, wrap lines and show whole file in the current diff (see [Diff options](history.md#diff-options)), file
  history of the current file, stage, unstage or discard the selected files, discard the current
  file or all unstaged changes, open the current file in your editor or diff tool, **Apply
  patch…** and **Save staged changes as patch…** (see [Patches and partial
  reverts](patches.md)).
- **Go to:** **Go to commit…**, **Go to branch…**, **Go to tag…** and **Go to stash…** (see
  below), open a sidebar section and move to it, for example **Go to Tags**, and **Show
  sidebar** or **Hide sidebar**.
- **Repositories:** switch to another tab (one command per tab, or **Switch to tab…** to pick
  one by name, folder name or path), **Rename tab…**, close the tab, open a terminal, reveal the
  repository in the file manager (see [Multiple repositories](tabs.md)).
- **General:** **Refresh**, undo and redo, search commits, the selected commit's actions
  (cherry-pick, revert, reset, check out, reword, squash, fixup, move, drop; for a search
  result or file history commit older than the loaded history, all but the history edits),
  worktree and submodule actions, **Preferences…**, **Show keyboard shortcuts**, **Check for
  updates** (desktop app), **Third-party licences** and **Open user guide**.

Commands that can't run right now are left out. For example, while a rebase is paused, the
palette offers only the commands that are safe until it is finished.

## Go to a commit, branch, tag or stash

**Go to commit…**, **Go to branch…**, **Go to tag…** and **Go to stash…** select something and
scroll the graph to it without checking anything out: the working tree stays as it is. Choose
one, then type to filter its list and press Enter (Backspace in the empty field goes back to the
commands).

- **Go to commit…** lists the loaded commits; type the start of a SHA or words from the commit
  message. **Look up "…" in the whole repository** finds a SHA, branch, tag or any revision git
  understands (for example `v1.0~2`) also outside the loaded history, and **Search all history
  for "…"** opens the commit search with your text.
- **Go to branch…** lists the local branches and the remote branches the graph knows. You can
  also click a branch in the Branches section.
- **Go to tag…** lists every local tag. You can also click a tag in the Tags section.
- **Go to stash…** shows a stash, like clicking it in the Stashes section.

When the commit is further down than the loaded history, the graph loads more until it gets
there. When the graph doesn't show it at all, the commit is still selected (its changes show),
and a note above the graph says why: it is only on a branch the graph hides (**Show ⟨branch⟩**
adds that branch to the graph), it is on a side branch of the one branch the graph shows
(**Show all branches**), or it is on no branch at all. A commit inside a collapsed merge (see
[Collapse merges](history.md#collapse-merges)) is shown by expanding that merge.

## Keyboard shortcuts

Press **?** (outside a text field), or choose **Keyboard shortcuts** in the header's **Help**
menu, for the full list of shortcuts. The most useful ones:

| Keys | What they do |
|---|---|
| Ctrl/Cmd+K | Open the command palette |
| Ctrl/Cmd+Alt+B | Show or hide the sidebar (where the host doesn't use it) |
| Ctrl/Cmd+Z | Undo the last Git operation |
| Ctrl/Cmd+Shift+Z or Ctrl+Y | Redo |
| Ctrl/Cmd+W | Close the active tab (in VSCode only where VSCode doesn't use it) |
| Ctrl/Cmd+Shift+T | Reopen the last closed tab (where the host doesn't use it) |
| Ctrl/Cmd+F | Search commits (with the commit list focused) |
| Ctrl/Cmd+Enter | Commit (in the commit message) |
| ↑/↓ or j/k | Move through commits or changed files |
| Shift+↑/↓ | Select several commits or changed files |
| Ctrl/Cmd+A | Select every file in a change list (with the list focused) |
| Home/End, PageUp/PageDown | Jump in the commit list |
| s | Stage or unstage the selected files, lines or hunk |
| d d | Discard the selected lines or hunk |
| [ and ] | Previous and next hunk (in a diff); parent and child commit (in the commit list) |
| Shift+[ | Choose one of a merge commit's parents (in the commit list) |
| →/← | Expand or collapse a merge with **Collapse merges** on (in the commit list) |
| Alt+Left/Right (Cmd+[ / Cmd+] on macOS) | Back and forward through the selected commits (where the host doesn't use them) |
| Left/Right, Home/End | Switch tabs, with a tab focused |
| Delete | Close the focused tab |
| F2 | Rename the focused tab |
| Menu key or Shift+F10 | Open the focused item's menu |
| Esc | Close a dialog or menu |

**Context menus from the keyboard.** The Menu key or Shift+F10 (Fn+Shift+F10 on most Mac
keyboards) opens the menu of the focused branch, remote, changed file, commit or tab. In a menu,
↑/↓, Home/End or a letter move between items, Enter or Space runs one, Esc closes the menu, and
Tab closes it and moves on.

**Resizing.** With a divider between the columns focused, Left/Right resize the columns.
Double-click a divider to collapse or expand the sidebar.
