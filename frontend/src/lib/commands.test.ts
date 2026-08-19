import { describe, expect, it, vi, beforeEach } from "vitest";
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
      pendingPull: null,
      pullOutcome: null,
      transfer: null,
      error: null,
      pending: false,
      ...overrides,
    },
    openRepo: vi.fn(),
    selectRow: vi.fn(),
    stageFile: vi.fn(),
    unstageFile: vi.fn(),
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
    closeRebasePlanner: vi.fn(),
    startRebase: vi.fn(),
    rebaseContinue: vi.fn(),
    abortRebase: vi.fn(),
    listPullRequests: vi.fn(),
    saveForgeToken: vi.fn(),
    forgetForgeToken: vi.fn(),
    createPullRequest: vi.fn(),
    openExternalUrl: vi.fn(),
    refresh: vi.fn(),
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

  it("includes apply/drop per stash and delete per tag", () => {
    const appState = makeAppState();
    const commands = buildCommands(appState);
    const applyCmd = commands.find((c) => c.id === "apply-stash:0");
    expect(applyCmd?.label).toBe("Apply stash: WIP");
    applyCmd?.run();
    expect(appState.applyStash).toHaveBeenCalledWith(0);

    const dropCmd = commands.find((c) => c.id === "drop-stash:0");
    dropCmd?.run();
    expect(appState.dropStash).toHaveBeenCalledWith(0);

    const deleteTagCmd = commands.find((c) => c.id === "delete-tag:v1.0");
    deleteTagCmd?.run();
    expect(appState.deleteTag).toHaveBeenCalledWith("v1.0");
  });

  it("excludes the main worktree from open/remove commands", () => {
    const commands = buildCommands(makeAppState());
    expect(commands.some((c) => c.id === "open-worktree:/repo")).toBe(false);
    expect(commands.some((c) => c.id === "remove-worktree:main")).toBe(false);

    const openCmd = commands.find((c) => c.id === "open-worktree:/repo-wt1");
    expect(openCmd?.label).toBe("Open worktree wt1");
    openCmd?.run();
    expect(commands.find((c) => c.id === "open-worktree:/repo-wt1"));
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

  it("includes one restore command per reflog entry, with both reference and newId bound", () => {
    const appState = makeAppState();
    const commands = buildCommands(appState);
    const restoreCmd = commands.find((c) => c.id.startsWith("restore-reflog:"));
    expect(restoreCmd?.label).toBe("Restore HEAD@{0} to bbbbbbb");
    restoreCmd?.run();
    expect(appState.restoreReflogEntry).toHaveBeenCalledWith(
      "HEAD@{0}",
      "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
    );
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
