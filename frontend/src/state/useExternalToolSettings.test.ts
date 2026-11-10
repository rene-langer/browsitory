import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useExternalToolSettings } from "./useExternalToolSettings";
import { fakeRepoClient } from "../test/fakeRepoClient";

describe("useExternalToolSettings", () => {
  it("loads, saves and reports a failed save", async () => {
    const setExternalToolSettings = vi
      .fn()
      .mockResolvedValueOnce(undefined)
      .mockRejectedValueOnce(new Error("disk full"));
    const client = fakeRepoClient({
      getExternalToolSettings: async () => ({ editor: "code", terminal: null }),
      setExternalToolSettings,
    });
    const { result } = renderHook(() => useExternalToolSettings(client));
    expect(result.current.settings).toBeNull();
    await waitFor(() => expect(result.current.settings).toEqual({ editor: "code", terminal: null }));
    await act(async () => expect(await result.current.save({ editor: null, terminal: "kitty" })).toBe(true));
    expect(setExternalToolSettings).toHaveBeenCalledWith({ editor: null, terminal: "kitty" });
    expect(result.current.settings).toEqual({ editor: null, terminal: "kitty" });
    await act(async () => expect(await result.current.save({ editor: "x", terminal: null })).toBe(false));
    expect(result.current.error).toBe("disk full");
    expect(result.current.settings).toEqual({ editor: null, terminal: "kitty" });
  });

  it("falls back to defaults and shows the error when loading fails", async () => {
    const client = fakeRepoClient({
      getExternalToolSettings: async () => {
        throw new Error("config unreadable");
      },
    });
    const { result } = renderHook(() => useExternalToolSettings(client));
    await waitFor(() => expect(result.current.settings).toEqual({ editor: null, terminal: null }));
    expect(result.current.error).toBe("config unreadable");
  });
});
