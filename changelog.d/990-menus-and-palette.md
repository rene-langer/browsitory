### Menus and command palette

Each action now has one name, the same in its menu, the command palette and the shortcut list.

- Changed: an item ends in "…" exactly when a dialog or confirmation follows ("Edit remote…",
  "Manage credentials…", "Remove remote…", "Add remote…", "Discard all unstaged changes…", a
  worktree's "Remove ⟨path⟩…"). The palette's name is the menu's plus what it acts on ("Push
  current branch to origin" in both, "Push current branch to…", "Reset main to 1a2b3c4…"). The
  palette's diff options name what they will do ("Show whitespace" or "Hide whitespace", "Wrap
  lines" or "Stop wrapping lines", "More context lines"), and the shortcut list uses the
  palette's names ("Pull", "Go to commit message", "Close tab"). The toolbar's stash menu reads
  "Stash all changes" (#990).
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
