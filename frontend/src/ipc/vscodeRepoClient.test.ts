import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { HeadInfo, RebaseAbort, RepoClient } from "./RepoClient";

/** `abortRebase` resolves to the restored flag. */
const ABORTED_REBASE: RebaseAbort = { restored: false };
/** `getHeadInfo` passes `rebase` through, including a diverged reason. */
const DIVERGED_HEAD: HeadInfo = {
  headId: "abc",
  detachedAt: null,
  headPushed: false,
  operation: null,
  operationLeftover: false,
  squashMessage: null,
  commitHooks: [],
  sequence: null,
  rebase: {
    branch: "main",
    onto: "def",
    currentStep: 2,
    totalSteps: 3,
    health: { kind: "Diverged", reason: "HeadMoved" },
  },
  bisect: null, terminalSequence: null,
  terminalRebase: { kind: "Am", branch: "", onto: "", currentStep: 1, totalSteps: 2 },
};

describe("vscodeRepoClient", () => {
  let postMessage: ReturnType<typeof vi.fn>;
  let vscodeRepoClient: RepoClient;
  let resetVscodeRepoClientForTests: (() => void) | undefined;

  beforeEach(async () => {
    vi.resetModules();
    postMessage = vi.fn();
    (
      globalThis as unknown as { acquireVsCodeApi: () => { postMessage: typeof postMessage } }
    ).acquireVsCodeApi = () => ({ postMessage });
    const clientModule = await import("./vscodeRepoClient");
    vscodeRepoClient = clientModule.vscodeRepoClient;
    resetVscodeRepoClientForTests = clientModule.__resetVscodeRepoClientForTests;
  });

  afterEach(() => {
    resetVscodeRepoClientForTests?.();
    vi.doUnmock("./tauriRepoClient");
  });

  function respond(id: number, result: unknown) {
    window.dispatchEvent(new MessageEvent("message", { data: { jsonrpc: "2.0", id, result } }));
  }

  function respondWithError(id: number, message: string) {
    window.dispatchEvent(
      new MessageEvent("message", {
        data: { jsonrpc: "2.0", id, error: { code: -32000, message } },
      }),
    );
  }

  it("posts a JSON-RPC request and resolves on a matching reply", async () => {
    const promise = vscodeRepoClient.getStatus("/repo");

    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_status",
      params: { repoPath: "/repo" },
    });
    respond(1, [{ path: "a.txt", oldPath: null, staged: false, kind: "New" }]);

    await expect(promise).resolves.toEqual([{ path: "a.txt", oldPath: null, staged: false, kind: "New" }]);
  });

  it("rejects when the sidecar returns a JSON-RPC error", async () => {
    const promise = vscodeRepoClient.getStatus("/missing");

    respondWithError(1, "repo not open: /missing");

    await expect(promise).rejects.toThrow("repo not open: /missing");
  });

  // #496: the kind comes from the error's `data` (the sidecar's `RepoErrorPayload`), or from the
  // extension host's transport-loss code; `String(error)` is the message alone.
  it.each([
    [{ code: -32000, message: "worker thread stopped", data: { kind: "WorkerStopped", message: "worker thread stopped" } }, "WorkerStopped"],
    [{ code: -32000, message: "repo not open: /missing", data: { kind: "Other", message: "repo not open: /missing" } }, "Other"],
    [{ code: -32000, message: "from the extension host" }, "Other"],
    [{ code: -32000, message: "newer sidecar", data: { kind: "SomethingNew", message: "newer sidecar" } }, "Other"],
    [{ code: -32001, message: "Browsitory sidecar exited with code 1" }, "TransportLost"],
  ])("rejects a JSON-RPC error %j with the kind %s", async (error, kind) => {
    const promise = vscodeRepoClient.getStatus("/repo");

    window.dispatchEvent(new MessageEvent("message", { data: { jsonrpc: "2.0", id: 1, error } }));

    const rejection: unknown = await promise.catch((caught: unknown) => caught);
    expect((rejection as { kind: unknown }).kind).toBe(kind);
    expect(String(rejection)).toBe(error.message);
  });

  it("loads without the Tauri transport module", async () => {
    // A VS Code webview does not ship @tauri-apps/api. Its adapter must therefore be loadable
    // even when the Tauri-specific client cannot be imported.
    vi.resetModules();
    vi.doMock("./tauriRepoClient", () => {
      throw new Error("Tauri transport is unavailable");
    });

    await expect(import("./vscodeRepoClient")).resolves.toHaveProperty("vscodeRepoClient");
  });

  it("correlates concurrent requests by id", async () => {
    const openPromise = vscodeRepoClient.openRepo("/repo");
    const statusPromise = vscodeRepoClient.getStatus("/repo");

    respond(2, [{ path: "b.txt", oldPath: null, staged: true, kind: "Modified" }]);
    respond(1, null);

    // A void-returning JSON-RPC method's wire response carries `result: null` (JSON has no
    // `undefined`); the client resolves with that `null` as-is rather than coercing it.
    await expect(openPromise).resolves.toBeNull();
    await expect(statusPromise).resolves.toEqual([
      { path: "b.txt", oldPath: null, staged: true, kind: "Modified" },
    ]);
  });

  it("wires VS Code-native host methods", async () => {
    const methods = [
      [() => vscodeRepoClient.pickRepoFolder(), "pick_repo_folder", {}, "/repos/a"],
      [() => vscodeRepoClient.getAppVersion(), "get_app_version", {}, "1.2.3"],
      [() => vscodeRepoClient.getLastSeenVersion(), "get_last_seen_version", {}, null],
      [() => vscodeRepoClient.setLastSeenVersion("1.2.3"), "set_last_seen_version", { version: "1.2.3" }, null],
      [() => vscodeRepoClient.openExternalUrl("https://example.com/pull/1"), "open_external_url", { url: "https://example.com/pull/1" }, null],
      [() => vscodeRepoClient.openInEditor("/r", "a.ts", null), "open_in_editor", { repoPath: "/r", path: "a.ts", line: null }, null],
      [() => vscodeRepoClient.revealInFileManager("/r", "a.ts"), "reveal_in_file_manager", { repoPath: "/r", path: "a.ts" }, null],
      [() => vscodeRepoClient.openTerminal("/r"), "open_terminal", { repoPath: "/r" }, null],
      [() => vscodeRepoClient.openDiffTool("/r", "a.ts", { kind: "Staged", oldPath: null }), "open_diff_tool", { repoPath: "/r", path: "a.ts", source: { kind: "Staged", oldPath: null } }, null],
      [() => vscodeRepoClient.openMergeTool("/r", "a.ts"), "open_merge_tool", { repoPath: "/r", path: "a.ts" }, "HandedOff"],
      [() => vscodeRepoClient.markResolved("/r", "a.ts", true), "mark_resolved", { repoPath: "/r", path: "a.ts", force: true }, "Resolved"],
      [() => vscodeRepoClient.getExternalToolSettings(), "get_external_tool_settings", {}, { editor: null, terminal: null }],
      [() => vscodeRepoClient.setExternalToolSettings({ editor: null, terminal: "kitty" }), "set_external_tool_settings", { settings: { editor: null, terminal: "kitty" } }, null],
      [() => vscodeRepoClient.getGitTools("/r"), "get_git_tools", { repoPath: "/r" }, { diffTool: null, mergeTool: null }],
      [() => vscodeRepoClient.openInEditor("/r", "a.ts", null), "open_in_editor", { repoPath: "/r", path: "a.ts", line: null }, null],
      [() => vscodeRepoClient.revealInFileManager("/r", "a.ts"), "reveal_in_file_manager", { repoPath: "/r", path: "a.ts" }, null],
      [() => vscodeRepoClient.openTerminal("/r"), "open_terminal", { repoPath: "/r" }, null],
      [() => vscodeRepoClient.openDiffTool("/r", "a.ts", { kind: "Staged", oldPath: null }), "open_diff_tool", { repoPath: "/r", path: "a.ts", source: { kind: "Staged", oldPath: null } }, null],
      [() => vscodeRepoClient.openMergeTool("/r", "a.ts"), "open_merge_tool", { repoPath: "/r", path: "a.ts" }, "HandedOff"],
      [() => vscodeRepoClient.markResolved("/r", "a.ts", true), "mark_resolved", { repoPath: "/r", path: "a.ts", force: true }, "Resolved"],
      [() => vscodeRepoClient.getExternalToolSettings(), "get_external_tool_settings", {}, { editor: null, terminal: null }],
      [() => vscodeRepoClient.setExternalToolSettings({ editor: null, terminal: "kitty" }), "set_external_tool_settings", { settings: { editor: null, terminal: "kitty" } }, null],
      [() => vscodeRepoClient.getGitTools("/r"), "get_git_tools", { repoPath: "/r" }, { diffTool: null, mergeTool: null }],
      [() => vscodeRepoClient.getWorkingDiff("/repo", "f.txt", false, { ignoreWhitespace: true, contextLines: 5, algorithm: "Patience" }), "get_working_diff", { repoPath: "/repo", path: "f.txt", staged: false, opts: { ignoreWhitespace: true, contextLines: 5, algorithm: "Patience" }, oldPath: null }, []],
      [() => vscodeRepoClient.getWorkingDiff("/repo", "new.txt", true, { ignoreWhitespace: true, contextLines: 5, algorithm: "Patience" }, "old.txt"), "get_working_diff", { repoPath: "/repo", path: "new.txt", staged: true, opts: { ignoreWhitespace: true, contextLines: 5, algorithm: "Patience" }, oldPath: "old.txt" }, []],
      [() => vscodeRepoClient.getCommitDiff("/repo", "abc", "f.txt", { ignoreWhitespace: true, contextLines: 5, algorithm: "Patience" }), "get_commit_diff", { repoPath: "/repo", commitId: "abc", path: "f.txt", opts: { ignoreWhitespace: true, contextLines: 5, algorithm: "Patience" } }, []],
      [() => vscodeRepoClient.stageHunk("/repo", "f.txt", 1, 2, { contextLines: 5, algorithm: "Patience" }), "stage_hunk", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5, algorithm: "Patience" }, null],
      [() => vscodeRepoClient.unstageHunk("/repo", "f.txt", 1, 2, { contextLines: 5, algorithm: "Patience" }), "unstage_hunk", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5, algorithm: "Patience" }, null],
      [() => vscodeRepoClient.discardHunk("/repo", "f.txt", 1, 2, { contextLines: 5, algorithm: "Patience" }), "discard_hunk", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5, algorithm: "Patience" }, null],
      [() => vscodeRepoClient.stageLines("/repo", "f.txt", 1, 2, { contextLines: 5, algorithm: "Patience" }, [3], ["x\n"]), "stage_lines", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5, algorithm: "Patience", lineIndices: [3], expectedLines: ["x\n"] }, null],
      [() => vscodeRepoClient.unstageLines("/repo", "f.txt", 1, 2, { contextLines: 5, algorithm: "Patience" }, [3], ["x\n"]), "unstage_lines", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5, algorithm: "Patience", lineIndices: [3], expectedLines: ["x\n"] }, null],
      [() => vscodeRepoClient.discardLines("/repo", "f.txt", 1, 2, { contextLines: 5, algorithm: "Patience" }, [3], ["x\n"]), "discard_lines", { repoPath: "/repo", path: "f.txt", oldStart: 1, newStart: 2, contextLines: 5, algorithm: "Patience", lineIndices: [3], expectedLines: ["x\n"] }, null],
      [() => vscodeRepoClient.discardFile("/repo", "f.txt"), "discard_file", { repoPath: "/repo", path: "f.txt" }, null],
      [() => vscodeRepoClient.discardAllUnstaged("/repo", ["a.txt"]), "discard_all_unstaged", { repoPath: "/repo", paths: ["a.txt"] }, null],
      [() => vscodeRepoClient.ignorePath("/repo", "*.log"), "ignore_path", { repoPath: "/repo", pattern: "*.log" }, null],
      [() => vscodeRepoClient.ignorePaths("/repo", ["*.log", "/out/"]), "ignore_paths", { repoPath: "/repo", patterns: ["*.log", "/out/"] }, null],
      [() => vscodeRepoClient.stagePaths("/repo", ["a.txt", "b.txt"]), "stage_paths", { repoPath: "/repo", paths: ["a.txt", "b.txt"] }, null],
      [() => vscodeRepoClient.unstagePaths("/repo", ["a.txt", "b.txt"]), "unstage_paths", { repoPath: "/repo", paths: ["a.txt", "b.txt"] }, null],
      [() => vscodeRepoClient.getRootFiles("/repo"), "get_root_files", { repoPath: "/repo" }, [".gitignore"]],
      [() => vscodeRepoClient.createRootFile("/repo", ".gitattributes"), "create_root_file", { repoPath: "/repo", name: ".gitattributes" }, null],
      [() => vscodeRepoClient.cherryPick("/r", "m", 2), "cherry_pick", { repoPath: "/r", commitId: "m", mainline: 2 }, { kind: "Committed", commitId: "c" }],
      [() => vscodeRepoClient.revertCommit("/r", "m", 1), "revert_commit", { repoPath: "/r", commitId: "m", mainline: 1 }, { kind: "Committed", commitId: "c" }],
      [() => vscodeRepoClient.cherryPickMany("/r", ["a", "b"]), "cherry_pick_many", { repoPath: "/r", commitIds: ["a", "b"] }, { kind: "Completed", applied: 2, skipped: [] }],
      [() => vscodeRepoClient.revertMany("/r", ["a"]), "revert_many", { repoPath: "/r", commitIds: ["a"] }, { kind: "Completed", applied: 1, skipped: [] }],
      [() => vscodeRepoClient.continueSequence("/r"), "continue_sequence", { repoPath: "/r", noVerify: false }, { kind: "Completed", applied: 1, skipped: [] }],
      [() => vscodeRepoClient.continueSequence("/r", true), "continue_sequence", { repoPath: "/r", noVerify: true }, { kind: "Completed", applied: 1, skipped: [] }],
      [() => vscodeRepoClient.abortSequence("/r"), "abort_sequence", { repoPath: "/r", confirmed: null }, { rewound: false }],
      [
        () => vscodeRepoClient.abortSequence("/r", { headId: "h", startedAt: "s", rewinds: true, commitsToRemove: 2, pausedOnConflict: false }),
        "abort_sequence",
        { repoPath: "/r", confirmed: { headId: "h", startedAt: "s", rewinds: true, commitsToRemove: 2, pausedOnConflict: false } },
        { rewound: true },
      ],
      [() => vscodeRepoClient.abortRebase("/r"), "abort_rebase", { repoPath: "/r" }, ABORTED_REBASE],
      [() => vscodeRepoClient.getHeadInfo("/r"), "get_head_info", { repoPath: "/r" }, DIVERGED_HEAD],
      [() => vscodeRepoClient.resolveWholeFile("/r", "a.txt", "Theirs"), "resolve_whole_file", { repoPath: "/r", path: "a.txt", side: "Theirs" }, null],
      [() => vscodeRepoClient.getFileSides("/r", "a.png", { kind: "Commit", commitId: "c", oldPath: "b.png" }), "get_file_sides", { repoPath: "/r", path: "a.png", source: { kind: "Commit", commitId: "c", oldPath: "b.png" } }, { old: { kind: "Missing" }, new: { kind: "Missing" } }],
      [() => vscodeRepoClient.getCommitTree("/r", "c", "src", null), "get_commit_tree", { repoPath: "/r", commitId: "c", dir: "src", filter: null }, { entries: [], truncated: false }],
      [() => vscodeRepoClient.getFileAtCommit("/r", "c", "a.txt"), "get_file_at_commit", { repoPath: "/r", commitId: "c", path: "a.txt" }, { kind: "Text", size: 1, text: "a" }],
      [() => vscodeRepoClient.saveFileVersion("/r", "c", "a.txt"), "save_file_version", { repoPath: "/r", commitId: "c", path: "a.txt" }, "/home/me/a.txt"],
      [() => vscodeRepoClient.restoreFileVersion("/r", "c", "a.txt"), "restore_file_version", { repoPath: "/r", commitId: "c", path: "a.txt" }, null],
      [() => vscodeRepoClient.listRecentAuthors("/r"), "list_recent_authors", { repoPath: "/r" }, [{ name: "Ada", email: "ada@x.org" }]],
      [() => vscodeRepoClient.listRepoAliases(), "list_repo_aliases", {}, [{ repoPath: "/r", alias: "app" }]],
      [() => vscodeRepoClient.setRepoAlias("/r", "app"), "set_repo_alias", { repoPath: "/r", alias: "app" }, null],
      [() => vscodeRepoClient.setRepoAlias("/r", null), "set_repo_alias", { repoPath: "/r", alias: null }, null],
      [() => vscodeRepoClient.getCommitMessageSettings(), "get_commit_message_settings", {}, { summaryLimit: 50, bodyRuler: 72, showRuler: true }],
      [() => vscodeRepoClient.setCommitMessageSettings({ summaryLimit: 60, bodyRuler: 80, showRuler: false }), "set_commit_message_settings", { settings: { summaryLimit: 60, bodyRuler: 80, showRuler: false } }, null],
      [() => vscodeRepoClient.commit("/r", "m", false, [{ key: "Co-authored-by", value: "Ada <ada@x.org>" }]), "commit", { repoPath: "/r", message: "m", noVerify: false, trailers: [{ key: "Co-authored-by", value: "Ada <ada@x.org>" }] }, { kind: "Committed", commitId: "c" }],
      [() => vscodeRepoClient.amendCommit("/r", "m", true, { author: { name: "Ada", email: "ada@x.org" }, trailers: [] }), "amend_commit", { repoPath: "/r", message: "m", noVerify: true, author: { name: "Ada", email: "ada@x.org" }, trailers: [] }, { kind: "Committed", commitId: "c" }],
      [() => vscodeRepoClient.getDiffSettings(), "get_diff_settings", {}, { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true, wordWrap: "Auto", algorithm: "Minimal" as const, showWhitespace: false, expandUntracked: true }],
      [() => vscodeRepoClient.getFetchSettings(), "get_fetch_settings", {}, { autoFetchMinutes: 0 }],
      [() => vscodeRepoClient.setFetchSettings({ autoFetchMinutes: 10 }), "set_fetch_settings", { settings: { autoFetchMinutes: 10 } }, null],
      [() => vscodeRepoClient.getAppearanceSettings(), "get_appearance_settings", {}, { codeFont: null, codeFontSize: null, uiSize: "Normal" as const, tabSize: 4 }],
      [() => vscodeRepoClient.setAppearanceSettings({ codeFont: "Mono", codeFontSize: 14, uiSize: "Small" as const, tabSize: 8 }), "set_appearance_settings", { settings: { codeFont: "Mono", codeFontSize: 14, uiSize: "Small" as const, tabSize: 8 } }, null],
      [() => vscodeRepoClient.getUpdateSettings(), "get_update_settings", {}, { checkAutomatically: true }],
      [() => vscodeRepoClient.setUpdateSettings({ checkAutomatically: false }), "set_update_settings", { settings: { checkAutomatically: false } }, null],
      [() => vscodeRepoClient.backgroundFetch("/r", null, 5), "background_fetch", { repoPath: "/r", prune: null, intervalMinutes: 5 }, "background-fetch-1"],
      [() => vscodeRepoClient.backgroundFetch("/r", true, 60), "background_fetch", { repoPath: "/r", prune: true, intervalMinutes: 60 }, null],
      [() => vscodeRepoClient.setDiffSettings({ ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true, wordWrap: "Auto", algorithm: "Minimal" as const, showWhitespace: false, expandUntracked: true }), "set_diff_settings", { settings: { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true, wordWrap: "Auto", algorithm: "Minimal" as const, showWhitespace: false, expandUntracked: true } }, null],
    ] as const;

    for (const [index, [call, method, params, result]] of methods.entries()) {
      const promise = call();
      expect(postMessage).toHaveBeenLastCalledWith({ jsonrpc: "2.0", id: index + 1, method, params });
      respond(index + 1, result);
      await expect(promise).resolves.toBe(result);
    }
  });

  it("rejects pending calls after a transportStatus notification and permits a fresh call", async () => {
    const first = vscodeRepoClient.getStatus("/repo");
    const second = vscodeRepoClient.listRecentRepos();
    const received = vi.fn();
    const { subscribeTransportStatus } = await import("./transportStatus");
    subscribeTransportStatus(received);

    window.dispatchEvent(new MessageEvent("message", {
      data: {
        jsonrpc: "2.0",
        method: "transportStatus",
        params: { state: "reconnecting", message: "sidecar is reconnecting" },
      },
    }));

    await expect(first).rejects.toThrow("sidecar is reconnecting");
    await expect(second).rejects.toMatchObject({ kind: "TransportLost", message: "sidecar is reconnecting" });
    expect(received).toHaveBeenCalledWith({ state: "reconnecting", message: "sidecar is reconnecting" });

    const afterReconnect = vscodeRepoClient.getStatus("/repo");
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0", id: 3, method: "get_status", params: { repoPath: "/repo" },
    });
    respond(3, []);
    await expect(afterReconnect).resolves.toEqual([]);
  });


  it("reports the extension host's panel visibility through the host visibility seam", async () => {
    const { isHostVisible, subscribeHostVisibility } = await import("./hostVisibility");
    const received = vi.fn();
    const unsubscribe = subscribeHostVisibility(received);
    const post = (params: unknown) =>
      window.dispatchEvent(new MessageEvent("message", { data: { jsonrpc: "2.0", method: "panelVisibility", params } }));

    post({ visible: false });
    expect(received).toHaveBeenLastCalledWith(false);
    expect(isHostVisible()).toBe(false);
    post({ visible: "nonsense" });
    expect(received).toHaveBeenCalledTimes(1);
    post({ visible: true });
    expect(received).toHaveBeenLastCalledWith(true);
    expect(isHostVisible()).toBe(true);
    unsubscribe();
  });

  it("rejects pending calls after a failed transportStatus notification", async () => {
    const request = vscodeRepoClient.getStatus("/repo");

    window.dispatchEvent(new MessageEvent("message", {
      data: {
        jsonrpc: "2.0",
        method: "transportStatus",
        params: { state: "failed", message: "sidecar failed" },
      },
    }));

    await expect(request).rejects.toThrow("sidecar failed");
  });

  it("stops delivering transport status notifications after unsubscribe", async () => {
    const received = vi.fn();
    const { subscribeTransportStatus } = await import("./transportStatus");
    const unsubscribe = subscribeTransportStatus(received);

    window.dispatchEvent(new MessageEvent("message", {
      data: {
        jsonrpc: "2.0",
        method: "transportStatus",
        params: { state: "reconnecting", message: "sidecar is reconnecting" },
      },
    }));
    unsubscribe();
    window.dispatchEvent(new MessageEvent("message", {
      data: {
        jsonrpc: "2.0",
        method: "transportStatus",
        params: { state: "failed", message: "sidecar failed" },
      },
    }));

    expect(received).toHaveBeenCalledOnce();
  });

  it("exports a deterministic test reset", () => {
    expect(resetVscodeRepoClientForTests).toEqual(expect.any(Function));
  });

  it("wires listRecentRepos", async () => {
    const promise = vscodeRepoClient.listRecentRepos();
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "list_recent_repos",
      params: {},
    });
    respond(1, ["/repos/a"]);
    await expect(promise).resolves.toEqual(["/repos/a"]);
  });

  it("runs VSCode's Git: Clone and Git: Initialize Repository through the extension host (#680)", async () => {
    expect(vscodeRepoClient.supportsHostGitCommands).toBe(true);
    const clone = vscodeRepoClient.runHostGitCommand("clone");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "run_vscode_git_command",
      params: { command: "clone" },
    });
    respond(1, null);
    await expect(clone).resolves.toBeNull();
  });

  it("wires listOpenRepos", async () => {
    const promise = vscodeRepoClient.listOpenRepos();
    respond(1, { entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" });
    await expect(promise).resolves.toEqual({
      entries: [{ path: "/repos/a", workspaceId: null }],
      activePath: "/repos/a",
    });
  });

  it("wires persistOpenRepos", async () => {
    const promise = vscodeRepoClient.persistOpenRepos(
      [{ path: "/repos/a", workspaceId: null }],
      "/repos/a",
    );
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "persist_open_repos",
      params: { entries: [{ path: "/repos/a", workspaceId: null }], activePath: "/repos/a" },
    });
    respond(1, null);
    await expect(promise).resolves.toBeNull();
  });

  it("wires scanReposInRoot", async () => {
    const promise = vscodeRepoClient.scanReposInRoot("/repos");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "scan_repos_in_root",
      params: { root: "/repos" },
    });
    respond(1, ["/repos/a"]);
    await expect(promise).resolves.toEqual(["/repos/a"]);
  });

  it("wires listWorkspaces", async () => {
    const promise = vscodeRepoClient.listWorkspaces();
    respond(1, [{ id: "w1", name: "Suite", rootPath: "/repos/suite", memberPaths: [] }]);
    await expect(promise).resolves.toEqual([
      { id: "w1", name: "Suite", rootPath: "/repos/suite", memberPaths: [] },
    ]);
  });

  it("wires saveWorkspace", async () => {
    const promise = vscodeRepoClient.saveWorkspace("Suite", "/repos/suite", ["/repos/suite/api"]);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "save_workspace",
      params: { name: "Suite", root: "/repos/suite", members: ["/repos/suite/api"] },
    });
    respond(1, "w1");
    await expect(promise).resolves.toBe("w1");
  });

  it("wires updateWorkspace", async () => {
    const promise = vscodeRepoClient.updateWorkspace("w1", "Renamed", ["/repos/suite/web"]);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "update_workspace",
      params: { id: "w1", name: "Renamed", members: ["/repos/suite/web"] },
    });
    respond(1, null);
    await expect(promise).resolves.toBeNull();
  });

  it("wires deleteWorkspace", async () => {
    const promise = vscodeRepoClient.deleteWorkspace("w1");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "delete_workspace",
      params: { id: "w1" },
    });
    respond(1, null);
    await expect(promise).resolves.toBeNull();
  });

  it("wires getGraphBranchSelection and setGraphBranchSelection", async () => {
    const getPromise = vscodeRepoClient.getGraphBranchSelection("/repo");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_graph_branch_selection",
      params: { repoPath: "/repo" },
    });
    respond(1, ["main"]);
    await expect(getPromise).resolves.toEqual(["main"]);

    const setPromise = vscodeRepoClient.setGraphBranchSelection("/repo", ["main", "feature"]);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "set_graph_branch_selection",
      params: { repoPath: "/repo", local: ["main", "feature"] },
    });
    respond(2, null);
    await expect(setPromise).resolves.toBeNull();
  });

  it("wires getGraphRemoteBranchSelection and setGraphRemoteBranchSelection", async () => {
    const getPromise = vscodeRepoClient.getGraphRemoteBranchSelection("/repo");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_graph_remote_branch_selection",
      params: { repoPath: "/repo" },
    });
    respond(1, ["origin/main"]);
    await expect(getPromise).resolves.toEqual(["origin/main"]);

    const setPromise = vscodeRepoClient.setGraphRemoteBranchSelection("/repo", ["origin/main"]);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "set_graph_remote_branch_selection",
      params: { repoPath: "/repo", remote: ["origin/main"] },
    });
    respond(2, null);
    await expect(setPromise).resolves.toBeNull();
  });

  it("wires clearGraphBranchSelection and clearGraphRemoteBranchSelection (#402)", async () => {
    const localPromise = vscodeRepoClient.clearGraphBranchSelection("/repo");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "clear_graph_branch_selection",
      params: { repoPath: "/repo" },
    });
    respond(1, null);
    await expect(localPromise).resolves.toBeNull();

    const remotePromise = vscodeRepoClient.clearGraphRemoteBranchSelection("/repo");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "clear_graph_remote_branch_selection",
      params: { repoPath: "/repo" },
    });
    respond(2, null);
    await expect(remotePromise).resolves.toBeNull();
  });

  it("wires getCommitFiles", async () => {
    const promise = vscodeRepoClient.getCommitFiles("/repo", "abc123");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_commit_files",
      params: { repoPath: "/repo", commitId: "abc123" },
    });
    respond(1, ["a.txt"]);
    await expect(promise).resolves.toEqual(["a.txt"]);
  });

  it("wires getCommitStats (#620)", async () => {
    const promise = vscodeRepoClient.getCommitStats("/repo", ["abc123", "def456"], false);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_commit_stats",
      params: { repoPath: "/repo", commitIds: ["abc123", "def456"], withFiles: false },
    });
    const entries = [{ commitId: "abc123", stats: null }];
    respond(1, entries);
    await expect(promise).resolves.toEqual(entries);
  });

  it("wires getCommitDetails (#641)", async () => {
    const promise = vscodeRepoClient.getCommitDetails("/repo", "abc123");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_commit_details",
      params: { repoPath: "/repo", commitId: "abc123" },
    });
    const person = { name: "Ada", email: "ada@example.com", timestamp: 1 };
    const details = { author: person, committer: person, branches: ["main"], remoteBranches: [], tags: [], complete: true };
    respond(1, details);
    await expect(promise).resolves.toEqual(details);
  });

  it("wires the compare methods", async () => {
    const opts = { ignoreWhitespace: true, contextLines: 5, algorithm: "Patience" as const };
    const files = vscodeRepoClient.getRangeFiles("/repo", "a", "b");
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_range_files",
      params: { repoPath: "/repo", fromId: "a", toId: "b" },
    });
    respond(1, [{ path: "new.txt", oldPath: "old.txt" }]);
    await expect(files).resolves.toEqual([{ path: "new.txt", oldPath: "old.txt" }]);

    void vscodeRepoClient.getRangeDiff("/repo", "a", "b", "new.txt", "old.txt", opts);
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "get_range_diff",
      params: { repoPath: "/repo", fromId: "a", toId: "b", path: "new.txt", oldPath: "old.txt", opts },
    });
    void vscodeRepoClient.getWorkingTreeCompareFiles("/repo", "a");
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "get_working_tree_compare_files",
      params: { repoPath: "/repo", commitId: "a" },
    });
    void vscodeRepoClient.getWorkingTreeCompareDiff("/repo", "a", "x.txt", null, opts);
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 4,
      method: "get_working_tree_compare_diff",
      params: { repoPath: "/repo", commitId: "a", path: "x.txt", oldPath: null, opts },
    });
    const range = vscodeRepoClient.getBranchCompareRange("/repo", "refs/remotes/origin/main");
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 5,
      method: "get_branch_compare_range",
      params: { repoPath: "/repo", refName: "refs/remotes/origin/main" },
    });
    respond(5, { fromId: "base", toId: "tip" });
    await expect(range).resolves.toEqual({ fromId: "base", toId: "tip" });
  });

  it("wires locateCommit (#441)", async () => {
    const promise = vscodeRepoClient.locateCommit("/repo", "v1.0", { local: null, remote: [] });
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "locate_commit",
      params: { repoPath: "/repo", revision: "v1.0", selection: { local: null, remote: [] } },
    });
    respond(1, null);
    await expect(promise).resolves.toBeNull();
  });

  it("wires getCommitMessage", async () => {
    const promise = vscodeRepoClient.getCommitMessage("/repo", "abc123");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_commit_message",
      params: { repoPath: "/repo", commitId: "abc123" },
    });
    respond(1, "subject\n\nbody\n");
    await expect(promise).resolves.toBe("subject\n\nbody\n");
  });

  it("wires stageFile and unstageFile", async () => {
    const stagePromise = vscodeRepoClient.stageFile("/repo", "a.txt");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "stage_file",
      params: { repoPath: "/repo", path: "a.txt" },
    });
    respond(1, null);
    await expect(stagePromise).resolves.toBeNull();

    const unstagePromise = vscodeRepoClient.unstageFile("/repo", "a.txt");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "unstage_file",
      params: { repoPath: "/repo", path: "a.txt" },
    });
    respond(2, null);
    await expect(unstagePromise).resolves.toBeNull();
  });

  it("wires stageHunk, unstageHunk, and discardHunk", async () => {
    const stagePromise = vscodeRepoClient.stageHunk("/repo", "a.txt", 1, 1, { contextLines: 3, algorithm: "Patience" });
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "stage_hunk",
      params: { repoPath: "/repo", path: "a.txt", oldStart: 1, newStart: 1, contextLines: 3, algorithm: "Patience" },
    });
    respond(1, null);
    await expect(stagePromise).resolves.toBeNull();

    const unstagePromise = vscodeRepoClient.unstageHunk("/repo", "a.txt", 1, 1, { contextLines: 3, algorithm: "Patience" });
    respond(2, null);
    await expect(unstagePromise).resolves.toBeNull();

    const discardPromise = vscodeRepoClient.discardHunk("/repo", "a.txt", 1, 1, { contextLines: 3, algorithm: "Patience" });
    respond(3, null);
    await expect(discardPromise).resolves.toBeNull();
  });

  it("wires commit", async () => {
    const promise = vscodeRepoClient.commit("/repo", "message", true);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "commit",
      params: { repoPath: "/repo", message: "message", noVerify: true, trailers: [] },
    });
    respond(1, { kind: "Committed", commitId: "abc123" });
    await expect(promise).resolves.toEqual({ kind: "Committed", commitId: "abc123" });
  });

  it("wires listBranches", async () => {
    const promise = vscodeRepoClient.listBranches("/repo");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "list_branches",
      params: { repoPath: "/repo" },
    });
    respond(1, [{ name: "main", isCurrent: true }]);
    await expect(promise).resolves.toEqual([{ name: "main", isCurrent: true }]);
  });

  it("wires createBranch, switchBranch, renameBranch, and deleteBranch", async () => {
    const createPromise = vscodeRepoClient.createBranch("/repo", "feature", "HEAD", false);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "create_branch",
      params: { repoPath: "/repo", name: "feature", startPoint: "HEAD", checkout: false },
    });
    respond(1, null);
    await expect(createPromise).resolves.toBeNull();

    const switchPromise = vscodeRepoClient.switchBranch("/repo", "feature");
    respond(2, null);
    await expect(switchPromise).resolves.toBeNull();

    const renamePromise = vscodeRepoClient.renameBranch("/repo", "feature", "renamed");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "rename_branch",
      params: { repoPath: "/repo", oldName: "feature", newName: "renamed" },
    });
    respond(3, null);
    await expect(renamePromise).resolves.toBeNull();

    const deletePromise = vscodeRepoClient.deleteBranch("/repo", "renamed", true);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 4,
      method: "delete_branch",
      params: { repoPath: "/repo", name: "renamed", force: true },
    });
    respond(4, null);
    await expect(deletePromise).resolves.toBeNull();
  });

  it("wires listWorktrees", async () => {
    const promise = vscodeRepoClient.listWorktrees("/repo");
    respond(1, [
      { name: "main", path: "/repo", head: "refs/heads/main", isMain: true, isLocked: false, isPrunable: false },
    ]);
    await expect(promise).resolves.toEqual([
      { name: "main", path: "/repo", head: "refs/heads/main", isMain: true, isLocked: false, isPrunable: false },
    ]);
  });

  it("wires createWorktree, removeWorktree, and pruneWorktrees", async () => {
    const createPromise = vscodeRepoClient.createWorktree("/repo", "feature-tree", "/repo-feature", "feature", "HEAD");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "create_worktree",
      params: { repoPath: "/repo", name: "feature-tree", path: "/repo-feature", branch: "feature", startPoint: "HEAD" },
    });
    respond(1, null);
    await expect(createPromise).resolves.toBeNull();

    const removePromise = vscodeRepoClient.removeWorktree("/repo", "feature-tree");
    respond(2, null);
    await expect(removePromise).resolves.toBeNull();

    const prunePromise = vscodeRepoClient.pruneWorktrees("/repo");
    respond(3, null);
    await expect(prunePromise).resolves.toBeNull();
  });

  it("wires listSubmodules, initSubmodule, and updateSubmodule", async () => {
    const listPromise = vscodeRepoClient.listSubmodules("/repo");
    respond(1, [{ path: "deps/child", url: null, gitlinkId: null, initialized: false, headId: null, workdir: null }]);
    await expect(listPromise).resolves.toEqual([
      { path: "deps/child", url: null, gitlinkId: null, initialized: false, headId: null, workdir: null },
    ]);

    const initPromise = vscodeRepoClient.initSubmodule("/repo", "deps/child");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "init_submodule",
      params: { repoPath: "/repo", path: "deps/child" },
    });
    respond(2, null);
    await expect(initPromise).resolves.toBeNull();

    const updatePromise = vscodeRepoClient.updateSubmodule("/repo", "deps/child", true);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "update_submodule",
      params: { repoPath: "/repo", path: "deps/child", recursive: true },
    });
    respond(3, null);
    await expect(updatePromise).resolves.toBeNull();
  });

  it("wires listReflogRefs, getReflog, and restoreReflogEntry", async () => {
    const refsPromise = vscodeRepoClient.listReflogRefs("/repo");
    respond(1, ["HEAD"]);
    await expect(refsPromise).resolves.toEqual(["HEAD"]);

    const reflogPromise = vscodeRepoClient.getReflog("/repo", "HEAD");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "get_reflog",
      params: { repoPath: "/repo", reference: "HEAD" },
    });
    respond(2, [
      {
        reference: "HEAD",
        oldId: "1111111",
        newId: "2222222",
        committerName: "Test User",
        committerEmail: "test@example.com",
        timestamp: 1_725_000_000,
        message: "commit: second commit",
        summary: "second commit",
      },
    ]);
    await expect(reflogPromise).resolves.toHaveLength(1);

    const restorePromise = vscodeRepoClient.restoreReflogEntry("/repo", "HEAD", "1111111");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "restore_reflog_entry",
      params: { repoPath: "/repo", reference: "HEAD", newId: "1111111" },
    });
    respond(3, null);
    await expect(restorePromise).resolves.toBeNull();
  });

  it("wires listRemotes, listRemoteBranches, and upstream methods", async () => {
    const remotesPromise = vscodeRepoClient.listRemotes("/repo");
    respond(1, [{ name: "origin", fetchUrl: "https://example.com/r.git", pushUrl: null, authMode: null, authUsername: null }]);
    await expect(remotesPromise).resolves.toHaveLength(1);

    const branchesPromise = vscodeRepoClient.listRemoteBranches("/repo", "origin");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "list_remote_branches",
      params: { repoPath: "/repo", remoteName: "origin" },
    });
    respond(2, ["main"]);
    await expect(branchesPromise).resolves.toEqual(["main"]);

    const currentPromise = vscodeRepoClient.getCurrentUpstream("/repo");
    respond(3, null);
    await expect(currentPromise).resolves.toBeNull();

    const upstreamsPromise = vscodeRepoClient.getRemoteUpstreams("/repo", "origin");
    respond(4, []);
    await expect(upstreamsPromise).resolves.toEqual([]);
  });

  it("wires addRemote and rejects embedded credentials before posting", async () => {
    const promise = vscodeRepoClient.addRemote("/repo", "origin", "https://example.com/r.git", null);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "add_remote",
      params: { repoPath: "/repo", name: "origin", fetchUrl: "https://example.com/r.git", pushUrl: null },
    });
    respond(1, null);
    await expect(promise).resolves.toBeNull();

    expect(() =>
      vscodeRepoClient.addRemote("/repo", "origin", "https://alice:secret@example.com/r.git", null),
    ).toThrow("Remote URLs must not contain embedded credentials");
  });

  it("wires renameRemote, updateRemoteUrls, and removeRemote", async () => {
    const renamePromise = vscodeRepoClient.renameRemote("/repo", "origin", "upstream");
    respond(1, null);
    await expect(renamePromise).resolves.toBeNull();

    const updatePromise = vscodeRepoClient.updateRemoteUrls("/repo", "upstream", "https://example.com/r2.git", null);
    respond(2, null);
    await expect(updatePromise).resolves.toBeNull();

    const removePromise = vscodeRepoClient.removeRemote("/repo", "upstream", true);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "remove_remote",
      params: { repoPath: "/repo", name: "upstream", clearUpstreams: true },
    });
    respond(3, null);
    await expect(removePromise).resolves.toBeNull();
  });

  it("wires https credential and auth mode methods", async () => {
    const savePromise = vscodeRepoClient.saveHttpsCredential("/repo", "origin", "alice", "secret");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "save_https_credential",
      params: { repoPath: "/repo", remoteName: "origin", username: "alice", token: "secret" },
    });
    respond(1, null);
    await expect(savePromise).resolves.toBeNull();

    const forgetPromise = vscodeRepoClient.forgetHttpsCredential("/repo", "origin");
    respond(2, null);
    await expect(forgetPromise).resolves.toBeNull();

    const authPromise = vscodeRepoClient.setRemoteAuthMode("/repo", "origin", "HttpsToken", "alice");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "set_remote_auth_mode",
      params: { repoPath: "/repo", remoteName: "origin", mode: "HttpsToken", username: "alice" },
    });
    respond(3, null);
    await expect(authPromise).resolves.toBeNull();
  });

  it("wires setCurrentUpstream and clearCurrentUpstream", async () => {
    const setPromise = vscodeRepoClient.setCurrentUpstream("/repo", "origin", "main");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "set_current_upstream",
      params: { repoPath: "/repo", remoteName: "origin", remoteBranch: "main" },
    });
    respond(1, null);
    await expect(setPromise).resolves.toBeNull();

    const clearPromise = vscodeRepoClient.clearCurrentUpstream("/repo");
    respond(2, null);
    await expect(clearPromise).resolves.toBeNull();
  });

  it("wires listTags, createTag, and deleteTag", async () => {
    const createPromise = vscodeRepoClient.createTag("/repo", "v1.0.0", "first release", "abc123");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "create_tag",
      params: { repoPath: "/repo", name: "v1.0.0", message: "first release", target: "abc123" },
    });
    respond(1, null);
    await expect(createPromise).resolves.toBeNull();

    const listPromise = vscodeRepoClient.listTags("/repo");
    respond(2, [
      { name: "v1.0.0", targetId: "abc", commitId: "abc", annotated: true, message: "first release", taggerName: "Test User", timestamp: 1_725_000_000 },
    ]);
    await expect(listPromise).resolves.toHaveLength(1);

    const deletePromise = vscodeRepoClient.deleteTag("/repo", "v1.0.0");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "delete_tag",
      params: { repoPath: "/repo", name: "v1.0.0" },
    });
    respond(3, null);
    await expect(deletePromise).resolves.toBeNull();
  });

  function notify(method: string, params: unknown) {
    window.dispatchEvent(new MessageEvent("message", { data: { jsonrpc: "2.0", method, params } }));
  }

  const progress = (phase: string) => ({
    operationId: "fetch-1",
    operation: "Fetch",
    phase,
    errorKind: null,
    current: 0,
    total: 0,
    receivedBytes: 0,
    message: null,
    hookOutput: null,
  });

  it("wires fetchRemote, pushCurrentBranch, and pushTags", async () => {
    const fetchPromise = vscodeRepoClient.fetchRemote("/repo", "origin", false);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "fetch_remote",
      params: { repoPath: "/repo", remoteName: "origin", prune: false },
    });
    respond(1, "fetch-1");
    await expect(fetchPromise).resolves.toBe("fetch-1");

    const pushPromise = vscodeRepoClient.pushCurrentBranch("/repo", "origin", false);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "push_current_branch",
      params: { repoPath: "/repo", remoteName: "origin", noVerify: false },
    });
    respond(2, "push-1");
    await expect(pushPromise).resolves.toBe("push-1");

    const tagsPromise = vscodeRepoClient.pushTags("/repo", "origin", ["v1.0.0"], true);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "push_tags",
      params: { repoPath: "/repo", remoteName: "origin", names: ["v1.0.0"], noVerify: true },
    });
    respond(3, "push-2");
    await expect(tagsPromise).resolves.toBe("push-2");
  });

  it("wires fetchAllRemotes, pushBranch and deleteRemoteBranch", async () => {
    const all = vscodeRepoClient.fetchAllRemotes("/repo", null);
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "fetch_all_remotes",
      params: { repoPath: "/repo", prune: null },
    });
    respond(1, "fetch-all-1");
    await expect(all).resolves.toBe("fetch-all-1");

    const push = vscodeRepoClient.pushBranch("/repo", "origin", "topic", "topic", false, true);
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "push_branch",
      params: { repoPath: "/repo", remoteName: "origin", localBranch: "topic", remoteBranch: "topic", setUpstream: false, noVerify: true },
    });
    respond(2, "push-2");
    await expect(push).resolves.toBe("push-2");

    const del = vscodeRepoClient.deleteRemoteBranch("/repo", "origin", "topic", false);
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "delete_remote_branch",
      params: { repoPath: "/repo", remoteName: "origin", branchName: "topic", noVerify: false },
    });
    respond(3, "delete-remote-3");
    await expect(del).resolves.toBe("delete-remote-3");

    const delTag = vscodeRepoClient.deleteRemoteTag("/repo", "origin", "v1", true);
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 4,
      method: "delete_remote_tag",
      params: { repoPath: "/repo", remoteName: "origin", tagName: "v1", noVerify: true },
    });
    respond(4, "delete-remote-4");
    await expect(delTag).resolves.toBe("delete-remote-4");
  });

  it("wires cancelTransfer", async () => {
    const promise = vscodeRepoClient.cancelTransfer("/repo", "fetch-1");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "cancel_transfer",
      params: { repoPath: "/repo", operationId: "fetch-1" },
    });
    respond(1, null);
    await expect(promise).resolves.toBeNull();
  });

  it("wires pullCurrentUpstream", async () => {
    const promise = vscodeRepoClient.pullCurrentUpstream("/repo");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "pull_current_upstream",
      params: { repoPath: "/repo" },
    });
    respond(1, { kind: "FastForwarded", upstreamRef: "refs/remotes/origin/main" });
    await expect(promise).resolves.toEqual({
      kind: "FastForwarded",
      upstreamRef: "refs/remotes/origin/main",
    });
  });

  it("delivers hookProgress notifications to subscribed listeners and stops after unsubscribe", () => {
    const received: unknown[] = [];
    const unsubscribe = vscodeRepoClient.subscribeHookProgress((event) => received.push(event));
    const event = { repoPath: "/r", operationId: "commit-1", hook: "pre-commit", phase: "Started", source: "Commit" };

    notify("hookProgress", event);
    expect(received).toEqual([event]);

    unsubscribe();
    notify("hookProgress", event);
    expect(received).toHaveLength(1);
  });

  it("delivers transferProgress notifications to subscribed listeners and stops after unsubscribe", () => {
    const received: unknown[] = [];
    const unsubscribe = vscodeRepoClient.subscribeTransferProgress((p) => received.push(p));

    notify("transferProgress", progress("Starting"));
    expect(received).toEqual([progress("Starting")]);

    unsubscribe();
    notify("transferProgress", progress("Completed"));
    expect(received).toHaveLength(1);
  });

  it("fans a transferProgress notification out to every subscribed listener", () => {
    const first: unknown[] = [];
    const second: unknown[] = [];
    const unsubscribeFirst = vscodeRepoClient.subscribeTransferProgress((p) => first.push(p));
    vscodeRepoClient.subscribeTransferProgress((p) => second.push(p));

    notify("transferProgress", progress("Receiving"));

    expect(first).toHaveLength(1);
    expect(second).toHaveLength(1);

    // Unsubscribing one listener leaves the other subscribed.
    unsubscribeFirst();
    notify("transferProgress", progress("Completed"));
    expect(first).toHaveLength(1);
    expect(second).toHaveLength(2);
  });

  it("ignores notifications for methods it doesn't recognize", () => {
    const received: unknown[] = [];
    vscodeRepoClient.subscribeTransferProgress((p) => received.push(p));

    notify("somethingElse", { anything: true });

    expect(received).toHaveLength(0);
  });

  it("never settles a pending request from a notification, and vice versa", async () => {
    const received: unknown[] = [];
    vscodeRepoClient.subscribeTransferProgress((p) => received.push(p));
    const promise = vscodeRepoClient.fetchRemote("/repo", "origin", null);
    let settled = false;
    void promise.then(() => {
      settled = true;
    });

    // A notification has a `method` and no `id`, so it can never be matched to request 1.
    notify("transferProgress", progress("Starting"));
    await Promise.resolve();
    expect(settled).toBe(false);
    expect(received).toHaveLength(1);

    // ...and the response, which has an `id`, is never mistaken for a notification.
    respond(1, "fetch-1");
    await expect(promise).resolves.toBe("fetch-1");
    expect(received).toHaveLength(1);
  });

  it("registers the message listener even when subscribeTransferProgress runs before any call", () => {
    const received: unknown[] = [];
    vscodeRepoClient.subscribeTransferProgress((p) => received.push(p));

    notify("transferProgress", progress("Starting"));

    expect(received).toHaveLength(1);
    expect(postMessage).not.toHaveBeenCalled();
  });

  it("wires listStashes, saveStash, applyStash, and dropStash", async () => {
    const stash = { message: "m", keepIndex: false, includeUntracked: true, paths: ["a.txt"] };
    const savePromise = vscodeRepoClient.saveStash("/repo", stash);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "save_stash",
      params: { repoPath: "/repo", options: stash },
    });
    respond(1, null);
    await expect(savePromise).resolves.toBeNull();

    const listPromise = vscodeRepoClient.listStashes("/repo");
    respond(2, [{ index: 0, message: "WIP", commitId: "abc123", branch: null, timestamp: 1700000000, fileCount: 1, hasStagedChanges: false }]);
    await expect(listPromise).resolves.toHaveLength(1);

    const applyPromise = vscodeRepoClient.applyStash("/repo", 0, "abc123", true);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "apply_stash",
      params: { repoPath: "/repo", index: 0, commitId: "abc123", restoreIndex: true },
    });
    respond(3, { kind: "Applied" });
    await expect(applyPromise).resolves.toEqual({ kind: "Applied" });

    const dropPromise = vscodeRepoClient.dropStash("/repo", 0, "abc123");
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 4,
      method: "drop_stash",
      params: { repoPath: "/repo", index: 0, commitId: "abc123" },
    });
    respond(4, null);
    await expect(dropPromise).resolves.toBeNull();
  });

  it("wires popStash and getStashFiles", async () => {
    const popPromise = vscodeRepoClient.popStash("/repo", 1, "s1", false);
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "pop_stash",
      params: { repoPath: "/repo", index: 1, commitId: "s1", restoreIndex: false },
    });
    respond(1, { kind: "Conflicted", files: ["f"] });
    await expect(popPromise).resolves.toEqual({ kind: "Conflicted", files: ["f"] });

    const filesPromise = vscodeRepoClient.getStashFiles("/repo", "abc");
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "get_stash_files",
      params: { repoPath: "/repo", commitId: "abc" },
    });
    const files = { tracked: ["t"], untracked: [], untrackedCommitId: null };
    respond(2, files);
    await expect(filesPromise).resolves.toEqual(files);
  });

  it("wires getMergeOptions and setMergeOptions", async () => {
    const options = { fastForward: "Never", squash: false } as const;
    const getPromise = vscodeRepoClient.getMergeOptions("/repo");
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_merge_options",
      params: { repoPath: "/repo" },
    });
    respond(1, options);
    await expect(getPromise).resolves.toEqual(options);

    const setPromise = vscodeRepoClient.setMergeOptions("/repo", options);
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "set_merge_options",
      params: { repoPath: "/repo", options },
    });
    respond(2, null);
    await expect(setPromise).resolves.toBeNull();
  });

  it("wires getBlame", async () => {
    const promise = vscodeRepoClient.getBlame("/repo", "HEAD", "file.txt");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_blame",
      params: { repoPath: "/repo", commitId: "HEAD", path: "file.txt" },
    });
    respond(1, [
      { lineNumber: 1, content: "hello", commitId: "abc", shortId: "a", authorName: "Test", timestamp: 0 },
    ]);
    await expect(promise).resolves.toHaveLength(1);
  });

  it("wires mergeBranch onto start_merge", async () => {
    const options = { fastForward: "Only", squash: true } as const;
    const promise = vscodeRepoClient.mergeBranch("/repo", "feature", options);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "start_merge",
      params: { repoPath: "/repo", branchName: "feature", options },
    });
    respond(1, { kind: "Conflicted", files: ["file.txt"] });
    await expect(promise).resolves.toEqual({ kind: "Conflicted", files: ["file.txt"] });
  });

  it("wires getConflictHunks, resolveConflict, abortMerge, getMergeMessage, and resolveAddDeleteConflict", async () => {
    const hunksPromise = vscodeRepoClient.getConflictHunks("/repo", "file.txt");
    respond(1, [{ kind: "Conflict", ours: "a", base: "", theirs: "b" }]);
    await expect(hunksPromise).resolves.toEqual([{ kind: "Conflict", ours: "a", base: "", theirs: "b" }]);

    const resolvePromise = vscodeRepoClient.resolveConflict("/repo", "file.txt", "resolved");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "resolve_conflict",
      params: { repoPath: "/repo", path: "file.txt", resolvedContent: "resolved" },
    });
    respond(2, null);
    await expect(resolvePromise).resolves.toBeNull();

    const abortPromise = vscodeRepoClient.abortMerge("/repo");
    respond(3, null);
    await expect(abortPromise).resolves.toBeNull();

    const messagePromise = vscodeRepoClient.getMergeMessage("/repo");
    respond(4, "Merge branch 'feature'");
    await expect(messagePromise).resolves.toBe("Merge branch 'feature'");

    const resolveAddDeletePromise = vscodeRepoClient.resolveAddDeleteConflict("/repo", "file.txt", "Ours");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 5,
      method: "resolve_add_delete_conflict",
      params: { repoPath: "/repo", path: "file.txt", choice: "Ours" },
    });
    respond(5, null);
    await expect(resolveAddDeletePromise).resolves.toBeNull();
  });

  it("wires commitsSince, startRebase, rebaseContinue, and abortRebase", async () => {
    const sincePromise = vscodeRepoClient.commitsSince("/repo", "main");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "commits_since",
      params: { repoPath: "/repo", onto: "main" },
    });
    respond(1, [{ id: "abc", shortId: "a", summary: "s", authorName: "Test", timestamp: 0 }]);
    await expect(sincePromise).resolves.toHaveLength(1);

    const plan = [{ commitId: "abc", action: { kind: "Pick" as const }, combinedMessage: null }];
    const startPromise = vscodeRepoClient.startRebase("/repo", "main", plan);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "start_rebase",
      params: { repoPath: "/repo", onto: "main", plan, edit: null },
    });
    respond(2, { kind: "Done" });
    await expect(startPromise).resolves.toEqual({ kind: "Done" });

    // #473: a one-click history edit names itself for the Undo entry.
    const edit = { kind: "Drop" as const, commitId: "abc" };
    const editPromise = vscodeRepoClient.startRebase("/repo", "main", plan, edit);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 3,
      method: "start_rebase",
      params: { repoPath: "/repo", onto: "main", plan, edit },
    });
    respond(3, { kind: "Done" });
    await expect(editPromise).resolves.toEqual({ kind: "Done" });

    const continuePromise = vscodeRepoClient.rebaseContinue("/repo");
    respond(4, { kind: "Advanced" });
    await expect(continuePromise).resolves.toEqual({ kind: "Advanced" });

    const abortPromise = vscodeRepoClient.abortRebase("/repo");
    respond(5, { restored: true });
    await expect(abortPromise).resolves.toEqual({ restored: true });
  });

  it("wires detectForgeRepository, saveForgeToken, and forgetForgeToken", async () => {
    const detectPromise = vscodeRepoClient.detectForgeRepository("/repo");
    respond(1, [{ provider: "GitHub", host: "github.com", owner: "acme", name: "widget", remoteName: "origin" }]);
    await expect(detectPromise).resolves.toHaveLength(1);

    const savePromise = vscodeRepoClient.saveForgeToken("/repo", "GitHub", "alice", "secret");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "save_forge_token",
      params: { repoPath: "/repo", provider: "GitHub", account: "alice", token: "secret" },
    });
    respond(2, null);
    await expect(savePromise).resolves.toBeNull();

    const forgetPromise = vscodeRepoClient.forgetForgeToken("/repo", "GitHub", "alice");
    respond(3, null);
    await expect(forgetPromise).resolves.toBeNull();
  });

  it("wires logFrontendError onto the native log_frontend_error method", async () => {
    const promise = vscodeRepoClient.logFrontendError("Uncaught error", new Error("boom"));

    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "log_frontend_error",
      params: { context: "Uncaught error", message: "Error: boom" },
    });
    respond(1, null);
    await expect(promise).resolves.toBeNull();
  });

  it("wires listPullRequests and createPullRequest", async () => {
    const listPromise = vscodeRepoClient.listPullRequests("/repo", "origin", "alice");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "list_pull_requests",
      params: { repoPath: "/repo", remoteName: "origin", account: "alice" },
    });
    respond(1, { pullRequests: [], truncated: false });
    await expect(listPromise).resolves.toEqual({ pullRequests: [], truncated: false });

    const createPromise = vscodeRepoClient.createPullRequest("/repo", "origin", "alice", {
      title: "Add feature",
      description: null,
      sourceBranch: "feature",
      targetBranch: "main",
    });
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "create_pull_request",
      params: {
        repoPath: "/repo",
        remoteName: "origin",
        account: "alice",
        pullRequest: { title: "Add feature", description: null, sourceBranch: "feature", targetBranch: "main" },
      },
    });
    respond(2, {
      id: "1",
      number: 7,
      title: "Add feature",
      url: "https://github.com/acme/widget/pull/7",
      author: "alice",
      sourceBranch: "feature",
      targetBranch: "main",
      state: "open",
    });
    await expect(createPromise).resolves.toMatchObject({ number: 7 });
  });

  it("wires the history-rewrite, head-info and force-push commands", async () => {
    const p1 = vscodeRepoClient.getHeadInfo("/repo");
    expect(postMessage).toHaveBeenCalledWith({ jsonrpc: "2.0", id: 1, method: "get_head_info", params: { repoPath: "/repo" } });
    respond(1, null);
    await p1;
    const p2 = vscodeRepoClient.amendCommit("/repo", "msg", false);
    expect(postMessage).toHaveBeenCalledWith({ jsonrpc: "2.0", id: 2, method: "amend_commit", params: { repoPath: "/repo", message: "msg", noVerify: false, author: null, trailers: [] } });
    respond(2, null);
    await p2;
    const p3 = vscodeRepoClient.cherryPick("/repo", "abc");
    expect(postMessage).toHaveBeenCalledWith({ jsonrpc: "2.0", id: 3, method: "cherry_pick", params: { repoPath: "/repo", commitId: "abc", mainline: null } });
    respond(3, null);
    await p3;
    const p4 = vscodeRepoClient.revertCommit("/repo", "abc");
    expect(postMessage).toHaveBeenCalledWith({ jsonrpc: "2.0", id: 4, method: "revert_commit", params: { repoPath: "/repo", commitId: "abc", mainline: null } });
    respond(4, null);
    await p4;
    const p5 = vscodeRepoClient.resetToCommit("/repo", "abc", "soft");
    expect(postMessage).toHaveBeenCalledWith({ jsonrpc: "2.0", id: 5, method: "reset_to_commit", params: { repoPath: "/repo", commitId: "abc", mode: "soft" } });
    respond(5, null);
    await p5;
    const p6 = vscodeRepoClient.checkoutCommit("/repo", "abc");
    expect(postMessage).toHaveBeenCalledWith({ jsonrpc: "2.0", id: 6, method: "checkout_commit", params: { repoPath: "/repo", commitId: "abc" } });
    respond(6, null);
    await p6;
    const p7 = vscodeRepoClient.forcePushCurrentBranch("/repo", "origin", true);
    expect(postMessage).toHaveBeenCalledWith({ jsonrpc: "2.0", id: 7, method: "force_push_current_branch", params: { repoPath: "/repo", remoteName: "origin", noVerify: true } });
    respond(7, null);
    await p7;
  });

  it("wires the undo commands", async () => {
    const methods = [
      ["getUndoState", "get_undo_state"],
      ["undo", "undo_last"],
      ["redo", "redo_last"],
    ] as const;
    for (const [index, [name, method]] of methods.entries()) {
      const id = index + 1;
      const promise = vscodeRepoClient[name]("/repo");
      expect(postMessage).toHaveBeenCalledWith({ jsonrpc: "2.0", id, method, params: { repoPath: "/repo" } });
      respond(id, null);
      await promise;
    }
  });

  it("wires getCommitSignature", async () => {
    const promise = vscodeRepoClient.getCommitSignature("/repo", "abc");
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_commit_signature",
      params: { repoPath: "/repo", commitId: "abc" },
    });
    const result = { status: "good", format: "ssh", signer: "me", key: "SHA256:x", detail: null };
    respond(1, result);
    await expect(promise).resolves.toEqual(result);
  });

  it("wires getCommitGraph with the local and the remote selection", async () => {
    const promise = vscodeRepoClient.getCommitGraph("/repo", 50, { local: null, remote: ["origin/feature"] }, null);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "get_commit_graph",
      params: { repoPath: "/repo", limit: 50, selection: { local: null, remote: ["origin/feature"] }, cursor: null },
    });
    const page = { commits: [], nextCursor: null, stale: false };
    respond(1, page);
    await expect(promise).resolves.toEqual(page);
  });

  it("wires searchCommits and cancelSearch", async () => {
    const promise = vscodeRepoClient.searchCommits("/repo", "fix", { local: ["main"], remote: ["origin/x"] }, null);
    expect(postMessage).toHaveBeenCalledWith({
      jsonrpc: "2.0",
      id: 1,
      method: "search_commits",
      params: { repoPath: "/repo", query: "fix", selection: { local: ["main"], remote: ["origin/x"] }, cursor: null },
    });
    const page = { commits: [], nextCursor: null, scanned: 3 };
    respond(1, page);
    await expect(promise).resolves.toEqual(page);

    const cancel = vscodeRepoClient.cancelSearch("/repo");
    expect(postMessage).toHaveBeenLastCalledWith({
      jsonrpc: "2.0",
      id: 2,
      method: "cancel_search",
      params: { repoPath: "/repo" },
    });
    respond(2, null);
    await expect(cancel).resolves.toBeNull();
  });
});

