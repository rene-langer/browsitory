import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { RepoError, type RepoClient, type StashEntry } from "../ipc/RepoClient";
import { useAppState, type UseAppStateResult } from "./useAppState";
import { TEST_REPO_PATH, transferClient } from "./useAppStateTestClient";

// #1032: every mutation's banner starts with the operation and its object and ends with the next
// step; git's own text stays in the middle (and in full in the operation log).
const ID = "4c5beaf0123456789abcdef0123456789abcdef0";
const RAW = "failed to lock file '.git/index.lock' for writing; class=Index (10); code=Locked (-14)";
const LOCKED = "Check that no other Git program is using the repository";

const stash: StashEntry = {
  index: 0,
  message: "WIP on main: abc1234 msg",
  commitId: "s0",
  branch: "main",
  timestamp: 1700000000,
  fileCount: 1,
  hasStagedChanges: false,
};

function rejects(): () => Promise<never> {
  return async () => {
    throw new RepoError("Other", RAW);
  };
}

async function failed(overrides: Partial<RepoClient>, run: (app: UseAppStateResult) => Promise<unknown>) {
  const { result } = renderHook(() => useAppState(transferClient({ listStashes: async () => [stash], ...overrides }), TEST_REPO_PATH));
  await act(() => result.current.refresh());
  await act(async () => void (await run(result.current)));
  return { error: result.current.state.error, hint: result.current.state.errorHint };
}

