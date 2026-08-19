import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { SplitView } from "./SplitView";

describe("SplitView", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("renders both panes", () => {
    render(<SplitView left={<div>left pane</div>} right={<div>right pane</div>} />);
    expect(screen.getByText("left pane")).toBeInTheDocument();
    expect(screen.getByText("right pane")).toBeInTheDocument();
  });

  it("defaults the left pane to 300px when no width props are given", () => {
    render(<SplitView left={<div>left</div>} right={<div>right</div>} />);
    const left = screen.getByText("left").parentElement;
    expect(left).toHaveStyle({ width: "300px" });
  });

  it("applies a custom defaultWidth", () => {
    render(<SplitView left={<div>left</div>} right={<div>right</div>} defaultWidth={220} />);
    const left = screen.getByText("left").parentElement;
    expect(left).toHaveStyle({ width: "220px" });
  });

  it("resizes the left pane by dragging the divider, clamped to min/max", () => {
    render(
      <SplitView
        left={<div>left</div>}
        right={<div>right</div>}
        defaultWidth={300}
        minWidth={160}
        maxWidth={480}
      />,
    );
    const divider = screen.getByRole("separator");
    fireEvent.pointerDown(divider, { clientX: 300 });
    fireEvent.pointerMove(window, { clientX: 400 });
    fireEvent.pointerUp(window);
    const left = screen.getByText("left").parentElement;
    expect(left).toHaveStyle({ width: "400px" });

    fireEvent.pointerDown(divider, { clientX: 400 });
    fireEvent.pointerMove(window, { clientX: 1000 });
    fireEvent.pointerUp(window);
    expect(left).toHaveStyle({ width: "480px" });

    fireEvent.pointerDown(divider, { clientX: 480 });
    fireEvent.pointerMove(window, { clientX: 0 });
    fireEvent.pointerUp(window);
    expect(left).toHaveStyle({ width: "160px" });
  });

  it("resizes via arrow keys on the focused divider", () => {
    render(<SplitView left={<div>left</div>} right={<div>right</div>} defaultWidth={300} />);
    const divider = screen.getByRole("separator");
    divider.focus();
    fireEvent.keyDown(divider, { key: "ArrowRight" });
    const left = screen.getByText("left").parentElement;
    expect(left).toHaveStyle({ width: "316px" });
    fireEvent.keyDown(divider, { key: "ArrowLeft" });
    fireEvent.keyDown(divider, { key: "ArrowLeft" });
    expect(left).toHaveStyle({ width: "284px" });
  });

  it("persists width to localStorage when storageKey is set, and restores it on next mount", () => {
    const { unmount } = render(
      <SplitView left={<div>left</div>} right={<div>right</div>} storageKey="test-split" defaultWidth={300} />,
    );
    const divider = screen.getByRole("separator");
    fireEvent.pointerDown(divider, { clientX: 300 });
    fireEvent.pointerMove(window, { clientX: 350 });
    fireEvent.pointerUp(window);
    expect(localStorage.getItem("test-split")).toBe("350");
    unmount();

    render(<SplitView left={<div>left</div>} right={<div>right</div>} storageKey="test-split" defaultWidth={300} />);
    const left = screen.getByText("left").parentElement;
    expect(left).toHaveStyle({ width: "350px" });
  });

  it("does not persist width when storageKey is omitted", () => {
    render(<SplitView left={<div>left</div>} right={<div>right</div>} defaultWidth={300} />);
    const divider = screen.getByRole("separator");
    fireEvent.pointerDown(divider, { clientX: 300 });
    fireEvent.pointerMove(window, { clientX: 350 });
    fireEvent.pointerUp(window);
    expect(localStorage.length).toBe(0);
  });

  it("snaps to 0 and back when collapsible and double-clicked", () => {
    render(
      <SplitView left={<div>left</div>} right={<div>right</div>} defaultWidth={300} collapsible />,
    );
    const divider = screen.getByRole("separator");
    const left = screen.getByText("left").parentElement;
    fireEvent.doubleClick(divider);
    expect(left).toHaveStyle({ width: "0px" });
    fireEvent.doubleClick(divider);
    expect(left).toHaveStyle({ width: "300px" });
  });

  it("does not collapse on double-click when collapsible is false", () => {
    render(<SplitView left={<div>left</div>} right={<div>right</div>} defaultWidth={300} />);
    const divider = screen.getByRole("separator");
    const left = screen.getByText("left").parentElement;
    fireEvent.doubleClick(divider);
    expect(left).toHaveStyle({ width: "300px" });
  });
});
