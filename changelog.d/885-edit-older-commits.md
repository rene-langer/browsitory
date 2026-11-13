### Edit older commits: planner, guide and E2E

- Changed: the rebase planner's **Edit** step stops like Edit commit…: the commit's changes
  staged on its parent, "Editing 1a2b3c4: change the files, then Continue" in the status strip,
  the commit's message editable in the commit area, and **Commit part** to split it. The guide's
  Rebase and Rewriting pages describe it, with a new screenshot (#885).
- Internal: an E2E flow per app edits an older commit's file, continues and checks the history
  (#885). The guide's merge editor screenshot shows the three panes (#933).
