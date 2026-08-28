import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { SIDEBAR_PANEL_IDS, useSidebarPanelVisibility } from "./useSidebarPanelVisibility";

describe("useSidebarPanelVisibility", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("defaults every known panel to visible", () => {
    const { result } = renderHook(() => useSidebarPanelVisibility());
    for (const id of SIDEBAR_PANEL_IDS) {
      expect(result.current.visibility[id]).toBe(true);
    }
  });

  it("setPanelVisible updates state and persists to localStorage", () => {
    const { result } = renderHook(() => useSidebarPanelVisibility());
    act(() => result.current.setPanelVisible("worktree", false));
    expect(result.current.visibility.worktree).toBe(false);
    expect(JSON.parse(localStorage.getItem("sidebar.panels") ?? "{}")).toMatchObject({ worktree: false });
  });

  it("loads a previously persisted map on mount", () => {
    localStorage.setItem("sidebar.panels", JSON.stringify({ reflog: false }));
    const { result } = renderHook(() => useSidebarPanelVisibility());
    expect(result.current.visibility.reflog).toBe(false);
    expect(result.current.visibility.stash).toBe(true);
  });

  it("falls back to defaults when the stored value is malformed", () => {
    localStorage.setItem("sidebar.panels", "not json");
    const { result } = renderHook(() => useSidebarPanelVisibility());
    expect(result.current.visibility.stash).toBe(true);
  });
});
