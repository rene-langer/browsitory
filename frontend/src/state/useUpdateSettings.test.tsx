import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { RepoClient } from "../ipc/RepoClient";
import { useUpdateSettings, useUpdateSettingsContext } from "./useUpdateSettings";
import { fakeRepoClient } from "../test/fakeRepoClient";

function clientWith(overrides: Partial<RepoClient>): RepoClient {
  return fakeRepoClient({
    getUpdateSettings: vi.fn(async () => ({ checkAutomatically: true })),
    setUpdateSettings: vi.fn(async () => {}),
    ...overrides,
  });
}

describe("useUpdateSettings (#630)", () => {
  it("is loading with the default (on) until the saved settings answer", async () => {
    const client = clientWith({ getUpdateSettings: vi.fn(async () => ({ checkAutomatically: false })) });
    const { result } = renderHook(() => useUpdateSettings(client));
    expect(result.current.status).toBe("loading");
    expect(result.current.settings).toEqual({ checkAutomatically: true });
    await waitFor(() => expect(result.current.status).toBe("loaded"));
    expect(result.current.settings).toEqual({ checkAutomatically: false });
  });

  it("reports a failed read, keeping the default", async () => {
    const client = clientWith({
      getUpdateSettings: vi.fn(async () => {
        throw new Error("broken config.toml");
      }),
    });
    const { result } = renderHook(() => useUpdateSettings(client));
    await waitFor(() => expect(result.current.status).toBe("failed"));
    expect(result.current.settings).toEqual({ checkAutomatically: true });
  });

  it("saves a change and counts it as loaded", async () => {
    const client = clientWith({
      getUpdateSettings: vi.fn(async () => {
        throw new Error("broken config.toml");
      }),
    });
    const { result } = renderHook(() => useUpdateSettings(client));
    await waitFor(() => expect(result.current.status).toBe("failed"));
    act(() => result.current.update({ checkAutomatically: false }));
    expect(result.current.status).toBe("loaded");
    expect(result.current.settings).toEqual({ checkAutomatically: false });
    await waitFor(() => expect(client.setUpdateSettings).toHaveBeenLastCalledWith({ checkAutomatically: false }));
  });

  it("reverts and reports when saving fails", async () => {
    const client = clientWith({
      setUpdateSettings: vi.fn(async () => {
        throw new Error("read-only");
      }),
    });
    const { result } = renderHook(() => useUpdateSettings(client));
    await waitFor(() => expect(result.current.status).toBe("loaded"));
    act(() => result.current.update({ checkAutomatically: false }));
    await waitFor(() => expect(result.current.error).toBe("read-only"));
    expect(result.current.settings).toEqual({ checkAutomatically: true });
    act(() => result.current.dismissError());
    expect(result.current.error).toBeNull();
  });

  it("never reads the settings when not enabled", async () => {
    const client = clientWith({});
    const { result } = renderHook(() => useUpdateSettings(client, false));
    await act(async () => {
      await Promise.resolve();
    });
    expect(client.getUpdateSettings).not.toHaveBeenCalled();
    expect(result.current.status).toBe("loading");
  });

  it("falls back to the defaults, loaded, without a provider", () => {
    const { result } = renderHook(() => useUpdateSettingsContext());
    expect(result.current.status).toBe("loaded");
    expect(result.current.settings).toEqual({ checkAutomatically: true });
  });
});
