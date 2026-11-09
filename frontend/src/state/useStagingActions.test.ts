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
    restoreFileVersion: vi.fn(async () => {}),
    discardAllUnstaged: vi.fn(async () => {}),
    ignorePath: vi.fn(async () => {}),
  };
  const runMutation = (async (fn: () => Promise<unknown>) => {
    await fn();
  }) as never;
  const { result } = renderHook(() =>
    useStagingActions(client as unknown as RepoClient, "/repo", runMutation, vi.fn() as never, vi.fn() as never, vi.fn()),
  );
  return { client, actions: result.current };
}

const HUNK = { contextLines: 5, algorithm: "Patience" } as const;

describe("useStagingActions", () => {
  it("passes the hunk options through hunk actions", async () => {
    const { client, actions } = setup();
    await actions.stageHunk("f.txt", 1, 2, HUNK);
    expect(client.stageHunk).toHaveBeenCalledWith("/repo", "f.txt", 1, 2, HUNK);
  });

  it("line actions call through with repoPath", async () => {
    const { client, actions } = setup();
    await actions.stageLines("f.txt", 1, 2, HUNK, [3], ["x\n"]);
    await actions.unstageLines("f.txt", 1, 2, HUNK, [3], ["x\n"]);
    await actions.discardLines("f.txt", 1, 2, HUNK, [3], ["x\n"]);
    expect(client.stageLines).toHaveBeenCalledWith("/repo", "f.txt", 1, 2, HUNK, [3], ["x\n"]);
    expect(client.unstageLines).toHaveBeenCalledWith("/repo", "f.txt", 1, 2, HUNK, [3], ["x\n"]);
    expect(client.discardLines).toHaveBeenCalledWith("/repo", "f.txt", 1, 2, HUNK, [3], ["x\n"]);
  });

  it("file, discard-all, ignore and restore actions call through", async () => {
    const { client, actions } = setup();
    await actions.discardFile("f.txt");
    await actions.discardAllUnstaged(["a.txt", "u.txt"]);
    await actions.ignorePath("*.log");
    await actions.restoreFileVersion("abc", "src/old.txt");
    expect(client.restoreFileVersion).toHaveBeenCalledWith("/repo", "abc", "src/old.txt");
    expect(client.discardFile).toHaveBeenCalledWith("/repo", "f.txt");
    expect(client.discardAllUnstaged).toHaveBeenCalledWith("/repo", ["a.txt", "u.txt"]);
    expect(client.ignorePath).toHaveBeenCalledWith("/repo", "*.log");
  });
});
