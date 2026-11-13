import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { chordsFor, mergeEditorChordsFor } from "../lib/keybindings";
import { KEYMAP_STORAGE_KEY, resetAllShortcuts, setShortcutChords } from "../lib/keymap";
import { KeyboardShortcutsSection } from "./KeyboardShortcutsSection";

afterEach(() => {
  resetAllShortcuts();
  localStorage.removeItem(KEYMAP_STORAGE_KEY);
});

const row = (description: string) => screen.getByRole("listitem", { name: description });

function startRecording(description: string): HTMLElement {
  fireEvent.click(within(row(description)).getByRole("button", { name: `Add a key for ${description}` }));
  const recorder = screen.getByRole("textbox", { name: `New key for ${description}` });
  expect(recorder).toHaveFocus();
  return recorder;
}

// #899: the Preferences Keyboard section.
describe("KeyboardShortcutsSection (#899)", () => {
  it("lists every rebindable shortcut with its keys, by group, in the platform's notation", () => {
    const { unmount } = render(<KeyboardShortcutsSection apple={false} />);
    expect(screen.getByRole("group", { name: "Keyboard shortcuts" })).toBeInTheDocument();
    expect(screen.getByRole("list", { name: "Global" })).toBeInTheDocument();
    expect(screen.getByRole("list", { name: "Merge editor" })).toBeInTheDocument();
    expect(within(row("Refresh the repository")).getByText("F5").tagName).toBe("KBD");
    expect(within(row("Redo")).getByText("Ctrl+Shift+Z")).toBeInTheDocument();
    expect(within(row("Redo")).getByText("Ctrl+Y")).toBeInTheDocument();
    expect(within(row("Accept ours")).getByText("1")).toBeInTheDocument();
    // Ctrl+1 … 9 stays fixed.
    expect(screen.queryByRole("listitem", { name: "Switch to tab 1 to 8, or the last tab" })).toBeNull();
    unmount();

    render(<KeyboardShortcutsSection apple />);
    expect(within(row("Redo")).getByText("⇧⌘Z")).toBeInTheDocument();
  });

  it("filters the list by command or key", () => {
    render(<KeyboardShortcutsSection apple={false} />);
    const search = screen.getByRole("searchbox", { name: "Search shortcuts" });
    fireEvent.change(search, { target: { value: "push" } });
    expect(row("Push the current branch")).toBeInTheDocument();
    expect(screen.queryByRole("listitem", { name: "Refresh the repository" })).toBeNull();
    fireEvent.change(search, { target: { value: "f5" } });
    expect(row("Refresh the repository")).toBeInTheDocument();
    expect(row("Fetch")).toBeInTheDocument();
    expect(screen.queryByRole("listitem", { name: "Push the current branch" })).toBeNull();
    fireEvent.change(search, { target: { value: "nothing like this" } });
    expect(screen.getByText("No shortcut matches.")).toBeInTheDocument();
  });

  it("records a new key by pressing it, and goes back to the Add key button", () => {
    render(<KeyboardShortcutsSection apple={false} />);
    const recorder = startRecording("Refresh the repository");
    // A modifier alone waits for the rest of the chord.
    fireEvent.keyDown(recorder, { key: "Control", code: "ControlLeft", ctrlKey: true });
    expect(recorder).toBeInTheDocument();
    fireEvent.keyDown(recorder, { key: "r", code: "KeyR", ctrlKey: true });
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(within(row("Refresh the repository")).getByText("Ctrl+R")).toBeInTheDocument();
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }, { mod: true, key: "r" }]);
    expect(within(row("Refresh the repository")).getByRole("button", { name: "Add a key for Refresh the repository" })).toHaveFocus();
  });

  it("cancels recording with Esc, and when the focus leaves", () => {
    render(<KeyboardShortcutsSection apple={false} />);
    let recorder = startRecording("Refresh the repository");
    fireEvent.keyDown(recorder, { key: "Escape", code: "Escape" });
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(within(row("Refresh the repository")).getByRole("button", { name: "Add a key for Refresh the repository" })).toHaveFocus();
    recorder = startRecording("Refresh the repository");
    // Tab moves on (keyboard-only use): it is not recorded.
    expect(fireEvent.keyDown(recorder, { key: "Tab", code: "Tab" })).toBe(true);
    fireEvent.blur(recorder);
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }]);
  });

  it("keeps the pressed keys from the app's own shortcuts while recording", () => {
    const onWindowKey = vi.fn();
    window.addEventListener("keydown", onWindowKey);
    render(<KeyboardShortcutsSection apple={false} />);
    const recorder = startRecording("Refresh the repository");
    fireEvent.keyDown(recorder, { key: "k", code: "KeyK", ctrlKey: true, altKey: true });
    window.removeEventListener("keydown", onWindowKey);
    expect(onWindowKey).not.toHaveBeenCalled();
  });

  it("refuses a reserved key and keeps recording", () => {
    render(<KeyboardShortcutsSection apple={false} />);
    const recorder = startRecording("Refresh the repository");
    fireEvent.keyDown(recorder, { key: "c", code: "KeyC", ctrlKey: true });
    expect(screen.getByRole("alert")).toHaveTextContent("Ctrl+C is reserved and can't be a shortcut.");
    expect(recorder).toHaveFocus();
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }]);
  });

  it("names the command that has the key, and Replace moves it", () => {
    render(<KeyboardShortcutsSection apple={false} />);
    const recorder = startRecording("Refresh the repository");
    fireEvent.keyDown(recorder, { key: "K", code: "KeyK", ctrlKey: true, shiftKey: true });
    expect(screen.getByRole("alert")).toHaveTextContent("Ctrl+Shift+K is already the shortcut for Push the current branch.");
    const replace = screen.getByRole("button", { name: "Replace" });
    expect(replace).toHaveFocus();
    fireEvent.click(replace);
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }, { mod: true, shift: true, key: "k" }]);
    expect(chordsFor("push", false)).toEqual([]);
    expect(within(row("Push the current branch")).getByText("No keys")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("cancels a conflicting key", () => {
    render(<KeyboardShortcutsSection apple={false} />);
    const recorder = startRecording("Refresh the repository");
    fireEvent.keyDown(recorder, { key: "K", code: "KeyK", ctrlKey: true, shiftKey: true });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(chordsFor("push", false)).toEqual([{ mod: true, shift: true, key: "k" }]);
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }]);
    expect(within(row("Refresh the repository")).getByRole("button", { name: "Add a key for Refresh the repository" })).toHaveFocus();
  });

  it("offers no Replace for a key of a fixed shortcut", () => {
    render(<KeyboardShortcutsSection apple={false} />);
    const recorder = startRecording("Refresh the repository");
    fireEvent.keyDown(recorder, { key: "1", code: "Digit1", ctrlKey: true });
    expect(screen.getByRole("alert")).toHaveTextContent(
      "Ctrl+1 is already the shortcut for Switch to tab 1 to 8, or the last tab, which can't be changed.",
    );
    expect(screen.queryByRole("button", { name: "Replace" })).toBeNull();
  });

  it("removes a key, and resets the shortcut", () => {
    render(<KeyboardShortcutsSection apple={false} />);
    expect(within(row("Refresh the repository")).queryByRole("button", { name: "Reset Refresh the repository" })).toBeNull();
    fireEvent.click(within(row("Refresh the repository")).getByRole("button", { name: "Remove F5 from Refresh the repository" }));
    expect(within(row("Refresh the repository")).getByText("No keys")).toBeInTheDocument();
    expect(chordsFor("refresh", false)).toEqual([]);
    fireEvent.click(within(row("Refresh the repository")).getByRole("button", { name: "Reset Refresh the repository" }));
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }]);
    expect(within(row("Refresh the repository")).getByText("F5")).toBeInTheDocument();
  });

  it("resets every shortcut", () => {
    render(<KeyboardShortcutsSection apple={false} />);
    const resetAll = screen.getByRole("button", { name: "Reset all shortcuts" });
    expect(resetAll).toBeDisabled();
    act(() => void setShortcutChords("refresh", [{ key: "F4" }]));
    act(() => void setShortcutChords("conflict-ours", [{ key: "q" }]));
    expect(resetAll).toBeEnabled();
    fireEvent.click(resetAll);
    expect(chordsFor("refresh", false)).toEqual([{ key: "F5" }]);
    expect(mergeEditorChordsFor("conflict-ours", false)).toEqual([{ key: "1" }]);
    expect(resetAll).toBeDisabled();
  });
});
