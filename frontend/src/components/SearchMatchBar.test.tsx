import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { SearchMatches } from "../state/useSearchMatches";
import { SearchMatchBar } from "./SearchMatchBar";

function matches(overrides: Partial<SearchMatches> = {}): SearchMatches {
  return {
    ids: new Set(["a"]),
    current: 0,
    text: "1 of 3",
    canGoPrevious: false,
    canGoNext: true,
    step: vi.fn(async () => {}),
    ...overrides,
  };
}

// #687: the match count, previous / next and the List toggle.
describe("SearchMatchBar", () => {
  it("shows the count and steps through the matches", () => {
    const found = matches();
    render(<SearchMatchBar matches={found} view="graph" onToggleList={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent("1 of 3");
    expect(screen.getByRole("button", { name: "Previous match" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Next match" }));
    expect(found.step).toHaveBeenCalledWith(1);
  });

  it("toggles the list view", () => {
    const onToggleList = vi.fn();
    const { rerender } = render(<SearchMatchBar matches={matches()} view="graph" onToggleList={onToggleList} />);
    const toggle = screen.getByRole("button", { name: "List" });
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(toggle);
    expect(onToggleList).toHaveBeenCalled();
    rerender(<SearchMatchBar matches={matches()} view="list" onToggleList={onToggleList} />);
    expect(screen.getByRole("button", { name: "List" })).toHaveAttribute("aria-pressed", "true");
  });
});
