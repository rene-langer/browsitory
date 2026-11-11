### Conflicts and rebase messages

- Fixed: the rebase planner's combined message for Squash rows held only the commit summaries,
  so a rebase started without editing it dropped every body. It now starts from the whole
  messages of the group's first commit and its Squash rows, bodies included, as `git rebase -i`
  does; Start waits until they have loaded (#721).
