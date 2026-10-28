import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { MergeToolOutcome, RepoClient } from "../ipc/RepoClient";
import { useExternalTools } from "./useExternalTools";

function client(overrides: Partial<RepoClient>): RepoClient {
  return overrides as RepoClient;
}

describe("useExternalTools", () => {
  it("forwards launches with the repo path and shows a failure", async () => {
    const openInEditor = vi.fn(async () => {
      throw new Error(
        "No editor is configured, and opening a.exe with the system default app could run it. Set an editor command in Preferences.",
      );
    });
    const { result } = renderHook(() => useExternalTools(client({ openInEditor }), "/r", [], async () => {}));
    act(() => result.current.openInEditor("a.exe", null));
    await waitFor(() => expect(result.current.error).toMatch(/^No editor is configured/));
    expect(openInEditor).toHaveBeenCalledWith("/r", "a.exe", null);
    act(() => result.current.dismissError());
    expect(result.current.error).toBeNull();
  });

  it("forwards reveal, terminal and diff tool launches", async () => {
    const revealInFileManager = vi.fn(async () => {});
    const openTerminal = vi.fn(async () => {});
    const openDiffTool = vi.fn(async () => {});
    const { result } = renderHook(() =>
      useExternalTools(client({ revealInFileManager, openTerminal, openDiffTool }), "/r", [], async () => {}),
    );
    act(() => {
      result.current.reveal(null);
      result.current.openTerminal();
      result.current.openDiffTool("a.ts", { kind: "Staged" });
    });
    await waitFor(() => expect(openDiffTool).toHaveBeenCalledWith("/r", "a.ts", { kind: "Staged" }));
    expect(revealInFileManager).toHaveBeenCalledWith("/r", null);
    expect(openTerminal).toHaveBeenCalledWith("/r");
    expect(result.current.error).toBeNull();
  });

  it("tracks a running merge tool, records its outcome and refreshes after it exits", async () => {
    let finish: (v: MergeToolOutcome) => void = () => {};
    const openMergeTool = vi.fn(() => new Promise<MergeToolOutcome>((resolve) => (finish = resolve)));
    const refresh = vi.fn(async () => {});
    const { result } = renderHook(() => useExternalTools(client({ openMergeTool }), "/r", ["a.txt"], refresh));
    act(() => result.current.openMergeTool("a.txt"));
    expect(result.current.mergeToolRunning.has("a.txt")).toBe(true);
    await act(async () => finish("Resolved"));
    expect(result.current.mergeToolRunning.has("a.txt")).toBe(false);
    expect(result.current.mergeToolResults.get("a.txt")).toBe("Resolved");
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("refreshes and shows the error when the merge tool fails", async () => {
    const openMergeTool = vi.fn(async (): Promise<MergeToolOutcome> => {
      throw new Error("The merge tool failed (exit 127): sh: meld: not found");
    });
    const refresh = vi.fn(async () => {});
    const { result } = renderHook(() => useExternalTools(client({ openMergeTool }), "/r", ["a.txt"], refresh));
    await act(async () => result.current.openMergeTool("a.txt"));
    expect(result.current.error).toBe("The merge tool failed (exit 127): sh: meld: not found");
    expect(result.current.mergeToolRunning.size).toBe(0);
    expect(result.current.mergeToolResults.size).toBe(0);
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("records nothing for a merge editor handed off to VSCode", async () => {
    const openMergeTool = vi.fn(async () => "HandedOff" as const);
    const { result } = renderHook(() => useExternalTools(client({ openMergeTool }), "/r", ["a.txt"], async () => {}));
    await act(async () => result.current.openMergeTool("a.txt"));
    expect(result.current.mergeToolResults.size).toBe(0);
  });

  it("drops a result once the path is no longer conflicted, and on dismiss", async () => {
    const openMergeTool = vi.fn(async () => "Unresolved" as const);
    const { result, rerender } = renderHook(
      ({ conflicted }) => useExternalTools(client({ openMergeTool }), "/r", conflicted, async () => {}),
      { initialProps: { conflicted: ["a.txt", "b.txt"] } },
    );
    await act(async () => result.current.openMergeTool("a.txt"));
    await act(async () => result.current.openMergeTool("b.txt"));
    expect(result.current.mergeToolResults.get("a.txt")).toBe("Unresolved");
    rerender({ conflicted: ["b.txt"] });
    expect([...result.current.mergeToolResults.keys()]).toEqual(["b.txt"]);
    act(() => result.current.dismissMergeToolResult("b.txt"));
    expect(result.current.mergeToolResults.size).toBe(0);
  });
});
