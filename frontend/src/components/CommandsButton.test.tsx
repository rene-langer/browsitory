import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandsButton } from "./CommandsButton";
import { onWorkspaceRequest } from "../lib/workspaceRequests";
import { removeShortcutChord, resetAllShortcuts, setShortcutChords } from "../lib/keymap";

describe("CommandsButton (#647)", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    resetAllShortcuts();
    localStorage.clear();
  });

  it("is a button advertising Ctrl+K on non-Apple platforms", () => {
    vi.stubGlobal("navigator", { platform: "Linux x86_64", userAgent: "X11; Linux" });
    render(<CommandsButton />);
    const button = screen.getByRole("button", { name: "Commands Ctrl+K" });
    expect(button).toHaveAttribute("aria-keyshortcuts", "Control+K");
    expect(button).toHaveAttribute("title", "Open the command palette");
    expect(screen.getByText("Ctrl+K").tagName).toBe("KBD");
  });

  it("advertises ⌘K on Apple platforms", () => {
    vi.stubGlobal("navigator", { platform: "MacIntel", userAgent: "Macintosh" });
    render(<CommandsButton />);
    expect(screen.getByRole("button", { name: "Commands ⌘K" })).toHaveAttribute("aria-keyshortcuts", "Meta+K");
  });

  it("asks the active workspace to open the palette", () => {
    const listener = vi.fn();
    const stop = onWorkspaceRequest(listener);
    render(<CommandsButton />);
    fireEvent.click(screen.getByRole("button", { name: /^Commands/ }));
    expect(listener).toHaveBeenCalledWith("palette");
    stop();
  });

  it("follows the user's keys as they change (#898)", () => {
    render(<CommandsButton />);
    act(() => void setShortcutChords("palette", [{ mod: true, key: "p" }]));
    const button = screen.getByRole("button", { name: "Commands Ctrl+P" });
    expect(button).toHaveAttribute("aria-keyshortcuts", "Control+P");
    act(() => removeShortcutChord("palette", { mod: true, key: "p" }));
    expect(screen.getByRole("button", { name: "Commands" })).not.toHaveAttribute("aria-keyshortcuts");
    expect(document.querySelector("kbd")).toBeNull();
  });
});
