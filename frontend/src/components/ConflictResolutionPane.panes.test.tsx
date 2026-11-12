import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RepoError, type ConflictSegment, type RepoClient } from "../ipc/RepoClient";
import { ConflictResolutionPane } from "./ConflictResolutionPane";
import { fakeRepoClient } from "../test/fakeRepoClient";
import { DEFAULT_DIFF_SETTINGS } from "../ipc/generated/wire-constants";
import { DiffSettingsContext } from "../state/useDiffSettings";
import { MergeEditorContext, useMergeEditorRegistry, type MergeEditorRegistry } from "../state/useCurrentMergeEditor";

// The three-pane merge editor (#875): ours | result | theirs for the whole file, the default
// layout. The block view (#720) is covered by ConflictResolutionPane.test.tsx.

const TEST_REPO_PATH = "/repo";

function fakeClient(overrides: Partial<RepoClient> = {}): RepoClient {
  return fakeRepoClient(overrides, "empty");
}

function mergeClient(segments: ConflictSegment[]): RepoClient {
  return fakeClient({
    getConflictHunks: async () => segments,
    getHeadInfo: async () => ({ ...(await fakeClient().getHeadInfo(TEST_REPO_PATH)), operation: "Merge" }),
    listBranches: async () => [{ name: "main", isCurrent: true }],
    getMergeMessage: async () => "Merge branch 'feature'\n",
  });
}

function renderPane(segments: ConflictSegment[], options: { client?: RepoClient; path?: string } = {}) {
  const onResolve = vi.fn();
  const view = render(
    <ConflictResolutionPane
      repoPath={TEST_REPO_PATH}
      client={options.client ?? fakeClient({ getConflictHunks: async () => segments })}
      path={options.path ?? "shared.txt"}
      onResolve={onResolve}
      onResolveAddDelete={vi.fn()}
    />,
  );
  return { onResolve, ...view };
}

/** Where a line's text is shown: its pane, its line number there, and the row it sits in. */
function placeOf(text: HTMLElement) {
  const line = text.closest("[data-line]");
  const cell = text.closest("[data-pane]");
  return {
    pane: cell?.getAttribute("data-pane") ?? null,
    line: line === null ? null : Number(line.getAttribute("data-line")),
    row: cell?.parentElement ?? null,
  };
}

/** The panes a line's text appears in, with that pane's line number. */
function linesOf(text: string): [string | null, number | null][] {
  return screen.getAllByText(text).map((element) => {
    const place = placeOf(element);
    return [place.pane, place.line];
  });
}

function cellOf(region: HTMLElement, pane: string): HTMLElement {
  const cell = region.querySelector<HTMLElement>(`[data-pane="${pane}"]`);
  if (cell === null) throw new Error(`no ${pane} pane in ${region.getAttribute("aria-label")}`);
  return cell;
}

function numbersIn(cell: HTMLElement): number[] {
  return [...cell.querySelectorAll("[data-line]")].map((line) => Number(line.getAttribute("data-line")));
}

// Several blocks of unequal lengths; the second one's ours side removed its lines.
const unequal: ConflictSegment[] = [
  { kind: "Clean", content: "a\nb\n" },
  { kind: "Conflict", ours: "o1\no2\no3\n", base: "x\n", theirs: "t1\n" },
  { kind: "Clean", content: "c\n" },
  { kind: "Conflict", ours: "", base: "y\n", theirs: "t2\nt3\n" },
  { kind: "Clean", content: "d\n" },
];

beforeEach(() => {
  localStorage.clear();
});

