import { render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { removeShortcutChord, resetAllShortcuts, setShortcutChords } from "../lib/keymap";
import { OpenPreferencesContext } from "../state/openPreferences";
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

  it("lists the merge editor keys (#764)", () => {
    render(<ShortcutSheet onClose={vi.fn()} apple={false} />);
    const heading = screen.getByRole("heading", { name: "Merge editor" });
    const group = heading.closest("section") ?? heading.parentElement!;
    for (const text of ["Accept ours", "Accept theirs", "Ours then theirs", "Theirs then ours", "Accept base"]) {
      expect(within(group).getByText(new RegExp(`^${text}`))).toBeInTheDocument();
    }
    expect(within(group).getByText("F7")).toBeInTheDocument();
    expect(within(group).getByText("Shift+F7")).toBeInTheDocument();
    expect(within(group).getByText(/back to its block/)).toBeInTheDocument();
  });

  // #880: the merge editor's own Undo and Redo (#878).
  it("lists the merge editor's undo and redo", () => {
    render(<ShortcutSheet onClose={vi.fn()} apple={false} />);
    const heading = screen.getByRole("heading", { name: "Merge editor" });
    const group = heading.closest("section") ?? heading.parentElement!;
    // One row: the global sheet already lists Redo's keys on their own, for the Git undo.
    expect(within(group).getByText(/^Undo \/ redo the last change in the merge editor/)).toBeInTheDocument();
    expect(within(group).getByText("Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y")).toBeInTheDocument();
  });

  describe("the user's keys (#898)", () => {
    afterEach(() => {
      resetAllShortcuts();
      localStorage.clear();
    });

    it("shows rebound keys and leaves out a shortcut without keys", () => {
      setShortcutChords("refresh", [{ key: "F4" }]);
      setShortcutChords("next-conflict", [{ key: "F8" }]);
      setShortcutChords("conflict-ours", [{ key: "q" }]);
      removeShortcutChord("push", { mod: true, shift: true, key: "k" });
      render(<ShortcutSheet onClose={vi.fn()} apple={false} />);
      expect(screen.getByText("Refresh the repository").closest("div")).toHaveTextContent("F4");
      expect(screen.getByText("Next conflict").closest("div")).toHaveTextContent("F8");
      expect(screen.getByText("Accept ours (in a conflict block, outside its result)").closest("div")).toHaveTextContent("q");
      expect(screen.queryByText("Push the current branch")).toBeNull();
    });
  });

  it("opens Preferences on the Keyboard group to change the shortcuts (#899)", () => {
    const onClose = vi.fn();
    const openPreferences = vi.fn();
    render(
      <OpenPreferencesContext.Provider value={openPreferences}>
        <ShortcutSheet onClose={onClose} vscode={false} />
      </OpenPreferencesContext.Provider>,
    );
    screen.getByRole("button", { name: "Change shortcuts…" }).click();
    expect(onClose).toHaveBeenCalledOnce();
    expect(openPreferences).toHaveBeenCalledWith("keyboard");
  });

  it("offers no Change shortcuts button without Preferences to open (#899)", () => {
    render(<ShortcutSheet onClose={vi.fn()} vscode={false} />);
    expect(screen.queryByRole("button", { name: "Change shortcuts…" })).toBeNull();
  });

  it("in VSCode, also opens Preferences on the Keyboard group, which links to VSCode's Keyboard Shortcuts (#900)", () => {
    const openPreferences = vi.fn();
    render(
      <OpenPreferencesContext.Provider value={openPreferences}>
        <ShortcutSheet onClose={vi.fn()} vscode />
      </OpenPreferencesContext.Provider>,
    );
    screen.getByRole("button", { name: "Change shortcuts…" }).click();
    expect(openPreferences).toHaveBeenCalledWith("keyboard");
  });
});