describe("vscodeRepoClient host requests (#677, #679)", () => {
  let postMessage: ReturnType<typeof vi.fn>;
  let client: RepoClient;
  let reset: (() => void) | undefined;

  beforeEach(async () => {
    vi.resetModules();
    postMessage = vi.fn();
    (globalThis as unknown as { acquireVsCodeApi: () => { postMessage: typeof postMessage } }).acquireVsCodeApi = () => ({
      postMessage,
    });
    const clientModule = await import("./vscodeRepoClient");
    client = clientModule.vscodeRepoClient;
    reset = clientModule.__resetVscodeRepoClientForTests;
  });

  afterEach(() => {
    reset?.();
  });

  it("tells the host it listens, reports the focus, and delivers the host's requests", () => {
    const listener = vi.fn();
    const stop = client.subscribeHostRequests(listener);

    expect(postMessage).toHaveBeenCalledWith({ jsonrpc: "2.0", method: "hostReady" });
    expect(postMessage).toHaveBeenCalledWith({ jsonrpc: "2.0", method: "webviewFocus", params: { focused: expect.any(Boolean) } });
    postMessage.mockClear();
    window.dispatchEvent(new FocusEvent("blur"));
    expect(postMessage).toHaveBeenCalledWith({ jsonrpc: "2.0", method: "webviewFocus", params: { focused: expect.any(Boolean) } });

    const request = { kind: "openRepositories", paths: ["/repo"] };
    window.dispatchEvent(new MessageEvent("message", { data: { jsonrpc: "2.0", method: "hostRequest", params: request } }));
    expect(listener).toHaveBeenCalledWith(request);

    stop();
    window.dispatchEvent(new MessageEvent("message", { data: { jsonrpc: "2.0", method: "hostRequest", params: request } }));
    expect(listener).toHaveBeenCalledOnce();
  });
});
