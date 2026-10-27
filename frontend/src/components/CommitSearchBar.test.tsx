import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { SEARCH_SYNTAX_FOOTER } from "../lib/searchSyntax";
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
  render(<CommitSearchBar {...props} />);
  return props;
}

it("reports typing and closes on Escape", () => {
  const props = renderBar({ query: "ada" });
  const input = screen.getByRole("searchbox", { name: "Search commits" });
  fireEvent.change(input, { target: { value: "author:ada" } });
  expect(props.onQueryChange).toHaveBeenCalledWith("author:ada");
  fireEvent.keyDown(input, { key: "Escape" });
  expect(props.onClose).toHaveBeenCalled();
});

it("moves focus to the results on ArrowDown", () => {
  const props = renderBar({ query: "x" });
  fireEvent.keyDown(screen.getByRole("searchbox", { name: "Search commits" }), { key: "ArrowDown" });
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
  const input = screen.getByRole("searchbox", { name: "Search commits" });
  fireEvent.keyDown(input, { key: "Escape" });
  expect(screen.queryByRole("region", { name: "Search syntax" })).toBeNull();
  expect(props.onClose).not.toHaveBeenCalled();
});

it("focuses the field when focus is requested and acknowledges it", () => {
  const props = renderBar({ focusRequested: true });
  expect(screen.getByRole("searchbox", { name: "Search commits" })).toHaveFocus();
  expect(props.onFocusHandled).toHaveBeenCalled();
});
