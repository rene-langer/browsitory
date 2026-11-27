import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { RepoError, type ConfirmedAbort, type HeadInfo, type RepoClient, type StashEntry, type TerminalSequence } from "../ipc/RepoClient";
import { fakeRepoClient } from "../test/fakeRepoClient";
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

/** `getHeadInfo` answering the empty repository's head with `extra` on top. */
function headWith(extra: Partial<HeadInfo>): RepoClient["getHeadInfo"] {
  return async () => ({ ...(await fakeRepoClient({}, "empty").getHeadInfo(TEST_REPO_PATH)), ...extra });
}

const terminalRevert: TerminalSequence = {
  kind: "Revert",
  pausedOnConflict: true,
  startedAt: ID,
  startedAtSummary: "start",
  rewinds: false,
  commitsToRemove: 0,
};

const confirmedAbort: ConfirmedAbort = { headId: ID, startedAt: ID, rewinds: false, commitsToRemove: 0, pausedOnConflict: true };

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
    // #1205: the kind comes from the stored sequence, or for a terminal one from that.
    [
      "abort a cherry-pick",
      { abortSequence: rejects(), getHeadInfo: headWith({ sequence: { kind: "CherryPick", currentStep: 1, totalSteps: 2, currentCommit: ID } }) },
      (app) => app.abortSequence(),
      "Couldn't abort the cherry-pick",
      `${LOCKED}, then abort again.`,
    ],
    [
      "abort a revert",
      { abortSequence: rejects(), getHeadInfo: headWith({ sequence: { kind: "Revert", currentStep: 1, totalSteps: 2, currentCommit: ID } }) },
      (app) => app.abortSequence(),
      "Couldn't abort the revert",
      `${LOCKED}, then abort again.`,
    ],
    [
      "abort a revert started in a terminal",
      { abortSequence: rejects(), getHeadInfo: headWith({ terminalSequence: terminalRevert }) },
      (app) => app.abortSequence(confirmedAbort),
      "Couldn't abort the revert",
      `${LOCKED}, then abort again.`,
    ],
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

  // #1122: staging, discarding, conflict resolution, remotes, the upstream, the graph's branch
  // selection and the forge account name the operation and the next step too.
  describe("for the actions #1032 left out (#1122)", () => {
    const HUNK = { contextLines: 3, algorithm: "Default" as const };
    const CHANGED = "Check that the file hasn't changed since the diff was shown, then";

    it.each<[string, Partial<RepoClient>, (app: UseAppStateResult) => Promise<unknown>, string, string | null]>([
      ["stage a file", { stageFile: rejects() }, (app) => app.stageFile("src/a.ts"), "Couldn't stage src/a.ts", `${LOCKED}, then stage again.`],
      ["unstage a file", { unstageFile: rejects() }, (app) => app.unstageFile("src/a.ts"), "Couldn't unstage src/a.ts", `${LOCKED}, then unstage again.`],
      ["stage files", { stagePaths: rejects() }, (app) => app.stageAllFiles(["a.ts", "b.ts"]), "Couldn't stage 2 files", `${LOCKED}, then stage again.`],
      ["stage one file of a selection", { stagePaths: rejects() }, (app) => app.stageAllFiles(["a.ts"]), "Couldn't stage a.ts", `${LOCKED}, then stage again.`],
      ["unstage files", { unstagePaths: rejects() }, (app) => app.unstageAllFiles(["a.ts", "b.ts"]), "Couldn't unstage 2 files", `${LOCKED}, then unstage again.`],
      ["stage a hunk", { stageHunk: rejects() }, (app) => app.stageHunk("a.ts", 1, 1, HUNK), "Couldn't stage the hunk in a.ts", `${CHANGED} stage it again.`],
      ["unstage a hunk", { unstageHunk: rejects() }, (app) => app.unstageHunk("a.ts", 1, 1, HUNK), "Couldn't unstage the hunk in a.ts", `${CHANGED} unstage it again.`],
      ["discard a hunk", { discardHunk: rejects() }, (app) => app.discardHunk("a.ts", 1, 1, HUNK), "Couldn't discard the hunk in a.ts", `${CHANGED} discard it again.`],
      [
        "stage lines",
        { stageLines: rejects() },
        (app) => app.stageLines("a.ts", 1, 1, HUNK, [0], ["x"]),
        "Couldn't stage the selected lines in a.ts",
        `${CHANGED} stage them again.`,
      ],
      [
        "unstage lines",
        { unstageLines: rejects() },
        (app) => app.unstageLines("a.ts", 1, 1, HUNK, [0], ["x"]),
        "Couldn't unstage the selected lines in a.ts",
        `${CHANGED} unstage them again.`,
      ],
      [
        "discard lines",
        { discardLines: rejects() },
        (app) => app.discardLines("a.ts", 1, 1, HUNK, [0], ["x"]),
        "Couldn't discard the selected lines in a.ts",
        `${CHANGED} discard them again.`,
      ],
      ["discard a file", { discardFile: rejects() }, (app) => app.discardFile("a.ts"), "Couldn't discard the changes to a.ts", `${LOCKED}, then discard again.`],
      [
        "discard all files",
        { discardAllUnstaged: rejects() },
        (app) => app.discardAllUnstaged(["a.ts", "b.ts", "c.ts"]),
        "Couldn't discard the changes to 3 files",
        `${LOCKED}, then discard again.`,
      ],
      [
        "restore a file's version",
        { restoreFileVersion: rejects() },
        (app) => app.restoreFileVersion(ID, "a.ts"),
        "Couldn't restore a.ts as of 4c5beaf",
        `${LOCKED}, then restore it again.`,
      ],
      [
        "ignore a path",
        { ignorePath: rejects() },
        (app) => app.ignorePath("build/"),
        "Couldn't ignore build/",
        "Check that you can write to .gitignore, then ignore it again.",
      ],
      [
        "ignore paths",
        { ignorePaths: rejects() },
        (app) => app.ignorePaths(["a.log", "b.log"]),
        "Couldn't ignore 2 paths",
        "Check that you can write to .gitignore, then ignore them again.",
      ],
      [
        "resolve a conflict",
        { resolveConflict: rejects() },
        (app) => app.resolveConflict("a.ts", "x\n"),
        "Couldn't resolve a.ts",
        `${LOCKED}, then resolve it again.`,
      ],
      [
        "resolve an add/delete conflict",
        { resolveAddDeleteConflict: rejects() },
        (app) => app.resolveAddDeleteConflict("a.ts", "Delete"),
        "Couldn't resolve a.ts",
        `${LOCKED}, then resolve it again.`,
      ],
      [
        "resolve a whole file",
        { resolveWholeFile: rejects() },
        (app) => app.resolveWholeFile("a.ts", "Ours"),
        "Couldn't resolve a.ts",
        `${LOCKED}, then resolve it again.`,
      ],
      [
        "mark resolved",
        { markResolved: rejects() },
        (app) => app.markResolved("a.ts", false),
        "Couldn't mark a.ts as resolved",
        `${LOCKED}, then mark it again.`,
      ],
      [
        "add a remote",
        { listRemotes: async () => [], addRemote: rejects() },
        (app) => app.addRemote("fork", "../fork.git", null),
        "Couldn't add remote fork",
        "Check the name and the URL, then add it again.",
      ],
      [
        "rename a remote",
        { renameRemote: rejects() },
        (app) => app.renameRemote("origin", "upstream"),
        "Couldn't rename remote origin to upstream",
        "Check the name, then rename it again.",
      ],
      [
        "edit a remote's URLs",
        { updateRemoteUrls: rejects() },
        (app) => app.updateRemoteUrls("origin", "../o.git", null),
        "Couldn't change the URLs of remote origin",
        "Check the URLs, then save them again.",
      ],
      ["remove a remote", { removeRemote: rejects() }, (app) => app.removeRemote("origin", false), "Couldn't remove remote origin", `${LOCKED}, then remove it again.`],
      [
        "save a credential",
        { saveHttpsCredential: rejects() },
        (app) => app.saveHttpsCredential("origin", "rene", "token"),
        "Couldn't save the credential for origin",
        "Check the user name and the token, then save them again.",
      ],
      [
        "forget a credential",
        { forgetHttpsCredential: rejects() },
        (app) => app.forgetHttpsCredential("origin"),
        "Couldn't forget the credential for origin",
        null,
      ],
      [
        "set a remote's sign-in mode",
        { setRemoteAuthMode: rejects() },
        (app) => app.setRemoteAuthMode("origin", "SshAgent", null),
        "Couldn't save how to sign in to origin",
        `${LOCKED}, then save it again.`,
      ],
      [
        "set the upstream",
        { setCurrentUpstream: rejects() },
        (app) => app.setCurrentUpstream("origin", "topic"),
        "Couldn't set origin/topic as upstream",
        "Check that the remote branch exists, then set it again.",
      ],
      ["clear the upstream", { clearCurrentUpstream: rejects() }, (app) => app.clearCurrentUpstream(), "Couldn't clear the upstream", `${LOCKED}, then clear it again.`],
      [
        "choose the graph's branches",
        { setGraphBranchSelection: rejects() },
        (app) => app.setGraphBranchSelection(["main"]),
        "Couldn't change which branches the graph shows",
        `${LOCKED}, then try again.`,
      ],
      [
        "choose the graph's remote branches",
        { setGraphRemoteBranchSelection: rejects() },
        (app) => app.setGraphRemoteBranchSelection(["origin/main"]),
        "Couldn't change which branches the graph shows",
        `${LOCKED}, then try again.`,
      ],
      [
        "reset the graph's branches",
        { clearGraphBranchSelection: rejects() },
        (app) => app.clearGraphBranchSelection(),
        "Couldn't change which branches the graph shows",
        `${LOCKED}, then try again.`,
      ],
      [
        "show all branches",
        { clearGraphBranchSelection: rejects() },
        (app) => app.showAllGraphBranches(),
        "Couldn't change which branches the graph shows",
        `${LOCKED}, then try again.`,
      ],
      [
        "isolate a branch",
        { setGraphBranchSelection: rejects() },
        (app) => app.isolateGraphBranch("local", "topic"),
        "Couldn't isolate topic",
        `${LOCKED}, then try again.`,
      ],
      ["collapse merges", { setGraphCollapseMerges: rejects() }, (app) => app.setCollapseMerges(true), "Couldn't collapse merges", `${LOCKED}, then try again.`],
      ["expand merges", { setGraphCollapseMerges: rejects() }, (app) => app.setCollapseMerges(false), "Couldn't expand merges", `${LOCKED}, then try again.`],
      [
        "connect a forge account",
        { saveForgeToken: rejects() },
        (app) => app.saveForgeToken("GitHub", "rene", "token"),
        "Couldn't connect to GitHub as rene",
        "Check the account and the token, then connect again.",
      ],
      [
        "disconnect a forge account",
        { forgetForgeToken: rejects() },
        (app) => app.forgetForgeToken("GitHub", "rene"),
        "Couldn't disconnect rene from GitHub",
        null,
      ],
      [
        "create a pull request",
        { createPullRequest: rejects() },
        (app) => app.createPullRequest("origin", "rene", { title: "x", description: null, sourceBranch: "topic", targetBranch: "main" }),
        "Couldn't create the pull request from topic into main",
        "Check the branches and the title, then create it again.",
      ],
    ])("%s", async (_name, overrides, run, operation, next) => {
      const { error, hint } = await failed(overrides, run);
      expect(error).toBe(`${operation}: ${RAW}`);
      expect(hint).toBe(next);
    });
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

  // Without a typed kind (#1033) a diverged branch can't be told from other failures, so a
  // fast-forward-only merge adds no next step of its own: git's text says what happened.
  it("adds no guessed next step to a fast-forward-only merge that failed", async () => {
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
    expect(hint).toBeNull();
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

  // A1: a pick that fails after some steps leaves a paused sequence (`SequenceRun::stopped`), a
  // paused step or a moved HEAD; that pick did start, so the banner doesn't say it didn't.
  it.each<[string, Partial<HeadInfo>, (app: UseAppStateResult) => Promise<unknown>, Partial<RepoClient>, string, string]>([
    [
      "a stopped cherry-pick sequence",
      { sequence: { kind: "CherryPick", currentStep: 2, totalSteps: 2, currentCommit: ID } },
      (app) => app.cherryPickMany([ID, ID]),
      { cherryPickMany: rejects() },
      "Couldn't finish the cherry-pick of 2 commits",
      "Fix what stopped it, then continue or abort the cherry-pick.",
    ],
    [
      "a paused revert step",
      { operation: "Revert" },
      (app) => app.revertCommit(ID),
      { revertCommit: rejects() },
      "Couldn't finish the revert of 4c5beaf",
      "Fix what stopped it, then continue or abort the revert.",
    ],
    [
      "a moved HEAD",
      { headId: "moved" },
      (app) => app.cherryPick(ID),
      { cherryPick: rejects() },
      "Couldn't finish the cherry-pick of 4c5beaf",
      "Fix what stopped it, then continue or abort the cherry-pick.",
    ],
  ])("says a pick that left %s couldn't finish, not that it didn't start", async (_name, after, run, call, operation, next) => {
    let picked = false;
    const before: HeadInfo = { ...(await fakeRepoClient({}, "empty").getHeadInfo(TEST_REPO_PATH)), headId: "before" };
    const reject = Object.values(call)[0] as () => Promise<never>;
    const { error, hint } = await failed(
      {
        ...Object.fromEntries(
          Object.keys(call).map((name) => [
            name,
            async () => {
              picked = true;
              return reject();
            },
          ]),
        ),
        getHeadInfo: async () => (picked ? { ...before, ...after } : before),
      },
      run,
    );
    expect(error).toBe(`${operation}: ${RAW}`);
    expect(hint).toBe(next);
  });
});
