import { describe, expect, it, vi, beforeEach } from "vitest";
import {
  type AppCommands,
  buildCommands,
  commitPickerItems,
  type CommandContext,
  type CommandGroup,
  type SidebarCommands,
  type TabCommands,
  type GoToCommands,
  filterAndSortCommands,
  loadRecentCommandIds,
  recordCommandUsed,
  type Command,
  type PickerItem,
} from "./commands";
import type { AppState, UseAppStateResult } from "../state/useAppState";
import { createElement } from "react";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { BranchTree } from "../components/BranchTree";
import { CommitGraph } from "../components/CommitGraph";
import { StashPanel } from "../components/StashPanel";
import { WorktreePanel } from "../components/WorktreePanel";
import { stashBlockers } from "./actions/stashActions";
import { operationBlockers } from "./operationStatus";
import type { OpenRepo } from "../state/useOpenRepos";
import type { SidebarPanelId } from "../state/useSidebarPanelVisibility";

function makeAppState(overrides: Partial<UseAppStateResult["state"]> = {}): UseAppStateResult {
  return {
    state: {
      repoPath: "/repo",
      selectedRow: "uncommitted",
      navigation: { entries: ["uncommitted"], index: 0, merging: false },
      status: [],
      commits: [],
      graphLimit: 300,
      hasMoreHistory: false,
      graphSelection: { local: null, remote: null },
      branches: [
        { name: "main", isCurrent: true },
        { name: "feature", isCurrent: false },
      ],
      worktrees: [
        { name: "main", path: "/repo", head: "abc", isMain: true, isLocked: false, isPrunable: false },
        { name: "wt1", path: "/repo-wt1", head: "def", isMain: false, isLocked: false, isPrunable: false },
      ],
      submodules: [
        { path: "libs/a", url: "https://example.com/a.git", gitlinkId: "aaa", initialized: false, headId: null, workdir: null },
        { path: "libs/b", url: "https://example.com/b.git", gitlinkId: "bbb", initialized: true, headId: "bbb111", workdir: "/repo/libs/b" },
      ],
      reflogRefs: [],
      selectedReflogReference: null,
      reflog: [
        {
          reference: "HEAD@{0}",
          oldId: "aaa",
          newId: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
          committerName: "Test",
          committerEmail: "test@example.com",
          timestamp: 0,
          message: "commit: test",
          summary: null,
        },
      ],
      remotes: [{ name: "origin", fetchUrl: "https://example.com/repo.git", pushUrl: null, authMode: null, authUsername: null }],
      tags: [{ name: "v1.0", targetId: "aaa", commitId: "aaa", annotated: false, message: null, taggerName: null, timestamp: null }],
      upstream: { localBranch: "main", remoteName: "origin", remoteBranch: "main" },
      remoteUpstreams: {},
      forgeRepositories: [],
      pullRequests: {},
      createBranchDraft: null,
      stashes: [{ index: 0, message: "WIP", commitId: "ccc" }],
      mergeMessage: null,
      forcePushOffer: null,
      fetchPrune: null,
      headInfo: { headId: null, detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null },
      amendMode: false,
      skipHooks: false,
      hookRun: null,
      hookFailure: null,
      clearCommitMessage: null,
      rebaseProgress: null,
      rebaseOnto: null,
      rebasePreset: null,
      historyEditOutcome: null,
      patchOutcome: null,
      pendingPull: null,
      pullOutcome: null,
      mergeOutcome: null,
      stashRestoreIndex: true,
      stashOutcome: null,
      transfer: null,
      error: null,
      errorHint: null,
      pending: false,
      addRemoteDraftOpen: false,
      dialog: null,
      refreshGeneration: 0,
      undoState: { undo: null, redo: null },
      ...overrides,
    },
    selectRow: vi.fn(),
    stageFile: vi.fn(),
    unstageFile: vi.fn(),
    stageAllFiles: vi.fn(),
    unstageAllFiles: vi.fn(),
    stageHunk: vi.fn(),
    unstageHunk: vi.fn(),
    discardHunk: vi.fn(),
    stageLines: vi.fn(),
    unstageLines: vi.fn(),
    discardLines: vi.fn(),
    discardFile: vi.fn(),
    restoreFileVersion: vi.fn(),
    discardAllUnstaged: vi.fn(),
    ignorePath: vi.fn(),
    ignorePaths: vi.fn(),
    commit: vi.fn(),
    createBranch: vi.fn(),
    switchBranch: vi.fn(),
    deleteBranch: vi.fn(),
    renameBranch: vi.fn(),
    createWorktree: vi.fn(),
    removeWorktree: vi.fn(),
    pruneWorktrees: vi.fn(),
    initSubmodule: vi.fn(),
    updateSubmodule: vi.fn(),
    selectReflogReference: vi.fn(),
    restoreReflogEntry: vi.fn(),
    addRemote: vi.fn(),
    renameRemote: vi.fn(),
    updateRemoteUrls: vi.fn(),
    removeRemote: vi.fn(),
    saveHttpsCredential: vi.fn(),
    forgetHttpsCredential: vi.fn(),
    setRemoteAuthMode: vi.fn(),
    setCurrentUpstream: vi.fn(),
    clearCurrentUpstream: vi.fn(),
    listRemoteBranches: vi.fn(),
    fetchRemote: vi.fn(),
    createTag: vi.fn(),
    openDialog: vi.fn(),
    closeDialog: vi.fn(),
    deleteTag: vi.fn(),
    pushCurrentBranch: vi.fn(),
    pushTags: vi.fn(),
    pullCurrentUpstream: vi.fn(),
    clearPendingPull: vi.fn(),
    openCreateBranchDraft: vi.fn(),
    closeCreateBranchDraft: vi.fn(),
    openAddRemoteDraft: vi.fn(),
    closeAddRemoteDraft: vi.fn(),
    cherryPick: vi.fn(),
    revertCommit: vi.fn(),
    cherryPickMany: vi.fn(),
    revertMany: vi.fn(),
    continueSequence: vi.fn(),
    abortSequence: vi.fn(),
    resetToCommit: vi.fn(),
    checkoutCommit: vi.fn(),
    amendCommit: vi.fn(),
    setAmendMode: vi.fn(),
    cancelForcePush: vi.fn(),
    forcePushCurrentBranch: vi.fn(),
    fetchAllRemotes: vi.fn(),
    setFetchPrune: vi.fn(),
    pushBranch: vi.fn(),
    deleteRemoteBranch: vi.fn(),
    deleteRemoteTag: vi.fn(),
    saveStash: vi.fn(),
    applyStash: vi.fn(),
    popStash: vi.fn(),
    setStashRestoreIndex: vi.fn(),
    dropStash: vi.fn(),
    getMergeOptions: vi.fn(),
    mergeBranch: vi.fn(),
    mergePull: vi.fn(),
    resolveConflict: vi.fn(),
    resolveAddDeleteConflict: vi.fn(),
    markResolved: vi.fn(),
    resolveWholeFile: vi.fn(),
    abortMerge: vi.fn(),
    openRebasePlanner: vi.fn(),
    openRebaseSelection: vi.fn(),
    closeRebasePlanner: vi.fn(),
    startRebase: vi.fn(),
    editHistory: vi.fn(),
    rebaseContinue: vi.fn(),
    abortRebase: vi.fn(),
    listPullRequests: vi.fn(),
    saveForgeToken: vi.fn(),
    forgetForgeToken: vi.fn(),
    createPullRequest: vi.fn(),
    openExternalUrl: vi.fn(),
    applyCommitChange: vi.fn(),
    savePatch: vi.fn(),
    applyPatchFile: vi.fn(),
    setGraphBranchSelection: vi.fn(),
    setGraphRemoteBranchSelection: vi.fn(),
    clearGraphBranchSelection: vi.fn(),
    showAllGraphBranches: vi.fn(),
    isolateGraphBranch: vi.fn(),
    loadMoreHistory: vi.fn(),
    loadHistoryThrough: vi.fn(),
    refresh: vi.fn(),
    undo: vi.fn(),
    redo: vi.fn(),
    refreshUndoState: vi.fn(),
    dismissError: vi.fn(),
    setSkipHooks: vi.fn(),
    cancelHookRun: vi.fn(),
    openHookFailure: vi.fn(),
    closeHookFailure: vi.fn(),
    clearCommitMessageHandled: vi.fn(),
    retryWithoutHooks: vi.fn(),
    stepNavigation: vi.fn(() => null),
  };
}

function sidebar(panelVisibility: Record<SidebarPanelId, boolean> = allPanelsVisible()): SidebarCommands {
  return { panelVisibility, revealSection: vi.fn(), revealRemote: vi.fn() };
}

function allPanelsVisible(): Record<SidebarPanelId, boolean> {
  return { stash: true, worktree: true, submodule: true, reflog: true, tags: true, pullRequests: true };
}

function tabs(overrides: Partial<TabCommands> = {}): TabCommands {
  return { onOpenRepoTab: vi.fn(), otherOpenRepos: [], onSwitchRepoTab: vi.fn(), onCloseActiveRepoTab: vi.fn(), ...overrides };
}

/** The palette's commands with the sidebar and tab handlers every workspace passes, plus `extra`. */
function commandsFor(appState: UseAppStateResult, extra: Partial<Omit<CommandContext, "appState">> = {}): Command[] {
  return buildCommands({ appState, sidebar: sidebar(), tabs: tabs(), ...extra });
}

