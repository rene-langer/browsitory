import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { requestWorkspace } from "../lib/workspaceRequests";
import { resetAllShortcuts, setShortcutChords } from "../lib/keymap";
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

describe("useWorkspaceShortcuts and the user's keys (#898)", () => {
  afterEach(() => {
    resetAllShortcuts();
    localStorage.clear();
    document.body.innerHTML = "";
  });

  it("runs a rebound shortcut with its new key, not the built-in one", () => {
    setShortcutChords("refresh", [{ key: "F4" }]);
    const props = setup();
    press({ key: "F5", code: "F5" });
    expect(props.runCommand).not.toHaveBeenCalled();
    press({ key: "F4", code: "F4" });
    expect(props.runCommand).toHaveBeenCalledWith("refresh");
  });

  it("goes back and forward with rebound keys", () => {
    setShortcutChords("go-back", [{ mod: true, alt: true, key: "ArrowLeft" }]);
    setShortcutChords("go-forward", [{ key: "F9" }]);
    const props = setup();
    press({ key: "ArrowLeft", code: "ArrowLeft", ctrlKey: true, altKey: true });
    expect(props.onNavigateBack).toHaveBeenCalledOnce();
    press({ key: "F9", code: "F9" });
    expect(props.onNavigateForward).toHaveBeenCalledOnce();
  });

  it("never takes a key that types a character in a text field", () => {
    // Refresh works from text fields, but a plain letter must still type there.
    setShortcutChords("refresh", [{ key: "r" }]);
    const props = setup();
    const field = document.createElement("textarea");
    document.body.append(field);
    field.focus();
    act(() => void field.dispatchEvent(new KeyboardEvent("keydown", { key: "r", code: "KeyR", bubbles: true })));
    expect(props.runCommand).not.toHaveBeenCalled();
    field.blur();
    press({ key: "r", code: "KeyR" });
    expect(props.runCommand).toHaveBeenCalledWith("refresh");
  });
});
