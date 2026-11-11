import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { FileHistoryEntry } from "../ipc/RepoClient";
import type { FileHistoryState } from "../state/useFileHistory";
import type { SelectedRow } from "../state/useAppState";
import { layoutInPane, ROW } from "../test/virtualLayout";
import { FileHistoryList } from "./FileHistoryList";

function entry(id: string, path = "f.txt", oldPath: string | null = null): FileHistoryEntry {
  return {
    commit: {
      id,
      shortId: id.slice(0, 7),
      summary: `commit ${id}`,
      authorName: "A",
      authorEmail: "a@example.com",
      timestamp: 1_700_000_000,
      parentIds: [],
      branchRefs: [],
      remoteBranchRefs: [],
    },
    path,
    oldPath,
  };
}

function state(overrides: Partial<FileHistoryState> = {}): FileHistoryState {
  return {
    target: { kind: "file", path: "f.txt" },
    path: "f.txt",
    entries: [],
    limit: 300,
    hasMore: false,
    truncated: false,
    loading: false,
    error: null,
    ...overrides,
  };
}

function baseProps(history: FileHistoryState, overrides: Partial<React.ComponentProps<typeof FileHistoryList>> = {}) {
  return {
    history,
    selectedRow: "uncommitted" as const,
    onSelectRow: vi.fn(),
    onClose: vi.fn(),
    onLoadMore: vi.fn(),
    onRetry: vi.fn(),
    ...overrides,
  };
}

function renderList(history: FileHistoryState, overrides: Partial<React.ComponentProps<typeof FileHistoryList>> = {}) {
  const props = baseProps(history, overrides);
  render(<FileHistoryList {...props} />);
  return props;
}

