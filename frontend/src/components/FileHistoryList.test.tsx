import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { FileHistoryEntry } from "../ipc/RepoClient";
import type { FileHistoryState } from "../state/useFileHistory";
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
  return { path: "f.txt", entries: [], limit: 300, hasMore: false, loading: false, error: null, ...overrides };
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
    expect(props.onSelectRow).toHaveBeenCalledWith({ commitId: "c1" });
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
});
