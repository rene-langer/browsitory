import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import {
  buildCommands,
  commandGroup,
  filterAndSortCommands,
  loadRecentCommandIds,
  recordCommandUsed,
  type Command,
} from "./commands";
import type { AppState, UseAppStateResult } from "../state/useAppState";

function makeAppState(overrides: Partial<UseAppStateResult["state"]> = {}): UseAppStateResult {
  return {
    state: {
      repoPath: "/repo",
      selectedRow: "uncommitted",
      status: [],
      commits: [],
      graphLimit: 300,
      hasMoreHistory: false,
      graphBranchSelection: null,
      graphRemoteBranchSelection: null,
      branches: [
        { name: "main", isCurrent: true },
        { name: "feature", isCurrent: false },
      ],
      worktrees: [
        { name: "main", path: "/repo", head: "abc", isMain: true, isLocked: false, isPrunable: false },
        { name: "wt1", path: "/repo-wt1", head: "def", isMain: false, isLocked: false, isPrunable: false },
      ],
      submodules: [
        { path: "libs/a", url: "https://example.com/a.git", gitlinkId: "aaa", initialized: false, headId: null },
        { path: "libs/b", url: "https://example.com/b.git", gitlinkId: "bbb", initialized: true, headId: "bbb111" },
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
      tags: [{ name: "v1.0", targetId: "aaa", annotated: false, message: null, taggerName: null, timestamp: null }],
      upstream: { localBranch: "main", remoteName: "origin", remoteBranch: "main" },
      remoteUpstreams: {},
      forgeRepositories: [],
      pullRequests: {},
      createBranchDraft: null,
      stashes: [{ index: 0, message: "WIP", commitId: "ccc" }],
      mergeMessage: null,
      forcePushOffer: null,
      forcePushConfirm: null,
      fetchPrune: null,
      pushDialog: null,
      deleteRemoteBranchDialog: null,
      headInfo: { headId: null, detachedAt: null, headPushed: false, operation: null, squashMessage: null, commitHooks: [] },
      amendMode: false,
      skipHooks: false,
      hookRun: null,
      hookFailure: null,
      clearCommitMessage: null,
      rebaseProgress: null,
      rebaseOnto: null,
      rebasePreset: null,
      pendingPull: null,
      pullOutcome: null,
      mergeDialog: null,
      mergeOutcome: null,
      stashDialog: null,
      stashRestoreIndex: true,
      stashOutcome: null,
      transfer: null,
      error: null,
      pending: false,
      addRemoteDraftOpen: false,
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
    discardAllUnstaged: vi.fn(),
    ignorePath: vi.fn(),
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
    deleteTag: vi.fn(),
    pushCurrentBranch: vi.fn(),
    pushTags: vi.fn(),
    pullCurrentUpstream: vi.fn(),
    clearPendingPull: vi.fn(),
    openCreateBranchDraft: vi.fn(),
    closeCreateBranchDraft: vi.fn(),
    openAddRemoteDraft: vi.fn(),
    closeAddRemoteDraft: vi.fn(),
    requestForcePush: vi.fn(),
    cherryPick: vi.fn(),
    revertCommit: vi.fn(),
    resetToCommit: vi.fn(),
    checkoutCommit: vi.fn(),
    amendCommit: vi.fn(),
    setAmendMode: vi.fn(),
    cancelForcePush: vi.fn(),
    forcePushCurrentBranch: vi.fn(),
    fetchAllRemotes: vi.fn(),
    setFetchPrune: vi.fn(),
    pushBranch: vi.fn(),
    openPushDialog: vi.fn(),
    closePushDialog: vi.fn(),
    deleteRemoteBranch: vi.fn(),
    openDeleteRemoteBranchDialog: vi.fn(),
    closeDeleteRemoteBranchDialog: vi.fn(),
    saveStash: vi.fn(),
    openStashDialog: vi.fn(),
    closeStashDialog: vi.fn(),
    applyStash: vi.fn(),
    popStash: vi.fn(),
    setStashRestoreIndex: vi.fn(),
    dropStash: vi.fn(),
    openMergeDialog: vi.fn(),
    closeMergeDialog: vi.fn(),
    getMergeOptions: vi.fn(),
    mergeBranch: vi.fn(),
    mergePull: vi.fn(),
    resolveConflict: vi.fn(),
    resolveAddDeleteConflict: vi.fn(),
    markResolved: vi.fn(),
    abortMerge: vi.fn(),
    openRebasePlanner: vi.fn(),
    openRebaseSelection: vi.fn(),
    closeRebasePlanner: vi.fn(),
    startRebase: vi.fn(),
    rebaseContinue: vi.fn(),
    abortRebase: vi.fn(),
    listPullRequests: vi.fn(),
    saveForgeToken: vi.fn(),
    forgetForgeToken: vi.fn(),
    createPullRequest: vi.fn(),
    openExternalUrl: vi.fn(),
    setGraphBranchSelection: vi.fn(),
    setGraphRemoteBranchSelection: vi.fn(),
    loadMoreHistory: vi.fn(),
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
  };
}

describe("buildCommands", () => {
  it("offers to toggle skip hooks only when commit hooks are installed, even while busy", () => {
    const withHooks = makeAppState({
      headInfo: { headId: "abc", detachedAt: null, headPushed: false, operation: null, squashMessage: null, commitHooks: ["pre-commit"] },
      skipHooks: false,
      pending: true,
    });
    const command = buildCommands(withHooks).find((c) => c.id === "toggle-skip-hooks");
    expect(command?.label).toBe("Skip hooks for the next commit");
    expect(command?.keywords).toEqual(expect.arrayContaining(["no-verify", "pre-commit"]));
    command?.run();
    expect(withHooks.selectRow).toHaveBeenCalledWith("uncommitted");
    expect(withHooks.setSkipHooks).toHaveBeenCalledWith(true);

    const on = makeAppState({
      headInfo: { headId: "abc", detachedAt: null, headPushed: false, operation: null, squashMessage: null, commitHooks: ["commit-msg"] },
      skipHooks: true,
    });
    const toggleOff = buildCommands(on).find((c) => c.id === "toggle-skip-hooks");
    expect(toggleOff?.label).toBe("Run hooks for the next commit");
    toggleOff?.run();
    expect(on.setSkipHooks).toHaveBeenCalledWith(false);

    expect(buildCommands(makeAppState()).some((c) => c.id === "toggle-skip-hooks")).toBe(false);
  });

  it("offers Fetch all remotes, Push branch to… and Delete remote branch… when a remote exists", () => {
    const appState = makeAppState();
    const commands = buildCommands(appState);
    const fetchAll = commands.find((command) => command.label === "Fetch all remotes")!;
    expect(fetchAll.keywords).toEqual(expect.arrayContaining(["fetch", "all", "remotes", "prune"]));
    fetchAll.run();
    commands.find((command) => command.label === "Push branch to…")!.run();
    commands.find((command) => command.label === "Delete remote branch…")!.run();
    expect(appState.fetchAllRemotes).toHaveBeenCalled();
    expect(appState.openPushDialog).toHaveBeenCalledWith("main");
    expect(appState.openDeleteRemoteBranchDialog).toHaveBeenCalledWith(null, null);
  });

  it("hides the new remote commands without a remote, and Push branch to… on a detached HEAD", () => {
    const labels = (overrides: Parameters<typeof makeAppState>[0]) =>
      buildCommands(makeAppState(overrides)).map((command) => command.label);
    expect(labels({ remotes: [] })).not.toContain("Fetch all remotes");
    expect(labels({ remotes: [] })).not.toContain("Push branch to…");
    expect(labels({ remotes: [] })).not.toContain("Delete remote branch…");
    const detached = labels({
      branches: [],
      headInfo: { headId: "abc", detachedAt: "abc1234", headPushed: false, operation: null, squashMessage: null, commitHooks: [] },
    });
    expect(detached).not.toContain("Push branch to…");
    expect(detached).toContain("Fetch all remotes");
  });

  it("hides the new remote commands while a repository operation runs", () => {
    const labels = buildCommands(makeAppState({ pending: true })).map((command) => command.label);
    expect(labels).not.toContain("Fetch all remotes");
    expect(labels).not.toContain("Push branch to…");
    expect(labels).not.toContain("Delete remote branch…");
  });

  it("includes a shortcuts command only when a handler is supplied", () => {
    const onShow = vi.fn();
    expect(buildCommands(makeAppState()).some((c) => c.id === "show-shortcuts")).toBe(false);
    const cmd = buildCommands(makeAppState(), undefined, undefined, undefined, undefined, onShow).find(
      (c) => c.id === "show-shortcuts",
    );
    expect(cmd?.label).toBe("Show keyboard shortcuts");
    cmd?.run();
    expect(onShow).toHaveBeenCalled();
  });

  it("offers Search commits when search commands are given", () => {
    const onSearchCommits = vi.fn();
    expect(buildCommands(makeAppState()).some((c) => c.id === "search-commits")).toBe(false);
    const cmd = buildCommands(
      makeAppState(),
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      undefined,
      { onSearchCommits },
    ).find((c) => c.id === "search-commits");
    expect(cmd?.label).toBe("Search commits");
    cmd?.run();
    expect(onSearchCommits).toHaveBeenCalled();
  });

  it("includes a Close tab command, grouped under Repositories, only when a close handler is supplied", () => {
    const onClose = vi.fn();
    expect(buildCommands(makeAppState()).some((c) => c.id === "close-tab")).toBe(false);
    const cmd = buildCommands(makeAppState(), undefined, undefined, undefined, undefined, undefined, onClose).find(
      (c) => c.id === "close-tab",
    );
    expect(cmd?.label).toBe("Close tab");
    expect(commandGroup("close-tab")).toBe("Repositories");
    cmd?.run();
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("omits Close tab while a repository operation is in progress", () => {
    const commands = buildCommands(makeAppState({ pending: true }), undefined, undefined, undefined, undefined, undefined, vi.fn());
    expect(commands.some((c) => c.id === "close-tab")).toBe(false);
  });

  it("adds clone and new-repository commands when app commands are given", () => {
    const onCloneRepository = vi.fn();
    const onNewRepository = vi.fn();
    const commands = buildCommands(makeAppState(), undefined, [], undefined, undefined, undefined, undefined, undefined, {
      onCloneRepository,
      onNewRepository,
    });
    commands.find((c) => c.label === "Clone repository…")!.run();
    commands.find((c) => c.label === "New repository…")!.run();
    expect(onCloneRepository).toHaveBeenCalled();
    expect(onNewRepository).toHaveBeenCalled();
  });

  it("omits clone and new-repository commands without app commands", () => {
    const labels = buildCommands(makeAppState()).map((c) => c.label);
    expect(labels).not.toContain("Clone repository…");
  });

  it("includes one switch-branch command per non-current branch", () => {
    const appState = makeAppState();
    const commands = buildCommands(appState);
    const switchCommands = commands.filter((c) => c.id.startsWith("switch-branch:"));
    expect(switchCommands).toHaveLength(1);
    expect(switchCommands[0].label).toBe("Switch to feature");
    switchCommands[0].run();
    expect(appState.switchBranch).toHaveBeenCalledWith("feature");
  });

  it("includes fetch/push/push-tags commands per remote", () => {
    const appState = makeAppState();
    const commands = buildCommands(appState);
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
    function mountBranchesWithRemoteFolder(remoteName: string): HTMLButtonElement {
      const workspace = document.createElement("div");
      workspace.setAttribute("data-active-repo", "true");
      document.body.appendChild(workspace);

      const section = document.createElement("section");
      section.setAttribute("aria-label", "Branches");
      workspace.appendChild(section);

      const sectionToggle = document.createElement("button");
      sectionToggle.setAttribute("aria-expanded", "true");
      sectionToggle.textContent = "Branches";
      section.appendChild(sectionToggle);

      const remoteToggle = document.createElement("button");
      remoteToggle.setAttribute("aria-expanded", "false");
      remoteToggle.textContent = remoteName;
      remoteToggle.scrollIntoView = vi.fn();
      remoteToggle.addEventListener("click", () => remoteToggle.setAttribute("aria-expanded", "true"));
      section.appendChild(remoteToggle);

      return remoteToggle;
    }

    afterEach(() => {
      document.body.innerHTML = "";
    });

    it("includes one browse-remote-branches command per remote", () => {
      const commands = buildCommands(makeAppState());
      const command = commands.find((c) => c.id === "browse-remote-branches:origin");
      expect(command?.label).toBe("Browse branches on origin");
    });

    it("expands the named remote's folder instead of mutating anything", () => {
      const remoteToggle = mountBranchesWithRemoteFolder("origin");
      const commands = buildCommands(makeAppState());
      commands.find((c) => c.id === "browse-remote-branches:origin")?.run();
      expect(remoteToggle.getAttribute("aria-expanded")).toBe("true");
    });

    it("is omitted while a repository operation is in progress", () => {
      const commands = buildCommands(makeAppState({ pending: true }));
      expect(commands.some((c) => c.id.startsWith("browse-remote-branches:"))).toBe(false);
    });
  });

  it("only includes pull when an upstream is set", () => {
    const withUpstream = buildCommands(makeAppState());
    expect(withUpstream.some((c) => c.id === "pull")).toBe(true);

    const withoutUpstream = buildCommands(makeAppState({ upstream: null }));
    expect(withoutUpstream.some((c) => c.id === "pull")).toBe(false);
  });

  it("only includes rebase continue/abort while a rebase is in progress", () => {
    const noRebase = buildCommands(makeAppState());
    expect(noRebase.some((c) => c.id === "rebase-continue")).toBe(false);

    const withRebase = buildCommands(makeAppState({ rebaseProgress: { currentStep: 1, totalSteps: 3 } }));
    expect(withRebase.some((c) => c.id === "rebase-continue")).toBe(true);
    expect(withRebase.some((c) => c.id === "rebase-abort")).toBe(true);
  });

  it("only includes abort merge while a merge is in progress", () => {
    const noMerge = buildCommands(makeAppState());
    expect(noMerge.some((c) => c.id === "merge-abort")).toBe(false);

    const withMerge = buildCommands(makeAppState({ mergeMessage: "Merge branch" }));
    expect(withMerge.some((c) => c.id === "merge-abort")).toBe(true);
  });

  it("includes apply/drop per stash", () => {
    const appState = makeAppState();
    const commands = buildCommands(appState);
    const applyCmd = commands.find((c) => c.id === "apply-stash:0");
    expect(applyCmd?.label).toBe("Apply stash: WIP");
    applyCmd?.run();
    expect(appState.applyStash).toHaveBeenCalledWith(0);

    expect(commands.some((c) => c.id === "drop-stash:0")).toBe(true);
  });

  it("offers merge-branch per non-current local branch and omits it while an operation runs", () => {
    const appState = makeAppState();
    const commands = buildCommands(appState);
    const merge = commands.find((c) => c.id === "merge-branch:feature");
    expect(merge?.label).toBe("Merge feature into current branch…");
    expect(commands.some((c) => c.id === "merge-branch:main")).toBe(false);
    merge?.run();
    expect(appState.openMergeDialog).toHaveBeenCalledWith("feature");

    const merging = buildCommands(makeAppState({ mergeMessage: "Merge branch" }));
    expect(merging.some((c) => c.id.startsWith("merge-branch:"))).toBe(false);
  });

  it("offers stash all and stash with options", () => {
    const appState = makeAppState();
    const commands = buildCommands(appState);
    const all = commands.find((c) => c.id === "save-stash");
    expect(all?.label).toBe("Stash all changes");
    all?.run();
    expect(appState.saveStash).toHaveBeenCalledWith();
    const withOptions = commands.find((c) => c.id === "stash-with-options");
    expect(withOptions?.label).toBe("Stash…");
    withOptions?.run();
    expect(appState.openStashDialog).toHaveBeenCalledWith([]);
  });

  it("offers show and pop per stash", () => {
    const appState = makeAppState();
    const commands = buildCommands(appState);
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
    expect(commandGroup("merge-branch:x")).toBe("Branches");
    for (const id of ["stash-with-options", "stash-current-file", "show-stash:0", "pop-stash:0"]) {
      expect(commandGroup(id)).toBe("Stash");
    }
  });

  it("excludes the main worktree from open/remove commands", () => {
    const appState = makeAppState();
    const commands = buildCommands(appState);
    expect(commands.some((c) => c.id === "open-worktree:/repo")).toBe(false);
    expect(commands.some((c) => c.id === "remove-worktree:main")).toBe(false);

    const openCmd = commands.find((c) => c.id === "open-worktree:/repo-wt1");
    expect(openCmd?.label).toBe("Open worktree wt1");
  });

  it("opening a worktree calls onOpenRepoTab instead of a state mutation", () => {
    const appState = makeAppState({
      worktrees: [{ name: "feature", path: "/repos/feature-wt", head: "abc123", isMain: false, isLocked: false, isPrunable: false }],
    });
    const onOpenRepoTab = vi.fn();
    const commands = buildCommands(appState, onOpenRepoTab, [], vi.fn());

    const openCommand = commands.find((c) => c.id === "open-worktree:/repos/feature-wt");
    openCommand?.run();
    expect(onOpenRepoTab).toHaveBeenCalledWith("/repos/feature-wt");
  });

  it("includes a Switch to <repo> command for every other open repo", () => {
    const appState = makeAppState();
    const otherRepos = [
      { path: "/repos/widget", displayName: "widget", workspaceId: null },
      { path: "/repos/gadget", displayName: "gadget", workspaceId: null },
    ];
    const onSwitchRepoTab = vi.fn();
    const commands = buildCommands(appState, vi.fn(), otherRepos, onSwitchRepoTab);

    const widgetCommand = commands.find((c) => c.id === "switch-repo:/repos/widget");
    expect(widgetCommand?.label).toBe("Switch to widget");
    widgetCommand?.run();
    expect(onSwitchRepoTab).toHaveBeenCalledWith("/repos/widget");
  });

  it("splits submodule commands by initialized state", () => {
    const commands = buildCommands(makeAppState());
    const initCmd = commands.find((c) => c.id === "init-submodule:libs/a");
    expect(initCmd?.label).toBe("Initialize submodule libs/a");
    initCmd?.run();

    const updateCmd = commands.find((c) => c.id === "update-submodule:libs/b");
    expect(updateCmd?.label).toBe("Update submodule libs/b");
    updateCmd?.run();

    expect(commands.some((c) => c.id === "init-submodule:libs/b")).toBe(false);
    expect(commands.some((c) => c.id === "update-submodule:libs/a")).toBe(false);
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
    const commands = buildCommands(makeAppState({ reflog: manyEntries }));
    expect(commands.some((c) => c.id.startsWith("restore-reflog:"))).toBe(false);
    expect(commands.filter((c) => c.id === "go-to:Reflog")).toHaveLength(1);
  });

  describe("destructive commands navigate instead of mutating directly", () => {
    // Mirrors one tab's `RepoWorkspace`: a wrapper div carrying `data-active-repo`, holding the
    // sidebar section. Every open tab stays mounted at once, so the wrapper (not the document)
    // is what `goToSidebarSection` scopes its lookup to.
    function mountWorkspace(active: boolean): HTMLDivElement {
      const workspace = document.createElement("div");
      workspace.setAttribute("data-active-repo", active ? "true" : "false");
      document.body.appendChild(workspace);
      return workspace;
    }

    function mountSectionIn(workspace: HTMLElement, title: string): HTMLButtonElement {
      const section = document.createElement("section");
      section.setAttribute("aria-label", title);
      // jsdom doesn't implement scrollIntoView; goToSidebarSection calls it
      // unconditionally after expanding, so stub it out.
      section.scrollIntoView = vi.fn();
      const button = document.createElement("button");
      button.setAttribute("aria-expanded", "false");
      // Mirror AccordionSection's real toggle behavior so we can assert on
      // the attribute goToSidebarSection reads and flips via button.click().
      button.addEventListener("click", () => {
        button.setAttribute("aria-expanded", "true");
      });
      section.appendChild(button);
      workspace.appendChild(section);
      return button;
    }

    function mountSection(title: string): HTMLButtonElement {
      return mountSectionIn(mountWorkspace(true), title);
    }

    afterEach(() => {
      document.body.innerHTML = "";
    });

    it("delete-tag expands the Tags section instead of calling deleteTag", () => {
      const button = mountSection("Tags");
      const appState = makeAppState();
      const commands = buildCommands(appState);
      const deleteTagCmd = commands.find((c) => c.id === "delete-tag:v1.0");
      expect(deleteTagCmd).toBeDefined();
      deleteTagCmd?.run();
      expect(button.getAttribute("aria-expanded")).toBe("true");
      expect(appState.deleteTag).not.toHaveBeenCalled();
    });

    it("drop-stash selects the stash and expands the Stashes section instead of dropping", () => {
      const button = mountSection("Stashes");
      const appState = makeAppState();
      const commands = buildCommands(appState);
      const dropCmd = commands.find((c) => c.id === "drop-stash:0");
      expect(dropCmd?.label).toBe("Drop stash: WIP");
      dropCmd?.run();
      expect(appState.dropStash).not.toHaveBeenCalled();
      expect(appState.selectRow).toHaveBeenCalledWith({ commitId: "ccc" });
      expect(button.getAttribute("aria-expanded")).toBe("true");
    });

    it("remove-worktree expands the Worktrees section instead of calling removeWorktree", () => {
      const button = mountSection("Worktrees");
      const appState = makeAppState();
      const commands = buildCommands(appState);
      const removeWorktreeCmd = commands.find((c) => c.id === "remove-worktree:wt1");
      expect(removeWorktreeCmd).toBeDefined();
      removeWorktreeCmd?.run();
      expect(button.getAttribute("aria-expanded")).toBe("true");
      expect(appState.removeWorktree).not.toHaveBeenCalled();
    });

    it("go-to:Reflog (the only reflog affordance left) expands the Reflog section instead of calling restoreReflogEntry", () => {
      const button = mountSection("Reflog");
      const appState = makeAppState();
      const commands = buildCommands(appState);
      const goToReflogCmd = commands.find((c) => c.id === "go-to:Reflog");
      expect(goToReflogCmd).toBeDefined();
      goToReflogCmd?.run();
      expect(button.getAttribute("aria-expanded")).toBe("true");
      expect(appState.restoreReflogEntry).not.toHaveBeenCalled();
    });

    it("targets the active tab's section, not the first tab's, when the active tab isn't first", () => {
      // Both tabs are mounted at once and both carry a "Reflog" section; only the second is
      // active. A document-wide querySelector would match the first (hidden) tab's button.
      const backgroundButton = mountSectionIn(mountWorkspace(false), "Reflog");
      const activeButton = mountSectionIn(mountWorkspace(true), "Reflog");

      buildCommands(makeAppState()).find((c) => c.id === "go-to:Reflog")?.run();

      expect(activeButton.getAttribute("aria-expanded")).toBe("true");
      expect(backgroundButton.getAttribute("aria-expanded")).toBe("false");
    });

    it("does nothing when no workspace is active, rather than expanding a hidden tab's section", () => {
      const backgroundButton = mountSectionIn(mountWorkspace(false), "Reflog");

      buildCommands(makeAppState()).find((c) => c.id === "go-to:Reflog")?.run();

      expect(backgroundButton.getAttribute("aria-expanded")).toBe("false");
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
        const commands = buildCommands(makeAppState(overrides));
        const ids = commands.map((c) => c.id);

        // Mutating families must be entirely absent, not merely disabled.
        expect(ids.some((id) => id.startsWith("switch-branch:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("fetch-remote:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("push-branch:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("push-tags:"))).toBe(false);
        expect(ids).not.toContain("pull");
        expect(ids.some((id) => id.startsWith("delete-tag:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("open-worktree:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("remove-worktree:"))).toBe(false);
        expect(ids).not.toContain("prune-worktrees");
        expect(ids.some((id) => id.startsWith("init-submodule:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("update-submodule:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("apply-stash:"))).toBe(false);
        expect(ids.some((id) => id.startsWith("drop-stash:"))).toBe(false);

        // Escape hatches and pure navigation stay available in every guarded state.
        expect(ids).toContain("refresh");
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
      const commands = buildCommands(makeAppState({ mergeMessage: "Merge branch" }));
      expect(commands.some((c) => c.id === "merge-abort")).toBe(true);
    });

    it("keeps rebase-continue/rebase-abort available while a rebase is in progress", () => {
      const commands = buildCommands(
        makeAppState({ rebaseProgress: { currentStep: 1, totalSteps: 3 } }),
      );
      expect(commands.some((c) => c.id === "rebase-continue")).toBe(true);
      expect(commands.some((c) => c.id === "rebase-abort")).toBe(true);
    });

    it("keeps save-stash unconditional, matching DiffPane's real button (no full guard)", () => {
      const commands = buildCommands(makeAppState({ pending: true }));
      expect(commands.some((c) => c.id === "save-stash")).toBe(true);
    });
  });

  it("always includes exactly one go-to command per sidebar section", () => {
    const commands = buildCommands(makeAppState());
    const goToCommands = commands.filter((c) => c.id.startsWith("go-to:"));
    expect(goToCommands.map((c) => c.label).sort()).toEqual(
      [
        "Go to Branches",
        "Go to Pull Requests",
        "Go to Reflog",
        "Go to Stashes",
        "Go to Submodules",
        "Go to Tags",
        "Go to Worktrees",
      ].sort(),
    );
  });

  it("no longer emits a separate 'Go to Remotes' command (folded into Branches)", () => {
    const commands = buildCommands(makeAppState());
    expect(commands.find((c) => c.id === "go-to:Remotes")).toBeUndefined();
    expect(commands.find((c) => c.id === "go-to:Branches")).toBeDefined();
  });

  describe("hidden sidebar panels", () => {
    // The sidebar panel-visibility toggles (a separate, already-merged task) fully unmount a
    // hidden panel rather than just CSS-hiding it. Without this filtering, every command that
    // navigates to or names something in a hidden panel became a dead no-op: `goToSidebarSection`
    // resolves the section by `aria-label` in the DOM and silently returns when nothing matches.
    const allVisible = { stash: true, worktree: true, submodule: true, reflog: true, tags: true, pullRequests: true };

    it("omits 'Go to <Panel>' for a panel whose visibility is false", () => {
      const commands = buildCommands(makeAppState(), vi.fn(), [], vi.fn(), { ...allVisible, tags: false });
      expect(commands.some((c) => c.id === "go-to:Tags")).toBe(false);
      // Unaffected sections keep their command.
      expect(commands.some((c) => c.id === "go-to:Stashes")).toBe(true);
    });

    it("keeps 'Go to Branches' even though Branches has no visibility toggle", () => {
      const commands = buildCommands(makeAppState(), vi.fn(), [], vi.fn(), { ...allVisible, tags: false });
      expect(commands.some((c) => c.id === "go-to:Branches")).toBe(true);
    });

    it("omits per-tag delete commands while the Tags panel is hidden", () => {
      const commands = buildCommands(makeAppState(), vi.fn(), [], vi.fn(), { ...allVisible, tags: false });
      expect(commands.some((c) => c.id.startsWith("delete-tag:"))).toBe(false);
    });

    it("omits per-stash apply/drop commands while the Stashes panel is hidden", () => {
      const commands = buildCommands(makeAppState(), vi.fn(), [], vi.fn(), { ...allVisible, stash: false });
      expect(commands.some((c) => c.id.startsWith("apply-stash:"))).toBe(false);
      expect(commands.some((c) => c.id.startsWith("drop-stash:"))).toBe(false);
    });

    it("omits per-worktree commands while the Worktrees panel is hidden", () => {
      const commands = buildCommands(makeAppState(), vi.fn(), [], vi.fn(), { ...allVisible, worktree: false });
      expect(commands.some((c) => c.id.startsWith("open-worktree:"))).toBe(false);
      expect(commands.some((c) => c.id.startsWith("remove-worktree:"))).toBe(false);
    });

    it("omits per-submodule commands while the Submodules panel is hidden", () => {
      const commands = buildCommands(makeAppState(), vi.fn(), [], vi.fn(), { ...allVisible, submodule: false });
      expect(commands.some((c) => c.id.startsWith("init-submodule:"))).toBe(false);
      expect(commands.some((c) => c.id.startsWith("update-submodule:"))).toBe(false);
    });

    it("defaults to every panel visible when no panelVisibility argument is passed", () => {
      const commands = buildCommands(makeAppState());
      expect(commands.some((c) => c.id === "go-to:Tags")).toBe(true);
      expect(commands.some((c) => c.id.startsWith("delete-tag:"))).toBe(true);
    });
  });

  it("emits an Add remote command that opens the add-remote draft", () => {
    const appState = makeAppState();
    const commands = buildCommands(appState);
    const addRemote = commands.find((c) => c.id === "add-remote");
    expect(addRemote).toBeDefined();
    addRemote?.run();
    expect(appState.openAddRemoteDraft).toHaveBeenCalledOnce();
  });

  it("omits Add remote while a repository operation is in progress", () => {
    const appState = makeAppState({ pending: true });
    const commands = buildCommands(appState);
    expect(commands.find((c) => c.id === "add-remote")).toBeUndefined();
  });
});

describe("filterAndSortCommands", () => {
  const commands: Command[] = [
    { id: "a", label: "Switch to main", keywords: ["branch", "switch", "main"], run: vi.fn() },
    { id: "b", label: "Switch to feature", keywords: ["branch", "switch", "feature"], run: vi.fn() },
    { id: "c", label: "Fetch origin", keywords: ["fetch", "remote", "origin"], run: vi.fn() },
  ];

  it("returns everything, unranked-but-stable, for an empty query", () => {
    expect(filterAndSortCommands(commands, "")).toHaveLength(3);
  });

  it("ranks a label-prefix match above a substring-only match", () => {
    const withPrefixAndSubstring: Command[] = [
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
    headInfo: { headId: "c1", detachedAt: null, headPushed: false, operation: null, squashMessage: null, commitHooks: [] },
  };

  it("offers the history commands when a commit is selected and nothing is running", () => {
    const ids = buildCommands(makeAppState(historyState)).map((command) => command.id);
    expect(ids).toEqual(expect.arrayContaining(HISTORY_IDS));
  });

  it("omits them while a merge is in progress", () => {
    const ids = buildCommands(makeAppState({ ...historyState, mergeMessage: "Merge x" })).map((c) => c.id);
    for (const id of HISTORY_IDS) expect(ids).not.toContain(id);
  });

  it("omits cherry-pick for HEAD and force push on a detached HEAD", () => {
    const ids = buildCommands(
      makeAppState({
        ...historyState,
        selectedRow: { commitId: "c1" },
        headInfo: { headId: "c1", detachedAt: "c1", headPushed: false, operation: null, squashMessage: null, commitHooks: [] },
      }),
    ).map((c) => c.id);
    expect(ids).not.toContain("cherry-pick-selected");
    expect(ids).not.toContain("force-push");
  });

  it("runs cherry-pick on the selected commit and force push through the confirm", () => {
    const cherryPick = vi.fn();
    const requestForcePush = vi.fn();
    const appState = { ...makeAppState(historyState), cherryPick, requestForcePush };
    const commands = buildCommands(appState);
    commands.find((command) => command.id === "cherry-pick-selected")!.run();
    commands.find((command) => command.id === "force-push")!.run();
    expect(cherryPick).toHaveBeenCalledWith("c2");
    expect(requestForcePush).toHaveBeenCalledWith("origin");
  });

  it("omits reset when there is no current branch (detached HEAD)", () => {
    const ids = buildCommands(
      makeAppState({
        ...historyState,
        branches: [{ name: "main", isCurrent: false }],
        headInfo: { headId: "c1", detachedAt: "c1", headPushed: false, operation: null, squashMessage: null, commitHooks: [] },
      }),
    ).map((c) => c.id);
    expect(ids).not.toContain("reset-selected");
    expect(ids).toContain("checkout-selected");
  });

  it("groups force push under Remotes", () => {
    expect(commandGroup("force-push")).toBe("Remotes");
  });
});

describe("diff option and discard commands", () => {
  const settings = { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true };
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
    return { diffCommands, commands: buildCommands(state, undefined, [], undefined, undefined, undefined, undefined, diffCommands) };
  };

  it("offers stashing the current file only when there is one", () => {
    const state = makeAppState();
    expect(build(state).commands.some((c) => c.id === "stash-current-file")).toBe(false);
    const { commands } = build(state, { currentFile: "src/a.ts" });
    const command = commands.find((c) => c.id === "stash-current-file");
    expect(command?.label).toBe("Stash src/a.ts…");
    command!.run();
    expect(state.openStashDialog).toHaveBeenCalledWith(["src/a.ts"]);
  });

  it("offers file history of the current file, even while an operation is running", () => {
    const state = makeAppState({ pending: true, status: [{ path: "a.txt", staged: false, kind: "Modified" }] });
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
    commands.find((c) => c.id === "increase-context-lines")!.run();
    expect(diffCommands.update).toHaveBeenCalledWith({ contextLines: 4 });
    commands.find((c) => c.id === "decrease-context-lines")!.run();
    expect(diffCommands.update).toHaveBeenCalledWith({ contextLines: 2 });
    commands.find((c) => c.id === "toggle-syntax-highlighting")!.run();
    expect(diffCommands.update).toHaveBeenCalledWith({ syntaxHighlight: false });
    expect(commands.some((c) => c.id === "discard-current-file")).toBe(false);
    expect(commands.some((c) => c.id === "discard-all-unstaged")).toBe(false);
  });

  it("offers discard commands that only request a confirmation", () => {
    const state = makeAppState({ status: [{ path: "a.txt", staged: false, kind: "Modified" }] });
    const { commands, diffCommands } = build(state, { currentFile: "a.txt" });
    commands.find((c) => c.id === "discard-current-file")!.run();
    expect(diffCommands.requestDiscardCurrentFile).toHaveBeenCalled();
    commands.find((c) => c.id === "discard-all-unstaged")!.run();
    expect(diffCommands.requestDiscardAllUnstaged).toHaveBeenCalled();
  });

  it("omits discard-all when only conflicted entries are unstaged", () => {
    const state = makeAppState({ status: [{ path: "a.txt", staged: false, kind: "Conflicted" }] });
    const { commands } = build(state);
    expect(commands.some((c) => c.id === "discard-all-unstaged")).toBe(false);
  });

  it("omits discard commands while an operation is running", () => {
    const state = makeAppState({ pending: true, status: [{ path: "a.txt", staged: false, kind: "Modified" }] });
    const { commands } = build(state, { currentFile: "a.txt" });
    expect(commands.some((c) => c.id === "discard-current-file")).toBe(false);
    expect(commands.some((c) => c.id === "discard-all-unstaged")).toBe(false);
    expect(commands.some((c) => c.id === "toggle-ignore-whitespace")).toBe(true);
  });

  it("groups the new commands", () => {
    expect(commandGroup("toggle-ignore-whitespace")).toBe("Diff");
    expect(commandGroup("discard-all-unstaged")).toBe("Diff");
  });
});

describe("undo commands", () => {
  it("lists undo and redo with their labels when available", () => {
    const onUndo = vi.fn();
    const commands = buildCommands(makeAppState(), undefined, [], undefined, undefined, undefined, undefined, undefined, undefined, {
      undoLabel: "Stage a.txt",
      redoLabel: null,
      onUndo,
      onRedo: vi.fn(),
    });
    const undo = commands.find((c) => c.id === "undo");
    expect(undo?.label).toBe("Undo Stage a.txt");
    expect(commands.find((c) => c.id === "redo")).toBeUndefined();
    undo?.run();
    expect(onUndo).toHaveBeenCalledOnce();
  });

  it("omits undo while a repository operation is running", () => {
    const commands = buildCommands(makeAppState({ pending: true }), undefined, [], undefined, undefined, undefined, undefined, undefined, undefined, {
      undoLabel: "Stage a.txt",
      redoLabel: "x",
      onUndo: vi.fn(),
      onRedo: vi.fn(),
    });
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
    settings: { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true },
    update: vi.fn(),
    currentFile,
    requestDiscardCurrentFile: vi.fn(),
    requestDiscardAllUnstaged: vi.fn(),
    openCurrentFileHistory: vi.fn(),
  });
  const build = (state: ReturnType<typeof makeAppState>, currentFile: string | null, toolCommands: ReturnType<typeof tools>) =>
    buildCommands(
      state,
      undefined,
      [],
      undefined,
      undefined,
      undefined,
      undefined,
      diffCommands(currentFile),
      undefined,
      undefined,
      undefined,
      toolCommands,
    );

  it("adds Preferences… with the app commands", () => {
    const onOpenPreferences = vi.fn();
    const commands = buildCommands(makeAppState(), undefined, [], undefined, undefined, undefined, undefined, undefined, {
      onCloneRepository: vi.fn(),
      onNewRepository: vi.fn(),
      onOpenPreferences,
    });
    const preferences = commands.find((c) => c.label === "Preferences…");
    expect(preferences?.keywords).toEqual(expect.arrayContaining(["settings", "editor", "terminal"]));
    preferences!.run();
    expect(onOpenPreferences).toHaveBeenCalled();
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
    expect(commandGroup("open-terminal")).toBe("Repositories");
    expect(commandGroup("open-current-file-in-diff-tool")).toBe("Diff");
  });

  it("keeps tool commands while an operation is running", () => {
    const labels = build(makeAppState({ pending: true }), "a.ts", tools()).map((c) => c.label);
    expect(labels).toEqual(
      expect.arrayContaining(["Open terminal here", "Reveal repository", "Open current file in editor"]),
    );
  });
});
