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
