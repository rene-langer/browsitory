import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { DiffHunk } from "../ipc/RepoClient";
import { wordDiff } from "../lib/wordDiff";
import type { ReactNode } from "react";
import { DiffSettingsContext, type DiffSettingsApi } from "../state/useDiffSettings";
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

const lineHunk: DiffHunk = {
  oldStart: 1, oldLines: 3, newStart: 1, newLines: 4,
  lines: [
    { origin: "Context", content: "a\n" },
    { origin: "Remove", content: "b\n" },
    { origin: "Add", content: "B\n" },
    { origin: "Context", content: "c\n" },
    { origin: "Add", content: "d\n" },
  ],
};

// What every line callback must send as `expectedLines`: the hunk exactly as displayed.
const shown = lineHunk.lines.map((l) => l.content);

const row = (container: HTMLElement, index: number) =>
  container.querySelector<HTMLElement>(`[data-hunk-index="0"][data-line-index="${index}"]`)!;

const TestDiffSettings = ({
  value,
  children,
}: {
  value: Pick<DiffSettingsApi, "settings" | "update">;
  children: ReactNode;
}) => (
  <DiffSettingsContext.Provider value={{ error: null, dismissError: () => {}, ...value }}>
    {children}
  </DiffSettingsContext.Provider>
);

