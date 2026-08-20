import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CommandPalette } from "./CommandPalette";
import type { Command } from "../lib/commands";

function makeCommands(): Command[] {
  return [
    { id: "switch-main", label: "Switch to main", keywords: ["branch"], run: vi.fn() },
    { id: "switch-feature", label: "Switch to feature", keywords: ["branch"], run: vi.fn() },
    { id: "fetch-origin", label: "Fetch origin", keywords: ["remote"], run: vi.fn() },
  ];
}

describe("CommandPalette", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("renders every command when the query is empty", () => {
    render(<CommandPalette commands={makeCommands()} onRun={vi.fn()} />);
    expect(screen.getByText("Switch to main")).toBeInTheDocument();
    expect(screen.getByText("Switch to feature")).toBeInTheDocument();
    expect(screen.getByText("Fetch origin")).toBeInTheDocument();
  });

  it("filters as the user types", () => {
    render(<CommandPalette commands={makeCommands()} onRun={vi.fn()} />);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "fetch" } });
    expect(screen.getByText("Fetch origin")).toBeInTheDocument();
    expect(screen.queryByText("Switch to main")).not.toBeInTheDocument();
  });

  it("runs the highlighted command and calls onRun when Enter is pressed", () => {
    const commands = makeCommands();
    const onRun = vi.fn();
    render(<CommandPalette commands={commands} onRun={onRun} />);
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    // With an empty query, filterAndSortCommands (Task 1) sorts tied-score results
    // alphabetically by label, so "Fetch origin" (commands[2]) is highlighted first —
    // not the original array order.
    expect(commands[2].run).toHaveBeenCalledOnce();
    expect(onRun).toHaveBeenCalledOnce();
  });

  it("moves the highlight with arrow keys before running", () => {
    const commands = makeCommands();
    render(<CommandPalette commands={commands} onRun={vi.fn()} />);
    const input = screen.getByRole("textbox");
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(commands[1].run).toHaveBeenCalledOnce();
    expect(commands[0].run).not.toHaveBeenCalled();
  });

  it("runs a command on click regardless of highlight", () => {
    const commands = makeCommands();
    const onRun = vi.fn();
    render(<CommandPalette commands={commands} onRun={onRun} />);
    fireEvent.click(screen.getByText("Fetch origin"));
    expect(commands[2].run).toHaveBeenCalledOnce();
    expect(onRun).toHaveBeenCalledOnce();
  });

  it("records the run command as recently used", () => {
    const commands = makeCommands();
    render(<CommandPalette commands={commands} onRun={vi.fn()} />);
    fireEvent.click(screen.getByText("Fetch origin"));
    expect(localStorage.getItem("command-palette-recent")).toContain("fetch-origin");
  });

  it("shows an empty-state message when nothing matches", () => {
    render(<CommandPalette commands={makeCommands()} onRun={vi.fn()} />);
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "zzz-no-match" } });
    expect(screen.getByText("No matching commands")).toBeInTheDocument();
  });
});
