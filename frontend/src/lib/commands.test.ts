import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import {
  buildCommands,
  filterAndSortCommands,
  loadRecentCommandIds,
  recordCommandUsed,
  type Command,
} from "./commands";
import type { UseAppStateResult } from "../state/useAppState";

function makeAppState(overrides: Partial<UseAppStateResult["state"]> = {}): UseAppStateResult {
  return {
    state: {
      repoPath: "/repo",
      selectedRow: "uncommitted",
      status: [],
      commits: [],
      graphBranchSelection: null,
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
      rebaseProgress: null,
      rebaseOnto: null,
      squashPreset: null,
      pendingPull: null,
      pullOutcome: null,
      transfer: null,
      error: null,
      pending: false,
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
    saveStash: vi.fn(),
    applyStash: vi.fn(),
    dropStash: vi.fn(),
    mergeBranch: vi.fn(),
    resolveConflict: vi.fn(),
    resolveAddDeleteConflict: vi.fn(),
    abortMerge: vi.fn(),
    openRebasePlanner: vi.fn(),
    openSquashPlanner: vi.fn(),
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
    refresh: vi.fn(),
    dismissError: vi.fn(),
  };
}

describe("buildCommands", () => {
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

    const dropCmd = commands.find((c) => c.id === "drop-stash:0");
    dropCmd?.run();
    expect(appState.dropStash).toHaveBeenCalledWith(0);
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
          "Worktrees",
          "Submodules",
          "Reflog",
          "Remotes",
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
        "Go to Remotes",
        "Go to Submodules",
        "Go to Tags",
        "Go to Worktrees",
      ].sort(),
    );
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