describe("buildCommands", () => {
  it("offers to toggle skip hooks only when commit hooks are installed, even while busy", () => {
    const withHooks = makeAppState({
      headInfo: { headId: "abc", detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: ["pre-commit"], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null },
      skipHooks: false,
      pending: true,
    });
    const command = commandsFor(withHooks).find((c) => c.id === "toggle-skip-hooks");
    expect(command?.label).toBe("Skip hooks for the next commit");
    expect(command?.keywords).toEqual(expect.arrayContaining(["no-verify", "pre-commit"]));
    command?.run();
    expect(withHooks.selectRow).toHaveBeenCalledWith("uncommitted");
    expect(withHooks.setSkipHooks).toHaveBeenCalledWith(true);

    const on = makeAppState({
      headInfo: { headId: "abc", detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: ["commit-msg"], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null },
      skipHooks: true,
    });
    const toggleOff = commandsFor(on).find((c) => c.id === "toggle-skip-hooks");
    expect(toggleOff?.label).toBe("Run hooks for the next commit");
    toggleOff?.run();
    expect(on.setSkipHooks).toHaveBeenCalledWith(false);

    expect(commandsFor(makeAppState()).some((c) => c.id === "toggle-skip-hooks")).toBe(false);
  });

  it("offers Fetch all remotes, Push branch to… and Delete remote branch… when a remote exists", () => {
    const appState = makeAppState();
    const commands = commandsFor(appState);
    const fetchAll = commands.find((command) => command.label === "Fetch all remotes")!;
    expect(fetchAll.keywords).toEqual(expect.arrayContaining(["fetch", "all", "remotes", "prune"]));
    fetchAll.run();
    commands.find((command) => command.label === "Push branch to…")!.run();
    commands.find((command) => command.label === "Delete remote branch…")!.run();
    expect(appState.fetchAllRemotes).toHaveBeenCalled();
    expect(appState.openDialog).toHaveBeenCalledWith({ kind: "push", localBranch: "main" });
    expect(appState.openDialog).toHaveBeenCalledWith({ kind: "deleteRemoteBranch", remoteName: null, branchName: null });
  });

  it("hides the new remote commands without a remote, and Push branch to… on a detached HEAD", () => {
    const labels = (overrides: Parameters<typeof makeAppState>[0]) =>
      commandsFor(makeAppState(overrides)).map((command) => command.label);
    expect(labels({ remotes: [] })).not.toContain("Fetch all remotes");
    expect(labels({ remotes: [] })).not.toContain("Push branch to…");
    expect(labels({ remotes: [] })).not.toContain("Delete remote branch…");
    const detached = labels({
      branches: [],
      headInfo: { headId: "abc", detachedAt: "abc1234", headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null },
    });
    expect(detached).not.toContain("Push branch to…");
    expect(detached).toContain("Fetch all remotes");
  });

  it("hides the new remote commands while a repository operation runs", () => {
    const labels = commandsFor(makeAppState({ pending: true })).map((command) => command.label);
    expect(labels).not.toContain("Fetch all remotes");
    expect(labels).not.toContain("Push branch to…");
    expect(labels).not.toContain("Delete remote branch…");
  });

  it("offers Apply patch… unless an operation runs, and Save staged changes as patch… with staged files (#448)", () => {
    const appState = makeAppState({ status: [{ path: "a.txt", oldPath: null, staged: true, kind: "Modified" }] });
    const commands = commandsFor(appState);
    commands.find((command) => command.id === "apply-patch")!.run();
    expect(appState.applyPatchFile).toHaveBeenCalled();
    commands.find((command) => command.id === "save-staged-patch")!.run();
    expect(appState.savePatch).toHaveBeenCalledWith({ kind: "Staged" }, "staged.patch");

    const ids = (overrides: Parameters<typeof makeAppState>[0]) => commandsFor(makeAppState(overrides)).map((c) => c.id);
    expect(ids({ pending: true })).not.toContain("apply-patch");
    expect(ids({ status: [] })).not.toContain("save-staged-patch");
  });

  it("includes a shortcuts command only when a handler is supplied", () => {
    const onShow = vi.fn();
    expect(commandsFor(makeAppState()).some((c) => c.id === "show-shortcuts")).toBe(false);
    const cmd = commandsFor(makeAppState(), { onShowShortcuts: onShow }).find(
      (c) => c.id === "show-shortcuts",
    );
    expect(cmd?.label).toBe("Show keyboard shortcuts");
    cmd?.run();
    expect(onShow).toHaveBeenCalled();
  });

  it("offers Search commits when search commands are given", () => {
    const onSearchCommits = vi.fn();
    expect(commandsFor(makeAppState()).some((c) => c.id === "search-commits")).toBe(false);
    const cmd = commandsFor(makeAppState(), { search: { onSearchCommits } }).find((c) => c.id === "search-commits");
    expect(cmd?.label).toBe("Search commits");
    cmd?.run();
    expect(onSearchCommits).toHaveBeenCalled();
  });

  it("includes a Close tab command, grouped under Repositories, only with the tab commands", () => {
    const onClose = vi.fn();
    expect(commandsFor(makeAppState(), { tabs: undefined }).some((c) => c.id === "close-tab")).toBe(false);
    const cmd = commandsFor(makeAppState(), { tabs: tabs({ onCloseActiveRepoTab: onClose }) }).find(
      (c) => c.id === "close-tab",
    );
    expect(cmd?.label).toBe("Close tab");
    expect(cmd?.group).toBe("Repositories");
    cmd?.run();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("omits Close tab while a repository operation is in progress", () => {
    const commands = commandsFor(makeAppState({ pending: true }), { tabs: tabs({ onCloseActiveRepoTab: vi.fn() }) });
    expect(commands.some((c) => c.id === "close-tab")).toBe(false);
  });

  it("adds clone and new-repository commands when app commands are given", () => {
    const onCloneRepository = vi.fn();
    const onNewRepository = vi.fn();
    const commands = commandsFor(makeAppState(), { app: {
      onCloneRepository,
      onNewRepository,
    } });
    commands.find((c) => c.label === "Clone repository…")!.run();
    commands.find((c) => c.label === "New repository…")!.run();
    expect(onCloneRepository).toHaveBeenCalled();
    expect(onNewRepository).toHaveBeenCalled();
  });

  it("omits clone and new-repository commands without app commands", () => {
    const labels = commandsFor(makeAppState()).map((c) => c.label);
    expect(labels).not.toContain("Clone repository…");
  });

  it("includes one switch-branch command per non-current branch", () => {
    const appState = makeAppState();
    const commands = commandsFor(appState);
    const switchCommands = commands.filter((c) => c.id.startsWith("switch-branch:"));
    expect(switchCommands).toHaveLength(1);
    expect(switchCommands[0].label).toBe("Switch to feature");
    switchCommands[0].run();
    expect(appState.switchBranch).toHaveBeenCalledWith("feature");
  });

  it("includes fetch/push/push-tags commands per remote", () => {
    const appState = makeAppState();
    const commands = commandsFor(appState);
    const fetchCmd = commands.find((c) => c.id === "fetch-remote:origin");
    expect(fetchCmd?.label).toBe("Fetch origin");
    fetchCmd?.run();
    expect(appState.fetchRemote).toHaveBeenCalledWith("origin");

    const pushCmd = commands.find((c) => c.id === "push-branch:origin");
    pushCmd?.run();
    expect(appState.pushCurrentBranch).toHaveBeenCalledWith("origin");

    const pushTagsCmd = commands.find((c) => c.id === "push-tags:origin");
    expect(pushTagsCmd?.label).toBe("Push all tags to origin");
    pushTagsCmd?.run();
    expect(appState.pushTags).toHaveBeenCalledWith("origin", []);
  });

  // AUD-2026-09-05-FE-001: remote-tracking branch Checkout/Set-upstream (BranchTree's
  // `remoteBranchItems`) have no keyboard path at all — their rows only exist in the DOM once the
  // remote's folder is expanded. This command expands that folder so the (now keyboard-focusable)
  // branch rows become reachable, mirroring the existing "navigate instead of mutating directly"
  // pattern below.
  describe("browse-remote-branches", () => {
    it("includes one browse-remote-branches command per remote", () => {
      const commands = commandsFor(makeAppState());
      const command = commands.find((c) => c.id === "browse-remote-branches:origin");
      expect(command?.label).toBe("Browse branches on origin");
    });

    it("asks the sidebar to reveal the remote's folder instead of mutating anything (#494)", () => {
      const appState = makeAppState();
      const sidebarCommands = sidebar();
      const commands = commandsFor(appState, { sidebar: sidebarCommands });
      commands.find((c) => c.id === "browse-remote-branches:origin")?.run();
      expect(sidebarCommands.revealRemote).toHaveBeenCalledWith("origin");
      expect(appState.fetchRemote).not.toHaveBeenCalled();
    });

    it("is omitted without the sidebar commands", () => {
      const commands = commandsFor(makeAppState(), { sidebar: undefined });
      expect(commands.some((c) => c.id.startsWith("browse-remote-branches:"))).toBe(false);
    });

    it("is omitted while a repository operation is in progress", () => {
      const commands = commandsFor(makeAppState({ pending: true }));
      expect(commands.some((c) => c.id.startsWith("browse-remote-branches:"))).toBe(false);
    });
  });

  it("only includes pull when an upstream is set", () => {
    const withUpstream = commandsFor(makeAppState());
    expect(withUpstream.some((c) => c.id === "pull")).toBe(true);

    const withoutUpstream = commandsFor(makeAppState({ upstream: null }));
    expect(withoutUpstream.some((c) => c.id === "pull")).toBe(false);
  });

  it("only includes rebase continue/abort while a rebase is in progress", () => {
    const noRebase = commandsFor(makeAppState());
    expect(noRebase.some((c) => c.id === "rebase-continue")).toBe(false);

    const withRebase = commandsFor(makeAppState({ rebaseProgress: { currentStep: 1, totalSteps: 3 } }));
    expect(withRebase.some((c) => c.id === "rebase-continue")).toBe(true);
    expect(withRebase.some((c) => c.id === "rebase-abort")).toBe(true);
  });

  it("omits Continue rebase while the rebase is interrupted but keeps Abort", () => {
    const rebase = { branch: "main", onto: "o", currentStep: 1, totalSteps: 3, health: { kind: "Diverged" as const, reason: "HeadMoved" as const } };
    const base = makeAppState().state.headInfo;
    const commands = commandsFor(makeAppState({ rebaseProgress: { currentStep: 1, totalSteps: 3 }, headInfo: { ...base, rebase } }));
    expect(commands.some((c) => c.id === "rebase-continue")).toBe(false);
    expect(commands.some((c) => c.id === "rebase-abort")).toBe(true);
  });

  it("only includes abort merge while a merge is in progress", () => {
    const noMerge = commandsFor(makeAppState());
    expect(noMerge.some((c) => c.id === "merge-abort")).toBe(false);

    const withMerge = commandsFor(makeAppState({ mergeMessage: "Merge branch" }));
    expect(withMerge.some((c) => c.id === "merge-abort")).toBe(true);
  });

  it("offers no Abort merge for a rebase started in a terminal, though git wrote MERGE_MSG (#263)", () => {
    const base = makeAppState().state.headInfo;
    const commands = commandsFor(makeAppState({
        headInfo: {
          ...base,
          terminalRebase: { kind: "Rebase", branch: "topic", onto: "abc", currentStep: 1, totalSteps: 2 },
        },
        mergeMessage: "t1",
      }));
    expect(commands.some((c) => c.id === "merge-abort")).toBe(false);
    expect(commands.some((c) => c.id === "rebase-abort" || c.id === "rebase-continue")).toBe(false);
  });

  it("offers Abort (no Continue) for a cherry-pick started in a terminal (#258, #253)", () => {
    const base = makeAppState().state.headInfo;
    const terminalSequence = {
      kind: "Revert" as const,
      pausedOnConflict: false,
      startedAt: "abc",
      startedAtSummary: "X",
      rewinds: true,
      commitsToRemove: 1,
    };
    const appState = makeAppState({ headInfo: { ...base, terminalSequence } });
    const commands = commandsFor(appState);
    const abort = commands.find((c) => c.id === "abort-sequence");
    expect(abort?.label).toBe("Abort revert");
    expect(commands.some((c) => c.id === "continue-sequence")).toBe(false);
    abort?.run();
    expect(appState.abortSequence).toHaveBeenCalled();

    // Its paused step: the same Abort, not "Abort merge" (which would end only the step).
    const paused = commandsFor(makeAppState({
        headInfo: { ...base, operation: "Revert", terminalSequence: { ...terminalSequence, pausedOnConflict: true } },
        mergeMessage: "Revert",
      }));
    expect(paused.find((c) => c.id === "abort-sequence")?.label).toBe("Abort revert");
    expect(paused.some((c) => c.id === "merge-abort")).toBe(false);

    // A merge started during it is a plain merge.
    const merging = commandsFor(makeAppState({ headInfo: { ...base, operation: "Merge", terminalSequence }, mergeMessage: "Merge" }));
    expect(merging.some((c) => c.id === "merge-abort")).toBe(true);
    expect(merging.some((c) => c.id === "abort-sequence")).toBe(false);
  });

  it("offers Continue/Abort for a stopped cherry-pick or revert sequence, and hides other operations", () => {
    const none = commandsFor(makeAppState());
    expect(none.some((c) => c.id === "continue-sequence" || c.id === "abort-sequence")).toBe(false);

    const base = makeAppState();
    const appState = makeAppState({
      headInfo: {
        ...base.state.headInfo,
        operation: "Revert",
        sequence: { kind: "Revert", currentStep: 2, totalSteps: 3, currentCommit: "c" },
      },
      mergeMessage: "Revert \"x\"",
    });
    const commands = commandsFor(appState);
    const cont = commands.find((c) => c.id === "continue-sequence");
    const abort = commands.find((c) => c.id === "abort-sequence");
    expect(cont?.label).toBe("Continue revert");
    expect(abort?.label).toBe("Abort revert");
    // The plain "Abort merge" would abort only the paused step's git state.
    expect(commands.some((c) => c.id === "merge-abort")).toBe(false);
    expect(commands.some((c) => c.id === "pull")).toBe(false);
    cont?.run();
    abort?.run();
    expect(appState.continueSequence).toHaveBeenCalled();
    expect(appState.abortSequence).toHaveBeenCalled();

    const picking = commandsFor(makeAppState({
        headInfo: { ...base.state.headInfo, sequence: { kind: "CherryPick", currentStep: 1, totalSteps: 2, currentCommit: "c" } },
      }));
    expect(picking.find((c) => c.id === "continue-sequence")?.label).toBe("Continue cherry-pick");
    expect(picking.find((c) => c.id === "abort-sequence")?.label).toBe("Abort cherry-pick");
  });

  it("includes apply/drop per stash", () => {
    const appState = makeAppState();
    const commands = commandsFor(appState);
    const applyCmd = commands.find((c) => c.id === "apply-stash:0");
    expect(applyCmd?.label).toBe("Apply stash: WIP");
    applyCmd?.run();
    expect(appState.applyStash).toHaveBeenCalledWith(0);

    expect(commands.some((c) => c.id === "drop-stash:0")).toBe(true);
  });

  it("offers merge-branch per non-current local branch and omits it while an operation runs", () => {
    const appState = makeAppState();
    const commands = commandsFor(appState);
    const merge = commands.find((c) => c.id === "merge-branch:feature");
    expect(merge?.label).toBe("Merge feature into current branch…");
    expect(commands.some((c) => c.id === "merge-branch:main")).toBe(false);
    merge?.run();
    expect(appState.openDialog).toHaveBeenCalledWith({ kind: "merge", branchName: "feature" });

    const merging = commandsFor(makeAppState({ mergeMessage: "Merge branch" }));
    expect(merging.some((c) => c.id.startsWith("merge-branch:"))).toBe(false);
  });

  it("offers stash all and stash with options", () => {
    const appState = makeAppState({ status: [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" }] });
    const commands = commandsFor(appState);
    const all = commands.find((c) => c.id === "save-stash");
    expect(all?.label).toBe("Stash all changes");
    all?.run();
    expect(appState.saveStash).toHaveBeenCalledWith();
    const withOptions = commands.find((c) => c.id === "stash-with-options");
    expect(withOptions?.label).toBe("Stash…");
    withOptions?.run();
    expect(appState.openDialog).toHaveBeenCalledWith({ kind: "stash", paths: [] });
  });

  it("offers show and pop per stash", () => {
    const appState = makeAppState();
    const commands = commandsFor(appState);
    const show = commands.find((c) => c.id === "show-stash:0");
    expect(show?.label).toBe("Show stash: WIP");
    show?.run();
    expect(appState.selectRow).toHaveBeenCalledWith({ commitId: "ccc" });
    const pop = commands.find((c) => c.id === "pop-stash:0");
    expect(pop?.label).toBe("Pop stash: WIP");
    pop?.run();
    expect(appState.popStash).toHaveBeenCalledWith(0);
  });

  it("groups merge and stash commands", () => {
    expect(groupOf("merge-branch:feature")).toBe("Branches");
    for (const id of ["stash-with-options", "stash-current-file", "show-stash:0", "pop-stash:0"]) {
      expect(groupOf(id)).toBe("Stash");
    }
  });

  // #561: like the Worktrees panel, the palette opens the main worktree too, never removes it.
  it("offers to open the main worktree, not to remove it", () => {
    const appState = makeAppState();
    const onOpenRepoTab = vi.fn();
    const commands = commandsFor(appState, { tabs: tabs({ onOpenRepoTab: onOpenRepoTab }) });
    const openMain = commands.find((c) => c.id === "open-worktree:/repo");
    expect(openMain?.label).toBe("Open main worktree");
    openMain?.run();
    expect(onOpenRepoTab).toHaveBeenCalledWith("/repo");
    expect(commands.some((c) => c.id === "remove-worktree:main")).toBe(false);

    const openCmd = commands.find((c) => c.id === "open-worktree:/repo-wt1");
    expect(openCmd?.label).toBe("Open worktree wt1");
  });

  it("opening a worktree calls onOpenRepoTab instead of a state mutation", () => {
    const appState = makeAppState({
      worktrees: [{ name: "feature", path: "/repos/feature-wt", head: "abc123", isMain: false, isLocked: false, isPrunable: false }],
    });
    const onOpenRepoTab = vi.fn();
    const commands = commandsFor(appState, { tabs: tabs({ onOpenRepoTab: onOpenRepoTab, onSwitchRepoTab: vi.fn() }) });

    const openCommand = commands.find((c) => c.id === "open-worktree:/repos/feature-wt");
    openCommand?.run();
    expect(onOpenRepoTab).toHaveBeenCalledWith("/repos/feature-wt");
  });

  it("includes a Switch to <repo> command for every other open repo", () => {
    const appState = makeAppState();
    const otherRepos = [
      { path: "/repos/widget", displayName: "widget", alias: null, workspaceId: null },
      { path: "/repos/gadget", displayName: "gadget", alias: null, workspaceId: null },
    ];
    const onSwitchRepoTab = vi.fn();
    const commands = commandsFor(appState, { tabs: tabs({ onOpenRepoTab: vi.fn(), otherOpenRepos: otherRepos, onSwitchRepoTab: onSwitchRepoTab }) });

    const widgetCommand = commands.find((c) => c.id === "switch-repo:/repos/widget");
    expect(widgetCommand?.label).toBe("Switch to widget");
    widgetCommand?.run();
    expect(onSwitchRepoTab).toHaveBeenCalledWith("/repos/widget");
  });

  it("splits submodule commands by initialized state", () => {
    const commands = commandsFor(makeAppState());
    const initCmd = commands.find((c) => c.id === "init-submodule:libs/a");
    expect(initCmd?.label).toBe("Initialize submodule libs/a");
    initCmd?.run();

    const updateCmd = commands.find((c) => c.id === "update-submodule:libs/b");
    expect(updateCmd?.label).toBe("Update submodule libs/b");
    updateCmd?.run();

    expect(commands.some((c) => c.id === "init-submodule:libs/b")).toBe(false);
    expect(commands.some((c) => c.id === "update-submodule:libs/a")).toBe(false);
  });

  it("opens a checked-out submodule's folder in a new tab, and offers nothing to open for one that isn't", () => {
    const onOpenRepoTab = vi.fn();
    const commands = commandsFor(makeAppState(), { tabs: tabs({ onOpenRepoTab: onOpenRepoTab, onSwitchRepoTab: vi.fn() }) });

    const openCmd = commands.find((c) => c.id === "open-submodule:libs/b");
    expect(openCmd?.label).toBe("Open submodule libs/b in new tab");
    expect(openCmd?.keywords).toEqual(expect.arrayContaining(["submodule", "open", "tab", "libs/b"]));
    openCmd?.run();
    expect(onOpenRepoTab).toHaveBeenCalledWith("/repo/libs/b");

    // Not checked out: the palette keeps offering "Initialize submodule libs/a" instead.
    expect(commands.some((c) => c.id === "open-submodule:libs/a")).toBe(false);
  });

  it("emits no per-entry restore-reflog commands, regardless of reflog size", () => {
    // state.reflog is an unbounded read of the whole reflog for the selected
    // reference — a per-entry command would grow without bound. Restoring is
    // reachable only via the single "Go to Reflog" navigate command.
    const manyEntries = Array.from({ length: 500 }, (_, i) => ({
      reference: "HEAD@{0}",
      oldId: "aaa",
      newId: `${i}`.padStart(40, "0"),
      committerName: "Test",
      committerEmail: "test@example.com",
      timestamp: i,
      message: "commit: test",
      summary: null,
    }));
    const commands = commandsFor(makeAppState({ reflog: manyEntries }));
    expect(commands.some((c) => c.id.startsWith("restore-reflog:"))).toBe(false);
    expect(commands.filter((c) => c.id === "go-to:Reflog")).toHaveLength(1);
  });

  describe("destructive commands navigate instead of mutating directly", () => {
    // #494: through the sidebar's reveal requests, which the owning sections handle
    // (`useSidebarReveal`), never by looking up the rendered sidebar.
    function run(id: string) {
      const appState = makeAppState();
      const sidebarCommands = sidebar();
      const command = commandsFor(appState, { sidebar: sidebarCommands }).find((c) => c.id === id);
      expect(command).toBeDefined();
      command?.run();
      return { appState, command, revealSection: sidebarCommands.revealSection };
    }

    it("delete-tag reveals the Tags section instead of calling deleteTag", () => {
      const { appState, revealSection } = run("delete-tag:v1.0");
      expect(revealSection).toHaveBeenCalledWith("tags");
      expect(appState.deleteTag).not.toHaveBeenCalled();
    });

    it("drop-stash selects the stash and reveals the Stashes section instead of dropping", () => {
      const { appState, command, revealSection } = run("drop-stash:0");
      expect(command?.label).toBe("Drop stash: WIP");
      expect(appState.dropStash).not.toHaveBeenCalled();
      expect(appState.selectRow).toHaveBeenCalledWith({ commitId: "ccc" });
      expect(revealSection).toHaveBeenCalledWith("stash");
    });

    it("remove-worktree reveals the Worktrees section instead of calling removeWorktree", () => {
      const { appState, revealSection } = run("remove-worktree:wt1");
      expect(revealSection).toHaveBeenCalledWith("worktree");
      expect(appState.removeWorktree).not.toHaveBeenCalled();
    });

    it("go-to:Reflog (the only reflog affordance left) reveals the Reflog section instead of calling restoreReflogEntry", () => {
      const { appState, revealSection } = run("go-to:Reflog");
      expect(revealSection).toHaveBeenCalledWith("reflog");
      expect(appState.restoreReflogEntry).not.toHaveBeenCalled();
    });

    it.each([
      ["go-to:Branches", "branches"],
      ["go-to:Stashes", "stash"],
      ["go-to:Worktrees", "worktree"],
      ["go-to:Submodules", "submodule"],
      ["go-to:Tags", "tags"],
      ["go-to:Pull Requests", "pullRequests"],
    ])("%s reveals the %s section", (id, section) => {
      expect(run(id).revealSection).toHaveBeenCalledWith(section);
    });

    it("are omitted without the sidebar commands", () => {
      const ids = commandsFor(makeAppState(), { sidebar: undefined }).map((c) => c.id);
      for (const prefix of ["delete-tag:", "drop-stash:", "remove-worktree:", "go-to:"]) {
        expect(ids.some((id) => id.startsWith(prefix))).toBe(false);
      }
    });
  });

  describe("repositoryOperationDisabled guard", () => {
    const guardedStates: [string, Partial<UseAppStateResult["state"]>][] = [
      ["pending", { pending: true }],
      [
        "transfer in progress",
        {
          transfer: {
            operationId: "op-1",
            operation: "Fetch",
            phase: "Receiving",
            errorKind: null,
            current: 1,
            total: 2,
            receivedBytes: 100,
            message: null,
            hookOutput: null,
          },
        },
      ],
      ["merge in progress", { mergeMessage: "Merge branch" }],
      ["rebase in progress", { rebaseProgress: { currentStep: 1, totalSteps: 3 } }],
    ];

    it.each(guardedStates)(
      "omits mutating command families while %s, but keeps escape hatches, refresh, and go-to navigation",
      (_label, overrides) => {
        const commands = commandsFor(makeAppState(overrides));
        const ids = commands.map((c) => c.id);

        // Mutating families must be entirely absent, not merely disabled.
        expect(ids.some((id) => id.startsWith("switch-branch:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("fetch-remote:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("push-branch:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("push-tags:"))).toBe(false);
        expect(ids).not.toContain("pull");
        expect(ids.some((id) => id.startsWith("delete-tag:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("delete-remote-tag:"))).toBe(false);
        expect(ids.some((id) => /^(edit-remote|manage-credentials|remove-remote|checkout-tag):/.test(id))).toBe(false);
        expect(ids.some((id) => id.startsWith("open-worktree:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("remove-worktree:"))).toBe(false);
        expect(ids).not.toContain("prune-worktrees");
        expect(ids.some((id) => id.startsWith("init-submodule:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("update-submodule:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("open-submodule:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("apply-stash:"))).toBe(false);
        // Dropping a stash is blocked only by an action in flight or a transfer (#521, #509).
        const busy = "pending" in overrides || "transfer" in overrides;
        expect(ids.some((id) => id.startsWith("drop-stash:"))).toBe(!busy);

        // Escape hatches and pure navigation stay available in every guarded state.
        expect(ids).toContain("refresh");
        expect(ids).toContain("show-stash:0");
        expect(ids).toContain("add-remote");
        for (const title of [
          "Branches",
          "Stashes",
          "Worktrees",
          "Submodules",
          "Reflog",
          "Tags",
          "Pull Requests",
        ]) {
          expect(ids).toContain(`go-to:${title}`);
        }
      },
    );

    it("keeps merge-abort available while a merge is in progress, even though it mutates state", () => {
      const commands = commandsFor(makeAppState({ mergeMessage: "Merge branch" }));
      expect(commands.some((c) => c.id === "merge-abort")).toBe(true);
    });

    it("keeps rebase-continue/rebase-abort available while a rebase is in progress", () => {
      const commands = commandsFor(makeAppState({ rebaseProgress: { currentStep: 1, totalSteps: 3 } }));
      expect(commands.some((c) => c.id === "rebase-continue")).toBe(true);
      expect(commands.some((c) => c.id === "rebase-abort")).toBe(true);
    });

    // DiffPane's Stash button and the palette share `stashBlockReason` (#495).
    const modified = [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" as const }];
    it.each<[string, Partial<UseAppStateResult["state"]>]>([
      ["an action is pending", { pending: true }],
      ["a merge is in progress", { mergeMessage: "Merge branch" }],
    ])("keeps the stash commands while %s, like DiffPane's Stash button", (_label, overrides) => {
      const ids = commandsFor(makeAppState({ status: modified, ...overrides })).map((c) => c.id);
      expect(ids).toContain("save-stash");
      expect(ids).toContain("stash-with-options");
    });

    it.each<[string, Partial<UseAppStateResult["state"]>]>([
      ["there are no changes", { status: [] }],
      ["a rebase is in progress", { status: modified, rebaseProgress: { currentStep: 1, totalSteps: 3 } }],
      [
        "a cherry-pick sequence is paused",
        { status: modified, headInfo: { ...makeAppState().state.headInfo, sequence: { kind: "CherryPick", currentStep: 1, totalSteps: 2, currentCommit: "c1" } } },
      ],
      [
        "a terminal rebase is paused",
        { status: modified, headInfo: { ...makeAppState().state.headInfo, terminalRebase: { kind: "Rebase", branch: "topic", onto: "c1", currentStep: 1, totalSteps: 2 } } },
      ],
    ])("omits the stash commands while %s, like DiffPane's Stash button", (_label, overrides) => {
      const ids = commandsFor(makeAppState(overrides), { diff: {
        settings: { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true, wordWrap: "Auto", algorithm: "Default", showWhitespace: false },
        update: vi.fn(),
        currentFile: "a.txt",
        requestDiscardCurrentFile: vi.fn(),
        requestDiscardAllUnstaged: vi.fn(),
        openCurrentFileHistory: vi.fn(),
      } }).map((c) => c.id);
      expect(ids).not.toContain("save-stash");
      expect(ids).not.toContain("stash-with-options");
      expect(ids).not.toContain("stash-current-file");
    });
  });

  it("always includes exactly one go-to command per sidebar section", () => {
    const commands = commandsFor(makeAppState());
    const goToCommands = commands.filter((c) => c.id.startsWith("go-to:"));
    expect(goToCommands.map((c) => c.label).sort()).toEqual(
      [
        "Go to Branches",
        "Go to Pull requests",
        "Go to Reflog",
        "Go to Stashes",
        "Go to Submodules",
        "Go to Tags",
        "Go to Worktrees",
      ].sort(),
    );
  });

  it("keeps the go-to id of a renamed section, so palette recents survive the rename (#320)", () => {
    const command = commandsFor(makeAppState()).find((c) => c.label === "Go to Pull requests");
    expect(command?.id).toBe("go-to:Pull Requests");
  });

  it("no longer emits a separate 'Go to Remotes' command (folded into Branches)", () => {
    const commands = commandsFor(makeAppState());
    expect(commands.find((c) => c.id === "go-to:Remotes")).toBeUndefined();
    expect(commands.find((c) => c.id === "go-to:Branches")).toBeDefined();
  });

  describe("hidden sidebar panels", () => {
    // The sidebar panel-visibility toggles (a separate, already-merged task) fully unmount a
    // hidden panel rather than just CSS-hiding it. Without this filtering, every command that
    // navigates to or names something in a hidden panel became a dead no-op: the hidden section
    // isn't there to handle the reveal request.
    const allVisible = { stash: true, worktree: true, submodule: true, reflog: true, tags: true, pullRequests: true };

    it("omits 'Go to <Panel>' for a panel whose visibility is false", () => {
      const commands = commandsFor(makeAppState(), { sidebar: sidebar({ ...allVisible, tags: false }), tabs: tabs({ onOpenRepoTab: vi.fn(), onSwitchRepoTab: vi.fn() }) });
      expect(commands.some((c) => c.id === "go-to:Tags")).toBe(false);
      // Unaffected sections keep their command.
      expect(commands.some((c) => c.id === "go-to:Stashes")).toBe(true);
    });

    it("keeps 'Go to Branches' even though Branches has no visibility toggle", () => {
      const commands = commandsFor(makeAppState(), { sidebar: sidebar({ ...allVisible, tags: false }), tabs: tabs({ onOpenRepoTab: vi.fn(), onSwitchRepoTab: vi.fn() }) });
      expect(commands.some((c) => c.id === "go-to:Branches")).toBe(true);
    });

    it("omits per-tag delete commands while the Tags panel is hidden", () => {
      const commands = commandsFor(makeAppState(), { sidebar: sidebar({ ...allVisible, tags: false }), tabs: tabs({ onOpenRepoTab: vi.fn(), onSwitchRepoTab: vi.fn() }) });
      expect(commands.some((c) => c.id.startsWith("delete-tag:"))).toBe(false);
    });

    it("omits per-stash apply/drop commands while the Stashes panel is hidden", () => {
      const commands = commandsFor(makeAppState(), { sidebar: sidebar({ ...allVisible, stash: false }), tabs: tabs({ onOpenRepoTab: vi.fn(), onSwitchRepoTab: vi.fn() }) });
      expect(commands.some((c) => c.id.startsWith("apply-stash:"))).toBe(false);
      expect(commands.some((c) => c.id.startsWith("drop-stash:"))).toBe(false);
    });

    it("omits per-worktree commands while the Worktrees panel is hidden", () => {
      const commands = commandsFor(makeAppState(), { sidebar: sidebar({ ...allVisible, worktree: false }), tabs: tabs({ onOpenRepoTab: vi.fn(), onSwitchRepoTab: vi.fn() }) });
      expect(commands.some((c) => c.id.startsWith("open-worktree:"))).toBe(false);
      expect(commands.some((c) => c.id.startsWith("remove-worktree:"))).toBe(false);
    });

    it("omits per-submodule commands while the Submodules panel is hidden", () => {
      const commands = commandsFor(makeAppState(), { sidebar: sidebar({ ...allVisible, submodule: false }), tabs: tabs({ onOpenRepoTab: vi.fn(), onSwitchRepoTab: vi.fn() }) });
      expect(commands.some((c) => c.id.startsWith("init-submodule:"))).toBe(false);
      expect(commands.some((c) => c.id.startsWith("update-submodule:"))).toBe(false);
      expect(commands.some((c) => c.id.startsWith("open-submodule:"))).toBe(false);
    });

    it("defaults to every panel visible when no panelVisibility argument is passed", () => {
      const commands = commandsFor(makeAppState());
      expect(commands.some((c) => c.id === "go-to:Tags")).toBe(true);
      expect(commands.some((c) => c.id.startsWith("delete-tag:"))).toBe(true);
    });
  });

  it("emits an Add remote command that opens the add-remote draft", () => {
    const appState = makeAppState();
    const commands = commandsFor(appState);
    const addRemote = commands.find((c) => c.id === "add-remote");
    expect(addRemote).toBeDefined();
    addRemote?.run();
    expect(appState.openAddRemoteDraft).toHaveBeenCalledOnce();
  });

  // #509: like "Create tag…", it only opens the form, whose Add remote button carries the busy rule.
  it("offers Add remote while a repository operation is in progress, as the Add menu does", () => {
    const appState = makeAppState({ pending: true });
    const commands = commandsFor(appState);
    expect(commands.find((c) => c.id === "add-remote")).toBeDefined();
  });
});

describe("filterAndSortCommands", () => {
  const commands: PickerItem[] = [
    { id: "a", label: "Switch to main", keywords: ["branch", "switch", "main"], run: vi.fn() },
    { id: "b", label: "Switch to feature", keywords: ["branch", "switch", "feature"], run: vi.fn() },
    { id: "c", label: "Fetch origin", keywords: ["fetch", "remote", "origin"], run: vi.fn() },
  ];

  it("returns everything, unranked-but-stable, for an empty query", () => {
    expect(filterAndSortCommands(commands, "")).toHaveLength(3);
  });

  it("ranks a label-prefix match above a substring-only match", () => {
    const withPrefixAndSubstring: PickerItem[] = [
      { id: "x", label: "Fetch origin", keywords: [], run: vi.fn() },
      { id: "y", label: "Switch to origin-backup", keywords: [], run: vi.fn() },
    ];
    const results = filterAndSortCommands(withPrefixAndSubstring, "fetch");
    expect(results[0].id).toBe("x");
  });

  it("matches on keywords, not just the label", () => {
    const results = filterAndSortCommands(commands, "checkout");
    expect(results).toHaveLength(0); // "checkout" isn't a keyword here — sanity check the negative case
    const results2 = filterAndSortCommands(commands, "origin");
    expect(results2.map((c) => c.id)).toContain("c");
  });

  it("excludes commands with no match at all", () => {
    const results = filterAndSortCommands(commands, "zzz-no-match");
    expect(results).toHaveLength(0);
  });
});

describe("recordCommandUsed / loadRecentCommandIds", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("persists used command ids, most recent first, deduplicated", () => {
    recordCommandUsed("a");
    recordCommandUsed("b");
    recordCommandUsed("a");
    expect(loadRecentCommandIds()).toEqual(["a", "b"]);
  });

  it("returns an empty array when nothing is stored", () => {
    expect(loadRecentCommandIds()).toEqual([]);
  });
});

describe("history commands", () => {
  const HISTORY_IDS = [
    "cherry-pick-selected",
    "revert-selected",
    "checkout-selected",
    "create-tag-selected",
    "create-tag-head",
    "reset-selected",
    "amend-last-commit",
    "force-push",
  ];
  const graphCommit = (id: string, parentIds: string[]) => ({
    id,
    shortId: id,
    summary: `commit ${id}`,
    authorName: "Test",
    authorEmail: "test@example.com",
    timestamp: 0,
    parentIds,
    branchRefs: [],
    remoteBranchRefs: [],
  });
  const historyState: Partial<AppState> = {
    selectedRow: { commitId: "c2" },
    commits: [graphCommit("c1", ["c2"]), graphCommit("c2", [])],
    headInfo: { headId: "c1", detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null },
  };
  // RepoWorkspace's commit actions: the menu's own (#507).
  const makeCommitCommands = () => ({ onCherryPick: vi.fn(), onRevert: vi.fn(), onChooseParent: vi.fn(), onReset: vi.fn() });
  const buildWithCommitCommands = (
    appState: UseAppStateResult,
    commitCommands: ReturnType<typeof makeCommitCommands> = makeCommitCommands(),
    onCloseActiveRepoTab?: () => void,
  ) =>
    commandsFor(appState, { tabs: tabs({ onCloseActiveRepoTab: onCloseActiveRepoTab }), commit: commitCommands });

  it("offers the history commands when a commit is selected and nothing is running", () => {
    const ids = buildWithCommitCommands(makeAppState(historyState)).map((command) => command.id);
    expect(ids).toEqual(expect.arrayContaining(HISTORY_IDS));
  });

  it("omits them while a merge is in progress, except Create tag, whose dialog says why it waits", () => {
    const ids = buildWithCommitCommands(makeAppState({ ...historyState, mergeMessage: "Merge x" })).map((c) => c.id);
    for (const id of HISTORY_IDS) {
      if (id.startsWith("create-tag-")) expect(ids).toContain(id);
      else expect(ids).not.toContain(id);
    }
  });

  it("offers cherry-pick and revert of a merge commit through the parent picker (#495, #507)", () => {
    const merge = { ...graphCommit("m1", ["c1", "c2"]), shortId: "m1" };
    const appState = makeAppState({ ...historyState, selectedRow: { commitId: "m1" }, commits: [merge, ...historyState.commits!] });
    const commitCommands = makeCommitCommands();
    const commands = buildWithCommitCommands(appState, commitCommands);
    expect(commands.find((c) => c.id === "cherry-pick-selected")?.label).toBe("Cherry-pick m1…");
    expect(commands.find((c) => c.id === "revert-selected")?.label).toBe("Revert m1…");
    commands.find((c) => c.id === "cherry-pick-selected")!.run();
    expect(commitCommands.onChooseParent).toHaveBeenLastCalledWith("Cherry-pick", "m1");
    commands.find((c) => c.id === "revert-selected")!.run();
    expect(commitCommands.onChooseParent).toHaveBeenLastCalledWith("Revert", "m1");
    expect(commitCommands.onCherryPick).not.toHaveBeenCalled();
    expect(commitCommands.onRevert).not.toHaveBeenCalled();
    expect(appState.cherryPick).not.toHaveBeenCalled();
    expect(appState.revertCommit).not.toHaveBeenCalled();
  });

  // #507: the palette runs the commit menu's own actions (toast, outcome handling) and opens the
  // reset dialog itself, so it works without the graph row (search results, file history).
  it("runs cherry-pick, revert and reset through the commit menu's actions (#507)", () => {
    const appState = makeAppState(historyState);
    const commitCommands = makeCommitCommands();
    const commands = buildWithCommitCommands(appState, commitCommands);
    commands.find((c) => c.id === "cherry-pick-selected")!.run();
    commands.find((c) => c.id === "revert-selected")!.run();
    commands.find((c) => c.id === "reset-selected")!.run();
    expect(commitCommands.onCherryPick).toHaveBeenCalledWith("c2");
    expect(commitCommands.onRevert).toHaveBeenCalledWith("c2");
    expect(commitCommands.onReset).toHaveBeenCalledWith("c2");
    expect(appState.cherryPick).not.toHaveBeenCalled();
    expect(appState.revertCommit).not.toHaveBeenCalled();
    expect(appState.resetToCommit).not.toHaveBeenCalled();
  });

  it("leaves out cherry-pick, revert and reset without the commit actions to run them", () => {
    const ids = commandsFor(makeAppState(historyState)).map((c) => c.id);
    for (const id of ["cherry-pick-selected", "revert-selected", "reset-selected"]) expect(ids).not.toContain(id);
    expect(ids).toContain("checkout-selected");
  });

  // #503: paused between steps, a terminal rebase writes no MERGE_MSG, so only
  // `headInfo.terminalRebase` says the repository is busy.
  it.each([
    ["rebase", { kind: "Rebase" as const, branch: "topic", onto: "c2", currentStep: 1, totalSteps: 2 }],
    ["git am", { kind: "Am" as const, branch: "", onto: "", currentStep: 1, totalSteps: 2 }],
  ])("omits cherry-pick, revert, checkout and other mutations during a %s started in a terminal (#503)", (_label, terminalRebase) => {
    const ids = buildWithCommitCommands(
      makeAppState({ ...historyState, headInfo: { ...historyState.headInfo!, terminalRebase } }),
      makeCommitCommands(),
      () => {},
    ).map((c) => c.id);
    for (const id of ["cherry-pick-selected", "revert-selected", "checkout-selected", "reset-selected", "pull", "close-tab"]) {
      expect(ids).not.toContain(id);
    }
    expect(ids.some((id) => id.startsWith("switch-branch:"))).toBe(false);
    expect(ids).toContain("refresh");
  });

  // #495: the commit menu and the palette decide with the same rule, in every blocked state.
  describe("agree with the commit menu", () => {
    const head = historyState.headInfo!;
    const states: [string, Partial<AppState>][] = [
      ["nothing runs", {}],
      ["an action is pending", { pending: true }],
      ["a merge is in progress", { mergeMessage: "Merge x" }],
      ["a rebase is in progress", { rebaseProgress: { currentStep: 1, totalSteps: 2 } }],
      ["a sequence is stored", { headInfo: { ...head, sequence: { kind: "CherryPick", currentStep: 1, totalSteps: 2, currentCommit: "c2" } } }],
      ["a terminal rebase is paused", { headInfo: { ...head, terminalRebase: { kind: "Rebase", branch: "topic", onto: "c2", currentStep: 1, totalSteps: 2 } } }],
      ["a terminal git am is paused", { headInfo: { ...head, terminalRebase: { kind: "Am", branch: "", onto: "", currentStep: 1, totalSteps: 2 } } }],
      ["HEAD is selected on a detached HEAD", { selectedRow: { commitId: "c1" }, branches: [{ name: "main", isCurrent: false }], headInfo: { ...head, detachedAt: "c1" } }],
    ];
    const pairs: [string, string][] = [
      ["Cherry-pick", "cherry-pick-selected"],
      ["Revert", "revert-selected"],
      ["Checkout (detached)", "checkout-selected"],
      ["Create tag…", "create-tag-selected"],
    ];

    it.each(states)("while %s", (_label, overrides) => {
      const appState = makeAppState({ ...historyState, ...overrides });
      const { state } = appState;
      const blockers = operationBlockers(state);
      const selectedId = (state.selectedRow as { commitId: string }).commitId;
      const currentBranch = state.branches.find((branch) => branch.isCurrent)?.name ?? null;
      render(
        createElement(CommitGraph, {
          status: state.status,
          commits: state.commits,
          selectedRow: state.selectedRow,
          operationsBlocked: blockers.blocked,
          operationsBlockedReason: blockers.reason,
          headInfo: state.headInfo,
          currentBranch,
          onSelectRow: vi.fn(),
          onBranchFromCommit: vi.fn(),
          onRebaseFromCommit: vi.fn(),
          onCherryPick: vi.fn(),
          onRevert: vi.fn(),
          onCheckoutCommit: vi.fn(),
          onChooseParent: vi.fn(),
          onRequestReset: vi.fn(),
          onEditHeadMessage: vi.fn(),
          onCreateTag: vi.fn(),
        }),
      );
      try {
        const row = screen.getAllByRole("option").find((option) => option.textContent?.includes(`commit ${selectedId}`));
        fireEvent.contextMenu(row!);
        const ids = buildWithCommitCommands(appState).map((command) => command.id);
        const resetItem = screen.getByRole("menuitem", { name: /^Reset .* to here…$/ });
        for (const [label, id] of [...pairs, [resetItem.textContent!, "reset-selected"] as [string, string]]) {
          const item = screen.getByRole("menuitem", { name: label });
          expect({ label, offered: ids.includes(id) }).toEqual({ label, offered: !item.hasAttribute("disabled") });
          if (item.hasAttribute("disabled") && blockers.blocked) expect(item).toHaveAttribute("title", blockers.reason);
        }
      } finally {
        cleanup();
      }
    });
  });

  it("omits cherry-pick for HEAD and force push on a detached HEAD", () => {
    const ids = buildWithCommitCommands(
      makeAppState({
        ...historyState,
        selectedRow: { commitId: "c1" },
        headInfo: { headId: "c1", detachedAt: "c1", headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null },
      }),
    ).map((c) => c.id);
    expect(ids).not.toContain("cherry-pick-selected");
    expect(ids).not.toContain("force-push");
  });

  it("runs cherry-pick on the selected commit and force push through the confirm", () => {
    const appState = makeAppState(historyState);
    const commitCommands = makeCommitCommands();
    const commands = buildWithCommitCommands(appState, commitCommands);
    commands.find((command) => command.id === "cherry-pick-selected")!.run();
    commands.find((command) => command.id === "force-push")!.run();
    expect(commitCommands.onCherryPick).toHaveBeenCalledWith("c2");
    expect(appState.openDialog).toHaveBeenCalledWith({ kind: "forcePush", remoteName: "origin" });
  });

  it("opens the create-tag dialog at the selected commit or at HEAD", () => {
    const appState = makeAppState(historyState);
    const commands = commandsFor(appState);
    const selected = commands.find((command) => command.id === "create-tag-selected")!;
    expect(selected.label).toBe("Create tag at c2…");
    selected.run();
    expect(appState.openDialog).toHaveBeenLastCalledWith({ kind: "createTag", commitId: "c2" });
    const head = commands.find((command) => command.id === "create-tag-head")!;
    expect(head.label).toBe("Create tag at HEAD…");
    head.run();
    expect(appState.openDialog).toHaveBeenLastCalledWith({ kind: "createTag", commitId: "c1" });
  });

  it("offers Create tag at HEAD… only once HEAD has a commit", () => {
    const ids = commandsFor(makeAppState({
        headInfo: { headId: null, detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null },
      })).map((c) => c.id);
    expect(ids).not.toContain("create-tag-head");
  });

  it("omits reset when there is no current branch (detached HEAD)", () => {
    const ids = buildWithCommitCommands(
      makeAppState({
        ...historyState,
        branches: [{ name: "main", isCurrent: false }],
        headInfo: { headId: "c1", detachedAt: "c1", headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: [], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null },
      }),
    ).map((c) => c.id);
    expect(ids).not.toContain("reset-selected");
    expect(ids).toContain("checkout-selected");
  });

  it("groups force push under Remotes", () => {
    expect(groupOf("force-push")).toBe("Remotes");
  });

  describe("one-click history edits (#442)", () => {
    // c1 (HEAD) -> c2 -> c3 -> c4 (root)
    const editState: Partial<AppState> = {
      ...historyState,
      selectedRow: { commitId: "c2" },
      commits: [graphCommit("c1", ["c2"]), graphCommit("c2", ["c3"]), graphCommit("c3", ["c4"]), graphCommit("c4", [])],
    };
    const EDIT_IDS = ["reword-selected", "squash-selected", "fixup-selected", "move-up-selected", "move-down-selected", "drop-selected"];

    it("offers every edit the commit menu enables, and runs it from the same base", () => {
      const appState = makeAppState(editState);
      const commands = commandsFor(appState);
      const byId = (id: string) => commands.find((command) => command.id === id)!;
      expect(EDIT_IDS.map((id) => byId(id).label)).toEqual([
        "Reword c2…",
        "Squash c2 with parent",
        "Fixup c2 into parent",
        "Move c2 up",
        "Move c2 down",
        "Drop c2",
      ]);
      byId("squash-selected").run();
      expect(appState.editHistory).toHaveBeenLastCalledWith("c2", "c4", { kind: "Squash" });
      byId("move-up-selected").run();
      expect(appState.editHistory).toHaveBeenLastCalledWith("c2", "c3", { kind: "MoveUp" });
      byId("drop-selected").run();
      expect(appState.editHistory).toHaveBeenLastCalledWith("c2", "c3", { kind: "Drop" });
      byId("reword-selected").run();
      expect(appState.openDialog).toHaveBeenCalledWith({ kind: "reword", commitId: "c2", onto: "c3" });
    });

    it("leaves out edits the menu disables", () => {
      const head = commandsFor(makeAppState({ ...editState, selectedRow: { commitId: "c1" } })).map((c) => c.id);
      // HEAD: no Move up; its Reword… is Amend last commit.
      expect(head).not.toContain("move-up-selected");
      expect(head).not.toContain("reword-selected");
      expect(head).toContain("drop-selected");
      const dirty = commandsFor(makeAppState({ ...editState, status: [{ path: "a", oldPath: null, staged: false, kind: "Modified" }] })).map((c) => c.id);
      for (const id of EDIT_IDS) expect(dirty).not.toContain(id);
      const busy = commandsFor(makeAppState({ ...editState, pending: true })).map((c) => c.id);
      for (const id of EDIT_IDS) expect(busy).not.toContain(id);
    });
  });
});

describe("diff option and discard commands", () => {
  const settings = { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true, wordWrap: "Auto" as const, algorithm: "Default" as const, showWhitespace: false };
  const build = (state: ReturnType<typeof makeAppState>, extra: Record<string, unknown> = {}) => {
    const diffCommands = {
      settings,
      update: vi.fn(),
      currentFile: null as string | null,
      requestDiscardCurrentFile: vi.fn(),
      requestDiscardAllUnstaged: vi.fn(),
      openCurrentFileHistory: vi.fn(),
      ...extra,
    };
    return { diffCommands, commands: commandsFor(state, { diff: diffCommands }) };
  };

  it("offers stashing the current file only when there is one", () => {
    const state = makeAppState({ status: [{ path: "src/a.ts", oldPath: null, staged: false, kind: "Modified" }] });
    expect(build(state).commands.some((c) => c.id === "stash-current-file")).toBe(false);
    const { commands } = build(state, { currentFile: "src/a.ts" });
    const command = commands.find((c) => c.id === "stash-current-file");
    expect(command?.label).toBe("Stash src/a.ts…");
    command!.run();
    expect(state.openDialog).toHaveBeenCalledWith({ kind: "stash", paths: ["src/a.ts"] });
  });

  it("offers file history of the current file, even while an operation is running", () => {
    const state = makeAppState({ pending: true, status: [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" }] });
    const { commands, diffCommands } = build(state, { currentFile: "a.txt" });

    const command = commands.find((c) => c.id === "file-history-current-file");
    expect(command?.label).toBe("File history of current file");
    command!.run();
    expect(diffCommands.openCurrentFileHistory).toHaveBeenCalled();
  });

  it("omits file history when there is no current file", () => {
    const { commands } = build(makeAppState());

    expect(commands.some((c) => c.id === "file-history-current-file")).toBe(false);
  });

  it("adds diff option commands that update settings", () => {
    const { commands, diffCommands } = build(makeAppState());
    commands.find((c) => c.id === "toggle-ignore-whitespace")!.run();
    expect(diffCommands.update).toHaveBeenCalledWith({ ignoreWhitespace: true });
    commands.find((c) => c.id === "toggle-show-whitespace")!.run();
    expect(diffCommands.update).toHaveBeenCalledWith({ showWhitespace: true });
    commands.find((c) => c.id === "increase-context-lines")!.run();
    expect(diffCommands.update).toHaveBeenCalledWith({ contextLines: 4 });
    commands.find((c) => c.id === "decrease-context-lines")!.run();
    expect(diffCommands.update).toHaveBeenCalledWith({ contextLines: 2 });
    commands.find((c) => c.id === "toggle-syntax-highlighting")!.run();
    expect(diffCommands.update).toHaveBeenCalledWith({ syntaxHighlight: false });
    expect(commands.some((c) => c.id === "discard-current-file")).toBe(false);
    expect(commands.some((c) => c.id === "discard-all-unstaged")).toBe(false);
  });

  it("offers wrap lines and show whole file for the current diff (#469)", () => {
    const currentDiff = { path: "src/a.ts", canShowWholeFile: true, toggleWrap: vi.fn(), toggleWholeFile: vi.fn() };
    const { commands } = build(makeAppState(), { currentDiff });
    const wrap = commands.find((c) => c.id === "toggle-wrap-lines")!;
    expect(wrap.label).toBe("Toggle wrap lines (src/a.ts)");
    wrap.run();
    expect(currentDiff.toggleWrap).toHaveBeenCalled();
    const whole = commands.find((c) => c.id === "toggle-show-whole-file")!;
    expect(whole.label).toBe("Toggle show whole file (src/a.ts)");
    whole.run();
    expect(currentDiff.toggleWholeFile).toHaveBeenCalled();
  });

  it("omits show whole file where the current diff has none, and both without a current diff", () => {
    const currentDiff = { path: null, canShowWholeFile: false, toggleWrap: vi.fn(), toggleWholeFile: vi.fn() };
    const ids = build(makeAppState(), { currentDiff }).commands.map((c) => c.id);
    expect(ids).toContain("toggle-wrap-lines");
    expect(ids).not.toContain("toggle-show-whole-file");
    const none = build(makeAppState()).commands.map((c) => c.id);
    expect(none).not.toContain("toggle-wrap-lines");
    expect(none).not.toContain("toggle-show-whole-file");
  });

  it("offers discard commands that only request a confirmation", () => {
    const state = makeAppState({ status: [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" }] });
    const { commands, diffCommands } = build(state, { currentFile: "a.txt" });
    commands.find((c) => c.id === "discard-current-file")!.run();
    expect(diffCommands.requestDiscardCurrentFile).toHaveBeenCalled();
    commands.find((c) => c.id === "discard-all-unstaged")!.run();
    expect(diffCommands.requestDiscardAllUnstaged).toHaveBeenCalled();
  });

  it("offers deleting the untracked files, which only requests a confirmation (#622)", () => {
    const status = [
      { path: "a.txt", oldPath: null, staged: false, kind: "Modified" as const },
      { path: "new.txt", oldPath: null, staged: false, kind: "New" as const },
    ];
    const requestDeleteUntracked = vi.fn();
    const { commands } = build(makeAppState({ status }), { requestDeleteUntracked });
    const command = commands.find((c) => c.id === "delete-untracked-files");
    expect(command?.label).toBe("Delete untracked files…");
    command!.run();
    expect(requestDeleteUntracked).toHaveBeenCalled();
    expect(command?.group).toBe("Diff");
    // Next to "Discard all unstaged changes" in the palette's list.
    const ids = commands.map((c) => c.id);
    expect(ids.indexOf("delete-untracked-files")).toBe(ids.indexOf("discard-all-unstaged") + 1);
  });

  it("omits deleting the untracked files without any, or while busy (#622)", () => {
    const extra = { requestDeleteUntracked: vi.fn() };
    const tracked = [
      { path: "a.txt", oldPath: null, staged: false, kind: "Modified" as const },
      { path: "staged.txt", oldPath: null, staged: true, kind: "New" as const },
    ];
    expect(build(makeAppState({ status: tracked }), extra).commands.some((c) => c.id === "delete-untracked-files")).toBe(
      false,
    );
    const untracked = [{ path: "new.txt", oldPath: null, staged: false, kind: "New" as const }];
    const busy = build(makeAppState({ pending: true, status: untracked }), extra).commands;
    expect(busy.some((c) => c.id === "delete-untracked-files")).toBe(false);
  });

  it("omits discard-all when only conflicted entries are unstaged", () => {
    const state = makeAppState({ status: [{ path: "a.txt", oldPath: null, staged: false, kind: "Conflicted" }] });
    const { commands } = build(state);
    expect(commands.some((c) => c.id === "discard-all-unstaged")).toBe(false);
  });

  it("omits discard commands while an operation is running", () => {
    const state = makeAppState({ pending: true, status: [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" }] });
    const { commands } = build(state, { currentFile: "a.txt" });
    expect(commands.some((c) => c.id === "discard-current-file")).toBe(false);
    expect(commands.some((c) => c.id === "discard-all-unstaged")).toBe(false);
    expect(commands.some((c) => c.id === "toggle-ignore-whitespace")).toBe(true);
  });

  it("keeps discard commands during a merge, blocking them only while busy (#521)", () => {
    const status = [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" as const }];
    const merging = build(makeAppState({ mergeMessage: "Merge branch 'x'", status }), { currentFile: "a.txt" }).commands;
    expect(merging.some((c) => c.id === "discard-current-file")).toBe(true);
    expect(merging.some((c) => c.id === "discard-all-unstaged")).toBe(true);
    const busy = build(makeAppState({ pending: true, mergeMessage: "Merge branch 'x'", status }), { currentFile: "a.txt" }).commands;
    expect(busy.some((c) => c.id === "discard-all-unstaged")).toBe(false);
  });

  it("offers the selected files' actions while two or more are selected (#449)", () => {
    const status = [
      { path: "a.txt", oldPath: null, staged: false, kind: "Modified" as const },
      { path: "b.txt", oldPath: null, staged: false, kind: "New" as const },
    ];
    const state = makeAppState({ status });
    const selection = { staged: false, paths: ["a.txt", "b.txt"] };
    const requestDiscardSelection = vi.fn();
    const { commands } = build(state, { selection, requestDiscardSelection });

    const stage = commands.find((c) => c.id === "stage-selected-files");
    expect(stage?.label).toBe("Stage 2 selected files");
    stage!.run();
    expect(state.stageAllFiles).toHaveBeenCalledWith(["a.txt", "b.txt"]);
    expect(commands.some((c) => c.id === "unstage-selected-files")).toBe(false);
    commands.find((c) => c.id === "discard-selected-files")!.run();
    expect(requestDiscardSelection).toHaveBeenCalled();
    commands.find((c) => c.id === "stash-selected-files")!.run();
    expect(state.openDialog).toHaveBeenCalledWith({ kind: "stash", paths: ["a.txt", "b.txt"] });

    expect(build(state).commands.some((c) => c.id.endsWith("-selected-files"))).toBe(false);
  });

  it("offers unstaging, not discarding, a staged selection", () => {
    const state = makeAppState({ status: [{ path: "a.txt", oldPath: null, staged: true, kind: "Modified" }] });
    const { commands } = build(state, { selection: { staged: true, paths: ["a.txt", "b.txt"] }, requestDiscardSelection: vi.fn() });

    commands.find((c) => c.id === "unstage-selected-files")!.run();
    expect(state.unstageAllFiles).toHaveBeenCalledWith(["a.txt", "b.txt"]);
    expect(commands.some((c) => c.id === "discard-selected-files")).toBe(false);
  });

  it("omits staging and discarding a selection while an operation is running", () => {
    const state = makeAppState({ pending: true, status: [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" }] });
    const { commands } = build(state, { selection: { staged: false, paths: ["a.txt", "b.txt"] }, requestDiscardSelection: vi.fn() });

    expect(commands.some((c) => c.id === "stage-selected-files")).toBe(false);
    expect(commands.some((c) => c.id === "discard-selected-files")).toBe(false);
  });

  it("offers staging the tracked files' changes only (#621)", () => {
    const status = [
      { path: "a.txt", oldPath: null, staged: false, kind: "Modified" as const },
      { path: "gone.txt", oldPath: null, staged: false, kind: "Deleted" as const },
      { path: "new.txt", oldPath: null, staged: false, kind: "New" as const },
      { path: "shared.txt", oldPath: null, staged: false, kind: "Conflicted" as const },
      { path: "b.txt", oldPath: null, staged: true, kind: "Modified" as const },
    ];
    const state = makeAppState({ status });
    const command = build(state).commands.find((c) => c.id === "stage-modified-files");
    expect(command?.label).toBe("Stage modified files (tracked only)");
    command!.run();
    expect(state.stageAllFiles).toHaveBeenCalledWith(["a.txt", "gone.txt"]);
    expect(command?.group).toBe("Diff");
  });

  it("omits staging the tracked files' changes when there are none, or while busy (#621)", () => {
    const untrackedAndConflict = [
      { path: "new.txt", oldPath: null, staged: false, kind: "New" as const },
      { path: "shared.txt", oldPath: null, staged: false, kind: "Conflicted" as const },
    ];
    const none = build(makeAppState({ status: untrackedAndConflict })).commands;
    expect(none.some((c) => c.id === "stage-modified-files")).toBe(false);
    const modified = [{ path: "a.txt", oldPath: null, staged: false, kind: "Modified" as const }];
    const busy = build(makeAppState({ pending: true, status: modified })).commands;
    expect(busy.some((c) => c.id === "stage-modified-files")).toBe(false);
  });

  it("groups the new commands", () => {
    expect(groupOf("toggle-ignore-whitespace")).toBe("Diff");
    expect(groupOf("discard-all-unstaged")).toBe("Diff");
    expect(groupOf("stage-selected-files")).toBe("Diff");
    expect(groupOf("discard-selected-files")).toBe("Diff");
    expect(groupOf("stash-selected-files")).toBe("Stash");
  });
});

describe("undo commands", () => {
  it("lists undo and redo with their labels when available", () => {
    const onUndo = vi.fn();
    const commands = commandsFor(makeAppState(), { undo: {
      undoLabel: "Stage a.txt",
      redoLabel: null,
      onUndo,
      onRedo: vi.fn(),
    } });
    const undo = commands.find((c) => c.id === "undo");
    expect(undo?.label).toBe("Undo Stage a.txt");
    expect(commands.find((c) => c.id === "redo")).toBeUndefined();
    undo?.run();
    expect(onUndo).toHaveBeenCalledOnce();
  });

  it("omits undo while a repository operation is running", () => {
    const commands = commandsFor(makeAppState({ pending: true }), { undo: {
      undoLabel: "Stage a.txt",
      redoLabel: "x",
      onUndo: vi.fn(),
      onRedo: vi.fn(),
    } });
    expect(commands.find((c) => c.id === "undo")).toBeUndefined();
  });
});

describe("preferences and external tool commands", () => {
  const tools = () => ({
    onOpenTerminal: vi.fn(),
    onRevealRepository: vi.fn(),
    onOpenCurrentFileInEditor: vi.fn(),
    onOpenCurrentFileInDiffTool: vi.fn(),
  });
  const diffCommands = (currentFile: string | null) => ({
    settings: { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true, wordWrap: "Auto" as const, algorithm: "Default" as const, showWhitespace: false },
    update: vi.fn(),
    currentFile,
    requestDiscardCurrentFile: vi.fn(),
    requestDiscardAllUnstaged: vi.fn(),
    openCurrentFileHistory: vi.fn(),
  });
  const build = (state: ReturnType<typeof makeAppState>, currentFile: string | null, toolCommands: ReturnType<typeof tools>) =>
    commandsFor(state, { diff: diffCommands(currentFile), tools: toolCommands });

  it("adds Preferences… with the app commands", () => {
    const onOpenPreferences = vi.fn();
    const commands = commandsFor(makeAppState(), { app: {
      onCloneRepository: vi.fn(),
      onNewRepository: vi.fn(),
      onOpenPreferences,
    } });
    const preferences = commands.find((c) => c.label === "Preferences…");
    expect(preferences?.keywords).toEqual(expect.arrayContaining(["settings", "editor", "terminal"]));
    preferences!.run();
    expect(onOpenPreferences).toHaveBeenCalled();
  });

  it("adds Third-party licences with the app commands", () => {
    const onOpenThirdPartyLicences = vi.fn();
    const commands = commandsFor(makeAppState(), { app: {
      onOpenThirdPartyLicences,
    } });
    const licences = commands.find((c) => c.label === "Third-party licences");
    expect(licences?.keywords).toEqual(expect.arrayContaining(["licenses", "about", "open source"]));
    licences!.run();
    expect(onOpenThirdPartyLicences).toHaveBeenCalled();
  });

  it("adds Open user guide with the app commands (#320)", () => {
    const onOpenUserGuide = vi.fn();
    const commands = commandsFor(makeAppState(), { app: {
      onOpenUserGuide,
    } });
    const guide = commands.find((c) => c.label === "Open user guide");
    expect(guide?.id).toBe("open-user-guide");
    expect(guide?.keywords).toEqual(expect.arrayContaining(["help", "documentation", "manual"]));
    expect(groupOf("open-user-guide")).toBe("General");
    guide!.run();
    expect(onOpenUserGuide).toHaveBeenCalled();
  });

  it("adds Reopen closed tab, grouped under Repositories, only when a closed tab can be reopened (#613)", () => {
    const onReopenClosedTab = vi.fn();
    const build = (appCommands: AppCommands) => buildCommands({ appState: makeAppState({ pending: true }), app: appCommands });
    expect(build({}).some((c) => c.id === "reopen-closed-tab")).toBe(false);
    // Offered while an operation runs: it opens another tab, it doesn't touch this one.
    const reopen = build({ onReopenClosedTab }).find((c) => c.id === "reopen-closed-tab");
    expect(reopen?.label).toBe("Reopen closed tab");
    expect(reopen?.group).toBe("Repositories");
    reopen!.run();
    expect(onReopenClosedTab).toHaveBeenCalledOnce();
  });

  it("adds Rename tab…, grouped under Repositories, also while an operation runs (#625)", () => {
    const onRenameTab = vi.fn();
    const build = (appCommands: AppCommands) => buildCommands({ appState: makeAppState({ pending: true }), app: appCommands });
    expect(build({}).some((c) => c.id === "rename-tab")).toBe(false);
    const rename = build({ onRenameTab }).find((c) => c.id === "rename-tab");
    expect(rename?.label).toBe("Rename tab…");
    expect(rename?.keywords).toEqual(expect.arrayContaining(["rename", "alias", "tab"]));
    expect(rename?.group).toBe("Repositories");
    rename!.run();
    expect(onRenameTab).toHaveBeenCalledOnce();
  });

  it("adds Switch to tab…, a picker of the other open tabs by name, folder name and path (#625)", () => {
    const onSwitchRepoTab = vi.fn();
    const otherRepos = [
      { path: "/work/one/app", displayName: "app (main)", alias: "app (main)", workspaceId: null },
      { path: "/work/two/app", displayName: "app", alias: null, workspaceId: null },
      { path: "/libs/core", displayName: "core", alias: null, workspaceId: "ws-1" },
    ];
    expect(commandsFor(makeAppState(), { tabs: tabs({ onSwitchRepoTab }) }).some((c) => c.id === "switch-to-tab")).toBe(false);

    const commands = commandsFor(makeAppState(), { tabs: tabs({ otherOpenRepos: otherRepos, onSwitchRepoTab }) });
    const switchTo = commands.find((c) => c.id === "switch-to-tab");
    expect(switchTo?.label).toBe("Switch to tab…");
    expect(switchTo?.group).toBe("Repositories");
    const items = (query: string) => switchTo!.picker!.items(query).map((item) => item.label);
    expect(items("")).toEqual(["app (main)", "app", "core"]);
    expect(items("main")).toEqual(["app (main)"]);
    // The renamed tab's folder name and the path find it too; an exact name comes first.
    expect(items("app")).toEqual(["app", "app (main)"]);
    expect(items("/work/two")).toEqual(["app"]);
    expect(items("libs")).toEqual(["core"]);

    switchTo!.picker!.items("core")[0].run();
    expect(onSwitchRepoTab).toHaveBeenCalledWith("/libs/core");
    // The one-step commands find a tab by its folder name and path as well.
    const renamed = commands.find((c) => c.id === "switch-repo:/work/one/app");
    expect(renamed?.label).toBe("Switch to app (main)");
    expect(renamed?.keywords).toEqual(expect.arrayContaining(["app", "/work/one/app"]));
  });

  it("adds tool commands, with current-file ones only when there is a current file", () => {
    const toolCommands = tools();
    const without = build(makeAppState(), null, toolCommands).map((c) => c.label);
    expect(without).toEqual(expect.arrayContaining(["Open terminal here", "Reveal repository"]));
    expect(without).not.toContain("Open current file in editor");
    expect(without).not.toContain("Open current file in diff tool");

    const commands = build(makeAppState(), "a.ts", toolCommands);
    for (const label of ["Open terminal here", "Reveal repository", "Open current file in editor", "Open current file in diff tool"]) {
      commands.find((c) => c.label === label)!.run();
    }
    expect(toolCommands.onOpenTerminal).toHaveBeenCalled();
    expect(toolCommands.onRevealRepository).toHaveBeenCalled();
    expect(toolCommands.onOpenCurrentFileInEditor).toHaveBeenCalled();
    expect(toolCommands.onOpenCurrentFileInDiffTool).toHaveBeenCalled();
    expect(groupOf("open-terminal")).toBe("Repositories");
    expect(groupOf("open-current-file-in-diff-tool")).toBe("Diff");
  });

  it("keeps tool commands while an operation is running", () => {
    const labels = build(makeAppState({ pending: true }), "a.ts", tools()).map((c) => c.label);
    expect(labels).toEqual(
      expect.arrayContaining(["Open terminal here", "Reveal repository", "Open current file in editor"]),
    );
  });
});

describe("remote and tag palette entries", () => {
  const remotes = [
    { name: "origin", fetchUrl: "a", pushUrl: null, authMode: null, authUsername: null },
    { name: "fork", fetchUrl: "b", pushUrl: null, authMode: null, authUsername: null },
  ];
  const allVisible = { stash: true, worktree: true, submodule: true, reflog: true, tags: true, pullRequests: true };

  it.each([
    ["edit-remote:fork", "Edit remote fork…", "edit"],
    ["manage-credentials:fork", "Manage credentials for fork…", "credentials"],
    ["remove-remote:fork", "Remove remote fork…", "remove"],
  ] as const)("%s (%s) opens the dialog for that remote", (id, label, kind) => {
    const appState = makeAppState({ remotes });
    const command = commandsFor(appState).find((c) => c.id === id);
    expect(command?.label).toBe(label);
    command?.run();
    expect(appState.openDialog).toHaveBeenCalledWith({ kind: "remote", action: kind, remoteName: "fork" });
    expect(command?.group).toBe("Remotes");
  });

  it("emits one entry per remote", () => {
    const ids = commandsFor(makeAppState({ remotes })).map((c) => c.id);
    expect(ids.filter((id) => id.startsWith("edit-remote:"))).toEqual(["edit-remote:origin", "edit-remote:fork"]);
    expect(ids.filter((id) => id.startsWith("manage-credentials:"))).toHaveLength(2);
    expect(ids.filter((id) => id.startsWith("remove-remote:"))).toHaveLength(2);
  });

  it("checks out the commit a tag peels to, even with the Tags panel hidden", () => {
    // An annotated tag of a tag targets the inner tag object; `commitId` is the peeled commit.
    const tags = [
      { name: "v1", targetId: "tagobj", commitId: "c1", annotated: true, message: "m", taggerName: "t", timestamp: 1 },
    ];
    const appState = makeAppState({ tags });
    const command = commandsFor(appState, { sidebar: sidebar({ ...allVisible, tags: false }), tabs: tabs({ onOpenRepoTab: vi.fn(), onSwitchRepoTab: vi.fn() }) }).find(
      (c) => c.id === "checkout-tag:v1",
    );
    expect(command?.label).toBe("Check out tag v1 (detached)");
    command?.run();
    expect(appState.checkoutCommit).toHaveBeenCalledWith("c1");
    expect(command?.group).toBe("Branches");
  });

  it("opens Delete from remote… for each tag while a remote exists", () => {
    const tags = [
      { name: "v1", targetId: "c1", commitId: "c1", annotated: false, message: null, taggerName: null, timestamp: null },
    ];
    const appState = makeAppState({ tags });
    const command = commandsFor(appState).find((c) => c.id === "delete-remote-tag:v1");
    expect(command?.label).toBe("Delete tag v1 from remote…");
    command?.run();
    expect(appState.openDialog).toHaveBeenCalledWith({ kind: "deleteRemoteTag", tagName: "v1" });

    const ids = commandsFor(makeAppState({ tags, remotes: [] })).map((c) => c.id);
    expect(ids).not.toContain("delete-remote-tag:v1");
    const busy = commandsFor(makeAppState({ tags, pending: true })).map((c) => c.id);
    expect(busy).not.toContain("delete-remote-tag:v1");
  });

  it("offers no checkout for a tag that doesn't peel to a commit", () => {
    const tags = [
      { name: "tree", targetId: "t1", commitId: null, annotated: true, message: "m", taggerName: "t", timestamp: 1 },
    ];
    const ids = commandsFor(makeAppState({ tags })).map((c) => c.id);
    expect(ids).not.toContain("checkout-tag:tree");
  });
});

describe("Go to pickers (#441)", () => {
  const commit = (id: string, summary: string, extra: Record<string, unknown> = {}) => ({
    id,
    shortId: id.slice(0, 7),
    summary,
    authorName: "a",
    authorEmail: "a@b",
    timestamp: 0,
    parentIds: [],
    branchRefs: [],
    remoteBranchRefs: [],
    ...extra,
  });
  const commits = [
    commit("abc1234000000000000000000000000000000000", "Fix the parser", { remoteBranchRefs: ["origin/main"] }),
    commit("def5678000000000000000000000000000000000", "Add a feature"),
  ];
  function goTo(): GoToCommands {
    return {
      onGoToCommit: vi.fn(),
      onSearchHistory: vi.fn(),
      onGoToBranch: vi.fn(),
      onGoToTag: vi.fn(),
      onGoToStash: vi.fn(),
    };
  }
  function pickers(state: Parameters<typeof makeAppState>[0] = {}, handlers = goTo()) {
    const commands = commandsFor(makeAppState({ commits, ...state }), { goTo: handlers });
    return { commands, handlers, picker: (id: string) => commands.find((c) => c.id === id) };
  }

  it("are offered under Go to, even while an operation runs, and only with something to go to", () => {
    const { picker } = pickers({ pending: true });
    for (const id of ["go-to-commit", "go-to-branch", "go-to-tag", "go-to-stash"]) {
      expect(picker(id)?.picker).toBeDefined();
      expect(groupOf(id)).toBe("Go to");
    }
    expect(picker("go-to-commit")?.label).toBe("Go to commit…");

    const empty = pickers({ tags: [], stashes: [] });
    expect(empty.picker("go-to-tag")).toBeUndefined();
    expect(empty.picker("go-to-stash")).toBeUndefined();
    expect(commandsFor(makeAppState()).some((c) => c.id === "go-to-commit")).toBe(false);
  });

  it("lists local branches, then remote ones the graph knows, and goes to the chosen one", () => {
    const { picker, handlers } = pickers({ graphSelection: { local: null, remote: ["upstream/dev"] } });
    const items = picker("go-to-branch")!.picker!.items("");
    expect(items.map((i) => i.label)).toEqual(["main (current)", "feature", "origin/main", "upstream/dev"]);

    const filtered = picker("go-to-branch")!.picker!.items("dev");
    expect(filtered.map((i) => i.label)).toEqual(["upstream/dev"]);
    filtered[0].run();
    items[1].run();
    expect(handlers.onGoToBranch).toHaveBeenCalledWith("remote", "upstream/dev");
    expect(handlers.onGoToBranch).toHaveBeenCalledWith("local", "feature");
  });

  it("offers Back and Forward only while there is somewhere to go (#618)", () => {
    expect(pickers().picker("go-back")).toBeUndefined();
    expect(pickers().picker("go-forward")).toBeUndefined();
    const handlers = { ...goTo(), onGoBack: vi.fn(), onGoForward: vi.fn() };
    const { picker } = pickers({ pending: true }, handlers);
    expect(picker("go-back")?.label).toBe("Back to previous commit");
    expect(picker("go-back")?.group).toBe("Go to");
    expect(picker("go-forward")?.group).toBe("Go to");
    picker("go-back")!.run();
    picker("go-forward")!.run();
    expect(handlers.onGoBack).toHaveBeenCalledOnce();
    expect(handlers.onGoForward).toHaveBeenCalledOnce();
  });

  it("goes to a tag's commit and to a stash", () => {
    const { picker, handlers } = pickers();
    picker("go-to-tag")!.picker!.items("v1")[0].run();
    expect(handlers.onGoToTag).toHaveBeenCalledWith("v1.0", "aaa");
    picker("go-to-stash")!.picker!.items("wip")[0].run();
    expect(handlers.onGoToStash).toHaveBeenCalledWith("ccc");
  });

  it("ranks a fully typed label first, ahead of a longer one that starts with it", () => {
    const { commands } = pickers();
    const labels = (query: string) => filterAndSortCommands(commands, query).map((c) => c.label);
    expect(labels("go to branch").slice(0, 2)).toEqual(["Go to branch…", "Go to Branches"]);
    expect(labels("Go to branches")[0]).toBe("Go to Branches");
    expect(labels("go to tag")[0]).toBe("Go to tag…");
  });

  describe("commitPickerItems", () => {
    it("lists the loaded commits newest first for an empty query", () => {
      const items = commitPickerItems(commits, "", goTo());
      expect(items.map((i) => i.label)).toEqual(["abc1234 Fix the parser", "def5678 Add a feature"]);
    });

    it("matches a SHA prefix or message text, then offers the commit search", () => {
      const handlers = goTo();
      const bySha = commitPickerItems(commits, "DEF56", handlers);
      expect(bySha.map((i) => i.label)).toEqual(["def5678 Add a feature", "Search all history for “DEF56”"]);
      bySha[0].run();
      expect(handlers.onGoToCommit).toHaveBeenCalledWith("def5678000000000000000000000000000000000");

      const byText = commitPickerItems(commits, "parser", handlers);
      expect(byText.map((i) => i.label)).toEqual([
        "abc1234 Fix the parser",
        "Look up “parser” in the whole repository",
        "Search all history for “parser”",
      ]);
      byText[2].run();
      expect(handlers.onSearchHistory).toHaveBeenCalledWith("parser");
    });

    it("offers a lookup first for a SHA or revision that isn't loaded, none for text with spaces", () => {
      const handlers = goTo();
      const items = commitPickerItems(commits, "v2.0~3", handlers);
      expect(items.map((i) => i.label)).toEqual([
        "Look up “v2.0~3” in the whole repository",
        "Search all history for “v2.0~3”",
      ]);
      items[0].run();
      expect(handlers.onGoToCommit).toHaveBeenCalledWith("v2.0~3");

      expect(commitPickerItems(commits, "two words", handlers).map((i) => i.id)).toEqual(["search-history"]);
    });
  });
});

// #509: the branch, remote, stash and worktree menus and the palette decide with the same rules
// (`lib/actions`), in every blocked state: a command is offered exactly where its item is enabled.
describe("agree with the branch, remote, stash and worktree menus (#509)", () => {
  const head = makeAppState().state.headInfo;
  const staged = [{ path: "a.txt", oldPath: null, staged: true, kind: "Modified" as const }];
  const conflicted = [{ path: "a.txt", oldPath: null, staged: false, kind: "Conflicted" as const }];
  const states: [string, Partial<AppState>][] = [
    ["nothing runs", {}],
    ["an action is pending", { pending: true }],
    ["a transfer runs", { transfer: { operationId: "op", operation: "Fetch", phase: "Receiving", errorKind: null, current: 1, total: 2, receivedBytes: 1, message: null, hookOutput: null } }],
    ["a merge is in progress", { mergeMessage: "Merge x", status: conflicted }],
    ["a rebase is in progress", { rebaseProgress: { currentStep: 1, totalSteps: 2 } }],
    ["a sequence is stored", { headInfo: { ...head, sequence: { kind: "CherryPick", currentStep: 1, totalSteps: 2, currentCommit: "c2" } } }],
    ["a terminal rebase is paused", { headInfo: { ...head, terminalRebase: { kind: "Rebase", branch: "topic", onto: "c2", currentStep: 1, totalSteps: 2 } } }],
    ["changes are staged", { status: staged }],
    ["no remote is configured", { remotes: [], upstream: null }],
  ];

  /** Opens a menu with `button` and returns its items' label → enabled, with each reason. */
  function menu(element: ReturnType<typeof createElement>, button: string, sectionTitle?: string) {
    localStorage.clear();
    render(element);
    try {
      if (sectionTitle !== undefined) fireEvent.click(screen.getByRole("button", { name: sectionTitle }));
      fireEvent.click(screen.getByRole("button", { name: button }));
      return new Map(
        screen.getAllByRole("menuitem").map((item) => [item.textContent!, { enabled: !item.hasAttribute("disabled"), title: item.getAttribute("title") }]),
      );
    } finally {
      cleanup();
    }
  }

  function branchTree(state: AppState) {
    const blockers = operationBlockers(state);
    const fn = () => vi.fn();
    return createElement(BranchTree, {
      branches: state.branches,
      createBranchDraft: null,
      onSwitchBranch: fn(),
      onCreateBranch: vi.fn().mockResolvedValue(null),
      onDeleteBranch: vi.fn().mockResolvedValue(undefined),
      onRenameBranch: fn(),
      onOpenCreateBranchDraft: fn(),
      onCloseCreateBranchDraft: fn(),
      onMergeBranch: fn(),
      operationDisabled: blockers.blocked,
      operationDisabledReason: blockers.reason,
      graphBranchSelection: null,
      onSetGraphBranchSelection: fn(),
      onClearGraphBranchSelection: fn(),
      onShowAllGraphBranches: fn(),
      graphRemoteBranchSelection: null,
      onSetGraphRemoteBranchSelection: fn(),
      onIsolateGraphBranch: fn(),
      remoteBranchesInGraph: [],
      remotes: state.remotes,
      upstream: state.upstream,
      onAddRemote: vi.fn().mockResolvedValue(null),
      onSetUpstream: vi.fn().mockResolvedValue(undefined),
      onClearUpstream: vi.fn().mockResolvedValue(undefined),
      onListRemoteBranches: vi.fn().mockResolvedValue([]),
      onFetchRemote: vi.fn().mockResolvedValue(undefined),
      onPushCurrentBranch: vi.fn().mockResolvedValue(undefined),
      onPull: vi.fn().mockResolvedValue(undefined),
      pendingPull: null,
      pullOutcome: null,
      onMergePull: vi.fn().mockResolvedValue(undefined),
      onRebasePull: fn(),
      onCancelPull: fn(),
      addRemoteDraftOpen: false,
      onOpenAddRemoteDraft: fn(),
      onCloseAddRemoteDraft: fn(),
      onOpenPushDialog: fn(),
      onOpenDeleteRemoteBranch: fn(),
      refreshGeneration: 0,
      onOpenRemoteDialog: fn(),
    });
  }

  /**
   * Each menu item agrees with its command, and a disabled one says why: `busy` (the rule that
   * blocks it, `operationBlockers`' reason by default) when that blocks, else some refusal.
   */
  function expectAgreement(
    items: Map<string, { enabled: boolean; title: string | null }>,
    ids: string[],
    pairs: [label: string, id: string, busy?: string | null][],
    state: AppState,
  ) {
    for (const [label, id, busy = operationBlockers(state).reason] of pairs) {
      const item = items.get(label);
      expect({ label, offered: ids.includes(id) }).toEqual({ label, offered: item?.enabled ?? false });
      if (item === undefined || item.enabled) continue;
      expect({ label, title: item.title }).toEqual({ label, title: busy ?? expect.any(String) });
    }
  }

  it.each(states)("while %s", (_label, overrides) => {
    const appState = makeAppState(overrides);
    const { state } = appState;
    const ids = commandsFor(appState, { tabs: tabs({ onOpenRepoTab: vi.fn() }) }).map((command) => command.id);

    expectAgreement(menu(branchTree(state), "Actions for feature"), ids, [
      ["Checkout", "switch-branch:feature"],
      ["Merge into current branch…", "merge-branch:feature"],
    ], state);
    expectAgreement(menu(branchTree(state), "Actions for main"), ids, [
      ["Checkout", "switch-branch:main"],
      ["Merge into current branch…", "merge-branch:main"],
      ["Push to…", "push-branch-to"],
    ], state);
    expectAgreement(menu(branchTree(state), "Add"), ids, [["Add remote…", "add-remote"]], state);
    if (state.remotes.length > 0) {
      expectAgreement(menu(branchTree(state), "Actions for remote origin"), ids, [
        ["Fetch", "fetch-remote:origin"],
        ["Push current branch here", "push-branch:origin"],
        ["Edit remote", "edit-remote:origin"],
        ["Manage credentials", "manage-credentials:origin"],
        ["Remove remote", "remove-remote:origin"],
      ], state);
    }

    const stashPanel = createElement(StashPanel, {
      stashes: state.stashes,
      onSelectRow: vi.fn(),
      onApplyStash: vi.fn(),
      onPopStash: vi.fn(),
      onDropStash: vi.fn(),
      restoreIndex: true,
      onSetRestoreIndex: vi.fn(),
      blockers: stashBlockers(state),
    });
    const stashRules = stashBlockers(state);
    expectAgreement(menu(stashPanel, "Actions for WIP", "Stashes"), ids, [
      ["Apply", "apply-stash:0", stashRules.apply],
      ["Pop", "pop-stash:0", stashRules.apply],
      ["Drop…", "drop-stash:0", stashRules.drop],
    ], state);

    // The worktree panel's buttons, for both worktrees (the main one can't be removed, #561).
    const blockers = operationBlockers(state);
    localStorage.clear();
    render(
      createElement(WorktreePanel, {
        worktrees: state.worktrees,
        branches: state.branches,
        onOpenWorktree: vi.fn(),
        onCreateWorktree: vi.fn(),
        onRemoveWorktree: vi.fn(),
        onPruneWorktrees: vi.fn(),
        operationDisabled: blockers.blocked,
        operationDisabledReason: blockers.reason,
      }),
    );
    try {
      fireEvent.click(screen.getByRole("button", { name: "Worktrees" }));
      for (const [label, id] of [
        ["Open /repo", "open-worktree:/repo"],
        ["Remove /repo", "remove-worktree:main"],
        ["Open /repo-wt1", "open-worktree:/repo-wt1"],
        ["Remove /repo-wt1", "remove-worktree:wt1"],
        ["Prune worktrees", "prune-worktrees"],
      ]) {
        const button = screen.getByRole("button", { name: label });
        expect({ label, offered: ids.includes(id) }).toEqual({ label, offered: !button.hasAttribute("disabled") });
        if (button.hasAttribute("disabled") && label !== "Remove /repo") expect(button).toHaveAttribute("title", blockers.reason);
      }
      expect(screen.getByRole("button", { name: "Remove /repo" })).toHaveAttribute(
        "title",
        blockers.reason ?? "The main worktree can't be removed.",
      );
    } finally {
      cleanup();
    }
  });
});

// #494: a state for each kind of command (an operation in progress, a clean working tree, ...),
// and the palette's commands for it with every family's handlers, as RepoWorkspace passes them.
const noop = () => {};
const REPRESENTATIVE_HEAD = { headId: "c1", detachedAt: null, headPushed: false, operation: null, operationLeftover: false, squashMessage: null, commitHooks: ["pre-commit"], sequence: null, rebase: null, bisect: null, terminalSequence: null, terminalRebase: null };
const representativeCommit = (id: string, parentIds: string[]) => ({ id, shortId: id, summary: `commit ${id}`, authorName: "T", authorEmail: "t@example.com", timestamp: 0, parentIds, branchRefs: [], remoteBranchRefs: [] });
const representativeState = (overrides: Partial<AppState> = {}) =>
  makeAppState({
    selectedRow: { commitId: "c2" },
    commits: [representativeCommit("c1", ["c2"]), representativeCommit("c2", ["c3"]), representativeCommit("c3", [])],
    headInfo: REPRESENTATIVE_HEAD,
    status: [
      { path: "a.txt", oldPath: null, staged: true, kind: "Modified" },
      { path: "b.txt", oldPath: null, staged: false, kind: "Modified" },
    ],
    undoState: { undo: "commit", redo: "checkout" },
    ...overrides,
  });
const STATES = {
  normal: () => representativeState(),
  rebase: () => representativeState({ rebaseProgress: { currentStep: 1, totalSteps: 2 } }),
  sequence: () => representativeState({ headInfo: { ...REPRESENTATIVE_HEAD, sequence: { kind: "CherryPick", currentStep: 1, totalSteps: 1 } } as AppState["headInfo"] }),
  merge: () => representativeState({ mergeMessage: "Merge x" }),
  clean: () => representativeState({ status: [], selectedRow: "uncommitted" }),
};
// Every family, as RepoWorkspace passes them.
const allFamilies = (appState: UseAppStateResult) =>
  buildCommands({
    appState,
    onShowShortcuts: noop,
    app: { onCloneRepository: noop, onNewRepository: noop, onOpenPreferences: noop, onOpenThirdPartyLicences: noop, onOpenUserGuide: noop },
    tabs: tabs({ otherOpenRepos: [{ path: "/other", displayName: "other" } as OpenRepo] }),
    sidebar: sidebar(),
    diff: {
      settings: { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true, wordWrap: "Auto", algorithm: "Default", showWhitespace: false },
      update: noop,
      currentFile: "b.txt",
      requestDiscardCurrentFile: noop,
      requestDiscardAllUnstaged: noop,
      requestDeleteUntracked: noop,
      openCurrentFileHistory: noop,
      selection: { staged: false, paths: ["b.txt", "c.txt"] },
      requestDiscardSelection: noop,
      currentDiff: { path: "b.txt", canShowWholeFile: true, toggleWrap: noop, toggleWholeFile: noop },
    },
    undo: { undoLabel: "commit", redoLabel: "checkout", onUndo: noop, onRedo: noop },
    search: { onSearchCommits: noop },
    tools: { onOpenTerminal: noop, onRevealRepository: noop, onOpenCurrentFileInEditor: noop, onOpenCurrentFileInDiffTool: noop },
    goTo: { onGoToCommit: noop, onSearchHistory: noop, onGoToBranch: noop, onGoToTag: noop, onGoToStash: noop },
    commit: { onCherryPick: noop, onRevert: noop, onChooseParent: noop, onReset: noop },
  });

/** The group of a command offered in one of the representative states. */
function groupOf(id: string): CommandGroup {
  for (const state of Object.values(STATES)) {
    const command = allFamilies(state()).find((candidate) => candidate.id === id);
    if (command !== undefined) return command.group;
  }
  throw new Error(`no representative state offers ${id}`);
}

// #494: the commands, ids and groups before the builders were split by family. Recents are stored
// by id, so a builder must keep the ids, and each group is now declared by the builder instead of
// derived from the id. #583 and #592 moved the remote, diff and tag commands that had fallen
// under "General" into their family's group. #621 added "stage-modified-files" since.
describe("command ids and groups", () => {
  const EXPECTED: Record<keyof typeof STATES, [string, CommandGroup][]> = {
    normal: [
      ["add-remote", "Remotes"],
      ["amend-last-commit", "General"],
      ["apply-patch", "Diff"],
      ["browse-remote-branches:origin", "Remotes"],
      ["checkout-selected", "General"],
      ["checkout-tag:v1.0", "Branches"],
      ["cherry-pick-selected", "General"],
      ["clone-repository", "General"],
      ["close-tab", "Repositories"],
      ["create-tag-head", "Branches"],
      ["create-tag-selected", "Branches"],
      ["decrease-context-lines", "Diff"],
      ["delete-remote-branch", "Remotes"],
      ["delete-remote-tag:v1.0", "Remotes"],
      ["delete-tag:v1.0", "Branches"],
      ["discard-all-unstaged", "Diff"],
      ["discard-current-file", "Diff"],
      ["discard-selected-files", "Diff"],
      ["drop-stash:0", "Stash"],
      ["edit-remote:origin", "Remotes"],
      ["fetch-all-remotes", "Remotes"],
      ["fetch-remote:origin", "Remotes"],
      ["file-history-current-file", "Diff"],
      ["force-push", "Remotes"],
      ["go-to-branch", "Go to"],
      ["go-to-commit", "Go to"],
      ["go-to-stash", "Go to"],
      ["go-to-tag", "Go to"],
      ["go-to:Branches", "Go to"],
      ["go-to:Pull Requests", "Go to"],
      ["go-to:Reflog", "Go to"],
      ["go-to:Stashes", "Go to"],
      ["go-to:Submodules", "Go to"],
      ["go-to:Tags", "Go to"],
      ["go-to:Worktrees", "Go to"],
      ["increase-context-lines", "Diff"],
      ["init-submodule:libs/a", "General"],
      ["manage-credentials:origin", "Remotes"],
      ["merge-branch:feature", "Branches"],
      ["new-repository", "General"],
      ["open-current-file-in-diff-tool", "Diff"],
      ["open-current-file-in-editor", "Diff"],
      ["open-submodule:libs/b", "General"],
      ["open-terminal", "Repositories"],
      ["open-user-guide", "General"],
      ["open-worktree:/repo", "General"],
      ["open-worktree:/repo-wt1", "General"],
      ["preferences", "General"],
      ["prune-worktrees", "General"],
      ["pull", "Remotes"],
      ["push-branch-to", "Remotes"],
      ["push-branch:origin", "Remotes"],
      ["push-tags:origin", "Remotes"],
      ["redo", "General"],
      ["refresh", "General"],
      ["remove-remote:origin", "Remotes"],
      ["remove-worktree:wt1", "General"],
      ["reset-selected", "General"],
      ["reveal-repository", "Repositories"],
      ["revert-selected", "General"],
      ["save-staged-patch", "Diff"],
      ["save-stash", "Stash"],
      ["search-commits", "General"],
      ["show-shortcuts", "General"],
      ["show-stash:0", "Stash"],
      ["stage-modified-files", "Diff"],
      ["stage-selected-files", "Diff"],
      ["stash-current-file", "Stash"],
      ["stash-selected-files", "Stash"],
      ["stash-with-options", "Stash"],
      ["switch-branch:feature", "Branches"],
      ["switch-repo:/other", "Repositories"],
      ["switch-to-tab", "Repositories"],
      ["third-party-licences", "General"],
      ["toggle-ignore-whitespace", "Diff"],
      ["toggle-show-whitespace", "Diff"],
      ["toggle-show-whole-file", "Diff"],
      ["toggle-skip-hooks", "General"],
      ["toggle-syntax-highlighting", "Diff"],
      ["toggle-wrap-lines", "Diff"],
      ["undo", "General"],
      ["update-submodule:libs/b", "General"],
    ],
    rebase: [
      ["add-remote", "Remotes"],
      ["clone-repository", "General"],
      ["create-tag-head", "Branches"],
      ["create-tag-selected", "Branches"],
      ["decrease-context-lines", "Diff"],
      ["discard-all-unstaged", "Diff"],
      ["discard-current-file", "Diff"],
      ["discard-selected-files", "Diff"],
      ["drop-stash:0", "Stash"],
      ["file-history-current-file", "Diff"],
      ["go-to-branch", "Go to"],
      ["go-to-commit", "Go to"],
      ["go-to-stash", "Go to"],
      ["go-to-tag", "Go to"],
      ["go-to:Branches", "Go to"],
      ["go-to:Pull Requests", "Go to"],
      ["go-to:Reflog", "Go to"],
      ["go-to:Stashes", "Go to"],
      ["go-to:Submodules", "Go to"],
      ["go-to:Tags", "Go to"],
      ["go-to:Worktrees", "Go to"],
      ["increase-context-lines", "Diff"],
      ["new-repository", "General"],
      ["open-current-file-in-diff-tool", "Diff"],
      ["open-current-file-in-editor", "Diff"],
      ["open-terminal", "Repositories"],
      ["open-user-guide", "General"],
      ["preferences", "General"],
      ["rebase-abort", "General"],
      ["rebase-continue", "General"],
      ["refresh", "General"],
      ["reveal-repository", "Repositories"],
      ["save-staged-patch", "Diff"],
      ["search-commits", "General"],
      ["show-shortcuts", "General"],
      ["show-stash:0", "Stash"],
      ["stage-modified-files", "Diff"],
      ["stage-selected-files", "Diff"],
      ["switch-repo:/other", "Repositories"],
      ["switch-to-tab", "Repositories"],
      ["third-party-licences", "General"],
      ["toggle-ignore-whitespace", "Diff"],
      ["toggle-show-whitespace", "Diff"],
      ["toggle-show-whole-file", "Diff"],
      ["toggle-skip-hooks", "General"],
      ["toggle-syntax-highlighting", "Diff"],
      ["toggle-wrap-lines", "Diff"],
    ],
    sequence: [
      ["abort-sequence", "General"],
      ["add-remote", "Remotes"],
      ["clone-repository", "General"],
      ["continue-sequence", "General"],
      ["create-tag-head", "Branches"],
      ["create-tag-selected", "Branches"],
      ["decrease-context-lines", "Diff"],
      ["discard-all-unstaged", "Diff"],
      ["discard-current-file", "Diff"],
      ["discard-selected-files", "Diff"],
      ["drop-stash:0", "Stash"],
      ["file-history-current-file", "Diff"],
      ["go-to-branch", "Go to"],
      ["go-to-commit", "Go to"],
      ["go-to-stash", "Go to"],
      ["go-to-tag", "Go to"],
      ["go-to:Branches", "Go to"],
      ["go-to:Pull Requests", "Go to"],
      ["go-to:Reflog", "Go to"],
      ["go-to:Stashes", "Go to"],
      ["go-to:Submodules", "Go to"],
      ["go-to:Tags", "Go to"],
      ["go-to:Worktrees", "Go to"],
      ["increase-context-lines", "Diff"],
      ["new-repository", "General"],
      ["open-current-file-in-diff-tool", "Diff"],
      ["open-current-file-in-editor", "Diff"],
      ["open-terminal", "Repositories"],
      ["open-user-guide", "General"],
      ["preferences", "General"],
      ["refresh", "General"],
      ["reveal-repository", "Repositories"],
      ["save-staged-patch", "Diff"],
      ["search-commits", "General"],
      ["show-shortcuts", "General"],
      ["show-stash:0", "Stash"],
      ["stage-modified-files", "Diff"],
      ["stage-selected-files", "Diff"],
      ["switch-repo:/other", "Repositories"],
      ["switch-to-tab", "Repositories"],
      ["third-party-licences", "General"],
      ["toggle-ignore-whitespace", "Diff"],
      ["toggle-show-whitespace", "Diff"],
      ["toggle-show-whole-file", "Diff"],
      ["toggle-skip-hooks", "General"],
      ["toggle-syntax-highlighting", "Diff"],
      ["toggle-wrap-lines", "Diff"],
    ],
    merge: [
      ["add-remote", "Remotes"],
      ["clone-repository", "General"],
      ["create-tag-head", "Branches"],
      ["create-tag-selected", "Branches"],
      ["decrease-context-lines", "Diff"],
      ["discard-all-unstaged", "Diff"],
      ["discard-current-file", "Diff"],
      ["discard-selected-files", "Diff"],
      ["drop-stash:0", "Stash"],
      ["file-history-current-file", "Diff"],
      ["go-to-branch", "Go to"],
      ["go-to-commit", "Go to"],
      ["go-to-stash", "Go to"],
      ["go-to-tag", "Go to"],
      ["go-to:Branches", "Go to"],
      ["go-to:Pull Requests", "Go to"],
      ["go-to:Reflog", "Go to"],
      ["go-to:Stashes", "Go to"],
      ["go-to:Submodules", "Go to"],
      ["go-to:Tags", "Go to"],
      ["go-to:Worktrees", "Go to"],
      ["increase-context-lines", "Diff"],
      ["merge-abort", "General"],
      ["new-repository", "General"],
      ["open-current-file-in-diff-tool", "Diff"],
      ["open-current-file-in-editor", "Diff"],
      ["open-terminal", "Repositories"],
      ["open-user-guide", "General"],
      ["preferences", "General"],
      ["refresh", "General"],
      ["reveal-repository", "Repositories"],
      ["save-staged-patch", "Diff"],
      ["save-stash", "Stash"],
      ["search-commits", "General"],
      ["show-shortcuts", "General"],
      ["show-stash:0", "Stash"],
      ["stage-modified-files", "Diff"],
      ["stage-selected-files", "Diff"],
      ["stash-current-file", "Stash"],
      ["stash-selected-files", "Stash"],
      ["stash-with-options", "Stash"],
      ["switch-repo:/other", "Repositories"],
      ["switch-to-tab", "Repositories"],
      ["third-party-licences", "General"],
      ["toggle-ignore-whitespace", "Diff"],
      ["toggle-show-whitespace", "Diff"],
      ["toggle-show-whole-file", "Diff"],
      ["toggle-skip-hooks", "General"],
      ["toggle-syntax-highlighting", "Diff"],
      ["toggle-wrap-lines", "Diff"],
    ],
    clean: [
      ["add-remote", "Remotes"],
      ["amend-last-commit", "General"],
      ["apply-patch", "Diff"],
      ["apply-stash:0", "Stash"],
      ["browse-remote-branches:origin", "Remotes"],
      ["checkout-tag:v1.0", "Branches"],
      ["clone-repository", "General"],
      ["close-tab", "Repositories"],
      ["create-tag-head", "Branches"],
      ["decrease-context-lines", "Diff"],
      ["delete-remote-branch", "Remotes"],
      ["delete-remote-tag:v1.0", "Remotes"],
      ["delete-tag:v1.0", "Branches"],
      ["discard-current-file", "Diff"],
      ["discard-selected-files", "Diff"],
      ["drop-stash:0", "Stash"],
      ["edit-remote:origin", "Remotes"],
      ["fetch-all-remotes", "Remotes"],
      ["fetch-remote:origin", "Remotes"],
      ["file-history-current-file", "Diff"],
      ["force-push", "Remotes"],
      ["go-to-branch", "Go to"],
      ["go-to-commit", "Go to"],
      ["go-to-stash", "Go to"],
      ["go-to-tag", "Go to"],
      ["go-to:Branches", "Go to"],
      ["go-to:Pull Requests", "Go to"],
      ["go-to:Reflog", "Go to"],
      ["go-to:Stashes", "Go to"],
      ["go-to:Submodules", "Go to"],
      ["go-to:Tags", "Go to"],
      ["go-to:Worktrees", "Go to"],
      ["increase-context-lines", "Diff"],
      ["init-submodule:libs/a", "General"],
      ["manage-credentials:origin", "Remotes"],
      ["merge-branch:feature", "Branches"],
      ["new-repository", "General"],
      ["open-current-file-in-diff-tool", "Diff"],
      ["open-current-file-in-editor", "Diff"],
      ["open-submodule:libs/b", "General"],
      ["open-terminal", "Repositories"],
      ["open-user-guide", "General"],
      ["open-worktree:/repo", "General"],
      ["open-worktree:/repo-wt1", "General"],
      ["pop-stash:0", "Stash"],
      ["preferences", "General"],
      ["prune-worktrees", "General"],
      ["pull", "Remotes"],
      ["push-branch-to", "Remotes"],
      ["push-branch:origin", "Remotes"],
      ["push-tags:origin", "Remotes"],
      ["redo", "General"],
      ["refresh", "General"],
      ["remove-remote:origin", "Remotes"],
      ["remove-worktree:wt1", "General"],
      ["reveal-repository", "Repositories"],
      ["search-commits", "General"],
      ["show-shortcuts", "General"],
      ["show-stash:0", "Stash"],
      ["stage-selected-files", "Diff"],
      ["switch-branch:feature", "Branches"],
      ["switch-repo:/other", "Repositories"],
      ["switch-to-tab", "Repositories"],
      ["third-party-licences", "General"],
      ["toggle-ignore-whitespace", "Diff"],
      ["toggle-show-whitespace", "Diff"],
      ["toggle-show-whole-file", "Diff"],
      ["toggle-skip-hooks", "General"],
      ["toggle-syntax-highlighting", "Diff"],
      ["toggle-wrap-lines", "Diff"],
      ["undo", "General"],
      ["update-submodule:libs/b", "General"],
    ],
  };

  it.each(Object.keys(STATES) as (keyof typeof STATES)[])("keeps every id and group (%s)", (name) => {
    const commands = allFamilies(STATES[name]());
    const actual = commands.map((command): [string, CommandGroup] => [command.id, command.group]);
    expect(actual.sort(([a], [b]) => a.localeCompare(b))).toEqual(EXPECTED[name].sort(([a], [b]) => a.localeCompare(b)));
    expect(new Set(commands.map((command) => command.id)).size).toBe(commands.length);
  });
});
