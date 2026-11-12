import { useState } from "react";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RenderErrorBoundary, RenderErrorFallback, type CaughtRenderError } from "./RenderErrorBoundary";
import { renderErrorDetails } from "../lib/renderErrorDetails";

// React reports every caught render error on the console; keep the test output readable.
beforeEach(() => {
  vi.spyOn(console, "error").mockImplementation(() => {});
});
afterEach(() => vi.restoreAllMocks());

const failing = { now: true };
function Thrower() {
  if (failing.now) throw new Error("boom");
  return <p>Recovered view</p>;
}

function Counter() {
  const [count, setCount] = useState(0);
  return (
    <button type="button" onClick={() => setCount(count + 1)}>
      Count {count}
    </button>
  );
}

describe("RenderErrorBoundary (#846)", () => {
  it("shows the fallback instead of a tree that throws while rendering, and reports it once", () => {
    failing.now = true;
    const onError = vi.fn<(caught: CaughtRenderError) => void>();
    render(
      <RenderErrorBoundary onError={onError} fallback={(caught) => <p>Fallback: {String(caught.error)}</p>}>
        <Thrower />
      </RenderErrorBoundary>,
    );
    expect(screen.getByText("Fallback: Error: boom")).toBeInTheDocument();
    expect(onError).toHaveBeenCalledOnce();
    const caught = onError.mock.calls[0][0];
    expect(caught.error).toBeInstanceOf(Error);
    expect((caught.error as Error).message).toBe("boom");
    expect(caught.componentStack).toContain("Thrower");
  });

  it("renders its children again, remounted, after a reset", () => {
    failing.now = true;
    let reset: () => void = () => {};
    const { rerender } = render(
      <RenderErrorBoundary
        onError={() => {}}
        fallback={(_caught, doReset) => {
          reset = doReset;
          return <p>Fallback</p>;
        }}
      >
        <Counter />
        <Thrower />
      </RenderErrorBoundary>,
    );
    expect(screen.getByText("Fallback")).toBeInTheDocument();
    failing.now = false;
    act(() => reset());
    expect(screen.getByText("Recovered view")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Count 0" }));
    expect(screen.getByRole("button", { name: "Count 1" })).toBeInTheDocument();

    // A second error and reset start the children from scratch: the counter's state is gone.
    failing.now = true;
    rerender(
      <RenderErrorBoundary
        onError={() => {}}
        fallback={(_caught, doReset) => {
          reset = doReset;
          return <p>Fallback</p>;
        }}
      >
        <Counter />
        <Thrower />
      </RenderErrorBoundary>,
    );
    expect(screen.getByText("Fallback")).toBeInTheDocument();
    failing.now = false;
    act(() => reset());
    expect(screen.getByRole("button", { name: "Count 0" })).toBeInTheDocument();
  });
});

describe("RenderErrorFallback (#846)", () => {
  const caught: CaughtRenderError = {
    error: Object.assign(new Error("boom"), { stack: "Error: boom\n    at Thrower (view.tsx:1:1)" }),
    componentStack: "\n    at Thrower\n    at RepoWorkspace",
  };

  function setup(overrides: Partial<Parameters<typeof RenderErrorFallback>[0]> = {}) {
    const onReload = vi.fn();
    const onClose = vi.fn();
    render(
      <RenderErrorFallback
        title="Couldn't show demo"
        caught={caught}
        hint="Reload the tab to read the repository again."
        reloadLabel="Reload tab"
        onReload={onReload}
        onClose={onClose}
        getAppVersion={async () => "1.2.3"}
        focusOnShow
        {...overrides}
      />,
    );
    return { onReload, onClose };
  }

  it("names what failed, the error and the next step, as an alert", () => {
    setup();
    const alert = screen.getByRole("alert");
    expect(alert).toHaveTextContent("Couldn't show demo");
    expect(alert).toHaveTextContent("boom");
    expect(alert).toHaveTextContent("Reload the tab to read the repository again.");
    expect(screen.getByRole("heading", { name: "Couldn't show demo" })).toBeInTheDocument();
    expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual([
      "Reload tab",
      "Copy details",
      "Close tab",
    ]);
  });

  it("runs Reload and Close", () => {
    const { onReload, onClose } = setup();
    fireEvent.click(screen.getByRole("button", { name: "Reload tab" }));
    expect(onReload).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", { name: "Close tab" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("offers no Close without one", () => {
    setup({ onClose: undefined, reloadLabel: "Reload" });
    expect(screen.getAllByRole("button").map((button) => button.textContent)).toEqual(["Reload", "Copy details"]);
  });

  it("moves the focus to Reload when the focus was lost with the failed view", () => {
    setup();
    expect(screen.getByRole("button", { name: "Reload tab" })).toHaveFocus();
  });

  it("leaves the focus alone when asked to (a tab in the background)", () => {
    setup({ focusOnShow: false });
    expect(screen.getByRole("button", { name: "Reload tab" })).not.toHaveFocus();
  });

  it("leaves the focus where it is when it sits somewhere else", () => {
    const outside = document.createElement("input");
    document.body.append(outside);
    outside.focus();
    setup();
    expect(outside).toHaveFocus();
    outside.remove();
  });

  it("Copy details copies the version, the error, its stack and the component stack", async () => {
    const writeText = vi.fn(async () => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    try {
      setup();
      fireEvent.click(screen.getByRole("button", { name: "Copy details" }));
      expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();
      expect(writeText).toHaveBeenCalledWith(renderErrorDetails(caught, "1.2.3"));
    } finally {
      Reflect.deleteProperty(navigator, "clipboard");
    }
  });

  it("says when the clipboard refused the copy", async () => {
    setup();
    fireEvent.click(screen.getByRole("button", { name: "Copy details" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "Couldn't copy" })).toBeInTheDocument());
  });
});

describe("renderErrorDetails (#846)", () => {
  it("lists the version, the error, its stack and the component stack", () => {
    const error = Object.assign(new Error("boom"), { stack: "Error: boom\n    at Thrower (view.tsx:1:1)" });
    expect(renderErrorDetails({ error, componentStack: "\n    at Thrower\n    at RepoWorkspace" }, "1.2.3")).toBe(
      [
        "Browsitory 1.2.3",
        "Error: boom",
        "",
        "Error: boom",
        "    at Thrower (view.tsx:1:1)",
        "",
        "Component stack:",
        "    at Thrower",
        "    at RepoWorkspace",
      ].join("\n"),
    );
  });

  it("copes with a thrown value that isn't an Error, and an unknown version", () => {
    expect(renderErrorDetails({ error: "plain text", componentStack: "" }, null)).toBe(
      ["Browsitory (version unknown)", "plain text"].join("\n"),
    );
  });
});
