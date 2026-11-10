import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DiffHunk } from "../ipc/RepoClient";
import { wordDiff } from "../lib/wordDiff";
import type { ReactNode } from "react";
import { DiffSettingsContext, type DiffSettingsApi } from "../state/useDiffSettings";
import { DiffView as PaneDiffView } from "./DiffView";
import { DiffOptionsToolbar } from "./DiffOptionsToolbar";
import { DiffViewOptionsProvider } from "../state/DiffViewOptionsProvider";

// A diff as a pane shows it (#640): under the pane's one options toolbar, sharing its options.
function DiffView(props: React.ComponentProps<typeof PaneDiffView>) {
  return (
    <DiffViewOptionsProvider>
      <DiffOptionsToolbar wholeFile={props.wholeFile !== undefined} />
      <PaneDiffView {...props} />
    </DiffViewOptionsProvider>
  );
}

// Split view and whole file persist in localStorage: no test may leave them on for the next.
afterEach(() => localStorage.clear());

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

  it("d discards the active hunk at once: Undo brings it back (#662)", () => {
    const hunks: DiffHunk[] = [{ oldStart: 5, oldLines: 1, newStart: 7, newLines: 1, lines: [] }];
    const onDiscardHunk = vi.fn();
    render(<DiffView hunks={hunks} onDiscardHunk={onDiscardHunk} />);
    const group = screen.getByRole("group", { name: "Diff hunks" });

    fireEvent.keyDown(group, { key: "d" });
    expect(onDiscardHunk).toHaveBeenCalledExactlyOnceWith(5, 7);
    expect(screen.queryByText("Confirm discard")).not.toBeInTheDocument();
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

  it("clicking Discard hunk calls onDiscardHunk at once, with no second confirmation", () => {
    const hunks: DiffHunk[] = [
      { oldStart: 5, oldLines: 2, newStart: 7, newLines: 3, lines: [] },
    ];
    const onDiscardHunk = vi.fn();

    render(<DiffView hunks={hunks} onDiscardHunk={onDiscardHunk} />);
    fireEvent.click(screen.getByText("Discard hunk"));

    expect(onDiscardHunk).toHaveBeenCalledExactlyOnceWith(5, 7);
  });

  it("marks Discard hunk with the danger tone and names each hunk action's key (#648)", () => {
    const hunks: DiffHunk[] = [{ oldStart: 5, oldLines: 2, newStart: 7, newLines: 3, lines: [] }];
    render(<DiffView hunks={hunks} onStageHunk={vi.fn()} onDiscardHunk={vi.fn()} />);
    const stage = screen.getByText("Stage hunk");
    const discard = screen.getByText("Discard hunk");
    expect(discard).toHaveAttribute("data-tone", "danger");
    expect(stage).not.toHaveAttribute("data-tone");
    expect(stage).toHaveAttribute("title", "Stage hunk (s)");
    expect(discard).toHaveAttribute("title", "Discard hunk (d). Undo brings it back.");
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
      <TestDiffSettings value={{ settings: { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true, wordWrap: "Auto", algorithm: "Default", showWhitespace: false, expandUntracked: true }, update }}>
        <DiffView hunks={[lineHunk]} />
      </TestDiffSettings>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Ignore whitespace" }));
    expect(update).toHaveBeenCalledWith({ ignoreWhitespace: true });
    fireEvent.click(screen.getByRole("button", { name: "More context lines" }));
    expect(update).toHaveBeenCalledWith({ contextLines: 4 });
  });
});

