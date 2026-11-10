import { renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { testWorkspaceActions } from "../test/workspaceActions";
import { useWorkspaceActions, WorkspaceActionsContext } from "./useWorkspaceActions";

describe("useWorkspaceActions", () => {
  it("returns the provided actions", () => {
    const actions = testWorkspaceActions({ onStageFile: vi.fn() });
    const wrapper = ({ children }: { children: ReactNode }) => (
      <WorkspaceActionsContext.Provider value={actions}>{children}</WorkspaceActionsContext.Provider>
    );

    const { result } = renderHook(() => useWorkspaceActions(), { wrapper });

    expect(result.current).toBe(actions);
  });

  it("fails without a provider instead of falling back to actions that do nothing", () => {
    // React logs the render error; keep the output clean.
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      expect(() => renderHook(() => useWorkspaceActions())).toThrow(/WorkspaceActionsContext provider/);
    } finally {
      consoleError.mockRestore();
    }
  });
});
