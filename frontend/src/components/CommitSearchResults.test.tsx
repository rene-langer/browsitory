import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import type { CommitSearchResultsState } from "../state/useCommitSearch";
import { searchStatusText } from "../lib/searchStatus";
import { layoutInPane, ROW } from "../test/virtualLayout";
import { CommitSearchResults } from "./CommitSearchResults";

function c(id: string): GraphCommit {
  return {
    id,
    shortId: id.slice(0, 7),
    summary: `commit ${id}`,
    authorName: "A",
    authorEmail: "a@example.com",
    timestamp: 1_700_000_000,
    parentIds: [],
    branchRefs: [],
    remoteBranchRefs: [],
  };
}

function renderResults(results: CommitSearchResultsState, selectedRow: SelectedRow = "uncommitted") {
  const props = {
    results,
    selectedRow,
    onSelectRow: vi.fn(),
    onClose: vi.fn(),
    onLoadMore: vi.fn(),
    onRetry: vi.fn(),
    onRequestSearch: vi.fn(),
  };
  render(<CommitSearchResults {...props} />);
  return props;
}

const base: CommitSearchResultsState = {
  query: "x",
  commits: [],
  nextCursor: null,
  scanned: 0,
  loading: false,
  error: null,
  queryError: null,
};

it.each([
  [{ loading: true, scanned: 0 }, "Searching…"],
  [{ loading: true, scanned: 20_000 }, "Scanned 20,000 commits…"],
  [{ loading: false, commits: [] }, "No commits match this search."],
  [{ commits: [c("a")] }, "1 match"],
  [{ commits: [c("a"), c("b")] }, "2 matches"],
  [{ commits: [c("a"), c("b")], nextCursor: "40000:fp", scanned: 40_000 }, "2 matches in the first 40,000 commits"],
  [{ queryError: "Invalid search: Missing closing parenthesis." }, "Fix the search above to see results."],
])("status for %o is %s", (partial, text) => {
  expect(searchStatusText({ ...base, ...partial })).toBe(text);
});

it("renders rows, selects on click, and loads more", () => {
  const props = renderResults({ ...base, commits: [c("a"), c("b")], nextCursor: "2:fp", scanned: 2 });
  const list = screen.getByRole("listbox", { name: "Search results" });
  expect(within(list).getAllByRole("option")).toHaveLength(2);
  fireEvent.click(within(list).getByText(/commit b/));
  expect(props.onSelectRow).toHaveBeenCalledWith({ commitId: "b" });
  fireEvent.click(screen.getByRole("button", { name: "Load more" }));
  expect(props.onLoadMore).toHaveBeenCalled();
});

it("hides Load more while loading or when the walk is done", () => {
  renderResults({ ...base, commits: [c("a")], nextCursor: "1:fp", loading: true });
  expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
});

it("navigates with arrows, closes on Escape, and asks for the field on Ctrl/Cmd+F", () => {
  const props = renderResults({ ...base, commits: [c("a"), c("b")] }, { commitId: "a" });
  const list = screen.getByRole("listbox", { name: "Search results" });
  fireEvent.keyDown(list, { key: "ArrowDown" });
  expect(props.onSelectRow).toHaveBeenCalledWith({ commitId: "b" }, "step");
  fireEvent.keyDown(list, { key: "f", ctrlKey: true });
  expect(props.onRequestSearch).toHaveBeenCalled();
  fireEvent.keyDown(list, { key: "Escape" });
  expect(props.onClose).toHaveBeenCalled();
});

it("shows the error inline with Retry and Dismiss", () => {
  const props = renderResults({ ...base, error: { message: "author: needs a value." } });
  expect(screen.getByRole("alert")).toHaveTextContent("author: needs a value.");
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(props.onRetry).toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Dismiss error" }));
  expect(props.onClose).toHaveBeenCalled();
  expect(screen.queryByText("No commits match this search.")).toBeNull();
});

it("renders branch, remote and tag badges on result rows", () => {
  renderResults({
    ...base,
    commits: [{ ...c("a"), branchRefs: ["main"], remoteBranchRefs: ["origin/main"], tags: ["v1.0"] }],
  });
  expect(screen.getByText("main")).toBeInTheDocument();
  expect(screen.getByText("origin/main")).toBeInTheDocument();
  expect(screen.getByTitle("Tag v1.0")).toHaveTextContent("v1.0");
});

// #775: only the rows in view render, as in the commit graph (#685).
describe("CommitSearchResults — windowed rows", () => {
  const many = Array.from({ length: 1000 }, (_, n) => c(`id${n}`));

  function renderInPane() {
    const layout = layoutInPane();
    const props = {
      results: { ...base, commits: many },
      selectedRow: "uncommitted" as SelectedRow,
      onSelectRow: vi.fn(),
      onClose: vi.fn(),
      onLoadMore: vi.fn(),
      onRetry: vi.fn(),
      onRequestSearch: vi.fn(),
    };
    const utils = render(<CommitSearchResults {...props} />, { container: layout.pane });
    return { ...utils, ...layout, props };
  }

  it("renders the rows in view plus a margin, and pads for the rest", () => {
    const { restore } = renderInPane();
    try {
      const list = screen.getByRole("listbox", { name: "Search results" });
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
      const list = screen.getByRole("listbox", { name: "Search results" });
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
      rerender(<CommitSearchResults {...props} selectedRow={{ commitId: "id700" }} />);
      // Row 700 ends at the pane's bottom edge.
      expect(pane.scrollTop).toBe(701 * ROW - 260);
      expect(screen.getByText(/commit id700$/).closest("li")).toHaveAttribute("aria-selected", "true");
    } finally {
      restore();
    }
  });
});