describe("DiffView show whitespace (#616)", () => {
  const wsHunk: DiffHunk = {
    oldStart: 1, oldLines: 2, newStart: 1, newLines: 2,
    lines: [
      { origin: "Context", content: "x y\r\n" },
      { origin: "Remove", content: "\tvalue\n" },
      { origin: "Add", content: "  value \n" },
    ],
  };
  const settingsWith = (showWhitespace: boolean, syntaxHighlight = false) => ({
    settings: { ignoreWhitespace: false, contextLines: 3, syntaxHighlight, wordWrap: "Auto" as const, algorithm: "Default" as const, showWhitespace, expandUntracked: true },
    update: vi.fn(),
  });
  const contents = (container: HTMLElement) =>
    [...container.querySelectorAll<HTMLElement>(".diff-line")].map((line) => line.lastElementChild as HTMLElement);
  const markers = (el: Element) =>
    [...el.querySelectorAll<HTMLElement>("[data-whitespace]")].map(
      (m) => `${m.dataset.whitespace}${m.dataset.trailing === undefined ? "" : "!"}`,
    );

  it("toggles through the settings context", () => {
    const value = settingsWith(false);
    render(
      <TestDiffSettings value={value}>
        <DiffView hunks={[wsHunk]} />
      </TestDiffSettings>,
    );
    const button = screen.getByRole("button", { name: "Show whitespace" });
    expect(button).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(button);
    expect(value.update).toHaveBeenCalledWith({ showWhitespace: true });
  });

  it("renders no markers while off", () => {
    const { container } = render(
      <TestDiffSettings value={settingsWith(false)}>
        <DiffView hunks={[wsHunk]} />
      </TestDiffSettings>,
    );
    expect(container.querySelector("[data-whitespace]")).toBeNull();
  });

  it.each([false, true])("marks spaces, tabs, carriage returns and an added line's trailing space (split %s)", (split) => {
    const { container } = render(
      <TestDiffSettings value={settingsWith(true)}>
        <DiffView hunks={[wsHunk]} />
      </TestDiffSettings>,
    );
    if (split) fireEvent.click(screen.getByRole("button", { name: "Split view" }));
    expect(screen.getByRole("button", { name: "Show whitespace" })).toHaveAttribute("aria-pressed", "true");
    const [context, removed, added] = contents(container);
    expect(markers(context)).toEqual(["space", "cr"]);
    expect(markers(removed)).toEqual(["tab"]);
    // Only the trailing space is highlighted, not the indentation.
    expect(markers(added)).toEqual(["space", "space", "space!"]);
    // Display only: the text (what a copy gives) is the original line.
    expect([context, removed, added].map((c) => c.textContent)).toEqual(wsHunk.lines.map((l) => l.content));
  });

  it("does not highlight trailing whitespace on removed or context lines", () => {
    const { container } = render(
      <TestDiffSettings value={settingsWith(true)}>
        <DiffView hunks={[{ ...wsHunk, lines: [{ origin: "Context", content: "a \n" }, { origin: "Remove", content: "b\t\n" }] }]} />
      </TestDiffSettings>,
    );
    expect(container.querySelector("[data-trailing]")).toBeNull();
    expect(container.querySelectorAll("[data-whitespace]")).toHaveLength(2);
  });

  it("keeps markers and the original text with syntax highlighting on", async () => {
    const hunk: DiffHunk = { ...wsHunk, lines: [{ origin: "Add", content: "\tconst x = 1; \n" }] };
    const { container } = render(
      <TestDiffSettings value={settingsWith(true, true)}>
        <DiffView hunks={[hunk]} path="f.ts" />
      </TestDiffSettings>,
    );
    await waitFor(() => expect(container.querySelector('[style*="--shiki-token"]')).not.toBeNull(), { timeout: 15000 });
    const [added] = contents(container);
    expect(markers(added)).toEqual(["tab", "space", "space", "space", "space!"]);
    expect(added.textContent).toBe("\tconst x = 1; \n");
  }, 20000);
});

describe("DiffView syntax highlighting", () => {
  it("colors tokens when highlighting is on", async () => {
    const { container } = render(
      <DiffView hunks={[{ ...lineHunk, lines: [{ origin: "Add", content: "const x = 1;\n" }] }]} path="f.ts" />,
    );
    await waitFor(() => expect(container.querySelector('[style*="--shiki-token"]')).not.toBeNull(), { timeout: 15000 });
  }, 20000);
});