describe("DiffView line selection", () => {
  it("click then shift-click selects a range and stages those lines", () => {
    const onStageLines = vi.fn();
    const { container } = render(<DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={onStageLines} />);
    fireEvent.click(row(container, 1));
    fireEvent.click(row(container, 4), { shiftKey: true });
    expect(screen.getByRole("button", { name: "Stage lines (3)" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Stage lines (3)" }));
    expect(onStageLines).toHaveBeenCalledWith(1, 1, [1, 2, 4], shown);
  });

  it("ctrl-click toggles single lines", () => {
    const onStageLines = vi.fn();
    const { container } = render(<DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={onStageLines} />);
    fireEvent.click(row(container, 2));
    fireEvent.click(row(container, 4), { ctrlKey: true });
    expect(row(container, 2)).toHaveAttribute("aria-selected", "true");
    expect(row(container, 4)).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText("2 lines selected")).toBeInTheDocument();
  });

  it("s stages the selection, or the hunk when nothing is selected", () => {
    const onStageHunk = vi.fn();
    const onStageLines = vi.fn();
    const { container } = render(<DiffView hunks={[lineHunk]} onStageHunk={onStageHunk} onStageLines={onStageLines} />);
    const group = screen.getByRole("group", { name: "Diff hunks" });
    fireEvent.keyDown(group, { key: "s" });
    expect(onStageHunk).toHaveBeenCalledWith(1, 1);
    fireEvent.click(row(container, 4));
    fireEvent.keyDown(group, { key: "s" });
    expect(onStageLines).toHaveBeenCalledWith(1, 1, [4], shown);
  });

  it("arrow keys move a line cursor and shift-arrow extends", () => {
    const onStageLines = vi.fn();
    render(<DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={onStageLines} />);
    const group = screen.getByRole("group", { name: "Diff hunks" });
    fireEvent.keyDown(group, { key: "ArrowDown" });
    fireEvent.keyDown(group, { key: "ArrowDown", shiftKey: true });
    fireEvent.keyDown(group, { key: "s" });
    expect(onStageLines).toHaveBeenCalledWith(1, 1, [1, 2], shown);
  });

  it("arrow cursor then d twice discards only the cursor line, never the hunk", () => {
    const onDiscardHunk = vi.fn();
    const onDiscardLines = vi.fn();
    render(
      <DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onDiscardHunk={onDiscardHunk} onDiscardLines={onDiscardLines} />,
    );
    const group = screen.getByRole("group", { name: "Diff hunks" });
    fireEvent.keyDown(group, { key: "ArrowDown" });
    fireEvent.keyDown(group, { key: "ArrowDown" });
    expect(screen.getByText("1 line selected")).toBeInTheDocument();
    fireEvent.keyDown(group, { key: "d" });
    fireEvent.keyDown(group, { key: "d" });
    expect(onDiscardHunk).not.toHaveBeenCalled();
    expect(onDiscardLines).toHaveBeenCalledWith(1, 1, [2], shown);
  });

  it("arrow cursor then s stages only the cursor line", () => {
    const onStageHunk = vi.fn();
    const onStageLines = vi.fn();
    render(<DiffView hunks={[lineHunk]} onStageHunk={onStageHunk} onStageLines={onStageLines} />);
    const group = screen.getByRole("group", { name: "Diff hunks" });
    fireEvent.keyDown(group, { key: "ArrowDown" });
    fireEvent.keyDown(group, { key: "s" });
    expect(onStageHunk).not.toHaveBeenCalled();
    expect(onStageLines).toHaveBeenCalledWith(1, 1, [1], shown);
  });

  it("d twice discards the selected lines", () => {
    const onDiscardLines = vi.fn();
    const { container } = render(
      <DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onDiscardHunk={vi.fn()} onDiscardLines={onDiscardLines} />,
    );
    fireEvent.click(row(container, 2));
    const group = screen.getByRole("group", { name: "Diff hunks" });
    fireEvent.keyDown(group, { key: "d" });
    fireEvent.keyDown(group, { key: "d" });
    expect(onDiscardLines).toHaveBeenCalledWith(1, 1, [2], shown);
  });

  it("partialActionsDisabled hides hunk and line actions and shows the hint", () => {
    render(<DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={vi.fn()} partialActionsDisabled />);
    expect(screen.queryByRole("button", { name: "Stage hunk" })).toBeNull();
    expect(screen.getByText("Turn off Ignore whitespace to stage partially.")).toBeInTheDocument();
  });

  it("split view cells select by their index in the hunk", () => {
    const onStageLines = vi.fn();
    const { container } = render(<DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={onStageLines} />);
    fireEvent.click(screen.getByRole("button", { name: "Split view" }));
    fireEvent.click(row(container, 2));
    fireEvent.click(row(container, 4), { shiftKey: true });
    expect(row(container, 2)).toHaveAttribute("data-diff-column", "new");
    fireEvent.click(screen.getByRole("button", { name: "Stage lines (2)" }));
    expect(onStageLines).toHaveBeenCalledWith(1, 1, [2, 4], shown);
  });

  it("dragging across lines selects the range", () => {
    const onStageLines = vi.fn();
    const { container } = render(<DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={onStageLines} />);
    fireEvent.mouseDown(row(container, 1), { button: 0 });
    fireEvent.mouseEnter(row(container, 2));
    fireEvent.mouseEnter(row(container, 4));
    fireEvent.mouseUp(window);
    fireEvent.mouseEnter(row(container, 1));
    expect(screen.getByText("3 lines selected")).toBeInTheDocument();
  });

  it("d twice with lines selected never falls back to the hunk discard", () => {
    const onDiscardHunk = vi.fn();
    const { container } = render(
      <DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={vi.fn()} onDiscardHunk={onDiscardHunk} />,
    );
    fireEvent.click(row(container, 2));
    const group = screen.getByRole("group", { name: "Diff hunks" });
    fireEvent.keyDown(group, { key: "d" });
    fireEvent.keyDown(group, { key: "d" });
    expect(onDiscardHunk).not.toHaveBeenCalled();
  });

  it("dragging lines in another hunk makes that hunk active for s", () => {
    const second: DiffHunk = { ...lineHunk, oldStart: 20, newStart: 21 };
    const onStageHunk = vi.fn();
    const onStageLines = vi.fn();
    const { container } = render(
      <DiffView hunks={[lineHunk, second]} onStageHunk={onStageHunk} onStageLines={onStageLines} />,
    );
    const cell = (index: number) =>
      container.querySelector<HTMLElement>(`[data-hunk-index="1"][data-line-index="${index}"]`)!;
    fireEvent.mouseDown(cell(1), { button: 0 });
    fireEvent.mouseEnter(cell(2));
    fireEvent.mouseUp(window);
    fireEvent.keyDown(screen.getByRole("group", { name: "Diff hunks" }), { key: "s" });
    expect(onStageLines).toHaveBeenCalledWith(20, 21, [1, 2], shown);
    expect(onStageHunk).not.toHaveBeenCalled();
  });

  it("keeps the ignore-whitespace hint when the diff comes back empty", () => {
    render(<DiffView hunks={[]} onStageHunk={vi.fn()} onStageLines={vi.fn()} partialActionsDisabled />);
    expect(screen.getByText("Turn off Ignore whitespace to stage partially.")).toBeInTheDocument();
  });

  it("selection clears when hunks change", () => {
    const { container, rerender } = render(<DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={vi.fn()} />);
    fireEvent.click(row(container, 2));
    rerender(<DiffView hunks={[{ ...lineHunk }]} onStageHunk={vi.fn()} onStageLines={vi.fn()} />);
    expect(screen.queryByText(/lines? selected/)).toBeNull();
  });
});

describe("DiffView options toolbar", () => {
  it("toggles ignore whitespace and steps context lines through the settings context", () => {
    const update = vi.fn();
    render(
      <TestDiffSettings value={{ settings: { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true }, update }}>
        <DiffView hunks={[lineHunk]} />
      </TestDiffSettings>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Ignore whitespace" }));
    expect(update).toHaveBeenCalledWith({ ignoreWhitespace: true });
    fireEvent.click(screen.getByRole("button", { name: "More context lines" }));
    expect(update).toHaveBeenCalledWith({ contextLines: 4 });
  });
});

describe("DiffView syntax highlighting", () => {
  it("colors tokens when highlighting is on", async () => {
    const { container } = render(
      <DiffView hunks={[{ ...lineHunk, lines: [{ origin: "Add", content: "const x = 1;\n" }] }]} path="f.ts" />,
    );
    await waitFor(() => expect(container.querySelector('[style*="--shiki-token"]')).not.toBeNull(), { timeout: 15000 });
  }, 20000);
});
