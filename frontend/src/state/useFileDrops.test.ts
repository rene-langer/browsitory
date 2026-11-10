import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { FileDropEvent } from "../ipc/RepoClient";
import { fakeRepoClient } from "../test/fakeRepoClient";
import { isPatchPath, useFileDrops } from "./useFileDrops";

describe("useFileDrops (#649)", () => {
  it("treats .patch, .diff and .mbox files as patches, in any case", () => {
    for (const path of ["/a/fix.patch", "C:\\x\\B.DIFF", "/m/series.mbox"]) expect(isPatchPath(path)).toBe(true);
    for (const path of ["/repos/app", "/a/notes.txt", "/a/patch", "/a/fix.patch.bak"]) expect(isPatchPath(path)).toBe(false);
  });

  it("splits a drop into folders and patches, and ignores an empty one", () => {
    let emit: (event: FileDropEvent) => void = () => {};
    const unsubscribe = vi.fn();
    const client = fakeRepoClient({
      subscribeFileDrops: (listener: (event: FileDropEvent) => void) => {
        emit = listener;
        return unsubscribe;
      },
    });
    const onDrop = vi.fn();
    const { result, unmount } = renderHook(() => useFileDrops(client, onDrop));
    expect(result.current).toBe(false);
    act(() => emit({ kind: "over" }));
    expect(result.current).toBe(true);
    act(() => emit({ kind: "drop", paths: ["/r/a", "/p/1.patch", "/r/b", "/p/2.diff"] }));
    expect(result.current).toBe(false);
    expect(onDrop).toHaveBeenCalledExactlyOnceWith({ folders: ["/r/a", "/r/b"], patches: ["/p/1.patch", "/p/2.diff"] });
    act(() => emit({ kind: "drop", paths: [] }));
    expect(onDrop).toHaveBeenCalledOnce();
    unmount();
    expect(unsubscribe).toHaveBeenCalledOnce();
  });
});