describe("ConflictResolutionPane three-pane view (#875)", () => {
  it("shows the file in three panes, ours | result | theirs, by default", async () => {
    renderPane(unequal, { client: mergeClient(unequal) });
    await screen.findByRole("region", { name: "Conflict 1 of 2" });

    const layout = screen.getByRole("group", { name: "Merge editor layout" });
    expect(within(layout).getByRole("button", { name: "Three panes" })).toHaveAttribute("aria-pressed", "true");
    expect(within(layout).getByRole("button", { name: "Blocks" })).toHaveAttribute("aria-pressed", "false");
    // Each pane is labelled once, at its top, with what the side is.
    expect(await screen.findByText("Ours (main)")).toBeInTheDocument();
    expect(screen.getByText("Theirs (feature)")).toBeInTheDocument();
    const headers = [...document.querySelectorAll("[data-pane-header]")].map((header) => [
      header.getAttribute("data-pane-header"),
      header.textContent,
    ]);
    expect(headers).toEqual([
      ["ours", "Ours (main)"],
      ["result", "Result"],
      ["theirs", "Theirs (feature)"],
    ]);
    // The unchanged lines are in all three panes, in that order.
    expect(linesOf("a").map(([pane]) => pane)).toEqual(["ours", "result", "theirs"]);
  });

  it("aligns the unchanged lines across the panes and numbers each pane's own lines", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const second = screen.getByRole("region", { name: "Conflict 2 of 2" });

    expect(linesOf("a")).toEqual([
      ["ours", 1],
      ["result", 1],
      ["theirs", 1],
    ]);
    expect(linesOf("b")).toEqual([
      ["ours", 2],
      ["result", 2],
      ["theirs", 2],
    ]);
    // A block's sides sit next to each other, whatever their lengths.
    expect(numbersIn(cellOf(first, "ours"))).toEqual([3, 4, 5]);
    expect(numbersIn(cellOf(first, "theirs"))).toEqual([3]);
    expect(cellOf(first, "ours").parentElement).toBe(cellOf(first, "theirs").parentElement);
    expect(cellOf(first, "result").parentElement).toBe(cellOf(first, "ours").parentElement);
    // After it the panes line up again, each counting its own lines; an unchosen block has no
    // lines in the result yet.
    expect(linesOf("c")).toEqual([
      ["ours", 6],
      ["result", 3],
      ["theirs", 4],
    ]);
    const rows = new Set(screen.getAllByText("c").map((text) => placeOf(text).row));
    expect(rows.size).toBe(1);
    expect(numbersIn(cellOf(second, "theirs"))).toEqual([5, 6]);
    expect(linesOf("d")).toEqual([
      ["ours", 7],
      ["result", 4],
      ["theirs", 7],
    ]);
  });

  it("numbers the result's lines from the blocks' results", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const second = screen.getByRole("region", { name: "Conflict 2 of 2" });

    fireEvent.click(within(first).getByRole("button", { name: "Accept ours" }));
    expect(within(first).getByLabelText("Result")).toHaveValue("o1\no2\no3\n");
    expect(numbersIn(cellOf(first, "result"))).toEqual([3, 4, 5]);
    expect(linesOf("c")).toEqual([
      ["ours", 6],
      ["result", 6],
      ["theirs", 4],
    ]);

    fireEvent.click(within(second).getByRole("button", { name: "Accept theirs" }));
    expect(numbersIn(cellOf(second, "result"))).toEqual([7, 8]);
    expect(linesOf("d")).toEqual([
      ["ours", 7],
      ["result", 9],
      ["theirs", 7],
    ]);
  });

  it("shows a block's side without lines as empty", async () => {
    renderPane(unequal);
    const second = await screen.findByRole("region", { name: "Conflict 2 of 2" });
    expect(cellOf(second, "ours")).toHaveTextContent("Empty: no lines on this side");
    expect(numbersIn(cellOf(second, "ours"))).toEqual([]);
    fireEvent.click(within(second).getByRole("button", { name: "Accept ours" }));
    expect(within(second).getByLabelText("Result")).toHaveValue("");
    expect(screen.getByRole("status")).toHaveTextContent("1 of 2 resolved");
  });

  it("shows an add/add conflict from line 1, with no base", async () => {
    renderPane([{ kind: "Conflict", ours: "x1\nx2\n", base: "", theirs: "y1\n" }]);
    const block = await screen.findByRole("region", { name: "Conflict 1 of 1" });
    expect(numbersIn(cellOf(block, "ours"))).toEqual([1, 2]);
    expect(numbersIn(cellOf(block, "theirs"))).toEqual([1]);
    fireEvent.click(screen.getByRole("button", { name: "Show base" }));
    expect(within(block).getByText("Empty: no common lines")).toBeInTheDocument();
  });

  it("collapses a long unchanged run once across the panes and shows it on request", async () => {
    const lines = Array.from({ length: 12 }, (_, i) => `same ${i + 1}\n`).join("");
    renderPane([
      { kind: "Conflict", ours: "a\n", base: "", theirs: "b\n" },
      { kind: "Clean", content: lines },
      { kind: "Conflict", ours: "c\n", base: "", theirs: "d\n" },
    ]);
    const expand = await screen.findByRole("button", { name: "Show 6 unchanged lines" });
    expect(screen.getAllByRole("button", { name: /unchanged lines/ })).toHaveLength(1);
    expect(linesOf("same 3")).toEqual([
      ["ours", 4],
      ["result", 3],
      ["theirs", 4],
    ]);
    expect(screen.queryByText("same 6")).not.toBeInTheDocument();
    expect(linesOf("same 10")).toEqual([
      ["ours", 11],
      ["result", 10],
      ["theirs", 11],
    ]);

    fireEvent.click(expand);
    expect(linesOf("same 6")).toEqual([
      ["ours", 7],
      ["result", 6],
      ["theirs", 7],
    ]);
    expect(screen.queryByRole("button", { name: /unchanged lines/ })).not.toBeInTheDocument();
  });

  it("takes a block's choices by button and key, edits its result, and saves", async () => {
    const { onResolve } = renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const second = screen.getByRole("region", { name: "Conflict 2 of 2" });
    const save = screen.getByRole("button", { name: "Save resolution" });
    expect(screen.getByRole("status")).toHaveTextContent("0 of 2 resolved");
    expect(save).toBeDisabled();

    fireEvent.click(within(first).getByRole("button", { name: "Ours then theirs" }));
    expect(within(first).getByRole("button", { name: "Ours then theirs" })).toHaveAttribute("aria-pressed", "true");
    expect(within(first).getByLabelText("Result")).toHaveValue("o1\no2\no3\nt1\n");

    act(() => second.focus());
    fireEvent.keyDown(second, { key: "2", code: "Digit2" });
    expect(within(second).getByRole("button", { name: "Accept theirs" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("status")).toHaveTextContent("2 of 2 resolved");

    fireEvent.change(within(second).getByLabelText("Result"), { target: { value: "typed\n" } });
    expect(within(second).getByText("Edited")).toBeInTheDocument();
    expect(save).toBeEnabled();
    fireEvent.click(save);
    expect(onResolve).toHaveBeenCalledWith("shared.txt", "a\nb\no1\no2\no3\nt1\nc\ntyped\nd\n");
  });

  it("moves between the blocks with F7 and the palette", async () => {
    let registry: MergeEditorRegistry | null = null;
    function Harness({ children }: { children: ReactNode }) {
      const value = useMergeEditorRegistry();
      registry = value;
      return <MergeEditorContext.Provider value={value}>{children}</MergeEditorContext.Provider>;
    }
    render(
      <Harness>
        <ConflictResolutionPane
          repoPath={TEST_REPO_PATH}
          client={fakeClient({ getConflictHunks: async () => unequal })}
          path="shared.txt"
          onResolve={vi.fn()}
          onResolveAddDelete={vi.fn()}
        />
      </Harness>,
    );
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const second = screen.getByRole("region", { name: "Conflict 2 of 2" });
    act(() => first.focus());
    fireEvent.keyDown(first, { key: "F7" });
    expect(second).toHaveFocus();
    fireEvent.click(screen.getByRole("button", { name: "Previous conflict" }));
    expect(first).toHaveFocus();

    await waitFor(() => expect(registry?.current).toMatchObject({ path: "shared.txt" }));
    act(() => registry!.current!.next());
    expect(second).toHaveFocus();
    act(() => registry!.current!.choose("theirs"));
    expect(within(second).getByLabelText("Result")).toHaveValue("t2\nt3\n");
  });

  it("shows a block's base behind Show base and edits the whole result", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    expect(within(first).queryByText("Base")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Show base" }));
    expect(within(first).getByText("Base")).toBeInTheDocument();
    expect(within(first).getByText("x")).toBeInTheDocument();
    fireEvent.click(within(first).getByRole("button", { name: "Accept base" }));
    expect(within(first).getByLabelText("Result")).toHaveValue("x\n");

    fireEvent.click(screen.getByRole("button", { name: "Edit whole result" }));
    expect(screen.getByLabelText("Whole result")).toHaveValue(
      "a\nb\nx\nc\n<<<<<<< ours\n||||||| base\ny\n=======\nt2\nt3\n>>>>>>> theirs\nd\n",
    );
    fireEvent.click(screen.getByRole("button", { name: "Back to blocks" }));
    // Back in the layout it came from.
    expect(within(screen.getByRole("group", { name: "Merge editor layout" })).getByRole("button", { name: "Three panes" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(document.querySelector("[data-pane-header]")).not.toBeNull();
  });

  it("switches to the block view, keeps the choices, and remembers the layout", async () => {
    const { unmount } = renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    fireEvent.click(within(first).getByRole("button", { name: "Accept theirs" }));

    fireEvent.click(screen.getByRole("button", { name: "Blocks" }));
    expect(screen.getByRole("button", { name: "Blocks" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "Three panes" })).toHaveAttribute("aria-pressed", "false");
    expect(document.querySelector("[data-pane]")).toBeNull();
    const block = screen.getByRole("region", { name: "Conflict 1 of 2" });
    expect(within(block).getByText("Ours")).toBeInTheDocument();
    expect(within(block).getByRole("button", { name: "Accept theirs" })).toHaveAttribute("aria-pressed", "true");
    expect(within(block).getByLabelText("Result")).toHaveValue("t1\n");
    unmount();

    renderPane(unequal);
    await screen.findByRole("region", { name: "Conflict 1 of 2" });
    expect(screen.getByRole("button", { name: "Blocks" })).toHaveAttribute("aria-pressed", "true");
    expect(document.querySelector("[data-pane]")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Three panes" }));
    expect(document.querySelector("[data-pane]")).not.toBeNull();
  });

  describe("in a narrow pane", () => {
    const width = window.innerWidth;
    beforeEach(() => {
      window.innerWidth = 480;
    });
    afterEach(() => {
      window.innerWidth = width;
    });

    it("falls back to the block view, keeping the chosen layout for a wider pane", async () => {
      renderPane(unequal);
      const block = await screen.findByRole("region", { name: "Conflict 1 of 2" });
      expect(document.querySelector("[data-pane]")).toBeNull();
      expect(within(block).getByText("Ours")).toBeInTheDocument();
      const threePanes = screen.getByRole("button", { name: "Three panes" });
      expect(threePanes).toBeDisabled();
      expect(threePanes).toHaveAttribute("aria-pressed", "false");
      expect(screen.getByRole("button", { name: "Blocks" })).toHaveAttribute("aria-pressed", "true");
      expect(localStorage.getItem("mergeEditor.view")).toBeNull();
    });
  });

  it("keeps the keep/delete choices for a file one side deleted", async () => {
    renderPane([], {
      client: fakeClient({
        getConflictHunks: async () => {
          throw new RepoError("AddDeleteConflict", "'gone.txt' is an add/delete conflict, not a text conflict");
        },
      }),
      path: "gone.txt",
    });
    expect(await screen.findByRole("button", { name: "Keep our version" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Delete file" })).toBeInTheDocument();
    expect(document.querySelector("[data-pane]")).toBeNull();
  });

  // #763: the panes follow the diff's syntax highlighting option.
  it("highlights the panes like the diff, the result fields staying plain", async () => {
    const tsSegments: ConflictSegment[] = [
      { kind: "Clean", content: "const one = 1;\n" },
      { kind: "Conflict", ours: "const ours = 2;\n", base: "const base = 2;\n", theirs: "const theirs = 2;\n" },
    ];
    const highlighted = (element: Element) => element.querySelector('[style*="--shiki-token"]') !== null;
    const pane = (syntaxHighlight: boolean) => (
      <DiffSettingsContext.Provider
        value={{
          settings: { ...DEFAULT_DIFF_SETTINGS, syntaxHighlight },
          update: () => {},
          error: null,
          dismissError: () => {},
        }}
      >
        <ConflictResolutionPane
          repoPath={TEST_REPO_PATH}
          client={fakeClient({ getConflictHunks: async () => tsSegments })}
          path="src/a.ts"
          onResolve={vi.fn()}
          onResolveAddDelete={vi.fn()}
        />
      </DiffSettingsContext.Provider>
    );
    const { unmount } = render(pane(true));
    const block = await screen.findByRole("region", { name: "Conflict 1 of 1" });
    await waitFor(() => expect(highlighted(cellOf(block, "theirs"))).toBe(true), { timeout: 15000 });
    expect(highlighted(cellOf(block, "ours"))).toBe(true);
    expect(cellOf(block, "theirs")).toHaveTextContent("const theirs = 2;");
    // The unchanged line in all three panes; only the block's result field stays plain.
    const cells = [...document.querySelectorAll("[data-pane]")].filter((cell) => cell.querySelector("textarea") === null);
    expect(cells.map((cell) => cell.getAttribute("data-pane"))).toEqual(["ours", "result", "theirs", "ours", "theirs"]);
    for (const cell of cells) expect(highlighted(cell)).toBe(true);
    unmount();

    const { container } = render(pane(false));
    await screen.findByRole("region", { name: "Conflict 1 of 1" });
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(highlighted(container)).toBe(false);
  }, 20000);
});
