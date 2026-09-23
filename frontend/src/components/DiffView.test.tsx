import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DiffHunk } from "../ipc/RepoClient";
import { wordDiff } from "../lib/wordDiff";
import { DiffView } from "./DiffView";

// Pass-through spy on the real `wordDiff`, so tests can count how often DiffView recomputes it.
vi.mock("../lib/wordDiff", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/wordDiff")>();
  return { ...actual, wordDiff: vi.fn(actual.wordDiff) };
});

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

    const { container } = render(<DiffView hunks={hunks} />);

    // Remove/Add is a replace-block pair, so its content renders as word-diff segments (a mix of
    // <mark>/<span> children) rather than a single text node — assert via textContent rather than
    // getByText, which only matches an element's *direct* text-node children.
    expect(container).toHaveTextContent("unchanged");
    expect(container).toHaveTextContent("old value");
    expect(container).toHaveTextContent("new value");
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

    const { container } = render(<DiffView hunks={hunks} />);

    // Word-diff segments split "old value"/"new value" across <mark>/<span> children, so locate
    // the rows by class rather than by getByText (see the previous test's note).
    const removedLine = container.querySelector(".diff-line-remove");
    const addedLine = container.querySelector(".diff-line-add");

    expect(removedLine).toHaveTextContent("old value");
    expect(removedLine).toHaveClass("diff-line-remove");
    expect(addedLine).toHaveTextContent("new value");
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

    const group = screen.getByRole("group", { name: "Diff hunks" });
    // Scoped to the hunk group: the top-level Split view toggle lives outside it and is a normal
    // tab stop, only the per-hunk action buttons are excluded from the tab order.
    for (const button of within(group).getAllByRole("button")) {
      expect(button).toHaveAttribute("tabindex", "-1");
    }
    expect(group).toHaveAttribute("tabindex", "0");
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

  it("highlights only the changed word in a Remove/Add line pair", () => {
    const hunks: DiffHunk[] = [
      {
        oldStart: 1,
        oldLines: 1,
        newStart: 1,
        newLines: 1,
        lines: [
          { origin: "Remove", content: "const foo = 1;" },
          { origin: "Add", content: "const foo = 2;" },
        ],
      },
    ];
    render(<DiffView hunks={hunks} />);
    const marks = document.querySelectorAll("mark");
    expect(marks).toHaveLength(2);
    expect(marks[0]).toHaveTextContent("1");
    expect(marks[1]).toHaveTextContent("2");
  });

  it("computes each pair's word diff once, not again on re-renders that keep the same hunks", () => {
    const hunks: DiffHunk[] = [
      {
        oldStart: 1,
        oldLines: 1,
        newStart: 1,
        newLines: 1,
        lines: [
          { origin: "Remove", content: "const foo = 1;" },
          { origin: "Add", content: "const foo = 2;" },
        ],
      },
    ];
    vi.mocked(wordDiff).mockClear();
    const { rerender } = render(<DiffView hunks={hunks} />);
    // One call for the pair — not one per side.
    expect(wordDiff).toHaveBeenCalledOnce();

    fireEvent.click(screen.getByRole("button", { name: "Split view" }));
    rerender(<DiffView hunks={hunks} />);

    expect(wordDiff).toHaveBeenCalledOnce();
    expect(document.querySelectorAll("mark")).toHaveLength(2);
  });

  it("does not word-diff unpaired Remove/Add lines", () => {
    const hunks: DiffHunk[] = [
      {
        oldStart: 1,
        oldLines: 1,
        newStart: 1,
        newLines: 1,
        lines: [{ origin: "Remove", content: "orphan removed line" }],
      },
    ];
    render(<DiffView hunks={hunks} />);
    expect(document.querySelectorAll("mark")).toHaveLength(0);
    expect(screen.getByText("orphan removed line")).toBeInTheDocument();
  });

  it("toggles to a side-by-side split view", () => {
    const hunks: DiffHunk[] = [
      {
        oldStart: 1,
        oldLines: 2,
        newStart: 1,
        newLines: 2,
        lines: [
          { origin: "Context", content: "unchanged" },
          { origin: "Remove", content: "const foo = 1;" },
          { origin: "Add", content: "const foo = 2;" },
        ],
      },
    ];
    render(<DiffView hunks={hunks} />);
    expect(screen.queryByRole("button", { name: "Split view" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Split view" }));
    expect(screen.getByRole("button", { name: "Unified view" })).toBeInTheDocument();
    expect(document.querySelectorAll('[data-diff-column="old"]')).toHaveLength(1);
    expect(document.querySelectorAll('[data-diff-column="new"]')).toHaveLength(1);
  });

  it("split view still highlights word-level changes in a paired row", () => {
    const hunks: DiffHunk[] = [
      {
        oldStart: 1,
        oldLines: 1,
        newStart: 1,
        newLines: 1,
        lines: [
          { origin: "Remove", content: "const foo = 1;" },
          { origin: "Add", content: "const foo = 2;" },
        ],
      },
    ];
    render(<DiffView hunks={hunks} />);
    fireEvent.click(screen.getByRole("button", { name: "Split view" }));

    const marks = document.querySelectorAll("mark");
    expect(marks).toHaveLength(2);
    expect(marks[0]).toHaveTextContent("1");
    expect(marks[1]).toHaveTextContent("2");
  });

  it("hunk shortcuts still work while in split view", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [] },
      { oldStart: 9, oldLines: 1, newStart: 11, newLines: 1, lines: [] },
    ];
    const onStageHunk = vi.fn();
    render(<DiffView hunks={hunks} onStageHunk={onStageHunk} />);
    fireEvent.click(screen.getByRole("button", { name: "Split view" }));
    const group = screen.getByRole("group", { name: "Diff hunks" });

    fireEvent.keyDown(group, { key: "s" });
    expect(onStageHunk).toHaveBeenLastCalledWith(1, 1);
    fireEvent.keyDown(group, { key: "]" });
    fireEvent.keyDown(group, { key: "s" });
    expect(onStageHunk).toHaveBeenLastCalledWith(9, 11);
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
