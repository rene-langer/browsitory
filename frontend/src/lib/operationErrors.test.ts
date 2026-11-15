import { describe, expect, it } from "vitest";
import { failedOperation, type FailedOperation } from "./operationErrors";

// #1032: every mutation's banner starts with the operation and its object, and ends with the
// next step for a failure only git's text explains (an `Other` error).
const ID = "4c5beaf0123456789abcdef0123456789abcdef0";

describe("failedOperation", () => {
  it.each<[string, FailedOperation, FailedOperation]>([
    [
      "switch to a branch",
      failedOperation.switchBranch("feature/x"),
      {
        failed: "Couldn't switch to feature/x",
        next: "Check that the branch still exists and no other Git program is using the repository, then switch again.",
      },
    ],
    [
      "switch to a commit",
      failedOperation.switchDetached(ID),
      {
        failed: "Couldn't switch to 4c5beaf (detached)",
        next: "Check that no other Git program is using the repository, then switch again.",
      },
    ],
    [
      "stash and switch",
      failedOperation.stashAndSwitch("feature/x"),
      { failed: "Couldn't stash and switch to feature/x", next: "Check the stash list for your changes, then switch again." },
    ],
    [
      "create a branch",
      failedOperation.createBranch("feature/x"),
      { failed: "Couldn't create branch feature/x", next: "Check the name and the start point, then create it again." },
    ],
    [
      "delete a branch",
      failedOperation.deleteBranch("feature/x"),
      {
        failed: "Couldn't delete branch feature/x",
        next: "If another worktree is on it, switch that worktree to another branch, then delete it again.",
      },
    ],
    [
      "rename a branch",
      failedOperation.renameBranch("feature/x", "feature/y"),
      { failed: "Couldn't rename branch feature/x to feature/y", next: "Pick another name, then rename it again." },
    ],
    [
      "create a tag",
      failedOperation.createTag("v1.0"),
      { failed: "Couldn't create tag v1.0", next: "Check the name and the commit, then create it again." },
    ],
    [
      "delete a tag",
      failedOperation.deleteTag("v1.0"),
      { failed: "Couldn't delete tag v1.0", next: "Check that the tag still exists, then delete it again." },
    ],
    [
      "save a stash",
      failedOperation.saveStash(),
      { failed: "Couldn't stash your changes", next: "Check that there are changes to stash, then stash again." },
    ],
    [
      "apply a stash",
      failedOperation.applyStash(1),
      { failed: "Couldn't apply stash@{1}", next: "Commit or stash your changes, then apply it again." },
    ],
    [
      "pop a stash",
      failedOperation.popStash(0),
      { failed: "Couldn't pop stash@{0}", next: "Commit or stash your changes, then pop it again." },
    ],
    [
      "drop a stash",
      failedOperation.dropStash(2),
      { failed: "Couldn't drop stash@{2}", next: "Check that the stash still exists, then drop it again." },
    ],
    [
      "reset",
      failedOperation.reset(ID),
      {
        failed: "Couldn't reset to 4c5beaf",
        next: "Check that no other Git program is using the repository, then reset again.",
      },
    ],
    [
      "commit",
      failedOperation.commit(),
      { failed: "Couldn't commit", next: "Check the message and the staged changes, then commit again." },
    ],
    [
      "amend",
      failedOperation.amend(),
      { failed: "Couldn't amend the last commit", next: "Check the message, then amend again." },
    ],
    [
      "merge",
      failedOperation.merge("refs/remotes/origin/main"),
      {
        failed: "Couldn't merge origin/main",
        next: "Check that no other Git program is using the repository, then merge again.",
      },
    ],
    [
      "merge fast-forward only",
      failedOperation.merge("feature", true),
      // Its failure may be a diverged branch or anything else; git's text says which (#1033).
      { failed: "Couldn't merge feature", next: null },
    ],
    [
      "rebase",
      failedOperation.rebaseOnto("refs/remotes/origin/main"),
      {
        failed: "Couldn't rebase onto origin/main",
        next: "Check that no other Git program is using the repository, then rebase again.",
      },
    ],
    [
      "fold staged changes",
      failedOperation.foldStaged(ID),
      {
        failed: "Couldn't fold the staged changes into 4c5beaf",
        next: "Check that no other Git program is using the repository, then try again.",
      },
    ],
    [
      "continue a rebase",
      failedOperation.continueRebase(),
      { failed: "Couldn't continue the rebase", next: "Check that every conflict is resolved, then continue again." },
    ],
    [
      "commit a part of a split",
      failedOperation.commitPart(),
      { failed: "Couldn't commit this part", next: "Check the message and the staged changes, then commit again." },
    ],
    [
      "abort a rebase",
      failedOperation.abortRebase(),
      {
        failed: "Couldn't abort the rebase",
        next: "Check that no other Git program is using the repository, then abort again.",
      },
    ],
    [
      "abort a merge",
      failedOperation.abortMerge(),
      {
        failed: "Couldn't abort the merge",
        next: "Check that no other Git program is using the repository, then abort again.",
      },
    ],
    [
      "cherry-pick one commit",
      failedOperation.pick("CherryPick", [ID]),
      {
        failed: "Cherry-pick of 4c5beaf didn't start",
        next: "Check that no other Git program is using the repository, then try again.",
      },
    ],
    [
      "revert three commits",
      failedOperation.pick("Revert", [ID, ID, ID]),
      {
        failed: "Revert of 3 commits didn't start",
        next: "Check that no other Git program is using the repository, then try again.",
      },
    ],
    [
      "create a worktree",
      failedOperation.createWorktree("hotfix"),
      {
        failed: "Couldn't create worktree hotfix",
        next: "Check that its folder doesn't exist yet and no other worktree is on its branch, then create it again.",
      },
    ],
    [
      "remove a worktree",
      failedOperation.removeWorktree("hotfix"),
      {
        failed: "Couldn't remove worktree hotfix",
        next: "Check that it has no uncommitted changes and isn't locked, then remove it again.",
      },
    ],
    [
      "prune worktrees",
      failedOperation.pruneWorktrees(),
      {
        failed: "Couldn't prune worktrees",
        next: "Check that no other Git program is using the repository, then prune again.",
      },
    ],
    [
      "initialize a submodule",
      failedOperation.initSubmodule("libs/core"),
      {
        failed: "Couldn't initialize submodule libs/core",
        next: "Check the submodule's entry in .gitmodules, then initialize it again.",
      },
    ],
    [
      "update a submodule",
      failedOperation.updateSubmodule("libs/core"),
      {
        failed: "Couldn't update submodule libs/core",
        next: "Check the submodule's URL and your connection, then update it again.",
      },
    ],
    [
      "revert a hunk of a commit",
      failedOperation.commitChange({ commitId: ID, path: "a.txt", hunk: 0, reverse: true }),
      {
        failed: "Couldn't revert the hunk in a.txt from 4c5beaf",
        next: "Commit or stash the changes it touches, then try again.",
      },
    ],
    [
      "apply a file of a commit",
      failedOperation.commitChange({ commitId: ID, path: "a.txt", hunk: null, reverse: false }),
      { failed: "Couldn't apply a.txt from 4c5beaf", next: "Commit or stash the changes it touches, then try again." },
    ],
    [
      "save a patch",
      failedOperation.savePatch(),
      { failed: "Couldn't save the patch", next: "Check that you can write to that folder, then save it again." },
    ],
    [
      "read a patch file",
      failedOperation.readPatch("/tmp/0001-fix.patch"),
      {
        failed: "Couldn't read 0001-fix.patch",
        next: "Check that the file exists and is a patch, then open it again.",
      },
    ],
    [
      "open a patch file",
      failedOperation.readPatch(null),
      {
        failed: "Couldn't read the patch file",
        next: "Check that the file exists and is a patch, then open it again.",
      },
    ],
    [
      "apply a patch",
      failedOperation.applyPatch(null),
      { failed: "Couldn't apply the patch", next: "Commit or stash the changes it touches, then apply it again." },
    ],
    [
      "apply a dropped patch file",
      failedOperation.applyPatch("C:\\patches\\0002-docs.patch"),
      { failed: "Couldn't apply 0002-docs.patch", next: "Commit or stash the changes it touches, then apply it again." },
    ],
    [
      "restore a reflog entry",
      failedOperation.restoreReflog("refs/heads/main", ID),
      {
        failed: "Couldn't restore main to 4c5beaf",
        next: "Check that no other Git program is using the repository, then restore it again.",
      },
    ],
  ])("%s", (_name, actual, expected) => {
    expect(actual).toEqual(expected);
  });

  it.each([
    ["Reword", "Couldn't reword 4c5beaf"],
    ["Squash", "Couldn't squash 4c5beaf into its parent"],
    ["Fixup", "Couldn't fix up 4c5beaf into its parent"],
    ["MoveUp", "Couldn't move 4c5beaf up"],
    ["MoveDown", "Couldn't move 4c5beaf down"],
    ["Drop", "Couldn't drop 4c5beaf"],
    ["Edit", "Couldn't start editing 4c5beaf"],
    ["Split", "Couldn't start splitting 4c5beaf"],
  ] as const)("names the history edit %s", (kind, failed) => {
    expect(failedOperation.historyEdit(kind, ID)).toEqual({
      failed,
      next: "Check that no other Git program is using the repository, then try again.",
    });
  });
});
