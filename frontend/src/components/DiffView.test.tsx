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

    expect(screen.getByText(/No text differences/)).toBeInTheDocument();
  });

  it("shows a loading placeholder, not the empty message, while hunks is null", () => {
    render(<DiffView hunks={null} />);

    expect(screen.getByText(/Loading diff/)).toBeInTheDocument();
    expect(screen.queryByText(/No text differences/)).not.toBeInTheDocument();
  });

  it("renders old and new line numbers", () => {
    const hunks: DiffHunk[] = [
      {
        oldStart: 10,
        oldLines: 2,
        newStart: 20,
        newLines: 2,
        lines: [
          { origin: "Context", content: "keep" },
          { origin: "Remove", content: "gone" },
          { origin: "Add", content: "fresh" },
        ],
      },
    ];
    const { container } = render(<DiffView hunks={hunks} />);
    const rows = Array.from(container.querySelectorAll(".diff-line")).map((row) =>
      Array.from(row.querySelectorAll("span[aria-hidden]"))
        .slice(0, 2)
        .map((s) => s.textContent),
    );

    expect(rows).toEqual([
      ["10", "20"],
      ["11", ""],
      ["", "21"],
    ]);
  });

  it("hunk buttons are not tab stops; the diff container is one", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [] },
      { oldStart: 9, oldLines: 1, newStart: 9, newLines: 1, lines: [] },
    ];
    render(<DiffView hunks={hunks} onStageHunk={vi.fn()} onDiscardHunk={vi.fn()} />);

    for (const button of screen.getAllByRole("button")) {
      expect(button).toHaveAttribute("tabindex", "-1");
    }
    expect(screen.getByRole("group", { name: "Diff hunks" })).toHaveAttribute("tabindex", "0");
  });

  it("] moves to the next hunk and s stages it", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [] },
      { oldStart: 9, oldLines: 1, newStart: 11, newLines: 1, lines: [] },
    ];
    const onStageHunk = vi.fn();
    render(<DiffView hunks={hunks} onStageHunk={onStageHunk} />);
    const group = screen.getByRole("group", { name: "Diff hunks" });

    fireEvent.keyDown(group, { key: "s" });
    expect(onStageHunk).toHaveBeenLastCalledWith(1, 1);
    fireEvent.keyDown(group, { key: "]" });
    fireEvent.keyDown(group, { key: "s" });
    expect(onStageHunk).toHaveBeenLastCalledWith(9, 11);
  });

  it("d arms discard, Escape disarms, second d discards", () => {
    const hunks: DiffHunk[] = [{ oldStart: 5, oldLines: 1, newStart: 7, newLines: 1, lines: [] }];
    const onDiscardHunk = vi.fn();
    render(<DiffView hunks={hunks} onDiscardHunk={onDiscardHunk} />);
    const group = screen.getByRole("group", { name: "Diff hunks" });

    fireEvent.keyDown(group, { key: "d" });
    expect(screen.getByText("Confirm discard")).toBeInTheDocument();
    fireEvent.keyDown(group, { key: "Escape" });
    expect(screen.getByText("Discard hunk")).toBeInTheDocument();
    expect(onDiscardHunk).not.toHaveBeenCalled();

    fireEvent.keyDown(group, { key: "d" });
    fireEvent.keyDown(group, { key: "d" });
    expect(onDiscardHunk).toHaveBeenCalledWith(5, 7);
  });

  it("armed discard disarms on blur", () => {
    const hunks: DiffHunk[] = [{ oldStart: 5, oldLines: 1, newStart: 7, newLines: 1, lines: [] }];
    render(<DiffView hunks={hunks} onDiscardHunk={vi.fn()} />);

    fireEvent.click(screen.getByText("Discard hunk"));
    fireEvent.blur(screen.getByText("Confirm discard"));

    expect(screen.getByText("Discard hunk")).toBeInTheDocument();
  });

  it("renders no action buttons when no hunk callbacks are passed", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 1, oldLines: 2, newStart: 1, newLines: 3, lines: [] },
    ];

    render(<DiffView hunks={hunks} />);

    expect(screen.queryByText("Stage hunk")).not.toBeInTheDocument();
    expect(screen.queryByText("Unstage hunk")).not.toBeInTheDocument();
    expect(screen.queryByText("Discard hunk")).not.toBeInTheDocument();
  });

  it("clicking Stage Hunk calls onStageHunk with that hunk's old/new start", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 5, oldLines: 2, newStart: 7, newLines: 3, lines: [] },
    ];
    const onStageHunk = vi.fn();

    render(<DiffView hunks={hunks} onStageHunk={onStageHunk} />);
    fireEvent.click(screen.getByText("Stage hunk"));

    expect(onStageHunk).toHaveBeenCalledWith(5, 7);
  });

  it("clicking Unstage Hunk calls onUnstageHunk with that hunk's old/new start", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 5, oldLines: 2, newStart: 7, newLines: 3, lines: [] },
    ];
    const onUnstageHunk = vi.fn();

    render(<DiffView hunks={hunks} onUnstageHunk={onUnstageHunk} />);
    fireEvent.click(screen.getByText("Unstage hunk"));

    expect(onUnstageHunk).toHaveBeenCalledWith(5, 7);
  });

  it("Discard Hunk requires a second click (Confirm Discard) before calling onDiscardHunk", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 5, oldLines: 2, newStart: 7, newLines: 3, lines: [] },
    ];
    const onDiscardHunk = vi.fn();

    render(<DiffView hunks={hunks} onDiscardHunk={onDiscardHunk} />);
    fireEvent.click(screen.getByText("Discard hunk"));

    expect(onDiscardHunk).not.toHaveBeenCalled();
    expect(screen.getByText("Confirm discard")).toBeInTheDocument();

    fireEvent.click(screen.getByText("Confirm discard"));

    expect(onDiscardHunk).toHaveBeenCalledWith(5, 7);
  });

  it("switching to a different hunks array resets any pending discard confirmation", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 5, oldLines: 2, newStart: 7, newLines: 3, lines: [] },
    ];
    const onDiscardHunk = vi.fn();

    const { rerender } = render(<DiffView hunks={hunks} onDiscardHunk={onDiscardHunk} />);
    fireEvent.click(screen.getByText("Discard hunk"));
    expect(screen.getByText("Confirm discard")).toBeInTheDocument();

    const otherHunks: DiffHunk[] = [
      { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [] },
    ];
    rerender(<DiffView hunks={otherHunks} onDiscardHunk={onDiscardHunk} />);

    expect(screen.getByText("Discard hunk")).toBeInTheDocument();
    expect(screen.queryByText("Confirm discard")).not.toBeInTheDocument();
  });
});
