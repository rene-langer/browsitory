import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { GraphFilterStrip } from "./GraphFilterStrip";

// #985: the strip above the graph that says the graph is filtered, and undoes it.
describe("GraphFilterStrip", () => {
  it("shows nothing while every local branch is shown", () => {
    const { container } = render(
      <GraphFilterStrip selection={{ local: null, remote: ["origin/main"] }} onShowAll={vi.fn()} />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("names an isolated local branch, and Show all branches undoes the filter", () => {
    const onShowAll = vi.fn();
    render(<GraphFilterStrip selection={{ local: ["feature/offline-tiles"], remote: null }} onShowAll={onShowAll} />);

    expect(screen.getByRole("status")).toHaveTextContent("Showing feature/offline-tiles only");
    fireEvent.click(screen.getByRole("button", { name: "Show all branches" }));
    expect(onShowAll).toHaveBeenCalledOnce();
  });

  it("names an isolated remote branch", () => {
    render(<GraphFilterStrip selection={{ local: [], remote: ["origin/topic"] }} onShowAll={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent("Showing origin/topic only");
  });

  it("counts several selected branches, local and remote", () => {
    render(<GraphFilterStrip selection={{ local: ["main", "topic"], remote: ["origin/main"] }} onShowAll={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent("Showing 3 selected branches");
  });

  it("says when no branch is shown", () => {
    render(<GraphFilterStrip selection={{ local: [], remote: [] }} onShowAll={vi.fn()} />);
    expect(screen.getByRole("status")).toHaveTextContent("No branches shown");
    expect(screen.getByRole("button", { name: "Show all branches" })).toBeInTheDocument();
  });
});
