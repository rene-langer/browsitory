import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { HelpMenuButton } from "./HelpMenuButton";
import { onWorkspaceRequest } from "../lib/workspaceRequests";

describe("HelpMenuButton (#647)", () => {
  function setup(shortcuts = true, onCheckForUpdates?: () => void) {
    const onOpenUserGuide = vi.fn();
    const onShowReleaseNotes = vi.fn();
    render(
      <HelpMenuButton
        className="icon"
        shortcuts={shortcuts}
        onOpenUserGuide={onOpenUserGuide}
        onShowReleaseNotes={onShowReleaseNotes}
        onCheckForUpdates={onCheckForUpdates}
      />,
    );
    return { onOpenUserGuide, onShowReleaseNotes, button: screen.getByRole("button", { name: "Help" }) };
  }

  it("opens a menu with the user guide, the keyboard shortcuts and what's new", () => {
    const { button } = setup();
    expect(button).toHaveAttribute("aria-haspopup", "menu");
    expect(button).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(button);
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "User guide",
      // The `?` that opens the sheet, as a hint after the label (#645).
      "Keyboard shortcuts?",
      "What’s new",
    ]);
    expect(screen.getByRole("menuitem", { name: "Keyboard shortcuts" })).toHaveAttribute("aria-keyshortcuts", "?");
  });

  it("runs the chosen item and closes", () => {
    const { button, onOpenUserGuide, onShowReleaseNotes } = setup();
    fireEvent.click(button);
    fireEvent.click(screen.getByRole("menuitem", { name: "User guide" }));
    expect(onOpenUserGuide).toHaveBeenCalledOnce();
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    fireEvent.click(button);
    fireEvent.click(screen.getByRole("menuitem", { name: "What’s new" }));
    expect(onShowReleaseNotes).toHaveBeenCalledOnce();
  });

  it("asks the active workspace for the shortcut sheet", () => {
    const listener = vi.fn();
    const stop = onWorkspaceRequest(listener);
    const { button } = setup();
    fireEvent.click(button);
    fireEvent.click(screen.getByRole("menuitem", { name: "Keyboard shortcuts" }));
    expect(listener).toHaveBeenCalledWith("shortcuts");
    stop();
  });

  it("asks the active workspace for the operation log (#889)", () => {
    const listener = vi.fn();
    const stop = onWorkspaceRequest(listener);
    const { button } = setup();
    fireEvent.click(button);
    fireEvent.click(screen.getByRole("menuitem", { name: "Operation log" }));
    expect(listener).toHaveBeenCalledWith("operationLog");
    stop();
  });

  it("leaves out the shortcuts without a repository open", () => {
    const { button } = setup(false);
    fireEvent.click(button);
    expect(screen.queryByRole("menuitem", { name: "Keyboard shortcuts" })).not.toBeInTheDocument();
  });

  it("offers Check for updates last, where there is an updater (#852)", () => {
    const onCheckForUpdates = vi.fn();
    const { button } = setup(true, onCheckForUpdates);
    fireEvent.click(button);
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "User guide",
      "Keyboard shortcuts?",
      "What’s new",
      "Check for updates",
    ]);
    fireEvent.click(screen.getByRole("menuitem", { name: "Check for updates" }));
    expect(onCheckForUpdates).toHaveBeenCalledOnce();
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("leaves out Check for updates without an updater (#852)", () => {
    const { button } = setup(true);
    fireEvent.click(button);
    expect(screen.queryByRole("menuitem", { name: "Check for updates" })).not.toBeInTheDocument();
  });

  it("closes on a second click of the button", () => {
    const { button } = setup();
    // A pointer click (`detail` 1): the button is where focus returns, so its press doesn't
    // count as outside the menu.
    fireEvent.click(button, { detail: 1 });
    fireEvent.mouseDown(button);
    fireEvent.click(button, { detail: 1 });
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});