describe("FileHistoryList", () => {
  it("shows the path in the banner and closes from the × button", () => {
    const props = renderList(state({ entries: [entry("c1")] }));

    expect(screen.getByRole("toolbar", { name: "File history" })).toHaveTextContent("History: f.txt");
    fireEvent.click(screen.getByRole("button", { name: "Close file history" }));
    expect(props.onClose).toHaveBeenCalled();
  });

  it("lists entries, marks the selected one and selects on click", () => {
    const props = renderList(state({ entries: [entry("c2"), entry("c1")] }), { selectedRow: { commitId: "c1" } });

    const options = screen.getAllByRole("option");
    expect(options).toHaveLength(2);
    expect(options[1]).toHaveAttribute("aria-selected", "true");
    fireEvent.click(screen.getByText(/commit c2/));
    expect(props.onSelectRow).toHaveBeenCalledWith({ commitId: "c2" });
  });

  it("labels rename entries with the old path", () => {
    renderList(state({ entries: [entry("c2", "new.txt", "old.txt")] }));

    expect(screen.getByText("renamed from old.txt")).toBeInTheDocument();
  });

  it("moves selection with arrow keys and closes on Escape", () => {
    const props = renderList(state({ entries: [entry("c2"), entry("c1")] }), { selectedRow: { commitId: "c2" } });
    const list = screen.getByRole("listbox", { name: "File history commits" });

    fireEvent.keyDown(list, { key: "ArrowDown" });
    expect(props.onSelectRow).toHaveBeenCalledWith({ commitId: "c1" }, "step");
    fireEvent.keyDown(list, { key: "Escape" });
    expect(props.onClose).toHaveBeenCalled();
  });

  it("offers Load more when more history exists", () => {
    const props = renderList(state({ entries: [entry("c1")], hasMore: true }));

    fireEvent.click(screen.getByRole("button", { name: "Load more" }));
    expect(props.onLoadMore).toHaveBeenCalled();
  });

  it("shows an empty state once loaded with no entries", () => {
    renderList(state());

    expect(screen.getByText(/No committed history for/)).toHaveTextContent("No committed history for f.txt.");
  });

  it("shows no empty state while loading", () => {
    renderList(state({ loading: true }));

    expect(screen.queryByText(/No committed history/)).toBeNull();
  });

  it("shows loading feedback only while the first load is in flight", () => {
    const { unmount } = render(<FileHistoryList {...baseProps(state({ loading: true }))} />);
    expect(screen.getByText("Loading history…")).toBeInTheDocument();
    unmount();
    render(<FileHistoryList {...baseProps(state())} />);
    expect(screen.queryByText("Loading history…")).toBeNull();
  });

  it("renders remote branch badges", () => {
    const e = entry("c1");
    e.commit.remoteBranchRefs = ["origin/main"];
    renderList(state({ entries: [e] }));

    expect(screen.getByText("origin/main")).toBeInTheDocument();
  });

  it("renders tag badges", () => {
    const e = entry("c1");
    e.commit.tags = ["v1.0"];
    renderList(state({ entries: [e] }));

    expect(screen.getByTitle("Tag v1.0")).toHaveTextContent("v1.0");
  });

  it("scrolls the newly selected row into view", () => {
    const spy = vi.fn();
    const original = Element.prototype.scrollIntoView;
    Element.prototype.scrollIntoView = spy;
    try {
      const history = state({ entries: [entry("c2"), entry("c1")] });
      const props = baseProps(history, { selectedRow: { commitId: "c2" } });
      const { rerender } = render(<FileHistoryList {...props} />);
      spy.mockClear();
      fireEvent.keyDown(screen.getByRole("listbox", { name: "File history commits" }), { key: "ArrowDown" });
      rerender(<FileHistoryList {...props} selectedRow={{ commitId: "c1" }} />);

      expect(spy).toHaveBeenCalledWith({ block: "nearest" });
      expect(spy.mock.contexts.at(-1)).toBe(screen.getAllByRole("option")[1]);
    } finally {
      Element.prototype.scrollIntoView = original;
    }
  });

  it("shows an error with Retry", () => {
    const props = renderList(state({ error: { message: "boom" } }));

    expect(screen.getByRole("alert")).toHaveTextContent("boom");
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(props.onRetry).toHaveBeenCalled();
  });

  describe("a line history (#447)", () => {
    const lines = { kind: "lines" as const, path: "f.txt", revision: "abcdef1234567890", startLine: 10, endLine: 12 };

    it("names the lines and the commit they were read at, and each row's lines", () => {
      renderList(
        state({
          target: lines,
          entries: [
            { ...entry("c2"), startLine: 10, endLine: 12 },
            { ...entry("c1"), startLine: 7, endLine: 7 },
          ],
        }),
      );

      const banner = screen.getByRole("toolbar", { name: "Line history" });
      expect(banner).toHaveTextContent("History of lines 10–12 of f.txt at abcdef1");
      expect(screen.getByText("lines 10–12")).toBeInTheDocument();
      expect(screen.getByText("line 7")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Close line history" })).toBeInTheDocument();
    });

    it("says when the backend stopped early", () => {
      renderList(state({ target: { ...lines, revision: "HEAD" }, truncated: true }));

      expect(screen.getByRole("toolbar", { name: "Line history" })).not.toHaveTextContent(" at ");
      expect(screen.getByText(/older changes to these lines may be missing/)).toBeInTheDocument();
    });

    it("names the lines when none changed", () => {
      renderList(state({ target: lines }));

      expect(screen.getByText(/No committed history for lines 10–12 of/)).toBeInTheDocument();
    });
  });
});

// #775: only the rows in view render, as in the commit graph (#685).
describe("FileHistoryList — windowed rows", () => {
  const many = Array.from({ length: 1000 }, (_, n) => entry(`id${n}`));

  function renderInPane(selectedRow: SelectedRow = "uncommitted") {
    const layout = layoutInPane();
    const props = baseProps(state({ entries: many }), { selectedRow });
    const utils = render(<FileHistoryList {...props} />, { container: layout.pane });
    return { ...utils, ...layout, props };
  }

  it("renders the rows in view plus a margin, and pads for the rest", () => {
    const { restore } = renderInPane();
    try {
      const list = screen.getByRole("listbox", { name: "File history commits" });
      const rendered = within(list).getAllByRole("option");
      // Rows 0–9 are in view, 20 more below them.
      expect(rendered).toHaveLength(30);
      expect(rendered[0]).toHaveTextContent("commit id0");
      expect(rendered[1]).toHaveAttribute("aria-posinset", "2");
      expect(rendered[1]).toHaveAttribute("aria-setsize", "1000");
      expect(list.style.paddingTop).toBe("0px");
      expect(list.style.paddingBottom).toBe(`${(1000 - 30) * ROW}px`);
    } finally {
      restore();
    }
  });

  it("renders the rows of the scrolled-to part", () => {
    const { scrollTo, restore } = renderInPane();
    try {
      scrollTo(ROW * 500);
      const list = screen.getByRole("listbox", { name: "File history commits" });
      const rendered = within(list).getAllByRole("option");
      expect(rendered[0]).toHaveAttribute("data-row-index", "480");
      expect(rendered[0]).toHaveTextContent("commit id480");
      expect(rendered.at(-1)).toHaveAttribute("data-row-index", "529");
      expect(list.style.paddingTop).toBe(`${480 * ROW}px`);
    } finally {
      restore();
    }
  });

  it("scrolls a row selected far down the list into view", () => {
    const { pane, rerender, props, restore } = renderInPane();
    try {
      rerender(<FileHistoryList {...props} selectedRow={{ commitId: "id700" }} />);
      // Row 700 ends at the pane's bottom edge.
      expect(pane.scrollTop).toBe(701 * ROW - 260);
      expect(screen.getByText(/commit id700$/).closest("li")).toHaveAttribute("aria-selected", "true");
    } finally {
      restore();
    }
  });
});
