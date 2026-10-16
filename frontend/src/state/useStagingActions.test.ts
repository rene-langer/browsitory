import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { useStagingActions } from "./useStagingActions";

function setup() {
  const client = {
    stageHunk: vi.fn(async () => {}),
    stageLines: vi.fn(async () => {}),
    unstageLines: vi.fn(async () => {}),
    discardLines: vi.fn(async () => {}),
    discardFile: vi.fn(async () => {}),
    discardAllUnstaged: vi.fn(async () => {}),
    ignorePath: vi.fn(async () => {}),
  };
  const runMutation = (async (fn: () => Promise<unknown>) => {
    await fn();
  }) as never;
  const { result } = renderHook(() =>
    useStagingActions(client as unknown as RepoClient, "/repo", runMutation, vi.fn() as never, vi.fn()),
  );
  return { client, actions: result.current };
}

describe("useStagingActions", () => {
  it("passes contextLines through hunk actions", async () => {
    const { client, actions } = setup();
    await actions.stageHunk("f.txt", 1, 2, 5);
    expect(client.stageHunk).toHaveBeenCalledWith("/repo", "f.txt", 1, 2, 5);
  });

  it("line actions call through with repoPath", async () => {
    const { client, actions } = setup();
    await actions.stageLines("f.txt", 1, 2, 5, [3], ["x\n"]);
    await actions.unstageLines("f.txt", 1, 2, 5, [3], ["x\n"]);
    await actions.discardLines("f.txt", 1, 2, 5, [3], ["x\n"]);
    expect(client.stageLines).toHaveBeenCalledWith("/repo", "f.txt", 1, 2, 5, [3], ["x\n"]);
    expect(client.unstageLines).toHaveBeenCalledWith("/repo", "f.txt", 1, 2, 5, [3], ["x\n"]);
    expect(client.discardLines).toHaveBeenCalledWith("/repo", "f.txt", 1, 2, 5, [3], ["x\n"]);
  });

  it("file, discard-all and ignore actions call through", async () => {
    const { client, actions } = setup();
    await actions.discardFile("f.txt");
    await actions.discardAllUnstaged();
    await actions.ignorePath("*.log");
    expect(client.discardFile).toHaveBeenCalledWith("/repo", "f.txt");
    expect(client.discardAllUnstaged).toHaveBeenCalledWith("/repo");
    expect(client.ignorePath).toHaveBeenCalledWith("/repo", "*.log");
  });
});
