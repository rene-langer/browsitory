import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { requestWorkspace } from "../lib/workspaceRequests";
import { useWorkspaceShortcuts } from "./useWorkspaceShortcuts";

function setup() {
  const props = {
    active: true,
    repositoryOperationDisabled: false,
    onCloseRepoTab: vi.fn(),
    onNavigateBack: vi.fn(),
    onNavigateForward: vi.fn(),
    repoPath: "/repos/a",
    handleUndo: vi.fn(),
    handleRedo: vi.fn(),
    pending: false,
    transfer: null,
    runCommand: vi.fn(),
    tabs: { paths: ["/repos/a", "/repos/b", "/repos/c"], onSwitch: vi.fn() },
  };
  renderHook(() => useWorkspaceShortcuts(props));
  return props;
}

const press = (init: KeyboardEventInit) => act(() => void window.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, ...init })));

describe("useWorkspaceShortcuts and VSCode's keybindings (#679)", () => {
  afterEach(() => {
    delete document.documentElement.dataset.host;
    document.body.innerHTML = "";
  });

  it("runs a shortcut the extension handed over", () => {
    const props = setup();
    act(() => requestWorkspace({ kind: "shortcut", id: "close-tab" }));
    expect(props.onCloseRepoTab).toHaveBeenCalledWith("/repos/a");
    act(() => requestWorkspace({ kind: "shortcut", id: "tab-by-number", index: 3 }));
    expect(props.tabs.onSwitch).toHaveBeenCalledWith("/repos/c");
    act(() => requestWorkspace({ kind: "shortcut", id: "refresh" }));
    expect(props.runCommand).toHaveBeenCalledWith("refresh");
    act(() => requestWorkspace({ kind: "shortcut", id: "go-forward" }));
    expect(props.onNavigateForward).toHaveBeenCalledOnce();
  });

  it("follows the focused element's rules: Back/Forward leave a text field alone", () => {
    const props = setup();
    const field = document.createElement("textarea");
    document.body.append(field);
    field.focus();
    act(() => requestWorkspace({ kind: "shortcut", id: "go-back" }));
    expect(props.onNavigateBack).not.toHaveBeenCalled();
  });

  it("in VSCode, leaves the keys VSCode binds to the extension's keybindings, and keeps the others", () => {
    document.documentElement.dataset.host = "vscode";
    const props = setup();
    press({ key: "w", code: "KeyW", ctrlKey: true });
    press({ key: "F5", code: "F5" });
    expect(props.onCloseRepoTab).not.toHaveBeenCalled();
    expect(props.runCommand).not.toHaveBeenCalled();
    press({ key: "z", code: "KeyZ", ctrlKey: true });
    expect(props.handleUndo).toHaveBeenCalledOnce();
    press({ key: "L", code: "KeyL", ctrlKey: true, shiftKey: true });
    expect(props.runCommand).toHaveBeenCalledWith("pull");
  });

  it("on the desktop, handles those keys itself", () => {
    const props = setup();
    press({ key: "w", code: "KeyW", ctrlKey: true });
    expect(props.onCloseRepoTab).toHaveBeenCalledWith("/repos/a");
  });
});
