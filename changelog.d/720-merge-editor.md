### Conflicts and rebase messages

The Resolve conflict pane is now a merge editor (#720).

- Added: each conflicting block shows the common ancestor's lines behind **Show base**, and can
  take them with **Accept base**, or both sides in either order (**Ours then theirs**, **Theirs
  then ours**) (#720).
- Added: a block's result is editable; typing in it marks the block "Edited", and **Reset**
  clears it. **Edit whole result** edits the whole file as one text, with conflict markers
  around blocks still open, and asks before dropping those edits on the way back (#720).
- Added: long runs of unchanged lines between the conflicts are collapsed to the lines next to
  them, with **Show N unchanged lines** (#720).
- Changed: a conflict where both sides added the file gets the merge editor, with an empty base,
  instead of the keep-ours/keep-theirs/delete choice (#720).
- Changed: **Accept both** is now **Ours then theirs** (#720).
- Fixed: a line of the file that looks like a conflict marker (`=======`, `<<<<<<<`) is no
  longer read as one when the file is split into blocks (#720).
- Fixed: opening a conflicted binary file read a merge result libgit2 doesn't produce for
  binary files (undefined behaviour, which could show an empty block view). The pane now says
  the file is binary and offers the whole-file choices (#720).
- Fixed: the rebase planner's combined message for Squash rows held only the commit summaries,
  so a rebase started without editing it dropped every body. It now starts from the whole
  messages of the group's first commit and its Squash rows, bodies included, as `git rebase -i`
  does; Start waits until they have loaded (#721).
