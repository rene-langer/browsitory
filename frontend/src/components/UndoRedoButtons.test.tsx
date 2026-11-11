import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { UndoRedoButtons } from "./UndoRedoButtons";

describe("UndoRedoButtons", () => {
  it("names the next steps in tooltips and runs them", () => {
    const onUndo = vi.fn();
    const onRedo = vi.fn();
    render(
      <UndoRedoButtons undoLabel="Stage a.txt" redoLabel="Commit “x”" disabled={false} disabledReason={null} onUndo={onUndo} onRedo={onRedo} />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Undo Stage a.txt" }));
    fireEvent.click(screen.getByRole("button", { name: "Redo Commit “x”" }));
    expect(onUndo).toHaveBeenCalledOnce();
    expect(onRedo).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: "Undo Stage a.txt" })).toHaveAttribute("title", "Undo Stage a.txt");
  });

  it("is disabled with 'Nothing to …' when a stack is empty", () => {
    render(<UndoRedoButtons undoLabel={null} redoLabel={null} disabled={false} disabledReason={null} onUndo={vi.fn()} onRedo={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Nothing to undo" })).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByRole("button", { name: "Nothing to redo" })).toHaveAttribute("aria-disabled", "true");
  });

  it("is disabled with the busy reason while an operation runs", () => {
    const onUndo = vi.fn();
    render(
      <UndoRedoButtons undoLabel="Stage a.txt" redoLabel={null} disabled disabledReason="Another action is in progress." onUndo={onUndo} onRedo={vi.fn()} />,
    );
    const undo = screen.getByRole("button", { name: "Undo Stage a.txt" });
    expect(undo).toHaveAttribute("aria-disabled", "true");
    expect(undo).toHaveAccessibleDescription("Another action is in progress.");
    fireEvent.click(undo);
    expect(onUndo).not.toHaveBeenCalled();
  });
});
