import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RepoClient } from "./RepoClient";

describe("vscodeRepoClient", () => {
  let postMessage: ReturnType<typeof vi.fn>;
  let vscodeRepoClient: RepoClient;

  beforeEach(async () => {
    vi.resetModules();
    postMessage = vi.fn();
    (
      globalThis as unknown as { acquireVsCodeApi: () => { postMessage: typeof postMessage } }
    ).acquireVsCodeApi = () => ({ postMessage });
    ({ vscodeRepoClient } = await import("./vscodeRepoClient"));
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
    respond(1, [{ path: "a.txt", staged: false, kind: "New" }]);

    await expect(promise).resolves.toEqual([{ path: "a.txt", staged: false, kind: "New" }]);
  });

  it("rejects when the sidecar returns a JSON-RPC error", async () => {
    const promise = vscodeRepoClient.getStatus("/missing");

    respondWithError(1, "repo not open: /missing");

    await expect(promise).rejects.toThrow("repo not open: /missing");
  });

  it("correlates concurrent requests by id", async () => {
    const openPromise = vscodeRepoClient.openRepo("/repo");
    const statusPromise = vscodeRepoClient.getStatus("/repo");

    respond(2, [{ path: "b.txt", staged: true, kind: "Modified" }]);
    respond(1, null);

    // A void-returning JSON-RPC method's wire response carries `result: null` (JSON has no
    // `undefined`); the client resolves with that `null` as-is rather than coercing it.
    await expect(openPromise).resolves.toBeNull();
    await expect(statusPromise).resolves.toEqual([
      { path: "b.txt", staged: true, kind: "Modified" },
    ]);
  });

  it("rejects unwired methods without touching postMessage", async () => {
    await expect(vscodeRepoClient.listBranches("/repo")).rejects.toThrow(
      "listBranches is not implemented yet",
    );
    expect(postMessage).not.toHaveBeenCalled();
  });
});