describe("DiffView word wrap", () => {
  const settingsWith = (wordWrap: "Auto" | "On" | "Off") => ({
    settings: { ignoreWhitespace: false, contextLines: 3, syntaxHighlight: true, wordWrap, algorithm: "Default" as const, showWhitespace: false, expandUntracked: true },
    update: vi.fn(),
  });
  const wrapButton = () => screen.getByRole("button", { name: "Wrap lines" });
  const lines = (container: HTMLElement) => container.querySelector<HTMLElement>("[data-wrap]")!;

  it("follows the setting: Auto leaves code unwrapped in the unified view and wraps the split view", () => {
    const { container } = render(
      <TestDiffSettings value={settingsWith("Auto")}>
        <DiffView hunks={[lineHunk]} path="src/main.rs" />
      </TestDiffSettings>,
    );
    expect(lines(container)).toHaveAttribute("data-wrap", "false");
    expect(wrapButton()).toHaveAttribute("aria-pressed", "mixed");
    fireEvent.click(screen.getByRole("button", { name: "Split view" }));
    expect(lines(container)).toHaveAttribute("data-wrap", "true");
  });

  it("the toolbar's Wrap lines sets the setting, so it applies to every diff and persists (#640)", () => {
    const value = settingsWith("On");
    const { container } = render(
      <TestDiffSettings value={value}>
        <DiffView hunks={[lineHunk]} path="src/main.rs" />
      </TestDiffSettings>,
    );
    expect(lines(container)).toHaveAttribute("data-wrap", "true");
    expect(wrapButton()).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(wrapButton());
    expect(value.update).toHaveBeenCalledWith({ wordWrap: "Off" });
  });

  it("keeps line numbers and line selection when unwrapped", () => {
    const onStageLines = vi.fn();
    const { container } = render(
      <TestDiffSettings value={settingsWith("Off")}>
        <DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={onStageLines} />
      </TestDiffSettings>,
    );
    expect(lines(container)).toHaveAttribute("data-wrap", "false");
    fireEvent.click(row(container, 4));
    fireEvent.click(screen.getByRole("button", { name: "Stage lines (1)" }));
    expect(onStageLines).toHaveBeenCalledWith(1, 1, [4], shown);
    expect(row(container, 4).querySelectorAll('[aria-hidden="true"]')[1]).toHaveTextContent("4");
  });
});

describe("DiffView whole file", () => {
  it("offers Show whole file only when the pane's diffs can fetch it", () => {
    const { rerender } = render(<DiffView hunks={[lineHunk]} />);
    expect(screen.queryByRole("button", { name: "Show whole file" })).toBeNull();
    rerender(<DiffView hunks={[lineHunk]} wholeFile={{ on: false, onToggle: vi.fn(), tooLongLines: null }} />);
    expect(screen.getByRole("button", { name: "Show whole file" })).toBeInTheDocument();
  });

  it("says when the file is too long to show whole", () => {
    render(<DiffView hunks={[lineHunk]} wholeFile={{ on: true, onToggle: vi.fn(), tooLongLines: 12345 }} />);
    expect(screen.getByText(/too long to show whole \(12,345 lines\)/)).toBeInTheDocument();
  });
});

