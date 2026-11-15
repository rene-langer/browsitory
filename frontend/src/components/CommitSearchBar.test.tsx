import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SEARCH_SYNTAX_FOOTER } from "../lib/searchSyntax";
import { chooseViewOption, viewOptionNames, viewOptionState } from "../test/viewMenu";
import { CommitSearchBar } from "./CommitSearchBar";

function renderBar(overrides: Partial<Parameters<typeof CommitSearchBar>[0]> = {}) {
  const props = {
    query: "",
    onQueryChange: vi.fn(),
    onClose: vi.fn(),
    onFocusResults: vi.fn(),
    focusRequested: false,
    onFocusHandled: vi.fn(),
    ...overrides,
  };
  const view = render(<CommitSearchBar {...props} />);
  return { ...props, rerender: (next: Partial<typeof props>) => view.rerender(<CommitSearchBar {...props} {...next} />) };
}

const field = () => screen.getByRole("combobox", { name: "Search commits" });

it("reports typing and closes on Escape", () => {
  const props = renderBar({ query: "ada" });
  const input = field();
  fireEvent.change(input, { target: { value: "author:ada" } });
  expect(props.onQueryChange).toHaveBeenCalledWith("author:ada");
  fireEvent.keyDown(input, { key: "Escape" });
  expect(props.onClose).toHaveBeenCalled();
});

it("moves focus to the results on ArrowDown", () => {
  const props = renderBar({ query: "x" });
  fireEvent.keyDown(field(), { key: "ArrowDown" });
  expect(props.onFocusResults).toHaveBeenCalled();
});

it("shows × only with text", () => {
  renderBar({ query: "" });
  expect(screen.queryByRole("button", { name: "Close search" })).toBeNull();
});

it("closes the search with ×", () => {
  const props = renderBar({ query: "x" });
  fireEvent.click(screen.getByRole("button", { name: "Close search" }));
  expect(props.onClose).toHaveBeenCalled();
});

it("toggles the syntax help", () => {
  renderBar();
  const help = screen.getByRole("button", { name: "Search syntax" });
  expect(help).toHaveAttribute("aria-expanded", "false");
  fireEvent.click(help);
  expect(help).toHaveAttribute("aria-expanded", "true");
  expect(screen.getByRole("region", { name: "Search syntax" })).toHaveTextContent("author:NAME");
  expect(screen.getByText(SEARCH_SYNTAX_FOOTER)).toBeInTheDocument();
});

it("Escape closes an open help panel before closing the search", () => {
  const props = renderBar({ query: "x" });
  fireEvent.click(screen.getByRole("button", { name: "Search syntax" }));
  fireEvent.keyDown(field(), { key: "Escape" });
  expect(screen.queryByRole("region", { name: "Search syntax" })).toBeNull();
  expect(props.onClose).not.toHaveBeenCalled();
});

it("Escape on any control of the bar closes the search, not only in the field (#1001)", () => {
  const props = renderBar({ query: "path:src/" });
  const help = screen.getByRole("button", { name: "Search syntax" });
  help.focus();
  fireEvent.keyDown(help, { key: "Escape" });
  expect(props.onClose).toHaveBeenCalledOnce();
});

it("Escape on the help button closes the open help before the search (#1001)", () => {
  const props = renderBar({ query: "x" });
  const help = screen.getByRole("button", { name: "Search syntax" });
  fireEvent.click(help);
  fireEvent.keyDown(help, { key: "Escape" });
  expect(screen.queryByRole("region", { name: "Search syntax" })).toBeNull();
  expect(props.onClose).not.toHaveBeenCalled();
});

it("Escape returns focus to where it was before the search (#1001)", () => {
  const opener = document.createElement("button");
  document.body.appendChild(opener);
  opener.focus();
  const props = renderBar({ query: "x" });
  field().focus();
  fireEvent.keyDown(field(), { key: "Escape" });
  expect(props.onClose).toHaveBeenCalledOnce();
  expect(opener).toHaveFocus();
  opener.remove();
});

it("focuses the field when focus is requested and acknowledges it", () => {
  const props = renderBar({ focusRequested: true });
  expect(field()).toHaveFocus();
  expect(props.onFocusHandled).toHaveBeenCalled();
});

