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

  it("restores a persisted collapsed width instead of clamping it back to minWidth", () => {
    const { unmount } = render(
      <SplitView
        left={<div>left</div>}
        right={<div>right</div>}
        storageKey="test-collapse"
        defaultWidth={300}
        minWidth={160}
        collapsible
      />,
    );
    fireEvent.doubleClick(screen.getByRole("separator"));
    expect(localStorage.getItem("test-collapse")).toBe("0");
    unmount();

    render(
      <SplitView
        left={<div>left</div>}
        right={<div>right</div>}
        storageKey="test-collapse"
        defaultWidth={300}
        minWidth={160}
        collapsible
      />,
    );
    expect(screen.getByText("left").parentElement).toHaveStyle({ width: "0px" });
  });

  it("hides the collapsed left pane from the tab order and the a11y tree", () => {
    render(
      <SplitView left={<button type="button">left action</button>} right={<div>right</div>} collapsible />,
    );
    expect(screen.getByRole("button", { name: "left action" })).toBeInTheDocument();
    fireEvent.doubleClick(screen.getByRole("separator"));
    expect(screen.queryByRole("button", { name: "left action" })).not.toBeInTheDocument();
  });

  it("re-expands a collapsed pane with an arrow key", () => {
    render(
      <SplitView
        left={<div>left</div>}
        right={<div>right</div>}
        storageKey="test-expand"
        defaultWidth={300}
        collapsible
      />,
    );
    const divider = screen.getByRole("separator");
    const left = screen.getByText("left").parentElement;
    fireEvent.doubleClick(divider);
    expect(left).toHaveStyle({ width: "0px" });

    fireEvent.keyDown(divider, { key: "ArrowRight" });
    expect(left).toHaveStyle({ width: "300px" });
    expect(localStorage.getItem("test-expand")).toBe("300");

    fireEvent.doubleClick(divider);
    fireEvent.keyDown(divider, { key: "ArrowLeft" });
    expect(left).toHaveStyle({ width: "300px" });
  });

  it("labels the divider and exposes its width through the value ARIA attributes", () => {
    render(
      <SplitView
        left={<div>left</div>}
        right={<div>right</div>}
        defaultWidth={300}
        minWidth={160}
        maxWidth={480}
        label="Resize sidebar"
      />,
    );
    const divider = screen.getByRole("separator", { name: "Resize sidebar" });
    expect(divider).toHaveAttribute("aria-valuenow", "300");
    expect(divider).toHaveAttribute("aria-valuemin", "160");
    expect(divider).toHaveAttribute("aria-valuemax", "480");

    fireEvent.keyDown(divider, { key: "ArrowRight" });
    expect(divider).toHaveAttribute("aria-valuenow", "316");
  });

  it("renders the right pane content inside the scrollable inner wrapper", () => {
    render(<SplitView left={<div>left</div>} right={<div>right pane</div>} />);
    const rightPane = screen.getByText("right pane");
    // right content sits in .rightInner, which sits in .right
    expect(rightPane.parentElement?.parentElement).toBe(
      screen.getByRole("separator").nextElementSibling,
    );
  });
});