describe("a failed operation's banner (#1032)", () => {
  it.each<[string, Partial<RepoClient>, (app: UseAppStateResult) => Promise<unknown>, string, string]>([
    [
      "switch to a branch",
      { switchBranch: rejects() },
      (app) => app.switchBranch("feature/x"),
      "Couldn't switch to feature/x",
      "Check that the branch still exists and no other Git program is using the repository, then switch again.",
    ],
    ["switch to a commit", { checkoutCommit: rejects() }, (app) => app.checkoutCommit(ID), "Couldn't switch to 4c5beaf (detached)", `${LOCKED}, then switch again.`],
    [
      "stash and switch",
      { stashAndSwitch: rejects() },
      (app) => app.stashAndSwitch("feature/x", true),
      "Couldn't stash and switch to feature/x",
      "Check the stash list for your changes, then switch again.",
    ],
    [
      "create a branch",
      { createBranch: rejects() },
      (app) => app.createBranch("feature/x", "main"),
      "Couldn't create branch feature/x",
      "Check the name and the start point, then create it again.",
    ],
    [
      "delete a branch",
      { deleteBranch: rejects() },
      (app) => app.deleteBranch("feature/x", false),
      "Couldn't delete branch feature/x",
      "If another worktree is on it, switch that worktree to another branch, then delete it again.",
    ],
    [
      "rename a branch",
      { renameBranch: rejects() },
      (app) => app.renameBranch("feature/x", "feature/y"),
      "Couldn't rename branch feature/x to feature/y",
      "Pick another name, then rename it again.",
    ],
    [
      "create a tag",
      { createTag: rejects() },
      (app) => app.createTag("v1.0", null, null),
      "Couldn't create tag v1.0",
      "Check the name and the commit, then create it again.",
    ],
    [
      "delete a tag",
      { deleteTag: rejects() },
      (app) => app.deleteTag("v1.0"),
      "Couldn't delete tag v1.0",
      "Check that the tag still exists, then delete it again.",
    ],
    [
      "save a stash",
      { saveStash: rejects() },
      (app) => app.saveStash(),
      "Couldn't stash your changes",
      "Check that there are changes to stash, then stash again.",
    ],
    [
      "apply a stash",
      { applyStash: rejects() },
      (app) => app.applyStash(0),
      "Couldn't apply stash@{0}",
      "Commit or stash your changes, then apply it again.",
    ],
    ["pop a stash", { popStash: rejects() }, (app) => app.popStash(0), "Couldn't pop stash@{0}", "Commit or stash your changes, then pop it again."],
    [
      "drop a stash",
      { dropStash: rejects() },
      (app) => app.dropStash(0),
      "Couldn't drop stash@{0}",
      "Check that the stash still exists, then drop it again.",
    ],
    ["reset", { resetToCommit: rejects() }, (app) => app.resetToCommit(ID, "mixed"), "Couldn't reset to 4c5beaf", `${LOCKED}, then reset again.`],
    [
      "commit",
      { commit: rejects() },
      (app) => app.commit("feat: x", false),
      "Couldn't commit",
      "Check the message and the staged changes, then commit again.",
    ],
    [
      "amend",
      { amendCommit: rejects() },
      (app) => app.amendCommit("feat: x", false),
      "Couldn't amend the last commit",
      "Check the message, then amend again.",
    ],
    [
      "merge",
      { mergeBranch: rejects(), setMergeOptions: async () => {} },
      (app) => app.mergeBranch("feature/x", { fastForward: "Allow", squash: false }),
      "Couldn't merge feature/x",
      `${LOCKED}, then merge again.`,
    ],
    [
      "rebase",
      { startRebase: rejects() },
      (app) => app.startRebase("main", []),
      "Couldn't rebase onto main",
      `${LOCKED}, then rebase again.`,
    ],
    [
      "a history edit",
      { commitsSince: rejects() },
      (app) => app.editHistory(ID, "main", { kind: "Drop" }),
      "Couldn't drop 4c5beaf",
      `${LOCKED}, then try again.`,
    ],
    [
      "fold staged changes",
      { foldStagedInto: rejects() },
      (app) => app.foldStaged(ID),
      "Couldn't fold the staged changes into 4c5beaf",
      `${LOCKED}, then try again.`,
    ],
    ["abort a rebase", { abortRebase: rejects() }, (app) => app.abortRebase(), "Couldn't abort the rebase", `${LOCKED}, then abort again.`],
    ["abort a merge", { abortMerge: rejects() }, (app) => app.abortMerge(), "Couldn't abort the merge", `${LOCKED}, then abort again.`],
    [
      "cherry-pick several commits",
      { cherryPickMany: rejects() },
      (app) => app.cherryPickMany([ID, ID]),
      "Cherry-pick of 2 commits didn't start",
      `${LOCKED}, then try again.`,
    ],
    [
      "create a worktree",
      { createWorktree: rejects() },
      (app) => app.createWorktree("hotfix", "../hotfix", "hotfix", null),
      "Couldn't create worktree hotfix",
      "Check that its folder doesn't exist yet and no other worktree is on its branch, then create it again.",
    ],
    [
      "remove a worktree",
      { removeWorktree: rejects() },
      (app) => app.removeWorktree("hotfix"),
      "Couldn't remove worktree hotfix",
      "Check that it has no uncommitted changes and isn't locked, then remove it again.",
    ],
    ["prune worktrees", { pruneWorktrees: rejects() }, (app) => app.pruneWorktrees(), "Couldn't prune worktrees", `${LOCKED}, then prune again.`],
    [
      "initialize a submodule",
      { initSubmodule: rejects() },
      (app) => app.initSubmodule("libs/core"),
      "Couldn't initialize submodule libs/core",
      "Check the submodule's entry in .gitmodules, then initialize it again.",
    ],
    [
      "update a submodule",
      { updateSubmodule: rejects() },
      (app) => app.updateSubmodule("libs/core", false),
      "Couldn't update submodule libs/core",
      "Check the submodule's URL and your connection, then update it again.",
    ],
    [
      "revert a file of a commit",
      { applyCommitChange: rejects() },
      (app) => app.applyCommitChange({ commitId: ID, path: "a.txt", oldPath: null, hunk: null, reverse: true, stage: false }),
      "Couldn't revert a.txt from 4c5beaf",
      "Commit or stash the changes it touches, then try again.",
    ],
    [
      "save a patch",
      { getPatch: rejects() },
      (app) => app.savePatch({ kind: "Commits", commitIds: [ID] }, "0001-x.patch"),
      "Couldn't save the patch",
      "Check that you can write to that folder, then save it again.",
    ],
    [
      "open a patch file",
      { openPatchFile: rejects() },
      (app) => app.applyPatchFile(),
      "Couldn't read the patch file",
      "Check that the file exists and is a patch, then open it again.",
    ],
    [
      "apply a dropped patch file",
      { readPatchFile: async () => ({ base64: "eA==" }), applyPatch: rejects() },
      (app) => app.applyPatchFiles(["/tmp/0001-fix.patch"]),
      "Couldn't apply 0001-fix.patch",
      "Commit or stash the changes it touches, then apply it again.",
    ],
    [
      "restore a reflog entry",
      { restoreReflogEntry: rejects() },
      (app) => app.restoreReflogEntry("refs/heads/main", ID),
      "Couldn't restore main to 4c5beaf",
      `${LOCKED}, then restore it again.`,
    ],
  ])("%s", async (_name, overrides, run, operation, next) => {
    const { error, hint } = await failed(overrides, run);
    expect(error).toBe(`${operation}: ${RAW}`);
    expect(hint).toBe(next);
  });

  it("ends with the next step the error's kind has, when it has one", async () => {
    const { error, hint } = await failed(
      {
        switchBranch: async () => {
          throw new RepoError("LocalChanges", "your local changes to src/map.js would be overwritten");
        },
        getSwitchConflicts: async () => ["src/map.js"],
      },
      (app) => app.switchBranch("feature/x"),
    );
    expect(error).toBe("Couldn't switch to feature/x: your local changes to src/map.js would be overwritten");
    expect(hint).toBe("Commit or stash the changes to those files first, or use Stash and switch.");
  });

  it("hands a create form the same text as the banner", async () => {
    const { result } = renderHook(() => useAppState(transferClient({ createBranch: rejects() }), TEST_REPO_PATH));
    let message: string | null = null;
    await act(async () => {
      message = await result.current.createBranch("feature/x", "main");
    });
    expect(message).toBe(`Couldn't create branch feature/x: ${RAW}`);
  });

  it("says what to do about a failed commit hook", async () => {
    const { error, hint } = await failed(
      { commit: async () => ({ kind: "HookFailed", hook: "pre-commit", output: "lint\n" }) },
      (app) => app.commit("feat: x", false),
    );
    expect(error).toBe("Couldn't commit: The pre-commit hook failed.");
    expect(hint).toBe("Fix what the hook reports, then commit again.");
  });

  it("says what to do about a failed amend hook", async () => {
    const { error, hint } = await failed(
      { amendCommit: async () => ({ kind: "HookFailed", hook: "commit-msg", output: "bad\n" }) },
      (app) => app.amendCommit("feat: x", false),
    );
    expect(error).toBe("Couldn't amend the last commit: The commit-msg hook failed.");
    expect(hint).toBe("Fix what the hook reports, then amend again.");
  });

  it("says what to do about a hook that rejected a patch", async () => {
    const { error, hint } = await failed(
      { applyPatch: async () => ({ kind: "HookFailed", hook: "applypatch-msg", output: "no\n", subject: "fix: x" }) },
      (app) => app.applyPatch({ base64: "eA==" }, { parts: [] } as never, "Commits"),
    );
    expect(error).toBe("The applypatch-msg hook failed on “fix: x”. Nothing was applied.");
    expect(hint).toBe("Fix what the hook reports, then apply the patch again.");
  });

  it("says what to do about a fast-forward-only merge that couldn't fast-forward", async () => {
    const { error, hint } = await failed(
      {
        setMergeOptions: async () => {},
        mergeBranch: async () => {
          throw new RepoError("Other", "Can't fast-forward: 'feature' has diverged from the current branch.");
        },
      },
      (app) => app.mergeBranch("feature", { fastForward: "Only", squash: false }),
    );
    expect(error).toBe("Couldn't merge feature: Can't fast-forward: 'feature' has diverged from the current branch.");
    expect(hint).toBe("Pick “Fast-forward if possible” and merge again, or rebase first.");
  });

  it("adds no next step when the history changed under a history edit: the message is the remediation", async () => {
    const { error, hint } = await failed(
      { commitsSince: async () => [{ id: "p" }, { id: "other" }] as never },
      (app) => app.editHistory(ID, "p", { kind: "Squash" }),
    );
    expect(error).toBe("Couldn't squash 4c5beaf into its parent: The history changed. Refresh and try again.");
    expect(hint).toBeNull();
  });

  it("keeps a dropped patch file's read error, which names the file, and adds the next step", async () => {
    const { error, hint } = await failed(
      {
        readPatchFile: async () => {
          throw new RepoError("Other", "Couldn't read /tmp/a.patch: gone");
        },
      },
      (app) => app.applyPatchFiles(["/tmp/a.patch"]),
    );
    expect(error).toBe("Couldn't read /tmp/a.patch: gone");
    expect(hint).toBe("Check that the file exists and is a patch, then open it again.");
  });
});
