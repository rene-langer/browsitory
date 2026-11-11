import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ShortcutSheet } from "./ShortcutSheet";

describe("ShortcutSheet", () => {
  it("lists the existing shortcuts", () => {
    render(<ShortcutSheet onClose={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "Keyboard shortcuts" })).toBeInTheDocument();
    expect(screen.getByText("Open the command palette")).toBeInTheDocument();
    expect(screen.getByText("Stage or unstage the selected files")).toBeInTheDocument();
    expect(screen.getByText("Ctrl+Enter")).toBeInTheDocument();
  });

  it("lists the line-selection shortcuts", () => {
    render(<ShortcutSheet onClose={vi.fn()} />);
    expect(screen.getByText("Move line cursor")).toBeInTheDocument();
    expect(screen.getByText("Extend line selection")).toBeInTheDocument();
    expect(screen.getAllByText("↑ / ↓").length).toBeGreaterThan(0);
    expect(screen.getByText("Shift+↑ / Shift+↓")).toBeInTheDocument();
    expect(screen.getByText(/Stage or unstage the selected lines or hunk/)).toBeInTheDocument();
    expect(screen.getByText(/Discard the selected lines or hunk/)).toBeInTheDocument();
  });

  it("writes the keys for the platform in one notation (#683)", () => {
    const { unmount } = render(<ShortcutSheet onClose={vi.fn()} apple={false} />);
    expect(screen.getByText("Ctrl+Shift+Z / Ctrl+Y")).toBeInTheDocument();
    expect(screen.getByText("Alt+←")).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/Cmd|⌘|(?<!Page )\bUp\b|(?<!Page )\bDown\b/);
    unmount();

    render(<ShortcutSheet onClose={vi.fn()} apple />);
    expect(screen.getByText("⇧⌘Z")).toBeInTheDocument();
    expect(screen.getByText("⌘[")).toBeInTheDocument();
    expect(screen.getByText("⌘↩")).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/Ctrl\/Cmd|Cmd\+/);
  });

  it("lists the core loop's shortcuts and F6 (#646, #670)", () => {
    render(<ShortcutSheet onClose={vi.fn()} apple={false} />);
    for (const [keys, description] of [
      ["F5", "Refresh the repository"],
      ["Shift+F5", "Fetch"],
      ["Ctrl+Shift+L", "Pull the current branch"],
      ["Ctrl+Shift+K", "Push the current branch"],
      ["Ctrl+Shift+A", "Stage all changes"],
      ["F6", "Move to the next pane"],
      ["Shift+F6", "Move to the previous pane"],
      ["Ctrl+1 … Ctrl+9", "Switch to tab 1 to 8, or the last tab (where the host doesn't claim it)"],
    ]) {
      const row = screen.getByText(description).closest("div")!;
      expect(row).toHaveTextContent(keys);
    }
  });

  it("in VSCode, drops the host caveat from the keys the extension binds, and says where to change them (#679)", () => {
    const { unmount } = render(<ShortcutSheet onClose={vi.fn()} apple={false} vscode={false} />);
    expect(screen.getByText("Close the active tab (where the host doesn't claim it)")).toBeInTheDocument();
    expect(screen.queryByText(/Keyboard Shortcuts/)).toBeNull();
    unmount();

    render(<ShortcutSheet onClose={vi.fn()} apple={false} vscode />);
    expect(screen.getByText("Close the active tab").closest("div")).toHaveTextContent("Ctrl+W");
    expect(document.body.textContent).not.toMatch(/where the host/);
    expect(screen.getByText(/search for "Browsitory" in VSCode's Keyboard Shortcuts/)).toBeInTheDocument();
  });

  it("Close calls onClose", () => {
    const onClose = vi.fn();
    render(<ShortcutSheet onClose={onClose} />);
    screen.getByRole("button", { name: "Close" }).click();
    expect(onClose).toHaveBeenCalled();
  });

  it("lists the context menu keys", () => {
    render(<ShortcutSheet onClose={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "Context menus" })).toBeInTheDocument();
    expect(screen.getByText("Open the focused item's menu")).toBeInTheDocument();
    expect(screen.getByText("Jump to the next item starting with it")).toBeInTheDocument();
    expect(screen.getByText("Close the menu and return to where it opened")).toBeInTheDocument();
    expect(screen.getByText("Open the selected file's menu")).toBeInTheDocument();
  });

  it("lists the command palette keys (#643)", () => {
    render(<ShortcutSheet onClose={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "Command palette" })).toBeInTheDocument();
    expect(screen.getByText("Move the highlight by a page")).toBeInTheDocument();
    expect(screen.getByText(/^First \/ last command/)).toBeInTheDocument();
  });
});
