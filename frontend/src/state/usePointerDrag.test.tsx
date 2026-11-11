import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { dropTargetProps, usePointerDrag, type PointerDragOptions } from "./usePointerDrag";

// jsdom has no layout: each test says which element is under the pointer and where it is.
const originalElementFromPoint = document.elementFromPoint;
afterEach(() => {
  document.elementFromPoint = originalElementFromPoint;
});

function pointAt(element: Element | null) {
  document.elementFromPoint = vi.fn(() => element);
}

function placeAt(element: Element, top: number, height: number) {
  element.getBoundingClientRect = () =>
    ({ top, bottom: top + height, height, left: 0, right: 100, width: 100, x: 0, y: top }) as DOMRect;
}

function Harness(props: Partial<PointerDragOptions<string>> & { onClickSource?: () => void }) {
  const { onClickSource, ...options } = props;
  const { drag, startDrag } = usePointerDrag<string>({ scope: "test", onDrop: vi.fn(), ...options });
  return (
    <div>
      <button type="button" onPointerDown={(event) => startDrag(event, "payload")} onClick={onClickSource}>
        source
      </button>
      <div data-testid="target-a" {...dropTargetProps("test", "a")}>
        <span>inside a</span>
      </div>
      <div data-testid="other-scope" {...dropTargetProps("other", "x")} />
      <p data-testid="state">
        {drag === null ? "idle" : `dragging ${drag.payload} over ${drag.over?.key ?? "nothing"} ${drag.over?.position ?? ""}`}
      </p>
    </div>
  );
}