it("shows an invalid query's message under the field and marks the field invalid", () => {
  renderBar({ query: "(a", error: "Invalid search: Missing closing parenthesis." });
  const input = field();
  expect(input).toHaveAttribute("aria-invalid", "true");
  expect(input).toHaveAccessibleDescription("Invalid search: Missing closing parenthesis.");
  expect(screen.getByText("Invalid search: Missing closing parenthesis.")).toBeVisible();
});

it("shows no message for a valid query", () => {
  renderBar({ query: "a" });
  expect(field()).toHaveAttribute("aria-invalid", "false");
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.getByRole("status")).toHaveTextContent("");
});

describe("invalid query announcement (#669)", () => {
  it("is polite, not an alert, and waits until the query stops changing", () => {
    vi.useFakeTimers();
    try {
      const props = renderBar({ query: "(", error: "Invalid search: one." });
      const status = screen.getByRole("status");
      expect(screen.queryByRole("alert")).toBeNull();
      expect(status).toHaveTextContent("");
      act(() => vi.advanceTimersByTime(400));
      // Another keystroke, another message: the wait starts over.
      props.rerender({ query: "(a", error: "Invalid search: two." });
      act(() => vi.advanceTimersByTime(400));
      expect(status).toHaveTextContent("");
      act(() => vi.advanceTimersByTime(1000));
      expect(status).toHaveTextContent("Invalid search: two.");
      // Fixed: nothing left to read out.
      props.rerender({ query: "(a)", error: null });
      expect(status).toHaveTextContent("");
      act(() => vi.advanceTimersByTime(10));
      // The same mistake again waits again.
      props.rerender({ query: "(a", error: "Invalid search: two." });
      expect(status).toHaveTextContent("");
      act(() => vi.advanceTimersByTime(1000));
      expect(status).toHaveTextContent("Invalid search: two.");
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("recent searches (#540)", () => {
  const recentSearches = ["author:ada", "fix", "path:src/"];
  const list = () => screen.queryByRole("listbox", { name: "Recent searches" });

  it("opens when the empty field is focused and closes on blur", () => {
    renderBar({ recentSearches });
    expect(list()).toBeNull();
    expect(field()).toHaveAttribute("aria-expanded", "false");

    fireEvent.focus(field());
    const options = within(list()!).getAllByRole("option");
    expect(options.map((o) => o.textContent)).toEqual(recentSearches);
    expect(field()).toHaveAttribute("aria-expanded", "true");
    expect(field()).toHaveAttribute("aria-controls", list()!.id);
    expect(field()).not.toHaveAttribute("aria-activedescendant");

    fireEvent.blur(field());
    expect(list()).toBeNull();
  });

  it("is not offered with text in the field, or without recent searches", () => {
    const bar = renderBar({ query: "x", recentSearches });
    fireEvent.focus(field());
    expect(list()).toBeNull();
    bar.rerender({ query: "", recentSearches: [] });
    expect(list()).toBeNull();
  });

  it("moves with ↓/↑, wrapping, and Enter runs the highlighted search", () => {
    const props = renderBar({ recentSearches });
    fireEvent.focus(field());
    fireEvent.keyDown(field(), { key: "ArrowDown" });
    fireEvent.keyDown(field(), { key: "ArrowDown" });
    let options = screen.getAllByRole("option");
    expect(options[1]).toHaveAttribute("aria-selected", "true");
    expect(field()).toHaveAttribute("aria-activedescendant", options[1].id);

    fireEvent.keyDown(field(), { key: "ArrowUp" });
    fireEvent.keyDown(field(), { key: "ArrowUp" });
    options = screen.getAllByRole("option");
    expect(options[2]).toHaveAttribute("aria-selected", "true");
    expect(props.onFocusResults).not.toHaveBeenCalled();

    fireEvent.keyDown(field(), { key: "Enter" });
    expect(props.onQueryChange).toHaveBeenCalledWith("path:src/");
  });

  it("runs a clicked search", () => {
    const props = renderBar({ recentSearches });
    fireEvent.focus(field());
    fireEvent.click(screen.getByRole("option", { name: "fix" }));
    expect(props.onQueryChange).toHaveBeenCalledWith("fix");
  });

  it("Escape closes the list without closing the search; ↓ opens it again", () => {
    const props = renderBar({ recentSearches });
    fireEvent.focus(field());
    fireEvent.keyDown(field(), { key: "Escape" });
    expect(list()).toBeNull();
    expect(props.onClose).not.toHaveBeenCalled();

    fireEvent.keyDown(field(), { key: "ArrowDown" });
    expect(screen.getAllByRole("option")[0]).toHaveAttribute("aria-selected", "true");
    expect(props.onFocusResults).not.toHaveBeenCalled();
  });

  it("removes a search with its × or with Delete on the highlighted one", () => {
    const onRemoveRecentSearch = vi.fn();
    const props = renderBar({ recentSearches, onRemoveRecentSearch });
    fireEvent.focus(field());
    fireEvent.click(screen.getByRole("button", { name: "Remove “fix” from recent searches" }));
    expect(onRemoveRecentSearch).toHaveBeenCalledWith("fix");
    expect(props.onQueryChange).not.toHaveBeenCalled();

    fireEvent.keyDown(field(), { key: "ArrowDown" });
    fireEvent.keyDown(field(), { key: "Delete" });
    expect(onRemoveRecentSearch).toHaveBeenLastCalledWith("author:ada");
  });

  it("offers no remove button without a remove handler", () => {
    renderBar({ recentSearches });
    fireEvent.focus(field());
    expect(screen.queryByRole("button", { name: /from recent searches/ })).toBeNull();
  });
});

it("toggles the commit list's lines-changed badges (#620)", () => {
  const onToggleLineStats = vi.fn();
  const props = renderBar({ showLineStats: true, onToggleLineStats });
  expect(viewOptionState("Show lines changed", "Graph view")).toBe("true");
  chooseViewOption("Show lines changed", "Graph view");
  expect(onToggleLineStats).toHaveBeenCalledTimes(1);
  props.rerender({ showLineStats: false });
  expect(viewOptionState("Show lines changed", "Graph view")).toBe("false");
});

it("toggles Collapse merges (#617)", () => {
  const onToggleCollapseMerges = vi.fn();
  const props = renderBar({ collapseMerges: false, onToggleCollapseMerges });
  expect(viewOptionState("Collapse merges", "Graph view")).toBe("false");
  chooseViewOption("Collapse merges", "Graph view");
  expect(onToggleCollapseMerges).toHaveBeenCalledTimes(1);
  props.rerender({ collapseMerges: true });
  expect(viewOptionState("Collapse merges", "Graph view")).toBe("true");
});

// #687: with a search open, Enter / F3 go to the next match, with Shift to the previous one.
describe("match keys", () => {
  it("steps through the matches from the field", () => {
    const onStepMatch = vi.fn();
    renderBar({ query: "fix", onStepMatch });
    const field = screen.getByRole("combobox", { name: "Search commits" });
    fireEvent.keyDown(field, { key: "Enter" });
    fireEvent.keyDown(field, { key: "Enter", shiftKey: true });
    fireEvent.keyDown(field, { key: "F3" });
    fireEvent.keyDown(field, { key: "F3", shiftKey: true });
    expect(onStepMatch.mock.calls).toEqual([[1], [-1], [1], [-1]]);
  });

  it("does nothing on Enter in an empty field", () => {
    const onStepMatch = vi.fn();
    renderBar({ query: "", onStepMatch });
    fireEvent.keyDown(screen.getByRole("combobox", { name: "Search commits" }), { key: "Enter" });
    expect(onStepMatch).not.toHaveBeenCalled();
  });
});

it("names the graph's View menu starting with its visible text (#1007)", () => {
  renderBar({ showLineStats: true, onToggleLineStats: vi.fn(), collapseMerges: false, onToggleCollapseMerges: vi.fn() });
  expect(screen.getByRole("button", { name: "View graph options" })).toHaveTextContent("View");
});

// #1007: the graph's rarely changed toggles are in a View menu, in words.
it("puts Show lines changed and Collapse merges in the Graph view menu", () => {
  const onToggleLineStats = vi.fn();
  const onToggleCollapseMerges = vi.fn();
  renderBar({ showLineStats: true, onToggleLineStats, collapseMerges: false, onToggleCollapseMerges });
  expect(screen.queryByRole("button", { name: "Show lines changed" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Collapse merges" })).toBeNull();
  expect(screen.getByRole("button", { name: "Graph view" })).toHaveTextContent("View");
  expect(viewOptionNames("Graph view")).toEqual(["Show lines changed", "Collapse merges"]);
  expect(viewOptionState("Show lines changed", "Graph view")).toBe("true");
  chooseViewOption("Collapse merges", "Graph view");
  expect(onToggleCollapseMerges).toHaveBeenCalledTimes(1);
});
