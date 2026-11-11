import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { RepoError, type GraphCommit, type RepoClient, type StatusEntry, type TagInfo } from "../ipc/RepoClient";
import { useAppState } from "./useAppState";
import { TEST_REPO_PATH, remote, upstream, transferClient } from "./useAppStateTestClient";

describe("useAppState", () => {
  it("reports a failed remote-auth mutation to dependent callers", async () => {
    const client = transferClient({
      setRemoteAuthMode: async () => {
        throw new RepoError("CredentialStoreFailure", "credential keychain failure: locked");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let configured = true;
    await act(async () => {
      configured = await result.current.setRemoteAuthMode("origin", "HttpsToken", "rene");
    });

    expect(configured).toBe(false);
    expect(result.current.state.error).toBe("The operating-system credential store is unavailable. Unlock it and try again.");
  });

  // #527: a forge token's keychain failure gets the same remediation as a remote's HTTPS token.
  it.each([
    ["saving", (state: ReturnType<typeof useAppState>) => state.saveForgeToken("GitHub", "alice", "secret")],
    ["forgetting", (state: ReturnType<typeof useAppState>) => state.forgetForgeToken("GitHub", "alice")],
  ] as const)("shows the credential store remediation when %s a forge token fails", async (_, run) => {
    const keychain = async () => {
      throw new RepoError("CredentialStoreFailure", "credential keychain failure: locked");
    };
    const client = transferClient({ saveForgeToken: keychain, forgetForgeToken: keychain });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(async () => {
      await run(result.current);
    });

    expect(result.current.state.error).toBe("The operating-system credential store is unavailable. Unlock it and try again.");
  });

  // AUD-2026-09-05-CONC-002: the per-repo worker thread can die mid-session, and every
  // `WorkerHandle` method in `crates/repo-service/src/worker/*` then rejects every subsequent
  // call on that repo with the `WorkerStopped` kind (#496). Retrying the same action can never
  // help there (the worker is gone for good), so it should surface as a distinct reconnect
  // message instead of the raw text riding the generic error path unchanged.
  it.each([
    "worker thread stopped",
    "worker thread stopped before replying",
  ])("reports a %s failure as a reconnect message, not the raw worker error", async (workerError) => {
    const client = transferClient({
      setRemoteAuthMode: async () => {
        throw new RepoError("WorkerStopped", workerError);
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let configured = true;
    await act(async () => {
      configured = await result.current.setRemoteAuthMode("origin", "HttpsToken", "rene");
    });

    expect(configured).toBe(false);
    expect(result.current.state.error).toBe(
      "Connection to this repository was lost. Close and reopen the repository to reconnect.",
    );
    expect(result.current.state.error).not.toBe(workerError);
  });

  // #496: the kind decides, not the text.
  it("shows a failure that only mentions a stopped worker as it is", async () => {
    const client = transferClient({
      setRemoteAuthMode: async () => {
        throw new RepoError("Other", "hook output: worker thread stopped");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(async () => {
      await result.current.setRemoteAuthMode("origin", "HttpsToken", "rene");
    });

    expect(result.current.state.error).toBe("hook output: worker thread stopped");
  });

  it("reports a worker-death failure hit during refresh as the same reconnect message", async () => {
    const client = transferClient({
      getStatus: async () => {
        throw new RepoError("WorkerStopped", "worker thread stopped before replying");
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());

    expect(result.current.state.error).toBe(
      "Connection to this repository was lost. Close and reopen the repository to reconnect.",
    );
  });

  it("loads the undo state on refresh", async () => {
    const client = transferClient({ getUndoState: async () => ({ undo: "Stage a.txt", redo: null }) });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    expect(result.current.state.undoState).toEqual({ undo: "Stage a.txt", redo: null });
  });

  it("advances refreshGeneration on every refresh, successful or not", async () => {
    let fail = false;
    const client = transferClient({
      getStatus: async () => {
        if (fail) throw new Error("boom");
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    expect(result.current.state.refreshGeneration).toBe(0);

    await act(() => result.current.refresh());
    expect(result.current.state.refreshGeneration).toBe(1);

    fail = true;
    await act(() => result.current.refresh());
    expect(result.current.state.refreshGeneration).toBe(2);
  });

  it("forwards a credential token directly to the client without placing it in state", async () => {
    let saved: [string, string, string] | null = null;
    const client = transferClient({
      saveHttpsCredential: async (_repoPath, remoteName, username, token) => {
        saved = [remoteName, username, token];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.saveHttpsCredential("origin", "rene", "token-123"));

    expect(saved).toEqual(["origin", "rene", "token-123"]);
    expect(JSON.stringify(result.current.state)).not.toContain("token-123");
  });

  it("refreshes tags after creating and deleting a tag", async () => {
    const release: TagInfo = {
      name: "v1.0.0",
      targetId: "abc123",
      commitId: "abc123",
      annotated: true,
      message: "first release",
      taggerName: "Test User",
      timestamp: 0,
    };
    let tags: TagInfo[] = [];
    const client = transferClient({
      listTags: async () => tags,
      createTag: async () => {
        tags = [release];
      },
      deleteTag: async () => {
        tags = [];
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    await act(() => result.current.createTag("v1.0.0", "first release", null));
    expect(result.current.state.tags).toEqual([release]);

    await act(() => result.current.deleteTag("v1.0.0"));
    expect(result.current.state.tags).toEqual([]);
  });

  it("refresh loads the saved branch selection and passes it to getCommitGraph", async () => {
    let receivedSelection: string[] | null | undefined;
    const client = transferClient({
      getGraphBranchSelection: async () => ["main"],
      getCommitGraph: async (_repoPath, _limit, selection) => {
        receivedSelection = selection.local;
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());

    expect(receivedSelection).toEqual(["main"]);
    expect(result.current.state.graphSelection.local).toEqual(["main"]);
  });

  it("loadMoreHistory refetches the graph with a larger limit", async () => {
    const limits: number[] = [];
    const client = transferClient({
      getCommitGraph: async (_repoPath, limit) => {
        limits.push(limit);
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    await act(() => result.current.loadMoreHistory());

    expect(limits).toEqual([300, 600]);
    expect(result.current.state.graphLimit).toBe(600);
  });

  it("loadHistoryThrough grows the limit in whole pages to reach a graph row (#441)", async () => {
    const limits: number[] = [];
    const client = transferClient({
      getCommitGraph: async (_repoPath, limit) => {
        limits.push(limit);
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    await act(() => result.current.loadHistoryThrough(700));
    // Already loaded far enough: reloads (the graph may be stale) without shrinking the limit.
    await act(() => result.current.loadHistoryThrough(5));

    expect(limits).toEqual([300, 900, 900]);
    expect(result.current.state.graphLimit).toBe(900);
  });

  it("reports more history only when the graph filled its limit", async () => {
    const commitFor = (n: number) => ({
      id: `id${n}`,
      shortId: `id${n}`,
      summary: `c${n}`,
      authorName: "a",
      authorEmail: "a@b",
      timestamp: n,
      parentIds: [],
      branchRefs: [],
      remoteBranchRefs: [],
    });
    const full = Array.from({ length: 300 }, (_, n) => commitFor(n));
    const client = transferClient({ getCommitGraph: async (_repoPath, limit) => full.slice(0, limit) });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    expect(result.current.state.hasMoreHistory).toBe(true);

    const short = transferClient({ getCommitGraph: async () => full.slice(0, 10) });
    const { result: shortResult } = renderHook(() => useAppState(short, TEST_REPO_PATH));
    await act(() => shortResult.current.refresh());
    expect(shortResult.current.state.hasMoreHistory).toBe(false);
  });

  it("setGraphBranchSelection persists the selection and refreshes the graph with it", async () => {
    let saved: string[] | undefined;
    let selectionForGraph: string[] | null | undefined;
    const client = transferClient({
      setGraphBranchSelection: async (_repoPath, branches) => {
        saved = branches;
      },
      getGraphBranchSelection: async () => saved ?? null,
      getCommitGraph: async (_repoPath, _limit, selection) => {
        selectionForGraph = selection.local;
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.setGraphBranchSelection(["feature"]));

    expect(saved).toEqual(["feature"]);
    expect(selectionForGraph).toEqual(["feature"]);
    expect(result.current.state.graphSelection.local).toEqual(["feature"]);
  });

  it("setGraphRemoteBranchSelection persists the selection and reloads it on refresh", async () => {
    let saved: string[] | undefined;
    const client = transferClient({
      setGraphRemoteBranchSelection: async (_repoPath, branches) => {
        saved = branches;
      },
      getGraphRemoteBranchSelection: async () => saved ?? null,
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.setGraphRemoteBranchSelection(["origin/feature"]));

    expect(saved).toEqual(["origin/feature"]);
    expect(result.current.state.graphSelection.remote).toEqual(["origin/feature"]);
  });

  it("setGraphRemoteBranchSelection leaves getCommitGraph's local-selection arg untouched", async () => {
    const selectedBranchesSeen: (string[] | null | undefined)[] = [];
    const client = transferClient({
      getGraphBranchSelection: async () => ["main"],
      setGraphRemoteBranchSelection: async () => {},
      getCommitGraph: async (_repoPath, _limit, selection) => {
        selectedBranchesSeen.push(selection.local);
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    await act(() => result.current.setGraphRemoteBranchSelection(["origin/feature"]));

    // The remote-branch selection is a separate axis from the local `selection.local`
    // `getCommitGraph` is called with — the two refreshes above must pass the identical local
    // selection regardless of the remote-selection mutation in between.
    expect(selectedBranchesSeen).toEqual([["main"], ["main"]]);
  });

  it("passes the saved remote selection to getCommitGraph, and none when nothing is saved (#385)", async () => {
    let savedRemote: string[] | null = null;
    const remoteSeen: string[][] = [];
    const client = transferClient({
      getGraphRemoteBranchSelection: async () => savedRemote,
      setGraphRemoteBranchSelection: async (_repoPath, branches) => {
        savedRemote = branches;
      },
      getCommitGraph: async (_repoPath, _limit, selection) => {
        remoteSeen.push(selection.remote);
        return [];
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    await act(() => result.current.setGraphRemoteBranchSelection(["origin/feature"]));

    expect(remoteSeen).toEqual([[], ["origin/feature"]]);
  });

  describe("isolateGraphBranch", () => {
    function isolateClient(initialLocal: string[] | null, initialRemote: string[] | null) {
      const saved = { local: initialLocal, remote: initialRemote };
      const writes: string[] = [];
      const graphCalls: [string[] | null, string[]][] = [];
      const client = transferClient({
        getGraphBranchSelection: async () => saved.local,
        getGraphRemoteBranchSelection: async () => saved.remote,
        setGraphBranchSelection: async (_repoPath, branches) => {
          writes.push(`local:${branches.join(",")}`);
          saved.local = branches;
        },
        setGraphRemoteBranchSelection: async (_repoPath, branches) => {
          writes.push(`remote:${branches.join(",")}`);
          saved.remote = branches;
        },
        getCommitGraph: async (_repoPath, _limit, selection) => {
          graphCalls.push([selection.local, selection.remote]);
          return [];
        },
      });
      return { client, writes, graphCalls };
    }

    it("on a remote branch selects only it, with no local branch", async () => {
      const { client, writes, graphCalls } = isolateClient(null, null);
      const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
      await act(() => result.current.refresh());

      await act(() => result.current.isolateGraphBranch("remote", "origin/feature"));

      expect(writes).toEqual(["local:", "remote:origin/feature"]);
      expect(graphCalls.at(-1)).toEqual([[], ["origin/feature"]]);
    });

    it("on a local branch clears a saved remote selection, so no remote branch joins the walk", async () => {
      const { client, writes, graphCalls } = isolateClient(null, ["origin/feature"]);
      const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
      await act(() => result.current.refresh());

      await act(() => result.current.isolateGraphBranch("local", "main"));

      expect(writes).toEqual(["local:main", "remote:"]);
      expect(graphCalls.at(-1)).toEqual([["main"], []]);
    });

    it("on a local branch keeps a never-saved remote selection (every remote badge stays)", async () => {
      const { client, writes, graphCalls } = isolateClient(null, null);
      const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
      await act(() => result.current.refresh());

      await act(() => result.current.isolateGraphBranch("local", "main"));

      expect(writes).toEqual(["local:main"]);
      expect(graphCalls.at(-1)).toEqual([["main"], []]);
      expect(result.current.state.graphSelection.remote).toBeNull();
    });
  });

  // #402: going back to the default forgets the saved selections; it doesn't save today's names.
  describe("clearing the graph selection", () => {
    function clearClient(initialLocal: string[] | null, initialRemote: string[] | null) {
      const saved = { local: initialLocal, remote: initialRemote };
      const writes: string[] = [];
      const graphCalls: [string[] | null, string[]][] = [];
      const client = transferClient({
        getGraphBranchSelection: async () => saved.local,
        getGraphRemoteBranchSelection: async () => saved.remote,
        setGraphBranchSelection: async (_repoPath, branches) => {
          writes.push(`local:${branches.join(",")}`);
          saved.local = branches;
        },
        setGraphRemoteBranchSelection: async (_repoPath, branches) => {
          writes.push(`remote:${branches.join(",")}`);
          saved.remote = branches;
        },
        clearGraphBranchSelection: async () => {
          writes.push("clear local");
          saved.local = null;
        },
        clearGraphRemoteBranchSelection: async () => {
          writes.push("clear remote");
          saved.remote = null;
        },
        getCommitGraph: async (_repoPath, _limit, selection) => {
          graphCalls.push([selection.local, selection.remote]);
          return [];
        },
      });
      return { client, writes, graphCalls };
    }

    it("showAllGraphBranches forgets both selections and reloads the default graph once", async () => {
      const { client, writes, graphCalls } = clearClient(["main"], ["origin/feature"]);
      const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
      await act(() => result.current.refresh());
      const callsBefore = graphCalls.length;

      await act(() => result.current.showAllGraphBranches());

      expect(writes).toEqual(["clear local", "clear remote"]);
      expect(graphCalls.slice(callsBefore)).toEqual([[null, []]]);
      expect(result.current.state.graphSelection.local).toBeNull();
      expect(result.current.state.graphSelection.remote).toBeNull();
    });

    it("clearGraphBranchSelection forgets only the local selection", async () => {
      const { client, writes, graphCalls } = clearClient([], ["origin/feature"]);
      const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
      await act(() => result.current.refresh());

      await act(() => result.current.clearGraphBranchSelection());

      expect(writes).toEqual(["clear local"]);
      expect(graphCalls.at(-1)).toEqual([null, ["origin/feature"]]);
      expect(result.current.state.graphSelection.local).toBeNull();
      expect(result.current.state.graphSelection.remote).toEqual(["origin/feature"]);
    });
  });

  it("refresh populates repository state including remotes and upstream", async () => {
    const entry: StatusEntry = { path: "a.txt", oldPath: null, staged: false, kind: "Modified" };
    const graphCommit: GraphCommit = {
      id: "abc123",
      shortId: "abc123",
      summary: "initial commit",
      authorName: "Author",
      authorEmail: "author@example.com",
      timestamp: 0,
      parentIds: [],
      branchRefs: [],
      remoteBranchRefs: [],
    };
    const client: RepoClient = transferClient({
      getStatus: async () => [entry],
      getCommitGraph: async () => [graphCommit],
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());

    expect(result.current.state.repoPath).toBe(TEST_REPO_PATH);
    expect(result.current.state.status.length).toBe(1);
    expect(result.current.state.commits.length).toBe(1);
    expect(result.current.state.remotes).toEqual([remote]);
    expect(result.current.state.upstream).toEqual(upstream);
    expect(result.current.state.remoteUpstreams).toEqual({ origin: [upstream] });
    expect(result.current.state.selectedRow).toBe("uncommitted");
  });

  it("selectRow updates selectedRow without refetching", async () => {
    let getStatusCalls = 0;
    const client: RepoClient = transferClient({
      getStatus: async () => {
        getStatusCalls += 1;
        return [];
      },
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    await act(() => result.current.refresh());
    expect(getStatusCalls).toBe(1);

    act(() => result.current.selectRow({ commitId: "abc123" }));

    expect(result.current.state.selectedRow).toEqual({ commitId: "abc123" });
    expect(getStatusCalls).toBe(1);
  });

  it("pending is true only while a mutation is in flight", async () => {
    let resolveStage: (() => void) | null = null;
    const client: RepoClient = transferClient({
      stageFile: async () =>
        new Promise<void>((resolve) => {
          resolveStage = resolve;
        }),
    });

    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));
    await act(() => result.current.refresh());
    expect(result.current.state.pending).toBe(false);

    let mutationPromise!: Promise<void>;
    act(() => {
      mutationPromise = result.current.stageFile("a.txt");
    });
    expect(result.current.state.pending).toBe(true);

    await act(async () => {
      resolveStage?.();
      await mutationPromise;
    });

    expect(result.current.state.pending).toBe(false);
  });

  // #342: replies can arrive out of order (the VSCode sidecar answers each request on its own
  // thread). A read sent before a commit that answers after the commit's refresh must not put
  // the pre-commit state back.
  it("keeps the commit's undo entry when a focus re-check sent before the commit answers late", async () => {
    let undo: string | null = "Stage a.txt";
    let answerFocusCheck: (() => void) | null = null;
    let holdNextUndoRead = false;
    const client = transferClient({
      getUndoState: async () => {
        const state = { undo, redo: null };
        if (!holdNextUndoRead) return state;
        holdNextUndoRead = false;
        await new Promise<void>((resolve) => {
          answerFocusCheck = resolve;
        });
        return state;
      },
      commit: async () => {
        undo = "Commit “x”";
        return { kind: "Committed", commitId: "c1" };
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    holdNextUndoRead = true;
    let focusCheck!: Promise<void>;
    act(() => {
      focusCheck = result.current.refreshUndoState();
    });
    await act(async () => {
      await result.current.commit("x", false);
    });
    expect(result.current.state.undoState.undo).toBe("Commit “x”");

    await act(async () => {
      answerFocusCheck?.();
      await focusCheck;
    });
    expect(result.current.state.undoState.undo).toBe("Commit “x”");
  });

  // #222: one refresh runs and one waits; calls made meanwhile share the waiting one, which reads
  // after the running one ends.
  it("coalesces refreshes called while one runs into one that reads afterwards", async () => {
    const staged: StatusEntry = { path: "a.txt", oldPath: null, kind: "New", staged: true };
    let status: StatusEntry[] = [];
    let statusReads = 0;
    let answerFirst: (() => void) | null = null;
    const client = transferClient({
      getStatus: async () => {
        statusReads += 1;
        const snapshot = status;
        if (statusReads === 1) {
          await new Promise<void>((resolve) => {
            answerFirst = resolve;
          });
        }
        return snapshot;
      },
    });
    const { result } = renderHook(() => useAppState(client, TEST_REPO_PATH));

    let first!: Promise<void>;
    act(() => {
      first = result.current.refresh();
    });
    await act(async () => {
      while (answerFirst === null) await Promise.resolve();
    });
    status = [staged];
    let second!: Promise<void>;
    let third!: Promise<void>;
    act(() => {
      second = result.current.refresh();
      third = result.current.refresh();
    });
    expect(second).toBe(third);
    // The waiting refresh doesn't start while the first one runs.
    await act(async () => {
      await Promise.resolve();
    });
    expect(statusReads).toBe(1);

    await act(async () => {
      answerFirst?.();
      await Promise.all([first, second, third]);
    });
    expect(statusReads).toBe(2);
    expect(result.current.state.status).toEqual([staged]);
    expect(result.current.state.refreshGeneration).toBe(2);
  });
});
