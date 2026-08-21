import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DiffHunk } from "../ipc/RepoClient";
import { DiffView } from "./DiffView";

describe("DiffView", () => {
  it("renders each line's content", () => {
    const hunks: DiffHunk[] = [
      {
        oldStart: 1,
        oldLines: 2,
        newStart: 1,
        newLines: 3,
        lines: [
          { origin: "Context", content: "unchanged" },
          { origin: "Remove", content: "old value" },
          { origin: "Add", content: "new value" },
        ],
      },
    ];

    render(<DiffView hunks={hunks} />);

    expect(screen.getByText(/unchanged/)).toBeInTheDocument();
    expect(screen.getByText(/old value/)).toBeInTheDocument();
    expect(screen.getByText(/new value/)).toBeInTheDocument();
  });

  it("added and removed lines get distinct CSS classes", () => {
    const hunks: DiffHunk[] = [
      {
        oldStart: 1,
        oldLines: 2,
        newStart: 1,
        newLines: 3,
        lines: [
          { origin: "Context", content: "unchanged" },
          { origin: "Remove", content: "old value" },
          { origin: "Add", content: "new value" },
        ],
      },
    ];

    render(<DiffView hunks={hunks} />);

    const removedLine = screen
      .getByText("old value", { exact: false })
      .closest(".diff-line");
    const addedLine = screen
      .getByText("new value", { exact: false })
      .closest(".diff-line");

    expect(removedLine).toHaveClass("diff-line-remove");
    expect(addedLine).toHaveClass("diff-line-add");
  });

  it("renders a message when there are no hunks", () => {
    render(<DiffView hunks={[]} />);

    expect(screen.getByText("No differences")).toBeInTheDocument();
  });

  it("renders no action buttons when no hunk callbacks are passed", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 1, oldLines: 2, newStart: 1, newLines: 3, lines: [] },
    ];

    render(<DiffView hunks={hunks} />);

    expect(screen.queryByText("Stage Hunk")).not.toBeInTheDocument();
    expect(screen.queryByText("Unstage Hunk")).not.toBeInTheDocument();
    expect(screen.queryByText("Discard Hunk")).not.toBeInTheDocument();
  });

  it("clicking Stage Hunk calls onStageHunk with that hunk's old/new start", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 5, oldLines: 2, newStart: 7, newLines: 3, lines: [] },
    ];
    const onStageHunk = vi.fn();

    render(<DiffView hunks={hunks} onStageHunk={onStageHunk} />);
    fireEvent.click(screen.getByText("Stage Hunk"));

    expect(onStageHunk).toHaveBeenCalledWith(5, 7);
  });

  it("clicking Unstage Hunk calls onUnstageHunk with that hunk's old/new start", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 5, oldLines: 2, newStart: 7, newLines: 3, lines: [] },
    ];
    const onUnstageHunk = vi.fn();

    render(<DiffView hunks={hunks} onUnstageHunk={onUnstageHunk} />);
    fireEvent.click(screen.getByText("Unstage Hunk"));

    expect(onUnstageHunk).toHaveBeenCalledWith(5, 7);
  });

  it("Discard Hunk requires a second click (Confirm Discard) before calling onDiscardHunk", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 5, oldLines: 2, newStart: 7, newLines: 3, lines: [] },
    ];
    const onDiscardHunk = vi.fn();

    render(<DiffView hunks={hunks} onDiscardHunk={onDiscardHunk} />);
    fireEvent.click(screen.getByText("Discard Hunk"));

    expect(onDiscardHunk).not.toHaveBeenCalled();
    expect(screen.getByText("Confirm Discard")).toBeInTheDocument();

    fireEvent.click(screen.getByText("Confirm Discard"));

    expect(onDiscardHunk).toHaveBeenCalledWith(5, 7);
  });

  it("switching to a different hunks array resets any pending discard confirmation", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 5, oldLines: 2, newStart: 7, newLines: 3, lines: [] },
    ];
    const onDiscardHunk = vi.fn();

    const { rerender } = render(<DiffView hunks={hunks} onDiscardHunk={onDiscardHunk} />);
    fireEvent.click(screen.getByText("Discard Hunk"));
    expect(screen.getByText("Confirm Discard")).toBeInTheDocument();

    const otherHunks: DiffHunk[] = [
      { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [] },
    ];
    rerender(<DiffView hunks={otherHunks} onDiscardHunk={onDiscardHunk} />);

    expect(screen.getByText("Discard Hunk")).toBeInTheDocument();
    expect(screen.queryByText("Confirm Discard")).not.toBeInTheDocument();
  });
});
