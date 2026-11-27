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

  it.each(["Backspace", "Delete", "Home", "End", "PageUp", "PageDown"])(
    "leaves %s with a modifier to a text field, which edits or moves with it (#1142)",
    (key) => {
      setShortcutChords("fetch", [{ mod: true, key }]);
      const props = setup();
      const field = document.createElement("textarea");
      document.body.append(field);
      field.focus();
      const event = new KeyboardEvent("keydown", { key, code: key, ctrlKey: true, bubbles: true, cancelable: true });
      act(() => void field.dispatchEvent(event));
      expect(props.runCommand).not.toHaveBeenCalled();
      expect(event.defaultPrevented).toBe(false);
      field.blur();
      press({ key, code: key, ctrlKey: true });
      expect(props.runCommand).toHaveBeenCalledWith("fetch");
    },
  );
});

describe("useWorkspaceShortcuts behind an open dialog (#1117)", () => {
  afterEach(() => {
    document.body.innerHTML = "";
  });

  function setupWithDialog(control: "button" | "input" = "button") {
    const props = {
      active: true,
      onToggleSidebar: vi.fn(),
      repositoryOperationDisabled: false,
      onCloseRepoTab: vi.fn(),
      onReopenClosedTab: vi.fn(),
      onNavigateBack: vi.fn(),
      onNavigateForward: vi.fn(),
      repoPath: "/repos/a",
      handleUndo: vi.fn(),
      handleRedo: vi.fn(),
      pending: false,
      transfer: null,
      runCommand: vi.fn(),
    };
    const { result } = renderHook(() => useWorkspaceShortcuts(props));
    const dialog = document.createElement("dialog");
    dialog.setAttribute("open", "");
    const focused = document.createElement(control);
    dialog.append(focused);
    document.body.append(dialog);
    focused.focus();
    const pressIn = (init: KeyboardEventInit) => {
      const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
      act(() => void focused.dispatchEvent(event));
      return event;
    };
    return { props, result, pressIn };
  }

  it.each([
    ["undo", { key: "z", code: "KeyZ", ctrlKey: true }, "handleUndo"],
    ["redo", { key: "y", code: "KeyY", ctrlKey: true }, "handleRedo"],
    ["close-tab", { key: "w", code: "KeyW", ctrlKey: true }, "onCloseRepoTab"],
    ["reopen-closed-tab", { key: "T", code: "KeyT", ctrlKey: true, shiftKey: true }, "onReopenClosedTab"],
    ["go-back", { key: "ArrowLeft", code: "ArrowLeft", altKey: true }, "onNavigateBack"],
    ["go-forward", { key: "ArrowRight", code: "ArrowRight", altKey: true }, "onNavigateForward"],
    ["toggle-sidebar", { key: "b", code: "KeyB", ctrlKey: true, altKey: true }, "onToggleSidebar"],
  ] as const)("doesn't run %s from a button in a dialog", (_id, init, handler) => {
    const { props, pressIn } = setupWithDialog();
    const event = pressIn(init);
    expect(props[handler]).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it.each([
    ["close-tab", { key: "w", code: "KeyW", ctrlKey: true }, "onCloseRepoTab"],
    ["reopen-closed-tab", { key: "T", code: "KeyT", ctrlKey: true, shiftKey: true }, "onReopenClosedTab"],
  ] as const)("doesn't run %s from a text field in a dialog", (_id, init, handler) => {
    const { props, pressIn } = setupWithDialog("input");
    pressIn(init);
    expect(props[handler]).not.toHaveBeenCalled();
  });

  it("doesn't open the palette or the shortcut sheet on top of a dialog", () => {
    const { result, pressIn } = setupWithDialog();
    pressIn({ key: "k", code: "KeyK", ctrlKey: true });
    pressIn({ key: "?", code: "Slash", shiftKey: true });
    expect(result.current.paletteOpen).toBe(false);
    expect(result.current.shortcutsOpen).toBe(false);
  });

  it("doesn't run a shortcut VSCode handed over while the focus is in a dialog", () => {
    const { props } = setupWithDialog();
    act(() => requestWorkspace({ kind: "shortcut", id: "undo" }));
    act(() => requestWorkspace({ kind: "shortcut", id: "close-tab" }));
    expect(props.handleUndo).not.toHaveBeenCalled();
    expect(props.onCloseRepoTab).not.toHaveBeenCalled();
  });

  it("still runs the shortcuts outside a dialog", () => {
    const { props } = setupWithDialog();
    (document.activeElement as HTMLElement).blur();
    press({ key: "z", code: "KeyZ", ctrlKey: true });
    expect(props.handleUndo).toHaveBeenCalledOnce();
  });
});