describe("DiffView for screen readers (#668)", () => {
  it("reads each line's kind and number before its text, not its marker or gutters", () => {
    const { container } = render(<DiffView hunks={[lineHunk]} />);
    const read = [...container.querySelectorAll<HTMLElement>(".diff-line")].map((line) =>
      [...line.childNodes]
        .filter((node) => !(node instanceof HTMLElement && node.getAttribute("aria-hidden") === "true"))
        .map((node) => (node as HTMLElement).textContent?.replace(/^[+\- ]/, ""))
        .join(""),
    );
    expect(read).toEqual([
      "Unchanged line 1: a\n",
      "Removed line 2: b\n",
      "Added line 2: B\n",
      "Unchanged line 3: c\n",
      "Added line 4: d\n",
    ]);
  });

  it("labels split-view lines by their own side's number", () => {
    const { container } = render(<DiffView hunks={[lineHunk]} />);
    fireEvent.click(screen.getByRole("button", { name: "Split view" }));
    expect(container.querySelector('[data-diff-column="old"]')).toHaveTextContent("Removed line 2:");
    expect(container.querySelector('[data-diff-column="new"]')).toHaveTextContent("Added line 2:");
  });

  it("does not label the no-newline marker line with a number", () => {
    const hunk: DiffHunk = {
      oldStart: 1, oldLines: 1, newStart: 1, newLines: 1,
      lines: [
        { origin: "Remove", content: "x" },
        { origin: "Context", content: "\\ No newline at end of file\n" },
        { origin: "Add", content: "y\n" },
      ],
    };
    const { container } = render(<DiffView hunks={[hunk]} />);
    expect(container.querySelector(".diff-line-context")).not.toHaveTextContent("Unchanged");
  });

  it("makes selectable lines options of a listbox per hunk, with their selected state", () => {
    render(<DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={vi.fn()} />);
    const listbox = screen.getByRole("listbox", { name: "Lines of hunk 1 of 1" });
    expect(listbox).toHaveAttribute("aria-multiselectable", "true");
    const options = within(listbox).getAllByRole("option");
    expect(options).toHaveLength(5);
    expect(options[0]).toHaveAttribute("aria-disabled", "true");
    expect(options[0]).not.toHaveAttribute("aria-selected");
    const added = within(listbox).getByRole("option", { name: /^Added line 4:\s*d$/ });
    expect(added).toHaveAttribute("aria-selected", "false");
    fireEvent.click(added);
    expect(added).toHaveAttribute("aria-selected", "true");
  });

  it("points the diff's active descendant at the keyboard cursor", () => {
    render(<DiffView hunks={[lineHunk]} onStageHunk={vi.fn()} onStageLines={vi.fn()} />);
    const group = screen.getByRole("group", { name: "Diff hunks" });
    expect(group).not.toHaveAttribute("aria-activedescendant");
    fireEvent.keyDown(group, { key: "ArrowDown" });
    const removed = screen.getByRole("option", { name: /^Removed line 2:\s*b$/ });
    expect(group).toHaveAttribute("aria-activedescendant", removed.id);
    fireEvent.keyDown(group, { key: "ArrowDown" });
    expect(group).toHaveAttribute("aria-activedescendant", screen.getByRole("option", { name: /^Added line 2:\s*B$/ }).id);
  });

  it("keeps a read-only diff's lines free of selection roles", () => {
    render(<DiffView hunks={[lineHunk]} />);
    expect(screen.queryByRole("listbox")).toBeNull();
    expect(screen.queryByRole("option")).toBeNull();
    expect(document.querySelector("[aria-selected]")).toBeNull();
  });
});

// A hunk of `count` added lines, numbered from `start`.
const addedHunk = (count: number, newStart = 1): DiffHunk => ({
  oldStart: 0, oldLines: 0, newStart, newLines: count,
  lines: Array.from({ length: count }, (_, i) => ({ origin: "Add" as const, content: `line ${newStart + i}\n` })),
});

