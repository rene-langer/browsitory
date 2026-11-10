import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CommandsButton } from "./CommandsButton";
import { onWorkspaceRequest } from "../lib/workspaceRequests";

describe("CommandsButton (#647)", () => {
  afterEach(() => vi.unstubAllGlobals());

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
});