describe("usePointerDrag", () => {
  it("doesn't start a drag for a press that moves less than the threshold", () => {
    const onDrop = vi.fn();
    const onStart = vi.fn();
    render(<Harness onDrop={onDrop} onStart={onStart} />);
    fireEvent.pointerDown(screen.getByText("source"), { clientX: 10, clientY: 10, button: 0 });
    fireEvent.pointerMove(window, { clientX: 12, clientY: 12 });
    fireEvent.pointerUp(window, { clientX: 12, clientY: 12 });
    expect(onStart).not.toHaveBeenCalled();
    expect(onDrop).not.toHaveBeenCalled();
    expect(screen.getByTestId("state")).toHaveTextContent("idle");
  });

  it("starts after the threshold and reports the target under the pointer and the half it is in", () => {
    const onStart = vi.fn();
    const onOver = vi.fn();
    render(<Harness onStart={onStart} onOver={onOver} />);
    const target = screen.getByTestId("target-a");
    placeAt(target, 100, 20);
    // The element under the pointer is the target's child: the target is found from it.
    pointAt(screen.getByText("inside a"));
    fireEvent.pointerDown(screen.getByText("source"), { clientX: 10, clientY: 10, button: 0 });
    fireEvent.pointerMove(window, { clientX: 10, clientY: 105 });
    expect(onStart).toHaveBeenCalledWith("payload");
    expect(screen.getByTestId("state")).toHaveTextContent("dragging payload over a before");
    fireEvent.pointerMove(window, { clientX: 10, clientY: 115 });
    expect(screen.getByTestId("state")).toHaveTextContent("dragging payload over a after");
    expect(onOver).toHaveBeenCalledWith("payload", expect.objectContaining({ key: "a" }));
  });

  it("drops on the target under the pointer and ends the drag", () => {
    const onDrop = vi.fn();
    render(<Harness onDrop={onDrop} />);
    const target = screen.getByTestId("target-a");
    placeAt(target, 100, 20);
    pointAt(target);
    fireEvent.pointerDown(screen.getByText("source"), { clientX: 10, clientY: 10, button: 0 });
    fireEvent.pointerMove(window, { clientX: 10, clientY: 115 });
    fireEvent.pointerUp(window, { clientX: 10, clientY: 115 });
    expect(onDrop).toHaveBeenCalledWith("payload", expect.objectContaining({ key: "a", position: "after" }));
    expect(screen.getByTestId("state")).toHaveTextContent("idle");
  });

  it("ignores targets of another scope and drops on nothing there", () => {
    const onDrop = vi.fn();
    render(<Harness onDrop={onDrop} />);
    pointAt(screen.getByTestId("other-scope"));
    fireEvent.pointerDown(screen.getByText("source"), { clientX: 10, clientY: 10, button: 0 });
    fireEvent.pointerMove(window, { clientX: 10, clientY: 60 });
    expect(screen.getByTestId("state")).toHaveTextContent("dragging payload over nothing");
    fireEvent.pointerUp(window, { clientX: 10, clientY: 60 });
    expect(onDrop).toHaveBeenCalledWith("payload", null);
  });

  it("cancels with Escape: no drop on release", () => {
    const onDrop = vi.fn();
    const onCancel = vi.fn();
    render(<Harness onDrop={onDrop} onCancel={onCancel} />);
    pointAt(screen.getByTestId("target-a"));
    fireEvent.pointerDown(screen.getByText("source"), { clientX: 10, clientY: 10, button: 0 });
    fireEvent.pointerMove(window, { clientX: 10, clientY: 60 });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onCancel).toHaveBeenCalledWith("payload");
    expect(screen.getByTestId("state")).toHaveTextContent("idle");
    fireEvent.pointerUp(window, { clientX: 10, clientY: 60 });
    expect(onDrop).not.toHaveBeenCalled();
  });

  it("cancels when the pointer is cancelled", () => {
    const onDrop = vi.fn();
    const onCancel = vi.fn();
    render(<Harness onDrop={onDrop} onCancel={onCancel} />);
    fireEvent.pointerDown(screen.getByText("source"), { clientX: 10, clientY: 10, button: 0 });
    fireEvent.pointerMove(window, { clientX: 10, clientY: 60 });
    fireEvent.pointerCancel(window);
    expect(onCancel).toHaveBeenCalled();
    expect(onDrop).not.toHaveBeenCalled();
  });

  it("ignores a press with another button than the primary one", () => {
    const onStart = vi.fn();
    render(<Harness onStart={onStart} />);
    fireEvent.pointerDown(screen.getByText("source"), { clientX: 10, clientY: 10, button: 2 });
    fireEvent.pointerMove(window, { clientX: 10, clientY: 60 });
    expect(onStart).not.toHaveBeenCalled();
  });

  it("swallows the click that ends a drag, but not a plain click", () => {
    const onClickSource = vi.fn();
    render(<Harness onClickSource={onClickSource} />);
    const source = screen.getByText("source");
    fireEvent.pointerDown(source, { clientX: 10, clientY: 10, button: 0 });
    fireEvent.pointerMove(window, { clientX: 10, clientY: 60 });
    fireEvent.pointerUp(window, { clientX: 10, clientY: 60 });
    fireEvent.click(source);
    expect(onClickSource).not.toHaveBeenCalled();

    fireEvent.pointerDown(source, { clientX: 10, clientY: 10, button: 0 });
    fireEvent.pointerUp(window, { clientX: 10, clientY: 10 });
    fireEvent.click(source);
    expect(onClickSource).toHaveBeenCalledTimes(1);
  });

  it("scrolls the scroll container while the pointer is near its top or bottom edge", () => {
    const container = document.createElement("div");
    placeAt(container, 0, 200);
    let scrollTop = 50;
    Object.defineProperty(container, "scrollTop", {
      get: () => scrollTop,
      set: (value: number) => {
        scrollTop = value;
      },
    });
    render(<Harness autoScroll={() => container} />);
    fireEvent.pointerDown(screen.getByText("source"), { clientX: 10, clientY: 100, button: 0 });
    fireEvent.pointerMove(window, { clientX: 10, clientY: 195 });
    expect(scrollTop).toBeGreaterThan(50);
    const afterDown = scrollTop;
    fireEvent.pointerMove(window, { clientX: 10, clientY: 5 });
    expect(scrollTop).toBeLessThan(afterDown);
    const afterUp = scrollTop;
    fireEvent.pointerMove(window, { clientX: 10, clientY: 100 });
    expect(scrollTop).toBe(afterUp);
    act(() => {
      fireEvent.pointerUp(window, { clientX: 10, clientY: 100 });
    });
  });

  it("stops listening when the component unmounts mid-drag", () => {
    const onDrop = vi.fn();
    const { unmount } = render(<Harness onDrop={onDrop} />);
    fireEvent.pointerDown(screen.getByText("source"), { clientX: 10, clientY: 10, button: 0 });
    fireEvent.pointerMove(window, { clientX: 10, clientY: 60 });
    unmount();
    fireEvent.pointerUp(window, { clientX: 10, clientY: 60 });
    expect(onDrop).not.toHaveBeenCalled();
  });
});
