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
    expect(first.querySelector('[data-pane="base"]')).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Show base" }));
    expect(cellOf(first, "base")).toBeInTheDocument();
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

// #876: an arrow in the ours and the theirs pane of every block puts that side into the result.
describe("ConflictResolutionPane take-side arrows (#876)", () => {
  const arrow = (region: HTMLElement, pane: "ours" | "theirs") =>
    within(cellOf(region, pane)).getByRole("button", { name: pane === "ours" ? "Take ours" : "Take theirs" });

  it("puts our lines into the result with the ours pane's arrow", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    fireEvent.click(arrow(first, "ours"));
    expect(within(first).getByLabelText("Result")).toHaveValue("o1\no2\no3\n");
    expect(within(first).getByRole("button", { name: "Accept ours" })).toHaveAttribute("aria-pressed", "true");
    expect(within(first).getByText("Ours", { selector: "span" })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("1 of 2 resolved");
    // Taken: the arrow has nothing more to add.
    expect(arrow(first, "ours")).toBeDisabled();
    expect(arrow(first, "theirs")).toBeEnabled();
  });

  it("puts their lines into the result with the theirs pane's arrow", async () => {
    renderPane(unequal);
    const second = await screen.findByRole("region", { name: "Conflict 2 of 2" });
    fireEvent.click(arrow(second, "theirs"));
    expect(within(second).getByLabelText("Result")).toHaveValue("t2\nt3\n");
    expect(within(second).getByRole("button", { name: "Accept theirs" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("status")).toHaveTextContent("1 of 2 resolved");
    expect(arrow(second, "theirs")).toBeDisabled();
    // The other block is untouched.
    expect(within(screen.getByRole("region", { name: "Conflict 1 of 2" })).getByLabelText("Result")).toHaveValue("");
  });

  it("appends the second side taken: ours then theirs, or theirs then ours", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const second = screen.getByRole("region", { name: "Conflict 2 of 2" });

    fireEvent.click(arrow(first, "ours"));
    fireEvent.click(arrow(first, "theirs"));
    expect(within(first).getByLabelText("Result")).toHaveValue("o1\no2\no3\nt1\n");
    expect(within(first).getByRole("button", { name: "Ours then theirs" })).toHaveAttribute("aria-pressed", "true");
    expect(arrow(first, "ours")).toBeDisabled();
    expect(arrow(first, "theirs")).toBeDisabled();

    fireEvent.click(arrow(second, "theirs"));
    fireEvent.click(arrow(second, "ours"));
    // Ours is empty in this block: theirs then (nothing of) ours.
    expect(within(second).getByLabelText("Result")).toHaveValue("t2\nt3\n");
    expect(within(second).getByRole("button", { name: "Theirs then ours" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("status")).toHaveTextContent("2 of 2 resolved");
  });

  it("takes theirs then ours on a block with lines on both sides", async () => {
    renderPane([{ kind: "Conflict", ours: "o\n", base: "", theirs: "t\n" }]);
    const block = await screen.findByRole("region", { name: "Conflict 1 of 1" });
    fireEvent.click(arrow(block, "theirs"));
    fireEvent.click(arrow(block, "ours"));
    expect(within(block).getByLabelText("Result")).toHaveValue("t\no\n");
  });

  it("is undone by Reset, which brings the block back to unresolved", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    fireEvent.click(arrow(first, "theirs"));
    expect(screen.getByRole("status")).toHaveTextContent("1 of 2 resolved");

    fireEvent.click(within(first).getByRole("button", { name: "Reset" }));
    expect(within(first).getByLabelText("Result")).toHaveValue("");
    expect(within(first).getByText("Not chosen")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("0 of 2 resolved");
    expect(arrow(first, "ours")).toBeEnabled();
    expect(arrow(first, "theirs")).toBeEnabled();
  });

  it("stays in sync with the choices and their keys", async () => {
    const { onResolve } = renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const second = screen.getByRole("region", { name: "Conflict 2 of 2" });

    // A key takes theirs; the arrow then appends ours after it.
    act(() => first.focus());
    fireEvent.keyDown(first, { key: "2", code: "Digit2" });
    expect(arrow(first, "theirs")).toBeDisabled();
    fireEvent.click(arrow(first, "ours"));
    expect(within(first).getByLabelText("Result")).toHaveValue("t1\no1\no2\no3\n");
    expect(within(first).getByRole("button", { name: "Theirs then ours" })).toHaveAttribute("aria-pressed", "true");

    // The base, or a typed result, is replaced by the side taken.
    fireEvent.click(within(second).getByRole("button", { name: "Accept base" }));
    fireEvent.click(arrow(second, "theirs"));
    expect(within(second).getByLabelText("Result")).toHaveValue("t2\nt3\n");
    fireEvent.change(within(second).getByLabelText("Result"), { target: { value: "typed\n" } });
    expect(arrow(second, "theirs")).toBeEnabled();
    fireEvent.click(arrow(second, "theirs"));
    expect(within(second).getByLabelText("Result")).toHaveValue("t2\nt3\n");

    fireEvent.click(screen.getByRole("button", { name: "Save resolution" }));
    expect(onResolve).toHaveBeenCalledWith("shared.txt", "a\nb\nt1\no1\no2\no3\nc\nt2\nt3\nd\n");
  });

  it("names the block each arrow acts on", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    expect(arrow(first, "ours")).toHaveAttribute("title", expect.stringContaining("Conflict 1 of 2"));
    expect(arrow(first, "theirs")).toHaveAttribute("title", expect.stringContaining("Conflict 1 of 2"));
  });

  it("has no arrows in the block view, which keeps its choices", async () => {
    localStorage.setItem("mergeEditor.view", "blocks");
    renderPane(unequal);
    await screen.findByRole("region", { name: "Conflict 1 of 2" });
    expect(screen.queryByRole("button", { name: "Take ours" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Take theirs" })).not.toBeInTheDocument();
  });
});

// #877: Show base adds the common ancestor as a fourth pane, aligned with the others (owner
// decision: a pane, not an overlay), in place of the block's base strip.
describe("ConflictResolutionPane base pane (#877)", () => {
  const headers = () =>
    [...document.querySelectorAll("[data-pane-header]")].map((header) => header.getAttribute("data-pane-header"));

  it("adds the base as a fourth pane with Show base, and takes it away again", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const toggle = screen.getByRole("button", { name: "Show base" });
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    expect(headers()).toEqual(["ours", "result", "theirs"]);
    expect(document.querySelector('[data-pane="base"]')).toBeNull();

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    expect(headers()).toEqual(["base", "ours", "result", "theirs"]);
    expect(document.querySelector('[data-pane-header="base"]')).toHaveTextContent("Base");
    expect(cellOf(first, "base")).toHaveTextContent("x");
    // No separate base strip above the block's panes any more: its base is in the row.
    expect(cellOf(first, "base").parentElement).toBe(cellOf(first, "ours").parentElement);
    expect(screen.getAllByText("x")).toHaveLength(1);

    fireEvent.click(toggle);
    expect(headers()).toEqual(["ours", "result", "theirs"]);
    expect(document.querySelector('[data-pane="base"]')).toBeNull();
  });

  it("aligns the base pane with the others and numbers the base's own lines", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const second = screen.getByRole("region", { name: "Conflict 2 of 2" });
    fireEvent.click(screen.getByRole("button", { name: "Show base" }));

    expect(linesOf("a")).toEqual([
      ["base", 1],
      ["ours", 1],
      ["result", 1],
      ["theirs", 1],
    ]);
    expect(numbersIn(cellOf(first, "base"))).toEqual([3]);
    expect(linesOf("c")).toEqual([
      ["base", 4],
      ["ours", 6],
      ["result", 3],
      ["theirs", 4],
    ]);
    expect(new Set(screen.getAllByText("c").map((text) => placeOf(text).row)).size).toBe(1);
    expect(numbersIn(cellOf(second, "base"))).toEqual([5]);
    expect(linesOf("d")).toEqual([
      ["base", 6],
      ["ours", 7],
      ["result", 4],
      ["theirs", 7],
    ]);
  });

  it("collapses a long unchanged run across all four panes", async () => {
    const lines = Array.from({ length: 12 }, (_, i) => `same ${i + 1}\n`).join("");
    renderPane([
      { kind: "Conflict", ours: "a\n", base: "", theirs: "b\n" },
      { kind: "Clean", content: lines },
      { kind: "Conflict", ours: "c\n", base: "z\n", theirs: "d\n" },
    ]);
    fireEvent.click(await screen.findByRole("button", { name: "Show base" }));
    expect(screen.getAllByRole("button", { name: /unchanged lines/ })).toHaveLength(1);
    expect(linesOf("same 10")).toEqual([
      ["base", 10],
      ["ours", 11],
      ["result", 10],
      ["theirs", 11],
    ]);
  });

  it("says a block without a base has no common lines (add/add)", async () => {
    renderPane([{ kind: "Conflict", ours: "x1\nx2\n", base: "", theirs: "y1\n" }]);
    const block = await screen.findByRole("region", { name: "Conflict 1 of 1" });
    fireEvent.click(screen.getByRole("button", { name: "Show base" }));
    expect(cellOf(block, "base")).toHaveTextContent("Empty: no common lines");
    expect(numbersIn(cellOf(block, "base"))).toEqual([]);
    expect(numbersIn(cellOf(block, "ours"))).toEqual([1, 2]);
  });

  it("keeps Accept base working and marks the base pane as taken", async () => {
    const { onResolve } = renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const second = screen.getByRole("region", { name: "Conflict 2 of 2" });
    fireEvent.click(screen.getByRole("button", { name: "Show base" }));
    expect(cellOf(first, "base")).not.toHaveAttribute("data-chosen");

    fireEvent.click(within(first).getByRole("button", { name: "Accept base" }));
    expect(within(first).getByLabelText("Result")).toHaveValue("x\n");
    expect(cellOf(first, "base")).toHaveAttribute("data-chosen", "true");
    expect(cellOf(first, "ours")).not.toHaveAttribute("data-chosen");
    act(() => second.focus());
    fireEvent.keyDown(second, { key: "5", code: "Digit5" });
    expect(within(second).getByLabelText("Result")).toHaveValue("y\n");

    fireEvent.click(screen.getByRole("button", { name: "Save resolution" }));
    expect(onResolve).toHaveBeenCalledWith("shared.txt", "a\nb\nx\nc\ny\nd\n");
  });

  it("keeps the base strip in the block view", async () => {
    localStorage.setItem("mergeEditor.view", "blocks");
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    fireEvent.click(screen.getByRole("button", { name: "Show base" }));
    expect(within(first).getByText("Base")).toBeInTheDocument();
    expect(document.querySelector("[data-pane]")).toBeNull();
  });
});

// #878: the result pane is editable for the whole file: unresolved blocks show git's conflict
// markers, resolved ones are highlighted by origin, and one undo history covers takes and typing.
describe("ConflictResolutionPane editable result pane (#878)", () => {
  const markers1 = "<<<<<<< ours\no1\no2\no3\n||||||| base\nx\n=======\nt1\n>>>>>>> theirs\n";
  const markers2 = "<<<<<<< ours\n||||||| base\ny\n=======\nt2\nt3\n>>>>>>> theirs\n";

  /** The result pane's text field in the row that shows `text` (an unchanged line). */
  function resultFieldWith(text: string): HTMLTextAreaElement {
    const line = screen.getAllByText(text).find((element) => placeOf(element).pane === "result");
    const field = line?.closest("[data-pane]")?.querySelector("textarea");
    if (field == null) throw new Error(`no result field next to ${text}`);
    return field;
  }

  /** Each result line's origin in `region`'s result pane, in order. */
  function originsIn(region: HTMLElement): (string | null)[] {
    return [...cellOf(region, "result").querySelectorAll("[data-line]")].map((line) => line.getAttribute("data-origin"));
  }

  const undoKey = (target: Element) => fireEvent.keyDown(target, { key: "z", code: "KeyZ", ctrlKey: true });
  const redoKey = (target: Element) =>
    fireEvent.keyDown(target, { key: "z", code: "KeyZ", ctrlKey: true, shiftKey: true });

  it("shows an unresolved block with git's conflict markers in the result and numbers them", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const second = screen.getByRole("region", { name: "Conflict 2 of 2" });
    expect(within(first).getByLabelText("Result")).toHaveValue(markers1);
    expect(within(second).getByLabelText("Result")).toHaveValue(markers2);
    expect(numbersIn(cellOf(first, "result"))).toEqual([3, 4, 5, 6, 7, 8, 9, 10, 11]);
    expect(originsIn(first)).toEqual(Array(9).fill("conflict"));
    expect(linesOf("c")).toEqual([
      ["ours", 6],
      ["result", 12],
      ["theirs", 4],
    ]);
    expect(screen.getByRole("status")).toHaveTextContent("0 of 2 resolved");
    expect(screen.getByRole("button", { name: "Save resolution" })).toBeDisabled();
  });

  it("types into the result outside the blocks, and saves what was typed", async () => {
    const { onResolve } = renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const second = screen.getByRole("region", { name: "Conflict 2 of 2" });

    fireEvent.change(resultFieldWith("a"), { target: { value: "a\nadded\nb\n" } });
    expect(resultFieldWith("a")).toHaveValue("a\nadded\nb\n");
    // The side panes keep their lines; the result's numbers after the edit move down one.
    expect(linesOf("b")).toEqual([
      ["ours", 2],
      ["result", 3],
      ["theirs", 2],
    ]);
    expect(linesOf("added")).toEqual([["result", 2]]);
    expect(placeOf(screen.getByText("added")).row).toBe(placeOf(screen.getAllByText("a")[0]).row);
    expect(screen.getByText("added").closest("[data-line]")).toHaveAttribute("data-origin", "edited");

    fireEvent.click(within(first).getByRole("button", { name: "Accept ours" }));
    fireEvent.click(within(second).getByRole("button", { name: "Accept theirs" }));
    fireEvent.change(resultFieldWith("d"), { target: { value: "d\nlast\n" } });
    fireEvent.click(screen.getByRole("button", { name: "Save resolution" }));
    expect(onResolve).toHaveBeenCalledWith("shared.txt", "a\nadded\nb\no1\no2\no3\nc\nt2\nt3\nd\nlast\n");
  });

  it("highlights a resolved block's lines by origin and marks a typed block Edited", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });

    fireEvent.click(within(first).getByRole("button", { name: "Accept ours" }));
    expect(originsIn(first)).toEqual(["ours", "ours", "ours"]);
    fireEvent.click(within(first).getByRole("button", { name: "Theirs then ours" }));
    expect(originsIn(first)).toEqual(["theirs", "ours", "ours", "ours"]);
    fireEvent.click(within(first).getByRole("button", { name: "Accept base" }));
    expect(originsIn(first)).toEqual(["base"]);

    fireEvent.change(within(first).getByLabelText("Result"), { target: { value: "x\nmine\n" } });
    expect(within(first).getByText("Edited")).toBeInTheDocument();
    expect(originsIn(first)).toEqual(["edited", "edited"]);
    expect(screen.getByRole("status")).toHaveTextContent("1 of 2 resolved");
  });

  it("resolves a block by typing over its conflict markers", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const second = screen.getByRole("region", { name: "Conflict 2 of 2" });
    const save = screen.getByRole("button", { name: "Save resolution" });
    fireEvent.click(within(second).getByRole("button", { name: "Accept ours" }));

    // Typed, but the start marker is left: still a conflict, and nothing can be saved.
    fireEvent.change(within(first).getByLabelText("Result"), {
      target: { value: "<<<<<<< ours\no1\no2\no3\nt1\n" },
    });
    expect(within(first).getByText("Edited")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("1 of 2 resolved");
    expect(save).toBeDisabled();
    expect(screen.getByText(/Remove the conflict markers/)).toBeInTheDocument();

    // An end marker typed into an unchanged run blocks the save too.
    fireEvent.change(within(first).getByLabelText("Result"), { target: { value: "o1\nt1\n" } });
    expect(screen.getByRole("status")).toHaveTextContent("2 of 2 resolved");
    expect(save).toBeEnabled();
    fireEvent.change(resultFieldWith("c"), { target: { value: "c\n>>>>>>> theirs\n" } });
    expect(save).toBeDisabled();
    fireEvent.change(resultFieldWith("c"), { target: { value: "c\n" } });
    expect(save).toBeEnabled();
  });

  it("undoes and redoes an arrow take with the keys and the buttons", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const undo = screen.getByRole("button", { name: "Undo" });
    const redo = screen.getByRole("button", { name: "Redo" });
    expect(undo).toBeDisabled();
    expect(redo).toBeDisabled();

    fireEvent.click(within(cellOf(first, "ours")).getByRole("button", { name: "Take ours" }));
    expect(within(first).getByLabelText("Result")).toHaveValue("o1\no2\no3\n");
    expect(undo).toBeEnabled();

    act(() => first.focus());
    undoKey(first);
    expect(within(first).getByLabelText("Result")).toHaveValue(markers1);
    expect(within(first).getByText("Not chosen")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("0 of 2 resolved");
    expect(undo).toBeDisabled();
    expect(redo).toBeEnabled();

    redoKey(first);
    expect(within(first).getByLabelText("Result")).toHaveValue("o1\no2\no3\n");
    fireEvent.click(undo);
    expect(within(first).getByLabelText("Result")).toHaveValue(markers1);
    fireEvent.click(redo);
    expect(within(first).getByLabelText("Result")).toHaveValue("o1\no2\no3\n");
    // Ctrl+Y redoes too.
    fireEvent.click(undo);
    fireEvent.keyDown(first, { key: "y", code: "KeyY", ctrlKey: true });
    expect(within(first).getByLabelText("Result")).toHaveValue("o1\no2\no3\n");
  });

  it("undoes typing in one step per field, and a new change drops the redo", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const field = within(first).getByLabelText("Result");
    fireEvent.click(within(first).getByRole("button", { name: "Accept theirs" }));

    // Keystrokes in one field go back together.
    fireEvent.change(field, { target: { value: "t1\nn" } });
    fireEvent.change(field, { target: { value: "t1\nne" } });
    fireEvent.change(field, { target: { value: "t1\nnew\n" } });
    fireEvent.change(resultFieldWith("c"), { target: { value: "c2\n" } });

    undoKey(resultFieldWith("c2"));
    expect(screen.queryByText("c2")).not.toBeInTheDocument();
    expect(resultFieldWith("c")).toHaveValue("c\n");
    undoKey(field);
    expect(field).toHaveValue("t1\n");
    expect(within(first).getByRole("button", { name: "Accept theirs" })).toHaveAttribute("aria-pressed", "true");
    undoKey(field);
    expect(field).toHaveValue(markers1);

    redoKey(field);
    expect(field).toHaveValue("t1\n");
    fireEvent.click(within(first).getByRole("button", { name: "Reset" }));
    expect(screen.getByRole("button", { name: "Redo" })).toBeDisabled();
    undoKey(first);
    expect(field).toHaveValue("t1\n");
  });

  it("keeps Ctrl+Z inside the merge editor away from the app's Git undo", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    const reached = vi.fn();
    window.addEventListener("keydown", reached);
    try {
      fireEvent.click(within(first).getByRole("button", { name: "Accept ours" }));
      undoKey(within(first).getByLabelText("Result"));
      redoKey(first);
      expect(reached).not.toHaveBeenCalled();
    } finally {
      window.removeEventListener("keydown", reached);
    }
  });

  it("undoes in the block view too, the history shared with the panes", async () => {
    renderPane(unequal);
    const first = await screen.findByRole("region", { name: "Conflict 1 of 2" });
    fireEvent.click(within(first).getByRole("button", { name: "Accept ours" }));
    fireEvent.click(screen.getByRole("button", { name: "Blocks" }));
    const block = screen.getByRole("region", { name: "Conflict 1 of 2" });
    fireEvent.click(within(block).getByRole("button", { name: "Accept theirs" }));
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(within(block).getByLabelText("Result")).toHaveValue("o1\no2\no3\n");
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    expect(within(block).getByText("Not chosen")).toBeInTheDocument();
  });

  it("stays quick on a 5,000-line file", async () => {
    const run = (from: number) => Array.from({ length: 2500 }, (_, i) => `line ${from + i}\n`).join("");
    const big: ConflictSegment[] = [
      { kind: "Clean", content: run(1) },
      { kind: "Conflict", ours: "ours\n", base: "base\n", theirs: "theirs\n" },
      { kind: "Clean", content: run(2501) },
    ];
    const { onResolve } = renderPane(big);
    const block = await screen.findByRole("region", { name: "Conflict 1 of 1" });
    // Collapsed: only the lines next to the conflict are in the page.
    expect(document.querySelectorAll("[data-line]").length).toBeLessThan(60);

    fireEvent.click(screen.getAllByRole("button", { name: /unchanged lines/ })[0]);
    const field = resultFieldWith("line 1");
    expect(field.value.split("\n")).toHaveLength(2501);

    const started = performance.now();
    let text = run(1);
    for (let i = 0; i < 20; i++) {
      text = `${text}typed ${i}\n`;
      fireEvent.change(field, { target: { value: text } });
    }
    fireEvent.click(within(cellOf(block, "theirs")).getByRole("button", { name: "Take theirs" }));
    // Generous for a slow CI runner; each keystroke re-renders only what changed.
    expect(performance.now() - started).toBeLessThan(8000);
    expect(linesOf("typed 19")).toEqual([["result", 2520]]);
    expect(numbersIn(cellOf(block, "result"))).toEqual([2521]);

    undoKey(field);
    expect(within(block).getByLabelText("Result")).toHaveValue(
      "<<<<<<< ours\nours\n||||||| base\nbase\n=======\ntheirs\n>>>>>>> theirs\n",
    );
    redoKey(field);
    fireEvent.click(screen.getByRole("button", { name: "Save resolution" }));
    expect(onResolve).toHaveBeenCalledWith("shared.txt", `${text}theirs\n${run(2501)}`);
  }, 30000);
});
