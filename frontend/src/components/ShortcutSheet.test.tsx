import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ShortcutSheet } from "./ShortcutSheet";

describe("ShortcutSheet", () => {
  it("lists the existing shortcuts", () => {
    render(<ShortcutSheet onClose={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "Keyboard shortcuts" })).toBeInTheDocument();
    expect(screen.getByText("Open the command palette")).toBeInTheDocument();
    expect(screen.getByText("Stage or unstage the selected file")).toBeInTheDocument();
    expect(screen.getByText("Ctrl/Cmd+Enter")).toBeInTheDocument();
  });

  it("lists the line-selection shortcuts", () => {
    render(<ShortcutSheet onClose={vi.fn()} />);
    expect(screen.getByText("Move line cursor")).toBeInTheDocument();
    expect(screen.getByText("Extend line selection")).toBeInTheDocument();
    expect(screen.getByText("↑ / ↓")).toBeInTheDocument();
    expect(screen.getByText("Shift+↑ / Shift+↓")).toBeInTheDocument();
    expect(screen.getByText(/Stage or unstage the selected lines or hunk/)).toBeInTheDocument();
    expect(screen.getByText(/Discard the selected lines or hunk/)).toBeInTheDocument();
  });

  it("Close calls onClose", () => {
    const onClose = vi.fn();
    render(<ShortcutSheet onClose={onClose} />);
    screen.getByRole("button", { name: "Close" }).click();
    expect(onClose).toHaveBeenCalled();
  });
});