describe("DiffView large and generated diffs (#686)", () => {
  const renderedLines = () => document.querySelectorAll(".diff-line").length;

  it("collapses a generated file's diff until Load diff is pressed, then focuses the diff", () => {
    render(<DiffView hunks={[addedHunk(3)]} path="frontend/pnpm-lock.yaml" onStageHunk={vi.fn()} />);
    expect(renderedLines()).toBe(0);
    expect(screen.getByText(/looks generated/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Load diff (3 lines)" }));
    expect(renderedLines()).toBe(3);
    expect(screen.getByRole("group", { name: "Diff hunks" })).toHaveFocus();
  });

  it("collapses a diff longer than 1,000 lines, but not one of 1,000", () => {
    const { rerender } = render(<DiffView hunks={[addedHunk(1000)]} path="a.txt" />);
    expect(renderedLines()).toBe(1000);
    rerender(<DiffView hunks={[addedHunk(1001)]} path="b.txt" />);
    expect(renderedLines()).toBe(0);
    expect(screen.getByText("This diff is large (1,001 lines), so it is collapsed.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Load diff (1,001 lines)" }));
    expect(renderedLines()).toBe(1001);
  });

  it("keeps a loaded diff loaded across a refetch, and collapses the next file's", () => {
    const { rerender } = render(<DiffView hunks={[addedHunk(2)]} path="Cargo.lock" />);
    fireEvent.click(screen.getByRole("button", { name: "Load diff (2 lines)" }));
    rerender(<DiffView hunks={[addedHunk(3)]} path="Cargo.lock" />);
    expect(renderedLines()).toBe(3);
    rerender(<DiffView hunks={[addedHunk(3)]} path="sub/Cargo.lock" />);
    expect(renderedLines()).toBe(0);
  });

  it("shows a loaded long diff 2,000 lines at a time", () => {
    render(<DiffView hunks={[addedHunk(1500), addedHunk(3000, 2000)]} path="big.txt" />);
    fireEvent.click(screen.getByRole("button", { name: "Load diff (4,500 lines)" }));
    expect(renderedLines()).toBe(2000);
    expect(screen.getByText("2,500 more lines not shown.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Show 2,000 more" }));
    expect(renderedLines()).toBe(4000);
    expect(screen.queryByRole("button", { name: "Show all" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Show 500 more" }));
    expect(renderedLines()).toBe(4500);
    expect(screen.queryByText(/not shown/)).toBeNull();
  });

  it("Show all renders every line", () => {
    render(<DiffView hunks={[addedHunk(5000)]} path="big.txt" />);
    fireEvent.click(screen.getByRole("button", { name: "Load diff (5,000 lines)" }));
    fireEvent.click(screen.getByRole("button", { name: "Show all" }));
    expect(renderedLines()).toBe(5000);
  });

  it("] to a hunk past the window shows it, and its actions use the whole hunk", () => {
    const onStageHunk = vi.fn();
    const onStageLines = vi.fn();
    const first = addedHunk(2500);
    render(
      <DiffView
        hunks={[first, addedHunk(10, 5000)]}
        path="big.txt"
        onStageHunk={onStageHunk}
        onStageLines={onStageLines}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Load diff (2,510 lines)" }));
    expect(renderedLines()).toBe(2000);
    // A line of the truncated hunk stages with every line of that hunk as expected content.
    fireEvent.click(document.querySelector<HTMLElement>('[data-hunk-index="0"][data-line-index="5"]')!);
    fireEvent.click(screen.getByRole("button", { name: "Stage lines (1)" }));
    expect(onStageLines).toHaveBeenCalledWith(0, 1, [5], first.lines.map((l) => l.content));
    const group = screen.getByRole("group", { name: "Diff hunks" });
    fireEvent.keyDown(group, { key: "]" });
    expect(renderedLines()).toBe(2510);
    fireEvent.keyDown(group, { key: "s" });
    expect(onStageHunk).toHaveBeenCalledWith(0, 5000);
  });

  it("shows more when the line cursor moves past the window", () => {
    render(<DiffView hunks={[addedHunk(2001)]} path="big.txt" onStageHunk={vi.fn()} onStageLines={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "Load diff (2,001 lines)" }));
    fireEvent.click(document.querySelector<HTMLElement>('[data-hunk-index="0"][data-line-index="1999"]')!);
    const group = screen.getByRole("group", { name: "Diff hunks" });
    fireEvent.keyDown(group, { key: "ArrowDown" });
    expect(renderedLines()).toBe(2001);
    expect(group).toHaveAttribute(
      "aria-activedescendant",
      document.querySelector('[data-line-index="2000"]')!.id,
    );
  });
});
