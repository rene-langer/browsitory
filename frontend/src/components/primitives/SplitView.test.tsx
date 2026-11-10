import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SplitView } from "./SplitView";

describe("SplitView", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("hides the left pane when forceCollapsed, keeping it mounted", () => {
    render(<SplitView left={<div>left</div>} right={<div>right</div>} forceCollapsed />);
    expect(screen.getByText("left").parentElement).toHaveAttribute("hidden");
    expect(screen.getByText("right")).toBeInTheDocument();
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

  describe("container resizing (#605)", () => {
    // jsdom has no layout, so a fake ResizeObserver hands SplitView its container width.
    const observed = new Map<Element, ResizeObserverCallback>();
    class FakeResizeObserver {
      private readonly callback: ResizeObserverCallback;
      constructor(callback: ResizeObserverCallback) {
        this.callback = callback;
      }
      observe(target: Element) {
        observed.set(target, this.callback);
      }
      unobserve(target: Element) {
        observed.delete(target);
      }
      disconnect() {
        for (const [target, callback] of observed) {
          if (callback === this.callback) observed.delete(target);
        }
      }
    }
    function resize(target: Element, width: number) {
      const callback = observed.get(target);
      if (callback === undefined) throw new Error("element is not observed");
      act(() => {
        callback(
          [{ target, contentRect: { width } } as unknown as ResizeObserverEntry],
          {} as ResizeObserver,
        );
      });
    }
    function splitViewOf(paneText: string): Element {
      const root = screen.getByText(paneText).parentElement?.parentElement;
      if (root == null) throw new Error("no split view");
      return root;
    }

    beforeEach(() => {
      observed.clear();
      vi.stubGlobal("ResizeObserver", FakeResizeObserver);
    });
    afterEach(() => {
      vi.unstubAllGlobals();
    });

    it("keeps the left pane width unchanged when the container shrinks", () => {
      render(<SplitView left={<div>left</div>} right={<div>right</div>} defaultWidth={420} />);
      const left = screen.getByText("left").parentElement;
      resize(splitViewOf("left"), 1600);
      expect(left).toHaveStyle({ width: "420px" });
      resize(splitViewOf("left"), 900);
      expect(left).toHaveStyle({ width: "420px" });
      // Narrower than the pane plus the right pane's minimum: a fixed pane still keeps its width.
      resize(splitViewOf("left"), 600);
      expect(left).toHaveStyle({ width: "420px" });
      expect(left).not.toHaveAttribute("hidden");
    });

    it("collapses a collapsible pane when the container is too narrow and restores it after", () => {
      render(
        <SplitView
          left={<div>left</div>}
          right={<div>right</div>}
          storageKey="narrow-split"
          defaultWidth={260}
          collapsible
        />,
      );
      const left = screen.getByText("left").parentElement;
      resize(splitViewOf("left"), 581); // 260 + 1px divider + 320 right minimum
      expect(left).toHaveStyle({ width: "260px" });
      resize(splitViewOf("left"), 580);
      expect(left).toHaveAttribute("hidden");
      expect(localStorage.getItem("narrow-split")).toBeNull();
      resize(splitViewOf("left"), 1200);
      expect(left).not.toHaveAttribute("hidden");
      expect(left).toHaveStyle({ width: "260px" });
    });

    it("collapses the outer pane when a nested split view's fixed pane would not fit", () => {
      render(
        <SplitView
          left={<div>sidebar</div>}
          right={
            <SplitView left={<div>history</div>} right={<div>diff</div>} defaultWidth={420} />
          }
          defaultWidth={260}
          collapsible
        />,
      );
      const sidebar = screen.getByText("sidebar").parentElement;
      const history = screen.getByText("history").parentElement;
      // 260 + 1 + (420 + 1 + 320) = 1002
      resize(splitViewOf("sidebar"), 1002);
      expect(sidebar).not.toHaveAttribute("hidden");
      resize(splitViewOf("sidebar"), 1001);
      expect(sidebar).toHaveAttribute("hidden");
      expect(history).toHaveStyle({ width: "420px" });
      resize(splitViewOf("sidebar"), 1400);
      expect(sidebar).not.toHaveAttribute("hidden");
      expect(sidebar).toHaveStyle({ width: "260px" });
    });
  });

  it("re-expands a collapsed pane to the width it had before collapsing", () => {
    const { unmount } = render(
      <SplitView
        left={<div>left</div>}
        right={<div>right</div>}
        storageKey="test-restore"
        defaultWidth={260}
        collapsible
      />,
    );
    const divider = screen.getByRole("separator");
    fireEvent.pointerDown(divider, { clientX: 260 });
    fireEvent.pointerMove(window, { clientX: 340 });
    fireEvent.pointerUp(window);
    fireEvent.doubleClick(divider);
    expect(screen.getByText("left").parentElement).toHaveStyle({ width: "0px" });
    fireEvent.doubleClick(divider);
    expect(screen.getByText("left").parentElement).toHaveStyle({ width: "340px" });

    // Also across a remount while collapsed.
    fireEvent.doubleClick(divider);
    unmount();
    render(
      <SplitView
        left={<div>left</div>}
        right={<div>right</div>}
        storageKey="test-restore"
        defaultWidth={260}
        collapsible
      />,
    );
    fireEvent.keyDown(screen.getByRole("separator"), { key: "ArrowRight" });
    expect(screen.getByText("left").parentElement).toHaveStyle({ width: "340px" });
  });

  it("renders the right pane content inside the scrollable inner wrapper", () => {
    render(<SplitView left={<div>left</div>} right={<div>right pane</div>} />);
    const rightPane = screen.getByText("right pane");
    // right content sits in .rightInner, which sits in .right
    expect(rightPane.parentElement?.parentElement).toBe(
      screen.getByRole("separator").nextElementSibling,
    );
  });

  describe("the drawer of a pane hidden for lack of room (#635)", () => {
    const leftPane = () => screen.getByText("left").parentElement as HTMLElement;

    it("opens from the divider over the right pane, and reports the width it shows", () => {
      const onLeftStateChange = vi.fn();
      render(
        <SplitView left={<button>left</button>} right={<div>right</div>} collapsible forceCollapsed defaultWidth={260} onLeftStateChange={onLeftStateChange} />,
      );
      const divider = screen.getByRole("separator");
      expect(divider).toHaveAttribute("aria-valuenow", "0");
      expect(onLeftStateChange).toHaveBeenLastCalledWith("collapsed");
      fireEvent.doubleClick(divider);
      expect(leftPane()).not.toHaveAttribute("hidden");
      expect(leftPane()).toHaveAttribute("data-overlay", "true");
      expect(leftPane()).toHaveStyle({ width: "260px" });
      expect(divider).toHaveAttribute("aria-valuenow", "260");
      expect(onLeftStateChange).toHaveBeenLastCalledWith("overlay");
      expect(leftPane()).toHaveFocus();
      // The stored width is untouched: the drawer is not a docked expand.
      expect(localStorage.length).toBe(0);
    });

    it("closes with Escape, returning focus, and with a click beside it", () => {
      render(<SplitView left={<button>left</button>} right={<div>right</div>} collapsible forceCollapsed />);
      const divider = screen.getByRole("separator");
      divider.focus();
      fireEvent.keyDown(divider, { key: "ArrowRight" });
      expect(leftPane()).toHaveAttribute("data-overlay", "true");
      fireEvent.keyDown(screen.getByRole("button", { name: "left" }), { key: "Escape" });
      expect(leftPane()).toHaveAttribute("hidden");
      expect(divider).toHaveFocus();

      fireEvent.doubleClick(divider);
      const backdrop = document.querySelector("[data-split-backdrop]") as HTMLElement;
      fireEvent.click(backdrop);
      expect(leftPane()).toHaveAttribute("hidden");
    });

    it("ignores a drag while hidden", () => {
      render(<SplitView left={<div>left</div>} right={<div>right</div>} collapsible forceCollapsed />);
      const divider = screen.getByRole("separator");
      fireEvent.pointerDown(divider, { clientX: 0 });
      fireEvent.pointerMove(window, { clientX: 200 });
      fireEvent.pointerUp(window);
      expect(leftPane()).toHaveAttribute("hidden");
      expect(localStorage.length).toBe(0);
    });

    it("docks again once there is room, without reopening the drawer later", () => {
      const view = render(<SplitView left={<div>left</div>} right={<div>right</div>} collapsible forceCollapsed />);
      fireEvent.doubleClick(screen.getByRole("separator"));
      view.rerender(<SplitView left={<div>left</div>} right={<div>right</div>} collapsible forceCollapsed={false} />);
      expect(leftPane()).not.toHaveAttribute("data-overlay");
      expect(leftPane()).not.toHaveAttribute("hidden");
      view.rerender(<SplitView left={<div>left</div>} right={<div>right</div>} collapsible forceCollapsed />);
      expect(leftPane()).toHaveAttribute("hidden");
    });
  });

  describe("requests (#635)", () => {
    const leftPane = () => screen.getByText("left").parentElement as HTMLElement;
    const view = (props: Partial<Parameters<typeof SplitView>[0]>) => (
      <SplitView left={<div>left</div>} right={<div>right</div>} collapsible storageKey="w" {...props} />
    );

    it("toggles a docked pane collapsed and back, and ignores the request it mounts with", () => {
      const { rerender } = render(view({ request: { kind: "toggle", nonce: 1 } }));
      expect(leftPane()).not.toHaveAttribute("hidden");
      rerender(view({ request: { kind: "toggle", nonce: 2 } }));
      expect(leftPane()).toHaveAttribute("hidden");
      expect(localStorage.getItem("w")).toBe("0");
      rerender(view({ request: { kind: "toggle", nonce: 3 } }));
      expect(leftPane()).not.toHaveAttribute("hidden");
      // "show" leaves a shown pane alone.
      rerender(view({ request: { kind: "show", nonce: 4 } }));
      expect(leftPane()).not.toHaveAttribute("hidden");
    });

    it("opens and closes the drawer while hidden for lack of room", () => {
      const { rerender } = render(view({ forceCollapsed: true }));
      rerender(view({ forceCollapsed: true, request: { kind: "toggle", nonce: 1 } }));
      expect(leftPane()).toHaveAttribute("data-overlay", "true");
      rerender(view({ forceCollapsed: true, request: { kind: "show", nonce: 2 } }));
      expect(leftPane()).toHaveAttribute("data-overlay", "true");
      rerender(view({ forceCollapsed: true, request: { kind: "toggle", nonce: 3 } }));
      expect(leftPane()).toHaveAttribute("hidden");
      expect(localStorage.getItem("w")).toBeNull();
    });
  });
});
