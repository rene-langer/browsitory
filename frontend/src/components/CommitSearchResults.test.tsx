import { fireEvent, render, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import type { GraphCommit } from "../ipc/RepoClient";
import type { SelectedRow } from "../state/useAppState";
import type { CommitSearchResultsState } from "../state/useCommitSearch";
import { searchStatusText } from "../lib/searchStatus";
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
};

it.each([
  [{ loading: true, scanned: 0 }, "Searching…"],
  [{ loading: true, scanned: 20_000 }, "Scanned 20,000 commits…"],
  [{ loading: false, commits: [] }, "No commits match this search."],
  [{ commits: [c("a")] }, "1 match"],
  [{ commits: [c("a"), c("b")] }, "2 matches"],
  [{ commits: [c("a"), c("b")], nextCursor: "40000:fp", scanned: 40_000 }, "2 matches in the first 40,000 commits"],
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
  expect(props.onSelectRow).toHaveBeenCalledWith({ commitId: "b" });
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
